// Règles des sites ignorés (JS pur, sans chrome.*). Mêmes règles que le côté Rust.

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;
const LABEL = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/;

// Normalise une saisie d'hôte. Renvoie null si elle est invalide.
// Ordre : trim+minuscules, schéma et chemin, port, point final, « *. », « www. » (une fois),
// puis punycode via URL.
export function normalizeHost(input) {
  if (input == null) return null;
  let h = String(input).trim().toLowerCase();
  h = h.replace(/^[a-z][a-z0-9+.-]*:\/\//, "");
  h = h.split(/[/?#]/)[0];
  h = h.replace(/:\d+$/, "");
  h = h.replace(/\.$/, "");
  h = h.replace(/^\*\./, "");
  h = h.replace(/^www\./, "");
  if (!h || /[\s:[\]@]/.test(h)) return null; // vide, IPv6, identifiants
  try {
    h = new URL("http://" + h + "/").hostname;
  } catch (_e) {
    return null;
  }
  if (h === "localhost" || IPV4.test(h)) return h;
  const labels = h.split(".");
  if (labels.length < 2) return null; // une seule étiquette (« com », « intranet »)
  return labels.every((l) => LABEL.test(l)) ? h : null;
}

// Hôte normalisé d'une URL http(s), ou null si invalide / non http(s).
export function hostOf(url) {
  try {
    const u = new URL(String(url ?? ""));
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    return normalizeHost(u.hostname);
  } catch (_e) {
    return null;
  }
}

// Entrée de la liste qui fait ignorer l'URL (hôte identique ou parent), sinon null.
// Un site IPv4 ne correspond qu'à l'hôte exact.
export function matchingSite(url, sites) {
  const host = hostOf(url);
  if (!host || !Array.isArray(sites)) return null;
  for (const raw of sites) {
    const site = normalizeHost(raw);
    if (!site) continue;
    if (host === site) return site;
    if (!IPV4.test(site) && host.endsWith("." + site)) return site;
  }
  return null;
}

export function isIgnored(url, sites) {
  return matchingSite(url, sites) !== null;
}
