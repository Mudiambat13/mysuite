//! Authentification à deux facteurs (TOTP, RFC 6238) avec codes de secours.
//!
//! - Le secret TOTP est chiffré au repos (AES-256-GCM, clé `MFA_ENCRYPTION_KEY`, lié à l'utilisateur).
//! - Un code TOTP ne sert qu'une fois (anti-rejeu) ; fenêtre de tolérance ±1 pas de 30 s.
//! - Les codes de secours sont à usage unique, stockés hachés, affichés une seule fois.
//! - Les échecs du second facteur comptent dans le verrouillage du compte (5 échecs → 15 min).
//! - Activer, désactiver ou régénérer exige le mot de passe actuel (et un code pour désactiver/régénérer).

use super::{
    Auth, hash_token,
    http::{LoginResponse, client_ip, finish_login, log_login},
    password,
};
use crate::{
    audit,
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use mysuite_shared::permissions::Permission;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha1::Sha1;
use uuid::Uuid;

const STEP_SECONDS: i64 = 30;
const MAX_CHALLENGE_ATTEMPTS: i32 = 5;
const MAX_FAILED_ATTEMPTS: i32 = 5;
const LOCK_MINUTES: i64 = 15;
const RECOVERY_CODES: usize = 10;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login/mfa", post(login_mfa))
        .route("/auth/mfa", get(status))
        .route("/security/policy", get(get_policy).put(set_policy))
        .route("/auth/mfa/setup", post(setup))
        .route("/auth/mfa/enable", post(enable))
        .route("/auth/mfa/disable", post(disable))
        .route("/auth/mfa/recovery-codes", post(regenerate))
        .route("/users/{user_id}/mfa-reset", post(admin_reset))
}

// ---------- TOTP ----------

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32_encode(data: &[u8]) -> String {
    let (mut out, mut buf, mut bits) = (String::new(), 0u32, 0u32);
    for &b in data {
        buf = (buf << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            out.push(B32[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Code à 6 chiffres pour un pas donné (HOTP RFC 4226 sur le compteur `step`).
pub fn totp_code(secret: &[u8], step: u64) -> u32 {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(secret).expect("HMAC accepte toute longueur");
    mac.update(&step.to_be_bytes());
    let r = mac.finalize().into_bytes();
    let o = (r[19] & 0x0f) as usize;
    let bin = (u32::from(r[o] & 0x7f) << 24)
        | (u32::from(r[o + 1]) << 16)
        | (u32::from(r[o + 2]) << 8)
        | u32::from(r[o + 3]);
    bin % 1_000_000
}

/// Pas accepté pour ce code (±1 pas) s'il est strictement postérieur au dernier pas utilisé.
pub fn match_step(secret: &[u8], code: u32, now_unix: i64, last_used: i64) -> Option<i64> {
    let current = now_unix / STEP_SECONDS;
    (current - 1..=current + 1)
        .find(|&s| s > last_used && s >= 0 && totp_code(secret, s as u64) == code)
}

// ---------- Chiffrement du secret ----------

fn cipher(state: &AppState) -> AppResult<Aes256Gcm> {
    let key = state.config.mfa_key.ok_or_else(|| {
        AppError::Validation(
            "le MFA n'est pas configuré sur ce serveur (variable MFA_ENCRYPTION_KEY)".into(),
        )
    })?;
    Ok(Aes256Gcm::new_from_slice(&key).expect("clé de 32 octets"))
}

fn encrypt(state: &AppState, user_id: Uuid, secret: &[u8]) -> AppResult<Vec<u8>> {
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ct = cipher(state)?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: secret,
                aad: user_id.as_bytes(),
            },
        )
        .map_err(|_| AppError::Internal(anyhow::anyhow!("chiffrement MFA")))?;
    Ok([nonce.to_vec(), ct].concat())
}

fn decrypt(state: &AppState, user_id: Uuid, blob: &[u8]) -> AppResult<Vec<u8>> {
    if blob.len() < 13 {
        return Err(AppError::Internal(anyhow::anyhow!("secret MFA corrompu")));
    }
    let (nonce, ct) = blob.split_at(12);
    cipher(state)?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: user_id.as_bytes(),
            },
        )
        .map_err(|_| AppError::Internal(anyhow::anyhow!("déchiffrement MFA impossible")))
}

// ---------- Second facteur ----------

fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_uppercase()
}

fn new_recovery_code() -> String {
    let mut b = [0u8; 7];
    rand::rngs::OsRng.fill_bytes(&mut b);
    let s = base32_encode(&b)[..10].to_string();
    format!("{}-{}", &s[..5], &s[5..])
}

/// Vérifie un code TOTP (6 chiffres) ou un code de secours pour un utilisateur dont le MFA est activé.
async fn verify_second_factor(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    raw: &str,
) -> AppResult<bool> {
    let code = normalize(raw);
    if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
        let row: Option<(Vec<u8>, i64)> = sqlx::query_as(
            "SELECT secret_enc, last_used_step FROM user_mfa WHERE user_id = $1 AND enabled FOR UPDATE",
        )
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((blob, last)) = row else {
            return Ok(false);
        };
        let secret = decrypt(state, user_id, &blob)?;
        let n: u32 = code.parse().unwrap_or(u32::MAX);
        return match match_step(&secret, n, Utc::now().timestamp(), last) {
            Some(step) => {
                sqlx::query("UPDATE user_mfa SET last_used_step = $2 WHERE user_id = $1")
                    .bind(user_id)
                    .bind(step)
                    .execute(&mut **tx)
                    .await?;
                Ok(true)
            }
            None => Ok(false),
        };
    }
    // Code de secours (usage unique).
    let used = sqlx::query(
        "UPDATE mfa_recovery_codes SET used_at = now() WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
    )
    .bind(user_id)
    .bind(hash_token(&code))
    .execute(&mut **tx)
    .await?;
    Ok(used.rows_affected() == 1)
}

async fn check_password(state: &AppState, user_id: Uuid, pwd: &str) -> AppResult<()> {
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    if password::verify(pwd, &hash) {
        Ok(())
    } else {
        // 403 (et non 401) : la session reste valide, seule l'action est refusée.
        Err(AppError::Forbidden)
    }
}

// ---------- Connexion, 2e étape ----------

#[derive(Deserialize)]
struct MfaLogin {
    mfa_token: String,
    code: String,
}

async fn login_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(b): Json<MfaLogin>,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    let ip = client_ip(&headers);
    let mut tx = state.db.begin().await?;
    let ch: Option<(Uuid, Uuid, i32)> = sqlx::query_as(
        "SELECT id, user_id, attempts FROM mfa_challenges WHERE token_hash = $1 AND expires_at > now() FOR UPDATE",
    )
    .bind(hash_token(&b.mfa_token))
    .fetch_optional(&mut *tx)
    .await?;
    let (challenge_id, user_id, attempts) = ch.ok_or(AppError::Unauthorized)?;
    let (email, status, locked): (String, String, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as("SELECT email, status, locked_until FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    if status != "active" {
        return Err(AppError::Unauthorized);
    }
    if locked.is_some_and(|t| t > Utc::now()) {
        return Err(AppError::TooManyAttempts);
    }
    if attempts >= MAX_CHALLENGE_ATTEMPTS {
        sqlx::query("DELETE FROM mfa_challenges WHERE id = $1")
            .bind(challenge_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(AppError::Unauthorized);
    }
    if verify_second_factor(&state, &mut tx, user_id, &b.code).await? {
        sqlx::query("DELETE FROM mfa_challenges WHERE id = $1")
            .bind(challenge_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return finish_login(&state, &headers, jar, user_id, &email, ip).await;
    }
    // Échec : compté sur le défi ET sur le compte (verrouillage 15 min après 5 échecs).
    sqlx::query("UPDATE mfa_challenges SET attempts = attempts + 1 WHERE id = $1")
        .bind(challenge_id)
        .execute(&mut *tx)
        .await?;
    let (fails,): (i32,) = sqlx::query_as(
        "UPDATE users SET failed_attempts = failed_attempts + 1 WHERE id = $1 RETURNING failed_attempts",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if fails >= MAX_FAILED_ATTEMPTS {
        sqlx::query("UPDATE users SET failed_attempts = 0, locked_until = $2 WHERE id = $1")
            .bind(user_id)
            .bind(Utc::now() + Duration::minutes(LOCK_MINUTES))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM mfa_challenges WHERE id = $1")
            .bind(challenge_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    log_login(&state, &email, Some(user_id), false, ip).await;
    Err(AppError::Unauthorized)
}

// ---------- Gestion du MFA par l'utilisateur ----------

#[derive(Serialize)]
struct MfaStatus {
    /// Le serveur peut-il chiffrer les secrets (MFA_ENCRYPTION_KEY définie) ?
    available: bool,
    enabled: bool,
    recovery_codes_remaining: i64,
}

async fn status(State(s): State<AppState>, auth: Auth) -> AppResult<Json<MfaStatus>> {
    let enabled: Option<(bool,)> =
        sqlx::query_as("SELECT enabled FROM user_mfa WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&s.db)
            .await?;
    let (left,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM mfa_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(auth.user_id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(MfaStatus {
        available: s.config.mfa_key.is_some(),
        enabled: enabled.is_some_and(|e| e.0),
        recovery_codes_remaining: left,
    }))
}

#[derive(Deserialize)]
struct PasswordBody {
    password: String,
}

#[derive(Serialize)]
struct Setup {
    /// Secret en base32, à saisir dans l'application d'authentification (affiché maintenant seulement).
    secret: String,
    otpauth_uri: String,
}

fn uri_encode(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn setup(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<PasswordBody>,
) -> AppResult<Json<Setup>> {
    cipher(&s)?;
    check_password(&s, auth.user_id, &b.password).await?;
    let already: Option<(bool,)> =
        sqlx::query_as("SELECT enabled FROM user_mfa WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&s.db)
            .await?;
    if already.is_some_and(|a| a.0) {
        return Err(AppError::Conflict("le MFA est déjà activé".into()));
    }
    let mut secret = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    let blob = encrypt(&s, auth.user_id, &secret)?;
    sqlx::query(
        "INSERT INTO user_mfa (user_id, secret_enc, enabled) VALUES ($1,$2,FALSE)
         ON CONFLICT (user_id) DO UPDATE SET secret_enc = EXCLUDED.secret_enc, enabled = FALSE, last_used_step = 0",
    )
    .bind(auth.user_id)
    .bind(blob)
    .execute(&s.db)
    .await?;
    let (email,): (String,) = sqlx::query_as("SELECT email FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&s.db)
        .await?;
    let secret_b32 = base32_encode(&secret);
    let uri = format!(
        "otpauth://totp/MySuite:{}?secret={}&issuer=MySuite&algorithm=SHA1&digits=6&period=30",
        uri_encode(&email),
        secret_b32
    );
    Ok(Json(Setup {
        secret: secret_b32,
        otpauth_uri: uri,
    }))
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
}

#[derive(Serialize)]
struct Recovery {
    /// Affichés une seule fois ; chacun ne sert qu'une fois.
    recovery_codes: Vec<String>,
}

async fn issue_recovery_codes(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
) -> AppResult<Vec<String>> {
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    let mut codes = Vec::new();
    for _ in 0..RECOVERY_CODES {
        let c = new_recovery_code();
        sqlx::query("INSERT INTO mfa_recovery_codes (user_id, code_hash) VALUES ($1,$2)")
            .bind(user_id)
            .bind(hash_token(&normalize(&c)))
            .execute(&mut **tx)
            .await?;
        codes.push(c);
    }
    Ok(codes)
}

async fn enable(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<CodeBody>,
) -> AppResult<Json<Recovery>> {
    let mut tx = s.db.begin().await?;
    let row: Option<(Vec<u8>, i64, bool)> = sqlx::query_as(
        "SELECT secret_enc, last_used_step, enabled FROM user_mfa WHERE user_id = $1 FOR UPDATE",
    )
    .bind(auth.user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (blob, last, enabled) =
        row.ok_or_else(|| AppError::Validation("lancez d'abord la configuration du MFA".into()))?;
    ensure(!enabled, "le MFA est déjà activé")?;
    let secret = decrypt(&s, auth.user_id, &blob)?;
    let code: u32 = normalize(&b.code)
        .parse()
        .map_err(|_| AppError::Validation("code à 6 chiffres attendu".into()))?;
    let step = match_step(&secret, code, Utc::now().timestamp(), last)
        .ok_or_else(|| AppError::Validation("code incorrect ou expiré".into()))?;
    sqlx::query(
        "UPDATE user_mfa SET enabled = TRUE, enabled_at = now(), last_used_step = $2 WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .bind(step)
    .execute(&mut *tx)
    .await?;
    let codes = issue_recovery_codes(&mut tx, auth.user_id).await?;
    audit::record(
        &mut *tx,
        auth.tenant.as_ref().map(|t| t.tenant_id),
        Some(auth.user_id),
        "mfa.enable",
        "user",
        Some(auth.user_id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Recovery {
        recovery_codes: codes,
    }))
}

#[derive(Deserialize)]
struct PasswordCode {
    password: String,
    code: String,
}

async fn guarded(
    s: &AppState,
    auth: &Auth,
    b: &PasswordCode,
) -> AppResult<sqlx::Transaction<'static, sqlx::Postgres>> {
    check_password(s, auth.user_id, &b.password).await?;
    let mut tx = s.db.begin().await?;
    if !verify_second_factor(s, &mut tx, auth.user_id, &b.code).await? {
        tx.rollback().await?;
        return Err(AppError::Forbidden);
    }
    Ok(tx)
}

async fn disable(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<PasswordCode>,
) -> AppResult<StatusCode> {
    let mut tx = guarded(&s, &auth, &b).await?;
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_mfa WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        auth.tenant.as_ref().map(|t| t.tenant_id),
        Some(auth.user_id),
        "mfa.disable",
        "user",
        Some(auth.user_id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn regenerate(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<PasswordCode>,
) -> AppResult<Json<Recovery>> {
    let mut tx = guarded(&s, &auth, &b).await?;
    let codes = issue_recovery_codes(&mut tx, auth.user_id).await?;
    audit::record(
        &mut *tx,
        auth.tenant.as_ref().map(|t| t.tenant_id),
        Some(auth.user_id),
        "mfa.recovery_regenerate",
        "user",
        Some(auth.user_id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(Recovery {
        recovery_codes: codes,
    }))
}

/// Appareil perdu : un administrateur retire le MFA d'un membre de sa société (journalisé, sessions révoquées).
async fn admin_reset(
    State(s): State<AppState>,
    auth: Auth,
    Path(user_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::UsersManage)?;
    if user_id == auth.user_id {
        return Err(AppError::Forbidden);
    }
    let member: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM memberships WHERE user_id = $1 AND tenant_id = $2")
            .bind(user_id)
            .bind(t.tenant_id)
            .fetch_optional(&s.db)
            .await?;
    if member.is_none() {
        return Err(AppError::NotFound);
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_mfa WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "mfa.admin_reset",
        "user",
        Some(user_id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vecteurs de test RFC 6238 (SHA-1, secret ASCII « 12345678901234567890 »), tronqués à 6 chiffres.
    #[test]
    fn rfc6238_vectors() {
        let secret = b"12345678901234567890";
        for (t, expected) in [
            (59u64, 287_082u32),
            (1_111_111_109, 81_804),
            (1_111_111_111, 50_471),
            (1_234_567_890, 5_924),
            (2_000_000_000, 279_037),
            (20_000_000_000, 353_130),
        ] {
            assert_eq!(totp_code(secret, t / 30), expected, "t={t}");
        }
    }

    #[test]
    fn base32_matches_rfc4648() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_encode(b"fooba"), "MZXW6YTB");
        assert_eq!(base32_encode(b""), "");
    }

    #[test]
    fn window_and_replay_protection() {
        let s = b"12345678901234567890";
        let now = 1_111_111_109; // pas 37037037
        let step = now / 30;
        let code = totp_code(s, step as u64);
        assert_eq!(match_step(s, code, now, 0), Some(step));
        // Rejeu : un pas déjà utilisé est refusé.
        assert_eq!(match_step(s, code, now, step), None);
        // Dérive d'horloge de ±1 pas tolérée, 2 pas refusés.
        assert_eq!(match_step(s, code, now + 30, 0), Some(step));
        assert_eq!(match_step(s, code, now + 90, 0), None);
    }

    #[test]
    fn recovery_codes_are_normalized() {
        assert_eq!(normalize(" ab-cd ef "), "ABCDEF");
        assert_eq!(new_recovery_code().len(), 11);
    }
}

#[derive(Serialize, Deserialize)]
struct Policy {
    mfa_required: bool,
}

async fn get_policy(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Policy>> {
    let t = auth.require(Permission::UsersManage)?;
    let (mfa_required,): (bool,) = sqlx::query_as("SELECT mfa_required FROM tenants WHERE id = $1")
        .bind(t.tenant_id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(Policy { mfa_required }))
}

/// Active ou désactive l'obligation de MFA pour la société. L'administrateur doit avoir activé son propre MFA
/// avant d'imposer la règle, sinon il se verrouillerait lui-même.
async fn set_policy(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<Policy>,
) -> AppResult<Json<Policy>> {
    let t = auth.require(Permission::UsersManage)?;
    if b.mfa_required {
        let own: Option<(bool,)> =
            sqlx::query_as("SELECT enabled FROM user_mfa WHERE user_id = $1")
                .bind(auth.user_id)
                .fetch_optional(&s.db)
                .await?;
        if !own.is_some_and(|(e,)| e) {
            return Err(AppError::Validation(
                "activez d'abord votre propre MFA avant de l'imposer à la société".into(),
            ));
        }
    }
    sqlx::query("UPDATE tenants SET mfa_required = $2 WHERE id = $1")
        .bind(t.tenant_id)
        .bind(b.mfa_required)
        .execute(&s.db)
        .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "tenant.mfa_policy",
        "tenant",
        Some(t.tenant_id.to_string()),
        json!({"mfa_required": b.mfa_required}),
    )
    .await?;
    Ok(Json(b))
}
