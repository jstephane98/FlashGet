import { describe, it, expect } from "vitest";
import type { Download } from "../types";
import {
  groupByDay,
  categoryCounts,
  filterLibrary,
  formatLabel,
  downloadDuration,
  formatDay,
} from "./library";

const now = new Date(2026, 10, 10, 15, 0); // 10 nov. 2026 15:00 local
const at = (y: number, m: number, d: number, h: number, mi: number) =>
  Math.floor(new Date(y, m, d, h, mi).getTime() / 1000);

function d(p: Partial<Download> = {}): Download {
  return {
    id: "x",
    url: "http://x/f",
    file_name: "f.bin",
    dest_path: "C:/f.bin",
    total_bytes: 10,
    downloaded_bytes: 10,
    status: "completed",
    supports_ranges: true,
    error: null,
    priority: "normal",
    category: "other",
    ...p,
  } as Download;
}
const c = (id: string, completed_at: number | null, p: Partial<Download> = {}) =>
  d({ id, completed_at, ...p });

describe("library", () => {
  it("groupByDay : aujourd'hui, hier, date, plus ancien", () => {
    const g = groupByDay(
      [c("a", at(2026, 10, 10, 14, 32)), c("b", at(2026, 10, 9, 22, 10)), c("c", at(2026, 10, 3, 9, 0)), c("d", null)],
      now,
    );
    expect(g.map((x) => x.label)).toEqual(["Aujourd'hui", "Hier", "3 nov. 2026", "Plus ancien"]);
  });
  it("groupByDay › sans date : jamais Invalid Date", () => {
    expect(JSON.stringify(groupByDay([c("d", null)], now))).not.toMatch(/Invalid|NaN/);
  });
  it("groupByDay ignore les non terminés et trie du plus récent au plus ancien", () => {
    const g = groupByDay(
      [
        c("old", at(2026, 10, 10, 8, 0)),
        c("run", at(2026, 10, 10, 9, 0), { status: "downloading" }),
        c("new", at(2026, 10, 10, 14, 0)),
        c("tie1", at(2026, 10, 10, 10, 0)),
        c("tie2", at(2026, 10, 10, 10, 0)),
      ],
      now,
    );
    expect(g).toHaveLength(1);
    expect(g[0].items.map((i) => i.id)).toEqual(["new", "tie2", "tie1", "old"]);
  });
  it("categoryCounts ne liste que les catégories présentes", () => {
    const r = categoryCounts([
      d({ category: "music" }),
      d({ category: "video" }),
      d({ category: "video" }),
      d({ category: "archive", status: "downloading" }),
    ]);
    expect(r).toEqual([
      { category: "video", count: 2 },
      { category: "music", count: 1 },
    ]);
  });
  it("filterLibrary par catégorie et par nom", () => {
    const items = [
      d({ id: "1", category: "video", file_name: "Concert.mp4" }),
      d({ id: "2", category: "music", file_name: "concert.mp3" }),
      d({ id: "3", category: "music", file_name: "Autre.mp3", status: "paused" }),
    ];
    expect(filterLibrary(items, "all", "").map((i) => i.id)).toEqual(["1", "2"]);
    expect(filterLibrary(items, "music", "").map((i) => i.id)).toEqual(["2"]);
    expect(filterLibrary(items, "all", "CONCERT").map((i) => i.id)).toEqual(["1", "2"]);
  });
  it("downloadDuration", () => {
    expect(downloadDuration(d({ started_at: 100, completed_at: 267 }))).toBe(167);
    expect(downloadDuration(d({ started_at: null, completed_at: 267 }))).toBeNull();
    expect(downloadDuration(d({ started_at: 300, completed_at: 267 }))).toBeNull();
  });
  it("formatLabel", () => {
    expect(formatLabel(d({ file_name: "Concert.mp4" }))).toBe("MP4");
    expect(formatLabel(d({ file_name: "README" }))).toBe("");
  });
  it("formatDay", () => {
    expect(formatDay(at(2026, 10, 3, 14, 32), now)).toBe("3 nov. 2026 à 14:32");
    expect(formatDay(at(2026, 10, 10, 14, 32), now)).toBe("aujourd'hui à 14:32");
  });
});
