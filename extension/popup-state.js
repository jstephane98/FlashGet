// Logique pure de la fenêtre de l'extension (sans chrome.*).

// Vrai si le service d'arrière-plan qui répond n'est pas celui de cette version de
// l'extension. Chrome recharge la fenêtre depuis le disque à chaque ouverture, mais garde
// l'ancien service d'arrière-plan tant que l'extension n'est pas rechargée : après une mise
// à jour des fichiers, la fenêtre parle alors à l'ancienne version (qui ne renvoie pas sa
// version, ou une autre).
export function isStaleBackground(status, popupVersion) {
  if (!status) return false; // pas de réponse : géré comme « non connecté »
  return status.version !== popupVersion;
}
