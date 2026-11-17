-- Badgeuses : appareils (clé API hachée), correspondance badge → employé, journal brut des passages.
ALTER TABLE attendance_entries DROP CONSTRAINT attendance_entries_source_check;
ALTER TABLE attendance_entries ADD CONSTRAINT attendance_entries_source_check
    CHECK (source IN ('self','manual','device','import'));

CREATE TABLE attendance_devices (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    api_key_hash TEXT NOT NULL UNIQUE,
    key_prefix   TEXT NOT NULL,
    active       BOOLEAN NOT NULL DEFAULT TRUE,
    last_seen_at TIMESTAMPTZ,
    created_by   UUID REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (id, tenant_id)
);

CREATE TABLE employee_badges (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    employee_id UUID NOT NULL,
    badge_code  TEXT NOT NULL CHECK (length(badge_code) BETWEEN 1 AND 64),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, badge_code),
    FOREIGN KEY (employee_id, tenant_id) REFERENCES employees (id, tenant_id) ON DELETE CASCADE
);
CREATE INDEX employee_badges_employee_idx ON employee_badges (employee_id);

CREATE TABLE attendance_raw_punches (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    device_id   UUID,
    source      TEXT NOT NULL CHECK (source IN ('device','import')),
    identifier  TEXT NOT NULL,
    punched_at  TIMESTAMPTZ NOT NULL,
    direction   TEXT CHECK (direction IN ('in','out')),
    employee_id UUID,
    status      TEXT NOT NULL CHECK (status IN ('in','out','duplicate','unknown_badge','rejected')),
    reason      TEXT,
    entry_id    UUID REFERENCES attendance_entries(id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Idempotence : une badgeuse qui renvoie le même passage ne crée rien de plus.
    UNIQUE (tenant_id, identifier, punched_at),
    FOREIGN KEY (device_id, tenant_id) REFERENCES attendance_devices (id, tenant_id)
);
CREATE INDEX attendance_raw_status_idx ON attendance_raw_punches (tenant_id, status, punched_at DESC);
