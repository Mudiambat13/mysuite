//! Congés : types, soldes, demandes et approbation.

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
    routing::{get, post, put},
};
use chrono::{Datelike, NaiveDate, Weekday};
use mysuite_shared::permissions::{Permission, Role, has_permission};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/leave/types", get(list_types).post(create_type))
        .route("/leave/balances", put(set_balance))
        .route("/leave/balances/me", get(my_balances))
        .route("/leave/requests", get(list_requests).post(create_request))
        .route("/leave/requests/{id}/decision", post(decide))
        .route("/leave/requests/{id}/cancel", post(cancel))
}

/// Jours ouvrés (lundi–vendredi) entre deux dates incluses.
/// Limite connue : les jours fériés par site ne sont pas encore pris en compte.
pub fn business_days(start: NaiveDate, end: NaiveDate) -> i64 {
    let mut n = 0;
    let mut d = start;
    while d <= end {
        if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            n += 1;
        }
        d = d.succ_opt().expect("date valide");
    }
    n
}

#[derive(Serialize, sqlx::FromRow)]
struct LeaveType {
    id: Uuid,
    code: String,
    name: String,
    days_per_year: Decimal,
    requires_balance: bool,
    active: bool,
}

async fn list_types(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<LeaveType>>> {
    let t = auth.tenant()?;
    Ok(Json(sqlx::query_as("SELECT id, code, name, days_per_year, requires_balance, active FROM leave_types WHERE tenant_id = $1 ORDER BY name").bind(t.tenant_id).fetch_all(&s.db).await?))
}

#[derive(Deserialize)]
struct NewType {
    code: String,
    name: String,
    days_per_year: Decimal,
    requires_balance: bool,
}

async fn create_type(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewType>,
) -> AppResult<(StatusCode, Json<LeaveType>)> {
    let t = auth.require(Permission::LeaveTypesManage)?;
    ensure(
        !b.code.trim().is_empty() && !b.name.trim().is_empty(),
        "code et nom requis",
    )?;
    ensure(
        b.days_per_year >= Decimal::ZERO,
        "droit annuel négatif interdit",
    )?;
    let row: LeaveType = sqlx::query_as(
        "INSERT INTO leave_types (tenant_id, code, name, days_per_year, requires_balance) VALUES ($1,$2,$3,$4,$5)
         RETURNING id, code, name, days_per_year, requires_balance, active",
    )
    .bind(t.tenant_id).bind(b.code.trim().to_uppercase()).bind(b.name.trim()).bind(b.days_per_year).bind(b.requires_balance)
    .fetch_one(&s.db).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "leave_type.create",
        "leave_type",
        Some(row.id.to_string()),
        json!({"code": row.code}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct BalanceBody {
    employee_id: Uuid,
    leave_type_id: Uuid,
    year: i32,
    entitled: Decimal,
}

async fn set_balance(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<BalanceBody>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::LeaveTypesManage)?;
    ensure(b.entitled >= Decimal::ZERO, "droit négatif interdit")?;
    let mut tx = s.db.begin().await?;
    // Les clés étrangères composites (employee_id, tenant_id) / (leave_type_id, tenant_id) bloquent tout accès inter-société.
    sqlx::query(
        "INSERT INTO leave_balances (tenant_id, employee_id, leave_type_id, year, entitled) VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (employee_id, leave_type_id, year) DO UPDATE SET entitled = EXCLUDED.entitled",
    )
    .bind(t.tenant_id).bind(b.employee_id).bind(b.leave_type_id).bind(b.year).bind(b.entitled)
    .execute(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "leave_balance.set",
        "employee",
        Some(b.employee_id.to_string()),
        json!({"year": b.year, "entitled": b.entitled}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
struct BalanceDto {
    leave_type_id: Uuid,
    leave_type_name: String,
    year: i32,
    entitled: Decimal,
    used: Decimal,
    pending: Decimal,
}

#[derive(Deserialize)]
struct YearQuery {
    year: Option<i32>,
}

async fn my_balances(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<YearQuery>,
) -> AppResult<Json<Vec<BalanceDto>>> {
    let t = auth.tenant()?;
    let eid = t.employee_id.ok_or(AppError::NotFound)?;
    let year = q.year.unwrap_or_else(|| chrono::Utc::now().year());
    let rows = sqlx::query_as(
        "SELECT lt.id AS leave_type_id, lt.name AS leave_type_name, $3::int AS year,
                COALESCE(b.entitled, 0) AS entitled,
                COALESCE((SELECT sum(days) FROM leave_requests r WHERE r.employee_id = $2 AND r.leave_type_id = lt.id AND r.status = 'approved' AND EXTRACT(YEAR FROM r.start_date) = $3), 0) AS used,
                COALESCE((SELECT sum(days) FROM leave_requests r WHERE r.employee_id = $2 AND r.leave_type_id = lt.id AND r.status = 'pending' AND EXTRACT(YEAR FROM r.start_date) = $3), 0) AS pending
         FROM leave_types lt LEFT JOIN leave_balances b ON b.leave_type_id = lt.id AND b.employee_id = $2 AND b.year = $3
         WHERE lt.tenant_id = $1 AND lt.active ORDER BY lt.name",
    )
    .bind(t.tenant_id).bind(eid).bind(year)
    .fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct RequestDto {
    id: Uuid,
    employee_id: Uuid,
    employee_name: String,
    leave_type_id: Uuid,
    leave_type_name: String,
    start_date: NaiveDate,
    end_date: NaiveDate,
    days: Decimal,
    reason: Option<String>,
    status: String,
    decision_comment: Option<String>,
}

const SELECT_REQUEST: &str = "
    SELECT r.id, r.employee_id, e.first_name || ' ' || e.last_name AS employee_name, r.leave_type_id, lt.name AS leave_type_name,
           r.start_date, r.end_date, r.days, r.reason, r.status, r.decision_comment
    FROM leave_requests r
    JOIN employees e ON e.id = r.employee_id AND e.tenant_id = r.tenant_id
    JOIN leave_types lt ON lt.id = r.leave_type_id AND lt.tenant_id = r.tenant_id";

#[derive(Deserialize)]
struct NewRequest {
    leave_type_id: Uuid,
    start_date: NaiveDate,
    end_date: NaiveDate,
    reason: Option<String>,
}

async fn create_request(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewRequest>,
) -> AppResult<(StatusCode, Json<RequestDto>)> {
    let t = auth.require(Permission::LeaveRequestOwn)?;
    let eid = t
        .employee_id
        .ok_or_else(|| AppError::Validation("aucun dossier employé lié à votre compte".into()))?;
    ensure(b.end_date >= b.start_date, "la fin précède le début")?;
    ensure(
        b.start_date.year() == b.end_date.year(),
        "une demande ne peut pas chevaucher deux années",
    )?;
    let days = business_days(b.start_date, b.end_date);
    ensure(days > 0, "la période ne contient aucun jour ouvré")?;
    let days = Decimal::from(days);

    let mut tx = s.db.begin().await?;
    // Verrouille la ligne employé pour sérialiser les demandes concurrentes (contrôle du solde et des chevauchements).
    sqlx::query("SELECT id FROM employees WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(eid)
        .bind(t.tenant_id)
        .fetch_one(&mut *tx)
        .await?;

    let lt: Option<(bool, bool)> = sqlx::query_as(
        "SELECT requires_balance, active FROM leave_types WHERE id = $1 AND tenant_id = $2",
    )
    .bind(b.leave_type_id)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (requires_balance, active) =
        lt.ok_or_else(|| AppError::Validation("type de congé inconnu".into()))?;
    ensure(active, "type de congé désactivé")?;

    let overlap: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM leave_requests WHERE employee_id = $1 AND tenant_id = $2 AND status IN ('pending','approved') AND start_date <= $4 AND end_date >= $3",
    )
    .bind(eid).bind(t.tenant_id).bind(b.start_date).bind(b.end_date).fetch_one(&mut *tx).await?;
    if overlap.0 > 0 {
        return Err(AppError::Conflict(
            "cette période chevauche une demande existante".into(),
        ));
    }

    if requires_balance {
        check_balance(
            &mut tx,
            t.tenant_id,
            eid,
            b.leave_type_id,
            b.start_date.year(),
            days,
        )
        .await?;
    }

    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO leave_requests (tenant_id, employee_id, leave_type_id, start_date, end_date, days, reason) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id",
    )
    .bind(t.tenant_id).bind(eid).bind(b.leave_type_id).bind(b.start_date).bind(b.end_date).bind(days).bind(&b.reason)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "leave.request",
        "leave_request",
        Some(id.to_string()),
        json!({"days": days}),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query_as(&format!(
        "{SELECT_REQUEST} WHERE r.id = $1 AND r.tenant_id = $2"
    ))
    .bind(id)
    .bind(t.tenant_id)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// Vérifie que droit − (approuvé + en attente) ≥ jours demandés.
async fn check_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    employee_id: Uuid,
    leave_type_id: Uuid,
    year: i32,
    days: Decimal,
) -> AppResult<()> {
    let (entitled,): (Decimal,) = sqlx::query_as("SELECT COALESCE((SELECT entitled FROM leave_balances WHERE employee_id = $1 AND leave_type_id = $2 AND year = $3 AND tenant_id = $4), 0)")
        .bind(employee_id).bind(leave_type_id).bind(year).bind(tenant_id).fetch_one(&mut **tx).await?;
    let (taken,): (Decimal,) = sqlx::query_as(
        "SELECT COALESCE(sum(days), 0) FROM leave_requests WHERE employee_id = $1 AND leave_type_id = $2 AND tenant_id = $4 AND status IN ('pending','approved') AND EXTRACT(YEAR FROM start_date) = $3",
    )
    .bind(employee_id).bind(leave_type_id).bind(year).bind(tenant_id).fetch_one(&mut **tx).await?;
    if entitled - taken < days {
        return Err(AppError::Validation(format!(
            "solde insuffisant : {} jour(s) disponible(s)",
            entitled - taken
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct ListQuery {
    scope: Option<String>,
    status: Option<String>,
}

async fn list_requests(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<RequestDto>>> {
    let t = auth.tenant()?;
    let scope = q.scope.as_deref().unwrap_or("mine");
    let (all, team, mine) = match scope {
        "all" => {
            ensure_perm(t, Permission::LeaveReadAll)?;
            (true, false, false)
        }
        "team" => {
            ensure_perm(t, Permission::LeaveApprove)?;
            // HR peut voir toute la société ; un manager seulement ses subordonnés.
            (!matches!(t.role, Role::Manager), true, false)
        }
        "mine" => (false, false, true),
        _ => return Err(AppError::Validation("scope invalide".into())),
    };
    let me = t.employee_id;
    let rows = sqlx::query_as(&format!(
        "{SELECT_REQUEST} WHERE r.tenant_id = $1 AND ($5::text IS NULL OR r.status = $5)
           AND ( $2::bool OR ($3::bool AND e.manager_id = $4) OR ($6::bool AND r.employee_id = $4) )
         ORDER BY r.created_at DESC LIMIT 200"
    ))
    .bind(t.tenant_id)
    .bind(all && !mine)
    .bind(team)
    .bind(me)
    .bind(&q.status)
    .bind(mine)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

fn ensure_perm(t: &TenantCtx, p: Permission) -> AppResult<()> {
    if has_permission(t.role, p) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[derive(Deserialize)]
struct Decision {
    approve: bool,
    comment: Option<String>,
}

async fn decide(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<Decision>,
) -> AppResult<Json<RequestDto>> {
    let t = auth.require(Permission::LeaveApprove)?;
    let mut tx = s.db.begin().await?;
    #[derive(sqlx::FromRow)]
    struct Req {
        employee_id: Uuid,
        leave_type_id: Uuid,
        start_date: NaiveDate,
        status: String,
        manager_id: Option<Uuid>,
    }
    let r: Req = sqlx::query_as(
        "SELECT r.employee_id, r.leave_type_id, r.start_date, r.status, e.manager_id
         FROM leave_requests r JOIN employees e ON e.id = r.employee_id AND e.tenant_id = r.tenant_id
         WHERE r.id = $1 AND r.tenant_id = $2 FOR UPDATE OF r",
    )
    .bind(id).bind(t.tenant_id).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;

    // Un manager ne décide que pour ses subordonnés directs ; personne ne valide sa propre demande.
    if matches!(t.role, Role::Manager) && r.manager_id != t.employee_id {
        return Err(AppError::Forbidden);
    }
    if t.employee_id == Some(r.employee_id) {
        return Err(AppError::Forbidden);
    }
    if r.status != "pending" {
        return Err(AppError::Conflict(
            "la demande n'est plus en attente".into(),
        ));
    }
    let status = if b.approve { "approved" } else { "rejected" };
    if b.approve {
        // Revérifie le solde (hors cette demande, déjà comptée en attente).
        let (requires_balance,): (bool,) = sqlx::query_as(
            "SELECT requires_balance FROM leave_types WHERE id = $1 AND tenant_id = $2",
        )
        .bind(r.leave_type_id)
        .bind(t.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
        if requires_balance {
            check_balance(
                &mut tx,
                t.tenant_id,
                r.employee_id,
                r.leave_type_id,
                r.start_date.year(),
                Decimal::ZERO,
            )
            .await?;
        }
    }
    sqlx::query("UPDATE leave_requests SET status = $3, decided_by = $4, decided_at = now(), decision_comment = $5 WHERE id = $1 AND tenant_id = $2")
        .bind(id).bind(t.tenant_id).bind(status).bind(auth.user_id).bind(&b.comment).execute(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        if b.approve {
            "leave.approve"
        } else {
            "leave.reject"
        },
        "leave_request",
        Some(id.to_string()),
        json!({"comment": b.comment}),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query_as(&format!(
        "{SELECT_REQUEST} WHERE r.id = $1 AND r.tenant_id = $2"
    ))
    .bind(id)
    .bind(t.tenant_id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

async fn cancel(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::LeaveRequestOwn)?;
    let mut tx = s.db.begin().await?;
    // Seul le demandeur annule, tant que la demande est en attente ou que le congé n'a pas commencé.
    let res = sqlx::query(
        "UPDATE leave_requests SET status = 'cancelled' WHERE id = $1 AND tenant_id = $2 AND employee_id = $3
           AND ((status = 'pending') OR (status = 'approved' AND start_date > current_date))",
    )
    .bind(id).bind(t.tenant_id).bind(t.employee_id).execute(&mut *tx).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "leave.cancel",
        "leave_request",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn counts_only_weekdays() {
        // lundi 2 → dimanche 8 mars 2026 : 5 jours ouvrés
        assert_eq!(business_days(d(2026, 3, 2), d(2026, 3, 8)), 5);
        assert_eq!(business_days(d(2026, 3, 7), d(2026, 3, 8)), 0);
        assert_eq!(business_days(d(2026, 3, 2), d(2026, 3, 2)), 1);
    }
}
