import { describe, it, expect } from "vitest";
import { buildCapture } from "./buildCapture.js";

describe("buildCapture", () => {
  it("builds a capture message for a valid url", () => {
    expect(buildCapture("http://x/file.zip")).toEqual({
      type: "capture",
      url: "http://x/file.zip",
    });
  });

  it("trims surrounding whitespace", () => {
    expect(buildCapture("  http://x/f.bin  ")).toEqual({
      type: "capture",
      url: "http://x/f.bin",
    });
  });

  it("throws on an empty url", () => {
    expect(() => buildCapture("   ")).toThrow();
  });
});
