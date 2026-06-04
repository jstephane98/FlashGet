import { describe, it, expect } from "vitest";
import { formatBytes, formatSpeed, percent, formatElapsed, recordingLabel } from "./format";

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

  it("formats elapsed as MM:SS", () => {
    expect(formatElapsed(0)).toBe("00:00");
    expect(formatElapsed(5)).toBe("00:05");
    expect(formatElapsed(65)).toBe("01:05");
    expect(formatElapsed(3725)).toBe("62:05");
    expect(formatElapsed(-3)).toBe("00:00");
  });

  it("builds the recording label", () => {
    expect(recordingLabel(0, 0, 0)).toBe("Enregistrement · 00:00 · 0 segments · 0 o");
    expect(recordingLabel(12, 1048576, 65)).toBe("Enregistrement · 01:05 · 12 segments · 1,0 Mo");
  });
});
