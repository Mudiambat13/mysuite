//! Paie : jeux de règles versionnés, périodes, calcul, validation, clôture et bulletins.
//!
//! Le calcul lui-même est dans la crate pure `mysuite-payroll`. Ici : données d'entrée, droits, cycle de vie, audit.

use crate::pdf::{Document, Font, format_amount};
use crate::{
    audit,
    auth::{Auth, TenantCtx},
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{NaiveDate, Utc};
use mysuite_payroll::{
    Allowance, Deduction, LineKind, PayrollInput, PayrollResult, RuleSet, compute,
};
use mysuite_shared::permissions::{Permission, has_permission};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/payroll/rule-sets",
            get(list_rule_sets).post(create_rule_set),
        )
        .route("/payroll/rule-sets/{id}/approve", post(approve_rule_set))
        .route("/payroll/periods", get(list_periods).post(create_period))
        .route("/payroll/runs", get(list_runs).post(create_run))
        .route("/payroll/runs/{id}", get(run_detail))
        .route("/payroll/runs/{id}/recalculate", post(recalculate))
        .route("/payroll/runs/{id}/validate", post(validate_run))
        .route("/payroll/runs/{id}/close", post(close_run))
        .route("/payslips/me", get(my_payslips))
        .route("/payslips/{item_id}", get(payslip_detail))
        .route("/payslips/{item_id}/pdf", get(payslip_pdf))
}

// ---------- Jeux de règles ----------

#[derive(Serialize, sqlx::FromRow)]
struct RuleSetDto {
    id: Uuid,
    country: String,
    name: String,
    version: i32,
    effective_from: NaiveDate,
    status: String,
    is_demo: bool,
    source: Option<String>,
    verified_on: Option<NaiveDate>,
    created_by: Option<Uuid>,
    params: Value,
}

async fn list_rule_sets(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<RuleSetDto>>> {
    let t = auth.require(Permission::PayrollRead)?;
    Ok(Json(
        sqlx::query_as("SELECT id, country, name, version, effective_from, status, is_demo, source, verified_on, created_by, params FROM payroll_rule_sets WHERE tenant_id = $1 ORDER BY country, name, version DESC")
            .bind(t.tenant_id).fetch_all(&s.db).await?,
    ))
}

#[derive(Deserialize)]
struct NewRuleSet {
    country: String,
    name: String,
    effective_from: NaiveDate,
    params: Value,
    source: Option<String>,
}

async fn create_rule_set(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewRuleSet>,
) -> AppResult<(StatusCode, Json<RuleSetDto>)> {
    let t = auth.require(Permission::PayrollRulesManage)?;
    ensure(
        !b.name.trim().is_empty() && b.country.trim().len() == 2,
        "nom et code pays (2 lettres) requis",
    )?;
    // Valide la structure ET les valeurs (taux, barème) par un calcul à blanc.
    let rules: RuleSet = serde_json::from_value(b.params.clone())
        .map_err(|e| AppError::Validation(format!("paramètres invalides : {e}")))?;
    let probe = PayrollInput {
        base_salary: Decimal::from(1000),
        allowances: vec![],
        deductions: vec![],
        dependents: 0,
        minimum_wage: None,
    };
    compute(&probe, &rules).map_err(|e| AppError::Validation(e.to_string()))?;
    let normalized = serde_json::to_value(&rules).map_err(|e| AppError::Internal(e.into()))?;

    let mut tx = s.db.begin().await?;
    let (next,): (i32,) = sqlx::query_as("SELECT COALESCE(max(version), 0) + 1 FROM payroll_rule_sets WHERE tenant_id = $1 AND country = $2 AND name = $3")
        .bind(t.tenant_id).bind(b.country.trim().to_uppercase()).bind(b.name.trim()).fetch_one(&mut *tx).await?;
    // Toujours créé en brouillon : un jeu de règles n'est utilisable pour une paie réelle qu'après approbation.
    let row: RuleSetDto = sqlx::query_as(
        "INSERT INTO payroll_rule_sets (tenant_id, country, name, version, effective_from, params, source, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id, country, name, version, effective_from, status, is_demo, source, verified_on, created_by, params",
    )
    .bind(t.tenant_id).bind(b.country.trim().to_uppercase()).bind(b.name.trim()).bind(next).bind(b.effective_from).bind(normalized).bind(&b.source).bind(auth.user_id)
    .fetch_one(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_rules.create",
        "rule_set",
        Some(row.id.to_string()),
        json!({"version": next}),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct ApproveBody {
    source: String,
    verified_on: NaiveDate,
}

async fn approve_rule_set(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<ApproveBody>,
) -> AppResult<Json<RuleSetDto>> {
    let t = auth.require(Permission::PayrollRulesManage)?;
    ensure(
        !b.source.trim().is_empty(),
        "référence aux textes officiels requise",
    )?;
    ensure(
        b.verified_on <= Utc::now().date_naive(),
        "date de vérification dans le futur",
    )?;
    let mut tx = s.db.begin().await?;
    let cur: Option<(String, bool, Option<Uuid>)> = sqlx::query_as("SELECT status, is_demo, created_by FROM payroll_rule_sets WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(id).bind(t.tenant_id).fetch_optional(&mut *tx).await?;
    let (status, is_demo, created_by) = cur.ok_or(AppError::NotFound)?;
    if is_demo {
        return Err(AppError::Conflict(
            "un jeu de règles de démonstration (valeurs fictives) ne peut pas être approuvé".into(),
        ));
    }
    if status != "draft" {
        return Err(AppError::Conflict(
            "seul un brouillon peut être approuvé".into(),
        ));
    }
    if s.config.payroll_four_eyes && created_by == Some(auth.user_id) {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE payroll_rule_sets SET status='approved', approved_by=$3, approved_at=now(), source=$4, verified_on=$5 WHERE id=$1 AND tenant_id=$2")
        .bind(id).bind(t.tenant_id).bind(auth.user_id).bind(b.source.trim()).bind(b.verified_on).execute(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_rules.approve",
        "rule_set",
        Some(id.to_string()),
        json!({"verified_on": b.verified_on, "source": b.source}),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query_as("SELECT id, country, name, version, effective_from, status, is_demo, source, verified_on, created_by, params FROM payroll_rule_sets WHERE id = $1").bind(id).fetch_one(&s.db).await?;
    Ok(Json(row))
}

// ---------- Périodes ----------

#[derive(Serialize, sqlx::FromRow)]
struct PeriodDto {
    id: Uuid,
    year: i32,
    month: i32,
    status: String,
}

async fn list_periods(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<PeriodDto>>> {
    let t = auth.require(Permission::PayrollRead)?;
    Ok(Json(sqlx::query_as("SELECT id, year, month, status FROM payroll_periods WHERE tenant_id = $1 ORDER BY year DESC, month DESC").bind(t.tenant_id).fetch_all(&s.db).await?))
}

#[derive(Deserialize)]
struct NewPeriod {
    year: i32,
    month: i32,
}

async fn create_period(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewPeriod>,
) -> AppResult<(StatusCode, Json<PeriodDto>)> {
    let t = auth.require(Permission::PayrollManage)?;
    ensure(
        (1..=12).contains(&b.month) && (2000..=2100).contains(&b.year),
        "période invalide",
    )?;
    let row: PeriodDto = sqlx::query_as("INSERT INTO payroll_periods (tenant_id, year, month) VALUES ($1,$2,$3) RETURNING id, year, month, status")
        .bind(t.tenant_id).bind(b.year).bind(b.month).fetch_one(&s.db).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_period.create",
        "period",
        Some(row.id.to_string()),
        json!({"year": b.year, "month": b.month}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

// ---------- Exécutions ----------

#[derive(Serialize, sqlx::FromRow)]
struct RunDto {
    id: Uuid,
    period_id: Uuid,
    year: i32,
    month: i32,
    rule_set_id: Uuid,
    rule_version: i32,
    rule_set_name: String,
    simulation: bool,
    status: String,
    total_gross: Decimal,
    total_net: Decimal,
    total_employer_cost: Decimal,
    employee_count: i64,
}

const SELECT_RUN: &str = "
    SELECT r.id, r.period_id, p.year, p.month, r.rule_set_id, rs.version AS rule_version, rs.name AS rule_set_name, r.simulation, r.status,
           COALESCE((SELECT sum(gross) FROM payroll_items i WHERE i.run_id = r.id), 0) AS total_gross,
           COALESCE((SELECT sum(net) FROM payroll_items i WHERE i.run_id = r.id), 0) AS total_net,
           COALESCE((SELECT sum(employer_cost) FROM payroll_items i WHERE i.run_id = r.id), 0) AS total_employer_cost,
           (SELECT count(*) FROM payroll_items i WHERE i.run_id = r.id) AS employee_count
    FROM payroll_runs r
    JOIN payroll_periods p ON p.id = r.period_id AND p.tenant_id = r.tenant_id
    JOIN payroll_rule_sets rs ON rs.id = r.rule_set_id AND rs.tenant_id = r.tenant_id";

async fn list_runs(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<RunDto>>> {
    let t = auth.require(Permission::PayrollRead)?;
    Ok(Json(sqlx::query_as(&format!("{SELECT_RUN} WHERE r.tenant_id = $1 ORDER BY p.year DESC, p.month DESC, r.created_at DESC")).bind(t.tenant_id).fetch_all(&s.db).await?))
}

#[derive(Serialize, sqlx::FromRow)]
struct ItemSummary {
    id: Uuid,
    employee_id: Uuid,
    matricule: String,
    employee_name: String,
    gross: Decimal,
    net: Decimal,
    employer_cost: Decimal,
}

#[derive(Serialize)]
struct RunDetail {
    run: RunDto,
    items: Vec<ItemSummary>,
    warnings: Vec<String>,
}

async fn run_detail(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RunDetail>> {
    let t = auth.require(Permission::PayrollRead)?;
    let run: RunDto = sqlx::query_as(&format!(
        "{SELECT_RUN} WHERE r.id = $1 AND r.tenant_id = $2"
    ))
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let items = sqlx::query_as(
        "SELECT i.id, i.employee_id, e.matricule, e.first_name || ' ' || e.last_name AS employee_name, i.gross, i.net, i.employer_cost
         FROM payroll_items i JOIN employees e ON e.id = i.employee_id AND e.tenant_id = i.tenant_id
         WHERE i.run_id = $1 AND i.tenant_id = $2 ORDER BY e.last_name, e.first_name",
    )
    .bind(id).bind(t.tenant_id).fetch_all(&s.db).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll.read",
        "payroll_run",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    Ok(Json(RunDetail {
        run,
        items,
        warnings: vec![],
    }))
}

#[derive(Deserialize)]
struct NewRun {
    period_id: Uuid,
    rule_set_id: Uuid,
}

fn month_bounds(year: i32, month: i32) -> AppResult<(NaiveDate, NaiveDate)> {
    let start = NaiveDate::from_ymd_opt(year, month as u32, 1)
        .ok_or_else(|| AppError::Validation("période invalide".into()))?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month as u32 + 1, 1)
    };
    Ok((
        start,
        next.expect("date valide").pred_opt().expect("date valide"),
    ))
}

/// Calcule et enregistre les lignes de paie d'une exécution brouillon. Retourne les avertissements (employés ignorés).
async fn compute_items(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    run_id: Uuid,
    year: i32,
    month: i32,
    rules: &RuleSet,
) -> AppResult<Vec<String>> {
    let (start, end) = month_bounds(year, month)?;
    let employees: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, matricule, first_name, last_name FROM employees
         WHERE tenant_id = $1 AND status IN ('active','on_leave') AND hire_date <= $3 AND (exit_date IS NULL OR exit_date >= $2) ORDER BY last_name, first_name",
    )
    .bind(tenant_id).bind(start).bind(end).fetch_all(&mut **tx).await?;

    let mut warnings = Vec::new();
    for (eid, matricule, first, last) in employees {
        let comp: Option<(Decimal,)> = sqlx::query_as("SELECT base_salary FROM employee_compensation WHERE employee_id = $1 AND tenant_id = $2 AND effective_from <= $3 ORDER BY effective_from DESC LIMIT 1")
            .bind(eid).bind(tenant_id).bind(end).fetch_optional(&mut **tx).await?;
        let Some((base,)) = comp else {
            warnings.push(format!(
                "{matricule} {first} {last} : aucune rémunération définie, ignoré"
            ));
            continue;
        };
        let dependents: Option<(i16,)> = sqlx::query_as("SELECT dependents FROM employee_dependents WHERE employee_id = $1 AND tenant_id = $2 AND effective_from <= $3 ORDER BY effective_from DESC LIMIT 1")
            .bind(eid).bind(tenant_id).bind(end).fetch_optional(&mut **tx).await?;
        let minimum_wage = crate::wagegrid::minimum_monthly(&mut **tx, tenant_id, eid, end).await?;
        let items: Vec<(String, String, String, Decimal)> = sqlx::query_as(
            "SELECT kind, code, label, amount FROM employee_pay_items WHERE employee_id = $1 AND tenant_id = $2 AND active_from <= $4 AND (active_to IS NULL OR active_to >= $3) ORDER BY code",
        )
        .bind(eid).bind(tenant_id).bind(start).bind(end).fetch_all(&mut **tx).await?;
        let mut input = PayrollInput {
            base_salary: base,
            allowances: vec![],
            deductions: vec![],
            dependents: dependents.map_or(0, |(n,)| n.max(0) as u32),
            minimum_wage,
        };
        for (kind, code, label, amount) in items {
            if kind == "allowance" {
                input.allowances.push(Allowance {
                    code,
                    label,
                    amount,
                });
            } else {
                input.deductions.push(Deduction {
                    code,
                    label,
                    amount,
                });
            }
        }
        let result = compute(&input, rules)
            .map_err(|e| AppError::Validation(format!("{matricule} : {e}")))?;
        sqlx::query("INSERT INTO payroll_items (tenant_id, run_id, employee_id, gross, net, employer_cost, result, input) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(tenant_id).bind(run_id).bind(eid).bind(result.gross).bind(result.net).bind(result.employer_cost)
            .bind(serde_json::to_value(&result).map_err(|e| AppError::Internal(e.into()))?)
            .bind(serde_json::to_value(&input).map_err(|e| AppError::Internal(e.into()))?)
            .execute(&mut **tx).await?;
    }
    Ok(warnings)
}

async fn load_rules(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    rule_set_id: Uuid,
    period_end: NaiveDate,
) -> AppResult<(RuleSet, bool)> {
    let row: Option<(Value, String, NaiveDate)> = sqlx::query_as("SELECT params, status, effective_from FROM payroll_rule_sets WHERE id = $1 AND tenant_id = $2")
        .bind(rule_set_id).bind(tenant_id).fetch_optional(&mut **tx).await?;
    let (params, status, effective_from) =
        row.ok_or_else(|| AppError::Validation("jeu de règles inconnu".into()))?;
    ensure(status != "retired", "jeu de règles retiré")?;
    ensure(
        effective_from <= period_end,
        "ce jeu de règles n'est pas encore en vigueur pour cette période",
    )?;
    let rules: RuleSet =
        serde_json::from_value(params).map_err(|e| AppError::Internal(e.into()))?;
    Ok((rules, status == "approved"))
}

async fn create_run(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewRun>,
) -> AppResult<(StatusCode, Json<RunDetail>)> {
    let t = auth.require(Permission::PayrollManage)?;
    let mut tx = s.db.begin().await?;
    let period: (i32, i32, String) = sqlx::query_as("SELECT year, month, status FROM payroll_periods WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(b.period_id).bind(t.tenant_id).fetch_optional(&mut *tx).await?.ok_or_else(|| AppError::Validation("période inconnue".into()))?;
    ensure(period.2 == "open", "période clôturée")?;
    let (_, end) = month_bounds(period.0, period.1)?;
    let (rules, approved) = load_rules(&mut tx, t.tenant_id, b.rule_set_id, end).await?;
    // Simulation tant que les règles ne sont pas approuvées par un humain habilité.
    let simulation = !approved;
    let (run_id,): (Uuid,) = sqlx::query_as("INSERT INTO payroll_runs (tenant_id, period_id, rule_set_id, simulation, created_by) VALUES ($1,$2,$3,$4,$5) RETURNING id")
        .bind(t.tenant_id).bind(b.period_id).bind(b.rule_set_id).bind(simulation).bind(auth.user_id).fetch_one(&mut *tx).await
        .map_err(|e| match AppError::from(e) { AppError::Conflict(_) => AppError::Conflict("une paie non clôturée existe déjà pour cette période".into()), other => other })?;
    let warnings = compute_items(&mut tx, t.tenant_id, run_id, period.0, period.1, &rules).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_run.create",
        "payroll_run",
        Some(run_id.to_string()),
        json!({"simulation": simulation, "warnings": warnings.len()}),
    )
    .await?;
    tx.commit().await?;
    let mut detail = run_detail(State(s), auth, Path(run_id)).await?.0;
    detail.warnings = warnings;
    Ok((StatusCode::CREATED, Json(detail)))
}

async fn recalculate(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RunDetail>> {
    let t = auth.require(Permission::PayrollManage)?;
    let mut tx = s.db.begin().await?;
    let run: (Uuid, Uuid, String) = sqlx::query_as("SELECT period_id, rule_set_id, status FROM payroll_runs WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(id).bind(t.tenant_id).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;
    if run.2 != "draft" {
        return Err(AppError::Conflict(
            "seule une paie en brouillon peut être recalculée ; utiliser une régularisation".into(),
        ));
    }
    let period: (i32, i32) =
        sqlx::query_as("SELECT year, month FROM payroll_periods WHERE id = $1 AND tenant_id = $2")
            .bind(run.0)
            .bind(t.tenant_id)
            .fetch_one(&mut *tx)
            .await?;
    let (_, end) = month_bounds(period.0, period.1)?;
    let (rules, _) = load_rules(&mut tx, t.tenant_id, run.1, end).await?;
    sqlx::query("DELETE FROM payroll_items WHERE run_id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(t.tenant_id)
        .execute(&mut *tx)
        .await?;
    let warnings = compute_items(&mut tx, t.tenant_id, id, period.0, period.1, &rules).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_run.recalculate",
        "payroll_run",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    let mut detail = run_detail(State(s), auth, Path(id)).await?.0;
    detail.warnings = warnings;
    Ok(Json(detail))
}

async fn validate_run(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RunDto>> {
    let t = auth.require(Permission::PayrollManage)?;
    let mut tx = s.db.begin().await?;
    let run: (String, bool) = sqlx::query_as(
        "SELECT status, simulation FROM payroll_runs WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if run.1 {
        return Err(AppError::Conflict(
            "paie en simulation : les règles ne sont pas approuvées, validation impossible".into(),
        ));
    }
    if run.0 != "draft" {
        return Err(AppError::Conflict("la paie n'est pas en brouillon".into()));
    }
    sqlx::query("UPDATE payroll_runs SET status='validated', validated_by=$3, validated_at=now() WHERE id=$1 AND tenant_id=$2").bind(id).bind(t.tenant_id).bind(auth.user_id).execute(&mut *tx).await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_run.validate",
        "payroll_run",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        sqlx::query_as(&format!("{SELECT_RUN} WHERE r.id = $1"))
            .bind(id)
            .fetch_one(&s.db)
            .await?,
    ))
}

async fn close_run(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RunDto>> {
    let t = auth.require(Permission::PayrollManage)?;
    let mut tx = s.db.begin().await?;
    let run: (String, Uuid) = sqlx::query_as(
        "SELECT status, period_id FROM payroll_runs WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    if run.0 != "validated" {
        return Err(AppError::Conflict(
            "seule une paie validée peut être clôturée".into(),
        ));
    }
    sqlx::query("UPDATE payroll_runs SET status='closed', closed_by=$3, closed_at=now() WHERE id=$1 AND tenant_id=$2").bind(id).bind(t.tenant_id).bind(auth.user_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE payroll_periods SET status='closed' WHERE id=$1 AND tenant_id=$2")
        .bind(run.1)
        .bind(t.tenant_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payroll_run.close",
        "payroll_run",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        sqlx::query_as(&format!("{SELECT_RUN} WHERE r.id = $1"))
            .bind(id)
            .fetch_one(&s.db)
            .await?,
    ))
}

// ---------- Bulletins ----------

#[derive(Serialize, sqlx::FromRow)]
struct PayslipSummary {
    item_id: Uuid,
    year: i32,
    month: i32,
    gross: Decimal,
    net: Decimal,
}

async fn my_payslips(
    State(s): State<AppState>,
    auth: Auth,
) -> AppResult<Json<Vec<PayslipSummary>>> {
    let t = auth.require(Permission::PayslipReadOwn)?;
    let eid = t.employee_id.ok_or(AppError::NotFound)?;
    // Uniquement les paies clôturées et non simulées, pour le propre dossier de l'utilisateur.
    let rows = sqlx::query_as(
        "SELECT i.id AS item_id, p.year, p.month, i.gross, i.net FROM payroll_items i
         JOIN payroll_runs r ON r.id = i.run_id AND r.tenant_id = i.tenant_id
         JOIN payroll_periods p ON p.id = r.period_id AND p.tenant_id = r.tenant_id
         WHERE i.tenant_id = $1 AND i.employee_id = $2 AND r.status = 'closed' AND r.simulation = FALSE
         ORDER BY p.year DESC, p.month DESC",
    )
    .bind(t.tenant_id).bind(eid).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct PayslipDetail {
    item_id: Uuid,
    employee_id: Uuid,
    matricule: String,
    employee_name: String,
    year: i32,
    month: i32,
    run_status: String,
    simulation: bool,
    rule_set_name: String,
    rule_version: i32,
    currency: String,
    result: Value,
    input: Value,
}

fn can_read_any(t: &TenantCtx) -> bool {
    has_permission(t.role, Permission::PayrollRead)
}

/// Charge un bulletin en appliquant les règles d'accès : titulaire (paie clôturée non simulée) ou lecture paie.
/// Un bulletin hors périmètre ou d'une autre société renvoie 404 pour ne pas révéler son existence.
async fn load_payslip(s: &AppState, t: &TenantCtx, item_id: Uuid) -> AppResult<PayslipDetail> {
    let row: Option<PayslipDetail> = sqlx::query_as(
        "SELECT i.id AS item_id, i.employee_id, e.matricule, e.first_name || ' ' || e.last_name AS employee_name, p.year, p.month,
                r.status AS run_status, r.simulation, rs.name AS rule_set_name, rs.version AS rule_version, tn.currency, i.result, i.input
         FROM payroll_items i
         JOIN payroll_runs r ON r.id = i.run_id AND r.tenant_id = i.tenant_id
         JOIN payroll_periods p ON p.id = r.period_id AND p.tenant_id = r.tenant_id
         JOIN payroll_rule_sets rs ON rs.id = r.rule_set_id AND rs.tenant_id = r.tenant_id
         JOIN employees e ON e.id = i.employee_id AND e.tenant_id = i.tenant_id
         JOIN tenants tn ON tn.id = i.tenant_id
         WHERE i.id = $1 AND i.tenant_id = $2",
    )
    .bind(item_id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?;
    let row = row.ok_or(AppError::NotFound)?;
    let is_own = t.employee_id == Some(row.employee_id)
        && has_permission(t.role, Permission::PayslipReadOwn)
        && row.run_status == "closed"
        && !row.simulation;
    if !(is_own || can_read_any(t)) {
        return Err(AppError::NotFound);
    }
    Ok(row)
}

async fn payslip_detail(
    State(s): State<AppState>,
    auth: Auth,
    Path(item_id): Path<Uuid>,
) -> AppResult<Json<PayslipDetail>> {
    let t = auth.tenant()?;
    let row = load_payslip(&s, t, item_id).await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payslip.read",
        "payroll_item",
        Some(item_id.to_string()),
        json!({"employee_id": row.employee_id}),
    )
    .await?;
    Ok(Json(row))
}

async fn payslip_pdf(
    State(s): State<AppState>,
    auth: Auth,
    Path(item_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let t = auth.tenant()?;
    let slip = load_payslip(&s, t, item_id).await?;
    let result: PayrollResult =
        serde_json::from_value(slip.result.clone()).map_err(|e| AppError::Internal(e.into()))?;
    let bytes = render_payslip(&slip, &result);
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "payslip.download",
        "payroll_item",
        Some(item_id.to_string()),
        json!({"employee_id": slip.employee_id}),
    )
    .await?;
    let filename = format!(
        "bulletin-{}-{}-{:02}.pdf",
        slip.matricule, slip.year, slip.month
    );
    // Le matricule est libre : on ne garde que des caractères sûrs dans le nom de fichier.
    let filename: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        bytes,
    ))
}

fn render_payslip(slip: &PayslipDetail, r: &PayrollResult) -> Vec<u8> {
    let cur = slip.currency.as_str();
    let money = |v: &Decimal| format!("{} {cur}", format_amount(&v.to_string()));
    let months = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    let period = format!(
        "{} {}",
        months
            .get((slip.month - 1).clamp(0, 11) as usize)
            .unwrap_or(&""),
        slip.year
    );

    let mut d = Document::new();
    let (left, right) = (50.0_f32, 545.0_f32);
    let mut y = 790.0_f32;
    d.text(left, y, 20.0, Font::Bold, "Bulletin de paie");
    d.text_right(right, y, 12.0, Font::Regular, &period);
    y -= 22.0;
    if slip.simulation {
        d.text(
            left,
            y,
            11.0,
            Font::Bold,
            "SIMULATION - ne constitue pas un bulletin de paie valable",
        );
        y -= 18.0;
    }
    d.text(
        left,
        y,
        11.0,
        Font::Regular,
        &format!(
            "Employé : {}   (matricule {})",
            slip.employee_name, slip.matricule
        ),
    );
    y -= 15.0;
    d.text(
        left,
        y,
        9.0,
        Font::Regular,
        &format!(
            "Règles de calcul : {} v{}",
            slip.rule_set_name, slip.rule_version
        ),
    );
    y -= 26.0;

    let groups = [
        (LineKind::Earning, "Gains"),
        (LineKind::EmployeeContribution, "Cotisations salariales"),
        (LineKind::Tax, "Impôts"),
        (LineKind::Deduction, "Retenues"),
        (
            LineKind::EmployerContribution,
            "Charges patronales (information)",
        ),
    ];
    for (kind, title) in groups {
        let lines: Vec<_> = r.lines.iter().filter(|l| l.kind == kind).collect();
        if lines.is_empty() {
            continue;
        }
        if y < 140.0 {
            d.new_page();
            y = 790.0;
        }
        d.text(left, y, 11.0, Font::Bold, title);
        d.text_right(350.0, y, 8.0, Font::Regular, "Base");
        d.text_right(420.0, y, 8.0, Font::Regular, "Taux");
        d.text_right(right, y, 8.0, Font::Regular, "Montant");
        y -= 15.0;
        for l in lines {
            if y < 80.0 {
                d.new_page();
                y = 790.0;
            }
            d.text(left, y, 10.0, Font::Regular, &l.label);
            if let Some(b) = &l.base {
                d.text_right(
                    350.0,
                    y,
                    10.0,
                    Font::Regular,
                    &format_amount(&b.to_string()),
                );
            }
            if let Some(rate) = &l.rate {
                d.text_right(
                    420.0,
                    y,
                    10.0,
                    Font::Regular,
                    &format!("{} %", (*rate * Decimal::from(100)).normalize()),
                );
            }
            d.text_right(right, y, 10.0, Font::Regular, &money(&l.amount));
            y -= 14.0;
            if let Some(note) = &l.note {
                d.text(left + 8.0, y + 2.0, 7.0, Font::Regular, note);
                y -= 9.0;
            }
        }
        y -= 10.0;
    }

    if y < 120.0 {
        d.new_page();
        y = 790.0;
    }
    let deductions = r.total_employee_contributions + r.tax + r.total_deductions;
    d.text(left, y, 11.0, Font::Regular, "Salaire brut");
    d.text_right(right, y, 11.0, Font::Regular, &money(&r.gross));
    y -= 16.0;
    d.text(left, y, 11.0, Font::Regular, "Total des retenues");
    d.text_right(right, y, 11.0, Font::Regular, &money(&deductions));
    y -= 20.0;
    d.text(left, y, 13.0, Font::Bold, "NET À PAYER");
    d.text_right(right, y, 13.0, Font::Bold, &money(&r.net));
    d.text(
        left,
        40.0,
        8.0,
        Font::Regular,
        "Document généré par MySuite. Conserver sans limitation de durée.",
    );
    d.finish()
}
