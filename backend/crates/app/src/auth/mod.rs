//! Authentification par sessions serveur (jeton opaque en cookie HttpOnly) et autorisations RBAC.

pub mod http;
pub mod mfa;
pub mod password;
pub mod reset;

use crate::{error::AppError, state::AppState};
use axum::{
    extract::{FromRequestParts, Request},
    http::{Method, request::Parts},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;
use mysuite_shared::permissions::{Permission, Role, has_permission};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "mysuite_session";

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Contexte authentifié. `tenant` est déterminé côté serveur depuis la session, jamais depuis la requête.
#[derive(Debug, Clone)]
pub struct Auth {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub is_platform_admin: bool,
    pub tenant: Option<TenantCtx>,
    /// La société exige le MFA et l'utilisateur ne l'a pas activé : seule la configuration du MFA est permise.
    pub mfa_blocked: bool,
}

/// Vrai si la société exige le MFA et que l'utilisateur n'a pas de MFA activé.
pub async fn mfa_blocked(
    db: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let r: Option<(bool,)> = sqlx::query_as(
        "SELECT t.mfa_required AND NOT EXISTS (SELECT 1 FROM user_mfa WHERE user_id = $1 AND enabled)
         FROM tenants t WHERE t.id = $2",
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(r.is_some_and(|(b,)| b))
}

#[derive(Debug, Clone)]
pub struct TenantCtx {
    pub tenant_id: Uuid,
    pub role: Role,
    /// Dossier employé lié à l'utilisateur dans cette société (portail libre-service).
    pub employee_id: Option<Uuid>,
}

impl Auth {
    /// Retourne le contexte société si le rôle possède la permission, sinon 403.
    pub fn require(&self, perm: Permission) -> Result<&TenantCtx, AppError> {
        let t = self.tenant.as_ref().ok_or(AppError::Forbidden)?;
        if self.mfa_blocked {
            return Err(AppError::MfaEnrollmentRequired);
        }
        if has_permission(t.role, perm) {
            Ok(t)
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub fn tenant(&self) -> Result<&TenantCtx, AppError> {
        if self.mfa_blocked {
            return Err(AppError::MfaEnrollmentRequired);
        }
        self.tenant.as_ref().ok_or(AppError::Forbidden)
    }

    pub fn require_platform_admin(&self) -> Result<(), AppError> {
        if self.is_platform_admin {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    session_id: Uuid,
    user_id: Uuid,
    is_platform_admin: bool,
    active_tenant_id: Option<Uuid>,
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(AppError::Unauthorized)?;
        let row: Option<SessionRow> = sqlx::query_as(
            "SELECT s.id AS session_id, u.id AS user_id, u.is_platform_admin, s.active_tenant_id
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now() AND u.status = 'active'",
        )
        .bind(hash_token(&token))
        .fetch_optional(&state.db)
        .await?;
        let row = row.ok_or(AppError::Unauthorized)?;

        let mut tenant = None;
        let mut blocked = false;
        if let Some(tid) = row.active_tenant_id {
            // Le rôle est relu à chaque requête : une révocation prend effet immédiatement.
            let m: Option<(String, Option<Uuid>)> = sqlx::query_as(
                "SELECT m.role, (SELECT e.id FROM employees e WHERE e.tenant_id = m.tenant_id AND e.user_id = m.user_id)
                 FROM memberships m JOIN tenants t ON t.id = m.tenant_id
                 WHERE m.user_id = $1 AND m.tenant_id = $2 AND t.status = 'active'",
            )
            .bind(row.user_id)
            .bind(tid)
            .fetch_optional(&state.db)
            .await?;
            if let Some((role, employee_id)) = m
                && let Some(role) = Role::parse(&role)
            {
                blocked = mfa_blocked(&state.db, row.user_id, tid).await?;
                tenant = Some(TenantCtx {
                    tenant_id: tid,
                    role,
                    employee_id,
                });
            }
        }

        // Mise à jour discrète de l'activité (au plus une fois par minute).
        let _ = sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE id = $1 AND last_seen_at < now() - interval '1 minute'")
            .bind(row.session_id)
            .execute(&state.db)
            .await;

        Ok(Auth {
            user_id: row.user_id,
            session_id: row.session_id,
            is_platform_admin: row.is_platform_admin,
            tenant,
            mfa_blocked: blocked,
        })
    }
}

/// Protection CSRF : cookie SameSite=Strict + en-tête personnalisé obligatoire sur les méthodes modifiantes.
/// Un formulaire cross-site ne peut pas ajouter cet en-tête.
pub async fn csrf_guard(req: Request, next: Next) -> Result<Response, AppError> {
    let mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if mutating {
        let ok = req
            .headers()
            .get("x-requested-with")
            .and_then(|v| v.to_str().ok())
            == Some("mysuite");
        if !ok {
            return Err(AppError::Forbidden);
        }
    }
    Ok(next.run(req).await)
}
