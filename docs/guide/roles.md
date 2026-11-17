# Rôles et permissions

Les permissions sont appliquées **côté serveur** à chaque requête. Le rôle est relu en base à chaque requête : un changement de rôle est immédiat.

| Rôle | Usage typique |
|---|---|
| `hr_admin` — Admin RH | Administration complète de la société, gestion des utilisateurs |
| `hr_manager` — Gestionnaire RH | Opérations RH courantes, sans gestion des utilisateurs ni accès à la rémunération |
| `payroll_manager` — Responsable paie | Rémunération, règles et exécutions de paie |
| `manager` — Manager | Équipe directe : congés et pointage |
| `employee` — Employé | Libre-service : ses congés, bulletins, pointage |
| `auditor` — Auditeur | Lecture seule, y compris le journal d'audit |

La matrice complète est dans la [référence des permissions](../reference/permissions.md).

## Règles transversales

- Un **manager** ne voit que ses **subordonnés directs**.
- **Personne** n'approuve ni ne corrige sa propre demande ou son propre pointage.
- Un employé ne voit que **ses** bulletins, uniquement ceux des paies **clôturées**.
- La **rémunération** n'est visible que par l'admin RH et le responsable paie ; chaque consultation est tracée.
- Hors périmètre (autre société, hors équipe), l'API répond **404** pour ne pas révéler l'existence de la ressource.
