// Règles des sites ignorés (JS pur, sans chrome.*). Mêmes règles que le côté Rust :
// minuscules, sans port, sans « www. » de tête.

// Normalise un nom d'hôte (ou « hôte:port »). Renvoie "" si vide.
export function normalizeHost(host) {
  let h = String(host ?? "").trim().toLowerCase();
  h = h.replace(/^\*\./, ""); // « *.exemple.com » -> « exemple.com »
  h = h.replace(/:\d+$/, ""); // port
  h = h.replace(/\.$/, ""); // point final
  h = h.replace(/^www\./, "");
  return h;
}

// Hôte normalisé d'une URL http(s), ou null si l'URL est invalide / non http(s).
export function hostOf(url) {
  try {
    const u = new URL(String(url ?? ""));
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    const h = normalizeHost(u.hostname);
    return h || null;
  } catch (_e) {
    return null;
  }
}

// Vrai si l'hôte de l'URL est un site ignoré ou l'un de ses sous-domaines.
export function isIgnored(url, sites) {
  const host = hostOf(url);
  if (!host || !Array.isArray(sites)) return false;
  return sites.some((s) => {
    const site = normalizeHost(s);
    return site !== "" && (host === site || host.endsWith("." + site));
  });
}
