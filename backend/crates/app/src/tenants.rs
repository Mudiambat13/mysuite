//! Administration plateforme (sociétés) et gestion des utilisateurs d'une société.

use crate::{
    audit,
    auth::{Auth, password},
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
};
use mysuite_shared::permissions::{Permission, Role};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/platform/tenants", get(list_tenants).post(create_tenant))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{user_id}/role", put(set_role))
}

#[derive(Serialize, sqlx::FromRow)]
struct TenantRow {
    id: Uuid,
    name: String,
    slug: String,
    country: String,
    currency: String,
    status: String,
}

async fn list_tenants(
    State(state): State<AppState>,
    auth: Auth,
) -> AppResult<Json<Vec<TenantRow>>> {
    auth.require_platform_admin()?;
    let rows = sqlx::query_as(
        "SELECT id, name, slug, country, currency, status FROM tenants ORDER BY name",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateTenant {
    name: String,
    slug: String,
    country: Option<String>,
    currency: Option<String>,
    admin_email: String,
    admin_name: String,
    admin_password: String,
}

fn valid_email(e: &str) -> bool {
    let e = e.trim();
    e.len() <= 254
        && e.split('@').count() == 2
        && e.split('@').all(|p| !p.is_empty())
        && e.contains('.')
}

async fn create_tenant(
    State(state): State<AppState>,
    auth: Auth,
    Json(b): Json<CreateTenant>,
) -> AppResult<(StatusCode, Json<TenantRow>)> {
    auth.require_platform_admin()?;
    ensure(!b.name.trim().is_empty(), "nom requis")?;
    ensure(valid_email(&b.admin_email), "email administrateur invalide")?;
    password::validate_strength(&b.admin_password).map_err(|m| AppError::Validation(m.into()))?;
    let email = b.admin_email.trim().to_lowercase();

    let mut tx = state.db.begin().await?;
    let tenant: TenantRow = sqlx::query_as(
        "INSERT INTO tenants (name, slug, country, currency) VALUES ($1,$2,COALESCE($3,'CD'),COALESCE($4,'CDF'))
         RETURNING id, name, slug, country, currency, status",
    )
    .bind(b.name.trim())
    .bind(&b.slug)
    .bind(b.country)
    .bind(b.currency)
    .fetch_one(&mut *tx)
    .await?;

    let existing: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&mut *tx)
        .await?;
    let admin_id = match existing {
        Some((id,)) => id,
        None => {
            let hash = password::hash(&b.admin_password)?;
            let (id,): (Uuid,) = sqlx::query_as("INSERT INTO users (email, password_hash, full_name) VALUES ($1,$2,$3) RETURNING id")
                .bind(&email)
                .bind(hash)
                .bind(b.admin_name.trim())
                .fetch_one(&mut *tx)
                .await?;
            id
        }
    };
    sqlx::query("INSERT INTO memberships (user_id, tenant_id, role) VALUES ($1,$2,'hr_admin')")
        .bind(admin_id)
        .bind(tenant.id)
        .execute(&mut *tx)
        .await?;

    // Types de congés de départ, droits à 0 : à paramétrer par la société (aucune loi nationale codée en dur).
    sqlx::query("INSERT INTO leave_types (tenant_id, code, name, days_per_year, requires_balance) VALUES ($1,'ANNUAL','Congé annuel',0,TRUE),($1,'SICK','Congé maladie',0,FALSE)")
        .bind(tenant.id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(tenant.id),
        Some(auth.user_id),
        "tenant.create",
        "tenant",
        Some(tenant.id.to_string()),
        json!({"slug": tenant.slug}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(tenant)))
}

#[derive(Serialize, sqlx::FromRow)]
struct UserRow {
    user_id: Uuid,
    email: String,
    full_name: String,
    role: String,
    status: String,
}

async fn list_users(State(state): State<AppState>, auth: Auth) -> AppResult<Json<Vec<UserRow>>> {
    let t = auth.require(Permission::UsersManage)?;
    let rows = sqlx::query_as(
        "SELECT u.id AS user_id, u.email, u.full_name, m.role, u.status FROM memberships m JOIN users u ON u.id = m.user_id
         WHERE m.tenant_id = $1 ORDER BY u.full_name",
    )
    .bind(t.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateUser {
    email: String,
    full_name: String,
    password: String,
    role: String,
    employee_id: Option<Uuid>,
}

async fn create_user(
    State(state): State<AppState>,
    auth: Auth,
    Json(b): Json<CreateUser>,
) -> AppResult<(StatusCode, Json<UserRow>)> {
    let t = auth.require(Permission::UsersManage)?;
    ensure(valid_email(&b.email), "email invalide")?;
    ensure(!b.full_name.trim().is_empty(), "nom requis")?;
    let role = Role::parse(&b.role).ok_or_else(|| AppError::Validation("rôle inconnu".into()))?;
    password::validate_strength(&b.password).map_err(|m| AppError::Validation(m.into()))?;
    let email = b.email.trim().to_lowercase();

    let mut tx = state.db.begin().await?;
    let existing: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&mut *tx)
        .await?;
    let user_id = match existing {
        Some((id,)) => id,
        None => {
            let hash = password::hash(&b.password)?;
            let (id,): (Uuid,) = sqlx::query_as("INSERT INTO users (email, password_hash, full_name) VALUES ($1,$2,$3) RETURNING id")
                .bind(&email)
                .bind(hash)
                .bind(b.full_name.trim())
                .fetch_one(&mut *tx)
                .await?;
            id
        }
    };
    sqlx::query("INSERT INTO memberships (user_id, tenant_id, role) VALUES ($1,$2,$3)")
        .bind(user_id)
        .bind(t.tenant_id)
        .bind(role.as_str())
        .execute(&mut *tx)
        .await?;
    if let Some(eid) = b.employee_id {
        // Lie le compte au dossier employé de CETTE société uniquement.
        let res = sqlx::query("UPDATE employees SET user_id = $3, updated_at = now() WHERE id = $1 AND tenant_id = $2")
            .bind(eid)
            .bind(t.tenant_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        ensure(res.rows_affected() == 1, "employé introuvable")?;
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "user.create",
        "user",
        Some(user_id.to_string()),
        json!({"role": role.as_str()}),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query_as("SELECT u.id AS user_id, u.email, u.full_name, m.role, u.status FROM users u JOIN memberships m ON m.user_id = u.id WHERE u.id = $1 AND m.tenant_id = $2")
        .bind(user_id)
        .bind(t.tenant_id)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct SetRole {
    role: String,
}

async fn set_role(
    State(state): State<AppState>,
    auth: Auth,
    Path(user_id): Path<Uuid>,
    Json(b): Json<SetRole>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::UsersManage)?;
    let role = Role::parse(&b.role).ok_or_else(|| AppError::Validation("rôle inconnu".into()))?;
    ensure(
        user_id != auth.user_id,
        "vous ne pouvez pas modifier votre propre rôle",
    )?;
    let mut tx = state.db.begin().await?;
    let res = sqlx::query("UPDATE memberships SET role = $3 WHERE user_id = $1 AND tenant_id = $2")
        .bind(user_id)
        .bind(t.tenant_id)
        .bind(role.as_str())
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "user.role_change",
        "user",
        Some(user_id.to_string()),
        json!({"role": role.as_str()}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
