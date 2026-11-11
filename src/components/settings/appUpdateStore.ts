import { useSyncExternalStore } from "react";
import * as api from "../../api";
import type { AppUpdateInfo, UpdateProgress } from "../../api";

export type UpdateStep = "check" | "download" | "install";

export type AppUpdateState =
  | { k: "idle" }
  | { k: "checking" }
  | { k: "uptodate" }
  | { k: "downloading"; p: UpdateProgress | null }
  | { k: "ready" }
  | { k: "installing" }
  | { k: "failed"; step: UpdateStep; msg: string };

export interface StoreSnapshot {
  state: AppUpdateState;
  lastCheck: number | null;
}

/** État du flux de mise à jour de FlashGet, conservé hors du composant (la section peut être démontée). */
let snap: StoreSnapshot = { state: { k: "idle" }, lastCheck: null };
const listeners = new Set<() => void>();

function set(next: Partial<StoreSnapshot>) {
  snap = { ...snap, ...next };
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};
const getSnapshot = () => snap;

export function useAppUpdate(): StoreSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot);
}

/** Pour les tests uniquement. */
export function resetAppUpdateStore() {
  snap = { state: { k: "idle" }, lastCheck: null };
  listeners.forEach((l) => l());
}

const busy = () => ["checking", "downloading", "installing"].includes(snap.state.k);

export async function refreshLastCheck() {
  try { set({ lastCheck: await api.getAppLastCheck() }); } catch { /* sans importance */ }
}

/** Recherche manuelle ; `report` est appelé même si la section a été démontée. */
export async function checkNow(report: (info: AppUpdateInfo | null) => void) {
  if (busy()) return;
  set({ state: { k: "checking" } });
  try {
    const found = await api.checkAppUpdate();
    report(found);
    set({ state: found ? { k: "idle" } : { k: "uptodate" } });
    await refreshLastCheck();
  } catch (e) {
    set({ state: { k: "failed", step: "check", msg: String(e) } });
  }
}

export async function downloadNow() {
  if (busy()) return;
  set({ state: { k: "downloading", p: null } });
  let un: (() => void) | undefined;
  try {
    un = await api.onAppUpdateProgress((p) => {
      if (snap.state.k === "downloading") set({ state: { k: "downloading", p } });
    });
    await api.downloadAppUpdate();
    set({ state: { k: "ready" } });
  } catch (e) {
    set({ state: { k: "failed", step: "download", msg: String(e) } });
  } finally {
    un?.();
  }
}

export async function installNow() {
  if (busy()) return;
  set({ state: { k: "installing" } });
  try {
    await api.installAppUpdate();
    // Encore en vie (hors Windows) : on revient à l'état prêt.
    set({ state: { k: "ready" } });
  } catch (e) {
    set({ state: { k: "failed", step: "install", msg: String(e) } });
  }
}
