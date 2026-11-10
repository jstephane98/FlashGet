import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import * as api from "../api";
import { THEME_CACHE_KEY, THEME_PREF_KEY, applyTheme, isThemePref, resolveTheme, type Theme, type ThemePref } from "./theme";

const DARK_QUERY = "(prefers-color-scheme: dark)";

interface ThemeCtx {
  pref: ThemePref;
  theme: Theme;
  setPref(p: ThemePref): void;
}

const Ctx = createContext<ThemeCtx>({ pref: "dark", theme: "dark", setPref: () => {} });

function readCache(): ThemePref {
  try {
    const v = localStorage.getItem(THEME_CACHE_KEY);
    if (isThemePref(v)) return v;
  } catch { /* stockage indisponible */ }
  return "dark";
}

function systemIsDark(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia(DARK_QUERY).matches;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [pref, setPrefState] = useState<ThemePref>(readCache);
  const [systemDark, setSystemDark] = useState<boolean>(systemIsDark);
  const theme = resolveTheme(pref, systemDark);

  useEffect(() => { applyTheme(theme); }, [theme]);

  // La valeur persistée côté Rust l'emporte sur le cache local.
  useEffect(() => {
    let cancelled = false;
    api.getUiPref(THEME_PREF_KEY).then((v) => {
      if (cancelled || !isThemePref(v)) return;
      setPrefState(v);
      try { localStorage.setItem(THEME_CACHE_KEY, v); } catch { /* ignoré */ }
    }).catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (pref !== "system" || typeof window.matchMedia !== "function") return;
    const mql = window.matchMedia(DARK_QUERY);
    setSystemDark(mql.matches);
    const onChange = (e: { matches: boolean }) => setSystemDark(e.matches);
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [pref]);

  const setPref = useCallback((p: ThemePref) => {
    setPrefState(p);
    try { localStorage.setItem(THEME_CACHE_KEY, p); } catch { /* ignoré */ }
    api.setUiPref(THEME_PREF_KEY, p).catch(() => {});
  }, []);

  const value = useMemo(() => ({ pref, theme, setPref }), [pref, theme, setPref]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useTheme(): ThemeCtx {
  return useContext(Ctx);
}
