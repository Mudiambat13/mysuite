-- Paie : règles versionnées, périodes, calculs, lignes immuables après clôture.
CREATE TABLE payroll_rule_sets (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    country        TEXT NOT NULL,
    name           TEXT NOT NULL,
    version        INT NOT NULL,
    effective_from DATE NOT NULL,
    params         JSONB NOT NULL,
    source         TEXT,            -- référence aux textes officiels
    verified_on    DATE,            -- date de vérification par un expert
    -- 'draft' = non validé : la paie reste en simulation. 'approved' = validé par un humain habilité.
    status         TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','approved','retired')),
    is_demo        BOOLEAN NOT NULL DEFAULT FALSE,
    approved_by    UUID,
    approved_at    TIMESTAMPTZ,
    created_by     UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, country, name, version),
    UNIQUE (id, tenant_id),
    CHECK (status <> 'approved' OR (approved_by IS NOT NULL AND approved_at IS NOT NULL AND verified_on IS NOT NULL AND source IS NOT NULL AND is_demo = FALSE))
);

CREATE TABLE payroll_periods (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    year       INT NOT NULL CHECK (year BETWEEN 2000 AND 2100),
    month      INT NOT NULL CHECK (month BETWEEN 1 AND 12),
    status     TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, year, month),
    UNIQUE (id, tenant_id)
);

CREATE TABLE payroll_runs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL,
    period_id     UUID NOT NULL,
    rule_set_id   UUID NOT NULL,
    -- Simulation tant que le jeu de règles n'est pas 'approved'.
    simulation    BOOLEAN NOT NULL,
    status        TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','validated','closed')),
    created_by    UUID,
    validated_by  UUID,
    validated_at  TIMESTAMPTZ,
    closed_by     UUID,
    closed_at     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, tenant_id),
    CHECK (simulation = FALSE OR status = 'draft'),
    FOREIGN KEY (period_id, tenant_id) REFERENCES payroll_periods (id, tenant_id),
    FOREIGN KEY (rule_set_id, tenant_id) REFERENCES payroll_rule_sets (id, tenant_id)
);
-- Une seule paie active (non clôturée) par période.
CREATE UNIQUE INDEX payroll_runs_one_open_per_period ON payroll_runs (period_id) WHERE status <> 'closed';

CREATE TABLE payroll_items (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL,
    run_id        UUID NOT NULL,
    employee_id   UUID NOT NULL,
    gross         NUMERIC(18,2) NOT NULL,
    net           NUMERIC(18,2) NOT NULL,
    employer_cost NUMERIC(18,2) NOT NULL,
    result        JSONB NOT NULL,   -- lignes détaillées et totaux
    input         JSONB NOT NULL,   -- données d'entrée figées pour reproductibilité
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, employee_id),
    FOREIGN KEY (run_id, tenant_id) REFERENCES payroll_runs (id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id)
);
CREATE INDEX payroll_items_employee_idx ON payroll_items (employee_id);

-- Une paie validée ou clôturée est figée : ses lignes ne peuvent plus être modifiées ni supprimées.
CREATE FUNCTION protect_finalized_payroll() RETURNS trigger AS $$
DECLARE run_status TEXT;
BEGIN
    SELECT status INTO run_status FROM payroll_runs WHERE id = COALESCE(OLD.run_id, NEW.run_id);
    IF run_status IN ('validated','closed') THEN
        RAISE EXCEPTION 'paie % : lignes figées, utiliser une régularisation', run_status;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER payroll_items_frozen BEFORE UPDATE OR DELETE ON payroll_items
    FOR EACH ROW EXECUTE FUNCTION protect_finalized_payroll();

CREATE FUNCTION protect_closed_run() RETURNS trigger AS $$
BEGIN
    IF OLD.status = 'closed' THEN
        RAISE EXCEPTION 'paie clôturée : modification interdite';
    END IF;
    IF OLD.status = 'validated' AND NEW.status NOT IN ('validated','closed') THEN
        RAISE EXCEPTION 'paie validée : retour en brouillon interdit';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER payroll_runs_guard BEFORE UPDATE ON payroll_runs
    FOR EACH ROW EXECUTE FUNCTION protect_closed_run();
