# Contribuer

- Une fonctionnalité = migration SQL + code + tests + doc. Pas de migration destructive sans stratégie de sauvegarde.
- Backend : `cargo fmt`, `cargo clippy -D warnings`, `cargo test`. SQL toujours paramétré (`.bind`), jamais concaténé avec des données utilisateur.
- Toute requête métier filtre par `tenant_id` issu de la session (`Auth`), jamais de la requête. Toute nouvelle table métier porte `tenant_id` et des clés étrangères composites `(id, tenant_id)`.
- Autorisation dans le handler via `auth.require(Permission::…)` ; ajouter un test d'accès négatif.
- Montants : `Decimal` uniquement, jamais de flottants. Règles de paie : versionnées et testées.
- Toute action sensible appelle `audit::record` dans la même transaction.
- Frontend : TypeScript strict, pas de `any`, pas de logique métier dans les composants, états chargement/erreur/vide obligatoires.
- Aucune donnée personnelle réelle dans les fixtures ni les tests.
