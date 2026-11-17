use serde_json::Value;
use sqlx::{Executor, Postgres};
use uuid::Uuid;

/// Ajoute une entrée d'audit (append-only). Utiliser la même transaction que l'action auditée.
pub async fn record<'e, E>(
    exec: E,
    tenant_id: Option<Uuid>,
    actor_id: Option<Uuid>,
    action: &str,
    entity: &str,
    entity_id: Option<String>,
    details: Value,
) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "INSERT INTO audit_logs (tenant_id, actor_id, action, entity, entity_id, details) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(tenant_id)
    .bind(actor_id)
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(details)
    .execute(exec)
    .await?;
    Ok(())
}
