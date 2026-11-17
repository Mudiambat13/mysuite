use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// Arrondi commercial (0,5 vers le haut).
    HalfUp,
    /// Arrondi bancaire.
    HalfEven,
    /// Troncature.
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionBase {
    /// Salaire brut (base + indemnités).
    Gross,
    /// Salaire de base uniquement.
    BaseSalary,
}

/// Cotisation à taux, avec plafond de base facultatif.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    pub code: String,
    pub label: String,
    pub rate: Decimal,
    pub base: ContributionBase,
    /// Plafond de la base de calcul (même période que la paie). `None` = sans plafond.
    #[serde(default)]
    pub ceiling: Option<Decimal>,
    /// Si vrai et si un minimum salarial est connu pour l'employé, la base ne peut pas lui être inférieure
    /// (appliqué avant le plafond).
    #[serde(default)]
    pub floor_to_minimum_wage: bool,
}

/// Tranche d'un barème progressif. `upto = None` : dernière tranche (sans borne).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bracket {
    pub upto: Option<Decimal>,
    pub rate: Decimal,
}

/// Réduction d'impôt pour personnes à charge : `rate_per_dependent` × min(personnes, `max_dependents`)
/// appliqué à l'impôt calculé sur la fraction de l'assiette ne dépassant pas `applies_up_to`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyReduction {
    pub rate_per_dependent: Decimal,
    pub max_dependents: u32,
    /// Borne haute de l'assiette sur laquelle la réduction s'applique (`None` = toute l'assiette).
    #[serde(default)]
    pub applies_up_to: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxRule {
    pub code: String,
    pub label: String,
    /// Barème exprimé pour la période de paie (ex. mensuel). Les bornes sont croissantes.
    pub brackets: Vec<Bracket>,
    /// Réduction pour personnes à charge, appliquée à l'impôt progressif (facultative).
    #[serde(default)]
    pub family_reduction: Option<FamilyReduction>,
    /// Plafond de l'impôt exprimé en fraction de l'assiette (ex. 0.30), appliqué après la réduction (facultatif).
    #[serde(default)]
    pub max_rate_of_base: Option<Decimal>,
    /// Arrondi final de l'impôt au multiple indiqué (ex. 100), mode d'arrondi du jeu de règles (facultatif).
    #[serde(default)]
    pub round_to_multiple: Option<Decimal>,
}

/// Jeu de règles complet, versionné par date d'effet côté base de données.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSet {
    pub rounding_scale: u32,
    pub rounding_mode: RoundingMode,
    #[serde(default)]
    pub employee_contributions: Vec<Contribution>,
    #[serde(default)]
    pub employer_contributions: Vec<Contribution>,
    /// Si vrai, l'assiette fiscale = brut − cotisations salariales ; sinon = brut.
    #[serde(default)]
    pub tax_base_after_employee_contributions: bool,
    #[serde(default)]
    pub tax: Option<TaxRule>,
}
