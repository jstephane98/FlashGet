import { describe, it, expect } from "vitest";
import type { Download, Priority, DownloadStatus } from "../types";
import { queueItems, applyDrop, queueSummary } from "./queue";
import { parseHHMM, inWindow, nextStart, timelineSegments, nextStartLabel } from "./schedule";
import type { Schedule } from "../api";

function dl(id: string, o: Partial<Download> = {}): Download {
  return {
    id, url: "http://x/" + id, file_name: id, dest_path: "", total_bytes: 100, downloaded_bytes: 0,
    status: "queued" as DownloadStatus, supports_ranges: true, error: null,
    priority: "normal" as Priority, category: "other", queue_rank: null, ...o,
  };
}
function sched(o: Partial<Schedule> = {}): Schedule {
  return {
    enabled: true, start: "02:00", end: "07:00",
    days: [true, true, true, true, true, false, false], limit_bps: null, at_end: "pause", ...o,
  };
}

describe("queueItems", () => {
  it("trie priorité, rang (null en dernier), position", () => {
    const list = [
      dl("a", { queue_rank: null }),
      dl("b", { queue_rank: 2 }),
      dl("c", { priority: "high", queue_rank: 9 }),
      dl("d", { queue_rank: 1 }),
      dl("e", { priority: "low", queue_rank: 0 }),
      dl("f", { queue_rank: null }),
    ];
    expect(queueItems(list).map((d) => d.id)).toEqual(["c", "d", "b", "a", "f", "e"]);
  });
  it("garde queued et held, exclut le reste", () => {
    const list = [
      dl("q"), dl("h", { status: "held" }), dl("x", { status: "completed" }),
      dl("y", { status: "downloading" }), dl("z", { status: "paused" }),
    ];
    expect(queueItems(list).map((d) => d.id)).toEqual(["q", "h"]);
  });
});

describe("applyDrop", () => {
  const items = [dl("a", { priority: "high" }), dl("b", { priority: "high" }), dl("c"), dl("d")];
  it("déplacement dans un groupe garde la priorité", () => {
    expect(applyDrop(items, 0, 1)).toEqual({ order: ["b", "a", "c", "d"], moved: "a", priority: "high" });
  });
  it("dépasser un High donne High", () => {
    expect(applyDrop(items, 2, 3)).toEqual({ order: ["a", "b", "d", "c"], moved: "c", priority: "normal" });
    expect(applyDrop(items, 3, 2)).toEqual({ order: ["a", "b", "d", "c"], moved: "d", priority: "high" });
    expect(applyDrop(items, 3, 1).priority).toBe("high");
  });
  it("arrivée en tête prend la priorité du suivant", () => {
    const r = applyDrop([dl("a", { priority: "normal" }), dl("b", { priority: "low" })], 0, 0);
    expect(r.priority).toBe("low");
    const r2 = applyDrop([dl("a", { priority: "low" }), dl("b", { priority: "normal" })], 1, 0);
    expect(r2).toEqual({ order: ["b", "a"], moved: "b", priority: "low" });
  });
});

describe("queueSummary", () => {
  it("somme ou null", () => {
    expect(queueSummary([dl("a", { total_bytes: 5 }), dl("b", { total_bytes: 7 })])).toEqual({ count: 2, bytes: 12 });
    expect(queueSummary([dl("a"), dl("b", { total_bytes: null })])).toEqual({ count: 2, bytes: null });
  });
});

describe("schedule", () => {
  it("parseHHMM", () => {
    expect(parseHHMM("02:30")).toBe(150);
    expect(parseHHMM("2:30")).toBeNull();
    expect(parseHHMM("24:00")).toBeNull();
    expect(parseHHMM("12:60")).toBeNull();
  });
  it("inWindow même jour", () => {
    const s = sched();
    expect(inWindow(s, 0, 180)).toBe(true);
    expect(inWindow(s, 0, 420)).toBe(false);
    expect(inWindow(s, 5, 180)).toBe(false);
    expect(inWindow(sched({ enabled: false }), 0, 180)).toBe(false);
  });
  it("inWindow nuit", () => {
    const s = sched({ start: "22:00", end: "06:00", days: [false, false, false, false, true, false, false] });
    expect(inWindow(s, 4, 23 * 60 + 30)).toBe(true);
    expect(inWindow(s, 5, 180)).toBe(true);
    expect(inWindow(s, 5, 420)).toBe(false);
    expect(inWindow(s, 3, 23 * 60 + 30)).toBe(false);
  });
  it("nextStart", () => {
    const s = sched();
    expect(nextStart(s, 0, 60)).toEqual([0, 120]);
    expect(nextStart(s, 0, 180)).toEqual([1, 120]);
    expect(nextStart(s, 4, 180)).toEqual([3, 120]);
    expect(nextStart(sched({ days: [false, false, false, false, false, false, false] }), 0, 60)).toBeNull();
  });
  it("timelineSegments", () => {
    const one = timelineSegments(sched());
    expect(one).toHaveLength(1);
    expect(one[0].left).toBeCloseTo(8.33, 1);
    expect(one[0].width).toBeCloseTo(20.83, 1);
    const two = timelineSegments(sched({ start: "22:00", end: "06:00" }));
    expect(two).toHaveLength(2);
    expect(two[0].left).toBeCloseTo(91.67, 1);
    expect(two[0].width).toBeCloseTo(8.33, 1);
    expect(two[1]).toEqual({ left: 0, width: 25 });
  });
  it("nextStartLabel", () => {
    // lundi 9 novembre 2026, 23:00
    expect(nextStartLabel(sched(), new Date(2026, 10, 9, 23, 0))).toBe("cette nuit à 02:00");
    // lundi 01:00 -> même nuit
    expect(nextStartLabel(sched(), new Date(2026, 10, 9, 1, 0))).toBe("cette nuit à 02:00");
    // vendredi 23:00 -> lundi
    expect(nextStartLabel(sched(), new Date(2026, 10, 13, 23, 0))).toBe("lundi à 02:00");
    // lundi 10:00, début 20:00 un mardi -> demain
    expect(nextStartLabel(sched({ start: "20:00", end: "23:00", days: [false, true, false, false, false, false, false] }), new Date(2026, 10, 9, 10, 0))).toBe("demain à 20:00");
    expect(nextStartLabel(sched({ enabled: false }), new Date(2026, 10, 9, 10, 0))).toBeNull();
  });
});
