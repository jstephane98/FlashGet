import { Fragment, type ReactNode } from "react";
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

/** Comme `t`, mais les valeurs peuvent être des éléments (ports, adresses en mono).
 *  Une variable absente de `vars` reste telle quelle dans le texte. */
export function tNodes(key: MsgKey, vars: Record<string, ReactNode>): ReactNode[] {
  const template: string = fr[key];
  return template.split(/(\{\w+\})/).map((part, i) => {
    const m = /^\{(\w+)\}$/.exec(part);
    return m && m[1] in vars ? <Fragment key={i}>{vars[m[1]]}</Fragment> : part;
  });
}
