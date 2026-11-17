# ADR 0001 — Monolithe modulaire, requêtes SQLx à l'exécution

**Statut** : accepté. **Contexte** : produit RH complet, petite équipe. **Décision** : un binaire, modules par domaine, moteur de paie isolé dans sa crate. Requêtes `sqlx::query_as` paramétrées (pas de macros `query!`) pour éviter une base de compilation ; la sûreté repose sur les tests d'intégration sur PostgreSQL réel. **Conséquences** : build simple ; à réévaluer (`query!` + `sqlx prepare`) quand le schéma se stabilise.
