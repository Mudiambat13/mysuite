//! Moteur de paie MySuite : déterministe, pur (sans E/S), testable.
//!
//! Aucun taux ni seuil légal n'est codé ici : tout vient d'un [`RuleSet`]
//! versionné fourni par l'appelant. Tous les montants sont des `Decimal`.

mod engine;
mod rules;

pub use engine::{
    Allowance, Deduction, LineKind, PayLine, PayrollError, PayrollInput, PayrollResult, compute,
};
pub use rules::{
    Bracket, Contribution, ContributionBase, FamilyReduction, RoundingMode, RuleSet, TaxRule,
};
