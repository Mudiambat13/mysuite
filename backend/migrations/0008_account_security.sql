-- MFA TOTP (secret chiffré au repos), codes de secours (hachés), défis de connexion, réinitialisations de mot de passe.
CREATE TABLE user_mfa (
    user_id        UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_enc     BYTEA NOT NULL,
    enabled        BOOLEAN NOT NULL DEFAULT FALSE,
    -- Dernier pas de 30 s accepté : un code ne sert qu'une fois (anti-rejeu).
    last_used_step BIGINT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    enabled_at     TIMESTAMPTZ
);

CREATE TABLE mfa_recovery_codes (
    id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    used_at   TIMESTAMPTZ,
    UNIQUE (user_id, code_hash)
);

-- Étape intermédiaire de la connexion : mot de passe validé, second facteur attendu.
CREATE TABLE mfa_challenges (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    attempts   INT NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Réinitialisation émise par un administrateur (aucun courriel n'est envoyé : lien remis hors bande).
CREATE TABLE password_resets (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  UUID REFERENCES tenants(id) ON DELETE SET NULL,
    issued_by  UUID REFERENCES users(id) ON DELETE SET NULL,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX password_resets_user_idx ON password_resets (user_id);
