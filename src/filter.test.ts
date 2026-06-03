import { describe, it, expect } from "vitest";
import { filterDownloads } from "./filter";
import type { Download } from "./types";

function dl(partial: Partial<Download>): Download {
  return {
    id: "x", url: "u", file_name: "file.bin", dest_path: "/x", total_bytes: 1,
    downloaded_bytes: 0, status: "queued", supports_ranges: true, error: null,
    priority: "normal", category: "other", ...partial,
  };
}

describe("filterDownloads", () => {
  const list: Download[] = [
    dl({ id: "a", file_name: "Movie.mp4", category: "video", status: "completed" }),
    dl({ id: "b", file_name: "report.pdf", category: "document", status: "downloading" }),
    dl({ id: "c", file_name: "movie-trailer.mkv", category: "video", status: "queued" }),
  ];

  it("returns all with empty filters", () => {
    expect(filterDownloads(list, { query: "", category: "all", status: "all" }).map(d => d.id)).toEqual(["a", "b", "c"]);
  });
  it("filters by name (case-insensitive substring)", () => {
    expect(filterDownloads(list, { query: "movie", category: "all", status: "all" }).map(d => d.id)).toEqual(["a", "c"]);
  });
  it("filters by category", () => {
    expect(filterDownloads(list, { query: "", category: "video", status: "all" }).map(d => d.id)).toEqual(["a", "c"]);
  });
  it("filters by status", () => {
    expect(filterDownloads(list, { query: "", category: "all", status: "downloading" }).map(d => d.id)).toEqual(["b"]);
  });
  it("combines filters", () => {
    expect(filterDownloads(list, { query: "movie", category: "video", status: "queued" }).map(d => d.id)).toEqual(["c"]);
  });
});
