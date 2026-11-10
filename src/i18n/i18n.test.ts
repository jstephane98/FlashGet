import { describe, it, expect } from "vitest";
import { t } from "./index";

describe("t()", () => {
  it("interpole les variables", () => {
    expect(t("status.counts", { active: 3, queued: 1 })).toBe("3 actifs · 1 en file");
  });

  it("renvoie le texte tel quel sans variable", () => {
    expect(t("nav.downloads")).toBe("Téléchargements");
  });
});
