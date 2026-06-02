import { describe, it, expect } from "vitest";
import { formatBytes, formatSpeed, percent } from "./format";

describe("format", () => {
  it("formats bytes", () => {
    expect(formatBytes(0)).toBe("0 o");
    expect(formatBytes(1024)).toBe("1,0 Ko");
    expect(formatBytes(1536)).toBe("1,5 Ko");
    expect(formatBytes(1048576)).toBe("1,0 Mo");
  });

  it("formats speed", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(1048576)).toBe("1,0 Mo/s");
  });

  it("computes percent", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(0, null)).toBe(0);
  });
});
