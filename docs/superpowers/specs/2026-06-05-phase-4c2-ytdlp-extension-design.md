# Spec — Bouton extension « Télécharger la vidéo de cette page » (yt-dlp)

**Date :** 2026-06-05
**Branche :** `phase-4c2-ytdlp-extension` (empilée sur `phase-4c-ytdlp`)
**Statut :** design validé par l'utilisateur, prêt pour le plan d'implémentation.

## 1. Problème / objectif

L'extracteur yt-dlp (phase 4c) ne se déclenche que par le champ « Ajouter une URL » (coller).
Depuis l'extension navigateur, il n'existe aucun moyen propre d'envoyer une **page vidéo**
vers yt-dlp : « Envoyer l'onglet courant » passe par `add_url` (téléchargement régulier),
qui essaie de télécharger la page HTML comme un fichier → cassé ; et la section « Vidéos sur
cette page » ne liste que les flux HLS/DASH sniffés (rien pour YouTube). On ajoute un bouton
dédié dans le popup pour router la page courante vers yt-dlp.

## 2. Décisions (brainstorm 2026-06-05)

- **Déclencheur :** un **bouton dédié** dans le popup (« Télécharger la vidéo de cette page »),
  toujours visible quand l'app est connectée. (Pas de menu contextuel, pas de détection auto.)
- **Routage (Approche A) :** le bouton envoie la capture avec un **`kind: "page"` explicite** ;
  le bridge route `kind == "page"` vers `commands::probe_ytdlp` (déjà existant). Pas de sonde
  de content-type (l'intention est explicite), aucun changement de comportement des autres
  captures.
- **Cookies/session :** la capture transporte déjà cookies/referer/user-agent du navigateur →
  yt-dlp les reçoit → permet de télécharger des vidéos nécessitant une **connexion** (avantage
  vs « coller URL »).

## 3. Périmètre

### Inclus
- Bouton popup + handler.
- `background.js` : fonction `sendPage` + handler `cmd: "sendPage"` (kind `"page"`,
  chemin immédiat + file d'attente).
- Bridge `server.rs` : bras de routage `kind == "page"` → `probe_ytdlp`.
- Test `protocol.rs` : la capture porte bien `kind: "page"`.

### Exclus (assumé, backlog)
- Détection automatique « page vidéo » / badge sur l'icône.
- Entrée de menu contextuel (clic droit).
- Playlists, auto-update yt-dlp, sous-titres (déjà hors périmètre 4c).

## 4. Architecture livrée

### 4.1 Extension — `popup.html` + `popup.js`
- `popup.html` : nouveau `<button id="sendPage">Télécharger la vidéo de cette page</button>`
  dans `#connectedView`, sous `#sendTab`.
- `popup.js` : handler du clic → récupère l'onglet actif
  (`chrome.tabs.query({active:true,currentWindow:true})`) → si `tab.url`,
  `chrome.runtime.sendMessage({ cmd: "sendPage", url: tab.url, tabId: tab.id })` →
  `flash("Envoyé ✓" | "Échec — non connecté")`.

### 4.2 Extension — `background.js`
- Nouvelle fonction `sendPage(url, referrer)` (calquée sur `sendMedia`) :
  - connecté → `socket.send(buildCapture(url, { ...headers, kind: "page" }))` ;
  - sinon → `pending.push({ url, referrer, kind: "page", ts: Date.now() })` + `connect()`.
- Handler `req.cmd === "sendPage"` : `chrome.tabs.get(req.tabId)` → `referrer = tab.url`
  → `sendPage(req.url, referrer)` → `sendResponse({ ok })`.
- `buildCapture` (whitelist contient déjà `kind`) et `flushPending` (transmet déjà
  `filename`+`kind`) : **inchangés**.

### 4.3 Bridge — `server.rs`
Dans le handler `ServerReply::CaptureRequested`, ajouter un bras après dash :
```rust
} else if kind.as_deref() == Some("page") {
    match commands::probe_ytdlp(&app, &url, headers).await {
        Ok(()) => serde_json::json!({ "type": "ack", "kind": "page" }).to_string(),
        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
    }
}
```
`protocol.rs` : **inchangé** (`kind` est déjà `Option<String>`, « page » transite).

### 4.4 Réutilisation (aucun changement app/cœur)
`probe_ytdlp` (déjà `pub(crate)`) émet `media://variants` → la boîte `HlsVariantDialog`
existante s'ouvre (titre pré-rempli via `yt-dlp -J`) → `add_media_download` →
`DownloadKind::Ytdlp` → `run_download`. Les en-têtes (cookies) passent jusqu'à yt-dlp.

## 5. Flux de données

```
[Popup] clic "Télécharger la vidéo de cette page"
   → cmd sendPage {url=tab.url, tabId}
[background] sendPage → gatherHeaders(url, referrer=tab.url)
   → buildCapture(url, { cookie, referer, user_agent, kind:"page" })  ──WS──►
[bridge server.rs] kind=="page" → commands::probe_ytdlp(url, headers)
   → yt-dlp -J → media://variants → HlsVariantDialog (titre pré-rempli)
   → add_media_download → Download{kind:Ytdlp} → run_download (cookies transmis)
```

## 6. Gestion d'erreurs
- App non connectée → mise en file (`pending`) + flash ; départ à la reconnexion (mécanisme
  existant).
- Page sans vidéo → `probe_ytdlp` émet `media://error` « aucune vidéo trouvée sur cette page »
  (déjà en place) ; l'app affiche la bannière et surgit.

## 7. Tests
- **Tauri lib** (`bridge/protocol.rs`) : `capture_carries_kind_page` — un message
  `{"type":"capture","url":"…","kind":"page"}` produit `CaptureRequested { kind: Some("page") }`
  (calqué sur `capture_carries_kind_hls`).
- Le routage `server.rs` (page→probe_ytdlp) et l'extension n'ont pas de runner unitaire →
  **vérif build + smoke manuel** (popup → bouton → boîte de variantes → téléchargement, y
  compris une vidéo nécessitant une session pour valider le transfert des cookies).
- Critères verts : `cargo test` (tauri lib 9→10 + download-core), `npm test` (Vitest, inchangé),
  build, **0 warning**.

## 8. Risques / limites (non bloquants)
- Le bouton est toujours visible : sur une page sans vidéo, l'utilisateur obtient « aucune
  vidéo trouvée » (acceptable ; pas de pré-détection fiable côté navigateur).
- Pas de badge/indicateur « cette page a une vidéo » (backlog).

## 9. Garde-fou processus
Implémentation **subagent-driven** : chaque prompt de subagent DOIT commencer par
`git checkout phase-4c2-ytdlp-extension` avant tout commit.
