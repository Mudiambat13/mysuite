# Moteur de paie (référence)

Crate `mysuite-payroll` : **fonctions pures**, sans E/S ni accès base. Mêmes entrées + même jeu de règles ⇒ même résultat, au centime.

## Jeu de règles (`RuleSet`)

```json
{
  "rounding_scale": 2,
  "rounding_mode": "half_up",
  "employee_contributions": [
    { "code": "EXEMPLE_SAL", "label": "Cotisation salariale (exemple)", "rate": "0.00", "base": "gross", "ceiling": null }
  ],
  "employer_contributions": [],
  "tax_base_after_employee_contributions": true,
  "tax": {
    "code": "EXEMPLE_IMPOT",
    "label": "Impôt (exemple)",
    "brackets": [ { "upto": "100.00", "rate": "0.00" }, { "upto": null, "rate": "0.00" } ],
    "family_reduction": { "rate_per_dependent": "0.00", "max_dependents": 0, "applies_up_to": null },
    "max_rate_of_base": null,
    "round_to_multiple": null
  }
}
```

!!! warning
    Les valeurs ci-dessus sont des **exemples sans valeur légale**. Les taux réels doivent venir d'un expert, avec source officielle.

| Champ | Valeurs |
|---|---|
| `rounding_mode` | `half_up` (commercial), `half_even` (bancaire), `down` (troncature) |
| `base` | `gross` (base + indemnités), `base_salary` (salaire de base) |
| `ceiling` | plafond de la base, même période que la paie ; `null` = sans plafond |
| `brackets` | bornes croissantes ; `upto: null` = dernière tranche |
| `tax.family_reduction` | facultatif : `rate_per_dependent` (0–1), `max_dependents`, `applies_up_to` (borne de l'assiette dont l'impôt est éligible, `null` = toute) |
| `tax.max_rate_of_base` | facultatif : plafond de l'impôt = taux × assiette (0–1) |
| `tax.round_to_multiple` | facultatif : arrondi final au multiple (ex. `100`), mode d'arrondi du jeu |
| `tax_base_after_employee_contributions` | vrai : assiette = brut − cotisations salariales ; faux : brut |

## Ordre de calcul

1. Brut = salaire de base + indemnités
2. Cotisations salariales = taux × min(base, plafond)
3. Cotisations patronales (idem)
4. Assiette fiscale
5. Impôt = somme par tranches, puis réduction pour personnes à charge, puis plafond, puis arrondi
6. Retenues
7. **Net** = brut − cotisations salariales − impôt − retenues
8. **Coût employeur** = brut + cotisations patronales

Entrée supplémentaire : `dependents` (personnes à charge, 0 par défaut). Arithmétique en `Decimal` exclusivement. Voir aussi [règles de paie](../business-rules/payroll.md).
