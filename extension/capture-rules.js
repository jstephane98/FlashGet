// Logique pure et testable des règles de capture automatique (aucun chrome.*).
import { isIgnored } from "./site-rules.js";

// Liste blanche par défaut (basée sur le catégoriseur du cœur + types IDM courants).
export const DEFAULT_CAPTURE_TYPES =
  "7z, rar, zip, tar, gz, bz2, xz, zst, iso, exe, msi, dmg, pkg, deb, rpm, apk, appimage, " +
  "mp4, mkv, avi, mov, webm, flv, wmv, m4v, mpg, mpeg, mp3, flac, wav, aac, ogg, m4a, wma, opus, " +
  "pdf, doc, docx, xls, xlsx, ppt, pptx, epub";

// Dernière extension en minuscules d'un nom de fichier ou d'une URL (ignore query/fragment
// et le chemin). Sans extension ⇒ "".
export function extensionOf(nameOrUrl) {
  let s = String(nameOrUrl ?? "");
  s = s.split(/[?#]/)[0]; // retirer query/fragment (URL)
  s = s.split(/[/\\]/).pop() || ""; // dernier segment de chemin
  const dot = s.lastIndexOf(".");
  if (dot <= 0 || dot === s.length - 1) return "";
  return s.slice(dot + 1).toLowerCase();
}

// Parse la liste de types : séparée par virgules/espaces/sauts de ligne, minuscules,
// sans point de tête, dédoublonnée, sans vides.
export function parseCaptureTypes(text) {
  const set = new Set();
  for (const raw of String(text ?? "").split(/[\s,]+/)) {
    const t = raw.trim().toLowerCase().replace(/^\.+/, "");
    if (t) set.add(t);
  }
  return [...set];
}

// Décide si un download doit être capturé. PURE — la garde "connected" est gérée par l'appelant.
// item = { url, filename?, referrer? }, settings = { autoCapture: bool, types: string[] },
// ignoredSites = hôtes normalisés : un téléchargement dont l'URL ou le référent est sur
// l'un d'eux n'est pas capturé.
export function shouldCapture(item, settings, ignoredSites = []) {
  if (!settings || !settings.autoCapture) return false;
  const url = String((item && item.url) ?? "");
  if (!/^https?:\/\//i.test(url)) return false; // exclut blob:/data:/file:/chrome:
  if (isIgnored(url, ignoredSites)) return false;
  if (item && item.referrer && isIgnored(item.referrer, ignoredSites)) return false;
  const ext = extensionOf(item && item.filename) || extensionOf(url);
  if (!ext) return false;
  return settings.types.includes(ext);
}
