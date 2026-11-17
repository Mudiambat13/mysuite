-- Fondations : sociétés (tenants), utilisateurs, appartenances, sessions, audit.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE tenants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
    slug        TEXT NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9-]{2,50}$'),
    country     TEXT NOT NULL DEFAULT 'CD',
    currency    TEXT NOT NULL DEFAULT 'CDF',
    locale      TEXT NOT NULL DEFAULT 'fr',
    timezone    TEXT NOT NULL DEFAULT 'Africa/Lubumbashi',
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email            TEXT NOT NULL CHECK (email = lower(email) AND position('@' in email) > 1),
    password_hash    TEXT NOT NULL,
    full_name        TEXT NOT NULL,
    is_platform_admin BOOLEAN NOT NULL DEFAULT FALSE,
    status           TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    failed_attempts  INT NOT NULL DEFAULT 0,
    locked_until     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_uq ON users (email);

-- Un utilisateur peut appartenir à plusieurs sociétés avec des rôles différents.
CREATE TABLE memberships (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    role       TEXT NOT NULL CHECK (role IN ('hr_admin','hr_manager','payroll_manager','manager','employee','auditor')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, tenant_id)
);
CREATE INDEX memberships_tenant_idx ON memberships (tenant_id);

-- Sessions côté serveur : jeton opaque, seul son hash SHA-256 est stocké.
CREATE TABLE sessions (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash       TEXT NOT NULL UNIQUE,
    active_tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    user_agent       TEXT,
    ip               TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at       TIMESTAMPTZ NOT NULL,
    revoked_at       TIMESTAMPTZ
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE login_events (
    id         BIGSERIAL PRIMARY KEY,
    email      TEXT NOT NULL,
    user_id    UUID,
    success    BOOLEAN NOT NULL,
    ip         TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Journal d'audit append-only (UPDATE/DELETE interdits par trigger).
CREATE TABLE audit_logs (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   UUID,
    actor_id    UUID,
    action      TEXT NOT NULL,
    entity      TEXT NOT NULL,
    entity_id   TEXT,
    details     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_logs_tenant_idx ON audit_logs (tenant_id, created_at DESC);

CREATE FUNCTION forbid_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'la table % est en ajout seul (append-only)', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_logs_no_update BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
