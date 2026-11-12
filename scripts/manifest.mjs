// Logique pure du manifeste de mise à jour (latest.json) lu par l'updater Tauri.

// Fichiers produits par `tauri build` selon le système qui compile.
// `files` : tout ce qui est déposé dans la Release ; `updater` : le fichier que l'updater installe.
export function platformTarget(nodePlatform, version) {
  switch (nodePlatform) {
    case "win32":
      return {
        key: "windows-x86_64",
        bundleDir: "nsis",
        updater: `FlashGet_${version}_x64-setup.exe`,
        files: [`FlashGet_${version}_x64-setup.exe`],
      };
    case "linux":
      return {
        key: "linux-x86_64",
        bundleDir: "appimage",
        updater: `FlashGet_${version}_amd64.AppImage`,
        files: [`FlashGet_${version}_amd64.AppImage`, `../deb/FlashGet_${version}_amd64.deb`],
      };
    default:
      return null;
  }
}

// Manifeste pour une plateforme, fusionné avec celui d'une autre plateforme de la même version
// (Windows et Linux sont compilés séparément, puis déposés dans la même Release).
export function buildManifest({ version, notes, pubDate, signature, fileName, repo, platform = "windows-x86_64", existing = null }) {
  const same = existing && existing.version === version;
  return {
    version,
    notes: notes || (same ? existing.notes ?? "" : ""),
    pub_date: pubDate.toISOString(),
    platforms: {
      ...(same ? existing.platforms ?? {} : {}),
      [platform]: {
        signature: signature.trim(),
        url: `https://github.com/${repo}/releases/download/v${version}/${fileName}`,
      },
    },
  };
}

export const PUBKEY_PLACEHOLDER = "REMPLACER_PAR_LA_CLE_PUBLIQUE";

// Refuse de publier tant que la clé publique de l'updater n'a pas été renseignée.
export function checkPubkey(conf) {
  const pubkey = conf?.plugins?.updater?.pubkey;
  if (!pubkey || pubkey === PUBKEY_PLACEHOLDER) {
    return {
      ok: false,
      message:
        "Clé publique de mise à jour absente : remplacez « plugins.updater.pubkey » dans " +
        "src-tauri/tauri.conf.json par votre clé publique avant de publier. Voir docs/RELEASE.md.",
    };
  }
  return { ok: true, message: "" };
}
