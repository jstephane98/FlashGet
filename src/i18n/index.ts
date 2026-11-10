import { fr, type MsgKey } from "./fr";

export type { MsgKey };

/** Renvoie le texte FR de `key`, en remplaçant chaque `{nom}` par `vars.nom`. */
export function t(key: MsgKey, vars?: Record<string, string | number>): string {
  const template: string = fr[key];
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}
