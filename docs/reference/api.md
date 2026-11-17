# Référence de l'API

Base : `/api/v1`. Authentification : cookie de session `mysuite_session` (`HttpOnly`, `SameSite=Strict`) sauf API appareil (`Bearer`).

## Conventions

| Sujet | Règle |
|---|---|
| CSRF | Toute requête modifiante exige `X-Requested-With: mysuite` |
| Erreurs | `{ "code", "message", "correlation_id" }` |
| Pagination | `?page=&limit=` (max 100) |
| Dates | RFC 3339 ; montants en chaînes décimales |
| Codes | 401 non authentifié · 403 permission · 404 hors périmètre/inexistant · 409 conflit d'état · 422 validation · 429 verrouillage · 503 indisponible |

## Routes par domaine

| Domaine | Routes |
|---|---|
| Santé | `GET /health`, `GET /health/ready` |
| Authentification | `POST /auth/login`, `/auth/login/mfa`, `/auth/logout`, `/auth/switch-tenant`, `/auth/change-password`, `/auth/reset-password` ; `GET /auth/me`, `/auth/sessions` ; `DELETE /auth/sessions/{id}` |
| MFA | `GET /auth/mfa` ; `POST /auth/mfa/setup`, `/enable`, `/disable`, `/recovery-codes` |
| Plateforme | `GET/POST /platform/tenants` |
| Utilisateurs | `GET/POST /users` ; `PUT /users/{id}/role` ; `POST /users/{id}/mfa-reset`, `/password-reset` |
| Organisation | `/org/sites`, `/org/departments`, `/org/positions` (GET/POST) |
| Politique MFA | `GET/PUT /security/policy` (`users_manage`) ; les sessions sans MFA reçoivent `403 mfa_enrollment_required` sauf sur `/auth/*` |
| Pièces | `GET/POST /employees/{id}/documents` (POST : corps = octets du fichier, `?name&kind&expires_on`), `GET /documents/{id}/download`, `DELETE /documents/{id}`, `GET /documents/expiring?days=` (`documents_read` / `documents_manage`) |
| Modèles de contrat | `GET/POST /contract-templates` (POST : `contracts_manage`), `GET /contract-templates/{id}`, `GET /contracts/{id}/document.pdf?template_id=` (`contracts_read`) |
| Grille SMIG | `GET/POST /wage-grids` (POST : `payroll_rules_manage`, `dry_run` possible), `GET /wage-grids/{id}`, `GET/POST /employees/{id}/wage-class`, `GET /reports/smig-compliance[.csv]` (`compensation_read`) |
| Employés | `GET/POST /employees` ; `GET /employees/me`, `/employees/export.csv` ; `GET/PUT /employees/{id}` ; `/{id}/compensation`, `/{id}/dependents` (GET `compensation_read` / POST `compensation_manage`), `/{id}/pay-items` |
| Contrats | `GET/POST /contracts` ; `GET /contracts/expiring` ; `POST /contracts/{id}/terminate`, `/amendments` |
| Congés | `/leave/types`, `/leave/balances`, `/leave/balances/me`, `/leave/requests`, `/leave/requests/{id}/decision`, `/leave/requests/{id}/cancel` |
| Paie | `/payroll/rule-sets`, `/payroll/rule-sets/{id}/approve`, `/payroll/periods`, `/payroll/runs`, `/payroll/runs/{id}`, `/recalculate`, `/validate`, `/close` |
| Règles (modèles) | `GET /payroll/rule-set-templates` (`payroll_rules_manage`) : modèles brouillons avec provenance, points à compléter et sources |
| Bulletins | `GET /payslips/me`, `/payslips/{item_id}`, `/payslips/{item_id}/pdf` |
| Pointage | `/attendance/settings`, `/clock-in`, `/clock-out`, `/me`, `/entries`, `/entries/{id}`, `/summary`, `/summary.csv` |
| Badgeuses | `/attendance/devices`, `/devices/{id}/revoke`, `/badges`, `/badges/{id}`, `/import`, `/punches`, `/punches/reprocess` ; `POST /device/punches` |
| Rapports | `/reports/headcount`, `/movements`, `/absences`, `/payroll-cost` (+ `.csv`), `/dashboard`, `/audit` |

Détails des corps de requête pour la sécurité et le pointage : [API (détails)](../api/openapi.md).

!!! info "OpenAPI"
    La génération OpenAPI (utoipa) n'est pas encore livrée ; voir la [feuille de route](../roadmap.md).
