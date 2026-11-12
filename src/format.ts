import { getLang, locale, t } from "./i18n";

const UNITS = {
  fr: ["o", "Ko", "Mo", "Go", "To"],
  en: ["B", "KB", "MB", "GB", "TB"],
} as const;

export function formatBytes(bytes: number): string {
  const lang = getLang();
  const units = UNITS[lang];
  if (bytes <= 0) return `0 ${units[0]}`;
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const fixed = v.toFixed(1);
  const text = i === 0 ? String(Math.round(v)) : lang === "fr" ? fixed.replace(".", ",") : fixed;
  return `${text} ${units[i]}`;
}

const pad2 = (n: number) => String(n).padStart(2, "0");

/** Heure : « 21:14 » en français, selon les réglages régionaux en anglais (« 9:14 PM »). */
export function formatClock(date: Date): string {
  if (getLang() === "fr") return `${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
  return date.toLocaleTimeString(locale(), { hour: "numeric", minute: "2-digit" });
}

/** Date courte dans la langue courante (« 12 nov. », « Nov 12 »). */
export function formatDate(date: Date, withYear = false): string {
  return date.toLocaleDateString(locale(), withYear ? { day: "numeric", month: "short", year: "numeric" } : { day: "numeric", month: "short" });
}

/** Date en chiffres : « 12/11/2026 » en français, selon les réglages régionaux en anglais. */
export function formatNumericDate(date: Date): string {
  if (getLang() === "fr") return `${pad2(date.getDate())}/${pad2(date.getMonth() + 1)}/${date.getFullYear()}`;
  return date.toLocaleDateString(locale());
}

/** Nombre décimal avec le séparateur de la langue courante. */
export function formatDecimal(n: number): string {
  return getLang() === "fr" ? String(n).replace(".", ",") : String(n);
}

export function formatSpeed(bps: number): string {
  if (bps <= 0) return "—";
  return `${formatBytes(bps)}/s`;
}

export function percent(downloaded: number, total: number | null): number {
  if (!total || total <= 0) return 0;
  return Math.min(100, Math.round((downloaded / total) * 100));
}

export function formatElapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  // Au-delà d'une heure (lives longs), préfixe les heures : H:MM:SS.
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function recordingLabel(segDone: number, bytes: number, elapsedSec: number): string {
  return t("dl.recording", { elapsed: formatElapsed(elapsedSec), n: segDone, size: formatBytes(bytes) });
}

export function formatEta(remainingBytes: number, bps: number): string {
  if (!(bps > 0) || !Number.isFinite(remainingBytes) || remainingBytes < 0) return "—";
  const s = Math.round(remainingBytes / bps);
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  const h = Math.floor(s / 3600);
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  return `${h} h ${mm}`;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}
