// Logique pure et testable : construit le message de capture WS à partir d'une URL
// et d'en-têtes optionnels (referrer / cookie / user_agent). Les champs vides sont omis.
export function buildCapture(url, extras = {}) {
  const trimmed = (url ?? "").trim();
  if (!trimmed) {
    throw new Error("URL vide");
  }
  const msg = { type: "capture", url: trimmed };
  for (const key of ["referrer", "cookie", "user_agent", "filename", "kind", "fallback_url", "fallback_kind"]) {
    const value = extras[key];
    if (value) {
      msg[key] = value;
    }
  }
  return msg;
}
