//! Dossiers employés, rémunération et éléments de paie récurrents.

use crate::{
    audit,
    auth::{Auth, TenantCtx},
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use chrono::NaiveDate;
use mysuite_shared::{
    pagination::{Page, PageParams},
    permissions::{Permission, Role},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/employees", get(list).post(create))
        .route("/employees/me", get(me))
        .route("/employees/export.csv", get(export_csv))
        .route("/employees/{id}", get(detail).put(update))
        .route(
            "/employees/{id}/compensation",
            get(list_compensation).post(add_compensation),
        )
        .route(
            "/employees/{id}/dependents",
            get(list_dependents).post(add_dependents),
        )
        .route(
            "/employees/{id}/pay-items",
            get(list_pay_items).post(add_pay_item),
        )
}

const STATUSES: [&str; 5] = ["candidate", "active", "suspended", "on_leave", "exited"];
const CONTRACTS: [&str; 5] = ["cdi", "cdd", "stage", "journalier", "consultant"];

#[derive(Serialize, sqlx::FromRow)]
pub struct EmployeeDto {
    id: Uuid,
    matricule: String,
    first_name: String,
    last_name: String,
    email: Option<String>,
    phone: Option<String>,
    birth_date: Option<NaiveDate>,
    hire_date: NaiveDate,
    exit_date: Option<NaiveDate>,
    status: String,
    contract_type: String,
    site_id: Option<Uuid>,
    site_name: Option<String>,
    department_id: Option<Uuid>,
    department_name: Option<String>,
    position_id: Option<Uuid>,
    position_title: Option<String>,
    manager_id: Option<Uuid>,
}

const SELECT_EMPLOYEE: &str = "
    SELECT e.id, e.matricule, e.first_name, e.last_name, e.email, e.phone, e.birth_date, e.hire_date, e.exit_date,
           e.status, e.contract_type, e.site_id, s.name AS site_name, e.department_id, d.name AS department_name,
           e.position_id, p.title AS position_title, e.manager_id
    FROM employees e
    LEFT JOIN sites s ON s.id = e.site_id
    LEFT JOIN departments d ON d.id = e.department_id
    LEFT JOIN positions p ON p.id = e.position_id";

/// Périmètre de visibilité : RH, paie et audit voient toute la société ; un manager voit lui-même et ses subordonnés directs.
fn scope(t: &TenantCtx) -> (bool, Option<Uuid>) {
    (!matches!(t.role, Role::Manager), t.employee_id)
}

#[derive(Deserialize)]
struct ListQuery {
    search: Option<String>,
    status: Option<String>,
    department_id: Option<Uuid>,
    site_id: Option<Uuid>,
    page: Option<i64>,
    limit: Option<i64>,
}

impl ListQuery {
    fn params(&self) -> PageParams {
        PageParams {
            page: self.page,
            limit: self.limit,
        }
    }
}

fn like_pattern(s: &str) -> String {
    let escaped = s
        .trim()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

const WHERE_LIST: &str = "
    WHERE e.tenant_id = $1
      AND ($2::bool OR e.id = $3 OR e.manager_id = $3)
      AND ($4::text IS NULL OR e.first_name ILIKE $4 OR e.last_name ILIKE $4 OR e.matricule ILIKE $4 OR e.email ILIKE $4
           OR (e.first_name || ' ' || e.last_name) ILIKE $4)
      AND ($5::text IS NULL OR e.status = $5)
      AND ($6::uuid IS NULL OR e.department_id = $6)
      AND ($7::uuid IS NULL OR e.site_id = $7)";

async fn list(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Page<EmployeeDto>>> {
    let t = auth.require(Permission::EmployeesRead)?;
    let (all, me) = scope(t);
    let search = q
        .search
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .map(like_pattern);
    let (page, limit) = q.params().normalized();
    let total: (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM employees e {WHERE_LIST}"))
        .bind(t.tenant_id)
        .bind(all)
        .bind(me)
        .bind(&search)
        .bind(&q.status)
        .bind(q.department_id)
        .bind(q.site_id)
        .fetch_one(&s.db)
        .await?;
    let items: Vec<EmployeeDto> = sqlx::query_as(&format!(
        "{SELECT_EMPLOYEE} {WHERE_LIST} ORDER BY e.last_name, e.first_name LIMIT $8 OFFSET $9"
    ))
    .bind(t.tenant_id)
    .bind(all)
    .bind(me)
    .bind(&search)
    .bind(&q.status)
    .bind(q.department_id)
    .bind(q.site_id)
    .bind(limit)
    .bind(q.params().offset())
    .fetch_all(&s.db)
    .await?;
    Ok(Json(Page {
        items,
        total: total.0,
        page,
        limit,
    }))
}

async fn me(State(s): State<AppState>, auth: Auth) -> AppResult<Json<EmployeeDto>> {
    let t = auth.tenant()?;
    let eid = t.employee_id.ok_or(AppError::NotFound)?;
    let row = sqlx::query_as(&format!(
        "{SELECT_EMPLOYEE} WHERE e.tenant_id = $1 AND e.id = $2"
    ))
    .bind(t.tenant_id)
    .bind(eid)
    .fetch_optional(&s.db)
    .await?;
    row.map(Json).ok_or(AppError::NotFound)
}

async fn fetch_visible(s: &AppState, t: &TenantCtx, id: Uuid) -> AppResult<EmployeeDto> {
    let (all, me) = scope(t);
    // Un employé hors périmètre ou d'une autre société renvoie 404 (pas de fuite d'existence).
    sqlx::query_as(&format!("{SELECT_EMPLOYEE} WHERE e.tenant_id = $1 AND e.id = $2 AND ($3::bool OR e.id = $4 OR e.manager_id = $4)"))
        .bind(t.tenant_id).bind(id).bind(all).bind(me)
        .fetch_optional(&s.db).await?
        .ok_or(AppError::NotFound)
}

async fn detail(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<EmployeeDto>> {
    let t = auth.require(Permission::EmployeesRead)?;
    Ok(Json(fetch_visible(&s, t, id).await?))
}

#[derive(Deserialize)]
struct EmployeeBody {
    matricule: String,
    first_name: String,
    last_name: String,
    email: Option<String>,
    phone: Option<String>,
    birth_date: Option<NaiveDate>,
    hire_date: NaiveDate,
    exit_date: Option<NaiveDate>,
    status: Option<String>,
    contract_type: Option<String>,
    site_id: Option<Uuid>,
    department_id: Option<Uuid>,
    position_id: Option<Uuid>,
    manager_id: Option<Uuid>,
}

fn validate(b: &EmployeeBody) -> AppResult<(String, String)> {
    ensure(!b.matricule.trim().is_empty(), "matricule requis")?;
    ensure(
        !b.first_name.trim().is_empty() && !b.last_name.trim().is_empty(),
        "nom et prénom requis",
    )?;
    let status = b.status.clone().unwrap_or_else(|| "active".into());
    ensure(STATUSES.contains(&status.as_str()), "statut invalide")?;
    let contract = b.contract_type.clone().unwrap_or_else(|| "cdi".into());
    ensure(
        CONTRACTS.contains(&contract.as_str()),
        "type de contrat invalide",
    )?;
    if let Some(x) = b.exit_date {
        ensure(
            x >= b.hire_date,
            "la date de sortie précède la date d'entrée",
        )?;
    }
    if status == "exited" {
        ensure(
            b.exit_date.is_some(),
            "date de sortie requise pour un employé sorti",
        )?;
    }
    Ok((status, contract))
}

async fn create(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<EmployeeBody>,
) -> AppResult<(StatusCode, Json<EmployeeDto>)> {
    let t = auth.require(Permission::EmployeesManage)?;
    let (status, contract) = validate(&b)?;
    let mut tx = s.db.begin().await?;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO employees (tenant_id, matricule, first_name, last_name, email, phone, birth_date, hire_date, exit_date, status, contract_type,
                                site_id, department_id, position_id, manager_id, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING id",
    )
    .bind(t.tenant_id).bind(b.matricule.trim()).bind(b.first_name.trim()).bind(b.last_name.trim())
    .bind(&b.email).bind(&b.phone).bind(b.birth_date).bind(b.hire_date).bind(b.exit_date).bind(&status).bind(&contract)
    .bind(b.site_id).bind(b.department_id).bind(b.position_id).bind(b.manager_id).bind(auth.user_id)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "employee.create",
        "employee",
        Some(id.to_string()),
        json!({"matricule": b.matricule.trim()}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(fetch_visible(&s, t, id).await?)))
}

async fn update(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<EmployeeBody>,
) -> AppResult<Json<EmployeeDto>> {
    let t = auth.require(Permission::EmployeesManage)?;
    let (status, contract) = validate(&b)?;
    ensure(
        b.manager_id != Some(id),
        "un employé ne peut pas être son propre manager",
    )?;
    let mut tx = s.db.begin().await?;
    let res = sqlx::query(
        "UPDATE employees SET matricule=$3, first_name=$4, last_name=$5, email=$6, phone=$7, birth_date=$8, hire_date=$9, exit_date=$10,
                status=$11, contract_type=$12, site_id=$13, department_id=$14, position_id=$15, manager_id=$16, updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id).bind(t.tenant_id).bind(b.matricule.trim()).bind(b.first_name.trim()).bind(b.last_name.trim())
    .bind(&b.email).bind(&b.phone).bind(b.birth_date).bind(b.hire_date).bind(b.exit_date).bind(&status).bind(&contract)
    .bind(b.site_id).bind(b.department_id).bind(b.position_id).bind(b.manager_id)
    .execute(&mut *tx).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "employee.update",
        "employee",
        Some(id.to_string()),
        json!({"status": status}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(fetch_visible(&s, t, id).await?))
}

/// Protège contre l'injection de formules dans Excel/LibreOffice à l'ouverture du CSV.
pub(crate) fn csv_cell(v: &str) -> String {
    let mut v = v.to_string();
    if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        v.insert(0, '\'');
    }
    if v.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

async fn export_csv(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::EmployeesRead)?;
    let (all, me) = scope(t);
    let search = q
        .search
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .map(like_pattern);
    let rows: Vec<EmployeeDto> = sqlx::query_as(&format!(
        "{SELECT_EMPLOYEE} {WHERE_LIST} ORDER BY e.last_name, e.first_name LIMIT 10000"
    ))
    .bind(t.tenant_id)
    .bind(all)
    .bind(me)
    .bind(&search)
    .bind(&q.status)
    .bind(q.department_id)
    .bind(q.site_id)
    .fetch_all(&s.db)
    .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "employee.export",
        "employee",
        None,
        json!({"rows": rows.len()}),
    )
    .await?;
    let mut out = String::from(
        "matricule,nom,prenom,email,statut,contrat,entree,sortie,departement,site,poste\n",
    );
    for e in rows {
        let cells = [
            e.matricule,
            e.last_name,
            e.first_name,
            e.email.unwrap_or_default(),
            e.status,
            e.contract_type,
            e.hire_date.to_string(),
            e.exit_date.map(|d| d.to_string()).unwrap_or_default(),
            e.department_name.unwrap_or_default(),
            e.site_name.unwrap_or_default(),
            e.position_title.unwrap_or_default(),
        ];
        out.push_str(
            &cells
                .iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"employes.csv\"",
            ),
        ],
        out,
    ))
}

#[derive(Serialize, sqlx::FromRow)]
struct CompensationDto {
    id: Uuid,
    base_salary: Decimal,
    currency: String,
    effective_from: NaiveDate,
}

#[derive(Deserialize)]
struct CompensationBody {
    base_salary: Decimal,
    currency: Option<String>,
    effective_from: NaiveDate,
}

async fn ensure_employee_in_tenant(s: &AppState, t: &TenantCtx, id: Uuid) -> AppResult<()> {
    let found: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM employees WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(t.tenant_id)
            .fetch_optional(&s.db)
            .await?;
    found.map(|_| ()).ok_or(AppError::NotFound)
}

async fn list_compensation(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<CompensationDto>>> {
    let t = auth.require(Permission::CompensationRead)?;
    ensure_employee_in_tenant(&s, t, id).await?;
    // Consultation d'une donnée salariale : tracée.
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "compensation.read",
        "employee",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    let rows = sqlx::query_as("SELECT id, base_salary, currency, effective_from FROM employee_compensation WHERE employee_id = $1 AND tenant_id = $2 ORDER BY effective_from DESC")
        .bind(id).bind(t.tenant_id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

async fn add_compensation(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<CompensationBody>,
) -> AppResult<(StatusCode, Json<CompensationDto>)> {
    let t = auth.require(Permission::CompensationManage)?;
    ensure(b.base_salary >= Decimal::ZERO, "salaire négatif interdit")?;
    ensure_employee_in_tenant(&s, t, id).await?;
    let currency = match b.currency {
        Some(c) => c,
        None => {
            sqlx::query_as::<_, (String,)>("SELECT currency FROM tenants WHERE id = $1")
                .bind(t.tenant_id)
                .fetch_one(&s.db)
                .await?
                .0
        }
    };
    let mut tx = s.db.begin().await?;
    let row: CompensationDto = sqlx::query_as(
        "INSERT INTO employee_compensation (tenant_id, employee_id, base_salary, currency, effective_from, created_by)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id, base_salary, currency, effective_from",
    )
    .bind(t.tenant_id).bind(id).bind(b.base_salary).bind(currency).bind(b.effective_from).bind(auth.user_id)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "compensation.add",
        "employee",
        Some(id.to_string()),
        json!({"effective_from": b.effective_from}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Serialize, sqlx::FromRow)]
struct DependentsDto {
    id: Uuid,
    dependents: i16,
    effective_from: NaiveDate,
}

#[derive(Deserialize)]
struct DependentsBody {
    dependents: i16,
    effective_from: NaiveDate,
}

/// Personnes fiscalement à charge (donnée familiale qui influe sur l'impôt) : mêmes droits que la rémunération.
async fn list_dependents(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<DependentsDto>>> {
    let t = auth.require(Permission::CompensationRead)?;
    ensure_employee_in_tenant(&s, t, id).await?;
    let rows = sqlx::query_as("SELECT id, dependents, effective_from FROM employee_dependents WHERE employee_id = $1 AND tenant_id = $2 ORDER BY effective_from DESC")
        .bind(id).bind(t.tenant_id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

async fn add_dependents(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<DependentsBody>,
) -> AppResult<(StatusCode, Json<DependentsDto>)> {
    let t = auth.require(Permission::CompensationManage)?;
    ensure(
        (0..=30).contains(&b.dependents),
        "nombre de personnes à charge entre 0 et 30",
    )?;
    ensure_employee_in_tenant(&s, t, id).await?;
    let mut tx = s.db.begin().await?;
    let row: DependentsDto = sqlx::query_as(
        "INSERT INTO employee_dependents (tenant_id, employee_id, dependents, effective_from, created_by)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (employee_id, effective_from) DO UPDATE SET dependents = EXCLUDED.dependents, created_by = EXCLUDED.created_by
         RETURNING id, dependents, effective_from",
    )
    .bind(t.tenant_id).bind(id).bind(b.dependents).bind(b.effective_from).bind(auth.user_id)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "employee.dependents",
        "employee",
        Some(id.to_string()),
        json!({"dependents": b.dependents, "effective_from": b.effective_from}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Serialize, sqlx::FromRow)]
struct PayItemDto {
    id: Uuid,
    kind: String,
    code: String,
    label: String,
    amount: Decimal,
    active_from: NaiveDate,
    active_to: Option<NaiveDate>,
}

#[derive(Deserialize)]
struct PayItemBody {
    kind: String,
    code: String,
    label: String,
    amount: Decimal,
    active_from: NaiveDate,
    active_to: Option<NaiveDate>,
}

async fn list_pay_items(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<PayItemDto>>> {
    let t = auth.require(Permission::CompensationRead)?;
    ensure_employee_in_tenant(&s, t, id).await?;
    let rows = sqlx::query_as("SELECT id, kind, code, label, amount, active_from, active_to FROM employee_pay_items WHERE employee_id = $1 AND tenant_id = $2 ORDER BY active_from DESC")
        .bind(id).bind(t.tenant_id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

async fn add_pay_item(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<PayItemBody>,
) -> AppResult<(StatusCode, Json<PayItemDto>)> {
    let t = auth.require(Permission::CompensationManage)?;
    ensure(
        ["allowance", "deduction"].contains(&b.kind.as_str()),
        "type invalide (allowance ou deduction)",
    )?;
    ensure(b.amount >= Decimal::ZERO, "montant négatif interdit")?;
    ensure(
        !b.code.trim().is_empty() && !b.label.trim().is_empty(),
        "code et libellé requis",
    )?;
    if let Some(to) = b.active_to {
        ensure(to >= b.active_from, "la fin précède le début")?;
    }
    ensure_employee_in_tenant(&s, t, id).await?;
    let mut tx = s.db.begin().await?;
    let row: PayItemDto = sqlx::query_as(
        "INSERT INTO employee_pay_items (tenant_id, employee_id, kind, code, label, amount, active_from, active_to, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id, kind, code, label, amount, active_from, active_to",
    )
    .bind(t.tenant_id).bind(id).bind(&b.kind).bind(b.code.trim().to_uppercase()).bind(b.label.trim()).bind(b.amount).bind(b.active_from).bind(b.active_to).bind(auth.user_id)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "pay_item.add",
        "employee",
        Some(id.to_string()),
        json!({"kind": b.kind, "code": row.code}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_neutralizes_formulas_and_quotes() {
        assert_eq!(csv_cell("=1+1"), "'=1+1");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("normal"), "normal");
    }

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("50%_"), "%50\\%\\_%");
    }
}
