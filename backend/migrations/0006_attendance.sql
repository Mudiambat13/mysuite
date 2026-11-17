-- Pointage : paramètres par société (aucune règle légale codée en dur) et entrées de présence.
CREATE TABLE attendance_settings (
    tenant_id     UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    timezone      TEXT NOT NULL DEFAULT 'UTC',
    work_start    TIME NOT NULL DEFAULT '08:00',
    grace_minutes INT  NOT NULL DEFAULT 10  CHECK (grace_minutes BETWEEN 0 AND 240),
    daily_minutes INT  NOT NULL DEFAULT 480 CHECK (daily_minutes BETWEEN 60 AND 1440),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE attendance_entries (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL,
    employee_id  UUID NOT NULL,
    clock_in     TIMESTAMPTZ NOT NULL,
    clock_out    TIMESTAMPTZ,
    source       TEXT NOT NULL DEFAULT 'self' CHECK (source IN ('self','manual')),
    note         TEXT,
    created_by   UUID REFERENCES users(id),
    corrected_by UUID REFERENCES users(id),
    corrected_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (clock_out IS NULL OR clock_out > clock_in),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE
);
-- Au plus un pointage ouvert par employé.
CREATE UNIQUE INDEX attendance_one_open_idx ON attendance_entries (employee_id) WHERE clock_out IS NULL;
CREATE INDEX attendance_employee_in_idx ON attendance_entries (tenant_id, employee_id, clock_in);
