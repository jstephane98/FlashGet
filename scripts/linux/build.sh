#!/usr/bin/env bash
# Compile FlashGet pour Linux dans le conteneur (voir scripts/linux/README.md).
# Le dépôt est monté en lecture seule sur /src ; le travail se fait dans /work, pour ne pas
# mélanger les node_modules et la compilation Windows avec ceux de Linux.
#   --release : compile, signe et dépose les paquets dans release/v{version}/ (clé montée sur /key)
#   sans option : simple compilation de vérification, sans signature.
set -euo pipefail

MODE="${1:-check}"
TRIPLE="x86_64-unknown-linux-gnu"

echo "== Copie du projet"
rsync -a --delete \
  --exclude node_modules --exclude src-tauri/target --exclude release --exclude .git \
  --exclude 'src-tauri/bin/*.exe' /src/ /work/
cd /work

echo "== yt-dlp pour Linux (empreinte vérifiée)"
mkdir -p src-tauri/bin
TAG=$(curl -fsSL https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest | grep -m1 '"tag_name"' | cut -d'"' -f4)
curl -fsSL -o /tmp/yt-dlp_linux "https://github.com/yt-dlp/yt-dlp/releases/download/${TAG}/yt-dlp_linux"
curl -fsSL -o /tmp/SUMS "https://github.com/yt-dlp/yt-dlp/releases/download/${TAG}/SHA2-256SUMS"
EXPECTED=$(awk '$2=="yt-dlp_linux"{print $1}' /tmp/SUMS)
echo "${EXPECTED}  /tmp/yt-dlp_linux" | sha256sum -c -
install -m 755 /tmp/yt-dlp_linux "src-tauri/bin/yt-dlp-${TRIPLE}"

echo "== FFmpeg statique pour Linux (LGPL, BtbN)"
FF="ffmpeg-master-latest-linux64-lgpl"
curl -fsSL -o /tmp/ff.tar.xz "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/${FF}.tar.xz"
curl -fsSL -o /tmp/ff.sums "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/checksums.sha256"
(cd /tmp && grep " ${FF}.tar.xz\$" ff.sums | sed "s| ${FF}.tar.xz| ff.tar.xz|" | sha256sum -c -)
tar -xJf /tmp/ff.tar.xz -C /tmp
install -m 755 "/tmp/${FF}/bin/ffmpeg" "src-tauri/bin/ffmpeg-${TRIPLE}"

echo "== Dépendances JavaScript"
npm ci

# Peu de mémoire : une seule unité de compilation Rust à la fois.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

if [ "$MODE" = "--release" ]; then
  export TAURI_SIGNING_PRIVATE_KEY=/key/flashget.key
  NOTES_ARG=()
  [ -n "${NOTES:-}" ] && NOTES_ARG=(--notes "$NOTES")
  npm run release -- "${NOTES_ARG[@]}"
  mkdir -p /src/release
  rsync -a /work/release/ /src/release/
  echo "== Paquets copiés dans release/ du dépôt"
else
  echo "== Tests Rust"
  (cd src-tauri && cargo test --workspace)
  echo "== Compilation de vérification (sans signature)"
  npx tauri build --bundles appimage,deb --config '{"bundle":{"createUpdaterArtifacts":false}}'
  mkdir -p /src/release/linux-check
  cp src-tauri/target/release/bundle/appimage/*.AppImage src-tauri/target/release/bundle/deb/*.deb /src/release/linux-check/
  echo "== Paquets de vérification dans release/linux-check/"
fi
