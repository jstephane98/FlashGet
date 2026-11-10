import type { Download } from "../types";
import { queueItems } from "../queue/queue";

export type DownloadTab = "all" | "active" | "paused" | "error";

export function inTab(d: Download, tab: DownloadTab): boolean {
  if (d.status === "completed") return false;
  switch (tab) {
    case "active": return d.status === "downloading" || d.status === "recording";
    case "paused": return d.status === "paused";
    case "error": return d.status === "error";
    default: return true;
  }
}

export function tabCounts(list: Download[]): Record<DownloadTab, number> {
  const counts: Record<DownloadTab, number> = { all: 0, active: 0, paused: 0, error: 0 };
  for (const tab of Object.keys(counts) as DownloadTab[]) {
    counts[tab] = list.filter((d) => inTab(d, tab)).length;
  }
  return counts;
}

export function visibleDownloads(list: Download[], tab: DownloadTab, query: string): Download[] {
  const q = query.trim().toLowerCase();
  return list.filter((d) => inTab(d, tab) && (!q || d.file_name.toLowerCase().includes(q)));
}

/** Rang de démarrage des `queued`, dans l'ordre exact de la page File d'attente (`queueItems`). */
export function queuePositions(list: Download[]): Record<string, number> {
  const position: Record<string, number> = {};
  queueItems(list)
    .filter((d) => d.status === "queued")
    .forEach((d, idx) => { position[d.id] = idx + 1; });
  return position;
}
