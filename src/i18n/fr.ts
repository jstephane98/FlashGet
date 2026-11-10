export const fr = {
  "nav.downloads": "Téléchargements",
  "window.minimize": "Réduire",
  "window.maximize": "Agrandir",
  "window.close": "Fermer",
  "status.counts": "{active} actifs · {queued} en file",
} as const;

export type MsgKey = keyof typeof fr;
