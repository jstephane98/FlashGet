import { describe, it, expect, afterEach } from "vitest";
import { fr } from "./fr";
import { en } from "./en";
import { getLang, isLangPref, locale, resolveLang, setLang, subscribeLang, systemLang, t, type MsgKey } from "./index";
import { formatBytes, formatClock, formatDecimal, formatNumericDate, formatSpeed, recordingLabel } from "../format";

const vars = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

afterEach(() => setLang("fr"));

describe("textes anglais", () => {
  it("ont exactement les clés du français", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(fr).sort());
  });

  it("gardent les mêmes variables que le français", () => {
    for (const key of Object.keys(fr) as MsgKey[]) {
      expect(vars(en[key]), key).toEqual(vars(fr[key]));
    }
  });

  it("ne sont ni vides ni restés en français", () => {
    for (const key of Object.keys(fr) as MsgKey[]) {
      expect(en[key].trim(), key).not.toBe("");
      expect(en[key], key).not.toMatch(/[éèêàçù]/);
    }
  });
});

describe("langue", () => {
  it("suit Windows en automatique : français si Windows l'est, anglais sinon", () => {
    expect(systemLang(["fr-FR"])).toBe("fr");
    expect(systemLang(["fr-CA", "en-US"])).toBe("fr");
    expect(systemLang(["en-US"])).toBe("en");
    expect(systemLang(["de-DE"])).toBe("en");
    expect(systemLang([])).toBe("en");
    expect(resolveLang("auto", "fr")).toBe("fr");
    expect(resolveLang("auto", "en")).toBe("en");
    expect(resolveLang("en", "fr")).toBe("en");
    expect(resolveLang("fr", "en")).toBe("fr");
  });

  it("valide les choix enregistrés", () => {
    expect(isLangPref("auto")).toBe(true);
    expect(isLangPref("en")).toBe(true);
    expect(isLangPref("de")).toBe(false);
    expect(isLangPref(null)).toBe(false);
  });

  it("change à chaud et prévient les abonnés", () => {
    let calls = 0;
    const off = subscribeLang(() => { calls++; });
    setLang("en");
    expect(getLang()).toBe("en");
    expect(document.documentElement.lang).toBe("en");
    expect(t("nav.downloads")).toBe("Downloads");
    expect(t("status.counts", { active: 3, queued: 1 })).toBe("3 active · 1 queued");
    setLang("en");
    expect(calls).toBe(1);
    off();
    setLang("fr");
    expect(calls).toBe(1);
    expect(t("nav.downloads")).toBe("Téléchargements");
  });

  it("utilise les réglages régionaux de la langue", () => {
    expect(locale("fr")).toBe("fr-FR");
    expect(locale("en")).toMatch(/^en/);
  });
});

describe("formats en anglais", () => {
  it("unités, décimales et heure", () => {
    setLang("en");
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatSpeed(14260633)).toBe("13.6 MB/s");
    expect(formatDecimal(2.5)).toBe("2.5");
    expect(recordingLabel(12, 1048576, 65)).toBe("Recording · 01:05 · 12 segments · 1.0 MB");
    expect(formatClock(new Date(2026, 10, 12, 21, 14))).toMatch(/9:14\s?PM/i);
    expect(formatNumericDate(new Date(2026, 10, 12))).toMatch(/11\/12\/2026|12\/11\/2026/);
  });

  it("restent en français par défaut", () => {
    expect(formatBytes(1536)).toBe("1,5 Ko");
    expect(formatDecimal(2.5)).toBe("2,5");
    expect(formatClock(new Date(2026, 10, 12, 9, 5))).toBe("09:05");
    expect(formatNumericDate(new Date(2026, 10, 12))).toBe("12/11/2026");
  });
});
