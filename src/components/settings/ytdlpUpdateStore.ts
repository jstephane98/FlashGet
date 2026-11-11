import { useSyncExternalStore } from "react";
import * as api from "../../api";
import type { UpdateProgress, YtdlpUpdated } from "../../api";

export type YtdlpStep = "check" | "update";

export type YtdlpUpdateState =
  | { k: "idle" }
  | { k: "checking" }
  | { k: "uptodate" }
  | { k: "updating"; target: string; p: UpdateProgress | null }
  | { k: "failed"; step: YtdlpStep; msg: string };

export interface YtdlpSnapshot {
  state: YtdlpUpdateState;
  /** Version affichée (`undefined` pendant le chargement, `null` si introuvable). */
  version: string | null | undefined;
  lastCheck: number | null;
  /** Version déjà téléchargée, utilisée au prochain démarrage. */
  pending: string | null;
}

const initial = (): YtdlpSnapshot => ({ state: { k: "idle" }, version: undefined, lastCheck: null, pending: null });

/** État du flux yt-dlp, conservé hors du composant (la section peut être démontée en cours de mise à jour). */
let snap: YtdlpSnapshot = initial();
const listeners = new Set<() => void>();

function set(next: Partial<YtdlpSnapshot>) {
  snap = { ...snap, ...next };
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};
const getSnapshot = () => snap;

export function useYtdlpUpdate(): YtdlpSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot);
}

/** Pour les tests uniquement. */
export function resetYtdlpUpdateStore() {
  snap = initial();
  listeners.forEach((l) => l());
}

const busy = () => snap.state.k === "checking" || snap.state.k === "updating";

export async function refreshVersion() {
  try { set({ version: await api.getYtdlpVersion() }); } catch { set({ version: null }); }
}

export async function refreshLastCheck() {
  try { set({ lastCheck: (await api.getYtdlpLastCheck()) ?? null }); } catch { /* sans importance */ }
}

/** Recherche manuelle ; `report` reçoit la dernière version disponible, ou null. */
export async function checkYtdlpNow(report: (latest: string | null) => void) {
  if (busy()) return;
  set({ state: { k: "checking" } });
  try {
    const r = await api.checkYtdlpUpdate();
    if (r.current !== undefined && r.current !== snap.version) set({ version: r.current });
    // La version trouvée est déjà téléchargée : rien de nouveau à proposer.
    const fresh = r.available && r.latest !== snap.pending;
    report(fresh ? r.latest : null);
    set({ state: r.available ? { k: "idle" } : { k: "uptodate" }, lastCheck: r.last_check ?? snap.lastCheck });
    await refreshLastCheck();
  } catch (e) {
    set({ state: { k: "failed", step: "check", msg: String(e) } });
  }
}

/** Prend en compte une mise à jour terminée (manuelle ou automatique au démarrage). */
export async function applyUpdated(r: YtdlpUpdated, report: (latest: string | null) => void) {
  set({ pending: r.pending ? r.version : null, state: { k: "idle" } });
  report(null);
  await refreshVersion();
}

export async function updateYtdlpNow(target: string, report: (latest: string | null) => void) {
  if (busy()) return;
  set({ state: { k: "updating", target, p: null } });
  let un: (() => void) | undefined;
  try {
    un = await api.onYtdlpUpdateProgress((p) => {
      if (snap.state.k === "updating") set({ state: { k: "updating", target, p } });
    });
    const r = await api.updateYtdlp();
    await applyUpdated(r, report);
  } catch (e) {
    set({ state: { k: "failed", step: "update", msg: String(e) } });
  } finally {
    un?.();
  }
}
