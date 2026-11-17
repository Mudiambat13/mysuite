# Tests et qualité

## Backend

```bash
cd backend
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
DATABASE_URL=postgres://mysuite:mysuite@localhost:5432/mysuite cargo test --workspace
```

- Les tests d'intégration (`crates/app/tests/api.rs`) utilisent `#[sqlx::test]` : **une base éphémère par test**, migrations appliquées. `DATABASE_URL` doit être défini et l'utilisateur doit pouvoir créer des bases.
- Tests unitaires : moteur de paie, matrice de permissions, TOTP (vecteurs RFC 6238), importation CSV.
- Chaque fonctionnalité doit couvrir : cas nominal, **accès refusé**, **isolation entre sociétés**, audit.

## Frontend

```bash
cd frontend
npm run lint && npm run typecheck && npm test && npm run build
```

Vitest + Testing Library pour les utilitaires et composants.

## Parcours navigateur et captures

Scripts Playwright dans `tools/` (`capture-*.cjs`), exécutés contre une API et un Vite locaux avec les données de démonstration. Ils vérifient les parcours (pointage, badgeuses, MFA…) et régénèrent `docs/screenshots/`.

```bash
cd frontend
NODE_PATH=$(npm root -g):$(pwd)/node_modules node ../tools/capture-security.cjs
```

## CI

`.github/workflows/ci.yml` : fmt, clippy, tests (PostgreSQL de service), `cargo audit`, lint/typecheck/tests/build frontend, `npm audit`.
