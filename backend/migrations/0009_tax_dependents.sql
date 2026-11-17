-- Personnes fiscalement à charge, avec historique daté (comme la rémunération).
-- Utilisé uniquement si le jeu de règles de paie prévoit une réduction pour personnes à charge.
CREATE TABLE employee_dependents (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL,
    employee_id    UUID NOT NULL,
    dependents     SMALLINT NOT NULL CHECK (dependents BETWEEN 0 AND 30),
    effective_from DATE NOT NULL,
    created_by     UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE,
    UNIQUE (employee_id, effective_from)
);
CREATE INDEX employee_dependents_lookup ON employee_dependents (tenant_id, employee_id, effective_from DESC);
