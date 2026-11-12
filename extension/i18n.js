// Traduction de l'interface via chrome.i18n (_locales/<langue>/messages.json).
// Les textes statiques des pages portent data-i18n="clé" (contenu texte) ou
// data-i18n-title / data-i18n-placeholder / data-i18n-aria-label (attributs).

const ATTRS = [
  ["data-i18n-title", "title"],
  ["data-i18n-placeholder", "placeholder"],
  ["data-i18n-aria-label", "aria-label"],
];

// Remplit les éléments marqués de `root` avec getMessage(clé). Un message absent ("")
// laisse le texte d'origine en place. Pur : getMessage est fourni par l'appelant.
export function localize(root, getMessage) {
  for (const el of root.querySelectorAll("[data-i18n]")) {
    const text = getMessage(el.getAttribute("data-i18n"));
    if (text) el.textContent = text;
  }
  for (const [data, attr] of ATTRS) {
    for (const el of root.querySelectorAll(`[${data}]`)) {
      const text = getMessage(el.getAttribute(data));
      if (text) el.setAttribute(attr, text);
    }
  }
}

// Message traduit ; la clé elle-même si elle est introuvable (repérable à l'écran).
export function t(key, substitutions) {
  return chrome.i18n.getMessage(key, substitutions) || key;
}

// Ajoute à `parent` le message `text`, où `token` est remplacé par le nœud `node`
// (ex. un hôte en police mono au milieu d'une phrase traduite).
export function appendWithNode(parent, text, token, node) {
  const i = text.indexOf(token);
  if (i < 0) {
    parent.append(text);
    return;
  }
  parent.append(text.slice(0, i), node, text.slice(i + token.length));
}

if (typeof chrome !== "undefined" && chrome.i18n && typeof document !== "undefined") {
  // Langue réellement servie (repli sur l'anglais si la langue du navigateur n'est pas traduite).
  document.documentElement.lang = chrome.i18n.getMessage("uiLang") || chrome.i18n.getUILanguage();
  localize(document, (key) => chrome.i18n.getMessage(key));
}
