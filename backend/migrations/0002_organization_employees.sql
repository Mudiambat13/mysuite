-- Organisation et dossiers employés. Toutes les tables métier portent tenant_id.
CREATE TABLE sites (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    city       TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name),
    UNIQUE (id, tenant_id)
);

CREATE TABLE departments (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    parent_id  UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name),
    UNIQUE (id, tenant_id),
    FOREIGN KEY (parent_id, tenant_id) REFERENCES departments (id, tenant_id)
);

CREATE TABLE positions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    level      TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, title),
    UNIQUE (id, tenant_id)
);

CREATE TABLE employees (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    matricule     TEXT NOT NULL,
    first_name    TEXT NOT NULL,
    last_name     TEXT NOT NULL,
    email         TEXT,
    phone         TEXT,
    birth_date    DATE,
    hire_date     DATE NOT NULL,
    exit_date     DATE,
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('candidate','active','suspended','on_leave','exited')),
    contract_type TEXT NOT NULL DEFAULT 'cdi' CHECK (contract_type IN ('cdi','cdd','stage','journalier','consultant')),
    site_id       UUID,
    department_id UUID,
    position_id   UUID,
    manager_id    UUID,
    user_id       UUID REFERENCES users(id) ON DELETE SET NULL,
    created_by    UUID,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, matricule),
    UNIQUE (id, tenant_id),
    CHECK (exit_date IS NULL OR exit_date >= hire_date),
    -- Les références restent dans la même société : impossible de rattacher un employé à un site d'une autre société.
    FOREIGN KEY (site_id, tenant_id) REFERENCES sites (id, tenant_id),
    FOREIGN KEY (department_id, tenant_id) REFERENCES departments (id, tenant_id),
    FOREIGN KEY (position_id, tenant_id) REFERENCES positions (id, tenant_id),
    FOREIGN KEY (manager_id, tenant_id) REFERENCES employees (id, tenant_id)
);
CREATE INDEX employees_tenant_status_idx ON employees (tenant_id, status);
CREATE INDEX employees_manager_idx ON employees (manager_id);
CREATE UNIQUE INDEX employees_user_per_tenant_uq ON employees (tenant_id, user_id) WHERE user_id IS NOT NULL;

-- Historique salarial (accès restreint par permission). Jamais modifié : une nouvelle ligne = un nouveau salaire.
CREATE TABLE employee_compensation (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL,
    employee_id    UUID NOT NULL,
    base_salary    NUMERIC(18,2) NOT NULL CHECK (base_salary >= 0),
    currency       TEXT NOT NULL,
    effective_from DATE NOT NULL,
    created_by     UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (employee_id, effective_from),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE
);

-- Éléments récurrents de rémunération (indemnités) et retenues, avec date d'effet.
CREATE TABLE employee_pay_items (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    employee_id UUID NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('allowance','deduction')),
    code        TEXT NOT NULL,
    label       TEXT NOT NULL,
    amount      NUMERIC(18,2) NOT NULL CHECK (amount >= 0),
    active_from DATE NOT NULL,
    active_to   DATE,
    created_by  UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE
);
