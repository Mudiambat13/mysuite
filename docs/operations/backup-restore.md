# Sauvegarde et restauration

```bash
# Sauvegarde (chiffrer et copier hors serveur)
docker compose exec -T db pg_dump -U "$POSTGRES_USER" -Fc "$POSTGRES_DB" > mysuite-$(date +%F).dump
# Restauration sur une base vide
docker compose exec -T db pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --clean --if-exists < mysuite-AAAA-MM-JJ.dump
```
**Une restauration doit être testée régulièrement** : ce test n'a pas été automatisé dans ce dépôt. Les sauvegardes contiennent des données salariales : chiffrement et accès restreint obligatoires.
