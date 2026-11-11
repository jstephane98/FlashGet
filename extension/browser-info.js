// Nom du navigateur Chromium, pour l'affichage côté application (JS pur).
// uaData = navigator.userAgentData (peut être absent), ua = navigator.userAgent.
// isBrave = Boolean(navigator.brave) (Brave se présente comme Chrome).
export function browserName(uaData, ua, isBrave = false) {
  if (isBrave) return "Brave";
  const brands = uaData && Array.isArray(uaData.brands) ? uaData.brands.map((b) => String(b.brand)) : [];
  if (brands.length > 0) {
    if (brands.some((b) => /edge/i.test(b))) return "Edge";
    if (brands.some((b) => /brave/i.test(b))) return "Brave";
    if (brands.some((b) => /google chrome/i.test(b))) return "Chrome";
    return "Chromium";
  }
  const s = String(ua ?? "");
  if (/\bEdg(e|A|iOS)?\//.test(s)) return "Edge";
  if (/\bBrave\b/i.test(s)) return "Brave";
  if (/\bChrome\//.test(s)) return "Chrome";
  return "Chromium";
}
