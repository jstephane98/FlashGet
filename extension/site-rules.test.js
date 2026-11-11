import { describe, it, expect } from "vitest";
import { hostOf, normalizeHost, isIgnored } from "./site-rules.js";

describe("normalizeHost", () => {
  it("lowercases, strips www. and the port", () => {
    expect(normalizeHost("WWW.Exemple.com")).toBe("exemple.com");
    expect(normalizeHost("exemple.com:8080")).toBe("exemple.com");
    expect(normalizeHost("localhost")).toBe("localhost");
    expect(normalizeHost("192.168.1.2")).toBe("192.168.1.2");
  });
  it("returns an empty string for blank input", () => {
    expect(normalizeHost("  ")).toBe("");
    expect(normalizeHost(null)).toBe("");
  });
});

describe("hostOf", () => {
  it("returns the normalized host of an http(s) url", () => {
    expect(hostOf("https://www.Exemple.com/page?x=1")).toBe("exemple.com");
    expect(hostOf("http://localhost:3000/a")).toBe("localhost");
  });
  it("returns null for invalid or non-http urls", () => {
    expect(hostOf("")).toBeNull();
    expect(hostOf("pas une url")).toBeNull();
    expect(hostOf(undefined)).toBeNull();
    expect(hostOf("chrome://extensions")).toBeNull();
    expect(hostOf("blob:https://h/abc")).toBeNull();
  });
});

describe("isIgnored", () => {
  it("matches the host itself and its subdomains", () => {
    expect(isIgnored("https://drive.google.com/x", ["drive.google.com"])).toBe(true);
    expect(isIgnored("https://docs.drive.google.com/x", ["drive.google.com"])).toBe(true);
  });
  it("does not ignore the parent domain", () => {
    expect(isIgnored("https://google.com/x", ["drive.google.com"])).toBe(false);
  });
  it("does not match on a plain suffix", () => {
    expect(isIgnored("https://exemple.com/", ["ample.com"])).toBe(false);
  });
  it("ignores www. and case in the url", () => {
    expect(isIgnored("https://WWW.Exemple.com/a", ["exemple.com"])).toBe(true);
  });
  it("is false for invalid urls or empty lists", () => {
    expect(isIgnored("nope", ["exemple.com"])).toBe(false);
    expect(isIgnored("https://exemple.com", [])).toBe(false);
    expect(isIgnored("https://exemple.com", undefined)).toBe(false);
  });
});
