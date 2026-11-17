-- Contrats et avenants. Un avenant ne modifie jamais le contrat : il s'ajoute à l'historique.
CREATE TABLE contracts (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL,
    employee_id      UUID NOT NULL,
    contract_type    TEXT NOT NULL CHECK (contract_type IN ('cdi','cdd','stage','journalier','consultant')),
    start_date       DATE NOT NULL,
    end_date         DATE,
    probation_end    DATE,
    job_title        TEXT,
    status           TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','terminated','expired')),
    notes            TEXT,
    created_by       UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, tenant_id),
    CHECK (end_date IS NULL OR end_date >= start_date),
    CHECK (probation_end IS NULL OR probation_end >= start_date),
    -- Un CDD doit avoir une date de fin.
    CHECK (contract_type <> 'cdd' OR end_date IS NOT NULL),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE
);
CREATE INDEX contracts_tenant_end_idx ON contracts (tenant_id, end_date) WHERE status = 'active';
-- Un seul contrat actif par employé.
CREATE UNIQUE INDEX contracts_one_active_per_employee ON contracts (employee_id) WHERE status = 'active';

CREATE TABLE contract_amendments (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL,
    contract_id    UUID NOT NULL,
    effective_date DATE NOT NULL,
    description    TEXT NOT NULL CHECK (length(trim(description)) > 0),
    new_end_date   DATE,
    created_by     UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (contract_id, tenant_id) REFERENCES contracts (id, tenant_id) ON DELETE CASCADE
);
