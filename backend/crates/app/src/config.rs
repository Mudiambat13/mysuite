use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// Cookie `Secure` : à laisser à `true` en production (HTTPS).
    pub cookie_secure: bool,
    pub session_ttl_hours: i64,
    pub run_migrations: bool,
    /// Origine du frontend autorisée (CORS), utile en développement avec un port différent.
    pub cors_origin: Option<String>,
    /// Exige qu'un jeu de règles de paie soit approuvé par un autre utilisateur que son créateur.
    pub payroll_four_eyes: bool,
    /// Clé AES-256 (64 caractères hexadécimaux) qui chiffre les secrets TOTP au repos. Sans elle, l'activation du MFA est refusée.
    pub mfa_key: Option<[u8; 32]>,
    /// Envoi de courriels ; `None` = désactivé.
    pub smtp: Option<crate::mail::SmtpConfig>,
}

fn flag(name: &str, default: bool) -> bool {
    env::var(name)
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(default)
}

fn parse_key(v: Option<&str>) -> Result<Option<[u8; 32]>> {
    let Some(v) = v.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let bytes = hex::decode(v.trim()).context("MFA_ENCRYPTION_KEY doit être hexadécimale")?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| {
        anyhow::anyhow!("MFA_ENCRYPTION_KEY doit faire 32 octets (64 caractères hexadécimaux)")
    })?;
    Ok(Some(key))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL").context("DATABASE_URL manquante")?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            cookie_secure: flag("COOKIE_SECURE", true),
            session_ttl_hours: env::var("SESSION_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),
            run_migrations: flag("RUN_MIGRATIONS", true),
            cors_origin: env::var("CORS_ORIGIN").ok().filter(|s| !s.is_empty()),
            payroll_four_eyes: flag("PAYROLL_FOUR_EYES", true),
            mfa_key: parse_key(env::var("MFA_ENCRYPTION_KEY").ok().as_deref())?,
            smtp: crate::mail::SmtpConfig::from_env()?,
        })
    }

    /// Configuration de test : cookie non sécurisé, quatre yeux actifs.
    pub fn for_tests() -> Self {
        Self {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".into(),
            cookie_secure: false,
            session_ttl_hours: 1,
            run_migrations: false,
            cors_origin: None,
            payroll_four_eyes: true,
            mfa_key: Some([7u8; 32]),
            smtp: None,
        }
    }
}
