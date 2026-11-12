import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import * as api from "../api";
import { LANG_CACHE_KEY, LANG_PREF_KEY, isLangPref, resolveLang, setLang, systemLang, useLang, type Lang, type LangPref } from "./index";

interface LangCtx {
  pref: LangPref;
  lang: Lang;
  system: Lang;
  setPref(p: LangPref): void;
}

const Ctx = createContext<LangCtx>({ pref: "auto", lang: "fr", system: "fr", setPref: () => {} });

function readCache(): LangPref {
  try {
    const v = localStorage.getItem(LANG_CACHE_KEY);
    if (isLangPref(v)) return v;
  } catch { /* stockage indisponible */ }
  return "auto";
}

/** Applique la langue tout de suite (avant le premier rendu) et la transmet au moteur. */
function apply(pref: LangPref, system: Lang): void {
  const lang = resolveLang(pref, system);
  setLang(lang);
  api.setAppLanguage(lang).catch(() => {});
}

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [system] = useState<Lang>(() => systemLang());
  const [pref, setPrefState] = useState<LangPref>(() => {
    const p = readCache();
    setLang(resolveLang(p, system));
    return p;
  });
  const lang = useLang();

  // Transmet la langue au moteur (menu de la zone de notification, notifications, erreurs).
  useEffect(() => { api.setAppLanguage(lang).catch(() => {}); }, [lang]);

  // La valeur persistée côté Rust l'emporte sur le cache local.
  useEffect(() => {
    let cancelled = false;
    api.getUiPref(LANG_PREF_KEY).then((v) => {
      if (cancelled || !isLangPref(v)) return;
      setPrefState(v);
      setLang(resolveLang(v, system));
      try { localStorage.setItem(LANG_CACHE_KEY, v); } catch { /* ignoré */ }
    }).catch(() => {});
    return () => { cancelled = true; };
  }, [system]);

  const setPref = useCallback((p: LangPref) => {
    setPrefState(p);
    apply(p, system);
    try { localStorage.setItem(LANG_CACHE_KEY, p); } catch { /* ignoré */ }
    api.setUiPref(LANG_PREF_KEY, p).catch(() => {});
  }, [system]);

  const value = useMemo(() => ({ pref, lang, system, setPref }), [pref, lang, system, setPref]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useLanguage(): LangCtx {
  return useContext(Ctx);
}
