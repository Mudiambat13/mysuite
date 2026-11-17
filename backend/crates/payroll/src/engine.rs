use crate::rules::{Bracket, Contribution, ContributionBase, RoundingMode, RuleSet};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PayrollError {
    #[error("montant négatif interdit pour {0}")]
    NegativeAmount(String),
    #[error("barème invalide : {0}")]
    InvalidBrackets(String),
    #[error("taux invalide pour {0} (doit être compris entre 0 et 1)")]
    InvalidRate(String),
    #[error("paramètre d'impôt invalide : {0}")]
    InvalidTaxParam(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allowance {
    pub code: String,
    pub label: String,
    pub amount: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deduction {
    pub code: String,
    pub label: String,
    pub amount: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayrollInput {
    pub base_salary: Decimal,
    #[serde(default)]
    pub allowances: Vec<Allowance>,
    #[serde(default)]
    pub deductions: Vec<Deduction>,
    /// Personnes fiscalement à charge (utilisé seulement si le jeu de règles prévoit une réduction).
    #[serde(default)]
    pub dependents: u32,
    /// Minimum salarial mensuel applicable à l'employé (ex. SMIG de sa classe), si connu.
    #[serde(default)]
    pub minimum_wage: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Earning,
    EmployeeContribution,
    EmployerContribution,
    Tax,
    Deduction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayLine {
    pub kind: LineKind,
    pub code: String,
    pub label: String,
    /// Assiette utilisée (si applicable).
    pub base: Option<Decimal>,
    pub rate: Option<Decimal>,
    pub amount: Decimal,
    /// Explication du calcul (ex. réduction pour personnes à charge, plafond appliqué).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayrollResult {
    pub gross: Decimal,
    pub total_employee_contributions: Decimal,
    pub total_employer_contributions: Decimal,
    pub tax_base: Decimal,
    pub tax: Decimal,
    pub total_deductions: Decimal,
    pub net: Decimal,
    pub employer_cost: Decimal,
    pub lines: Vec<PayLine>,
}

fn round(v: Decimal, scale: u32, mode: RoundingMode) -> Decimal {
    let strat = match mode {
        RoundingMode::HalfUp => RoundingStrategy::MidpointAwayFromZero,
        RoundingMode::HalfEven => RoundingStrategy::MidpointNearestEven,
        RoundingMode::Down => RoundingStrategy::ToZero,
    };
    v.round_dp_with_strategy(scale, strat)
}

fn check_rate(code: &str, rate: Decimal) -> Result<(), PayrollError> {
    if rate < Decimal::ZERO || rate > Decimal::ONE {
        Err(PayrollError::InvalidRate(code.to_string()))
    } else {
        Ok(())
    }
}

fn check_param(name: &str, v: Decimal) -> Result<(), PayrollError> {
    if v < Decimal::ZERO || v > Decimal::ONE {
        Err(PayrollError::InvalidTaxParam(format!(
            "{name} doit être compris entre 0 et 1"
        )))
    } else {
        Ok(())
    }
}

fn validate_brackets(brackets: &[Bracket]) -> Result<(), PayrollError> {
    if brackets.is_empty() {
        return Err(PayrollError::InvalidBrackets("aucune tranche".into()));
    }
    let mut prev: Option<Decimal> = None;
    for (i, b) in brackets.iter().enumerate() {
        check_rate("bracket", b.rate)?;
        match b.upto {
            Some(u) => {
                if let Some(p) = prev
                    && u <= p
                {
                    return Err(PayrollError::InvalidBrackets(
                        "bornes non croissantes".into(),
                    ));
                }
                prev = Some(u);
            }
            None => {
                if i != brackets.len() - 1 {
                    return Err(PayrollError::InvalidBrackets(
                        "tranche sans borne non finale".into(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Impôt progressif : chaque tranche taxe la part de l'assiette comprise dans la tranche.
fn progressive_tax(base: Decimal, brackets: &[Bracket]) -> Decimal {
    let mut remaining = base.max(Decimal::ZERO);
    let mut lower = Decimal::ZERO;
    let mut total = Decimal::ZERO;
    for b in brackets {
        if remaining <= Decimal::ZERO {
            break;
        }
        let width = match b.upto {
            Some(u) => (u - lower).max(Decimal::ZERO),
            None => remaining,
        };
        let taxed = remaining.min(width);
        total += taxed * b.rate;
        remaining -= taxed;
        if let Some(u) = b.upto {
            lower = u;
        }
    }
    total
}

fn contribution_amount(
    c: &Contribution,
    gross: Decimal,
    base_salary: Decimal,
    minimum_wage: Option<Decimal>,
    scale: u32,
    mode: RoundingMode,
) -> Result<PayLineParts, PayrollError> {
    check_rate(&c.code, c.rate)?;
    let mut raw = match c.base {
        ContributionBase::Gross => gross,
        ContributionBase::BaseSalary => base_salary,
    };
    let mut note = None;
    if let (true, Some(min)) = (c.floor_to_minimum_wage, minimum_wage)
        && raw < min
    {
        note = Some(format!(
            "base relevée au minimum salarial applicable : {} au lieu de {}",
            min.normalize(),
            raw.normalize()
        ));
        raw = min;
    }
    let base = match c.ceiling {
        Some(cap) => raw.min(cap),
        None => raw,
    };
    Ok(PayLineParts {
        base,
        amount: round(base * c.rate, scale, mode),
        note,
    })
}

struct PayLineParts {
    base: Decimal,
    amount: Decimal,
    note: Option<String>,
}

/// Calcule une paie individuelle. Pure et déterministe : mêmes entrées + mêmes règles = même résultat.
pub fn compute(input: &PayrollInput, rules: &RuleSet) -> Result<PayrollResult, PayrollError> {
    let scale = rules.rounding_scale;
    let mode = rules.rounding_mode;
    let mut lines = Vec::new();

    if input.base_salary < Decimal::ZERO {
        return Err(PayrollError::NegativeAmount("salaire de base".into()));
    }
    let base_salary = round(input.base_salary, scale, mode);
    lines.push(PayLine {
        kind: LineKind::Earning,
        code: "BASE".into(),
        label: "Salaire de base".into(),
        base: None,
        rate: None,
        amount: base_salary,
        note: None,
    });

    let mut gross = base_salary;
    for a in &input.allowances {
        if a.amount < Decimal::ZERO {
            return Err(PayrollError::NegativeAmount(a.code.clone()));
        }
        let amount = round(a.amount, scale, mode);
        gross += amount;
        lines.push(PayLine {
            kind: LineKind::Earning,
            code: a.code.clone(),
            label: a.label.clone(),
            base: None,
            rate: None,
            amount,
            note: None,
        });
    }

    let mut total_emp = Decimal::ZERO;
    for c in &rules.employee_contributions {
        let p = contribution_amount(c, gross, base_salary, input.minimum_wage, scale, mode)?;
        total_emp += p.amount;
        lines.push(PayLine {
            kind: LineKind::EmployeeContribution,
            code: c.code.clone(),
            label: c.label.clone(),
            base: Some(p.base),
            rate: Some(c.rate),
            amount: p.amount,
            note: p.note,
        });
    }

    let mut total_er = Decimal::ZERO;
    for c in &rules.employer_contributions {
        let p = contribution_amount(c, gross, base_salary, input.minimum_wage, scale, mode)?;
        total_er += p.amount;
        lines.push(PayLine {
            kind: LineKind::EmployerContribution,
            code: c.code.clone(),
            label: c.label.clone(),
            base: Some(p.base),
            rate: Some(c.rate),
            amount: p.amount,
            note: p.note,
        });
    }

    let tax_base = if rules.tax_base_after_employee_contributions {
        (gross - total_emp).max(Decimal::ZERO)
    } else {
        gross
    };
    let mut tax = Decimal::ZERO;
    if let Some(rule) = &rules.tax {
        validate_brackets(&rule.brackets)?;
        let mut t = progressive_tax(tax_base, &rule.brackets);
        let mut notes: Vec<String> = Vec::new();
        if let Some(f) = &rule.family_reduction {
            check_param("réduction par personne", f.rate_per_dependent)?;
            let eligible_base = match f.applies_up_to {
                Some(cap) => tax_base.min(cap),
                None => tax_base,
            };
            let eligible_tax = progressive_tax(eligible_base, &rule.brackets);
            let n = Decimal::from(input.dependents.min(f.max_dependents));
            let reduction = eligible_tax * f.rate_per_dependent * n;
            if reduction > Decimal::ZERO {
                notes.push(format!(
                    "réduction personnes à charge : {n} × {} % de l'impôt sur {} (soit {}) = -{}",
                    (f.rate_per_dependent * Decimal::from(100)).normalize(),
                    round(eligible_base, scale, mode).normalize(),
                    round(eligible_tax, scale, mode).normalize(),
                    round(reduction, scale, mode).normalize()
                ));
            }
            t -= reduction;
        }
        if let Some(cap) = rule.max_rate_of_base {
            check_param("plafond de l'impôt", cap)?;
            if t > tax_base * cap {
                notes.push(format!("plafonné à {cap} de l'assiette"));
                t = tax_base * cap;
            }
        }
        t = t.max(Decimal::ZERO);
        tax = match rule.round_to_multiple {
            Some(m) => {
                if m <= Decimal::ZERO {
                    return Err(PayrollError::InvalidTaxParam(
                        "arrondi (multiple > 0)".into(),
                    ));
                }
                round(t / m, 0, mode) * m
            }
            None => round(t, scale, mode),
        };
        let tax_note = if notes.is_empty() {
            None
        } else {
            Some(notes.join(" ; "))
        };
        lines.push(PayLine {
            kind: LineKind::Tax,
            code: rule.code.clone(),
            label: rule.label.clone(),
            base: Some(tax_base),
            rate: None,
            amount: tax,
            note: tax_note,
        });
    }

    let mut total_ded = Decimal::ZERO;
    for d in &input.deductions {
        if d.amount < Decimal::ZERO {
            return Err(PayrollError::NegativeAmount(d.code.clone()));
        }
        let amount = round(d.amount, scale, mode);
        total_ded += amount;
        lines.push(PayLine {
            kind: LineKind::Deduction,
            code: d.code.clone(),
            label: d.label.clone(),
            base: None,
            rate: None,
            amount,
            note: None,
        });
    }

    let net = gross - total_emp - tax - total_ded;
    Ok(PayrollResult {
        gross,
        total_employee_contributions: total_emp,
        total_employer_contributions: total_er,
        tax_base,
        tax,
        total_deductions: total_ded,
        net,
        employer_cost: gross + total_er,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Bracket, Contribution, ContributionBase, RoundingMode, RuleSet, TaxRule};
    use rust_decimal_macros::dec;

    /// Règles FICTIVES de test : aucune valeur ne correspond à une loi réelle.
    fn test_rules() -> RuleSet {
        RuleSet {
            rounding_scale: 2,
            rounding_mode: RoundingMode::HalfUp,
            employee_contributions: vec![Contribution {
                code: "T_EMP".into(),
                label: "Cotisation test salarié".into(),
                rate: dec!(0.05),
                base: ContributionBase::Gross,
                ceiling: None,
                floor_to_minimum_wage: false,
            }],
            employer_contributions: vec![Contribution {
                code: "T_ER".into(),
                label: "Cotisation test employeur".into(),
                rate: dec!(0.10),
                base: ContributionBase::Gross,
                ceiling: Some(dec!(1000)),
                floor_to_minimum_wage: false,
            }],
            tax_base_after_employee_contributions: true,
            tax: Some(TaxRule {
                family_reduction: None,
                max_rate_of_base: None,
                round_to_multiple: None,
                code: "T_TAX".into(),
                label: "Impôt test".into(),
                brackets: vec![
                    Bracket {
                        upto: Some(dec!(500)),
                        rate: dec!(0),
                    },
                    Bracket {
                        upto: Some(dec!(1000)),
                        rate: dec!(0.10),
                    },
                    Bracket {
                        upto: None,
                        rate: dec!(0.20),
                    },
                ],
            }),
        }
    }

    fn input(base: Decimal) -> PayrollInput {
        PayrollInput {
            base_salary: base,
            allowances: vec![],
            deductions: vec![],
            dependents: 0,
            minimum_wage: None,
        }
    }

    #[test]
    fn simple_computation() {
        let r = compute(&input(dec!(1200)), &test_rules()).unwrap();
        assert_eq!(r.gross, dec!(1200.00));
        assert_eq!(r.total_employee_contributions, dec!(60.00));
        // plafond employeur 1000 -> 100
        assert_eq!(r.total_employer_contributions, dec!(100.00));
        // assiette fiscale 1140 : 0*500 + 0.10*500 + 0.20*140 = 78
        assert_eq!(r.tax_base, dec!(1140.00));
        assert_eq!(r.tax, dec!(78.00));
        assert_eq!(r.net, dec!(1200.00) - dec!(60.00) - dec!(78.00));
        assert_eq!(r.employer_cost, dec!(1300.00));
    }

    #[test]
    fn zero_salary_gives_zero_everything() {
        let r = compute(&input(dec!(0)), &test_rules()).unwrap();
        assert_eq!(r.net, dec!(0));
        assert_eq!(r.tax, dec!(0));
    }

    #[test]
    fn below_first_bracket_no_tax() {
        let r = compute(&input(dec!(400)), &test_rules()).unwrap();
        assert_eq!(r.tax, dec!(0.00));
        assert_eq!(r.net, dec!(380.00));
    }

    #[test]
    fn allowances_and_deductions_flow_through() {
        let mut i = input(dec!(1000));
        i.allowances.push(Allowance {
            code: "TRANSPORT".into(),
            label: "Transport".into(),
            amount: dec!(100),
        });
        i.deductions.push(Deduction {
            code: "ADV".into(),
            label: "Avance".into(),
            amount: dec!(50),
        });
        let r = compute(&i, &test_rules()).unwrap();
        assert_eq!(r.gross, dec!(1100.00));
        assert_eq!(r.total_deductions, dec!(50.00));
        assert_eq!(
            r.net,
            r.gross - r.total_employee_contributions - r.tax - dec!(50.00)
        );
    }

    #[test]
    fn rounding_modes_differ_on_midpoint() {
        let mut rules = test_rules();
        rules.tax = None;
        rules.employee_contributions[0].rate = dec!(0.005);
        // 0.005 * 100 = 0.50 -> scale 0
        rules.rounding_scale = 0;
        rules.rounding_mode = RoundingMode::HalfUp;
        assert_eq!(
            compute(&input(dec!(100)), &rules)
                .unwrap()
                .total_employee_contributions,
            dec!(1)
        );
        rules.rounding_mode = RoundingMode::HalfEven;
        assert_eq!(
            compute(&input(dec!(100)), &rules)
                .unwrap()
                .total_employee_contributions,
            dec!(0)
        );
        rules.rounding_mode = RoundingMode::Down;
        assert_eq!(
            compute(&input(dec!(100)), &rules)
                .unwrap()
                .total_employee_contributions,
            dec!(0)
        );
    }

    #[test]
    fn deterministic() {
        let a = compute(&input(dec!(987.65)), &test_rules()).unwrap();
        let b = compute(&input(dec!(987.65)), &test_rules()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_negative_and_invalid_rules() {
        assert!(matches!(
            compute(&input(dec!(-1)), &test_rules()),
            Err(PayrollError::NegativeAmount(_))
        ));
        let mut bad = test_rules();
        bad.employee_contributions[0].rate = dec!(1.5);
        assert!(matches!(
            compute(&input(dec!(100)), &bad),
            Err(PayrollError::InvalidRate(_))
        ));
        let mut bad = test_rules();
        bad.tax.as_mut().unwrap().brackets = vec![
            Bracket {
                upto: Some(dec!(10)),
                rate: dec!(0.1),
            },
            Bracket {
                upto: Some(dec!(5)),
                rate: dec!(0.2),
            },
        ];
        assert!(matches!(
            compute(&input(dec!(100)), &bad),
            Err(PayrollError::InvalidBrackets(_))
        ));
    }

    #[test]
    fn no_float_drift_on_repeated_small_amounts() {
        // 0.1 + 0.2 doit valoir exactement 0.3 avec Decimal
        let mut i = input(dec!(0.1));
        i.allowances.push(Allowance {
            code: "A".into(),
            label: "A".into(),
            amount: dec!(0.2),
        });
        let mut rules = test_rules();
        rules.employee_contributions.clear();
        rules.employer_contributions.clear();
        rules.tax = None;
        assert_eq!(compute(&i, &rules).unwrap().gross, dec!(0.30));
    }
}

#[cfg(test)]
mod tax_mechanics_tests {
    use super::*;
    use crate::rules::{Bracket, FamilyReduction, RoundingMode, RuleSet, TaxRule};
    use rust_decimal_macros::dec;

    /// Barème FICTIF (valeurs de test, sans valeur légale) pour exercer les mécanismes.
    fn rules(
        family: Option<FamilyReduction>,
        cap: Option<Decimal>,
        multiple: Option<Decimal>,
    ) -> RuleSet {
        RuleSet {
            rounding_scale: 2,
            rounding_mode: RoundingMode::HalfUp,
            employee_contributions: vec![],
            employer_contributions: vec![],
            tax_base_after_employee_contributions: false,
            tax: Some(TaxRule {
                code: "T".into(),
                label: "Impôt test".into(),
                brackets: vec![
                    Bracket {
                        upto: Some(dec!(1000)),
                        rate: dec!(0.10),
                    },
                    Bracket {
                        upto: Some(dec!(2000)),
                        rate: dec!(0.20),
                    },
                    Bracket {
                        upto: Some(dec!(3000)),
                        rate: dec!(0.30),
                    },
                    Bracket {
                        upto: None,
                        rate: dec!(0.40),
                    },
                ],
                family_reduction: family,
                max_rate_of_base: cap,
                round_to_multiple: multiple,
            }),
        }
    }

    fn input(base: Decimal, dependents: u32) -> PayrollInput {
        PayrollInput {
            base_salary: base,
            allowances: vec![],
            deductions: vec![],
            dependents,
            minimum_wage: None,
        }
    }

    fn fam() -> Option<FamilyReduction> {
        Some(FamilyReduction {
            rate_per_dependent: dec!(0.02),
            max_dependents: 3,
            applies_up_to: Some(dec!(3000)),
        })
    }

    #[test]
    fn without_options_behaviour_is_unchanged() {
        // 1000×10% + 1000×20% + 500×30% = 450
        let r = compute(&input(dec!(2500), 5), &rules(None, None, None)).unwrap();
        assert_eq!(r.tax, dec!(450.00));
        assert!(r.lines.iter().all(|l| l.note.is_none()));
    }

    #[test]
    fn family_reduction_is_limited_by_count_and_by_bracket() {
        // Base 5000 : impôt = 100+200+300+2000×40% = 1400. Éligible (≤ 3000) = 600.
        // 2 personnes → 600 × 0,02 × 2 = 24 → 1376.
        let r = compute(&input(dec!(5000), 2), &rules(fam(), None, None)).unwrap();
        assert_eq!(r.tax, dec!(1376.00));
        // 10 personnes : plafonné à 3 → 600 × 0,02 × 3 = 36 → 1364.
        let r = compute(&input(dec!(5000), 10), &rules(fam(), None, None)).unwrap();
        assert_eq!(r.tax, dec!(1364.00));
        assert!(r.lines.iter().any(|l| {
            l.note
                .as_deref()
                .is_some_and(|n| n.contains("personnes à charge"))
        }));
        // 0 personne : aucune réduction, aucune note.
        let r = compute(&input(dec!(5000), 0), &rules(fam(), None, None)).unwrap();
        assert_eq!(r.tax, dec!(1400.00));
        assert!(r.lines.iter().all(|l| l.note.is_none()));
    }

    #[test]
    fn tax_cap_applies_after_reduction_and_is_noted() {
        // Base 10000 : 100+200+300+7000×40% = 3400 > 30 % × 10000 = 3000.
        let r = compute(&input(dec!(10000), 0), &rules(None, Some(dec!(0.30)), None)).unwrap();
        assert_eq!(r.tax, dec!(3000.00));
        assert!(
            r.lines
                .iter()
                .any(|l| l.note.as_deref().is_some_and(|n| n.contains("plafonné")))
        );
        // Sous le plafond : inchangé.
        let r = compute(&input(dec!(2500), 0), &rules(None, Some(dec!(0.30)), None)).unwrap();
        assert_eq!(r.tax, dec!(450.00));
    }

    #[test]
    fn tax_rounds_to_the_requested_multiple_half_up() {
        // 2 personnes sur base 5000 → 1376 → arrondi à la centaine = 1400.
        let r = compute(&input(dec!(5000), 2), &rules(fam(), None, Some(dec!(100)))).unwrap();
        assert_eq!(r.tax, dec!(1400));
        // Reste exactement 50 → vers le haut ; 49,99 → vers le bas.
        let r = compute(&input(dec!(2350), 0), &rules(None, None, Some(dec!(100)))).unwrap();
        // 100+200+350×30% = 405 → 400
        assert_eq!(r.tax, dec!(400));
        let r = compute(&input(dec!(2500), 0), &rules(None, None, Some(dec!(100)))).unwrap();
        // 450 → 500 (≥ 50 monte)
        assert_eq!(r.tax, dec!(500));
        // net cohérent avec l'impôt arrondi
        assert_eq!(r.net, dec!(2500) - dec!(500));
    }

    #[test]
    fn invalid_tax_parameters_are_rejected() {
        let bad = Some(FamilyReduction {
            rate_per_dependent: dec!(1.5),
            max_dependents: 3,
            applies_up_to: None,
        });
        assert!(matches!(
            compute(&input(dec!(100), 1), &rules(bad, None, None)),
            Err(PayrollError::InvalidTaxParam(_))
        ));
        assert!(matches!(
            compute(&input(dec!(100), 1), &rules(None, Some(dec!(2)), None)),
            Err(PayrollError::InvalidTaxParam(_))
        ));
        assert!(matches!(
            compute(&input(dec!(100), 1), &rules(None, None, Some(dec!(0)))),
            Err(PayrollError::InvalidTaxParam(_))
        ));
    }

    #[test]
    fn computation_is_deterministic_with_new_options() {
        let rs = rules(fam(), Some(dec!(0.30)), Some(dec!(100)));
        let a = compute(&input(dec!(7777), 3), &rs).unwrap();
        let b = compute(&input(dec!(7777), 3), &rs).unwrap();
        assert_eq!(a, b);
    }
}

#[cfg(test)]
mod minimum_wage_floor_tests {
    use super::*;
    use crate::rules::{Contribution, ContributionBase, RoundingMode, RuleSet};
    use rust_decimal_macros::dec;

    fn rules(floor: bool, ceiling: Option<Decimal>) -> RuleSet {
        RuleSet {
            rounding_scale: 2,
            rounding_mode: RoundingMode::HalfUp,
            employee_contributions: vec![Contribution {
                code: "C".into(),
                label: "Cotisation test".into(),
                rate: dec!(0.05),
                base: ContributionBase::Gross,
                ceiling,
                floor_to_minimum_wage: floor,
            }],
            employer_contributions: vec![],
            tax_base_after_employee_contributions: false,
            tax: None,
        }
    }

    fn input(base: Decimal, min: Option<Decimal>) -> PayrollInput {
        PayrollInput {
            base_salary: base,
            allowances: vec![],
            deductions: vec![],
            dependents: 0,
            minimum_wage: min,
        }
    }

    #[test]
    fn base_is_raised_to_the_minimum_and_explained() {
        // Brut 400 < minimum 559 : cotisation sur 559 (5 % = 27,95), net = brut − cotisation.
        let r = compute(&input(dec!(400), Some(dec!(559))), &rules(true, None)).unwrap();
        assert_eq!(r.total_employee_contributions, dec!(27.95));
        assert_eq!(r.net, dec!(400) - dec!(27.95));
        let line = r.lines.iter().find(|l| l.code == "C").unwrap();
        assert_eq!(line.base, Some(dec!(559)));
        assert!(line.note.as_deref().unwrap().contains("minimum"));
    }

    #[test]
    fn floor_is_inactive_without_flag_minimum_or_when_gross_is_higher() {
        let plain = compute(&input(dec!(400), Some(dec!(559))), &rules(false, None)).unwrap();
        assert_eq!(plain.total_employee_contributions, dec!(20.00));
        let unknown = compute(&input(dec!(400), None), &rules(true, None)).unwrap();
        assert_eq!(unknown.total_employee_contributions, dec!(20.00));
        let above = compute(&input(dec!(800), Some(dec!(559))), &rules(true, None)).unwrap();
        assert_eq!(above.total_employee_contributions, dec!(40.00));
        assert!(above.lines.iter().all(|l| l.note.is_none()));
    }

    #[test]
    fn floor_applies_before_the_ceiling() {
        // Plafond 500 < minimum 559 : la base finale reste plafonnée à 500.
        let r = compute(
            &input(dec!(400), Some(dec!(559))),
            &rules(true, Some(dec!(500))),
        )
        .unwrap();
        assert_eq!(r.total_employee_contributions, dec!(25.00));
    }
}
