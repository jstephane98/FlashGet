import { describe, it, expect } from "vitest";
import * as api from "./api";

describe("api", () => {
  it("n'écoute plus download://added (jamais émis depuis la page Nouveau)", () => {
    expect("onDownloadAdded" in api).toBe(false);
  });
});
