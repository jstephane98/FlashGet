# Environnement de compilation de FlashGet pour Linux (AppImage et .deb).
# Ubuntu 22.04 : un AppImage construit ici fonctionne sur Ubuntu 22.04 et plus récent,
# Debian 12, Fedora 38+, etc. (glibc 2.35).
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential curl wget file ca-certificates git pkg-config xz-utils patchelf rsync \
      libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
 && rm -rf /var/lib/apt/lists/*

# Node.js 20
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/*

# Rust stable
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable

# xdg-open, embarqué dans l'AppImage pour ouvrir fichiers et liens.
RUN apt-get update && apt-get install -y --no-install-recommends xdg-utils \
 && rm -rf /var/lib/apt/lists/*

# Les outils AppImage ne peuvent pas monter FUSE dans un conteneur : ils s'extraient d'abord.
ENV APPIMAGE_EXTRACT_AND_RUN=1

WORKDIR /work
