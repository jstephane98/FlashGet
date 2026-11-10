import type { FormatKind, MediaFormat } from "../api";
import { formatElapsed } from "../format";

const KIND_ORDER: FormatKind[] = ["av", "audio", "video"];

export function availableKinds(formats: MediaFormat[]): FormatKind[] {
  return KIND_ORDER.filter((k) => formats.some((f) => f.kind === k));
}

export function formatsOf(formats: MediaFormat[], kind: FormatKind): MediaFormat[] {
  return formats.filter((f) => f.kind === kind);
}

export function defaultFormat(formats: MediaFormat[], kind: FormatKind): MediaFormat | undefined {
  const list = formatsOf(formats, kind);
  return list.find((f) => f.recommended) ?? list[0];
}

export function outputExtension(f: MediaFormat): string {
  // Ligne de conversion (`format_id` "convert") : son conteneur est le format des réglages.
  if (f.kind === "audio" && f.container) return "." + f.container.toLowerCase();
  if (f.format_id === "convert") return ".mp3";
  return ".mp4";
}

/** Mêmes extensions média que `ensure_extension` côté Rust. */
const KNOWN_EXT = /\.(mp4|mp3|m4a|webm|opus|mkv)$/i;

export function withExtension(stem: string, ext: string): string {
  return stem.replace(KNOWN_EXT, "") + ext;
}

/** Même règle que `parse_clock` côté Rust : "90", "1:30", "01:02:03". */
export function parseClock(s: string): number | null {
  const v = s.trim();
  if (v === "") return null;
  const parts = v.split(":");
  if (parts.length > 3) return null;
  const nums: number[] = [];
  for (const p of parts) {
    if (!/^[0-9]+$/.test(p)) return null;
    nums.push(Number(p));
  }
  if (nums.length > 1 && nums.slice(1).some((n) => n >= 60)) return null;
  return nums.reduce((acc, n) => acc * 60 + n, 0);
}

export type CutError = "cut.badStart" | "cut.badEnd" | "cut.endBeforeStart" | "cut.beyondEnd";

export function validateCut(start: string, end: string, durationS: number | null): CutError | null {
  const s = parseClock(start);
  if (s === null) return "cut.badStart";
  if (end.trim() === "") return null;
  const e = parseClock(end);
  if (e === null) return "cut.badEnd";
  if (e <= s) return "cut.endBeforeStart";
  if (durationS !== null && e > durationS) return "cut.beyondEnd";
  return null;
}

export function formatDuration(s: number): string {
  return formatElapsed(s);
}
