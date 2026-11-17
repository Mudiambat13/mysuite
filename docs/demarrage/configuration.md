# Configuration

Toute la configuration passe par des **variables d'environnement** (fichier `.env` avec Docker Compose).

| Variable | Défaut | Description |
|---|---|---|
| `DATABASE_URL` | *obligatoire* | URL PostgreSQL de l'API |
| `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` | — | Initialisation du conteneur PostgreSQL |
| `BIND_ADDR` | `0.0.0.0:8080` | Adresse d'écoute de l'API |
| `COOKIE_SECURE` | `true` (code) ; `false` dans `.env.example` | Cookie de session `Secure`. `false` uniquement en développement HTTP |
| `SESSION_TTL_HOURS` | `12` | Durée de vie d'une session |
| `RUN_MIGRATIONS` | `true` | Applique les migrations au démarrage |
| `PAYROLL_FOUR_EYES` | `true` | Le créateur d'un jeu de règles de paie ne peut pas l'approuver |
| `CORS_ORIGIN` | vide | Origine CORS autorisée ; laisser vide si frontend et API partagent l'origine |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM`, `APP_BASE_URL` | vides | Envoi du lien de réinitialisation par courriel. Sans `SMTP_HOST`, rien n'est envoyé. `SMTP_FROM` et `APP_BASE_URL` sont requis si `SMTP_HOST` est défini ; `SMTP_TLS=implicit` pour TLS implicite (465), sinon STARTTLS (587) |
| `MFA_ENCRYPTION_KEY` | vide | 32 octets en hexadécimal (`openssl rand -hex 32`). Sans elle, le MFA est indisponible |
| `MYSUITE_ADMIN_PASSWORD` | — | Mot de passe lu par `create-admin` |
| `RUST_LOG` | `info,sqlx=warn` | Niveau de logs (JSON) |

!!! warning "Clé MFA"
    Perdre `MFA_ENCRYPTION_KEY` rend les secrets MFA existants inutilisables : un administrateur doit alors [réinitialiser le MFA](../guide/utilisateurs.md#reinitialiser-le-mfa) des comptes concernés. Sauvegardez-la séparément de la base. Il n'existe pas encore de rotation automatisée.

## Paramètres par société (dans l'application)

| Paramètre | Où | Contenu |
|---|---|---|
| Pointage | Pointage → Paramètres | Fuseau IANA, heure de début, tolérance de retard, durée standard |
| Types de congé | Congés | Droits annuels, avec ou sans solde |
| Règles de paie | Paie | Jeux de règles versionnés et approuvés |

## Reverse proxy

- Transmettre `X-Forwarded-For` (journal des connexions).
- Appliquer la **limitation de débit** sur `/api/v1/auth/login` et `/api/v1/auth/login/mfa` (non fournie par l'application).
- HTTPS obligatoire en production.
