# Déploiement

1. Serveur Linux avec Docker. `cp .env.example .env` et définir des secrets forts ; `COOKIE_SECURE=true`.
2. Placer le service `web` derrière un TLS (Caddy/Nginx avec certificat) : le cookie `Secure` exige HTTPS. Transmettre `X-Forwarded-For` ; appliquer la limitation de débit sur `/api/v1/auth/login` au proxy.
3. `docker compose up --build -d`, puis `mysuite create-admin`. Les migrations s'appliquent au démarrage (`RUN_MIGRATIONS=true`).
4. Surveillance : `/health/ready`, logs JSON de l'API.
Ne pas exposer PostgreSQL publiquement. Ne jamais lancer `seed-demo` en production.
