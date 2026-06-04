import { describe, it, expect } from "vitest";
import { extensionOf, parseCaptureTypes, shouldCapture, DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";

describe("extensionOf", () => {
  it("extracts the lowercase last extension", () => {
    expect(extensionOf("video.MP4")).toBe("mp4");
    expect(extensionOf("backup.tar.gz")).toBe("gz");
  });
  it("ignores url query/fragment and path", () => {
    expect(extensionOf("https://h/path/file.zip?x=1#y")).toBe("zip");
  });
  it("returns empty when there is no extension", () => {
    expect(extensionOf("https://h/download?id=123")).toBe("");
    expect(extensionOf("noext")).toBe("");
    expect(extensionOf("")).toBe("");
  });
});

describe("parseCaptureTypes", () => {
  it("splits on commas/spaces/newlines, lowercases, strips dots, dedupes", () => {
    expect(parseCaptureTypes("zip, exe,mp4")).toEqual(["zip", "exe", "mp4"]);
    expect(parseCaptureTypes(".pdf .PDF\nrar")).toEqual(["pdf", "rar"]);
    expect(parseCaptureTypes("")).toEqual([]);
  });
  it("the default list is non-empty and includes common types", () => {
    const t = parseCaptureTypes(DEFAULT_CAPTURE_TYPES);
    expect(t).toContain("zip");
    expect(t).toContain("exe");
    expect(t).toContain("mp4");
  });
});

describe("shouldCapture", () => {
  const settings = { autoCapture: true, types: ["zip", "mp4"] };
  it("captures a whitelisted http(s) type", () => {
    expect(shouldCapture({ url: "https://h/f.zip" }, settings)).toBe(true);
  });
  it("uses the filename extension when the url has none", () => {
    expect(shouldCapture({ url: "https://h/download?id=1", filename: "movie.mp4" }, settings)).toBe(true);
  });
  it("rejects non-whitelisted types", () => {
    expect(shouldCapture({ url: "https://h/f.txt" }, settings)).toBe(false);
  });
  it("rejects when auto-capture is off", () => {
    expect(shouldCapture({ url: "https://h/f.zip" }, { autoCapture: false, types: ["zip"] })).toBe(false);
  });
  it("rejects blob/data/non-http schemes", () => {
    expect(shouldCapture({ url: "blob:https://h/abc" }, settings)).toBe(false);
    expect(shouldCapture({ url: "data:text/plain;base64,AAAA" }, settings)).toBe(false);
    expect(shouldCapture({ url: "file:///c:/x.zip" }, settings)).toBe(false);
  });
});
