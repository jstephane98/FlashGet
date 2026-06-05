# Spec — Bouton vidéo : routage yt-dlp d'abord + repli sniffé

**Date :** 2026-06-05
**Branche :** `phase-4c5-smart-routing` (depuis `main`, après merge de toute la pile 4b-3→4c4)
**Statut :** design validé par l'utilisateur, prêt pour le plan.

## 1. Problème
Le bouton vidéo du popup (4c4) route **« flux HLS/DASH sniffé d'abord, sinon yt-dlp »**.
Sur un site à URL HLS **signée/expirable** (ex. phncdn/Pornhub), le `.m3u8` sniffé est
récupéré par notre moteur et renvoie **HTTP 410** (URL expirée/à usage unique). yt-dlp,
lui, a un extracteur dédié qui **ré-extrait une URL fraîche** → YouTube/Facebook (yt-dlp)
marchent. Il faut **inverser la priorité** : yt-dlp d'abord, repli sur le flux sniffé.

## 2. Décision (brainstorm 2026-06-05)
**yt-dlp d'abord + repli sniffé.** Le bouton tente yt-dlp sur l'URL de la page ; si yt-dlp
ne trouve rien, repli sur le flux HLS/DASH sniffé. Le repli vit **côté app** (là où le
résultat de la sonde est connu) : l'extension transmet l'URL de page **et** le flux sniffé
de repli dans une seule capture ; l'app orchestre yt-dlp→sniffé.

## 3. Périmètre
### Inclus
- Protocole capture : 2 nouveaux champs `fallback_url`/`fallback_kind` (Option).
- `kind:"smart"` routé vers une nouvelle `commands::probe_smart` (yt-dlp inner → repli).
- Extension : popup `smartDownload` ; background `sendSmart` (page + repli depuis
  `detectedByTab`) ; `buildCapture`/`flushPending` apprennent les 2 champs.

### Exclus (assumé)
- Plusieurs flux sniffés → on garde le **premier** comme repli.
- Pas de course parallèle (séquentiel yt-dlp puis sniffé).
- Anciens bras bridge `hls`/`dash`/`page` conservés (inutilisés depuis l'extension ;
  `page` sert encore au flux interne ; `add_download` paste inchangé).
- Cas de niche : si yt-dlp gère lui-même un live, il le prend en charge à sa façon (pas
  l'UI Recording dédiée) ; le repli sniffé ne préserve le live que pour les sites yt-dlp
  ne gère pas.

## 4. Architecture livrée

### 4.1 Bridge — `protocol.rs`
`RawMessage::Capture`, `ClientMessage::Capture`, `ServerReply::CaptureRequested` gagnent
`#[serde(default)] fallback_url: Option<String>` et `fallback_kind: Option<String>`.
`parse_client_message` et `handle_message` les transmettent (pass-through, comme `kind`).
Les tests existants utilisent `{ .. }` donc restent verts. Nouveau test
`capture_carries_smart_fallback` (une capture `kind:"smart"` + `fallback_url`/`fallback_kind`
les porte jusqu'à `CaptureRequested`).

### 4.2 Bridge — `server.rs`
La destructuration `CaptureRequested { … }` inclut `fallback_url`, `fallback_kind`.
Nouveau bras **avant** le `else` final :
```rust
} else if kind.as_deref() == Some("smart") {
    match commands::probe_smart(&app, &url, fallback_url, fallback_kind, headers).await {
        Ok(()) => serde_json::json!({ "type": "ack", "kind": "smart" }).to_string(),
        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
    }
}
```

### 4.3 App — `commands.rs::probe_smart`
```rust
/// Bouton vidéo intelligent : yt-dlp d'abord (ré-extrait des URL fraîches, gère les sites
/// à URL signée), puis repli sur le flux HLS/DASH sniffé si yt-dlp ne trouve rien.
pub(crate) async fn probe_smart(
    app: &AppHandle,
    page_url: &str,
    fallback_url: Option<String>,
    fallback_kind: Option<String>,
    headers: RequestHeaders,
) -> Result<(), String> {
    // 1) yt-dlp (variante interne : n'émet PAS media://error en cas d'échec).
    let yt = probe_ytdlp_inner(app, page_url, headers.clone()).await;
    if yt.is_ok() {
        return Ok(()); // probe_ytdlp_inner a déjà émis media://variants + surgi la fenêtre
    }
    // 2) repli sur le flux sniffé (variantes internes, sans émission d'erreur).
    if let Some(fb) = fallback_url {
        let fr = if fallback_kind.as_deref() == Some("dash") {
            probe_dash_inner(app, &fb, headers, None).await
        } else {
            probe_hls_inner(app, &fb, headers, None).await
        };
        if fr.is_ok() {
            return Ok(());
        }
    }
    // 3) tout a échoué → une seule bannière d'erreur (message yt-dlp) + surgir la fenêtre.
    let e = yt.err().unwrap_or_else(|| "aucune vidéo trouvée sur cette page".to_string());
    let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
    if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    Err(e)
}
```
Notes : `probe_ytdlp_inner(app, url, headers)`, `probe_hls_inner(app, url, headers, Option<String>)`,
`probe_dash_inner(app, url, headers, Option<String>)` existent déjà (privés) et émettent
`media://variants` + surgissent la fenêtre en cas de succès, **sans** émettre d'erreur en cas
d'échec — d'où l'émission unique de `media://error` à l'étape 3. `RequestHeaders` est `Clone`.

### 4.4 Extension — `popup.js`
Le handler du bouton `#downloadVideo` devient :
```js
document.getElementById("downloadVideo").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.id) return;
  chrome.runtime.sendMessage({ cmd: "smartDownload", tabId: tab.id }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});
```
(Plus de `listMedia` ni de routage côté popup : c'est le background qui décide.)

### 4.5 Extension — `background.js`
- Nouvelle fonction `sendSmart(url, referrer, fallbackUrl)` (calquée sur `sendPage`) :
  ```js
  async function sendSmart(url, referrer, fallbackUrl) {
    const extras = { kind: "smart" };
    if (fallbackUrl) {
      extras.fallback_url = fallbackUrl;
      extras.fallback_kind = isDashUrl(fallbackUrl) ? "dash" : "hls";
    }
    if (socket && socket.readyState === WebSocket.OPEN && connected) {
      try {
        const headers = await gatherHeaders(url, referrer);
        socket.send(JSON.stringify(buildCapture(url, { ...headers, ...extras })));
        return true;
      } catch (_e) { return false; }
    }
    pending.push({ url, referrer, ...extras, ts: Date.now() });
    if (pending.length > 20) pending.shift();
    connect();
    return true;
  }
  ```
- Handler `cmd:"smartDownload"` : récupère l'onglet (`chrome.tabs.get(req.tabId)`) → `url=tab.url`,
  `referrer=tab.url` ; prend le 1ᵉʳ flux sniffé `detectedByTab.get(req.tabId)?.[0]?.url` comme
  `fallbackUrl` ; appelle `sendSmart` ; `sendResponse({ok})`.
- `buildCapture` (`buildCapture.js`) : ajouter `fallback_url` et `fallback_kind` à la liste
  blanche des champs.
- `flushPending` : transmettre aussi `fallback_url`/`fallback_kind` des items en file.

## 5. Flux
```
[clic bouton] → smartDownload(tabId)
[background] url=tab.url ; fallbackUrl = 1er flux sniffé (detectedByTab) ; sendSmart
   → buildCapture(url, { cookie, referer, ua, kind:"smart", fallback_url, fallback_kind })
[bridge] kind=="smart" → probe_smart(page_url, fallback_url, fallback_kind, headers)
   1) probe_ytdlp_inner(page) — OK → media://variants
   2) sinon repli probe_hls/dash_inner(fallback) — OK → media://variants
   3) sinon media://error (une fois)
```

## 6. Tests
- **Tauri lib** : `protocol.rs` `capture_carries_smart_fallback` (porte `kind:"smart"` +
  `fallback_url`/`fallback_kind`). Suites existantes inchangées (destructurations en `..`).
- `probe_smart`, extension : pas de runner → **smoke manuel** : (a) Pornhub/site à URL
  signée → bouton → yt-dlp → ✅ ; (b) YouTube/FB → ✅ ; (c) site de niche sniffé non géré par
  yt-dlp → repli → ✅ ; (d) page sans vidéo → « aucune vidéo trouvée ».
- `cargo test` (tauri lib +1), `cargo test` download-core inchangé, `npm test`/`build`
  inchangés ; **0 warning**.

## 7. Risques / limites (non bloquants)
- yt-dlp tourne d'abord à chaque clic (≈ latence `-J`) même si un flux sniffé direct
  existait — acceptable pour la fiabilité.
- Repli = premier flux sniffé seulement.
- Live : préservé uniquement quand yt-dlp ne gère pas le site (sinon yt-dlp prend la main).

## 8. Garde-fou
Implémentation subagent-driven : `git checkout phase-4c5-smart-routing` avant tout commit.
