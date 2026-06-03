// Logique pure et testable : construit le message de capture WS à partir d'une URL.
export function buildCapture(url) {
  const trimmed = (url ?? "").trim();
  if (!trimmed) {
    throw new Error("URL vide");
  }
  return { type: "capture", url: trimmed };
}
