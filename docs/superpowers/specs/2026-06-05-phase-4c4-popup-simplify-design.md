# Spec — Popup : un seul bouton vidéo intelligent

**Date :** 2026-06-05
**Branche :** `phase-4c4-popup-simplify` (empilée sur `phase-4c3-ytdlp-name-fallback`)
**Statut :** design validé par l'utilisateur, prêt pour le plan.

## 1. Problème
Le popup a accumulé plusieurs entrées qui ressemblent toutes à « récupérer la vidéo » :
« Envoyer l'onglet courant », « Télécharger la vidéo de cette page » (yt-dlp), et la liste
dynamique « Vidéos sur cette page » (flux HLS/DASH sniffés). Un nouvel utilisateur ne sait
pas lequel cliquer. On consolide en **un seul bouton vidéo intelligent**.

## 2. Décisions (brainstorm 2026-06-05)
- **Un seul bouton** « Télécharger la vidéo de cette page », qui **route tout seul** :
  flux HLS/DASH sniffé présent → le capturer (`sendMedia`) ; sinon → yt-dlp sur l'URL de
  la page (`sendPage`).
- **Supprimer** « Envoyer l'onglet courant » et la liste « Vidéos sur cette page ».
- **Garder** le champ « coller une URL » (télécharger un fichier par URL), **clairement
  séparé** sous un libellé dédié.

## 3. Périmètre
### Inclus
- `extension/popup.html` : nouvelle structure (1 bouton vidéo + section « fichier par URL »).
- `extension/popup.js` : handler unique `#downloadVideo` (routage sniffé→yt-dlp) ;
  suppression des handlers `#sendTab`, `#sendPage`, et de `refreshMedia`.

### Exclus (assumé)
- Aucun changement `background.js` / bridge / app / cœur : `sendMedia`, `sendPage`,
  `listMedia` existent déjà et sont réutilisés.
- Choix entre plusieurs flux sniffés distincts (rare) : on prend le **premier**.
- Détection auto « page vidéo » / badge sur l'icône (backlog).

## 4. Architecture livrée

### 4.1 `popup.html` — `#connectedView`
Remplacer le contenu de `#connectedView` par :
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
Supprimés : `#sendTab`, `#sendPage`, `#mediaSection`/`#mediaList`. Conservés : `#url`,
`#sendUrl` (relibellé « Envoyer »), `#toast`.

### 4.2 `popup.js`
- **Supprimer** : le handler `#sendTab`, le handler `#sendPage`, la fonction `refreshMedia`
  et son appel `refreshMedia();`.
- **Ajouter** le handler du bouton unique (routage au clic) :
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
- **Conserver** : `#sendUrl` (cmd `send`), `#openOptions`, `#retry`, `refresh()` + son appel,
  `flash`, `render`.

## 5. Flux
```
[clic "Télécharger la vidéo de cette page"]
  → listMedia(tabId)
     ├─ streams.length > 0 → sendMedia(streams[0].url, tabId)  → bridge probe_hls/dash
     └─ sinon              → sendPage(tab.url, tabId)          → bridge probe_ytdlp
  → app : boîte de variantes (qualité) → download
  (page sans vidéo → "aucune vidéo trouvée sur cette page")
```

## 6. Tests
- Pas de runner pour l'extension → **vérif par smoke manuel** : (a) page YouTube → bouton →
  variantes (yt-dlp) ; (b) page avec flux HLS/DASH sniffé → bouton → variantes (capture
  directe), live → enregistrement ; (c) champ « coller une URL » → fichier téléchargé ;
  (d) plus aucun bouton « Envoyer l'onglet courant » ni liste « Vidéos sur cette page ».
- Suites automatiques (Rust/Vitest) **inchangées** (aucun code testé modifié) — à relancer
  pour confirmer 0 régression : `cargo test`, `npm test`, `npm run build`.

## 7. Risques / limites (non bloquants)
- Plusieurs flux sniffés distincts → on prend le premier (peut être une pub sur de rares
  pages). Avant : liste de choix. Compromis simplicité.
- Si un site (ex. Facebook) est sniffé comme DASH, il passe par le moteur DASH plutôt que
  yt-dlp ; sinon yt-dlp. Inversable plus tard si régression constatée.
- Le live HLS/DASH reste déclenchable (chemin sniffé conservé via `sendMedia`).

## 8. Garde-fou
Implémentation subagent-driven : `git checkout phase-4c4-popup-simplify` avant tout commit.
