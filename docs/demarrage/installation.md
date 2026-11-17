# Installation

## Avec Docker (recommandé)

Prérequis : Docker et Docker Compose.

```bash
git clone <votre-depot> mysuite && cd mysuite
cp .env.example .env            # puis changer TOUS les secrets
# Clé de chiffrement des secrets MFA (à sauvegarder !)
sed -i "s/^MFA_ENCRYPTION_KEY=.*/MFA_ENCRYPTION_KEY=$(openssl rand -hex 32)/" .env
docker compose up --build -d

# Administrateur de plateforme (mot de passe lu depuis MYSUITE_ADMIN_PASSWORD)
docker compose exec api mysuite create-admin admin@exemple.test "Admin Plateforme"

# Facultatif — données de démonstration FICTIVES (jamais en production)
docker compose exec api mysuite seed-demo
```

L'interface est disponible sur <http://localhost:8081>. Les migrations s'appliquent au démarrage (`RUN_MIGRATIONS=true`).

!!! danger "Production"
    Placez le service `web` derrière un reverse proxy TLS, mettez `COOKIE_SECURE=true`, changez tous les mots de passe et n'exposez jamais PostgreSQL. Détails : [Déploiement](../operations/deployment.md).

## Services

| Service | Rôle | Port par défaut |
|---|---|---|
| `db` | PostgreSQL 16 (volume `dbdata`) | interne |
| `api` | API Rust, migrations au démarrage | interne (8080) |
| `web` | Frontend statique + reverse proxy `/api` | 8081 |

## Commandes de l'exécutable `mysuite`

| Commande | Effet |
|---|---|
| `mysuite` | Démarre l'API |
| `mysuite migrate` | Applique les migrations puis quitte |
| `mysuite create-admin <email> <nom>` | Crée l'administrateur de plateforme (`MYSUITE_ADMIN_PASSWORD`) |
| `mysuite seed-demo` | Crée une société fictive avec comptes et données de démonstration |

## Sans Docker

Voir [Environnement de développement](../developpement/environnement.md).

## Vérifier l'installation

```bash
docker compose ps                               # db, api, web : "running"
docker compose logs api | tail                  # migrations appliquées, "listening"
docker compose exec api wget -qO- http://127.0.0.1:8080/health/ready
```

`/health` (liveness) et `/health/ready` (base joignable) sont exposées par l'API sur son port interne ; adaptez la sonde à votre proxy ([Déploiement](../operations/deployment.md)).
