import { Fragment, useSyncExternalStore, type ReactNode } from "react";
import { fr, type MsgKey } from "./fr";
import { en } from "./en";

export type { MsgKey };

/** Langue affichée. */
export type Lang = "fr" | "en";
/** Choix de l'utilisateur : « auto » suit la langue de Windows. */
export type LangPref = "auto" | Lang;

export const LANG_PREF_KEY = "ui.lang";
export const LANG_CACHE_KEY = "flashget.lang";
/** Nom de chaque langue dans sa propre langue, pour qu'on retrouve la sienne. */
export const LANG_NAMES: Record<Lang, string> = { fr: "Français", en: "English" };

const DICTS: Record<Lang, Record<MsgKey, string>> = { fr, en };

export function isLangPref(v: unknown): v is LangPref {
  return v === "auto" || v === "fr" || v === "en";
}

function navigatorLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

/** Langue du système : français si Windows est en français, anglais sinon. */
export function systemLang(languages: readonly string[] = navigatorLanguages()): Lang {
  const first = (languages[0] ?? "").toLowerCase();
  return first.startsWith("fr") ? "fr" : "en";
}

export function resolveLang(pref: LangPref, system: Lang = systemLang()): Lang {
  return pref === "auto" ? system : pref;
}

/** Nom du système, glissé dans les textes à la place de `{os}`. */
export type OsName = "Windows" | "Linux" | "macOS";

export function detectOs(ua: string = typeof navigator === "undefined" ? "" : navigator.userAgent): OsName {
  if (/Windows/i.test(ua)) return "Windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "macOS";
  if (/Linux|X11/i.test(ua)) return "Linux";
  return "Windows";
}

const OS_NAME: OsName = detectOs();

let current: Lang = "fr";
const listeners = new Set<() => void>();

export function getLang(): Lang {
  return current;
}

export function setLang(lang: Lang): void {
  if (lang === current) return;
  current = lang;
  if (typeof document !== "undefined") document.documentElement.lang = lang;
  listeners.forEach((l) => l());
}

export function subscribeLang(cb: () => void): () => void {
  listeners.add(cb);
  return () => { listeners.delete(cb); };
}

/** Langue courante, avec un nouveau rendu à chaque changement. */
export function useLang(): Lang {
  return useSyncExternalStore(subscribeLang, getLang, getLang);
}

/** Paramètres régionaux pour les dates et les nombres. */
export function locale(lang: Lang = current): string {
  if (lang === "fr") return "fr-FR";
  const nav = typeof navigator === "undefined" ? "" : navigator.language ?? "";
  return nav.toLowerCase().startsWith("en") ? nav : "en-US";
}

/** Renvoie le texte de `key` dans la langue courante, en remplaçant chaque `{nom}` par `vars.nom`. */
export function t(key: MsgKey, vars?: Record<string, string | number>): string {
  const template: string = DICTS[current][key] ?? fr[key];
  const all: Record<string, string | number> = { os: OS_NAME, ...vars };
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in all ? String(all[name]) : match,
  );
}

/** Comme `t`, mais les valeurs peuvent être des éléments (ports, adresses en mono).
 *  Une variable absente de `vars` reste telle quelle dans le texte. */
export function tNodes(key: MsgKey, vars: Record<string, ReactNode>): ReactNode[] {
  const template: string = DICTS[current][key] ?? fr[key];
  return template.split(/(\{\w+\})/).map((part, i) => {
    const m = /^\{(\w+)\}$/.exec(part);
    if (m && m[1] in vars) return <Fragment key={i}>{vars[m[1]]}</Fragment>;
    return m && m[1] === "os" ? OS_NAME : part;
  });
}
