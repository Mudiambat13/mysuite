# Ajouter un module

Liste de contrôle, dans l'ordre :

1. **Migration** `backend/migrations/00NN_nom.sql` : tables avec `tenant_id`, clés composites `(id, tenant_id)`, contraintes `CHECK`, index. Pas de modification d'une migration existante.
2. **Permissions** : variantes dans `shared/src/permissions.rs`, attribution par rôle, test unitaire.
3. **Module Rust** `crates/app/src/<module>.rs` : `pub fn routes() -> Router<AppState>`, handlers avec `Auth`, `auth.require(...)`, requêtes **paramétrées** filtrées par `auth.tenant_id`, transaction + `audit::record` pour les actions sensibles. Enregistrer dans `lib.rs` (`.merge(module::routes())`).
4. **Tests d'intégration** : nominal, 403, 404 inter-société, validation, audit.
5. **Frontend** : types dans `lib/types.ts`, page dans `features/<module>/`, route dans `app/App.tsx`, entrée de navigation dans `Layout.tsx` (visible selon la permission), textes dans `i18n`. États chargement/erreur/vide.
6. **Documentation** : page guide, règles métier, tableau d'API, `CHANGELOG.md`, `ROADMAP.md`, captures si utile.

## Modèles à suivre

- Ressource simple : `organization.rs`.
- Workflow avec décision : `leave.rs`.
- Calcul figé et versionné : `payroll.rs` + `crates/payroll`.
- Ingestion externe authentifiée par clé : `timeclock.rs`.
- Authentification forte : `auth/mfa.rs`.
