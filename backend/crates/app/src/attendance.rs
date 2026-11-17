//! Pointage : entrée/sortie, corrections RH, résumé par période.
//!
//! Principes : l'heure est celle du **serveur** (jamais celle du client) pour les pointages
//! libre-service ; une correction manuelle exige un motif et est auditée (avant/après) ;
//! personne ne corrige son propre pointage. Les paramètres (fuseau, heure de début, tolérance,
//! durée journalière) sont propres à chaque société : aucune règle légale n'est codée en dur,
//! et les « heures supplémentaires » sont **indicatives** (elles n'alimentent pas la paie).

use crate::{
    analytics::{csv_line, csv_response},
    audit,
    auth::Auth,
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use mysuite_shared::permissions::{Permission, Role};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// Un pointage ouvert depuis plus longtemps est considéré oublié : correction RH requise.
const MAX_OPEN_HOURS: i64 = 16;
const MAX_ENTRY_HOURS: i64 = 24;
const MAX_LIST_DAYS: i64 = 93;
const MAX_SUMMARY_DAYS: i64 = 366;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/attendance/settings", get(get_settings).put(put_settings))
        .route("/attendance/clock-in", post(clock_in))
        .route("/attendance/clock-out", post(clock_out))
        .route("/attendance/me", get(my_status))
        .route("/attendance/entries", get(list_entries).post(create_entry))
        .route("/attendance/entries/{id}", put(correct_entry))
        .route("/attendance/summary", get(summary))
        .route("/attendance/summary.csv", get(summary_csv))
}

// ---------- Paramètres ----------

#[derive(Serialize, Deserialize, Clone, sqlx::FromRow)]
pub(crate) struct Settings {
    pub(crate) timezone: String,
    pub(crate) work_start: NaiveTime,
    pub(crate) grace_minutes: i32,
    pub(crate) daily_minutes: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            timezone: "UTC".into(),
            work_start: NaiveTime::from_hms_opt(8, 0, 0).expect("heure valide"),
            grace_minutes: 10,
            daily_minutes: 480,
        }
    }
}

pub(crate) async fn load_settings(db: &PgPool, tenant_id: Uuid) -> AppResult<Settings> {
    let row: Option<Settings> = sqlx::query_as(
        "SELECT timezone, work_start, grace_minutes, daily_minutes FROM attendance_settings WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    Ok(row.unwrap_or_default())
}

async fn get_settings(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Settings>> {
    let t = auth.tenant()?;
    Ok(Json(load_settings(&s.db, t.tenant_id).await?))
}

async fn put_settings(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<Settings>,
) -> AppResult<Json<Settings>> {
    let t = auth.require(Permission::AttendanceManage)?;
    ensure(
        (0..=240).contains(&b.grace_minutes),
        "tolérance entre 0 et 240 minutes",
    )?;
    ensure(
        (60..=1440).contains(&b.daily_minutes),
        "durée journalière entre 60 et 1440 minutes",
    )?;
    let (known,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
            .bind(&b.timezone)
            .fetch_one(&s.db)
            .await?;
    ensure(known, "fuseau horaire inconnu (ex. Africa/Lubumbashi)")?;
    let mut tx = s.db.begin().await?;
    sqlx::query(
        "INSERT INTO attendance_settings (tenant_id, timezone, work_start, grace_minutes, daily_minutes)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (tenant_id) DO UPDATE SET timezone = EXCLUDED.timezone, work_start = EXCLUDED.work_start,
            grace_minutes = EXCLUDED.grace_minutes, daily_minutes = EXCLUDED.daily_minutes, updated_at = now()",
    )
    .bind(t.tenant_id)
    .bind(&b.timezone)
    .bind(b.work_start)
    .bind(b.grace_minutes)
    .bind(b.daily_minutes)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.settings",
        "tenant",
        Some(t.tenant_id.to_string()),
        json!({"timezone": b.timezone, "work_start": b.work_start, "grace_minutes": b.grace_minutes, "daily_minutes": b.daily_minutes}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(b))
}

// ---------- Entrées ----------

#[derive(Serialize, sqlx::FromRow)]
struct EntryDto {
    id: Uuid,
    employee_id: Uuid,
    employee_name: String,
    matricule: String,
    clock_in: DateTime<Utc>,
    clock_out: Option<DateTime<Utc>>,
    minutes: Option<i64>,
    source: String,
    note: Option<String>,
    corrected: bool,
}

const SELECT_ENTRY: &str = "
    SELECT a.id, a.employee_id, e.first_name || ' ' || e.last_name AS employee_name, e.matricule,
           a.clock_in, a.clock_out,
           (EXTRACT(EPOCH FROM (a.clock_out - a.clock_in)) / 60)::bigint AS minutes,
           a.source, a.note, (a.corrected_at IS NOT NULL) AS corrected
    FROM attendance_entries a
    JOIN employees e ON e.id = a.employee_id AND e.tenant_id = a.tenant_id";

async fn entry_by_id(db: &PgPool, tenant_id: Uuid, id: Uuid) -> AppResult<EntryDto> {
    sqlx::query_as(&format!(
        "{SELECT_ENTRY} WHERE a.id = $1 AND a.tenant_id = $2"
    ))
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

/// Verrouille l'employé (sérialise les pointages concurrents) et renvoie son statut.
pub(crate) async fn lock_employee(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    employee_id: Uuid,
) -> AppResult<String> {
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM employees WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(employee_id)
            .bind(tenant_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(AppError::NotFound)?;
    Ok(status)
}

async fn clock_in(
    State(s): State<AppState>,
    auth: Auth,
) -> AppResult<(StatusCode, Json<EntryDto>)> {
    let t = auth.require(Permission::AttendanceClockOwn)?;
    let eid = t
        .employee_id
        .ok_or_else(|| AppError::Validation("aucun dossier employé lié à votre compte".into()))?;
    let mut tx = s.db.begin().await?;
    let status = lock_employee(&mut tx, t.tenant_id, eid).await?;
    ensure(
        status == "active",
        "seuls les employés actifs peuvent pointer",
    )?;
    let (open,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM attendance_entries WHERE employee_id = $1 AND tenant_id = $2 AND clock_out IS NULL",
    )
    .bind(eid)
    .bind(t.tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    if open > 0 {
        return Err(AppError::Conflict("un pointage est déjà ouvert".into()));
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO attendance_entries (tenant_id, employee_id, clock_in, source, created_by)
         VALUES ($1,$2,now(),'self',$3) RETURNING id",
    )
    .bind(t.tenant_id)
    .bind(eid)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.clock_in",
        "attendance_entry",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(entry_by_id(&s.db, t.tenant_id, id).await?),
    ))
}

async fn clock_out(State(s): State<AppState>, auth: Auth) -> AppResult<Json<EntryDto>> {
    let t = auth.require(Permission::AttendanceClockOwn)?;
    let eid = t
        .employee_id
        .ok_or_else(|| AppError::Validation("aucun dossier employé lié à votre compte".into()))?;
    let mut tx = s.db.begin().await?;
    lock_employee(&mut tx, t.tenant_id, eid).await?;
    let open: Option<(Uuid, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, clock_in, now() FROM attendance_entries
         WHERE employee_id = $1 AND tenant_id = $2 AND clock_out IS NULL FOR UPDATE",
    )
    .bind(eid)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (id, started, now) =
        open.ok_or_else(|| AppError::Conflict("aucun pointage ouvert".into()))?;
    if now - started > Duration::hours(MAX_OPEN_HOURS) {
        return Err(AppError::Conflict(
            "pointage ouvert depuis trop longtemps : demandez une correction aux RH".into(),
        ));
    }
    sqlx::query("UPDATE attendance_entries SET clock_out = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.clock_out",
        "attendance_entry",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(entry_by_id(&s.db, t.tenant_id, id).await?))
}

#[derive(Serialize)]
struct MyStatus {
    settings: Settings,
    open: Option<EntryDto>,
    today: Vec<EntryDto>,
    today_minutes: i64,
}

async fn my_status(State(s): State<AppState>, auth: Auth) -> AppResult<Json<MyStatus>> {
    let t = auth.require(Permission::AttendanceClockOwn)?;
    let eid = t.employee_id.ok_or(AppError::NotFound)?;
    let settings = load_settings(&s.db, t.tenant_id).await?;
    let today: Vec<EntryDto> = sqlx::query_as(&format!(
        "{SELECT_ENTRY} WHERE a.tenant_id = $1 AND a.employee_id = $2
           AND (a.clock_in AT TIME ZONE $3)::date = (now() AT TIME ZONE $3)::date
         ORDER BY a.clock_in"
    ))
    .bind(t.tenant_id)
    .bind(eid)
    .bind(&settings.timezone)
    .fetch_all(&s.db)
    .await?;
    let open = sqlx::query_as(&format!(
        "{SELECT_ENTRY} WHERE a.tenant_id = $1 AND a.employee_id = $2 AND a.clock_out IS NULL"
    ))
    .bind(t.tenant_id)
    .bind(eid)
    .fetch_optional(&s.db)
    .await?;
    let today_minutes = today.iter().filter_map(|e| e.minutes).sum();
    Ok(Json(MyStatus {
        settings,
        open,
        today,
        today_minutes,
    }))
}

// ---------- Portée et période ----------

#[derive(Deserialize)]
struct RangeQuery {
    scope: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    employee_id: Option<Uuid>,
}

/// (tout, équipe de ce manager, uniquement cet employé)
type Scope = (bool, Option<Uuid>, Option<Uuid>);

fn resolve_scope(auth: &Auth, scope: Option<&str>) -> AppResult<Scope> {
    match scope.unwrap_or("mine") {
        "mine" => {
            let t = auth.require(Permission::AttendanceClockOwn)?;
            Ok((false, None, Some(t.employee_id.ok_or(AppError::NotFound)?)))
        }
        "team" => {
            let t = auth.require(Permission::AttendanceRead)?;
            if matches!(t.role, Role::Manager) {
                // Un manager ne voit que ses subordonnés directs.
                Ok((false, Some(t.employee_id.ok_or(AppError::Forbidden)?), None))
            } else {
                Ok((true, None, None))
            }
        }
        "all" => {
            let t = auth.require(Permission::AttendanceRead)?;
            if matches!(t.role, Role::Manager) {
                return Err(AppError::Forbidden);
            }
            Ok((true, None, None))
        }
        _ => Err(AppError::Validation("scope invalide".into())),
    }
}

fn range(q: &RangeQuery, max_days: i64) -> AppResult<(NaiveDate, NaiveDate)> {
    let today = Utc::now().date_naive();
    let from = q
        .from
        .unwrap_or_else(|| today.with_day(1).expect("jour 1 valide"));
    let to = q.to.unwrap_or(today);
    ensure(to >= from, "la fin précède le début")?;
    ensure(
        (to - from).num_days() < max_days,
        "période trop longue pour ce rapport",
    )?;
    Ok((from, to))
}

async fn list_entries(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<Vec<EntryDto>>> {
    let t = auth.tenant()?;
    let (all, team_of, only) = resolve_scope(&auth, q.scope.as_deref())?;
    let (from, to) = range(&q, MAX_LIST_DAYS)?;
    let settings = load_settings(&s.db, t.tenant_id).await?;
    let rows = sqlx::query_as(&format!(
        "{SELECT_ENTRY} WHERE a.tenant_id = $1
           AND (a.clock_in AT TIME ZONE $2)::date BETWEEN $3 AND $4
           AND ($5::bool OR e.manager_id = $6 OR a.employee_id = $7)
           AND ($8::uuid IS NULL OR a.employee_id = $8)
         ORDER BY a.clock_in DESC LIMIT 500"
    ))
    .bind(t.tenant_id)
    .bind(&settings.timezone)
    .bind(from)
    .bind(to)
    .bind(all)
    .bind(team_of)
    .bind(only)
    .bind(q.employee_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

// ---------- Saisie et correction RH ----------

#[derive(Deserialize)]
struct ManualEntry {
    employee_id: Uuid,
    clock_in: DateTime<Utc>,
    clock_out: Option<DateTime<Utc>>,
    note: String,
}

#[derive(Deserialize)]
struct Correction {
    clock_in: DateTime<Utc>,
    clock_out: Option<DateTime<Utc>>,
    note: String,
}

fn validate_times(
    clock_in: DateTime<Utc>,
    clock_out: Option<DateTime<Utc>>,
    note: &str,
) -> AppResult<()> {
    ensure(!note.trim().is_empty(), "un motif est obligatoire")?;
    let limit = Utc::now() + Duration::minutes(1);
    ensure(clock_in <= limit, "heure d'entrée dans le futur")?;
    if let Some(out) = clock_out {
        ensure(out > clock_in, "la sortie doit suivre l'entrée")?;
        ensure(out <= limit, "heure de sortie dans le futur")?;
        ensure(
            out - clock_in <= Duration::hours(MAX_ENTRY_HOURS),
            "un pointage ne peut pas dépasser 24 h",
        )?;
    }
    Ok(())
}

/// Vrai si l'intervalle chevauche un autre pointage du même employé.
pub(crate) async fn overlaps(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    employee_id: Uuid,
    except: Option<Uuid>,
    clock_in: DateTime<Utc>,
    clock_out: Option<DateTime<Utc>>,
) -> AppResult<bool> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM attendance_entries
         WHERE tenant_id = $1 AND employee_id = $2 AND ($3::uuid IS NULL OR id <> $3)
           AND clock_in < COALESCE($5, 'infinity'::timestamptz)
           AND COALESCE(clock_out, 'infinity'::timestamptz) > $4",
    )
    .bind(tenant_id)
    .bind(employee_id)
    .bind(except)
    .bind(clock_in)
    .bind(clock_out)
    .fetch_one(&mut **tx)
    .await?;
    Ok(n > 0)
}

async fn create_entry(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<ManualEntry>,
) -> AppResult<(StatusCode, Json<EntryDto>)> {
    let t = auth.require(Permission::AttendanceManage)?;
    validate_times(b.clock_in, b.clock_out, &b.note)?;
    if t.employee_id == Some(b.employee_id) {
        return Err(AppError::Forbidden);
    }
    let mut tx = s.db.begin().await?;
    lock_employee(&mut tx, t.tenant_id, b.employee_id).await?;
    if overlaps(
        &mut tx,
        t.tenant_id,
        b.employee_id,
        None,
        b.clock_in,
        b.clock_out,
    )
    .await?
    {
        return Err(AppError::Conflict(
            "chevauche un autre pointage de cet employé".into(),
        ));
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO attendance_entries (tenant_id, employee_id, clock_in, clock_out, source, note, created_by)
         VALUES ($1,$2,$3,$4,'manual',$5,$6) RETURNING id",
    )
    .bind(t.tenant_id)
    .bind(b.employee_id)
    .bind(b.clock_in)
    .bind(b.clock_out)
    .bind(b.note.trim())
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.manual_entry",
        "attendance_entry",
        Some(id.to_string()),
        json!({"employee_id": b.employee_id, "clock_in": b.clock_in, "clock_out": b.clock_out, "note": b.note.trim()}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(entry_by_id(&s.db, t.tenant_id, id).await?),
    ))
}

async fn correct_entry(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(b): Json<Correction>,
) -> AppResult<Json<EntryDto>> {
    let t = auth.require(Permission::AttendanceManage)?;
    validate_times(b.clock_in, b.clock_out, &b.note)?;
    let mut tx = s.db.begin().await?;
    let before: (Uuid, DateTime<Utc>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT employee_id, clock_in, clock_out FROM attendance_entries WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    // Personne ne corrige son propre pointage (même principe que pour les congés).
    if t.employee_id == Some(before.0) {
        return Err(AppError::Forbidden);
    }
    lock_employee(&mut tx, t.tenant_id, before.0).await?;
    if overlaps(
        &mut tx,
        t.tenant_id,
        before.0,
        Some(id),
        b.clock_in,
        b.clock_out,
    )
    .await?
    {
        return Err(AppError::Conflict(
            "chevauche un autre pointage de cet employé".into(),
        ));
    }
    sqlx::query(
        "UPDATE attendance_entries SET clock_in = $1, clock_out = $2, note = $3, corrected_by = $4, corrected_at = now()
         WHERE id = $5 AND tenant_id = $6",
    )
    .bind(b.clock_in)
    .bind(b.clock_out)
    .bind(b.note.trim())
    .bind(auth.user_id)
    .bind(id)
    .bind(t.tenant_id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.correct",
        "attendance_entry",
        Some(id.to_string()),
        json!({
            "employee_id": before.0,
            "before": {"clock_in": before.1, "clock_out": before.2},
            "after": {"clock_in": b.clock_in, "clock_out": b.clock_out},
            "note": b.note.trim()
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(entry_by_id(&s.db, t.tenant_id, id).await?))
}

// ---------- Résumé ----------

#[derive(Serialize, sqlx::FromRow)]
struct SummaryRow {
    employee_id: Uuid,
    matricule: String,
    name: String,
    days_present: i64,
    worked_minutes: i64,
    late_days: i64,
    overtime_minutes: i64,
    open_days: i64,
}

#[derive(Serialize)]
struct Summary {
    from: NaiveDate,
    to: NaiveDate,
    settings: Settings,
    rows: Vec<SummaryRow>,
}

async fn build_summary(s: &AppState, auth: &Auth, q: &RangeQuery) -> AppResult<Summary> {
    let t = auth.tenant()?;
    let scope = q.scope.as_deref().unwrap_or("team");
    ensure(
        scope != "mine",
        "le résumé concerne une équipe ou la société",
    )?;
    let (all, team_of, _) = resolve_scope(auth, Some(scope))?;
    let (from, to) = range(q, MAX_SUMMARY_DAYS)?;
    let st = load_settings(&s.db, t.tenant_id).await?;
    let rows = sqlx::query_as(
        "WITH e AS (
            SELECT a.employee_id, (a.clock_in AT TIME ZONE $2)::date AS d, a.clock_in, a.clock_out
            FROM attendance_entries a
            JOIN employees emp ON emp.id = a.employee_id AND emp.tenant_id = a.tenant_id
            WHERE a.tenant_id = $1 AND (a.clock_in AT TIME ZONE $2)::date BETWEEN $3 AND $4
              AND ($5::bool OR emp.manager_id = $6)
         ), day AS (
            SELECT employee_id, d, min(clock_in) AS first_in,
                   COALESCE(sum(EXTRACT(EPOCH FROM (clock_out - clock_in)) / 60) FILTER (WHERE clock_out IS NOT NULL), 0)::bigint AS minutes,
                   bool_or(clock_out IS NULL) AS is_open
            FROM e GROUP BY employee_id, d
         )
         SELECT day.employee_id, emp.matricule, emp.first_name || ' ' || emp.last_name AS name,
                count(*)::bigint AS days_present,
                COALESCE(sum(minutes), 0)::bigint AS worked_minutes,
                count(*) FILTER (WHERE (first_in AT TIME ZONE $2)::time > $7::time + ($8::int * interval '1 minute'))::bigint AS late_days,
                COALESCE(sum(GREATEST(minutes - $9::int, 0)), 0)::bigint AS overtime_minutes,
                count(*) FILTER (WHERE is_open)::bigint AS open_days
         FROM day JOIN employees emp ON emp.id = day.employee_id AND emp.tenant_id = $1
         GROUP BY day.employee_id, emp.matricule, emp.first_name, emp.last_name
         ORDER BY name",
    )
    .bind(t.tenant_id)
    .bind(&st.timezone)
    .bind(from)
    .bind(to)
    .bind(all)
    .bind(team_of)
    .bind(st.work_start)
    .bind(st.grace_minutes)
    .bind(st.daily_minutes)
    .fetch_all(&s.db)
    .await?;
    Ok(Summary {
        from,
        to,
        settings: st,
        rows,
    })
}

async fn summary(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<Summary>> {
    Ok(Json(build_summary(&s, &auth, &q).await?))
}

async fn summary_csv(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<RangeQuery>,
) -> AppResult<impl IntoResponse> {
    let t = auth.tenant()?;
    let sm = build_summary(&s, &auth, &q).await?;
    let mut out = csv_line(
        &[
            "Matricule",
            "Nom",
            "Jours de présence",
            "Minutes travaillées",
            "Jours en retard",
            "Minutes au-delà de la durée standard (indicatif)",
            "Jours avec pointage ouvert",
        ]
        .map(String::from),
    );
    for r in &sm.rows {
        out.push_str(&csv_line(&[
            r.matricule.clone(),
            r.name.clone(),
            r.days_present.to_string(),
            r.worked_minutes.to_string(),
            r.late_days.to_string(),
            r.overtime_minutes.to_string(),
            r.open_days.to_string(),
        ]));
    }
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.export",
        "attendance_summary",
        None,
        json!({"from": sm.from, "to": sm.to, "rows": sm.rows.len()}),
    )
    .await?;
    Ok(csv_response(
        &format!("pointage_{}_{}.csv", sm.from, sm.to),
        out,
    ))
}
