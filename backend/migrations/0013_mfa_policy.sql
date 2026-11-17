-- Politique de société : MFA obligatoire pour tous les membres.
ALTER TABLE tenants ADD COLUMN mfa_required BOOLEAN NOT NULL DEFAULT FALSE;
