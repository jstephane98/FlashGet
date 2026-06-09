import { describe, it, expect } from "vitest";
import { recordStream, getStreams, clearStreams } from "./stream-store.js";

// Faux chrome.storage.session : un objet `data` partagé + l'API get/set/remove async.
function fakeSession(data = {}) {
  return {
    data,
    get: async (k) => (k in data ? { [k]: data[k] } : {}),
    set: async (obj) => { Object.assign(data, obj); },
    remove: async (k) => { delete data[k]; },
  };
}

describe("stream-store (persistance des flux détectés)", () => {
  it("enregistre puis relit un flux pour un onglet", async () => {
    const s = fakeSession();
    await recordStream(s, 1, "https://cdn/a.m3u8", 1000);
    expect(await getStreams(s, 1)).toEqual([{ url: "https://cdn/a.m3u8", ts: 1000 }]);
  });

  it("survit à un redémarrage du service worker (données dans storage, pas en mémoire)", async () => {
    // Régression visée : l'ancienne Map en mémoire perdait tout à l'éviction du SW.
    const data = {};
    await recordStream(fakeSession(data), 7, "https://cdn/live.m3u8", 1000); // SW vivant
    // SW tué puis relancé : NOUVELLE instance de l'API, mais storage.session conserve `data`.
    expect(await getStreams(fakeSession(data), 7)).toEqual([{ url: "https://cdn/live.m3u8", ts: 1000 }]);
  });

  it("dédoublonne par URL et conserve l'ordre d'apparition", async () => {
    const s = fakeSession();
    await recordStream(s, 1, "https://cdn/a.m3u8", 1);
    await recordStream(s, 1, "https://cdn/a.m3u8", 2); // doublon ignoré
    await recordStream(s, 1, "https://cdn/b.m3u8", 3);
    expect((await getStreams(s, 1)).map((x) => x.url)).toEqual([
      "https://cdn/a.m3u8",
      "https://cdn/b.m3u8",
    ]);
  });

  it("borne la liste à 20 entrées", async () => {
    const s = fakeSession();
    for (let i = 0; i < 25; i++) await recordStream(s, 1, `https://cdn/${i}.m3u8`, i);
    expect((await getStreams(s, 1)).length).toBe(20);
  });

  it("ignore les tabId négatifs", async () => {
    const s = fakeSession();
    await recordStream(s, -1, "https://cdn/a.m3u8", 1);
    expect(await getStreams(s, -1)).toEqual([]);
  });

  it("clearStreams purge l'onglet", async () => {
    const s = fakeSession();
    await recordStream(s, 1, "https://cdn/a.m3u8", 1);
    await clearStreams(s, 1);
    expect(await getStreams(s, 1)).toEqual([]);
  });

  it("isole les onglets entre eux", async () => {
    const s = fakeSession();
    await recordStream(s, 1, "https://cdn/a.m3u8", 1);
    await recordStream(s, 2, "https://cdn/b.m3u8", 2);
    expect(await getStreams(s, 1)).toEqual([{ url: "https://cdn/a.m3u8", ts: 1 }]);
    expect(await getStreams(s, 2)).toEqual([{ url: "https://cdn/b.m3u8", ts: 2 }]);
  });
});
