-- Modèles de contrat fournis par la société (MySuite ne fournit aucune clause juridique).
CREATE TABLE contract_templates (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 50000),
    created_by  UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, tenant_id)
);
CREATE INDEX contract_templates_tenant_idx ON contract_templates (tenant_id, created_at DESC);
