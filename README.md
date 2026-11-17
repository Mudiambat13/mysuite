# MySuite — Suite RH multi-société

Plateforme RH modulaire : backend **Rust** (Axum, SQLx, PostgreSQL) et frontend **React + TypeScript**.
Ce dépôt livre la **Phase 0 (fondations)** et un socle vertical fonctionnel des phases 1 à 4. Voir [`CHANGELOG.md`](CHANGELOG.md) et la section *Limites connues*.

## Aperçu

Captures réelles de l'application avec les **données de démonstration fictives** (société « Société Démo », valeurs de paie inventées). Elles se régénèrent avec `tools/capture-screenshots.cjs`.

| | |
|---|---|
| **Connexion** ![Connexion](docs/screenshots/01-connexion.png) | **Tableau de bord RH** — KPI et répartition de l'effectif ![Tableau de bord](docs/screenshots/02-tableau-de-bord.png) |
| **Employés** — recherche, filtres, pagination, export CSV ![Employés](docs/screenshots/03-employes.png) | **Fiche employé** — rémunération (accès restreint, consultation tracée) ![Rémunération](docs/screenshots/04-fiche-employe-remuneration.png) |
| **Contrats** — échéances, essais, avenants ![Contrats](docs/screenshots/05-contrats.png) | **Journal d'audit** — ajout seul ![Audit](docs/screenshots/10-audit.png) |
| **Congés (manager)** — approbation des demandes de l'équipe ![Congés manager](docs/screenshots/06-conges-manager.png) | **Congés (employé)** — soldes et demande ![Congés employé](docs/screenshots/07-conges-employe.png) |
| **Paie** — périodes, exécutions, jeux de règles versionnés ![Paie](docs/screenshots/08-paie.png) | **Mode sombre** ![Mode sombre](docs/screenshots/11-mode-sombre.png) |
| **Rapports** — mouvements ![Rapports](docs/screenshots/12-rapports-mouvements.png) | **Rapports** — absences ![Absences](docs/screenshots/13-rapports-absences.png) |
| **Graphique interactif** — anneau, survol, dimension ![Anneau](docs/screenshots/18-graphique-anneau.png) | **Rapports interactifs** — infobulle, sélection, légende ![Interactif](docs/screenshots/20-rapports-interactif.png) |
| **Mode sombre** — rapports ![Rapports sombre](docs/screenshots/21-rapports-sombre.png) | **Mode sombre** — tableau de bord ![Dashboard sombre](docs/screenshots/22-tableau-de-bord-sombre.png) |
| **Pointage** — employé en poste ![Pointage](docs/screenshots/23-pointage-en-poste.png) | **Pointage** — présences, retards, détail ![Présences](docs/screenshots/24-pointage-equipe.png) |
| **Pointage** — correction RH motivée ![Correction](docs/screenshots/25-pointage-correction.png) | **Pointage** — mobile ![Mobile](docs/screenshots/26-pointage-mobile.png) |
| **Badgeuse** — clé d'appareil affichée une seule fois ![Clé](docs/screenshots/27-badgeuse-cle.png) | **Badgeuse** — import CSV avec simulation ![Import](docs/screenshots/28-badgeuse-import-simulation.png) |
| **Sécurité** — activation du MFA (QR code) ![MFA](docs/screenshots/30-securite-activation.png) | **Connexion** — second facteur ![Connexion MFA](docs/screenshots/32-connexion-mfa.png) |
| **Utilisateurs** — lien de réinitialisation à usage unique ![Lien](docs/screenshots/33-lien-reinitialisation.png) | **Sécurité** — codes de secours affichés une fois ![Codes](docs/screenshots/31-securite-codes-secours.png) |
| **Mobile** — tableau de bord ![Mobile](docs/screenshots/14-mobile-tableau-de-bord.png) | **Mobile** — menu ![Menu](docs/screenshots/15-mobile-menu.png) |

**Paie : calcul explicable et bulletin** (paie en *simulation*, car le jeu de règles de démonstration n'est pas approuvable) :

![Détail de paie et bulletin](docs/screenshots/09-paie-detail-calcul.png)

## Ce qui fonctionne aujourd'hui

| Domaine | Livré |
|---|---|
| Multi-société | Isolation par `tenant_id` côté serveur, clés étrangères composites, sociétés créées par un admin plateforme, bascule de société |
| Authentification | Sessions serveur (jeton opaque, hash SHA-256 en base), cookie `HttpOnly`/`SameSite=Strict`, Argon2id, verrouillage après 5 échecs, MFA TOTP + codes de secours, réinitialisation de mot de passe par lien administrateur, révocation d'appareils, protection CSRF par en-tête |
| RBAC | 6 rôles, permissions vérifiées dans chaque handler |
| Organisation | Sites, départements, postes |
| Employés | Fiche, recherche/filtres/pagination, export CSV (anti-injection de formules), historique salarial, indemnités/retenues récurrentes |
| Congés | Types paramétrables, soldes, demandes, contrôle des chevauchements et du solde, approbation par le manager direct, annulation |
| Paie | Moteur pur en `Decimal`, règles versionnées, simulation / validation / clôture, lignes figées en base, bulletins avec détail de calcul |
| Contrats | Contrats, avenants, résiliation, alertes d'échéance et de période d'essai |
| Bulletins | Consultation à l'écran et **PDF** (texte, A4) avec contrôle d'accès et audit |
| Audit | Journal en ajout seul (triggers SQL) |
| UI | Navigation latérale, thème clair/sombre, tableaux, états vide/chargement/erreur, français (i18n prêt) |

## Documentation

Site complet (MkDocs Material) dans [`docs/`](docs/index.md), publié sur GitHub Pages par [`docs.yml`](.github/workflows/docs.yml) : guides utilisateur, règles métier, API, sécurité, exploitation, développement. Voir [Publier la documentation](docs/developpement/documentation.md).

## Pack paie RDC

Un **modèle de règles RDC en brouillon** (IRPP ex-IPR, CNSS, personnes à charge) est fourni avec ses sources et ses points ouverts. Il provient de **sources secondaires** et doit être **confirmé par un expert** avant toute paie réelle : voir [Pack paie RDC](docs/guide/paie-rdc.md). Une **grille SMIG** importable (classe par employé, rapport de conformité non bloquant, plancher de la base CNSS) complète le pack : voir [Grille SMIG](docs/guide/smig.md).

## Démarrage rapide (Docker)

```bash
cp .env.example .env            # puis changer les mots de passe
sed -i "s/^MFA_ENCRYPTION_KEY=.*/MFA_ENCRYPTION_KEY=$(openssl rand -hex 32)/" .env   # active le MFA (à sauvegarder !)
docker compose up --build -d
# Créer l'administrateur plateforme (mot de passe lu depuis MYSUITE_ADMIN_PASSWORD) :
docker compose exec api mysuite create-admin admin@exemple.test "Admin Plateforme"
# Facultatif — données de démonstration FICTIVES :
docker compose exec api mysuite seed-demo
```

Ouvrir <http://localhost:8081>. En local HTTP, laisser `COOKIE_SECURE=false` ; **en production, `true` + HTTPS**.

Comptes de démo (mot de passe `Demo-Passw0rd-2026`, **à ne jamais utiliser en production**) :
`rh@demo.test`, `paie1@demo.test`, `paie2@demo.test`, `manager@demo.test`, `employe@demo.test`, `audit@demo.test`.

## Développement sans Docker

Prérequis : Rust stable, Node 22, PostgreSQL 16.

```bash
# Base
createdb mysuite
export MFA_ENCRYPTION_KEY=$(openssl rand -hex 32)  # sans elle, le MFA est indisponible
export DATABASE_URL=postgres://mysuite:mysuite@localhost:5432/mysuite COOKIE_SECURE=false BIND_ADDR=127.0.0.1:8080

# API
cd backend
cargo run -p mysuite-app -- seed-demo        # données fictives (optionnel)
cargo run -p mysuite-app                     # http://127.0.0.1:8080

# Frontend (autre terminal) — proxifie /api vers :8080
cd frontend && npm install && npm run dev    # http://127.0.0.1:5173
```

## Qualité

```bash
cd backend  && cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --workspace
cd frontend && npm run lint && npm run typecheck && npm test && npm run build
```

Les tests d'intégration backend utilisent `sqlx::test` : une base PostgreSQL éphémère par test (`DATABASE_URL` requis, rôle autorisé à créer des bases).

## Paie : avertissement important

MySuite **ne fournit aucun taux ni barème légal** (CNSS, IPR ou autre). Les jeux de règles sont saisis, datés et versionnés ; tant qu'un jeu n'est pas **approuvé** (source officielle, date de vérification, second utilisateur), toute paie reste en **simulation** : elle ne peut être ni validée, ni clôturée, et les bulletins ne sont pas visibles des employés. Le jeu de démonstration est explicitement fictif et non approuvable. Faire valider les paramètres par un expert comptable/fiscal local.

## Documentation

- [`docs/architecture/overview.md`](docs/architecture/overview.md) et [`decisions/`](docs/architecture/decisions/)
- [`docs/security/security-model.md`](docs/security/security-model.md)
- [`docs/business-rules/payroll.md`](docs/business-rules/payroll.md), [`leave.md`](docs/business-rules/leave.md)
- [`docs/api/openapi.md`](docs/api/openapi.md)
- [`docs/operations/deployment.md`](docs/operations/deployment.md), [`backup-restore.md`](docs/operations/backup-restore.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md), [`ROADMAP.md`](ROADMAP.md)

## Limites connues

- Interface mobile : menu en tiroir et absence de débordement vérifiés sur le tableau de bord, les employés et les rapports ; les autres écrans (tableaux larges, formulaires de paie) défilent horizontalement mais n'ont pas été retouchés un par un.
- Pointage : heure serveur, un pointage ouvert au plus par employé, corrections RH motivées et auditées ; les « heures au-delà de la durée standard » sont **indicatives** et n'alimentent pas la paie ; pas de planning de postes, de pauses ni de jours fériés ; une journée est rattachée à la date locale de l'entrée (un poste de nuit est compté le jour de son début).
- Rapports : masse salariale ventilée par département *actuel* ; exports CSV uniquement (pas de XLSX).

Voir [`ROADMAP.md`](ROADMAP.md) : MFA obligatoire par politique, WebAuthn, réinitialisation de mot de passe par courriel (aucun SMTP), stockage de documents (fichiers de contrat, pièces jointes), modèles de contrats et génération du PDF de contrat, feuilles de temps, protocoles propriétaires de badgeuses (ZKTeco, etc. — seule l'API HTTP générique et l'import CSV sont livrés), workflows configurables, notifications, prêts, notes de frais, recrutement, performance, OpenAPI généré, tests E2E Playwright versionnés et audit de dépendances exécuté en CI ne sont **pas** encore livrés.
