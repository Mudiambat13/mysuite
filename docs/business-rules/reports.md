# Règles des rapports

- Période : par défaut du 1er janvier à aujourd'hui, maximum 5 ans, `to >= from`.
- Mouvements : entrées = date d'embauche, sorties = date de sortie dans la période.
- Absences : congés **approuvés**, comptés selon leur date de début.
- Masse salariale : uniquement les exécutions **clôturées et non simulées** ; ventilation par département *actuel* (pas d'historique d'affectation). Permission `payroll_read` requise (l'admin RH n'y a pas accès) ; consultation auditée (`report.payroll_cost`).
- Exports CSV : audités (`report.export`), BOM UTF-8, cellules neutralisées contre l'injection de formules.
