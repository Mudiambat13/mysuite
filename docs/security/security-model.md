# Modèle de sécurité

- **Authentification** : Argon2id, mot de passe ≥ 12 caractères avec lettres et chiffres, verrouillage 15 min après 5 échecs, temps de réponse égalisé pour les emails inconnus, journal des connexions (`login_events`), liste/révocation des sessions, changement de mot de passe révoquant les autres sessions.
- **Sessions** : voir ADR 0002. Durée configurable (`SESSION_TTL_HOURS`).
- **Autorisation** : matrice rôle→permissions (`crates/shared/src/permissions.rs`), contrôlée dans chaque handler ; les gardes du frontend ne sont que cosmétiques. Un manager n'agit que sur ses subordonnés directs ; nul ne valide sa propre demande ; un bulletin n'est lisible que par son titulaire (paie clôturée non simulée) ou par la paie/audit, sinon 404.
- **Isolation** : ADR 0003, testée (`tenants_are_strictly_isolated`).
- **Intégrité** : audit append-only (trigger), paie figée (triggers), contraintes SQL.
- **Entrées** : validation serveur, SQL paramétré, recherche avec échappement des jokers, export CSV neutralisant les formules, taille de corps limitée à 2 Mo, en-têtes de sécurité.
- **Contrats** : lecture/gestion réservées aux rôles RH (lecture seule pour l'auditeur) ; un employé ne voit pas son propre contrat via l'API à ce stade.
- **Données sensibles** : consultation de la rémunération, des bulletins et leur téléchargement PDF auditées ; pas de données personnelles dans les logs applicatifs.
- **Non couvert** : limitation de débit par IP (à faire au reverse proxy), antivirus/upload, chiffrement applicatif des autres données, RLS PostgreSQL, réinitialisation de mot de passe en libre-service par courriel (pas de SMTP). Voir `ROADMAP.md`.

## MFA (TOTP) et réinitialisation de mot de passe
- **TOTP** : RFC 6238 (SHA‑1, 6 chiffres, pas de 30 s), tolérance ±1 pas, un code ne peut servir qu'une fois (pas strictement croissant, `last_used_step`). Vérifié contre les vecteurs de test de la RFC.
- **Secret** : 160 bits aléatoires, chiffré AES‑256‑GCM (nonce aléatoire, données associées = id utilisateur) avec `MFA_ENCRYPTION_KEY` (32 octets hex). Sans clé, le MFA est refusé proprement. Perdre la clé rend les secrets inutilisables : un administrateur réinitialise alors le MFA des comptes. Pas de rotation de clé automatisée (voir ROADMAP).
- **Codes de secours** : 10, usage unique, stockés hachés (SHA‑256, entropie suffisante), affichés une seule fois, régénérables (les anciens sont invalidés).
- **Connexion** : le mot de passe seul ne crée aucune session ; il produit un défi (jeton opaque, 5 min, 5 essais). Les échecs du second facteur comptent pour le verrouillage du compte (5 échecs → 15 min) et ne sont pas remis à zéro par un mot de passe correct.
- **Actions sensibles** : activer exige le mot de passe puis un code valide ; désactiver et régénérer exigent mot de passe **et** code (TOTP ou secours).
- **Réinitialisation du MFA par un administrateur** (`UsersManage`) : même société uniquement, jamais sur soi-même, sessions de la cible révoquées, journalisé (`mfa.admin_reset`). Un utilisateur d'une autre société est vu comme inexistant (404).
- **Lien de réinitialisation de mot de passe** (administrateur) : jeton 256 bits, haché en base, 1 h, usage unique, un seul lien valide par utilisateur ; le consommer change le mot de passe, révoque toutes les sessions, remet le compteur d'échecs à zéro. Réponse d'erreur identique pour jeton inconnu, expiré ou déjà utilisé. Interdit sur soi-même et sur les administrateurs de plateforme. Le lien est remis par l'administrateur hors application : à transmettre par un canal fiable. Événements : `user.password_reset_issued`, `user.password_reset`.
- **Limites** : le MFA n'est pas obligatoire (pas de politique par société) ; la réinitialisation du MFA/mot de passe par un administrateur repose sur la confiance accordée à ce rôle ; pas de WebAuthn/clés de sécurité.



## Pointage

Heure serveur uniquement (anti-falsification côté client), corrections motivées et auditées avec avant/après, séparation des tâches (nul ne corrige son propre pointage), visibilité par rôle (manager : subordonnés directs), isolation par `tenant_id` (clés étrangères composites).

### Badgeuses

Authentification appareil par clé `Bearer` (256 bits, hachée, révocable), isolée par société ; l'appareil n'a aucun autre droit. L'ingestion est idempotente, l'heure d'un passage doit porter un fuseau (ou être lue dans le fuseau de la société pour un CSV) et le journal brut conserve tout passage reçu. Pas de limitation de débit applicative : à placer au niveau du reverse proxy (voir `ROADMAP.md`).
