import { describe, it, expect } from "vitest";
import { isHlsUrl, isHlsContentType, dedupeStreams } from "./media-rules.js";

describe("isHlsUrl", () => {
  it("matches .m3u8 path ignoring query", () => {
    expect(isHlsUrl("https://h/p/master.m3u8")).toBe(true);
    expect(isHlsUrl("https://h/p/master.m3u8?token=abc")).toBe(true);
  });
  it("rejects non-m3u8", () => {
    expect(isHlsUrl("https://h/p/video.mp4")).toBe(false);
    expect(isHlsUrl("https://h/p/seg.ts")).toBe(false);
  });
});

describe("isHlsContentType", () => {
  it("matches HLS mime types ignoring charset/case", () => {
    expect(isHlsContentType("application/vnd.apple.mpegurl")).toBe(true);
    expect(isHlsContentType("application/x-mpegURL; charset=utf-8")).toBe(true);
    expect(isHlsContentType("audio/mpegurl")).toBe(true);
  });
  it("rejects others", () => {
    expect(isHlsContentType("video/mp4")).toBe(false);
    expect(isHlsContentType("")).toBe(false);
  });
});

describe("dedupeStreams", () => {
  it("removes duplicate urls preserving order", () => {
    const out = dedupeStreams([
      { url: "a" },
      { url: "b" },
      { url: "a" },
    ]);
    expect(out.map((s) => s.url)).toEqual(["a", "b"]);
  });
});
