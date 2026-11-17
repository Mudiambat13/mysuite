# Environnement de développement

Prérequis : Rust stable (édition 2024), Node 22, PostgreSQL 16.

```bash
createdb mysuite
export MFA_ENCRYPTION_KEY=$(openssl rand -hex 32)
export DATABASE_URL=postgres://mysuite:mysuite@localhost:5432/mysuite COOKIE_SECURE=false BIND_ADDR=127.0.0.1:8080

# API
cd backend
cargo run -- migrate
MYSUITE_ADMIN_PASSWORD='Admin-Passw0rd-2026' cargo run -- create-admin admin@exemple.test "Admin"
cargo run -- seed-demo       # données fictives
cargo run                    # démarre l'API

# Frontend (autre terminal)
cd frontend
npm ci
npm run dev                  # http://localhost:5173, /api proxifié vers 8080
```

## Structure

```text
backend/
  crates/shared/    rôles, permissions, pagination (sans E/S)
  crates/payroll/   moteur de paie pur
  crates/app/       HTTP (Axum) + SQLx, un module par domaine
  migrations/       SQL versionné
frontend/src/
  features/         un dossier par fonctionnalité
  components/       UI partagée (ui.tsx, charts.tsx, Layout.tsx)
  lib/              api, types, format
docs/               ce site (MkDocs)
tools/              scripts Playwright de captures et de parcours
```
