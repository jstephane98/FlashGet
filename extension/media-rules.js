// Logique pure et testable de détection HLS (aucun chrome.*).

// Vrai si le chemin de l'URL finit par .m3u8 (query/fragment ignorés).
export function isHlsUrl(url) {
  const path = String(url ?? "").split(/[?#]/)[0];
  return /\.m3u8$/i.test(path);
}

// Vrai si le Content-Type est un type HLS (paramètres ;charset ignorés, insensible à la casse).
export function isHlsContentType(ct) {
  const base = String(ct ?? "").split(";")[0].trim().toLowerCase();
  return [
    "application/vnd.apple.mpegurl",
    "application/x-mpegurl",
    "audio/mpegurl",
    "audio/x-mpegurl",
  ].includes(base);
}

// Vrai si le chemin de l'URL finit par .mpd (query/fragment ignorés).
export function isDashUrl(url) {
  const path = String(url ?? "").split(/[?#]/)[0];
  return /\.mpd$/i.test(path);
}

// Vrai si le Content-Type est DASH (application/dash+xml).
export function isDashContentType(ct) {
  const base = String(ct ?? "").split(";")[0].trim().toLowerCase();
  return base === "application/dash+xml";
}

// Dédoublonne une liste de flux par URL, en conservant l'ordre d'apparition.
export function dedupeStreams(list) {
  const seen = new Set();
  const out = [];
  for (const s of list || []) {
    if (seen.has(s.url)) continue;
    seen.add(s.url);
    out.push(s);
  }
  return out;
}
