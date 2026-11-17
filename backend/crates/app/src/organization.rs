//! Sites, départements et postes.

use crate::{
    audit,
    auth::Auth,
    error::{AppResult, ensure},
    state::AppState,
};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use mysuite_shared::permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/org/sites", get(list_sites).post(create_site))
        .route(
            "/org/departments",
            get(list_departments).post(create_department),
        )
        .route("/org/positions", get(list_positions).post(create_position))
}

#[derive(Serialize, sqlx::FromRow)]
struct Site {
    id: Uuid,
    name: String,
    city: Option<String>,
}
#[derive(Deserialize)]
struct NewSite {
    name: String,
    city: Option<String>,
}

async fn list_sites(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<Site>>> {
    let t = auth.require(Permission::OrgRead)?;
    Ok(Json(
        sqlx::query_as("SELECT id, name, city FROM sites WHERE tenant_id = $1 ORDER BY name")
            .bind(t.tenant_id)
            .fetch_all(&s.db)
            .await?,
    ))
}

async fn create_site(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewSite>,
) -> AppResult<(StatusCode, Json<Site>)> {
    let t = auth.require(Permission::OrgManage)?;
    ensure(!b.name.trim().is_empty(), "nom requis")?;
    let row: Site = sqlx::query_as(
        "INSERT INTO sites (tenant_id, name, city) VALUES ($1,$2,$3) RETURNING id, name, city",
    )
    .bind(t.tenant_id)
    .bind(b.name.trim())
    .bind(b.city)
    .fetch_one(&s.db)
    .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "site.create",
        "site",
        Some(row.id.to_string()),
        json!({"name": row.name}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Serialize, sqlx::FromRow)]
struct Department {
    id: Uuid,
    name: String,
    parent_id: Option<Uuid>,
}
#[derive(Deserialize)]
struct NewDepartment {
    name: String,
    parent_id: Option<Uuid>,
}

async fn list_departments(
    State(s): State<AppState>,
    auth: Auth,
) -> AppResult<Json<Vec<Department>>> {
    let t = auth.require(Permission::OrgRead)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT id, name, parent_id FROM departments WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(t.tenant_id)
        .fetch_all(&s.db)
        .await?,
    ))
}

async fn create_department(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewDepartment>,
) -> AppResult<(StatusCode, Json<Department>)> {
    let t = auth.require(Permission::OrgManage)?;
    ensure(!b.name.trim().is_empty(), "nom requis")?;
    // La clé étrangère composite (parent_id, tenant_id) empêche un parent d'une autre société.
    let row: Department = sqlx::query_as("INSERT INTO departments (tenant_id, name, parent_id) VALUES ($1,$2,$3) RETURNING id, name, parent_id")
        .bind(t.tenant_id)
        .bind(b.name.trim())
        .bind(b.parent_id)
        .fetch_one(&s.db)
        .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "department.create",
        "department",
        Some(row.id.to_string()),
        json!({"name": row.name}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Serialize, sqlx::FromRow)]
struct Position {
    id: Uuid,
    title: String,
    level: Option<String>,
}
#[derive(Deserialize)]
struct NewPosition {
    title: String,
    level: Option<String>,
}

async fn list_positions(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<Position>>> {
    let t = auth.require(Permission::OrgRead)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT id, title, level FROM positions WHERE tenant_id = $1 ORDER BY title",
        )
        .bind(t.tenant_id)
        .fetch_all(&s.db)
        .await?,
    ))
}

async fn create_position(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewPosition>,
) -> AppResult<(StatusCode, Json<Position>)> {
    let t = auth.require(Permission::OrgManage)?;
    ensure(!b.title.trim().is_empty(), "intitulé requis")?;
    let row: Position = sqlx::query_as("INSERT INTO positions (tenant_id, title, level) VALUES ($1,$2,$3) RETURNING id, title, level")
        .bind(t.tenant_id)
        .bind(b.title.trim())
        .bind(b.level)
        .fetch_one(&s.db)
        .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "position.create",
        "position",
        Some(row.id.to_string()),
        json!({"title": row.title}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}
