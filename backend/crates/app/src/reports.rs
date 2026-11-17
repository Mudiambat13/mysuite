//! Indicateurs du tableau de bord, calculés côté serveur.

use crate::{auth::Auth, error::AppResult, state::AppState};
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use mysuite_shared::{
    pagination::{Page, PageParams},
    permissions::{Permission, Role},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/reports/headcount", get(headcount))
        .route("/dashboard", get(dashboard))
        .route("/audit", get(audit_log))
}

#[derive(Serialize, sqlx::FromRow)]
struct Bucket {
    label: String,
    count: i64,
}

#[derive(Serialize)]
struct Headcount {
    total_active: i64,
    by_status: Vec<Bucket>,
    by_department: Vec<Bucket>,
    by_site: Vec<Bucket>,
    by_contract: Vec<Bucket>,
}

async fn headcount(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Headcount>> {
    let t = auth.require(Permission::ReportsRead)?;
    let by_status = sqlx::query_as("SELECT status AS label, count(*) AS count FROM employees WHERE tenant_id = $1 GROUP BY status ORDER BY count DESC").bind(t.tenant_id).fetch_all(&s.db).await?;
    let by_department = sqlx::query_as("SELECT COALESCE(d.name, 'Non affecté') AS label, count(*) AS count FROM employees e LEFT JOIN departments d ON d.id = e.department_id WHERE e.tenant_id = $1 AND e.status IN ('active','on_leave') GROUP BY 1 ORDER BY 2 DESC").bind(t.tenant_id).fetch_all(&s.db).await?;
    let by_site = sqlx::query_as("SELECT COALESCE(st.name, 'Non affecté') AS label, count(*) AS count FROM employees e LEFT JOIN sites st ON st.id = e.site_id WHERE e.tenant_id = $1 AND e.status IN ('active','on_leave') GROUP BY 1 ORDER BY 2 DESC").bind(t.tenant_id).fetch_all(&s.db).await?;
    let by_contract = sqlx::query_as("SELECT contract_type AS label, count(*) AS count FROM employees WHERE tenant_id = $1 AND status IN ('active','on_leave') GROUP BY 1 ORDER BY 2 DESC").bind(t.tenant_id).fetch_all(&s.db).await?;
    let total_active: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM employees WHERE tenant_id = $1 AND status IN ('active','on_leave')",
    )
    .bind(t.tenant_id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(Headcount {
        total_active: total_active.0,
        by_status,
        by_department,
        by_site,
        by_contract,
    }))
}

#[derive(Serialize)]
struct Dashboard {
    active_employees: Option<i64>,
    pending_leave_to_review: Option<i64>,
    my_pending_leave: i64,
    on_leave_today: Option<i64>,
}

async fn dashboard(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Dashboard>> {
    let t = auth.tenant()?;
    let can_see_staff =
        mysuite_shared::permissions::has_permission(t.role, Permission::EmployeesRead);
    let active = if can_see_staff && !matches!(t.role, Role::Manager) {
        Some(sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM employees WHERE tenant_id = $1 AND status IN ('active','on_leave')").bind(t.tenant_id).fetch_one(&s.db).await?.0)
    } else {
        None
    };
    let to_review = if mysuite_shared::permissions::has_permission(t.role, Permission::LeaveApprove)
    {
        let all = !matches!(t.role, Role::Manager);
        Some(sqlx::query_as::<_, (i64,)>(
            "SELECT count(*) FROM leave_requests r JOIN employees e ON e.id = r.employee_id AND e.tenant_id = r.tenant_id
             WHERE r.tenant_id = $1 AND r.status = 'pending' AND ($2::bool OR e.manager_id = $3) AND r.employee_id IS DISTINCT FROM $3",
        ).bind(t.tenant_id).bind(all).bind(t.employee_id).fetch_one(&s.db).await?.0)
    } else {
        None
    };
    let mine = sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM leave_requests WHERE tenant_id = $1 AND employee_id = $2 AND status = 'pending'").bind(t.tenant_id).bind(t.employee_id).fetch_one(&s.db).await?.0;
    let on_leave = if mysuite_shared::permissions::has_permission(t.role, Permission::LeaveReadAll)
    {
        Some(sqlx::query_as::<_, (i64,)>("SELECT count(DISTINCT employee_id) FROM leave_requests WHERE tenant_id = $1 AND status = 'approved' AND current_date BETWEEN start_date AND end_date").bind(t.tenant_id).fetch_one(&s.db).await?.0)
    } else {
        None
    };
    Ok(Json(Dashboard {
        active_employees: active,
        pending_leave_to_review: to_review,
        my_pending_leave: mine,
        on_leave_today: on_leave,
    }))
}

#[derive(Serialize, sqlx::FromRow)]
struct AuditRow {
    id: i64,
    actor_id: Option<Uuid>,
    actor_name: Option<String>,
    action: String,
    entity: String,
    entity_id: Option<String>,
    details: Value,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct AuditQuery {
    page: Option<i64>,
    limit: Option<i64>,
}

async fn audit_log(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<Page<AuditRow>>> {
    let t = auth.require(Permission::AuditRead)?;
    let params = PageParams {
        page: q.page,
        limit: q.limit,
    };
    let (page, limit) = params.normalized();
    let total: (i64,) = sqlx::query_as("SELECT count(*) FROM audit_logs WHERE tenant_id = $1")
        .bind(t.tenant_id)
        .fetch_one(&s.db)
        .await?;
    let items = sqlx::query_as(
        "SELECT a.id, a.actor_id, u.full_name AS actor_name, a.action, a.entity, a.entity_id, a.details, a.created_at
         FROM audit_logs a LEFT JOIN users u ON u.id = a.actor_id WHERE a.tenant_id = $1 ORDER BY a.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(t.tenant_id).bind(limit).bind(params.offset()).fetch_all(&s.db).await?;
    Ok(Json(Page {
        items,
        total: total.0,
        page,
        limit,
    }))
}
