use super::{Auth, SESSION_COOKIE, hash_token, new_token, password};
use crate::{
    audit,
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Duration, Utc};
use mysuite_shared::permissions::{Role, permissions_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const MAX_FAILED_ATTEMPTS: i32 = 5;
const LOCK_MINUTES: i64 = 15;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/switch-tenant", post(switch_tenant))
        .route("/auth/change-password", post(change_password))
        .route("/auth/sessions", get(list_sessions))
        .route("/auth/sessions/{id}", delete(revoke_session))
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub struct TenantInfo {
    id: Uuid,
    name: String,
    slug: String,
    role: String,
    currency: String,
}

/// Réponse de connexion : session ouverte, ou second facteur à fournir (`POST /auth/login/mfa`).
#[derive(Serialize)]
#[serde(untagged)]
pub enum LoginResponse {
    Me(Box<MeResponse>),
    MfaRequired {
        mfa_required: bool,
        mfa_token: String,
    },
}

#[derive(Serialize)]
pub struct MeResponse {
    user: UserInfo,
    tenants: Vec<TenantInfo>,
    active_tenant_id: Option<Uuid>,
    role: Option<String>,
    permissions: Vec<&'static str>,
    employee_id: Option<Uuid>,
    /// La société exige le MFA : l'interface doit amener l'utilisateur à le configurer.
    mfa_enrollment_required: bool,
}

#[derive(Serialize)]
struct UserInfo {
    id: Uuid,
    email: String,
    full_name: String,
    is_platform_admin: bool,
}

pub(super) fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
}

fn session_cookie(token: String, secure: bool, ttl_hours: i64) -> Cookie<'static> {
    let mut c = Cookie::new(SESSION_COOKIE, token);
    c.set_http_only(true);
    c.set_secure(secure);
    c.set_same_site(SameSite::Strict);
    c.set_path("/");
    c.set_max_age(time_duration(ttl_hours));
    c
}

// axum-extra utilise le crate `time` ; on évite une dépendance directe en passant par cookie::time.
fn time_duration(hours: i64) -> time::Duration {
    time::Duration::hours(hours)
}

async fn build_me(state: &AppState, auth: &Auth) -> AppResult<MeResponse> {
    let user: (Uuid, String, String, bool) =
        sqlx::query_as("SELECT id, email, full_name, is_platform_admin FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;
    let rows: Vec<(Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.name, t.slug, m.role, t.currency FROM memberships m JOIN tenants t ON t.id = m.tenant_id
         WHERE m.user_id = $1 AND t.status = 'active' ORDER BY t.name",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let tenants = rows
        .into_iter()
        .map(|(id, name, slug, role, currency)| TenantInfo {
            id,
            name,
            slug,
            role,
            currency,
        })
        .collect();
    let (active, role, permissions, employee_id) = match &auth.tenant {
        Some(t) => (
            Some(t.tenant_id),
            Some(t.role.as_str().to_string()),
            // Sans MFA dans une société qui l'exige, aucune permission n'est exposée.
            if auth.mfa_blocked {
                vec![]
            } else {
                permissions_for(t.role).iter().map(|p| p.as_str()).collect()
            },
            t.employee_id,
        ),
        None => (None, None, vec![], None),
    };
    Ok(MeResponse {
        user: UserInfo {
            id: user.0,
            email: user.1,
            full_name: user.2,
            is_platform_admin: user.3,
        },
        tenants,
        active_tenant_id: active,
        role,
        permissions,
        employee_id,
        mfa_enrollment_required: auth.mfa_blocked,
    })
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<LoginBody>,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    let email = body.email.trim().to_lowercase();
    let ip = client_ip(&headers);

    #[derive(sqlx::FromRow)]
    struct U {
        id: Uuid,
        password_hash: String,
        status: String,
        failed_attempts: i32,
        locked_until: Option<DateTime<Utc>>,
    }
    let user: Option<U> =
        sqlx::query_as("SELECT id, password_hash, status, failed_attempts, locked_until FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;

    let Some(user) = user else {
        // Temps de réponse comparable : on vérifie un hash factice.
        let _ = password::verify(&body.password, &state.dummy_hash);
        log_login(&state, &email, None, false, ip).await;
        return Err(AppError::Unauthorized);
    };

    if user.locked_until.is_some_and(|t| t > Utc::now()) {
        log_login(&state, &email, Some(user.id), false, ip).await;
        return Err(AppError::TooManyAttempts);
    }

    if user.status != "active" || !password::verify(&body.password, &user.password_hash) {
        let attempts = user.failed_attempts + 1;
        let lock = if attempts >= MAX_FAILED_ATTEMPTS {
            Some(Utc::now() + Duration::minutes(LOCK_MINUTES))
        } else {
            None
        };
        sqlx::query("UPDATE users SET failed_attempts = $2, locked_until = $3 WHERE id = $1")
            .bind(user.id)
            .bind(if lock.is_some() { 0 } else { attempts })
            .bind(lock)
            .execute(&state.db)
            .await?;
        log_login(&state, &email, Some(user.id), false, ip).await;
        return Err(AppError::Unauthorized);
    }

    // Second facteur : le compteur d'échecs n'est PAS remis à zéro à ce stade, sinon quelqu'un qui connaît
    // le mot de passe pourrait relancer indéfiniment la saisie des codes.
    let mfa: Option<(bool,)> = sqlx::query_as("SELECT enabled FROM user_mfa WHERE user_id = $1")
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?;
    if mfa.is_some_and(|m| m.0) {
        let token = new_token();
        sqlx::query("DELETE FROM mfa_challenges WHERE user_id = $1 OR expires_at < now()")
            .bind(user.id)
            .execute(&state.db)
            .await?;
        sqlx::query("INSERT INTO mfa_challenges (user_id, token_hash, expires_at) VALUES ($1,$2, now() + interval '5 minutes')")
            .bind(user.id)
            .bind(hash_token(&token))
            .execute(&state.db)
            .await?;
        return Ok((
            jar,
            Json(LoginResponse::MfaRequired {
                mfa_required: true,
                mfa_token: token,
            }),
        ));
    }
    finish_login(&state, &headers, jar, user.id, &email, ip).await
}

/// Ouvre la session (après mot de passe seul, ou après mot de passe + second facteur).
pub(super) async fn finish_login(
    state: &AppState,
    headers: &HeaderMap,
    jar: CookieJar,
    user_id: Uuid,
    email: &str,
    ip: Option<String>,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    sqlx::query("UPDATE users SET failed_attempts = 0, locked_until = NULL WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    let first_tenant: Option<(Uuid,)> = sqlx::query_as(
        "SELECT m.tenant_id FROM memberships m JOIN tenants t ON t.id = m.tenant_id
         WHERE m.user_id = $1 AND t.status = 'active' ORDER BY m.created_at LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let token = new_token();
    let expires = Utc::now() + Duration::hours(state.config.session_ttl_hours);
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect::<String>());
    let session_id: (Uuid,) = sqlx::query_as(
        "INSERT INTO sessions (user_id, token_hash, active_tenant_id, user_agent, ip, expires_at) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(user_id)
    .bind(hash_token(&token))
    .bind(first_tenant.map(|t| t.0))
    .bind(ua)
    .bind(&ip)
    .bind(expires)
    .fetch_one(&state.db)
    .await?;
    log_login(state, email, Some(user_id), true, ip).await;

    let auth = super::Auth {
        user_id,
        session_id: session_id.0,
        is_platform_admin: false,
        tenant: None,
        mfa_blocked: false,
    };
    // Reconstruit le contexte complet via la même logique que l'extracteur.
    let me = me_for_session(state, &auth, first_tenant.map(|t| t.0)).await?;
    let jar = jar.add(session_cookie(
        token,
        state.config.cookie_secure,
        state.config.session_ttl_hours,
    ));
    Ok((jar, Json(LoginResponse::Me(Box::new(me)))))
}

async fn me_for_session(
    state: &AppState,
    base: &Auth,
    tenant_id: Option<Uuid>,
) -> AppResult<MeResponse> {
    let mut auth = base.clone();
    let is_admin: (bool,) = sqlx::query_as("SELECT is_platform_admin FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    auth.is_platform_admin = is_admin.0;
    if let Some(tid) = tenant_id {
        let m: Option<(String, Option<Uuid>)> = sqlx::query_as(
            "SELECT m.role, (SELECT e.id FROM employees e WHERE e.tenant_id = m.tenant_id AND e.user_id = m.user_id)
             FROM memberships m WHERE m.user_id = $1 AND m.tenant_id = $2",
        )
        .bind(auth.user_id)
        .bind(tid)
        .fetch_optional(&state.db)
        .await?;
        if let Some((role, employee_id)) = m
            && let Some(role) = Role::parse(&role)
        {
            auth.mfa_blocked = super::mfa_blocked(&state.db, auth.user_id, tid).await?;
            auth.tenant = Some(super::TenantCtx {
                tenant_id: tid,
                role,
                employee_id,
            });
        }
    }
    build_me(state, &auth).await
}

pub(super) async fn log_login(
    state: &AppState,
    email: &str,
    user_id: Option<Uuid>,
    success: bool,
    ip: Option<String>,
) {
    let _ =
        sqlx::query("INSERT INTO login_events (email, user_id, success, ip) VALUES ($1,$2,$3,$4)")
            .bind(email)
            .bind(user_id)
            .bind(success)
            .bind(ip)
            .execute(&state.db)
            .await;
}

async fn logout(
    State(state): State<AppState>,
    auth: Auth,
    jar: CookieJar,
) -> AppResult<(CookieJar, StatusCode)> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1")
        .bind(auth.session_id)
        .execute(&state.db)
        .await?;
    let mut removal = Cookie::from(SESSION_COOKIE);
    removal.set_path("/");
    Ok((jar.remove(removal), StatusCode::NO_CONTENT))
}

async fn me(State(state): State<AppState>, auth: Auth) -> AppResult<Json<MeResponse>> {
    Ok(Json(build_me(&state, &auth).await?))
}

#[derive(Deserialize)]
struct SwitchBody {
    tenant_id: Uuid,
}

async fn switch_tenant(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<SwitchBody>,
) -> AppResult<Json<MeResponse>> {
    // L'appartenance est vérifiée côté serveur : on ne fait jamais confiance à l'identifiant reçu.
    let ok: Option<(Uuid,)> = sqlx::query_as(
        "SELECT m.tenant_id FROM memberships m JOIN tenants t ON t.id = m.tenant_id
         WHERE m.user_id = $1 AND m.tenant_id = $2 AND t.status = 'active'",
    )
    .bind(auth.user_id)
    .bind(body.tenant_id)
    .fetch_optional(&state.db)
    .await?;
    if ok.is_none() {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE sessions SET active_tenant_id = $2 WHERE id = $1")
        .bind(auth.session_id)
        .bind(body.tenant_id)
        .execute(&state.db)
        .await?;
    let me = me_for_session(&state, &auth, Some(body.tenant_id)).await?;
    Ok(Json(me))
}

#[derive(Deserialize)]
struct ChangePasswordBody {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<ChangePasswordBody>,
) -> AppResult<StatusCode> {
    password::validate_strength(&body.new_password).map_err(|m| AppError::Validation(m.into()))?;
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    if !password::verify(&body.current_password, &hash) {
        return Err(AppError::Forbidden);
    }
    let new_hash = password::hash(&body.new_password)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(auth.user_id)
        .bind(new_hash)
        .execute(&mut *tx)
        .await?;
    // Les autres appareils sont déconnectés.
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL")
        .bind(auth.user_id)
        .bind(auth.session_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        auth.tenant.as_ref().map(|t| t.tenant_id),
        Some(auth.user_id),
        "password.change",
        "user",
        Some(auth.user_id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
struct SessionInfo {
    id: Uuid,
    user_agent: Option<String>,
    ip: Option<String>,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    #[sqlx(skip)]
    current: bool,
}

async fn list_sessions(
    State(state): State<AppState>,
    auth: Auth,
) -> AppResult<Json<Vec<SessionInfo>>> {
    let mut rows: Vec<SessionInfo> = sqlx::query_as(
        "SELECT id, user_agent, ip, created_at, last_seen_at FROM sessions
         WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now() ORDER BY last_seen_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    for r in &mut rows {
        r.current = r.id == auth.session_id;
    }
    Ok(Json(rows))
}

async fn revoke_session(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    ensure(res.rows_affected() > 0, "session introuvable")?;
    Ok(StatusCode::NO_CONTENT)
}
