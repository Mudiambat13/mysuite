//! Modèles de contrat de la société et génération PDF. MySuite ne fournit aucune clause : le texte vient de la société
//! (à faire valider par un juriste) ; seuls des champs de fusion sont remplacés.

use crate::{
    audit,
    auth::Auth,
    error::{AppError, AppResult, ensure},
    pdf::{Document, Font, text_width},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use mysuite_shared::permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/contract-templates",
            get(list_templates).post(create_template),
        )
        .route("/contract-templates/{id}", get(get_template))
        .route("/contracts/{id}/document.pdf", get(document))
}

pub const FIELDS: [&str; 8] = [
    "company",
    "employee_name",
    "matricule",
    "job_title",
    "contract_type",
    "start_date",
    "end_date",
    "probation_end",
];

/// Champs `{{nom}}` présents dans le texte. Erreur si un champ est inconnu ou mal fermé.
pub fn fields_in(body: &str) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else {
            return Err("« {{ » sans « }} » correspondant".into());
        };
        let name = after[..j].trim();
        if !FIELDS.contains(&name) {
            return Err(format!(
                "champ inconnu « {name} » (disponibles : {})",
                FIELDS.join(", ")
            ));
        }
        found.push(name.to_string());
        rest = &after[j + 2..];
    }
    Ok(found)
}

pub fn merge(body: &str, values: &[(&str, String)]) -> String {
    let mut out = body.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
        out = out.replace(&format!("{{{{ {k} }}}}"), v);
    }
    out
}

fn wrap(text: &str, size: f32, width: f32, font: Font) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.lines() {
        if para.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut cur = String::new();
        for word in para.split_whitespace() {
            let cand = if cur.is_empty() {
                word.to_string()
            } else {
                format!("{cur} {word}")
            };
            if text_width(&cand, size, font) > width && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = cand;
            }
        }
        lines.push(cur);
    }
    lines
}

pub fn render(title: &str, text: &str) -> Vec<u8> {
    let (_, h) = Document::page_size();
    let (left, width, size, lead) = (56.0, 483.0, 10.5, 15.0);
    let mut doc = Document::new();
    let mut y = h - 70.0;
    for l in wrap(title, 16.0, width, Font::Bold) {
        doc.text(left, y, 16.0, Font::Bold, &l);
        y -= 24.0;
    }
    y -= 10.0;
    for l in wrap(text, size, width, Font::Regular) {
        if y < 60.0 {
            doc.new_page();
            y = h - 60.0;
        }
        if !l.is_empty() {
            doc.text(left, y, size, Font::Regular, &l);
        }
        y -= lead;
    }
    doc.finish()
}

#[derive(Serialize, sqlx::FromRow)]
struct TemplateDto {
    id: Uuid,
    name: String,
    body: String,
    created_at: DateTime<Utc>,
}

async fn list_templates(
    State(s): State<AppState>,
    auth: Auth,
) -> AppResult<Json<Vec<TemplateDto>>> {
    let t = auth.require(Permission::ContractsRead)?;
    let rows = sqlx::query_as("SELECT id, name, body, created_at FROM contract_templates WHERE tenant_id = $1 ORDER BY created_at DESC")
        .bind(t.tenant_id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

async fn get_template(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TemplateDto>> {
    let t = auth.require(Permission::ContractsRead)?;
    sqlx::query_as("SELECT id, name, body, created_at FROM contract_templates WHERE id = $1 AND tenant_id = $2")
        .bind(id).bind(t.tenant_id).fetch_optional(&s.db).await?
        .map(Json).ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
struct NewTemplate {
    name: String,
    body: String,
}

async fn create_template(
    State(s): State<AppState>,
    auth: Auth,
    Json(b): Json<NewTemplate>,
) -> AppResult<(StatusCode, Json<TemplateDto>)> {
    let t = auth.require(Permission::ContractsManage)?;
    ensure(
        !b.name.trim().is_empty() && b.name.len() <= 120,
        "nom requis (120 caractères maximum)",
    )?;
    ensure(
        !b.body.trim().is_empty() && b.body.len() <= 50_000,
        "texte requis (50 000 caractères maximum)",
    )?;
    fields_in(&b.body).map_err(AppError::Validation)?;
    let row: TemplateDto = sqlx::query_as(
        "INSERT INTO contract_templates (tenant_id, name, body, created_by) VALUES ($1,$2,$3,$4)
         RETURNING id, name, body, created_at",
    )
    .bind(t.tenant_id)
    .bind(b.name.trim())
    .bind(&b.body)
    .bind(auth.user_id)
    .fetch_one(&s.db)
    .await?;
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "contract_template.create",
        "contract_template",
        Some(row.id.to_string()),
        json!({"name": row.name}),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize)]
struct DocQuery {
    template_id: Uuid,
}

#[derive(sqlx::FromRow)]
struct Facts {
    company: String,
    employee_name: String,
    matricule: String,
    job_title: Option<String>,
    contract_type: String,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    probation_end: Option<NaiveDate>,
}

fn fr_date(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

async fn document(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<DocQuery>,
) -> AppResult<impl IntoResponse> {
    let t = auth.require(Permission::ContractsRead)?;
    let tpl: (String, String) = sqlx::query_as(
        "SELECT name, body FROM contract_templates WHERE id = $1 AND tenant_id = $2",
    )
    .bind(q.template_id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let f: Facts = sqlx::query_as(
        "SELECT tn.name AS company, e.first_name || ' ' || e.last_name AS employee_name, e.matricule,
                c.job_title, c.contract_type, c.start_date, c.end_date, c.probation_end
         FROM contracts c
         JOIN employees e ON e.id = c.employee_id AND e.tenant_id = c.tenant_id
         JOIN tenants tn ON tn.id = c.tenant_id
         WHERE c.id = $1 AND c.tenant_id = $2",
    )
    .bind(id)
    .bind(t.tenant_id)
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let na = "non applicable".to_string();
    let values = [
        ("company", f.company),
        ("employee_name", f.employee_name),
        ("matricule", f.matricule.clone()),
        ("job_title", f.job_title.unwrap_or_else(|| na.clone())),
        ("contract_type", f.contract_type.to_uppercase()),
        ("start_date", fr_date(f.start_date)),
        (
            "end_date",
            f.end_date.map(fr_date).unwrap_or_else(|| na.clone()),
        ),
        ("probation_end", f.probation_end.map(fr_date).unwrap_or(na)),
    ];
    let bytes = render(&tpl.0, &merge(&tpl.1, &values));
    audit::record(
        &s.db,
        Some(t.tenant_id),
        Some(auth.user_id),
        "contract.document",
        "contract",
        Some(id.to_string()),
        json!({"template_id": q.template_id}),
    )
    .await?;
    let safe: String = f
        .matricule
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
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
                format!("attachment; filename=\"contrat-{safe}.pdf\""),
            ),
        ],
        bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_validated() {
        assert_eq!(
            fields_in("Entre {{company}} et {{ employee_name }}.").unwrap(),
            vec!["company", "employee_name"]
        );
        assert!(fields_in("{{salaire}}").unwrap_err().contains("inconnu"));
        assert!(fields_in("{{company").is_err());
    }

    #[test]
    fn merge_replaces_both_spellings() {
        let m = merge(
            "{{company}} / {{ company }}",
            &[("company", "ACME".to_string())],
        );
        assert_eq!(m, "ACME / ACME");
    }

    #[test]
    fn long_text_paginates_into_a_valid_pdf() {
        let text = "Article. ".repeat(2000);
        let pdf = render("Contrat", &text);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.matches("/Type /Page ").count() >= 2);
    }
}
