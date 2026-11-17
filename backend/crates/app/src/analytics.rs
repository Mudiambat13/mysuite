//! Rapports filtrables : mouvements de personnel, absences, masse salariale. Calculés côté serveur, exportables en CSV.

use crate::{
    audit,
    auth::{Auth, TenantCtx},
    employees::csv_cell,
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::header,
    response::IntoResponse,
    routing::get,
};
use chrono::{Datelike, NaiveDate, Utc};
use mysuite_shared::permissions::Permission;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/reports/movements", get(movements))
        .route("/reports/movements.csv", get(movements_csv))
        .route("/reports/absences", get(absences))
        .route("/reports/absences.csv", get(absences_csv))
        .route("/reports/payroll-cost", get(payroll_cost))
        .route("/reports/payroll-cost.csv", get(payroll_cost_csv))
}

#[derive(Deserialize)]
struct PeriodQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    department_id: Option<Uuid>,
    site_id: Option<Uuid>,
}

/// Période par défaut : 1er janvier de l'année en cours → aujourd'hui. Plage limitée à 5 ans.
fn range(q: &PeriodQuery) -> AppResult<(NaiveDate, NaiveDate)> {
    let today = Utc::now().date_naive();
    let from = q
        .from
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("date valide"));
    let to = q.to.unwrap_or(today);
    ensure(to >= from, "la fin de période précède le début")?;
    ensure((to - from).num_days() <= 366 * 5, "période limitée à 5 ans")?;
    Ok((from, to))
}

pub(crate) fn csv_response(filename: &str, body: String) -> impl IntoResponse + use<> {
    // BOM UTF-8 : Excel affiche correctement les accents.
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        format!("\u{feff}{body}"),
    )
}

pub(crate) fn csv_line(cells: &[String]) -> String {
    let mut l = cells
        .iter()
        .map(|c| csv_cell(c))
        .collect::<Vec<_>>()
        .join(",");
    l.push('\n');
    l
}

// ---------- Mouvements ----------

#[derive(Serialize, sqlx::FromRow)]
struct MovementRow {
    employee_id: Uuid,
    matricule: String,
    name: String,
    date: NaiveDate,
    department_name: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct MonthRow {
    month: String,
    entries: i64,
    exits: i64,
}

#[derive(Serialize)]
struct Movements {
    from: NaiveDate,
    to: NaiveDate,
    entries: Vec<MovementRow>,
    exits: Vec<MovementRow>,
    by_month: Vec<MonthRow>,
}

const FILTERS: &str =
    "($4::uuid IS NULL OR e.department_id = $4) AND ($5::uuid IS NULL OR e.site_id = $5)";

async fn load_movements(s: &AppState, t: &TenantCtx, q: &PeriodQuery) -> AppResult<Movements> {
    let (from, to) = range(q)?;
    let list = |col: &str| {
        format!(
            "SELECT e.id AS employee_id, e.matricule, e.first_name || ' ' || e.last_name AS name, e.{col} AS date, d.name AS department_name
             FROM employees e LEFT JOIN departments d ON d.id = e.department_id AND d.tenant_id = e.tenant_id
             WHERE e.tenant_id = $1 AND e.{col} BETWEEN $2 AND $3 AND {FILTERS} ORDER BY e.{col} DESC, e.last_name LIMIT 1000"
        )
    };
    let entries = sqlx::query_as(&list("hire_date"))
        .bind(t.tenant_id)
        .bind(from)
        .bind(to)
        .bind(q.department_id)
        .bind(q.site_id)
        .fetch_all(&s.db)
        .await?;
    let exits = sqlx::query_as(&list("exit_date"))
        .bind(t.tenant_id)
        .bind(from)
        .bind(to)
        .bind(q.department_id)
        .bind(q.site_id)
        .fetch_all(&s.db)
        .await?;
    let by_month = sqlx::query_as(&format!(
        "SELECT to_char(g.m, 'YYYY-MM') AS month,
           (SELECT count(*) FROM employees e WHERE e.tenant_id = $1 AND e.hire_date >= g.m::date AND e.hire_date < (g.m + interval '1 month')::date AND {FILTERS}) AS entries,
           (SELECT count(*) FROM employees e WHERE e.tenant_id = $1 AND e.exit_date >= g.m::date AND e.exit_date < (g.m + interval '1 month')::date AND {FILTERS}) AS exits
         FROM generate_series(date_trunc('month', $2::date)::timestamp, date_trunc('month', $3::date)::timestamp, interval '1 month') AS g(m)
         ORDER BY g.m"
    ))
    .bind(t.tenant_id)
    .bind(from)
    .bind(to)
    .bind(q.department_id)
    .bind(q.site_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Movements {
        from,
        to,
        entries,
        exits,
        by_month,
    })
}

async fn movements(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<PeriodQuery>,
) -> AppResult<Json<Movements>> {
    let t = auth.require(Permission::ReportsRead)?;
    Ok(Json(load_movements(&s, t, &q).await?))
}

async fn movements_csv(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<PeriodQuery>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::ReportsRead)?;
    let m = load_movements(&s, t, &q).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "report.export",
        "movements",
        None,
        json!({"from": m.from, "to": m.to}),
    )
    .await?;
    let mut out = csv_line(&[
        "mouvement".into(),
        "matricule".into(),
        "nom".into(),
        "date".into(),
        "departement".into(),
    ]);
    for (kind, rows) in [("entree", &m.entries), ("sortie", &m.exits)] {
        for r in rows {
            out.push_str(&csv_line(&[
                kind.into(),
                r.matricule.clone(),
                r.name.clone(),
                r.date.to_string(),
                r.department_name.clone().unwrap_or_default(),
            ]));
        }
    }
    Ok(csv_response("mouvements.csv", out))
}

// ---------- Absences ----------

#[derive(Serialize, sqlx::FromRow)]
struct AbsenceRow {
    label: String,
    days: Decimal,
    requests: i64,
}

#[derive(Serialize)]
struct Absences {
    from: NaiveDate,
    to: NaiveDate,
    total_days: Decimal,
    by_type: Vec<AbsenceRow>,
    by_department: Vec<AbsenceRow>,
}

async fn load_absences(s: &AppState, t: &TenantCtx, q: &PeriodQuery) -> AppResult<Absences> {
    let (from, to) = range(q)?;
    // Absences approuvées dont le début tombe dans la période (un congé à cheval est compté dans la période de son début).
    let sql = |label: &str, join: &str| {
        format!(
            "SELECT {label} AS label, sum(r.days) AS days, count(*) AS requests
             FROM leave_requests r
             JOIN employees e ON e.id = r.employee_id AND e.tenant_id = r.tenant_id
             JOIN leave_types lt ON lt.id = r.leave_type_id AND lt.tenant_id = r.tenant_id
             {join}
             WHERE r.tenant_id = $1 AND r.status = 'approved' AND r.start_date BETWEEN $2 AND $3 AND {FILTERS}
             GROUP BY 1 ORDER BY 2 DESC"
        )
    };
    let by_type: Vec<AbsenceRow> = sqlx::query_as(&sql("lt.name", ""))
        .bind(t.tenant_id)
        .bind(from)
        .bind(to)
        .bind(q.department_id)
        .bind(q.site_id)
        .fetch_all(&s.db)
        .await?;
    let by_department = sqlx::query_as(&sql(
        "COALESCE(d.name, 'Non affecté')",
        "LEFT JOIN departments d ON d.id = e.department_id AND d.tenant_id = e.tenant_id",
    ))
    .bind(t.tenant_id)
    .bind(from)
    .bind(to)
    .bind(q.department_id)
    .bind(q.site_id)
    .fetch_all(&s.db)
    .await?;
    let total_days = by_type.iter().map(|r| r.days).sum();
    Ok(Absences {
        from,
        to,
        total_days,
        by_type,
        by_department,
    })
}

async fn absences(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<PeriodQuery>,
) -> AppResult<Json<Absences>> {
    let t = auth.require(Permission::ReportsRead)?;
    Ok(Json(load_absences(&s, t, &q).await?))
}

async fn absences_csv(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<PeriodQuery>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::ReportsRead)?;
    let a = load_absences(&s, t, &q).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "report.export",
        "absences",
        None,
        json!({"from": a.from, "to": a.to}),
    )
    .await?;
    let mut out = csv_line(&[
        "regroupement".into(),
        "libelle".into(),
        "jours".into(),
        "demandes".into(),
    ]);
    for (kind, rows) in [("type", &a.by_type), ("departement", &a.by_department)] {
        for r in rows {
            out.push_str(&csv_line(&[
                kind.into(),
                r.label.clone(),
                r.days.to_string(),
                r.requests.to_string(),
            ]));
        }
    }
    Ok(csv_response("absences.csv", out))
}

// ---------- Masse salariale ----------

#[derive(Deserialize)]
struct YearQuery {
    year: Option<i32>,
}

#[derive(Serialize, sqlx::FromRow)]
struct CostMonth {
    month: i32,
    gross: Decimal,
    net: Decimal,
    employer_cost: Decimal,
    employees: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct CostDept {
    label: String,
    gross: Decimal,
    employer_cost: Decimal,
    employees: i64,
}

#[derive(Serialize)]
struct PayrollCost {
    year: i32,
    currency: String,
    months: Vec<CostMonth>,
    by_department: Vec<CostDept>,
}

async fn load_cost(s: &AppState, t: &TenantCtx, year: i32) -> AppResult<PayrollCost> {
    // Uniquement les paies clôturées ET non simulées : une simulation n'est jamais une masse salariale.
    let months = sqlx::query_as(
        "SELECT p.month, sum(i.gross) AS gross, sum(i.net) AS net, sum(i.employer_cost) AS employer_cost, count(*) AS employees
         FROM payroll_items i
         JOIN payroll_runs r ON r.id = i.run_id AND r.tenant_id = i.tenant_id
         JOIN payroll_periods p ON p.id = r.period_id AND p.tenant_id = r.tenant_id
         WHERE i.tenant_id = $1 AND p.year = $2 AND r.status = 'closed' AND r.simulation = FALSE
         GROUP BY p.month ORDER BY p.month",
    )
    .bind(t.tenant_id)
    .bind(year)
    .fetch_all(&s.db)
    .await?;
    // Ventilation selon le département ACTUEL de l'employé (l'historique d'affectation n'est pas encore géré).
    let by_department = sqlx::query_as(
        "SELECT COALESCE(d.name, 'Non affecté') AS label, sum(i.gross) AS gross, sum(i.employer_cost) AS employer_cost,
                count(DISTINCT i.employee_id) AS employees
         FROM payroll_items i
         JOIN payroll_runs r ON r.id = i.run_id AND r.tenant_id = i.tenant_id
         JOIN payroll_periods p ON p.id = r.period_id AND p.tenant_id = r.tenant_id
         JOIN employees e ON e.id = i.employee_id AND e.tenant_id = i.tenant_id
         LEFT JOIN departments d ON d.id = e.department_id AND d.tenant_id = e.tenant_id
         WHERE i.tenant_id = $1 AND p.year = $2 AND r.status = 'closed' AND r.simulation = FALSE
         GROUP BY 1 ORDER BY 3 DESC",
    )
    .bind(t.tenant_id)
    .bind(year)
    .fetch_all(&s.db)
    .await?;
    let (currency,): (String,) = sqlx::query_as("SELECT currency FROM tenants WHERE id = $1")
        .bind(t.tenant_id)
        .fetch_one(&s.db)
        .await?;
    Ok(PayrollCost {
        year,
        currency,
        months,
        by_department,
    })
}

fn cost_year(q: &YearQuery) -> AppResult<i32> {
    let y = q.year.unwrap_or_else(|| Utc::now().year());
    if (2000..=2100).contains(&y) {
        Ok(y)
    } else {
        Err(AppError::Validation("année invalide".into()))
    }
}

async fn payroll_cost(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<YearQuery>,
) -> AppResult<Json<PayrollCost>> {
    let t = auth.require(Permission::PayrollRead)?;
    let year = cost_year(&q)?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "report.payroll_cost",
        "payroll",
        None,
        json!({"year": year}),
    )
    .await?;
    Ok(Json(load_cost(&s, t, year).await?))
}

async fn payroll_cost_csv(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<YearQuery>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::PayrollRead)?;
    let year = cost_year(&q)?;
    let c = load_cost(&s, t, year).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "report.export",
        "payroll_cost",
        None,
        json!({"year": year}),
    )
    .await?;
    let mut out = csv_line(&[
        "regroupement".into(),
        "libelle".into(),
        "brut".into(),
        "net".into(),
        "cout_employeur".into(),
        "employes".into(),
        "devise".into(),
    ]);
    for m in &c.months {
        out.push_str(&csv_line(&[
            "mois".into(),
            format!("{}-{:02}", c.year, m.month),
            m.gross.to_string(),
            m.net.to_string(),
            m.employer_cost.to_string(),
            m.employees.to_string(),
            c.currency.clone(),
        ]));
    }
    for d in &c.by_department {
        out.push_str(&csv_line(&[
            "departement".into(),
            d.label.clone(),
            d.gross.to_string(),
            String::new(),
            d.employer_cost.to_string(),
            d.employees.to_string(),
            c.currency.clone(),
        ]));
    }
    Ok(csv_response("masse-salariale.csv", out))
}
