// @ts-expect-error module .mjs sans déclaration de types
import { buildManifest, checkPubkey, platformTarget } from "./manifest.mjs";

const base = {
  version: "2.0.0",
  notes: "Première version",
  pubDate: new Date("2026-11-11T10:00:00Z"),
  signature: "c2lnbmF0dXJl\n",
  fileName: "FlashGet_2.0.0_x64-setup.exe",
  repo: "jstephane98/FlashGet",
};

describe("buildManifest", () => {
  it("construit l'URL exacte de l'installeur", () => {
    const m = buildManifest(base);
    expect(m.platforms["windows-x86_64"].url).toBe(
      "https://github.com/jstephane98/FlashGet/releases/download/v2.0.0/FlashGet_2.0.0_x64-setup.exe",
    );
    expect(m.version).toBe("2.0.0");
    expect(m.notes).toBe("Première version");
  });

  it("retire le saut de ligne final de la signature", () => {
    const m = buildManifest(base);
    expect(m.platforms["windows-x86_64"].signature).toBe("c2lnbmF0dXJl");
  });

  it("formate pub_date en RFC 3339", () => {
    const m = buildManifest(base);
    expect(m.pub_date).toBe("2026-11-11T10:00:00.000Z");
    expect(m.pub_date).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/);
  });
});

describe("plusieurs plateformes", () => {
  it("donne les fichiers de chaque système", () => {
    expect(platformTarget("win32", "2.1.0")).toMatchObject({ key: "windows-x86_64", bundleDir: "nsis", updater: "FlashGet_2.1.0_x64-setup.exe" });
    const linux = platformTarget("linux", "2.1.0");
    expect(linux).toMatchObject({ key: "linux-x86_64", bundleDir: "appimage", updater: "FlashGet_2.1.0_amd64.AppImage" });
    expect(linux.files).toContain("../deb/FlashGet_2.1.0_amd64.deb");
    expect(platformTarget("darwin", "2.1.0")).toBeNull();
  });

  it("ajoute Linux au manifeste Windows de la même version", () => {
    const win = buildManifest(base);
    const both = buildManifest({ ...base, notes: "", platform: "linux-x86_64", fileName: "FlashGet_2.0.0_amd64.AppImage", existing: win });
    expect(Object.keys(both.platforms).sort()).toEqual(["linux-x86_64", "windows-x86_64"]);
    expect(both.platforms["windows-x86_64"]).toEqual(win.platforms["windows-x86_64"]);
    expect(both.platforms["linux-x86_64"].url).toBe("https://github.com/jstephane98/FlashGet/releases/download/v2.0.0/FlashGet_2.0.0_amd64.AppImage");
    expect(both.notes).toBe("Première version");
  });

  it("ignore le manifeste d'une autre version", () => {
    const old = buildManifest({ ...base, version: "1.9.0" });
    const m = buildManifest({ ...base, platform: "linux-x86_64", existing: old });
    expect(Object.keys(m.platforms)).toEqual(["linux-x86_64"]);
  });
});

describe("checkPubkey", () => {
  it("refuse la clé publique de remplacement", () => {
    const r = checkPubkey({ plugins: { updater: { pubkey: "REMPLACER_PAR_LA_CLE_PUBLIQUE" } } });
    expect(r.ok).toBe(false);
    expect(r.message).toContain("docs/RELEASE.md");
  });

  it("refuse une clé absente", () => {
    expect(checkPubkey({}).ok).toBe(false);
  });

  it("accepte une vraie clé", () => {
    expect(checkPubkey({ plugins: { updater: { pubkey: "dW50cnVzdGVkIGNvbW1lbnQ6IA==" } } }).ok).toBe(true);
  });
});
