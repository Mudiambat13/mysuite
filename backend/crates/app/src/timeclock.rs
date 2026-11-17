//! Badgeuses : réception des passages (API appareil à clé, import CSV), correspondance badge → employé
//! et transformation des passages en pointages.
//!
//! Principes : l'ingestion est **idempotente** (une badgeuse qui renvoie un passage ne crée rien de plus),
//! chaque passage reçu est conservé brut avec son résultat (« journal des passages »), les passages
//! inconnus ou refusés ne sont jamais perdus et peuvent être retraités après correction, et un appareil ne peut
//! rien faire d'autre qu'envoyer des passages pour SA société (clé révocable, stockée hachée).

use crate::{
    attendance::{load_settings, lock_employee, overlaps},
    audit,
    auth::{Auth, hash_token, new_token},
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::{delete, get, post},
};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use mysuite_shared::permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const MAX_DEVICE_BATCH: usize = 500;
const MAX_IMPORT_ROWS: usize = 2000;
/// Deux passages d'un même employé à moins de 60 s : le second est un double badgeage.
const DOUBLE_PUNCH_SECONDS: i64 = 60;
const MAX_OPEN_HOURS: i64 = 16;
/// Une badgeuse hors ligne peut renvoyer ses passages tamponnés jusqu'à 31 jours après.
const DEVICE_MAX_AGE_DAYS: i64 = 31;
const IMPORT_MAX_AGE_DAYS: i64 = 366;

/// Routes avec session utilisateur (protégées par le garde CSRF).
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/attendance/devices", get(list_devices).post(create_device))
        .route("/attendance/devices/{id}/revoke", post(revoke_device))
        .route("/attendance/badges", get(list_badges).post(create_badge))
        .route("/attendance/badges/{id}", delete(delete_badge))
        .route("/attendance/import", post(import_csv))
        .route("/attendance/punches", get(list_punches))
        .route("/attendance/punches/reprocess", post(reprocess))
}

/// Routes appareil : authentification par clé `Bearer`, sans cookie donc sans CSRF.
pub fn device_routes() -> Router<AppState> {
    Router::new().route("/punches", post(device_punches))
}

// ---------- Types ----------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    In,
    Out,
}

impl Direction {
    fn parse(v: &str) -> Option<Option<Self>> {
        match v.trim().to_lowercase().as_str() {
            "" | "auto" | "toggle" => Some(None),
            "in" | "i" | "e" | "entree" | "entrée" | "entry" | "checkin" | "check-in" => {
                Some(Some(Self::In))
            }
            "out" | "o" | "s" | "sortie" | "exit" | "checkout" | "check-out" => {
                Some(Some(Self::Out))
            }
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
        }
    }
}

struct Incoming {
    identifier: String,
    at: DateTime<Utc>,
    direction: Option<Direction>,
    device_id: Option<Uuid>,
    source: &'static str,
    line: Option<usize>,
}

#[derive(Serialize, Clone)]
pub struct Outcome {
    line: Option<usize>,
    identifier: String,
    at: Option<DateTime<Utc>>,
    /// in | out | duplicate | unknown_badge | rejected
    status: String,
    reason: Option<String>,
    employee_name: Option<String>,
}

#[derive(Serialize, Default)]
struct Counts {
    total: usize,
    entries_opened: usize,
    entries_closed: usize,
    duplicates: usize,
    unknown_badges: usize,
    rejected: usize,
}

impl Counts {
    fn of(rows: &[Outcome]) -> Self {
        let mut c = Self {
            total: rows.len(),
            ..Self::default()
        };
        for r in rows {
            match r.status.as_str() {
                "in" => c.entries_opened += 1,
                "out" => c.entries_closed += 1,
                "duplicate" => c.duplicates += 1,
                "unknown_badge" => c.unknown_badges += 1,
                _ => c.rejected += 1,
            }
        }
        c
    }
}

#[derive(Serialize)]
struct IngestResult {
    dry_run: bool,
    counts: Counts,
    rows: Vec<Outcome>,
}

// ---------- Cœur : passage → pointage ----------

async fn resolve_employee(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    identifier: &str,
) -> AppResult<Option<(Uuid, String)>> {
    // Le badge a priorité ; à défaut, le matricule.
    let by_badge: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT e.id, e.first_name || ' ' || e.last_name FROM employee_badges b
         JOIN employees e ON e.id = b.employee_id AND e.tenant_id = b.tenant_id
         WHERE b.tenant_id = $1 AND b.badge_code = $2",
    )
    .bind(tenant_id)
    .bind(identifier)
    .fetch_optional(&mut **tx)
    .await?;
    if by_badge.is_some() {
        return Ok(by_badge);
    }
    Ok(sqlx::query_as(
        "SELECT id, first_name || ' ' || last_name FROM employees WHERE tenant_id = $1 AND matricule = $2",
    )
    .bind(tenant_id)
    .bind(identifier)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Applique les passages **dans l'ordre chronologique**. Ne valide pas la transaction.
async fn process(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    label: &str,
    mut incoming: Vec<Incoming>,
) -> AppResult<Vec<Outcome>> {
    incoming.sort_by_key(|p| p.at);
    let now = Utc::now();
    let mut out = Vec::with_capacity(incoming.len());
    for p in incoming {
        let mk = |status: &str, reason: Option<String>, name: Option<String>| Outcome {
            line: p.line,
            identifier: p.identifier.clone(),
            at: Some(p.at),
            status: status.into(),
            reason,
            employee_name: name,
        };
        // 1. Journal brut ; conflit = passage déjà reçu (idempotence).
        let raw: Option<(Uuid,)> = sqlx::query_as(
            "INSERT INTO attendance_raw_punches (tenant_id, device_id, source, identifier, punched_at, direction, status)
             VALUES ($1,$2,$3,$4,$5,$6,'rejected')
             ON CONFLICT (tenant_id, identifier, punched_at) DO NOTHING RETURNING id",
        )
        .bind(tenant_id)
        .bind(p.device_id)
        .bind(p.source)
        .bind(&p.identifier)
        .bind(p.at)
        .bind(p.direction.map(Direction::as_str))
        .fetch_optional(&mut **tx)
        .await?;
        let Some((raw_id,)) = raw else {
            out.push(mk("duplicate", Some("passage déjà reçu".into()), None));
            continue;
        };

        let (status, reason, employee, entry) = decide(tx, tenant_id, label, &p, now).await?;
        sqlx::query(
            "UPDATE attendance_raw_punches SET status = $1, reason = $2, employee_id = $3, entry_id = $4 WHERE id = $5",
        )
        .bind(status)
        .bind(&reason)
        .bind(employee.as_ref().map(|e| e.0))
        .bind(entry)
        .bind(raw_id)
        .execute(&mut **tx)
        .await?;
        out.push(mk(status, reason, employee.map(|e| e.1)));
    }
    Ok(out)
}

type RawRow = (Option<Uuid>, String, String, DateTime<Utc>, Option<String>);

type Decision = (
    &'static str,
    Option<String>,
    Option<(Uuid, String)>,
    Option<Uuid>,
);

async fn decide(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    label: &str,
    p: &Incoming,
    now: DateTime<Utc>,
) -> AppResult<Decision> {
    let rej = |r: &str, e: Option<(Uuid, String)>| -> AppResult<Decision> {
        Ok(("rejected", Some(r.to_string()), e, None))
    };
    let Some(emp) = resolve_employee(tx, tenant_id, &p.identifier).await? else {
        return Ok((
            "unknown_badge",
            Some("badge ou matricule inconnu".into()),
            None,
            None,
        ));
    };
    let max_age = if p.source == "device" {
        DEVICE_MAX_AGE_DAYS
    } else {
        IMPORT_MAX_AGE_DAYS
    };
    if p.at > now + Duration::minutes(5) {
        return rej("passage dans le futur", Some(emp));
    }
    if p.at < now - Duration::days(max_age) {
        return rej("passage trop ancien", Some(emp));
    }
    let status = lock_employee(tx, tenant_id, emp.0).await?;
    if status != "active" {
        return rej("employé non actif", Some(emp));
    }
    let (near,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM attendance_raw_punches
         WHERE tenant_id = $1 AND employee_id = $2 AND status IN ('in','out')
           AND punched_at BETWEEN $3 AND $4",
    )
    .bind(tenant_id)
    .bind(emp.0)
    .bind(p.at - Duration::seconds(DOUBLE_PUNCH_SECONDS))
    .bind(p.at + Duration::seconds(DOUBLE_PUNCH_SECONDS))
    .fetch_one(&mut **tx)
    .await?;
    if near > 0 {
        return Ok(("duplicate", Some("double badgeage".into()), Some(emp), None));
    }
    let open: Option<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, clock_in FROM attendance_entries WHERE tenant_id = $1 AND employee_id = $2 AND clock_out IS NULL FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(emp.0)
    .fetch_optional(&mut **tx)
    .await?;
    let note = format!("badgeuse : {label}");
    match open {
        Some((id, started)) => {
            if p.direction == Some(Direction::In) {
                return rej("un pointage est déjà ouvert", Some(emp));
            }
            if p.at <= started {
                return rej(
                    "un pointage plus récent est encore ouvert : fermez-le d'abord (correction RH), puis retraitez",
                    Some(emp),
                );
            }
            if p.at - started > Duration::hours(MAX_OPEN_HOURS) {
                return rej(
                    "pointage ouvert depuis trop longtemps : correction RH requise",
                    Some(emp),
                );
            }
            // La sortie ne doit pas recouvrir un autre pointage de l'employé.
            if overlaps(tx, tenant_id, emp.0, Some(id), started, Some(p.at)).await? {
                return rej("la sortie chevauche un autre pointage", Some(emp));
            }
            sqlx::query("UPDATE attendance_entries SET clock_out = $1 WHERE id = $2")
                .bind(p.at)
                .bind(id)
                .execute(&mut **tx)
                .await?;
            Ok(("out", None, Some(emp), Some(id)))
        }
        None => {
            if p.direction == Some(Direction::Out) {
                return rej("sortie sans entrée ouverte", Some(emp));
            }
            // L'entrée doit tomber hors de tout pointage existant (la sortie sera vérifiée à sa réception).
            if overlaps(
                tx,
                tenant_id,
                emp.0,
                None,
                p.at,
                Some(p.at + Duration::seconds(1)),
            )
            .await?
            {
                return rej("chevauche un pointage existant", Some(emp));
            }
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO attendance_entries (tenant_id, employee_id, clock_in, source, note)
                 VALUES ($1,$2,$3,$4,$5) RETURNING id",
            )
            .bind(tenant_id)
            .bind(emp.0)
            .bind(p.at)
            .bind(if p.source == "device" {
                "device"
            } else {
                "import"
            })
            .bind(note)
            .fetch_one(&mut **tx)
            .await?;
            Ok(("in", None, Some(emp), Some(id)))
        }
    }
}

// ---------- API appareil ----------

struct DeviceCtx {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
}

async fn authenticate_device(db: &PgPool, headers: &HeaderMap) -> AppResult<DeviceCtx> {
    let key = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "UPDATE attendance_devices SET last_seen_at = now()
         WHERE api_key_hash = $1 AND active RETURNING id, tenant_id, name",
    )
    .bind(hash_token(key))
    .fetch_optional(db)
    .await?;
    let (id, tenant_id, name) = row.ok_or(AppError::Unauthorized)?;
    Ok(DeviceCtx {
        id,
        tenant_id,
        name,
    })
}

#[derive(Deserialize)]
struct DevicePunch {
    badge: String,
    /// RFC 3339 **avec décalage** (ex. 2026-03-02T08:03:00+02:00) : jamais d'heure locale ambiguë.
    at: DateTime<Utc>,
    direction: Option<String>,
}

#[derive(Deserialize)]
struct DeviceBatch {
    punches: Vec<DevicePunch>,
}

async fn device_punches(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<DeviceBatch>,
) -> AppResult<Json<IngestResult>> {
    let dev = authenticate_device(&s.db, &headers).await?;
    ensure(!b.punches.is_empty(), "aucun passage")?;
    ensure(
        b.punches.len() <= MAX_DEVICE_BATCH,
        "trop de passages dans un envoi (500 maximum)",
    )?;
    let mut incoming = Vec::new();
    let mut rows = Vec::new();
    for (i, p) in b.punches.into_iter().enumerate() {
        let badge = p.badge.trim().to_string();
        let dir = Direction::parse(p.direction.as_deref().unwrap_or(""));
        if badge.is_empty() || badge.len() > 64 || dir.is_none() {
            rows.push(Outcome {
                line: Some(i + 1),
                identifier: badge,
                at: Some(p.at),
                status: "rejected".into(),
                reason: Some("badge ou direction invalide".into()),
                employee_name: None,
            });
            continue;
        }
        incoming.push(Incoming {
            identifier: badge,
            at: p.at,
            direction: dir.flatten(),
            device_id: Some(dev.id),
            source: "device",
            line: Some(i + 1),
        });
    }
    let mut tx = s.db.begin().await?;
    rows.extend(process(&mut tx, dev.tenant_id, &dev.name, incoming).await?);
    let counts = Counts::of(&rows);
    audit::record(
        &mut *tx,
        Some(dev.tenant_id),
        None,
        "attendance.device_ingest",
        "attendance_device",
        Some(dev.id.to_string()),
        json!({"device": dev.name, "counts": counts}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(IngestResult {
        dry_run: false,
        counts,
        rows,
    }))
}

// ---------- Appareils ----------

#[derive(Serialize, sqlx::FromRow)]
struct DeviceDto {
    id: Uuid,
    name: String,
    key_prefix: String,
    active: bool,
    last_seen_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

async fn list_devices(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<DeviceDto>>> {
    let t = auth.require(Permission::AttendanceManage)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT id, name, key_prefix, active, last_seen_at, created_at FROM attendance_devices
             WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(t.tenant_id)
        .fetch_all(&s.db)
        .await?,
    ))
}

#[derive(Deserialize)]
struct NewDevice {
    name: String,
}

#[derive(Serialize)]
struct CreatedDevice {
    id: Uuid,
    name: String,
    /// Affichée UNE seule fois : seul son empreinte est conservée.
    api_key: String,
}

async fn create_device(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewDevice>,
) -> AppResult<(StatusCode, Json<CreatedDevice>)> {
    let t = auth.require(Permission::AttendanceManage)?;
    let name = b.name.trim().to_string();
    ensure(
        !name.is_empty() && name.len() <= 80,
        "nom requis (80 caractères maximum)",
    )?;
    let key = format!("mskey_{}", new_token());
    let mut tx = s.db.begin().await?;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO attendance_devices (tenant_id, name, api_key_hash, key_prefix, created_by)
         VALUES ($1,$2,$3,$4,$5) RETURNING id",
    )
    .bind(t.tenant_id)
    .bind(&name)
    .bind(hash_token(&key))
    .bind(&key[..12])
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.device_create",
        "attendance_device",
        Some(id.to_string()),
        json!({"name": name}),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedDevice {
            id,
            name,
            api_key: key,
        }),
    ))
}

async fn revoke_device(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::AttendanceManage)?;
    let mut tx = s.db.begin().await?;
    let done = sqlx::query(
        "UPDATE attendance_devices SET active = FALSE WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(t.tenant_id)
    .execute(&mut *tx)
    .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.device_revoke",
        "attendance_device",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Badges ----------

#[derive(Serialize, sqlx::FromRow)]
struct BadgeDto {
    id: Uuid,
    employee_id: Uuid,
    employee_name: String,
    matricule: String,
    badge_code: String,
}

async fn list_badges(State(s): State<AppState>, auth: Auth) -> AppResult<Json<Vec<BadgeDto>>> {
    let t = auth.require(Permission::AttendanceManage)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT b.id, b.employee_id, e.first_name || ' ' || e.last_name AS employee_name, e.matricule, b.badge_code
             FROM employee_badges b JOIN employees e ON e.id = b.employee_id AND e.tenant_id = b.tenant_id
             WHERE b.tenant_id = $1 ORDER BY e.last_name, e.first_name, b.badge_code",
        )
        .bind(t.tenant_id)
        .fetch_all(&s.db)
        .await?,
    ))
}

#[derive(Deserialize)]
struct NewBadge {
    employee_id: Uuid,
    badge_code: String,
}

async fn create_badge(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewBadge>,
) -> AppResult<(StatusCode, Json<BadgeDto>)> {
    let t = auth.require(Permission::AttendanceManage)?;
    let code = b.badge_code.trim().to_string();
    ensure(
        !code.is_empty() && code.len() <= 64,
        "code badge requis (64 caractères maximum)",
    )?;
    let mut tx = s.db.begin().await?;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO employee_badges (tenant_id, employee_id, badge_code) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(t.tenant_id)
    .bind(b.employee_id)
    .bind(&code)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.badge_assign",
        "employee",
        Some(b.employee_id.to_string()),
        json!({"badge_code": code}),
    )
    .await?;
    tx.commit().await?;
    let row = sqlx::query_as(
        "SELECT b.id, b.employee_id, e.first_name || ' ' || e.last_name AS employee_name, e.matricule, b.badge_code
         FROM employee_badges b JOIN employees e ON e.id = b.employee_id AND e.tenant_id = b.tenant_id
         WHERE b.id = $1 AND b.tenant_id = $2",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn delete_badge(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::AttendanceManage)?;
    let mut tx = s.db.begin().await?;
    let done = sqlx::query("DELETE FROM employee_badges WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(t.tenant_id)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.badge_remove",
        "employee_badge",
        Some(id.to_string()),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Import CSV ----------

#[derive(Deserialize)]
struct ImportBody {
    content: String,
    #[serde(default)]
    dry_run: bool,
}

/// Découpe un CSV (séparateur `,` `;` ou tabulation, guillemets `"` doublés pour s'échapper).
pub(crate) fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let content = content.trim_start_matches('\u{feff}');
    let first = content.lines().next().unwrap_or("");
    let sep = [';', '\t', ',']
        .into_iter()
        .max_by_key(|c| first.matches(*c).count())
        .unwrap_or(',');
    let mut rows = Vec::new();
    let (mut row, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (c, false) if c == sep => row.push(std::mem::take(&mut cell)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut cell));
                if row.iter().any(|c| !c.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            (c, _) => cell.push(c),
        }
    }
    row.push(cell);
    if row.iter().any(|c| !c.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

enum Parsed {
    Utc(DateTime<Utc>),
    Local(NaiveDateTime),
}

fn parse_when(date: &str, time: Option<&str>) -> Option<Parsed> {
    let d = date.trim();
    if time.is_none() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(d) {
            return Some(Parsed::Utc(dt.with_timezone(&Utc)));
        }
        for f in [
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M",
            "%Y-%m-%dT%H:%M:%S",
            "%Y-%m-%dT%H:%M",
            "%d/%m/%Y %H:%M:%S",
            "%d/%m/%Y %H:%M",
        ] {
            if let Ok(n) = NaiveDateTime::parse_from_str(d, f) {
                return Some(Parsed::Local(n));
            }
        }
        return None;
    }
    let t = time?.trim();
    let date = NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(d, "%d/%m/%Y"))
        .ok()?;
    let time = NaiveTime::parse_from_str(t, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(t, "%H:%M"))
        .ok()?;
    Some(Parsed::Local(date.and_time(time)))
}

fn find_col(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|h| names.contains(&h.trim().to_lowercase().as_str()))
}

/// Interprète le CSV → passages + lignes en erreur. Les heures sans fuseau sont lues dans le fuseau de la société.
async fn parse_import(
    db: &PgPool,
    tenant_id: Uuid,
    content: &str,
) -> AppResult<(Vec<Incoming>, Vec<Outcome>)> {
    let rows = parse_csv(content);
    ensure(rows.len() >= 2, "fichier vide ou sans ligne de données")?;
    ensure(
        rows.len() - 1 <= MAX_IMPORT_ROWS,
        "trop de lignes (2000 maximum par import)",
    )?;
    let header = &rows[0];
    let id_col =
        find_col(header, &["badge", "matricule", "identifiant", "id"]).ok_or_else(|| {
            AppError::Validation("colonne badge/matricule/identifiant absente".into())
        })?;
    let dt_col = find_col(
        header,
        &["datetime", "date_heure", "horodatage", "timestamp", "date"],
    )
    .ok_or_else(|| AppError::Validation("colonne datetime/date_heure/date absente".into()))?;
    let time_col = find_col(header, &["heure", "time"]);
    let dir_col = find_col(header, &["direction", "sens", "type", "etat", "état"]);
    let tz = load_settings(db, tenant_id).await?.timezone;

    let mut pending: Vec<(usize, String, Parsed, Option<Direction>)> = Vec::new();
    let mut errors = Vec::new();
    for (n, r) in rows.iter().enumerate().skip(1) {
        let line = n + 1;
        let cell = |i: usize| r.get(i).map(String::as_str).unwrap_or("");
        let ident = cell(id_col).trim().to_string();
        let when = parse_when(cell(dt_col), time_col.map(cell));
        let dir = dir_col.map_or(Some(None), |c| Direction::parse(cell(c)));
        let problem = if ident.is_empty() || ident.len() > 64 {
            Some("identifiant vide ou trop long")
        } else if when.is_none() {
            Some("date/heure illisible")
        } else if dir.is_none() {
            Some("direction inconnue (attendu : entrée/sortie ou vide)")
        } else {
            None
        };
        match (problem, when, dir) {
            (None, Some(w), Some(d)) => pending.push((line, ident, w, d)),
            (p, _, _) => errors.push(Outcome {
                line: Some(line),
                identifier: ident,
                at: None,
                status: "rejected".into(),
                reason: p.map(|s| format!("ligne {line} : {s}")),
                employee_name: None,
            }),
        }
    }
    // Heures locales → UTC côté base (fuseau IANA, heure d'été comprise).
    let locals: Vec<String> = pending
        .iter()
        .filter_map(|(_, _, w, _)| match w {
            Parsed::Local(n) => Some(n.format("%Y-%m-%d %H:%M:%S").to_string()),
            Parsed::Utc(_) => None,
        })
        .collect();
    let converted: Vec<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT (t::timestamp AT TIME ZONE $2) FROM unnest($1::text[]) WITH ORDINALITY AS u(t, n) ORDER BY n",
    )
    .bind(&locals)
    .bind(&tz)
    .fetch_all(db)
    .await?;
    let mut conv = converted.into_iter();
    let incoming = pending
        .into_iter()
        .map(|(line, identifier, w, direction)| Incoming {
            identifier,
            at: match w {
                Parsed::Utc(u) => u,
                Parsed::Local(_) => conv.next().map(|c| c.0).unwrap_or_else(Utc::now),
            },
            direction,
            device_id: None,
            source: "import",
            line: Some(line),
        })
        .collect();
    Ok((incoming, errors))
}

async fn import_csv(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<ImportBody>,
) -> AppResult<Json<IngestResult>> {
    let t = auth.require(Permission::AttendanceManage)?;
    let (incoming, mut rows) = parse_import(&s.db, t.tenant_id, &b.content).await?;
    let mut tx = s.db.begin().await?;
    rows.extend(process(&mut tx, t.tenant_id, "import CSV", incoming).await?);
    rows.sort_by_key(|r| r.line);
    let counts = Counts::of(&rows);
    if b.dry_run {
        // Simulation : tout est annulé, rien n'est enregistré ni audité.
        tx.rollback().await?;
    } else {
        audit::record(
            &mut *tx,
            Some(t.tenant_id),
            Some(auth.user_id),
            "attendance.import",
            "attendance_import",
            None,
            json!({"counts": counts}),
        )
        .await?;
        tx.commit().await?;
    }
    Ok(Json(IngestResult {
        dry_run: b.dry_run,
        counts,
        rows,
    }))
}

// ---------- Journal des passages ----------

#[derive(Deserialize)]
struct PunchQuery {
    status: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct PunchDto {
    id: Uuid,
    source: String,
    device_name: Option<String>,
    identifier: String,
    punched_at: DateTime<Utc>,
    direction: Option<String>,
    employee_name: Option<String>,
    status: String,
    reason: Option<String>,
}

async fn list_punches(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<PunchQuery>,
) -> AppResult<Json<Vec<PunchDto>>> {
    let t = auth.require(Permission::AttendanceManage)?;
    Ok(Json(
        sqlx::query_as(
            "SELECT r.id, r.source, d.name AS device_name, r.identifier, r.punched_at, r.direction,
                    e.first_name || ' ' || e.last_name AS employee_name, r.status, r.reason
             FROM attendance_raw_punches r
             LEFT JOIN attendance_devices d ON d.id = r.device_id AND d.tenant_id = r.tenant_id
             LEFT JOIN employees e ON e.id = r.employee_id AND e.tenant_id = r.tenant_id
             WHERE r.tenant_id = $1 AND ($2::text IS NULL OR r.status = $2)
             ORDER BY r.punched_at DESC LIMIT 200",
        )
        .bind(t.tenant_id)
        .bind(q.status.filter(|v| !v.is_empty()))
        .fetch_all(&s.db)
        .await?,
    ))
}

/// Retraite les passages « badge inconnu » ou « refusé » (après affectation d'un badge ou correction RH).
async fn reprocess(State(s): State<AppState>, auth: Auth) -> AppResult<Json<IngestResult>> {
    let t = auth.require(Permission::AttendanceManage)?;
    let mut tx = s.db.begin().await?;
    let rows: Vec<RawRow> = sqlx::query_as(
        "DELETE FROM attendance_raw_punches
         WHERE id IN (SELECT id FROM attendance_raw_punches WHERE tenant_id = $1 AND status IN ('unknown_badge','rejected')
                      ORDER BY punched_at LIMIT 2000)
         RETURNING device_id, source, identifier, punched_at, direction",
    )
    .bind(t.tenant_id)
    .fetch_all(&mut *tx)
    .await?;
    let incoming = rows
        .into_iter()
        .map(|(device_id, source, identifier, at, dir)| Incoming {
            identifier,
            at,
            direction: dir.as_deref().and_then(|d| Direction::parse(d).flatten()),
            device_id,
            source: if source == "device" {
                "device"
            } else {
                "import"
            },
            line: None,
        })
        .collect();
    let out = process(&mut tx, t.tenant_id, "retraitement", incoming).await?;
    let counts = Counts::of(&out);
    audit::record(
        &mut *tx,
        Some(t.tenant_id),
        Some(auth.user_id),
        "attendance.reprocess",
        "attendance_import",
        None,
        json!({"counts": counts}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(IngestResult {
        dry_run: false,
        counts,
        rows: out,
    }))
}
