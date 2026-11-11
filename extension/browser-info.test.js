import { describe, it, expect } from "vitest";
import { browserName } from "./browser-info.js";

const brands = (...names) => ({ brands: names.map((brand) => ({ brand, version: "1" })) });

describe("browserName", () => {
  it("detects Edge from userAgentData brands", () => {
    expect(browserName(brands("Chromium", "Microsoft Edge", "Not A;Brand"), "")).toBe("Edge");
  });
  it("detects Brave from userAgentData brands", () => {
    expect(browserName(brands("Chromium", "Brave"), "")).toBe("Brave");
  });
  it("detects Chrome from userAgentData brands", () => {
    expect(browserName(brands("Chromium", "Google Chrome"), "")).toBe("Chrome");
  });
  it("falls back to Chromium when only Chromium is advertised", () => {
    expect(browserName(brands("Chromium", "Not A;Brand"), "")).toBe("Chromium");
  });
  it("uses the UA string without userAgentData", () => {
    expect(browserName(undefined, "Mozilla/5.0 Chrome/120.0 Safari/537.36 Edg/120.0")).toBe("Edge");
    expect(browserName(undefined, "Mozilla/5.0 Chrome/120.0 Safari/537.36")).toBe("Chrome");
    expect(browserName(undefined, "Mozilla/5.0 Chromium/120.0")).toBe("Chromium");
    expect(browserName(undefined, "")).toBe("Chromium");
  });
});
