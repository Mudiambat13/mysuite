CREATE TABLE leave_types (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    code             TEXT NOT NULL,
    name             TEXT NOT NULL,
    -- Droit annuel paramétrable : aucune loi nationale codée en dur.
    days_per_year    NUMERIC(6,2) NOT NULL DEFAULT 0 CHECK (days_per_year >= 0),
    requires_balance BOOLEAN NOT NULL DEFAULT TRUE,
    active           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, code),
    UNIQUE (id, tenant_id)
);

CREATE TABLE leave_balances (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL,
    employee_id   UUID NOT NULL,
    leave_type_id UUID NOT NULL,
    year          INT NOT NULL,
    entitled      NUMERIC(6,2) NOT NULL CHECK (entitled >= 0),
    UNIQUE (employee_id, leave_type_id, year),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY (leave_type_id, tenant_id) REFERENCES leave_types (id, tenant_id)
);

CREATE TABLE leave_requests (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID NOT NULL,
    employee_id      UUID NOT NULL,
    leave_type_id    UUID NOT NULL,
    start_date       DATE NOT NULL,
    end_date         DATE NOT NULL,
    days             NUMERIC(6,2) NOT NULL CHECK (days > 0),
    reason           TEXT,
    status           TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','rejected','cancelled')),
    decided_by       UUID,
    decided_at       TIMESTAMPTZ,
    decision_comment TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (end_date >= start_date),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY (leave_type_id, tenant_id) REFERENCES leave_types (id, tenant_id)
);
CREATE INDEX leave_requests_tenant_status_idx ON leave_requests (tenant_id, status);
CREATE INDEX leave_requests_employee_idx ON leave_requests (employee_id, start_date);
