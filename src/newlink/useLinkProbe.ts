import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import type { LinkProbe } from "../api";
import { t } from "../i18n";

export type ProbeState =
  | { status: "idle" }
  | { status: "probing"; url: string }
  | { status: "error"; url: string; message: string }
  | { status: "ready"; probe: LinkProbe };

const TIMEOUT_MS = 90_000;

export function useLinkProbe() {
  const [state, setState] = useState<ProbeState>({ status: "idle" });
  const token = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = () => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  /** Invalide toute analyse en cours et renvoie le nouveau jeton. */
  const supersede = () => {
    clearTimer();
    return ++token.current;
  };

  useEffect(() => () => { token.current++; clearTimer(); }, []);

  const fail = useCallback((url: string, message: string) => {
    supersede();
    setState({ status: "error", url, message });
  }, []);

  /** Délai maximal d'une analyse ; ignoré si une autre action l'a remplacée entre-temps. */
  const armTimeout = useCallback((mine: number, url: string) => {
    timer.current = setTimeout(() => {
      if (token.current !== mine) return;
      fail(url, t("new.timeout"));
    }, TIMEOUT_MS);
  }, [fail]);

  /** Analyse lancée par l'extension (résultat via `load` / `fail`) : même délai qu'`analyze`. */
  const showProbing = useCallback((url: string) => {
    const mine = supersede();
    setState({ status: "probing", url });
    armTimeout(mine, url);
  }, [armTimeout]);

  const load = useCallback((probe: LinkProbe) => {
    supersede();
    setState({ status: "ready", probe });
  }, []);

  const reset = useCallback(() => {
    supersede();
    setState({ status: "idle" });
  }, []);

  const analyze = useCallback((url: string) => {
    const u = url.trim();
    if (!/^https?:\/\//i.test(u)) {
      fail(url, t("new.badUrl"));
      return;
    }
    const mine = supersede();
    setState({ status: "probing", url: u });
    armTimeout(mine, u);
    api.probeLink(u).then(
      (probe) => {
        if (token.current !== mine) return;
        clearTimer();
        setState({ status: "ready", probe });
      },
      (err) => {
        if (token.current !== mine) return;
        clearTimer();
        setState({ status: "error", url: u, message: typeof err === "string" ? err : String(err?.message ?? err) });
      },
    );
  }, [fail, armTimeout]);

  return { state, analyze, showProbing, load, fail, reset };
}
