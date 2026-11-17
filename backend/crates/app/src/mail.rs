//! Envoi de courriels (SMTP), facultatif : sans `SMTP_HOST`, rien n'est envoyé et le lien reste à transmettre à la main.

use anyhow::{Context, Result};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
    /// Adresse publique de l'application (ex. https://rh.exemple.cd), sans barre finale.
    pub base_url: String,
    /// `true` : STARTTLS (587), `false` : TLS implicite (465).
    pub starttls: bool,
}

// Debug manuel : le mot de passe SMTP ne doit jamais apparaître dans les journaux.
impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("from", &self.from)
            .finish_non_exhaustive()
    }
}

impl SmtpConfig {
    /// `None` si `SMTP_HOST` est absent. Erreur si la configuration est incomplète.
    pub fn from_env() -> Result<Option<Self>> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let Some(host) = get("SMTP_HOST") else {
            return Ok(None);
        };
        let starttls = get("SMTP_TLS").as_deref() != Some("implicit");
        Ok(Some(Self {
            host,
            port: get("SMTP_PORT")
                .map(|p| p.parse().context("SMTP_PORT invalide"))
                .transpose()?
                .unwrap_or(if starttls { 587 } else { 465 }),
            username: get("SMTP_USERNAME"),
            password: get("SMTP_PASSWORD"),
            from: get("SMTP_FROM").context("SMTP_FROM requis quand SMTP_HOST est défini")?,
            base_url: get("APP_BASE_URL")
                .context("APP_BASE_URL requis quand SMTP_HOST est défini")?
                .trim_end_matches('/')
                .to_string(),
            starttls,
        }))
    }
}

/// Corps du courriel de réinitialisation (texte brut, aucune donnée sensible hors du lien).
pub fn reset_body(company: &str, link: &str, minutes: i64) -> String {
    format!(
        "Bonjour,\n\nUn administrateur de {company} a demandé la réinitialisation de votre mot de passe MySuite.\n\
         Ouvrez ce lien pour choisir un nouveau mot de passe (valable {minutes} minutes, utilisable une seule fois) :\n\n{link}\n\n\
         Si vous n'êtes pas à l'origine de cette demande, ne cliquez pas et prévenez votre administrateur.\n"
    )
}

pub async fn send_reset(
    cfg: &SmtpConfig,
    to: &str,
    company: &str,
    path: &str,
    minutes: i64,
) -> Result<()> {
    let link = format!("{}{}", cfg.base_url, path);
    let msg = Message::builder()
        .from(cfg.from.parse().context("SMTP_FROM invalide")?)
        .to(to.parse().context("adresse destinataire invalide")?)
        .subject("MySuite : réinitialisation de votre mot de passe")
        .header(ContentType::TEXT_PLAIN)
        .body(reset_body(company, &link, minutes))?;
    let builder = if cfg.starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?
    };
    let mut builder = builder.port(cfg.port);
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        builder = builder.credentials(Credentials::new(u.clone(), p.clone()));
    }
    builder.build().send(msg).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_password() {
        let c = SmtpConfig {
            host: "h".into(),
            port: 587,
            username: Some("u".into()),
            password: Some("secret-pw".into()),
            from: "a@b.cd".into(),
            base_url: "https://x".into(),
            starttls: true,
        };
        assert!(!format!("{c:?}").contains("secret-pw"));
    }

    #[test]
    fn body_contains_link_and_validity_only() {
        let b = reset_body("Société X", "https://rh.test/reset-password?token=abc", 60);
        assert!(b.contains("https://rh.test/reset-password?token=abc"));
        assert!(b.contains("60 minutes"));
        assert!(b.contains("Société X"));
    }
}
