//! Réinitialisation de mot de passe émise par un administrateur.
//!
//! Aucun courriel n'est envoyé (pas de SMTP configuré) : l'administrateur remet le lien à l'utilisateur
//! par un canal de son choix. Le jeton (256 bits) est à usage unique, valable 1 h, stocké haché ;
//! l'utiliser révoque toutes les sessions et lève le verrouillage du compte.

use super::{Auth, hash_token, new_token, password};
use crate::{
    audit,
    error::{AppError, AppResult},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use chrono::{DateTime, Duration, Utc};
use mysuite_shared::permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const RESET_VALID_MINUTES: i64 = 60;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users/{user_id}/password-reset", post(issue))
        .route("/auth/reset-password", post(redeem))
}

#[derive(Serialize)]
struct Issued {
    /// Affiché une seule fois à l'administrateur.
    reset_token: String,
    /// Chemin relatif à ajouter à l'adresse de l'application.
    path: String,
    expires_at: DateTime<Utc>,
    /// `true` si le lien a aussi été envoyé par courriel (SMTP configuré et envoi réussi).
    emailed: bool,
}

async fn issue(
    State(s): State<AppState>,
    auth: Auth,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Issued>> {
    let t = auth.require(Permission::UsersManage)?;
    // Soi-même : « changer le mot de passe » ; un administrateur de plateforme ne se réinitialise pas par ce canal.
    if user_id == auth.user_id {
        return Err(AppError::Forbidden);
    }
    let target: Option<(bool,)> = sqlx::query_as(
        "SELECT u.is_platform_admin FROM users u JOIN memberships m ON m.user_id = u.id
         WHERE u.id = $1 AND m.tenant_id = $2 AND u.status = 'active'",
    )
    .bind(user_id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?;
    match target {
        None => return Err(AppError::NotFound),
        Some((true,)) => return Err(AppError::Forbidden),
        Some(_) => {}
    }
    let token = new_token();
    let expires = Utc::now() + Duration::minutes(RESET_VALID_MINUTES);
    let mut tx = s.db.begin().await?;
    // Un seul lien valide à la fois par utilisateur.
    sqlx::query(
        "UPDATE password_resets SET used_at = now() WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO password_resets (user_id, tenant_id, issued_by, token_hash, expires_at) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(user_id)
    .bind(t.tenant_id)
    .bind(auth.user_id)
    .bind(hash_token(&token))
    .bind(expires)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "user.password_reset_issued",
        "user",
        Some(user_id.to_string()),
        json!({"expires_at": expires}),
    )
    .await?;
    tx.commit().await?;
    let path = format!("/reset-password?token={token}");
    let mut emailed = false;
    if let Some(cfg) = &s.config.smtp {
        let info: Option<(String, String)> = sqlx::query_as(
            "SELECT u.email, t.name FROM users u, tenants t WHERE u.id = $1 AND t.id = $2",
        )
        .bind(user_id)
        .bind(t.tenant_id)
        .fetch_optional(&s.db)
        .await?;
        if let Some((email, company)) = info {
            match crate::mail::send_reset(cfg, &email, &company, &path, RESET_VALID_MINUTES).await {
                Ok(()) => emailed = true,
                // Le lien manuel reste valable : l'échec d'envoi n'est pas une erreur pour l'administrateur.
                Err(e) => {
                    tracing::warn!(error = %e, "envoi du courriel de réinitialisation échoué")
                }
            }
        }
        audit::record(
            &s.db,
            Some(t.tenant_id),
            Some(auth.user_id),
            "user.password_reset_emailed",
            "user",
            Some(user_id.to_string()),
            json!({"sent": emailed}),
        )
        .await?;
    }
    Ok(Json(Issued {
        path,
        reset_token: token,
        expires_at: expires,
        emailed,
    }))
}

#[derive(Deserialize)]
struct Redeem {
    token: String,
    new_password: String,
}

async fn redeem(State(s): State<AppState>, Json(b): Json<Redeem>) -> AppResult<StatusCode> {
    password::validate_strength(&b.new_password)
        .map_err(|m| AppError::Validation(m.to_string()))?;
    let mut tx = s.db.begin().await?;
    let row: Option<(Uuid, Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, user_id, tenant_id FROM password_resets
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(hash_token(&b.token))
    .fetch_optional(&mut *tx)
    .await?;
    // Message volontairement unique : on ne distingue ni jeton inconnu, ni expiré, ni déjà utilisé.
    let (id, user_id, tenant_id) =
        row.ok_or_else(|| AppError::Validation("lien invalide ou expiré".into()))?;
    let hash = password::hash(&b.new_password)?;
    sqlx::query(
        "UPDATE users SET password_hash = $2, failed_attempts = 0, locked_until = NULL WHERE id = $1",
    )
    .bind(user_id)
    .bind(hash)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE password_resets SET used_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        tenant_id,
        Some(user_id),
        "user.password_reset",
        "user",
        Some(user_id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
