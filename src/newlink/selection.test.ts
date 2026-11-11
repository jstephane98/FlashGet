import { describe, it, expect } from "vitest";
import type { MediaFormat } from "../api";
import { availableKinds, defaultFormat, formatsOf, outputExtension, withExtension, parseClock, validateCut, formatDuration } from "./selection";

const f = (o: Partial<MediaFormat>): MediaFormat => ({
  index: 0, kind: "av", format_id: "x", height: null, label: "", container: null,
  codecs: null, fps: null, est_size: null, recommended: false, ...o,
});
const a720 = f({ label: "720p", height: 720 });
const a1080rec = f({ label: "1080p", height: 1080, recommended: true });
const au = f({ kind: "audio", container: "M4A", label: "Audio" });
const mp3 = f({ kind: "audio", format_id: "convert", container: "MP3" });
const convM4a = f({ kind: "audio", format_id: "convert", label: "M4A · 256 kb/s", container: "M4A" });
const convOpus = f({ kind: "audio", format_id: "convert", label: "Opus", container: "OPUS" });
const m4a = f({ kind: "audio", container: "M4A" });
const v1080 = f({ kind: "video", height: 1080 });

describe("selection", () => {
  it("onglets dans l'ordre et sans vides", () => {
    expect(availableKinds([au, a720])).toEqual(["av", "audio"]);
    expect(availableKinds([v1080, au, a720])).toEqual(["av", "audio", "video"]);
    expect(availableKinds([])).toEqual([]);
  });
  it("formatsOf filtre", () => { expect(formatsOf([a720, au], "audio")).toEqual([au]); });
  it("format par défaut = recommandé, sinon premier", () => {
    expect(defaultFormat([a720, a1080rec], "av")?.label).toBe("1080p");
    expect(defaultFormat([a720, a1080rec].map((x) => ({ ...x, recommended: false })), "av")?.label).toBe("720p");
    expect(defaultFormat([a720], "audio")).toBeUndefined();
  });
  it("extensions de sortie", () => {
    expect(outputExtension(mp3)).toBe(".mp3");
    expect(outputExtension(convM4a)).toBe(".m4a");
    expect(outputExtension(convOpus)).toBe(".opus");
    // Une ligne vidéo dont l'identifiant natif est « mp3 » n'est pas une conversion.
    expect(outputExtension(f({ format_id: "mp3" }))).toBe(".mp4");
    expect(outputExtension(m4a)).toBe(".m4a");
    expect(outputExtension(v1080)).toBe(".mp4");
    expect(outputExtension(a720)).toBe(".mp4");
  });
  it("withExtension remplace ou ajoute", () => {
    expect(withExtension("Clip.mp4", ".mp3")).toBe("Clip.mp3");
    expect(withExtension("Clip", ".mp4")).toBe("Clip.mp4");
    expect(withExtension("v1.2 final", ".mp4")).toBe("v1.2 final.mp4");
    expect(withExtension("a.WEBM", ".m4a")).toBe("a.m4a");
  });
  it("parseClock", () => {
    expect(parseClock("90")).toBe(90);
    expect(parseClock("1:30")).toBe(90);
    expect(parseClock("01:02:03")).toBe(3723);
    expect(parseClock("1:75")).toBeNull();
    expect(parseClock("")).toBeNull();
    expect(parseClock("abc")).toBeNull();
    expect(parseClock("1:2:3:4")).toBeNull();
    expect(parseClock("1::3")).toBeNull();
    expect(parseClock("-5")).toBeNull();
    expect(parseClock(" 1:30 ")).toBe(90);
  });
  it("validateCut", () => {
    expect(validateCut("1:30", "2:45", 768)).toBeNull();
    expect(validateCut("1:30", "", 768)).toBeNull();
    expect(validateCut("x", "", 768)).toBe("cut.badStart");
    expect(validateCut("0:10", "x", 768)).toBe("cut.badEnd");
    expect(validateCut("2:00", "1:00", 768)).toBe("cut.endBeforeStart");
    expect(validateCut("0:00", "20:00", 768)).toBe("cut.beyondEnd");
    expect(validateCut("0:00", "20:00", null)).toBeNull();
  });
  it("formatDuration", () => { expect(formatDuration(768)).toBe("12:48"); });
});
