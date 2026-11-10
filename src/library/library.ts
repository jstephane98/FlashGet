import type { Category, Download } from "../types";
import { t } from "../i18n";

export type DayGroup = { key: string; label: string; items: Download[] };

const CATEGORY_ORDER: Category[] = ["video", "music", "document", "program", "archive", "other"];

const pad = (n: number) => String(n).padStart(2, "0");

/** Clé de jour en heure locale : AAAA-MM-JJ. */
function dayKey(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function dateLabel(date: Date): string {
  return date.toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric" });
}

function timeLabel(date: Date): string {
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

const isDone = (d: Download) => d.status === "completed";

export function groupByDay(items: Download[], now: Date): DayGroup[] {
  const todayKey = dayKey(now);
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  const yesterdayKey = dayKey(yesterday);

  const done = items.filter(isDone);
  // Ordre inverse de la liste, puis tri stable par date décroissante :
  // à égalité, l'élément le plus tardif de la liste passe en premier.
  const sorted = done
    .map((d, i) => ({ d, i }))
    .sort((a, b) => {
      const ta = a.d.completed_at ?? -Infinity;
      const tb = b.d.completed_at ?? -Infinity;
      if (ta !== tb) return tb > ta ? 1 : -1;
      return b.i - a.i;
    })
    .map((x) => x.d);

  const groups: DayGroup[] = [];
  const byKey = new Map<string, DayGroup>();
  for (const d of sorted) {
    let key: string;
    let label: string;
    if (d.completed_at == null) {
      key = "older";
      label = t("lib.older");
    } else {
      const date = new Date(d.completed_at * 1000);
      key = dayKey(date);
      label =
        key === todayKey ? t("lib.today") : key === yesterdayKey ? t("lib.yesterday") : dateLabel(date);
    }
    let g = byKey.get(key);
    if (!g) {
      g = { key, label, items: [] };
      byKey.set(key, g);
      groups.push(g);
    }
    g.items.push(d);
  }
  return groups;
}

export function categoryCounts(items: Download[]): { category: Category; count: number }[] {
  const counts = new Map<Category, number>();
  for (const d of items) {
    if (!isDone(d)) continue;
    counts.set(d.category, (counts.get(d.category) ?? 0) + 1);
  }
  return CATEGORY_ORDER.filter((c) => counts.has(c)).map((category) => ({
    category,
    count: counts.get(category)!,
  }));
}

export function filterLibrary(items: Download[], category: Category | "all", query: string): Download[] {
  const q = query.trim().toLowerCase();
  return items.filter(
    (d) =>
      isDone(d) &&
      (category === "all" || d.category === category) &&
      (q === "" || d.file_name.toLowerCase().includes(q)),
  );
}

/** Extension en majuscules ; la hauteur (" · {h}p") n'est pas stockée en phase 3. */
export function formatLabel(d: Download): string {
  const dot = d.file_name.lastIndexOf(".");
  if (dot <= 0 || dot === d.file_name.length - 1) return "";
  return d.file_name.slice(dot + 1).toUpperCase();
}

export function downloadDuration(d: Download): number | null {
  if (d.started_at == null || d.completed_at == null) return null;
  const diff = d.completed_at - d.started_at;
  return diff >= 0 ? diff : null;
}

export function formatDay(ts: number, now: Date = new Date()): string {
  const date = new Date(ts * 1000);
  const time = timeLabel(date);
  if (dayKey(date) === dayKey(now)) return t("lib.todayAt", { time });
  return t("lib.dateAt", { date: dateLabel(date), time });
}
