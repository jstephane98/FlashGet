// Le dossier de l'extension est fourni par l'installeur via `bundle.resources`
// (tauri.conf.json), qui liste les fichiers un par un. Ce test garantit que la liste
// suit le dossier : un fichier oublié casserait l'extension installée.
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const root = resolve(__dirname, "..");
const extDir = join(root, "extension");

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

const shipped = (path) => !/\.test\.js$/.test(path) && !/README\.md$/.test(path);

describe("dossier de l'extension dans l'installeur", () => {
  const conf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
  const resources = conf.bundle.resources;
  const listed = Object.keys(resources).map((src) => src.replace(/^\.\.\/extension\//, ""));
  const files = walk(extDir)
    .map((p) => relative(extDir, p).split("\\").join("/"))
    .filter(shipped);

  it("liste chaque fichier utile de l'extension", () => {
    expect(listed.sort()).toEqual(files.sort());
  });

  it("copie chaque fichier dans extension/ sous le même nom", () => {
    for (const [src, dest] of Object.entries(resources)) {
      expect(dest).toBe(src.replace(/^\.\.\//, ""));
    }
  });
});
