# Publier la documentation

Le site est construit avec **MkDocs Material** et publié sur **GitHub Pages** par **GitHub Actions** (`.github/workflows/docs.yml`).

## Activer GitHub Pages (une seule fois)

1. Pousser le dépôt sur GitHub (branche `main`).
2. **Settings → Pages → Build and deployment → Source : GitHub Actions**.
3. Pousser une modification dans `docs/` (ou lancer le workflow à la main : **Actions → Documentation → Run workflow**).
4. L'URL est affichée dans le job *deploy* : `https://<utilisateur>.github.io/<depot>/`.

!!! note "Dépôt privé"
    GitHub Pages pour un dépôt privé dépend de votre offre GitHub. Le site contient la documentation technique et de sécurité : vérifiez ce que vous souhaitez rendre public.

## Comportement du workflow

| Événement | Action |
|---|---|
| Pull request touchant la doc | Build **strict** uniquement (aucun déploiement) |
| Push sur `main` touchant la doc | Build strict puis déploiement |
| Manuel (`workflow_dispatch`) | Idem |

Le build est **strict** : lien cassé, image manquante ou ancre invalide ⇒ le job échoue.

## Prévisualiser en local

```bash
pip install -r docs/requirements.txt
mkdocs serve            # http://127.0.0.1:8000, rechargement automatique
mkdocs build --strict   # même contrôle que la CI, sortie dans site/
```

## Écrire

- Une page = un fichier Markdown sous `docs/`, à déclarer dans `nav` de `mkdocs.yml`.
- `CHANGELOG.md`, `ROADMAP.md` et `CONTRIBUTING.md` sont **inclus** depuis la racine (`--8<--`) : ne pas les dupliquer.
- Images dans `docs/screenshots/` ; les régénérer avec les scripts `tools/capture-*.cjs`.
- Onglets : `=== "Titre"` ; encadrés : `!!! note`, `!!! warning`.
- Les versions de `mkdocs` et `mkdocs-material` sont épinglées dans `docs/requirements.txt` ; mettez-les à jour volontairement.
