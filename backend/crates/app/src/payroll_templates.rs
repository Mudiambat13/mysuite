//! Modèles de jeux de règles de paie, fournis comme **points de départ à faire valider** — jamais approuvés
//! automatiquement. Chaque valeur est accompagnée de son niveau de confiance et de ses sources ; ce qui n'a pas pu
//! être établi n'est pas inventé : il est listé dans `a_completer`.
//!
//! Le modèle RDC a été constitué le 2026-11-15 à partir de sources **secondaires** (résumés d'éditeurs de paie,
//! presse, cabinets). Le texte officiel de la loi n° 23/053 et les décrets CNSS n'ont pas été consultés directement.

use axum::{Json, Router, routing::get};
use mysuite_shared::permissions::Permission;
use serde_json::{Value, json};

use crate::{auth::Auth, error::AppResult, state::AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/payroll/rule-set-templates", get(list_templates))
}

async fn list_templates(auth: Auth) -> AppResult<Json<Vec<Value>>> {
    auth.require(Permission::PayrollRulesManage)?;
    Ok(Json(vec![drc_template(), drc_template_katanga()]))
}

/// Paramètres du moteur pour le modèle RDC (montants mensuels, CDF).
pub fn drc_params() -> Value {
    json!({
        "rounding_scale": 0,
        "rounding_mode": "half_up",
        "employee_contributions": [
            { "code": "CNSS_PENSION_SAL", "label": "CNSS pension — part salariale", "rate": "0.05", "base": "gross", "ceiling": null, "floor_to_minimum_wage": true }
        ],
        "employer_contributions": [
            { "code": "CNSS_PENSION_PAT", "label": "CNSS pension — part patronale", "rate": "0.05", "base": "gross", "ceiling": null, "floor_to_minimum_wage": true },
            { "code": "CNSS_PF_PAT", "label": "CNSS prestations familiales — part patronale", "rate": "0.065", "base": "gross", "ceiling": null, "floor_to_minimum_wage": true },
            { "code": "CNSS_RP_PAT", "label": "CNSS risques professionnels — part patronale", "rate": "0.015", "base": "gross", "ceiling": null, "floor_to_minimum_wage": true }
        ],
        "tax_base_after_employee_contributions": true,
        "tax": {
            "code": "IRPP",
            "label": "IRPP (ex-IPR)",
            "brackets": [
                { "upto": "162000", "rate": "0.03" },
                { "upto": "1800000", "rate": "0.15" },
                { "upto": "3600000", "rate": "0.30" },
                { "upto": null, "rate": "0.40" }
            ],
            "family_reduction": { "rate_per_dependent": "0.02", "max_dependents": 9, "applies_up_to": "3600000" },
            "max_rate_of_base": "0.30",
            "round_to_multiple": "100"
        }
    })
}

/// Variante pour l'ex-province du Katanga : prestations familiales au taux transitoire de 4 % (décret 18/041, art. 10).
/// La durée de validité de ce taux transitoire n'est pas établie : à confirmer auprès de la CNSS.
pub fn drc_template_katanga() -> Value {
    let mut t = drc_template();
    t["id"] = json!("CD-2026-brouillon-ex-katanga");
    t["name"] = json!("RDC — modèle 2026, ex-Katanga (à valider)");
    t["params"]["employer_contributions"][1]["rate"] = json!("0.04");
    t["params"]["employer_contributions"][1]["label"] =
        json!("CNSS prestations familiales — taux transitoire ex-Katanga");
    t["variante"] = json!(
        "Prestations familiales à 4 % (taux transitoire de l'ex-province du Katanga, art. 10 du décret 18/041) au lieu de 6,5 %. \
Durée de validité du taux transitoire non établie : à confirmer auprès de la CNSS avant usage."
    );
    t
}

pub fn drc_template() -> Value {
    json!({
        "id": "CD-2026-brouillon",
        "country": "CD",
        "name": "RDC — modèle 2026 (à valider)",
        "currency": "CDF",
        "period": "mensuel",
        "effective_from": "2026-01-01",
        "status": "brouillon_non_verifie",
        "params": drc_params(),
        "avertissement": "Ces valeurs proviennent de sources secondaires et n'ont pas été vérifiées contre le texte officiel. \
    Un expert (fiscaliste/paie RDC) doit les confirmer, compléter les points listés dans `a_completer`, puis un second utilisateur \
    approuve le jeu de règles en indiquant la source officielle et la date de vérification.",
        "provenance": [
            {
                "champ": "tax.brackets",
                "valeur": "Barème annuel 0–1 944 000 : 3 % ; –21 600 000 : 15 % ; –43 200 000 : 30 % ; au-delà : 40 %. Mensuel = annuel ÷ 12.",
                "confiance": "corroboré par plusieurs sources secondaires",
                "sources": ["monrespro", "congoquotidien", "sage"]
            },
            {
                "champ": "tax.family_reduction",
                "valeur": "2 % par personne à charge, 9 personnes maximum ; sans effet sur l'impôt de la fraction au-delà de la 3ᵉ tranche (art. 123, loi 23/053).",
                "confiance": "corroboré par deux sources secondaires",
                "sources": ["sage", "congoquotidien"]
            },
            {
                "champ": "tax.max_rate_of_base",
                "valeur": "Impôt plafonné à 30 % de la base imposable.",
                "confiance": "deux sources secondaires ; absent du résumé Sage ; ordre d'application avec la réduction familiale non confirmé (le modèle applique la réduction puis le plafond)",
                "sources": ["monrespro", "congoquotidien"]
            },
            {
                "champ": "tax.round_to_multiple",
                "valeur": "Impôt arrondi à la centaine de CDF la plus proche (reste ≥ 50 vers le haut) (art. 150).",
                "confiance": "une source secondaire (Sage)",
                "sources": ["sage"]
            },
            {
                "champ": "floor_to_minimum_wage (CNSS)",
                "valeur": "La base de cotisation CNSS ne peut pas être inférieure au SMIG (décret 18/041, art. 8) : appliquée si une grille de salaires minimums est importée et si une classe est affectée à l'employé.",
                "confiance": "texte du décret (via base juridique en ligne) ; le SMIG applicable provient de la grille importée par la société, dont la convention de calcul mensuel est à confirmer",
                "sources": ["decret-cnss-2018"]
            },
            {
                "champ": "tax_base_after_employee_contributions",
                "valeur": "true : assiette = brut − cotisations salariales.",
                "confiance": "NON CONFIRMÉ — hypothèse de départ à trancher par l'expert (déductibilité des cotisations, autres abattements)",
                "sources": []
            },
            {
                "champ": "employee_contributions / employer_contributions (CNSS)",
                "valeur": "Salarié 5 % (pensions) ; employeur 13 % = 5 % pensions + 6,5 % prestations familiales + 1,5 % risques professionnels (décret n° 18/041 du 24/11/2018, art. 2 à 4, taux applicables depuis le 01/01/2019).",
                "confiance": "texte du décret lu via une base juridique en ligne (droitcongolais.info), à confirmer contre le Journal officiel ; concorde avec Africarrieres ; discordant avec MonRespro (3,5 %, taux transitoire ancien)",
                "sources": ["decret-cnss-2018", "africarrieres"]
            }
        ],
        "a_completer": [
            "CNSS : le décret n° 18/041 ne prévoit aucun plafond de rémunération (le modèle n'en applique pas) ; une source secondaire évoque un plafond fixé par arrêté ministériel, non établi : à confirmer auprès de la CNSS.",
            "CNSS : le décret prévoit un taux transitoire de 4 % (au lieu de 6,5 %) pour les prestations familiales dans l'ex-province du Katanga (art. 10) ; sa durée de validité est inconnue : à confirmer pour les sociétés concernées (Haut-Katanga, Lualaba, Tanganyika, Haut-Lomami).",
            "CNSS : majoration possible jusqu'au double du taux risques professionnels pour un employeur en infraction (art. 5), et taux de 1,5 % général à confirmer selon le secteur.",
            "SMIG : importer la grille et affecter une classe à chaque employé pour activer le plancher de la base CNSS ; sans classe, aucun plancher n'est appliqué.",
            "INPP (environ 1 % à 3 % selon la taille de l'entreprise) et ONEM (environ 0,2 % à 0,5 % — valeurs contradictoires selon les sources) : non inclus, à ajouter selon la situation de la société.",
            "Abattement forfaitaire pour frais professionnels et autres déductions de la base imposable : non établis ni modélisés.",
            "Minimum de perception (une source mentionne 2 000 FC/mois) : non établi ni modélisé.",
            "Avantages en nature, indemnités exonérées (art. 69), heures supplémentaires, paie en USD et règle de conversion en CDF : non modélisés.",
            "Texte officiel de la loi n° 23/053 du 30 novembre 2023 et décret CNSS fixant les taux : à consulter directement."
        ],
        "sources": {
            "sage": {
                "titre": "Sage — DRC: Tax reforms affecting payroll (Law No. 23/053)",
                "url": "https://communityhub.sage.com/za/sage-payroll-professional/f/announcements/257553/democratic-republic-of-congo-drc-tax-reforms-affecting-payroll-law-no-23-053",
                "type": "secondaire"
            },
            "congoquotidien": {
                "titre": "Congo Quotidien — Impôt revenu RDC : simulateur et cas pratiques (2025-06-24)",
                "url": "https://www.congoquotidien.com/2025/06/24/impot-revenu-rdc-simulateur-cas-pratiques/",
                "type": "secondaire"
            },
            "monrespro": {
                "titre": "MonRespro — L'IPR en RDC (page datée du 2026-05-16)",
                "url": "https://business.monrespro.com/blog/ipr-impot-professionnel-remunerations-rdc",
                "type": "secondaire"
            },
            "africarrieres": {
                "titre": "Africarrieres — Charges sociales et impôts employeur en RDC (guide 2026)",
                "url": "https://africarrieres.com/rd-congo/pt/guide/employeur-entreprise/charges-employeur",
                "type": "secondaire"
            },
            "decret-cnss-2018": {
                "titre": "Décret n° 18/041 du 24 novembre 2018 fixant les taux de cotisations dues à la CNSS",
                "url": "https://www.droitcongolais.info/files/830.11.18.8-Decret-du-24-novembre-2018_taux-de-cotisation-cnss.pdf",
                "type": "texte officiel (reproduit par une base juridique en ligne)"
            },
            "arrete-cnss-2018": {
                "titre": "Arrêté ministériel n° 146/CAB/MINETAT/MTEPS/01/2018 du 10 novembre 2018 (cotisations CNSS : renvoie à un décret du Premier ministre pour les taux)",
                "url": "https://www.droitcongolais.info/files/830.11.18.3-Arrete-du-10-novembre-2018_affiliation_cotisations.pdf",
                "type": "officiel (ne contient pas les taux)"
            }
        },
        "consulte_le": "2026-11-15"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mysuite_payroll::{PayrollInput, RuleSet, compute};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    fn input(base: Decimal, dependents: u32) -> PayrollInput {
        PayrollInput {
            base_salary: base,
            allowances: vec![],
            deductions: vec![],
            dependents,
            minimum_wage: None,
        }
    }

    #[test]
    fn template_params_parse_and_compute() {
        let rules: RuleSet = serde_json::from_value(drc_params()).expect("paramètres valides");
        let r = compute(&input(dec!(1000000), 0), &rules).unwrap();
        // CNSS salarié 5 % de 1 000 000 = 50 000 ; assiette 950 000.
        assert_eq!(r.total_employee_contributions, dec!(50000));
        assert_eq!(r.tax_base, dec!(950000));
        // 162 000×3 % + 788 000×15 % = 4 860 + 118 200 = 123 060 → arrondi centaine = 123 100.
        assert_eq!(r.tax, dec!(123100));
        // Patronal 13 % = 130 000.
        assert_eq!(r.total_employer_contributions, dec!(130000));
        assert_eq!(r.net, dec!(1000000) - dec!(50000) - dec!(123100));
    }

    /// Cas chiffré publié par une source secondaire (475 000 CDF/mois, 3 personnes à charge), recalculé sans CNSS :
    /// 51 810 /mois avant réduction ; réduction 6 % → 48 701,40, arrondi ici à la centaine = 48 700.
    #[test]
    fn published_worked_example_is_reproduced_without_social_contributions() {
        let mut p = drc_params();
        p["employee_contributions"] = json!([]);
        p["employer_contributions"] = json!([]);
        p["tax_base_after_employee_contributions"] = json!(false);
        let rules: RuleSet = serde_json::from_value(p).unwrap();
        assert_eq!(
            compute(&input(dec!(475000), 0), &rules).unwrap().tax,
            dec!(51800)
        );
        assert_eq!(
            compute(&input(dec!(475000), 3), &rules).unwrap().tax,
            dec!(48700)
        );
    }

    #[test]
    fn family_reduction_does_not_touch_tax_above_third_bracket() {
        let mut p = drc_params();
        p["employee_contributions"] = json!([]);
        p["employer_contributions"] = json!([]);
        p["tax_base_after_employee_contributions"] = json!(false);
        p["tax"]["max_rate_of_base"] = json!(null);
        p["tax"]["round_to_multiple"] = json!(null);
        let rules: RuleSet = serde_json::from_value(p).unwrap();
        // Base 5 000 000 : 4 860 + 1 638 000×15 % + 1 800 000×30 % + 1 400 000×40 %
        let sans = compute(&input(dec!(5000000), 0), &rules).unwrap().tax;
        let avec = compute(&input(dec!(5000000), 2), &rules).unwrap().tax;
        // Impôt éligible (≤ 3 600 000) = 4 860 + 245 700 + 540 000 = 790 560 ; 2 × 2 % = 4 %.
        assert_eq!(sans - avec, dec!(31622));
    }

    #[test]
    fn katanga_variant_only_changes_family_benefits_rate() {
        let (a, b) = (drc_template(), drc_template_katanga());
        assert_eq!(a["params"]["employer_contributions"][1]["rate"], "0.065");
        assert_eq!(b["params"]["employer_contributions"][1]["rate"], "0.04");
        assert_eq!(a["params"]["tax"], b["params"]["tax"]);
        assert!(b["variante"].as_str().unwrap().contains("confirmer"));
        let rules: RuleSet = serde_json::from_value(b["params"].clone()).unwrap();
        let r = compute(&input(dec!(1000000), 0), &rules).unwrap();
        // 5 % + 4 % + 1,5 % = 10,5 % de 1 000 000.
        assert_eq!(r.total_employer_contributions, dec!(105000));
    }

    #[test]
    fn cnss_lines_carry_the_minimum_wage_floor() {
        let rules: RuleSet = serde_json::from_value(drc_params()).unwrap();
        let mut i = input(dec!(400000), 0);
        i.minimum_wage = Some(dec!(559000));
        let r = compute(&i, &rules).unwrap();
        assert_eq!(r.total_employee_contributions, dec!(27950));
        assert_eq!(r.total_employer_contributions, dec!(72670));
    }

    #[test]
    fn template_exposes_provenance_and_open_points() {
        let t = drc_template();
        assert_eq!(t["country"], "CD");
        assert!(t["provenance"].as_array().unwrap().len() >= 5);
        assert!(!t["a_completer"].as_array().unwrap().is_empty());
        assert!(t["avertissement"].as_str().unwrap().contains("expert"));
    }
}
