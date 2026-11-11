import { describe, expect, it } from "vitest";
import { isStaleBackground } from "./popup-state.js";

describe("isStaleBackground", () => {
  it("ancienne version sans champ version : à recharger", () => {
    expect(isStaleBackground({ connected: true }, "2.0.0")).toBe(true);
  });
  it("version différente : à recharger", () => {
    expect(isStaleBackground({ connected: true, version: "1.9.0" }, "2.0.0")).toBe(true);
  });
  it("même version : rien à faire", () => {
    expect(isStaleBackground({ connected: true, version: "2.0.0" }, "2.0.0")).toBe(false);
  });
  it("pas de réponse : pas d'alerte de rechargement", () => {
    expect(isStaleBackground(undefined, "2.0.0")).toBe(false);
  });
});
