import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { localize, appendWithNode } from "./i18n.js";

const extDir = resolve(__dirname);
const read = (p) => readFileSync(join(extDir, p), "utf8");
const messages = (lang) => JSON.parse(read(`_locales/${lang}/messages.json`));
const en = messages("en");
const fr = messages("fr");

// Noms de placeholders déclarés (Chrome les compare sans tenir compte de la casse).
const declared = (m) => Object.keys(m.placeholders || {}).map((p) => p.toLowerCase()).sort();
// Placeholders $NOM$ utilisés dans le texte du message.
const used = (m) => [...new Set([...m.message.matchAll(/\$([A-Za-z0-9_@]+)\$/g)].map((x) => x[1].toLowerCase()))].sort();

describe("traductions de l'extension", () => {
  it("fr et en ont exactement les mêmes clés", () => {
    expect(Object.keys(fr).sort()).toEqual(Object.keys(en).sort());
  });

  it("fr et en ont les mêmes placeholders, tous déclarés et utilisés", () => {
    for (const key of Object.keys(en)) {
      expect(declared(fr[key]), key).toEqual(declared(en[key]));
      for (const m of [en[key], fr[key]]) {
        expect(used(m), key).toEqual(declared(m));
        for (const p of Object.values(m.placeholders || {})) expect(p.content, key).toMatch(/^\$\d$/);
      }
      const contents = (m) => Object.entries(m.placeholders || {}).map(([k, p]) => k.toLowerCase() + "=" + p.content).sort();
      expect(contents(fr[key]), key).toEqual(contents(en[key]));
    }
  });

  it("aucun message vide", () => {
    for (const [lang, all] of [["en", en], ["fr", fr]]) {
      for (const [key, m] of Object.entries(all)) expect(m.message.trim(), `${lang}.${key}`).not.toBe("");
    }
  });

  it("chaque clé utilisée par le code, les pages et le manifeste existe", () => {
    const files = readdirSync(extDir).filter((f) => /\.(js|html|json)$/.test(f) && !/\.test\.js$/.test(f));
    const keys = new Set();
    for (const f of files) {
      const src = read(f);
      for (const m of src.matchAll(/__MSG_(\w+)__/g)) keys.add(m[1]);
      for (const m of src.matchAll(/data-i18n(?:-[a-z-]+)?="(\w+)"/g)) keys.add(m[1]);
      for (const m of src.matchAll(/getMessage\("(\w+)"/g)) keys.add(m[1]);
      for (const m of src.matchAll(/\bt\(\s*"(\w+)"/g)) keys.add(m[1]);
      for (const m of src.matchAll(/\?\s*"(\w+)"\s*:\s*"(\w+)"/g)) {
        // t(cond ? "cléA" : "cléB") et ternaires imbriqués dans un appel t(...)
        if (en[m[1]] || en[m[2]]) keys.add(m[1]).add(m[2]);
      }
    }
    expect(keys.size).toBeGreaterThan(20);
    for (const k of keys) expect(en[k], k).toBeDefined();
  });
});

describe("localize", () => {
  it("remplit le texte et les attributs marqués", () => {
    document.body.innerHTML = `
      <p data-i18n="a">old</p>
      <input data-i18n-placeholder="b" placeholder="old" />
      <button data-i18n-aria-label="c" data-i18n-title="d"></button>
      <span data-i18n="missing">kept</span>`;
    const dict = { a: "Texte", b: "Indice", c: "Nom", d: "Titre" };
    localize(document, (k) => dict[k] || "");
    expect(document.querySelector("p").textContent).toBe("Texte");
    expect(document.querySelector("input").getAttribute("placeholder")).toBe("Indice");
    expect(document.querySelector("button").getAttribute("aria-label")).toBe("Nom");
    expect(document.querySelector("button").getAttribute("title")).toBe("Titre");
    expect(document.querySelector("span").textContent).toBe("kept");
  });
});

describe("appendWithNode", () => {
  it("insère le nœud à la place du jeton", () => {
    const parent = document.createElement("small");
    const host = document.createElement("span");
    host.textContent = "exemple.com";
    appendWithNode(parent, "sur {host}", "{host}", host);
    expect(parent.innerHTML).toBe("sur&nbsp;<span>exemple.com</span>");
    const parent2 = document.createElement("small");
    appendWithNode(parent2, "{host} is ignored", "{host}", host.cloneNode(true));
    expect(parent2.innerHTML).toBe("<span>exemple.com</span>&nbsp;is ignored");
  });

  it("sans jeton, ajoute le texte seul", () => {
    const parent = document.createElement("small");
    appendWithNode(parent, "texte", "{host}", document.createElement("span"));
    expect(parent.innerHTML).toBe("texte");
  });
});
