// Persistance des flux HLS/DASH détectés par onglet, dans chrome.storage.session.
//
// Pourquoi : l'ancienne implémentation gardait ces flux dans une `Map` en mémoire du
// service worker MV3. Chrome tue le SW après ~30 s d'inactivité, ce qui vidait la Map ;
// au clic « smart » suivant, le flux sniffé (fallback) était perdu et l'app tentait
// yt-dlp seul (échec sur les sites non supportés). `chrome.storage.session` survit à
// l'éviction du SW (et se vide à la fermeture du navigateur), durée de vie idéale.
//
// `storage` est injecté (= chrome.storage.session) pour rester testable sans chrome.*.
import { dedupeStreams } from "./media-rules.js";

const MAX = 20;
const key = (tabId) => `streams:${tabId}`;

// Enregistre un flux détecté pour `tabId` (dédupliqué, borné, ordre conservé).
export async function recordStream(storage, tabId, url, now) {
  if (tabId < 0) return;
  const k = key(tabId);
  const cur = (await storage.get(k))[k] || [];
  const next = dedupeStreams([...cur, { url, ts: now }]).slice(-MAX);
  await storage.set({ [k]: next });
}

// Renvoie les flux détectés pour `tabId` (liste éventuellement vide).
export async function getStreams(storage, tabId) {
  const k = key(tabId);
  return (await storage.get(k))[k] || [];
}

// Purge les flux d'un onglet (navigation vers une nouvelle URL, ou fermeture).
export async function clearStreams(storage, tabId) {
  await storage.remove(key(tabId));
}
