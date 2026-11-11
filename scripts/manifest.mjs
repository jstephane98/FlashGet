// Logique pure du manifeste de mise à jour (latest.json) lu par l'updater Tauri.
export function buildManifest({ version, notes, pubDate, signature, fileName, repo }) {
  return {
    version,
    notes,
    pub_date: pubDate.toISOString(),
    platforms: {
      "windows-x86_64": {
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
