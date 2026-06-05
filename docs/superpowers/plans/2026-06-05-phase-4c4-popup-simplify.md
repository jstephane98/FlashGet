# Popup : un seul bouton vidéo intelligent — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **GARDE-FOU GIT (obligatoire) :** chaque subagent DOIT exécuter `git checkout phase-4c4-popup-simplify` avant tout commit.

**Goal:** Remplacer les multiples boutons « vidéo » du popup par un seul bouton intelligent (flux HLS/DASH sniffé → sinon yt-dlp), et garder le champ « coller une URL » clairement à part.

**Architecture:** Changement extension-only. Le bouton interroge `listMedia` au clic et route vers `sendMedia` (flux sniffé) ou `sendPage` (yt-dlp). `background.js`/bridge/app inchangés (`sendMedia`/`sendPage`/`listMedia` existent déjà).

**Tech Stack:** Extension Chromium MV3 (HTML/JS).

**Spec :** `docs/superpowers/specs/2026-06-05-phase-4c4-popup-simplify-design.md`

---

## File Structure
- `extension/popup.html` — **modifier** : `#connectedView` (1 bouton vidéo + section fichier-par-URL).
- `extension/popup.js` — **modifier** : handler unique `#downloadVideo` ; suppression de `#sendTab`, `#sendPage`, `refreshMedia`.

(Aucun autre fichier. `background.js` garde `sendMedia`/`sendPage`/`listMedia` inchangés.)

---

## Task 1: Simplifier le popup (un seul bouton vidéo)

**Files:**
- Modify: `extension/popup.html`
- Modify: `extension/popup.js`

> Pas de runner de test pour l'extension → vérification par revue + smoke manuel.

- [ ] **Step 1: Remplacer le contenu de `#connectedView` dans `popup.html`**

Remplacer le bloc actuel :
```html
    <div id="connectedView" class="hidden">
      <input id="url" type="text" placeholder="Coller une URL…" />
      <button id="sendUrl">Envoyer à FlashGet</button>
      <button id="sendTab">Envoyer l'onglet courant</button>
      <button id="sendPage">Télécharger la vidéo de cette page</button>
      <div id="toast"></div>
      <div id="mediaSection" class="hidden">
        <hr />
        <div class="muted">Vidéos sur cette page</div>
        <div id="mediaList"></div>
      </div>
    </div>
```
par :
```html
    <div id="connectedView" class="hidden">
      <button id="downloadVideo">⬇ Télécharger la vidéo de cette page</button>
      <div id="toast"></div>
      <hr />
      <div class="muted">Télécharger un fichier par URL</div>
      <input id="url" type="text" placeholder="Coller une URL…" />
      <button id="sendUrl">Envoyer</button>
    </div>
```
(Supprime `#sendTab`, `#sendPage`, `#mediaSection`/`#mediaList`. Conserve `#url`, `#sendUrl`, `#toast`.)

- [ ] **Step 2: Mettre à jour `popup.js` — supprimer les anciens handlers**

Dans `extension/popup.js`, **supprimer** :
- le handler `document.getElementById("sendTab").addEventListener(...)` (≈ lignes 33-39) ;
- le handler `document.getElementById("sendPage").addEventListener(...)` (≈ lignes 41-47) ;
- la fonction `async function refreshMedia() { ... }` (≈ lignes 59-79) **et** son appel `refreshMedia();` (dernière ligne).

- [ ] **Step 3: Ajouter le handler du bouton unique dans `popup.js`**

Ajouter (par ex. juste après le handler `#sendUrl`) :
```js
document.getElementById("downloadVideo").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.url) return;
  // Flux HLS/DASH détecté sur l'onglet → capture directe (gère aussi le live) ;
  // sinon → yt-dlp sur l'URL de la page (YouTube, Facebook, etc.).
  chrome.runtime.sendMessage({ cmd: "listMedia", tabId: tab.id }, (res) => {
    const streams = (res && res.streams) || [];
    const msg = streams.length > 0
      ? { cmd: "sendMedia", url: streams[0].url, tabId: tab.id }
      : { cmd: "sendPage", url: tab.url, tabId: tab.id };
    chrome.runtime.sendMessage(msg, (r) => {
      flash(r && r.ok ? "Envoyé ✓" : "Échec — non connecté");
    });
  });
});
```

- [ ] **Step 4: Vérifier le `#sendUrl` conservé**

Le handler `#sendUrl` existant doit rester (lecture de `#url`, `cmd: "send"`). Vérifier qu'il n'a pas été supprimé par erreur :
```js
document.getElementById("sendUrl").addEventListener("click", () => {
  const url = document.getElementById("url").value;
  if (!url.trim()) return;
  chrome.runtime.sendMessage({ cmd: "send", url }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});
```
(Inchangé. Les handlers `#openOptions`, `#retry`, la fonction `refresh()` + son appel, `render`, `flash` restent aussi.)

- [ ] **Step 5: Revue manuelle (cohérence statique)**

Vérifier qu'aucune référence aux ids supprimés ne subsiste dans `popup.js` (`sendTab`, `sendPage`, `mediaSection`, `mediaList`, `refreshMedia`). Vérifier que tout id référencé par `popup.js` (`downloadVideo`, `url`, `sendUrl`, `toast`, `dot`, `statusText`, `connectedView`, `disconnectedView`, `openOptions`, `retry`) existe bien dans `popup.html`.

- [ ] **Step 6: Commit**

```bash
git checkout phase-4c4-popup-simplify
git add extension/popup.html extension/popup.js
git commit -m "feat(extension): popup à un seul bouton vidéo intelligent (sniffé → yt-dlp)"
```

---

## Task 2: Non-régression + smoke

**Files:** aucun (vérification globale).

- [ ] **Step 1: Suites automatiques (doivent rester inchangées)**

Run: `cd src-tauri && cargo test` ; `cd c:/Users/steph/Documents/H/idm-clone && npm test && npm run build`
Expected: tout vert (aucun code Rust/front testé modifié) — confirme 0 régression.

- [ ] **Step 2: Smoke manuel (note pour le réviseur)**

Recharger l'extension. Vérifier dans le popup connecté : un seul bouton vidéo + une section « Télécharger un fichier par URL » ; plus de « Envoyer l'onglet courant » ni de liste « Vidéos sur cette page ». Tester : (a) page YouTube → bouton → boîte de variantes (yt-dlp) ; (b) page avec flux HLS/DASH sniffé → bouton → variantes (capture directe), live → enregistrement ; (c) coller une URL de fichier → téléchargement.

- [ ] **Step 3: Commit éventuel** (si ajustements)

```bash
git checkout phase-4c4-popup-simplify
git add -A
git commit -m "chore(phase-4c4): vérification finale popup simplifié"
```

---

## Notes d'implémentation
- **DRY/YAGNI :** réutilise `sendMedia`/`sendPage`/`listMedia` ; aucune logique backend ajoutée.
- **Accessibilité :** un seul libellé d'action vidéo explicite ; la section fichier est libellée séparément.
- **Garde git :** `git checkout phase-4c4-popup-simplify` avant chaque commit de subagent.
