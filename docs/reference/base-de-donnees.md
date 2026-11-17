# Base de données

PostgreSQL 16. Migrations SQLx dans `backend/migrations`, appliquées dans l'ordre, **jamais modifiées** une fois publiées.

| Migration | Contenu |
|---|---|
| `0001_foundation` | `tenants`, `users`, `memberships`, `sessions`, `login_events`, `audit_logs` (+ triggers d'ajout seul) |
| `0002_organization_employees` | `sites`, `departments`, `positions`, `employees`, `employee_compensation`, `employee_pay_items` |
| `0003_leave` | `leave_types`, `leave_balances`, `leave_requests` |
| `0004_payroll` | `payroll_rule_sets`, `payroll_periods`, `payroll_runs`, `payroll_items` (+ triggers de gel) |
| `0005_contracts` | `contracts`, `contract_amendments` |
| `0006_attendance` | `attendance_settings`, `attendance_entries` |
| `0007_timeclock` | `attendance_devices`, `employee_badges`, `attendance_raw_punches` |
| `0009_tax_dependents` | `employee_dependents` (historique daté des personnes à charge) |
| `0010_wage_grid` | `wage_grids`, `wage_grid_rows`, `employee_wage_class` (grille SMIG, classes, historique daté par employé) |
| `0011_contract_templates` | `contract_templates` (modèles de contrat par société) |
| `0012_employee_documents` | `employee_documents` (pièces en `bytea`, métadonnées conservées après suppression) |
| `0013_mfa_policy` | `tenants.mfa_required` (MFA obligatoire par société) |
| `0008_account_security` | `user_mfa`, `mfa_recovery_codes`, `mfa_challenges`, `password_resets` |

## Invariants garantis par la base

- Toute table métier porte `tenant_id` ; les clés étrangères sont **composites** `(id, tenant_id)` : impossible de référencer une ligne d'une autre société.
- `audit_logs` : `UPDATE`/`DELETE` interdits par trigger.
- Paie : une seule exécution non clôturée par période (index unique partiel) ; lignes figées dès la validation ; un jeu de règles approuvé exige approbateur, date de vérification, source, et n'est pas de démonstration.
- Pointage : un seul pointage ouvert par employé ; `clock_out > clock_in`.
- Badgeuses : `UNIQUE (tenant_id, identifier, punched_at)` pour l'idempotence.
- MFA : secrets chiffrés (`secret_enc BYTEA`), codes de secours hachés, un lien de réinitialisation valide par utilisateur.

## Statuts

| Objet | Valeurs |
|---|---|
| Employé | `candidate`, `active`, `suspended`, `on_leave`, `exited` |
| Contrat (type) | `cdi`, `cdd`, `stage`, `journalier`, `consultant` |
| Contrat (statut) | `active`, `terminated`, `expired` |
| Congé | `pending`, `approved`, `rejected`, `cancelled` |
| Règles de paie | `draft`, `approved`, `retired` |
| Paie | `draft`, `validated`, `closed` |
| Passage badgeuse | `in`, `out`, `duplicate`, `unknown_badge`, `rejected` |
