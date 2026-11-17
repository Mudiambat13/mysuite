//! Grille de salaires minimums (ex. SMIG) fournie par la société, classe par employé, contrôle de conformité.
//!
//! MySuite ne fournit aucune grille : elle est importée avec sa source. Les contrôles sont **informatifs**
//! (aucun blocage automatique) car la convention de calcul et la date d'effet relèvent de la société.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use chrono::{NaiveDate, Utc};
use mysuite_shared::permissions::Permission;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    analytics::{csv_line, csv_response},
    audit,
    auth::Auth,
    error::{AppError, AppResult, ensure},
    state::AppState,
    timeclock::parse_csv,
};

const MAX_ROWS: usize = 200;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/wage-grids", get(list_grids).post(import_grid))
        .route("/wage-grids/{id}", get(grid_rows))
        .route(
            "/employees/{id}/wage-class",
            get(list_classes).post(set_class),
        )
        .route("/reports/smig-compliance", get(compliance))
        .route("/reports/smig-compliance.csv", get(compliance_csv))
}

/// Minimum mensuel applicable à un employé à une date : classe en vigueur → grille en vigueur (même devise que la
/// société) → montant mensuel de la classe. `None` si l'un des éléments manque.
pub async fn minimum_monthly<'e, E>(
    exec: E,
    tenant_id: Uuid,
    employee_id: Uuid,
    at: NaiveDate,
) -> AppResult<Option<Decimal>>
where
    E: sqlx::PgExecutor<'e>,
{
    let row: Option<(Decimal,)> = sqlx::query_as(
        "WITH cls AS (
            SELECT class_no FROM employee_wage_class
            WHERE employee_id = $1 AND tenant_id = $2 AND effective_from <= $3
            ORDER BY effective_from DESC LIMIT 1
         ), grid AS (
            SELECT g.id FROM wage_grids g JOIN tenants t ON t.id = g.tenant_id
            WHERE g.tenant_id = $2 AND g.effective_from <= $3 AND g.currency = t.currency
            ORDER BY g.effective_from DESC LIMIT 1
         )
         SELECT r.monthly FROM wage_grid_rows r, cls, grid
         WHERE r.tenant_id = $2 AND r.grid_id = grid.id AND r.class_no = cls.class_no",
    )
    .bind(employee_id)
    .bind(tenant_id)
    .bind(at)
    .fetch_optional(exec)
    .await?;
    Ok(row.map(|(m,)| m))
}

#[derive(Serialize, sqlx::FromRow)]
struct GridDto {
    id: Uuid,
    name: String,
    currency: String,
    effective_from: NaiveDate,
    source: String,
    row_count: i64,
}

async fn list_grids(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<GridDto>>> {
    let t = auth.require(Permission::CompensationRead)?;
    let rows = sqlx::query_as(
        "SELECT g.id, g.name, g.currency, g.effective_from, g.source,
                (SELECT count(*) FROM wage_grid_rows r WHERE r.grid_id = g.id AND r.tenant_id = g.tenant_id) AS row_count
         FROM wage_grids g WHERE g.tenant_id = $1 ORDER BY g.effective_from DESC, g.created_at DESC",
    )
    .bind(t.tenant_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow, Clone)]
struct RowDto {
    class_no: i32,
    category: String,
    reference: String,
    coefficient: Option<Decimal>,
    daily: Decimal,
    monthly: Decimal,
}

async fn grid_rows(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<RowDto>>> {
    let t = auth.require(Permission::CompensationRead)?;
    let found: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM wage_grids WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(t.tenant_id)
            .fetch_optional(&s.db)
            .await?;
    found.ok_or(AppError::NotFound)?;
    let rows = sqlx::query_as(
        "SELECT class_no, category, reference, coefficient, daily, monthly FROM wage_grid_rows WHERE grid_id = $1 AND tenant_id = $2 ORDER BY class_no",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct ImportBody {
    name: String,
    currency: String,
    effective_from: NaiveDate,
    source: String,
    csv: String,
    #[serde(default)]
    dry_run: bool,
}

fn norm(h: &str) -> String {
    h.trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .replace(['é', 'è'], "e")
}

fn number(raw: &str) -> Option<Decimal> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
        .collect::<String>()
        .replace(',', ".");
    cleaned.parse::<Decimal>().ok()
}

fn parse_grid(csv: &str) -> Result<Vec<(i32, RowDto)>, Vec<String>> {
    let rows = parse_csv(csv);
    let Some((head, body)) = rows.split_first() else {
        return Err(vec!["fichier vide".into()]);
    };
    let idx: HashMap<String, usize> = head.iter().enumerate().map(|(i, h)| (norm(h), i)).collect();
    let col = |names: &[&str]| names.iter().find_map(|n| idx.get(*n).copied());
    let (Some(c_class), Some(c_cat), Some(c_daily), Some(c_monthly)) = (
        col(&["classe"]),
        col(&["categorie"]),
        col(&["journalier"]),
        col(&["mensuel"]),
    ) else {
        return Err(vec![
            "colonnes obligatoires : Classe, Catégorie, Journalier, Mensuel (le mensuel n'est jamais déduit)".into(),
        ]);
    };
    let c_ref = col(&["repere paie", "reference"]);
    let c_coef = col(&["tension", "coefficient"]);
    let (mut out, mut errors) = (Vec::new(), Vec::new());
    if body.len() > MAX_ROWS {
        return Err(vec![format!("{MAX_ROWS} lignes maximum")]);
    }
    for (n, r) in body.iter().enumerate() {
        let line = n + 2;
        let get = |i: usize| r.get(i).map(|c| c.trim().to_string()).unwrap_or_default();
        let class = get(c_class).parse::<i32>().ok().filter(|c| *c > 0);
        let daily = number(&get(c_daily)).filter(|d| *d > Decimal::ZERO);
        let monthly = number(&get(c_monthly)).filter(|d| *d > Decimal::ZERO);
        let category = get(c_cat);
        let (Some(class), Some(daily), Some(monthly)) = (class, daily, monthly) else {
            errors.push(format!(
                "ligne {line} : classe, journalier et mensuel doivent être des nombres positifs"
            ));
            continue;
        };
        if category.is_empty() {
            errors.push(format!("ligne {line} : catégorie manquante"));
            continue;
        }
        out.push((
            class,
            RowDto {
                class_no: class,
                category,
                reference: c_ref.map(get).unwrap_or_default(),
                coefficient: c_coef.and_then(|i| number(&get(i))),
                daily,
                monthly,
            },
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for (c, _) in &out {
        if !seen.insert(*c) {
            errors.push(format!("classe {c} en double"));
        }
    }
    if out.is_empty() && errors.is_empty() {
        errors.push("aucune ligne".into());
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

async fn import_grid(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<ImportBody>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::PayrollRulesManage)?;
    ensure(!b.name.trim().is_empty(), "nom requis")?;
    ensure(
        !b.source.trim().is_empty(),
        "source requise (texte officiel ou document d'origine)",
    )?;
    ensure(
        b.currency.len() == 3 && b.currency.chars().all(|c| c.is_ascii_uppercase()),
        "devise sur 3 lettres majuscules (ex. CDF)",
    )?;
    let rows = parse_grid(&b.csv)
        .map_err(|e| AppError::Validation(format!("import refusé : {}", e.join(" ; "))))?;
    let preview: Vec<RowDto> = rows.iter().map(|(_, r)| r.clone()).collect();
    if b.dry_run {
        return Ok((
            StatusCode::OK,
            Json(json!({"dry_run": true, "rows": preview.len(), "preview": preview})),
        ));
    }
    let mut tx = s.db.begin().await?;
    let (gid,): (Uuid,) = sqlx::query_as(
        "INSERT INTO wage_grids (tenant_id, name, currency, effective_from, source, created_by)
         VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(t.tenant_id)
    .bind(b.name.trim())
    .bind(&b.currency)
    .bind(b.effective_from)
    .bind(b.source.trim())
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;
    for (_, r) in &rows {
        sqlx::query(
            "INSERT INTO wage_grid_rows (tenant_id, grid_id, class_no, category, reference, coefficient, daily, monthly)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(t.tenant_id)
        .bind(gid)
        .bind(r.class_no)
        .bind(&r.category)
        .bind(&r.reference)
        .bind(r.coefficient)
        .bind(r.daily)
        .bind(r.monthly)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "wage_grid.import",
        "wage_grid",
        Some(gid.to_string()),
        json!({"name": b.name, "rows": rows.len(), "effective_from": b.effective_from, "source": b.source}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": gid, "rows": rows.len()})),
    ))
}

#[derive(Serialize, sqlx::FromRow)]
struct ClassDto {
    id: Uuid,
    class_no: i32,
    effective_from: NaiveDate,
}

#[derive(Deserialize)]
struct ClassBody {
    class_no: i32,
    effective_from: NaiveDate,
}

async fn employee_in_tenant(s: &AppState, tenant: Uuid, id: Uuid) -> AppResult<()> {
    let f: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM employees WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant)
            .fetch_optional(&s.db)
            .await?;
    f.map(|_| ()).ok_or(AppError::NotFound)
}

async fn list_classes(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<ClassDto>>> {
    let t = auth.require(Permission::CompensationRead)?;
    employee_in_tenant(&s, t.tenant_id, id).await?;
    let rows = sqlx::query_as("SELECT id, class_no, effective_from FROM employee_wage_class WHERE employee_id = $1 AND tenant_id = $2 ORDER BY effective_from DESC")
        .bind(id).bind(t.tenant_id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

async fn set_class(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<ClassBody>,
) -> AppResult<(StatusCode, Json<ClassDto>)> {
    let t = auth.require(Permission::CompensationManage)?;
    ensure((1..=1000).contains(&b.class_no), "classe entre 1 et 1000")?;
    employee_in_tenant(&s, t.tenant_id, id).await?;
    let mut tx = s.db.begin().await?;
    let row: ClassDto = sqlx::query_as(
        "INSERT INTO employee_wage_class (tenant_id, employee_id, class_no, effective_from, created_by)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (employee_id, effective_from) DO UPDATE SET class_no = EXCLUDED.class_no, created_by = EXCLUDED.created_by
         RETURNING id, class_no, effective_from",
    )
    .bind(t.tenant_id).bind(id).bind(b.class_no).bind(b.effective_from).bind(auth.user_id)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "employee.wage_class",
        "employee",
        Some(id.to_string()),
        json!({"class_no": b.class_no, "effective_from": b.effective_from}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct AsOf {
    as_of: Option<NaiveDate>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ComplianceRow {
    employee_id: Uuid,
    matricule: String,
    full_name: String,
    class_no: Option<i32>,
    base_salary: Option<Decimal>,
    salary_currency: Option<String>,
    grid_currency: Option<String>,
    minimum_monthly: Option<Decimal>,
}

#[derive(Serialize)]
struct ComplianceOut {
    #[serde(flatten)]
    row: ComplianceRow,
    status: &'static str,
    gap: Option<Decimal>,
}

fn classify(r: &ComplianceRow) -> (&'static str, Option<Decimal>) {
    let Some(_) = r.class_no else {
        return ("no_class", None);
    };
    let Some(salary) = r.base_salary else {
        return ("no_salary", None);
    };
    let Some(min) = r.minimum_monthly else {
        return ("no_grid", None);
    };
    if r.salary_currency != r.grid_currency {
        return ("currency_mismatch", None);
    }
    if salary < min {
        ("below", Some(min - salary))
    } else {
        ("ok", None)
    }
}

async fn compliance_rows(
    s: &AppState,
    tenant: Uuid,
    at: NaiveDate,
) -> AppResult<Vec<ComplianceOut>> {
    let rows: Vec<ComplianceRow> = sqlx::query_as(
        "SELECT e.id AS employee_id, e.matricule, e.first_name || ' ' || e.last_name AS full_name,
                c.class_no, comp.base_salary, comp.currency AS salary_currency,
                gr.currency AS grid_currency, gr.monthly AS minimum_monthly
         FROM employees e
         LEFT JOIN LATERAL (SELECT class_no FROM employee_wage_class w WHERE w.employee_id = e.id AND w.tenant_id = e.tenant_id AND w.effective_from <= $2 ORDER BY w.effective_from DESC LIMIT 1) c ON true
         LEFT JOIN LATERAL (SELECT base_salary, currency FROM employee_compensation k WHERE k.employee_id = e.id AND k.tenant_id = e.tenant_id AND k.effective_from <= $2 ORDER BY k.effective_from DESC LIMIT 1) comp ON true
         LEFT JOIN LATERAL (SELECT g.id, g.currency FROM wage_grids g WHERE g.tenant_id = e.tenant_id AND g.effective_from <= $2 ORDER BY g.effective_from DESC LIMIT 1) g ON true
         LEFT JOIN LATERAL (SELECT r.monthly, g.currency FROM wage_grid_rows r WHERE r.grid_id = g.id AND r.tenant_id = e.tenant_id AND r.class_no = c.class_no) gr ON true
         WHERE e.tenant_id = $1 AND e.status IN ('active','on_leave') AND e.hire_date <= $2 AND (e.exit_date IS NULL OR e.exit_date >= $2)
         ORDER BY e.last_name, e.first_name",
    )
    .bind(tenant)
    .bind(at)
    .fetch_all(&s.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (status, gap) = classify(&row);
            ComplianceOut { row, status, gap }
        })
        .collect())
}

async fn compliance(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<AsOf>,
) -> AppResult<Json<serde_json::Value>> {
    let t = auth.require(Permission::CompensationRead)?;
    let at = q.as_of.unwrap_or_else(|| Utc::now().date_naive());
    let rows = compliance_rows(&s, t.tenant_id, at).await?;
    let below = rows.iter().filter(|r| r.status == "below").count();
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "wage_grid.compliance_read",
        "report",
        None,
        json!({"as_of": at, "below": below}),
    )
    .await?;
    Ok(Json(
        json!({"as_of": at, "below_minimum": below, "rows": rows}),
    ))
}

async fn compliance_csv(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<AsOf>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::CompensationRead)?;
    let at = q.as_of.unwrap_or_else(|| Utc::now().date_naive());
    let rows = compliance_rows(&s, t.tenant_id, at).await?;
    let mut out = csv_line(
        &[
            "Matricule",
            "Nom",
            "Classe",
            "Salaire de base",
            "Devise",
            "Minimum mensuel",
            "Écart",
            "Statut",
        ]
        .map(String::from),
    );
    for r in &rows {
        out.push_str(&csv_line(&[
            r.row.matricule.clone(),
            r.row.full_name.clone(),
            r.row.class_no.map(|c| c.to_string()).unwrap_or_default(),
            r.row.base_salary.map(|d| d.to_string()).unwrap_or_default(),
            r.row.salary_currency.clone().unwrap_or_default(),
            r.row
                .minimum_monthly
                .map(|d| d.to_string())
                .unwrap_or_default(),
            r.gap.map(|d| d.to_string()).unwrap_or_default(),
            r.status.to_string(),
        ]));
    }
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "wage_grid.compliance_export",
        "report",
        None,
        json!({"as_of": at}),
    )
    .await?;
    Ok(csv_response(&format!("conformite-smig-{at}.csv"), out))
}
