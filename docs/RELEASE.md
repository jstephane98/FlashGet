# Publier une version de FlashGet

La publication est locale : `npm run release` compile, signe et prépare les fichiers ; vous les déposez ensuite à la main dans une Release GitHub de `jstephane98/FlashGet`.

## 1. Générer la clé de signature (une seule fois)

```powershell
npx tauri signer generate -w $env:USERPROFILE\.tauri\flashget.key
```

(Sous `cmd` : `-w %USERPROFILE%\.tauri\flashget.key`.)

- Vous choisissez un mot de passe. Conservez la clé privée **et** son mot de passe : sans eux, plus aucune mise à jour ne peut être signée.
- Seul le contenu du fichier `.pub` va dans `src-tauri/tauri.conf.json`, à `plugins.updater.pubkey`.
- Ne placez jamais la clé ni le mot de passe dans le dépôt.

## 2. Préparer l'environnement

ffmpeg et yt-dlp doivent être présents dans `src-tauri/bin/` avant de compiler (voir `src-tauri/bin/README.md`).

Dans PowerShell, à chaque session de publication (à saisir vous-même, sans les enregistrer dans un fichier du dépôt) :

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\flashget.key"
```

`TAURI_SIGNING_PRIVATE_KEY` accepte soit le contenu de la clé, soit son chemin. Le script vérifie seulement que la clé est définie (il accepte aussi `TAURI_SIGNING_PRIVATE_KEY_PATH` et la transmet à Tauri sous le bon nom).

Laissez `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` non définie : Tauri demande alors le mot de passe à la signature. Le saisir dans `$env:` peut le laisser dans l'historique de PowerShell.

Attention : à cause de `createUpdaterArtifacts`, même un `npx tauri build` direct exige la variable `TAURI_SIGNING_PRIVATE_KEY` (Tauri ne lit pas `TAURI_SIGNING_PRIVATE_KEY_PATH`). `tauri dev` n'est pas concerné.

## 3. Changer le numéro de version

Mettez la même version dans :

- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `package.json`
- `package-lock.json`

## 4. Compiler et préparer les fichiers

```powershell
npm run release -- --notes notes.md
```

`--notes` est facultatif (le texte de `notes.md` s'affiche aux utilisateurs). Pour vérifier l'environnement sans compiler : `npm run release -- --dry-run`.

Le script crée `release/v{version}/` avec :

- `FlashGet_{version}_x64-setup.exe`
- `FlashGet_{version}_x64-setup.exe.sig`
- `latest.json`

## 5. Créer la Release GitHub

1. Sur `https://github.com/jstephane98/FlashGet/releases`, créez une Release avec le tag `v{version}`.
2. Déposez les 3 fichiers ci-dessus.
3. Publiez.

L'application interroge `https://github.com/jstephane98/FlashGet/releases/latest/download/latest.json`. Le dépôt, ou au moins ses Releases, doit donc être **public**, sinon l'updater n'atteindra pas `latest.json`.

## 6. Premier passage depuis FlashGet 0.1.0

L'ancien FlashGet 0.1.0 n'a pas d'updater : l'installeur 2.0.0 doit être lancé à la main, une seule fois.

- Les données sont migrées automatiquement.
- Le thème peut être réinitialisé une fois.
- Si « Applications installées » affiche deux FlashGet, désinstallez celui en 0.1.0.

## 7. Avertissement SmartScreen

L'installeur n'est pas signé par un certificat de signature de code : Windows SmartScreen affichera un avertissement (« Informations complémentaires », puis « Exécuter quand même »). La signature Tauri protège les mises à jour, pas ce premier lancement.

## Points d'attention

- La Release GitHub doit être **publiée** : ni brouillon (draft), ni pré-version (pre-release). Sinon `releases/latest` ne pointe pas vers elle et les mises à jour ne la voient pas.
- Fermez FlashGet 0.1.0 avant le premier lancement de 2.0.0. La migration des données copie sa base, et les deux versions se disputeraient le port de l'extension et l'entrée de démarrage automatique.
