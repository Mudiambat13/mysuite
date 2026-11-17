//! Pièces des employés : dépôt (PDF/PNG/JPEG, 5 Mo max, type vérifié sur le contenu), téléchargement audité,
//! suppression (le contenu est effacé, les métadonnées restent pour l'audit) et échéances.

use crate::{
    audit,
    auth::Auth,
    error::{AppError, AppResult, ensure},
    state::AppState,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get},
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use mysuite_shared::permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const MAX_BYTES: usize = 5 * 1024 * 1024;
const KINDS: [&str; 4] = ["identity", "diploma", "certificate", "other"];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/employees/{id}/documents",
            get(list)
                .post(upload)
                .layer(DefaultBodyLimit::max(MAX_BYTES + 1024)),
        )
        .route("/documents/expiring", get(expiring))
        .route("/documents/{id}/download", get(download))
        .route("/documents/{id}", delete(remove))
}

/// Type réel d'après les premiers octets : l'en-tête déclaré par le client n'est jamais cru seul.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else {
        None
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct DocDto {
    id: Uuid,
    employee_id: Uuid,
    kind: String,
    name: String,
    content_type: String,
    size_bytes: i32,
    expires_on: Option<NaiveDate>,
    uploaded_at: DateTime<Utc>,
}

const COLS: &str = "id, employee_id, kind, name, content_type, size_bytes, expires_on, uploaded_at";

async fn employee_in_tenant(s: &AppState, tenant: Uuid, id: Uuid) -> AppResult<()> {
    let f: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM employees WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant)
            .fetch_optional(&s.db)
            .await?;
    f.map(|_| ()).ok_or(AppError::NotFound)
}

async fn list(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<DocDto>>> {
    let t = auth.require(Permission::DocumentsRead)?;
    employee_in_tenant(&s, t.tenant_id, id).await?;
    let rows = sqlx::query_as(&format!(
        "SELECT {COLS} FROM employee_documents WHERE employee_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY uploaded_at DESC"
    ))
    .bind(id)
    .bind(t.tenant_id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
    kind: String,
    expires_on: Option<NaiveDate>,
}

async fn upload(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<DocDto>)> {
    let t = auth.require(Permission::DocumentsManage)?;
    employee_in_tenant(&s, t.tenant_id, id).await?;
    let name = q.name.trim();
    ensure(
        !name.is_empty() && name.chars().count() <= 160,
        "nom requis (160 caractères maximum)",
    )?;
    ensure(KINDS.contains(&q.kind.as_str()), "type de pièce invalide")?;
    ensure(!body.is_empty(), "fichier vide")?;
    ensure(
        body.len() <= MAX_BYTES,
        "fichier trop volumineux (5 Mo maximum)",
    )?;
    let ct = sniff(&body)
        .ok_or_else(|| AppError::Validation("format non accepté (PDF, PNG ou JPEG)".into()))?;
    let sha = hex::encode(Sha256::digest(&body));
    let row: DocDto = sqlx::query_as(&format!(
        "INSERT INTO employee_documents (tenant_id, employee_id, kind, name, content_type, size_bytes, sha256, expires_on, content, uploaded_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING {COLS}"
    ))
    .bind(t.tenant_id)
    .bind(id)
    .bind(&q.kind)
    .bind(name)
    .bind(ct)
    .bind(body.len() as i32)
    .bind(&sha)
    .bind(q.expires_on)
    .bind(body.as_ref())
    .bind(auth.user_id)
    .fetch_one(&s.db)
    .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "document.upload",
        "employee_document",
        Some(row.id.to_string()),
        json!({"employee_id": id, "kind": q.kind, "size": body.len(), "sha256": sha}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn download(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::DocumentsRead)?;
    let (name, ct, emp, content): (String, String, Uuid, Vec<u8>) = sqlx::query_as(
        "SELECT name, content_type, employee_id, content FROM employee_documents
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "document.download",
        "employee_document",
        Some(id.to_string()),
        json!({"employee_id": emp}),
    )
    .await?;
    let ext = match ct.as_str() {
        "application/pdf" => "pdf",
        "image/png" => "png",
        _ => "jpg",
    };
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, ct.parse().unwrap());
    h.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{stem}.{ext}\"")
            .parse()
            .unwrap(),
    );
    h.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    Ok((h, content))
}

async fn remove(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let t = auth.require(Permission::DocumentsManage)?;
    let done: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE employee_documents SET content = NULL, deleted_at = now()
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL RETURNING employee_id",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?;
    let (emp,) = done.ok_or(AppError::NotFound)?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "document.delete",
        "employee_document",
        Some(id.to_string()),
        json!({"employee_id": emp}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ExpQuery {
    days: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ExpiringDto {
    id: Uuid,
    employee_id: Uuid,
    employee_name: String,
    matricule: String,
    kind: String,
    name: String,
    expires_on: NaiveDate,
}

/// Pièces expirées ou expirant dans les N prochains jours (défaut 60, max 365).
async fn expiring(
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<ExpQuery>,
) -> AppResult<Json<Vec<ExpiringDto>>> {
    let t = auth.require(Permission::DocumentsRead)?;
    let limit = Utc::now().date_naive() + Duration::days(q.days.unwrap_or(60).clamp(1, 365));
    let rows = sqlx::query_as(
        "SELECT d.id, d.employee_id, e.first_name || ' ' || e.last_name AS employee_name, e.matricule,
                d.kind, d.name, d.expires_on
         FROM employee_documents d JOIN employees e ON e.id = d.employee_id AND e.tenant_id = d.tenant_id
         WHERE d.tenant_id = $1 AND d.deleted_at IS NULL AND d.expires_on IS NOT NULL AND d.expires_on <= $2
           AND e.status IN ('active','on_leave')
         ORDER BY d.expires_on",
    )
    .bind(t.tenant_id)
    .bind(limit)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing_trusts_content_not_declared_type() {
        assert_eq!(sniff(b"%PDF-1.7 ..."), Some("application/pdf"));
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff(b"<html><script>"), None);
        assert_eq!(sniff(b"MZ\x90\x00"), None);
    }
}
