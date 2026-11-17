# ADR 0003 — Isolation multi-société

Le `tenant_id` provient exclusivement de la session (appartenance relue en base). Les tables métier portent `tenant_id`, et les références croisées utilisent des clés étrangères composites `(id, tenant_id)` : la base refuse un rattachement inter-société même en cas d'oubli applicatif. Une ressource d'une autre société renvoie 404. Piste ultérieure : Row-Level Security PostgreSQL en défense supplémentaire.
