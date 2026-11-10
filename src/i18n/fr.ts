export const fr = {
  "nav.downloads": "Téléchargements",
  "nav.new": "Nouveau",
  "nav.completed": "Terminés",
  "nav.queue": "File d'attente",
  "nav.scheduler": "Planificateur",
  "nav.settings": "Paramètres",
  "status.limit.none": "Limite : aucune",
  "status.limit": "Limite : {speed}",
  "status.bridge.on": "Extension : prête",
  "status.bridge.off": "Extension : inactive",
  "status.speed": "↓ {speed}",
  "window.minimize": "Réduire",
  "window.maximize": "Agrandir",
  "window.close": "Fermer",
  "status.counts": "{active} actifs · {queued} en file",
} as const;

export type MsgKey = keyof typeof fr;
