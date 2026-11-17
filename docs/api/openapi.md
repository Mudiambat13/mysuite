# API

Base : `/api/v1`. Erreurs : `{ "code", "message", "correlation_id" }`. Toute requête modifiante exige l'en-tête `X-Requested-With: mysuite`. Pagination `?page=&limit=` (limite max 100).
OpenAPI généré (utoipa) : **non livré** (voir `ROADMAP.md`). Routes : `auth/*`, `platform/tenants`, `users`, `org/{sites,departments,positions}`, `employees` (+ `/me`, `/export.csv`, `/{id}/compensation`, `/{id}/pay-items`), `leave/{types,balances,requests}`, `payroll/{rule-sets,periods,runs}`, `payslips/{me,id,id/pdf}`, `contracts` (+ `/expiring`, `/{id}/terminate`, `/{id}/amendments`), `reports/{headcount,movements,absences,payroll-cost}` (+ `.csv` pour les trois derniers), `dashboard`, `audit`. Sondes : `/health`, `/health/ready`.


## Sécurité du compte

| Méthode et chemin | Rôle |
|---|---|
| `POST /auth/login` | Retourne le profil, ou `{mfa_required: true, mfa_token}` si le MFA est actif (aucune session créée) |
| `POST /auth/login/mfa` `{mfa_token, code}` | 2ᵉ étape : code TOTP ou code de secours ; retourne le profil + cookie de session |
| `GET /auth/mfa` | `{available, enabled, recovery_codes_remaining}` |
| `POST /auth/mfa/setup` `{password}` | Génère le secret (`secret`, `otpauth_uri`) — 422 si `MFA_ENCRYPTION_KEY` absente |
| `POST /auth/mfa/enable` `{code}` | Active ; retourne `recovery_codes` (une seule fois) |
| `POST /auth/mfa/disable` `{password, code}` | Désactive (204) |
| `POST /auth/mfa/recovery-codes` `{password, code}` | Régénère les codes de secours |
| `POST /users/{id}/mfa-reset` | `UsersManage` : retire le MFA d'un membre de la société (204) |
| `POST /users/{id}/password-reset` | `UsersManage` : `{reset_token, path, expires_at}` (affiché une fois) |
| `POST /auth/reset-password` `{token, new_password}` | Public : 204, ou 422 « lien invalide ou expiré » |

## Pointage

| Méthode | Chemin | Permission | Remarques |
|---|---|---|---|
| GET/PUT | `/attendance/settings` | tout membre / `attendance_manage` | fuseau IANA, début, tolérance, durée standard |
| POST | `/attendance/clock-in`, `/attendance/clock-out` | `attendance_clock_own` | heure du serveur ; 409 si déjà ouvert / aucun ouvert |
| GET | `/attendance/me` | `attendance_clock_own` | pointage ouvert + journée |
| GET | `/attendance/entries?scope=mine\|team\|all&from&to&employee_id` | selon scope | manager : subordonnés directs ; `all` interdit au manager |
| POST | `/attendance/entries` | `attendance_manage` | motif obligatoire, pas de chevauchement ni de futur |
| PUT | `/attendance/entries/{id}` | `attendance_manage` | audit avant/après ; jamais sur son propre pointage |
| GET | `/attendance/summary(.csv)?from&to&scope=team\|all` | `attendance_read` | export audité (`attendance.export`) |

### Badgeuses

| Méthode | Chemin | Authentification | Remarques |
|---|---|---|---|
| POST | `/device/punches` | `Bearer <clé d'appareil>` | `{punches:[{badge, at (RFC 3339 + décalage), direction?}]}` ; résultat par passage ; sans CSRF |
| GET/POST | `/attendance/devices` | `attendance_manage` | la clé n'est renvoyée qu'à la création |
| POST | `/attendance/devices/{id}/revoke` | `attendance_manage` | |
| GET/POST/DELETE | `/attendance/badges[/{id}]` | `attendance_manage` | code unique par société |
| POST | `/attendance/import` | `attendance_manage` | `{content, dry_run}` ; simulation = rien d'écrit ni audité |
| GET | `/attendance/punches?status=` | `attendance_manage` | journal brut (200 derniers) |
| POST | `/attendance/punches/reprocess` | `attendance_manage` | rejoue inconnus et refusés |
