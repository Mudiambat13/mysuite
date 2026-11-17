# Référence des permissions

Source de vérité : `backend/crates/shared/src/permissions.rs`. Tableau ci-dessous aligné sur le code ; un test unitaire protège les cas critiques.

| Permission | Admin RH | Gest. RH | Resp. paie | Manager | Employé | Auditeur |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| `users_manage` | ✔ | | | | | |
| `org_read` | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ |
| `org_manage` | ✔ | ✔ | | | | |
| `employees_read` | ✔ | ✔ | ✔ | ✔ | | ✔ |
| `employees_manage` | ✔ | ✔ | | | | |
| `contracts_read` | ✔ | ✔ | | | | ✔ |
| `contracts_manage` | ✔ | ✔ | | | | |
| `documents_read` | ✔ | ✔ | | | | |
| `documents_manage` | ✔ | ✔ | | | | |
| `compensation_read` | ✔ | | ✔ | | | |
| `compensation_manage` | ✔ | | ✔ | | | |
| `leave_request_own` | ✔ | ✔ | ✔ | ✔ | ✔ | |
| `leave_approve` | ✔ | ✔ | | ✔ | | |
| `leave_read_all` | ✔ | ✔ | ✔ | | | ✔ |
| `leave_types_manage` | ✔ | ✔ | | | | |
| `payroll_read` | | | ✔ | | | ✔ |
| `payroll_manage` | | | ✔ | | | |
| `payroll_rules_manage` | | | ✔ | | | |
| `payslip_read_own` | ✔ | ✔ | ✔ | ✔ | ✔ | |
| `attendance_clock_own` | ✔ | ✔ | ✔ | ✔ | ✔ | |
| `attendance_read` | ✔ | ✔ | ✔ | ✔ (équipe) | | ✔ |
| `attendance_manage` | ✔ | ✔ | | | | |
| `reports_read` | ✔ | ✔ | ✔ | | | ✔ |
| `audit_read` | ✔ | | | | | ✔ |

!!! note "Administrateur de plateforme"
    Le drapeau `is_platform_admin` (hors matrice) donne accès à la page **Plateforme** (création de sociétés) mais pas aux données d'une société.

## Ajouter une permission

1. Ajouter la variante dans `Permission` et `as_str`.
2. L'attribuer dans `permissions_for`.
3. Protéger le handler avec `auth.require(Permission::…)`.
4. Ajouter un test d'accès négatif dans `tests/api.rs` et documenter ici.
