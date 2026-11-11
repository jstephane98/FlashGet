import { describe, it, expect } from "vitest";
import { hostOf, normalizeHost, isIgnored, matchingSite } from "./site-rules.js";

describe("normalizeHost", () => {
  it("normalizes the contract cases", () => {
    expect(normalizeHost("WWW.Exemple.com")).toBe("exemple.com");
    expect(normalizeHost("exemple.com:8080")).toBe("exemple.com");
    expect(normalizeHost("*.exemple.com")).toBe("exemple.com");
    expect(normalizeHost("exemple.com.")).toBe("exemple.com");
    expect(normalizeHost("exemple.com.:8080")).toBe("exemple.com");
    expect(normalizeHost("https://www.Exemple.com/page")).toBe("exemple.com");
    expect(normalizeHost("https://exemple.com?x=1")).toBe("exemple.com");
    expect(normalizeHost("localhost")).toBe("localhost");
    expect(normalizeHost("192.168.1.2")).toBe("192.168.1.2");
  });
  it("converts IDN to punycode", () => {
    expect(normalizeHost("éxemple.com")).toBe("xn--xemple-9ua.com");
  });
  it("strips www. only once", () => {
    expect(normalizeHost("www.www.exemple.com")).toBe("www.exemple.com");
  });
  it("rejects invalid input with null", () => {
    expect(normalizeHost("  ")).toBeNull();
    expect(normalizeHost(null)).toBeNull();
    expect(normalizeHost("www.")).toBeNull();
    expect(normalizeHost("www.com")).toBeNull();
    expect(normalizeHost("intranet")).toBeNull();
    expect(normalizeHost("::1")).toBeNull();
    expect(normalizeHost("[::1]")).toBeNull();
    expect(normalizeHost("-a.com")).toBeNull();
    expect(normalizeHost("a b.com")).toBeNull();
  });
});

describe("hostOf", () => {
  it("returns the normalized host of an http(s) url", () => {
    expect(hostOf("https://www.Exemple.com/page?x=1")).toBe("exemple.com");
    expect(hostOf("http://localhost:3000/a")).toBe("localhost");
    expect(hostOf("https://éxemple.com/")).toBe("xn--xemple-9ua.com");
  });
  it("returns null for invalid or non-http urls", () => {
    expect(hostOf("")).toBeNull();
    expect(hostOf("pas une url")).toBeNull();
    expect(hostOf(undefined)).toBeNull();
    expect(hostOf("chrome://extensions")).toBeNull();
    expect(hostOf("blob:https://h/abc")).toBeNull();
    expect(hostOf("http://[::1]:8080/")).toBeNull();
  });
});

describe("isIgnored / matchingSite", () => {
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
  it("matches an IPv4 site only exactly", () => {
    expect(isIgnored("http://192.168.1.2/", ["192.168.1.2"])).toBe(true);
    expect(isIgnored("http://x.192.168.1.2/", ["192.168.1.2"])).toBe(false);
  });
  it("matches punycode hosts", () => {
    expect(isIgnored("https://xn--xemple-9ua.com/", ["xn--xemple-9ua.com"])).toBe(true);
  });
  it("is false for invalid urls or empty lists", () => {
    expect(isIgnored("nope", ["exemple.com"])).toBe(false);
    expect(isIgnored("https://exemple.com", [])).toBe(false);
    expect(isIgnored("https://exemple.com", undefined)).toBe(false);
  });
  it("matchingSite names the entry causing the match", () => {
    expect(matchingSite("https://sub.exemple.com/", ["autre.com", "exemple.com"])).toBe("exemple.com");
    expect(matchingSite("https://exemple.com/", ["exemple.com"])).toBe("exemple.com");
    expect(matchingSite("https://autre.org/", ["exemple.com"])).toBeNull();
    expect(matchingSite("nope", ["exemple.com"])).toBeNull();
  });
});
