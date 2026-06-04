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

  it("includes referrer, cookie and user_agent when provided", () => {
    expect(
      buildCapture("http://x/f.zip", {
        referrer: "http://x/page",
        cookie: "sid=1; a=2",
        user_agent: "UA/1",
      }),
    ).toEqual({
      type: "capture",
      url: "http://x/f.zip",
      referrer: "http://x/page",
      cookie: "sid=1; a=2",
      user_agent: "UA/1",
    });
  });

  it("omits empty / missing extras", () => {
    expect(buildCapture("http://x/f.zip", { referrer: "", cookie: undefined })).toEqual({
      type: "capture",
      url: "http://x/f.zip",
    });
  });

  it("includes filename when provided", () => {
    expect(buildCapture("http://x/dl?id=1", { filename: "movie.mp4" })).toEqual({
      type: "capture",
      url: "http://x/dl?id=1",
      filename: "movie.mp4",
    });
  });
});
