# Feuille de route (non livré)

Livré depuis 0.1.0 : contrats et avenants, bulletins PDF.
Priorité haute : **validation du pack RDC par un expert** (plafond CNSS, INPP, ONEM, abattements, ordre réduction/plafond, assiette fiscale, paie en USD) ; WebAuthn et rotation de la clé `MFA_ENCRYPTION_KEY` ; régularisations de paie (rappels) ; export de paiement ; connecteurs de protocoles propriétaires de badgeuses (le lot actuel couvre l'API HTTP générique et l'import CSV), plannings de postes et feuilles de temps ; jours fériés par site dans le calcul des congés ; workflows configurables et notifications (outbox + file avec retries) ; OpenAPI généré (utoipa) ; tests E2E Playwright dans le dépôt ; stockage documentaire S3/MinIO avec URL temporaires (les pièces sont aujourd'hui en base) ; limitation de débit par IP (reverse proxy) ; rapports avancés XLSX/PDF.
Phases 5 à 7 de la spécification : avances/prêts, notes de frais, recrutement, performance, formation, observabilité, sauvegardes testées, tests de charge.
Pack pays `CD` : aucun taux fourni ; à saisir et faire valider par un expert (voir `docs/business-rules/payroll.md`).
