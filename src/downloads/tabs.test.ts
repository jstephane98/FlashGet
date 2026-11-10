import { describe, it, expect } from "vitest";
import { tabCounts, visibleDownloads, queuePositions, inTab } from "./tabs";
import type { Download, Priority } from "../types";

function mk(id: string, status: Download["status"], file_name: string, priority: Priority = "normal"): Download {
  return {
    id, url: "https://x/" + id, file_name, dest_path: "C:/dl/" + file_name,
    total_bytes: null, downloaded_bytes: 0, status, supports_ranges: true,
    error: null, priority, category: "other", kind: "regular",
  };
}
const q = (id: string, p: Priority) => mk(id, "queued", id, p);
const list = [
  mk("1", "downloading", "Clip A.mp4"),
  mk("2", "recording", "Direct.ts"),
  mk("3", "paused", "clip b.mp4"),
  mk("4", "error", "photos.zip"),
  mk("5", "queued", "rapport.pdf"),
  mk("6", "completed", "Clip C.mp4"),
];

describe("tabs", () => {
  it("compte par onglet", () => {
    expect(tabCounts(list)).toEqual({ all: 5, active: 2, paused: 1, error: 1 });
  });
  it("inTab", () => {
    expect(inTab(list[5], "all")).toBe(false);
    expect(inTab(list[1], "active")).toBe(true);
  });
  it("exclut les terminés et filtre sans casse en gardant l'ordre", () => {
    expect(visibleDownloads(list, "all", "").map((d) => d.status)).not.toContain("completed");
    expect(visibleDownloads(list, "all", "CLIP").map((d) => d.file_name)).toEqual(["Clip A.mp4", "clip b.mp4"]);
  });
  it("positions de file", () => {
    expect(queuePositions([q("a", "low"), q("b", "high"), q("c", "normal")])).toEqual({ b: 1, c: 2, a: 3 });
  });
});
