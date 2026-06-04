const UNITS = ["o", "Ko", "Mo", "Go", "To"];

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 o";
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const text = i === 0 ? String(Math.round(v)) : v.toFixed(1).replace(".", ",");
  return `${text} ${UNITS[i]}`;
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
  const mm = String(Math.floor(s / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return `${mm}:${ss}`;
}

export function recordingLabel(segDone: number, bytes: number, elapsedSec: number): string {
  return `Enregistrement · ${formatElapsed(elapsedSec)} · ${segDone} segments · ${formatBytes(bytes)}`;
}
