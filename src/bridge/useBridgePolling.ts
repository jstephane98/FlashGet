import { useEffect } from "react";
import * as api from "../api";
import type { BridgeStatus } from "../types";

const POLL_MS = 30_000;

/** Rust ne signale pas chaque ping de l'extension : on relit l'état toutes les 30 s
 *  tant qu'il peut changer sans événement (jeton refusé qui expire, activité affichée
 *  dans les Paramètres), et une fois à l'ouverture des Paramètres. */
export function useBridgePolling(
  state: BridgeStatus["state"] | undefined,
  settingsOpen: boolean,
  onStatus: (s: BridgeStatus) => void,
) {
  const poll = state === "rejected" || (settingsOpen && state === "connected");
  useEffect(() => {
    if (settingsOpen) api.getBridgeStatus().then(onStatus).catch(() => {});
  }, [settingsOpen, onStatus]);
  useEffect(() => {
    if (!poll) return;
    const id = setInterval(() => { api.getBridgeStatus().then(onStatus).catch(() => {}); }, POLL_MS);
    return () => clearInterval(id);
  }, [poll, onStatus]);
}
