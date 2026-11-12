# Compiler FlashGet pour Linux

La version Linux (AppImage et paquet .deb) se compile dans un conteneur Docker, depuis Windows.
Il faut Docker Desktop démarré. La première construction de l'image télécharge environ 2 à 3 Go
(Ubuntu 22.04, Rust, Node.js, GTK et WebKitGTK).

Toutes les commandes se lancent depuis la racine du dépôt, dans PowerShell.

## 1. Construire l'image (une seule fois)

```powershell
docker build -t flashget-linux scripts/linux
```

## 2. Compilation de vérification (sans signature)

Lance les tests Rust sous Linux, puis produit un AppImage et un .deb non signés dans
`release/linux-check/`.

```powershell
docker run --rm -v "${PWD}:/src" flashget-linux bash /src/scripts/linux/build.sh
```

## 3. Publication

Comme sous Windows, le mot de passe de la clé de signature est demandé pendant la compilation
(d'où `-it`). La clé est montée en lecture seule, elle n'est jamais copiée dans le dépôt.

```powershell
docker run --rm -it -v "${PWD}:/src" -v "$env:USERPROFILE\.tauri:/key:ro" -e NOTES=docs/release-notes/2.1.0.md flashget-linux bash /src/scripts/linux/build.sh --release
```

Les fichiers arrivent dans `release/v{version}/` : `FlashGet_{version}_amd64.AppImage`, sa signature
`.sig` et `FlashGet_{version}_amd64.deb`. Si `npm run release` a déjà été lancé sous Windows pour la
même version, `latest.json` est complété avec la plateforme Linux au lieu d'être remplacé.
Déposez ensuite tous les fichiers de `release/v{version}/` dans la Release GitHub.

## Ce que fait le script

- Copie le projet dans le conteneur sans `node_modules` ni la compilation Windows.
- Télécharge `yt-dlp_linux` (empreinte SHA-256 vérifiée) et FFmpeg statique LGPL de BtbN
  (empreinte vérifiée), placés dans `src-tauri/bin/` sous leur nom Linux.
- Compile avec une seule unité Rust à la fois (`CARGO_BUILD_JOBS=1`), pour les machines avec peu de
  mémoire. Pour aller plus vite : `-e CARGO_BUILD_JOBS=4`.

## Dépendances sur la machine Linux

- AppImage : rien à installer sur les distributions récentes (Ubuntu 22.04 et plus récent,
  Debian 12, Fedora 38+). L'icône de la zone de notification demande `libayatana-appindicator3`.
- .deb : les dépendances (`libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1`) sont
  installées par `apt`.
- Le lecteur intégré de la bibliothèque passe par GStreamer : sans les greffons
  `gstreamer1.0-plugins-good` / `-bad` / `-libav`, certaines vidéos ne se lisent qu'avec « Ouvrir ».
