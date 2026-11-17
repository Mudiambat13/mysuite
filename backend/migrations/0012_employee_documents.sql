-- Pièces des employés (PDF, PNG, JPEG), stockées en base : sauvegardées avec elles, isolées par société.
CREATE TABLE employee_documents (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL,
    employee_id  UUID NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('identity','diploma','certificate','other')),
    name         TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 160),
    content_type TEXT NOT NULL CHECK (content_type IN ('application/pdf','image/png','image/jpeg')),
    size_bytes   INTEGER NOT NULL CHECK (size_bytes BETWEEN 1 AND 5242880),
    sha256       TEXT NOT NULL,
    expires_on   DATE,
    content      BYTEA,                      -- NULL après suppression (métadonnées conservées pour l'audit)
    uploaded_by  UUID,
    uploaded_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at   TIMESTAMPTZ,
    UNIQUE (id, tenant_id),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE,
    CHECK ((deleted_at IS NULL) = (content IS NOT NULL))
);
CREATE INDEX employee_documents_emp_idx ON employee_documents (tenant_id, employee_id) WHERE deleted_at IS NULL;
CREATE INDEX employee_documents_exp_idx ON employee_documents (tenant_id, expires_on) WHERE deleted_at IS NULL AND expires_on IS NOT NULL;
