export type ThemePref = "dark" | "light" | "system";
export type Theme = "dark" | "light";

export const THEME_CACHE_KEY = "flashget.theme";
export const THEME_PREF_KEY = "ui.theme";

export function isThemePref(v: unknown): v is ThemePref {
  return v === "dark" || v === "light" || v === "system";
}

export function resolveTheme(pref: ThemePref, systemDark: boolean): Theme {
  if (pref === "system") return systemDark ? "dark" : "light";
  return pref;
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}
