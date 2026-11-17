-- Grille de salaires minimums (ex. SMIG), fournie et importée par la société ; classe par employé (historique daté).
CREATE TABLE wage_grids (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES tenants (id),
    name           TEXT NOT NULL,
    currency       TEXT NOT NULL,
    effective_from DATE NOT NULL,
    source         TEXT NOT NULL,
    created_by     UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, tenant_id)
);
CREATE INDEX wage_grids_lookup ON wage_grids (tenant_id, effective_from DESC);

CREATE TABLE wage_grid_rows (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    grid_id     UUID NOT NULL,
    class_no    INTEGER NOT NULL CHECK (class_no > 0),
    category    TEXT NOT NULL,
    reference   TEXT NOT NULL DEFAULT '',
    coefficient NUMERIC(10,2),
    daily       NUMERIC(14,2) NOT NULL CHECK (daily > 0),
    monthly     NUMERIC(14,2) NOT NULL CHECK (monthly > 0),
    FOREIGN KEY (grid_id, tenant_id) REFERENCES wage_grids (id, tenant_id) ON DELETE CASCADE,
    UNIQUE (grid_id, class_no)
);

CREATE TABLE employee_wage_class (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL,
    employee_id    UUID NOT NULL,
    class_no       INTEGER NOT NULL CHECK (class_no > 0),
    effective_from DATE NOT NULL,
    created_by     UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE,
    UNIQUE (employee_id, effective_from)
);
CREATE INDEX employee_wage_class_lookup ON employee_wage_class (tenant_id, employee_id, effective_from DESC);
