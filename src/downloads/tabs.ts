import type { Download, Priority } from "../types";

export type DownloadTab = "all" | "active" | "paused" | "error";

const RANK: Record<Priority, number> = { high: 0, normal: 1, low: 2 };

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

export function queuePositions(list: Download[]): Record<string, number> {
  const queued = list
    .map((d, i) => ({ d, i }))
    .filter((x) => x.d.status === "queued")
    .sort((a, b) => RANK[a.d.priority] - RANK[b.d.priority] || a.i - b.i);
  const position: Record<string, number> = {};
  queued.forEach((x, idx) => { position[x.d.id] = idx + 1; });
  return position;
}
