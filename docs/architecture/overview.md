# Architecture

Monolithe modulaire. Workspace Cargo :

- `crates/shared` : rôles, permissions (matrice RBAC), pagination. Sans E/S.
- `crates/payroll` : moteur de calcul **pur** (aucune E/S, aucun accès base). Entrées + `RuleSet` ⇒ résultat détaillé, déterministe.
- `crates/app` : HTTP (Axum), SQLx, un module par domaine (`auth`, `tenants`, `organization`, `employees`, `leave`, `payroll`, `reports`, `audit`).

Écart assumé avec la spécification : un seul crate applicatif découpé en modules (plutôt qu'un crate par domaine) pour garder une base simple ; le découpage en crates reste possible puisque le moteur de paie est déjà isolé. Les DTO sont distincts des requêtes SQL ; pas de couche `domain/application/infrastructure` formelle à ce stade (voir ADR 0001).

Frontend : React 19, Vite, TanStack Query (cache serveur), React Hook Form + Zod, Tailwind, Recharts, i18next. Une feature = un dossier sous `src/features`.

Flux : navigateur → Caddy (`/api` → API, reste → fichiers statiques) → Axum → PostgreSQL. Même origine ⇒ cookie `SameSite=Strict`.
