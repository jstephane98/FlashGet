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
