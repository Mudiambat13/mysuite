//! Contrats et avenants. Les avenants s'ajoutent à l'historique ; un contrat n'est jamais supprimé.

use crate::{
    audit,
    auth::{Auth, TenantCtx},
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{Duration, NaiveDate, Utc};
use mysuite_shared::permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/contracts", get(list).post(create))
        .route("/contracts/expiring", get(expiring))
        .route("/contracts/{id}/terminate", post(terminate))
        .route(
            "/contracts/{id}/amendments",
            get(list_amendments).post(add_amendment),
        )
}

const TYPES: [&str; 5] = ["cdi", "cdd", "stage", "journalier", "consultant"];

#[derive(Serialize, sqlx::FromRow)]
struct ContractDto {
    id: Uuid,
    employee_id: Uuid,
    employee_name: String,
    matricule: String,
    contract_type: String,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    probation_end: Option<NaiveDate>,
    job_title: Option<String>,
    status: String,
    notes: Option<String>,
}

const SELECT: &str = "
    SELECT c.id, c.employee_id, e.first_name || ' ' || e.last_name AS employee_name, e.matricule,
           c.contract_type, c.start_date, c.end_date, c.probation_end, c.job_title, c.status, c.notes
    FROM contracts c JOIN employees e ON e.id = c.employee_id AND e.tenant_id = c.tenant_id";

#[derive(Deserialize)]
struct ListQuery {
    employee_id: Option<Uuid>,
    status: Option<String>,
}

async fn list(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ContractDto>>> {
    let t = auth.require(Permission::ContractsRead)?;
    let rows = sqlx::query_as(&format!(
        "{SELECT} WHERE c.tenant_id = $1 AND ($2::uuid IS NULL OR c.employee_id = $2)
           AND ($3::text IS NULL OR c.status = $3) ORDER BY c.start_date DESC LIMIT 500"
    ))
    .bind(t.tenant_id)
    .bind(q.employee_id)
    .bind(&q.status)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct NewContract {
    employee_id: Uuid,
    contract_type: String,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    probation_end: Option<NaiveDate>,
    job_title: Option<String>,
    notes: Option<String>,
}

async fn fetch(s: &AppState, t: &TenantCtx, id: Uuid) -> AppResult<ContractDto> {
    sqlx::query_as(&format!("{SELECT} WHERE c.id = $1 AND c.tenant_id = $2"))
        .bind(id)
        .bind(t.tenant_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn create(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewContract>,
) -> AppResult<(StatusCode, Json<ContractDto>)> {
    let t = auth.require(Permission::ContractsManage)?;
    ensure(
        TYPES.contains(&b.contract_type.as_str()),
        "type de contrat invalide",
    )?;
    if let Some(e) = b.end_date {
        ensure(e >= b.start_date, "la fin précède le début")?;
    }
    if b.contract_type == "cdd" {
        ensure(b.end_date.is_some(), "un CDD doit avoir une date de fin")?;
    }
    if let Some(p) = b.probation_end {
        ensure(
            p >= b.start_date,
            "la fin de période d'essai précède le début",
        )?;
    }
    let mut tx = s.db.begin().await?;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO contracts (tenant_id, employee_id, contract_type, start_date, end_date, probation_end, job_title, notes, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(t.tenant_id)
    .bind(b.employee_id)
    .bind(&b.contract_type)
    .bind(b.start_date)
    .bind(b.end_date)
    .bind(b.probation_end)
    .bind(&b.job_title)
    .bind(&b.notes)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict(_) => AppError::Conflict("cet employé a déjà un contrat actif".into()),
        other => other,
    })?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "contract.create",
        "contract",
        Some(id.to_string()),
        json!({"employee_id": b.employee_id, "type": b.contract_type}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(fetch(&s, t, id).await?)))
}

#[derive(Deserialize)]
struct ExpiringQuery {
    days: Option<i64>,
}

/// Contrats actifs arrivant à échéance (ou en période d'essai) dans les N prochains jours (défaut 60, max 365).
async fn expiring(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<ExpiringQuery>,
) -> AppResult<Json<Vec<ContractDto>>> {
    let t = auth.require(Permission::ContractsRead)?;
    let days = q.days.unwrap_or(60).clamp(1, 365);
    let limit = Utc::now().date_naive() + Duration::days(days);
    let rows = sqlx::query_as(&format!(
        "{SELECT} WHERE c.tenant_id = $1 AND c.status = 'active'
           AND ((c.end_date IS NOT NULL AND c.end_date <= $2) OR (c.probation_end IS NOT NULL AND c.probation_end >= current_date AND c.probation_end <= $2))
         ORDER BY COALESCE(c.end_date, c.probation_end)"
    ))
    .bind(t.tenant_id)
    .bind(limit)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct Terminate {
    end_date: NaiveDate,
}

async fn terminate(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<Terminate>,
) -> AppResult<Json<ContractDto>> {
    let t = auth.require(Permission::ContractsManage)?;
    let mut tx = s.db.begin().await?;
    let cur: (NaiveDate, String) = sqlx::query_as(
        "SELECT start_date, status FROM contracts WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if cur.1 != "active" {
        return Err(AppError::Conflict("le contrat n'est pas actif".into()));
    }
    ensure(b.end_date >= cur.0, "la fin précède le début du contrat")?;
    sqlx::query("UPDATE contracts SET status = 'terminated', end_date = $3 WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(t.tenant_id)
        .bind(b.end_date)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "contract.terminate",
        "contract",
        Some(id.to_string()),
        json!({"end_date": b.end_date}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(fetch(&s, t, id).await?))
}

#[derive(Serialize, sqlx::FromRow)]
struct AmendmentDto {
    id: Uuid,
    effective_date: NaiveDate,
    description: String,
    new_end_date: Option<NaiveDate>,
}

async fn list_amendments(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<AmendmentDto>>> {
    let t = auth.require(Permission::ContractsRead)?;
    fetch(&s, t, id).await?;
    let rows = sqlx::query_as(
        "SELECT id, effective_date, description, new_end_date FROM contract_amendments WHERE contract_id = $1 AND tenant_id = $2 ORDER BY effective_date, created_at",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct NewAmendment {
    effective_date: NaiveDate,
    description: String,
    new_end_date: Option<NaiveDate>,
}

async fn add_amendment(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<NewAmendment>,
) -> AppResult<(StatusCode, Json<AmendmentDto>)> {
    let t = auth.require(Permission::ContractsManage)?;
    ensure(!b.description.trim().is_empty(), "description requise")?;
    let mut tx = s.db.begin().await?;
    let cur: (NaiveDate, String) = sqlx::query_as(
        "SELECT start_date, status FROM contracts WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if cur.1 != "active" {
        return Err(AppError::Conflict("le contrat n'est pas actif".into()));
    }
    if let Some(e) = b.new_end_date {
        ensure(e >= cur.0, "la nouvelle fin précède le début du contrat")?;
    }
    let row: AmendmentDto = sqlx::query_as(
        "INSERT INTO contract_amendments (tenant_id, contract_id, effective_date, description, new_end_date, created_by)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id, effective_date, description, new_end_date",
    )
    .bind(t.tenant_id)
    .bind(id)
    .bind(b.effective_date)
    .bind(b.description.trim())
    .bind(b.new_end_date)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;
    // Un avenant de prolongation met à jour la date de fin effective ; l'historique reste dans l'avenant.
    if let Some(e) = b.new_end_date {
        sqlx::query("UPDATE contracts SET end_date = $3 WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(t.tenant_id)
            .bind(e)
            .execute(&mut *tx)
            .await?;
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "contract.amend",
        "contract",
        Some(id.to_string()),
        json!({"amendment_id": row.id}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}
