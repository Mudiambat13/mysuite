# ADR 0004 — Paie versionnée et simulation par défaut

Aucune règle légale en dur. `payroll_rule_sets` (JSON validé par un calcul à blanc) est créé en brouillon ; l'approbation exige source officielle, date de vérification et (par défaut) un utilisateur différent du créateur. Sans jeu approuvé ⇒ paie en simulation, non validable. Une paie validée/clôturée est figée par triggers SQL ; les corrections passeront par des régularisations (non encore livrées). Arrondi configurable (`half_up`, `half_even`, `down`) appliqué à chaque ligne.
