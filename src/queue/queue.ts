import type { Download, Priority } from "../types";

const PRIORITY_RANK: Record<Priority, number> = { high: 0, normal: 1, low: 2 };

/** File d'attente (en attente + retenus), triée comme `pick_to_start` côté Rust. */
export function queueItems(list: Download[]): Download[] {
  return list
    .map((d, i) => ({ d, i }))
    .filter(({ d }) => d.status === "queued" || d.status === "held")
    .sort(
      (a, b) =>
        PRIORITY_RANK[a.d.priority] - PRIORITY_RANK[b.d.priority] ||
        (a.d.queue_rank ?? Number.MAX_SAFE_INTEGER) - (b.d.queue_rank ?? Number.MAX_SAFE_INTEGER) ||
        a.i - b.i,
    )
    .map(({ d }) => d);
}

/** Résultat d'un glisser-déposer : nouvel ordre, élément déplacé et sa priorité. */
export function applyDrop(
  items: Download[],
  from: number,
  to: number,
): { order: string[]; moved: string; priority: Priority } {
  const next = items.slice();
  const [m] = next.splice(from, 1);
  next.splice(to, 0, m);
  const ref = to > 0 ? next[to - 1] : next[1];
  return { order: next.map((d) => d.id), moved: m.id, priority: ref ? ref.priority : m.priority };
}

export function queueSummary(items: Download[]): { count: number; bytes: number | null } {
  let bytes: number | null = 0;
  for (const d of items) {
    if (d.total_bytes == null) {
      bytes = null;
      break;
    }
    bytes += d.total_bytes;
  }
  return { count: items.length, bytes };
}
