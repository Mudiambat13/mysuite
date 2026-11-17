# Règles du pointage

Aucune règle légale n'est codée en dur : tous les seuils sont des **paramètres de la société** (`attendance_settings`).

- **Heure** : celle du serveur pour les pointages libre-service ; l'heure envoyée par un client est ignorée.
- **Un pointage ouvert au plus** par employé (index unique partiel) ; seuls les employés `active` pointent.
- **Pointage oublié** : une sortie plus de 16 h après l'entrée est refusée → correction RH.
- **Corrections/saisies RH** (`attendance_manage` : admin RH, gestionnaire RH) : motif obligatoire, pas de chevauchement, pas dans le futur, 24 h maximum par entrée ; journal d'audit avec avant/après. **Personne ne corrige son propre pointage.**
- **Visibilité** : l'employé voit les siens ; le manager ses subordonnés directs ; RH, paie et auditeur toute la société (lecture seule pour paie et auditeur).
- **Jour de présence** : date locale (fuseau de la société) de la première entrée ; un poste de nuit est compté le jour où il commence.
- **Retard** : première entrée du jour après `heure de début + tolérance`.
- **Au-delà de la durée standard** : minutes travaillées dépassant la durée journalière configurée, par jour. **Indicatif** : ne génère ni heures supplémentaires légales ni éléments de paie.
- **Pointage ouvert** : ses minutes ne sont pas comptées tant que la sortie manque ; il est signalé (« ouverts »).
- Export CSV : BOM UTF-8, cellules neutralisées contre l'injection de formules, audité.

## Badgeuses (ingestion)

- **Deux voies** : API appareil (`POST /api/v1/device/punches`, `Authorization: Bearer <clé>`, passages en RFC 3339 **avec décalage**, 500 par envoi) et import CSV (2 000 lignes, simulation possible, heures sans fuseau lues dans le fuseau de la société, dates `JJ/MM/AAAA` ou `AAAA-MM-JJ`).
- **Clé d'appareil** : 256 bits aléatoires, affichée une seule fois, seule l'empreinte SHA-256 est stockée ; révocable immédiatement ; ne donne accès à rien d'autre qu'à l'envoi de passages pour sa société. Pas de cookie, donc pas de CSRF.
- **Identifiant** : le code de badge associé à un employé ; à défaut, le matricule.
- **Idempotence** : un passage (identifiant, instant) déjà reçu est ignoré ; un double badgeage à moins de 60 s aussi.
- **Sens** : explicite (entrée/sortie) ou bascule automatique (ouvre s'il n'y a pas de pointage ouvert, sinon ferme).
- **Refus conservés** (jamais perdus) : badge inconnu, employé inactif, passage dans le futur ou trop ancien (31 jours pour un appareil, 366 pour un import), entrée dans un pointage existant, sortie qui en recouvrirait un autre, pointage ouvert depuis plus de 16 h, pointage plus récent encore ouvert. Après correction (badge associé, pointage fermé par les RH), « Retraiter » les rejoue dans l'ordre chronologique.
- **Audit** : `attendance.device_ingest` (sans utilisateur), `attendance.import` (imports réels seulement), création/révocation d'appareil, association/retrait de badge, retraitement.
- **Hors périmètre** : protocoles propriétaires (ZKTeco, etc.) ; une passerelle peut les traduire vers l'API HTTP ci-dessus.
