# Changelog

## 0.6.0
- MFA obligatoire par société (`0013_mfa_policy.sql`) : `GET/PUT /security/policy`, l'administrateur doit avoir son propre MFA ; les membres sans MFA sont bloqués (`403 mfa_enrollment_required`) hors configuration du MFA, avec redirection dans l'interface ; audité (`tenant.mfa_policy`).
- Pièces des employés (`0012_employee_documents.sql`) : dépôt PDF/PNG/JPEG ≤ 5 Mo vérifié sur le contenu, échéances, téléchargement et suppression audités ; nouvelles permissions `documents_read` / `documents_manage` réservées aux rôles RH.
- Modèles de contrat par société (texte fourni par la société, champs de fusion contrôlés, salaire jamais fusionné) et génération PDF d'un contrat (`0011_contract_templates.sql`) ; audités.
- Envoi facultatif du lien de réinitialisation par courriel (SMTP, `lettre`) : désactivé par défaut, l'échec d'envoi n'empêche pas l'émission du lien manuel ; audit `user.password_reset_emailed` ; le mot de passe SMTP n'apparaît jamais dans les journaux.
- Grille SMIG : import CSV (vérification à blanc puis import, source officielle obligatoire, montant mensuel lu tel quel), versionnée par date d'effet et par société (migration `0010_wage_grid.sql`).
- Classe SMIG par employé avec historique daté (`/employees/{id}/wage-class`).
- Rapport de conformité (`/reports/smig-compliance` et `.csv`) : salaires sous le minimum signalés, **jamais bloquants** ; statuts classe/salaire/grille manquants et devises différentes.
- Moteur : option `floor_to_minimum_wage` sur une cotisation (base CNSS non inférieure au SMIG de la classe, décret 18/041 art. 8), expliquée sur le bulletin ; sans classe ni grille, aucun plancher.
- Modèle RDC : plancher activé sur les lignes CNSS ; variante ex-Katanga (prestations familiales 4 %, durée du taux transitoire non établie, à confirmer).
- Écran « Grille SMIG et conformité » et carte « Classe » sur la fiche employé.

## 0.5.0
- Pack paie RDC : modèle de jeu de règles **brouillon** (CDF, mensuel) avec provenance par paramètre, niveau de confiance, points à compléter et sources liées ; jamais approuvé automatiquement ; avertissement si la devise de la société diffère. Valeurs issues de sources secondaires, à faire confirmer par un expert (IPR remplacé par l'IRPP au 01/01/2026 selon ces sources).
- Moteur de paie : réduction d'impôt pour personnes à charge, plafond de l'impôt en % de l'assiette, arrondi à un multiple, explication de chaque ajustement sur le bulletin (écran et PDF). Comportement inchangé pour les jeux de règles existants.
- Personnes à charge par employé avec historique daté (`/employees/{id}/dependents`, migration `0009_tax_dependents.sql`), accès comme la rémunération, audité.

## 0.4.0
- Documentation : site MkDocs Material (guides par rôle et module, règles métier, référence API/permissions/base/moteur de paie, sécurité, exploitation, développement), build strict, publication GitHub Pages via GitHub Actions (`.github/workflows/docs.yml`).
- Sécurité du compte : MFA TOTP (RFC 6238, SHA‑1, 6 chiffres, 30 s, tolérance ±1 pas, anti-rejeu), secret chiffré au repos (AES‑256‑GCM, clé `MFA_ENCRYPTION_KEY`), 10 codes de secours à usage unique stockés hachés, connexion en deux étapes (défi de 5 min, 5 essais, échecs comptés pour le verrouillage du compte), activation/désactivation/régénération soumises au mot de passe (et au code pour les deux dernières), réinitialisation du MFA par un administrateur (même société, jamais sur soi-même, sessions révoquées, audit).
- Réinitialisation de mot de passe émise par un administrateur (aucun SMTP : lien à usage unique, 1 h, haché, un seul lien valide par utilisateur ; l'utiliser ferme toutes les sessions et lève le verrouillage ; message d'erreur unique). Pages « Sécurité » (QR code généré localement), connexion en deux étapes, `/reset-password`, actions sur la page Utilisateurs. Migration `0008_account_security.sql`.
- **Changement d'API** : `POST /auth/login` peut répondre `{mfa_required, mfa_token}` au lieu du profil quand le MFA est actif.

## 0.3.0
- Badgeuses : API appareil (`POST /api/v1/device/punches`, clé `Bearer` hachée, révocable, idempotente), import CSV avec simulation, correspondance badge → employé (repli sur le matricule), journal brut des passages (inconnus/refusés conservés et retraitables). Migration `0007_timeclock.sql`.
- Pointage : entrée/sortie en libre-service (heure du serveur), corrections et saisies RH avec motif obligatoire et audit avant/après, impossibilité de corriger son propre pointage, résumé par employé (jours, heures, retards, heures au-delà de la durée standard indicatives) et export CSV audité ; paramètres par société (fuseau, heure de début, tolérance, durée standard). Migration `0006_attendance.sql`.
- Rapports : mouvements (entrées/sorties), absences approuvées (par type et département), masse salariale (paies clôturées non simulées uniquement, permission paie requise) ; filtres, graphiques, exports CSV audités (BOM UTF-8, neutralisation des formules).
- Graphiques interactifs (composant `components/charts.tsx`) : infobulles, légende cliquable pour masquer/afficher une série, bascule barres/courbe/aires, clic sur un point pour son détail ; tableau de bord avec anneau cliquable et changement de dimension (département, site, contrat, statut). Vérifié dans le navigateur par `tools/capture-charts.cjs`.
- Thème : sélecteur Clair / Système / Sombre (en-tête et page de connexion), « Système » suit le réglage de l'OS en direct, choix mémorisé par navigateur ; infobulles et axes des graphiques adaptés au sombre. Vérifié par `tools/capture-themes.cjs`.
- Interface mobile : menu en tiroir sous 768 px, en-tête compact, aucun débordement horizontal sur les pages vérifiées.

## 0.2.0
- Contrats : création (un seul contrat actif par employé, CDD avec date de fin), avenants (historique conservé, prolongation de la date de fin), résiliation, alertes d'échéance et de fin de période d'essai ; réservé RH et auditeur (lecture).
- Bulletins PDF : génération sans dépendance externe, mêmes contrôles d'accès que l'écran, téléchargement audité (`payslip.download`), mention SIMULATION si la paie n'est pas approuvée.
- Correctif : pagination par query string (`?page=&limit=`) sur employés et audit.

## 0.1.0 — socle initial
- Phase 0 : monorepo, Docker Compose, migrations PostgreSQL, configuration par variables d'environnement, logs JSON, health checks (`/health`, `/health/ready`), CI.
- Phase 1 (partielle) : multi-société, authentification par sessions, RBAC, audit append-only, layout React.
- Phase 2 (partielle) : organisation, dossiers employés, rémunération, export CSV.
- Phase 3 (partielle) : congés avec soldes et approbation manager.
- Phase 4 (partielle) : moteur de paie Decimal versionné, simulation/validation/clôture, bulletins consultables à l'écran.
