# Bouton vidéo : yt-dlp d'abord + repli sniffé — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **GARDE-FOU GIT (obligatoire) :** chaque subagent DOIT exécuter `git checkout phase-4c5-smart-routing` avant tout commit.

**Goal:** Le bouton vidéo tente yt-dlp d'abord (URL fraîches → corrige les sites à URL HLS signée comme Pornhub) puis replie sur le flux HLS/DASH sniffé.

**Architecture:** Nouveau `kind:"smart"` portant l'URL de page + un flux sniffé de repli (`fallback_url`/`fallback_kind`). Côté app, `probe_smart` essaie `probe_ytdlp_inner(page)` puis, en cas d'échec, `probe_hls_inner`/`probe_dash_inner(fallback)`, et émet `media://error` une seule fois si tout échoue.

**Tech Stack:** Rust (Tauri bridge + commands), extension Chromium MV3 (JS).

**Spec :** `docs/superpowers/specs/2026-06-05-phase-4c5-smart-routing-design.md`

---

## File Structure
- `src-tauri/src/bridge/protocol.rs` — **modifier** : champs `fallback_url`/`fallback_kind` (+ test).
- `src-tauri/src/commands.rs` — **modifier** : nouvelle `probe_smart`.
- `src-tauri/src/bridge/server.rs` — **modifier** : destructure + bras `kind=="smart"`.
- `extension/popup.js` — **modifier** : `#downloadVideo` → `cmd:"smartDownload"`.
- `extension/background.js` — **modifier** : `sendSmart` + handler + `flushPending`.
- `extension/buildCapture.js` — **modifier** : whitelist `fallback_url`/`fallback_kind`.

---

## Task 1: Rust — protocole, `probe_smart`, route bridge

**Files:**
- Modify: `src-tauri/src/bridge/protocol.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/bridge/server.rs`

- [ ] **Step 1: Ajouter les champs au protocole**

Dans `src-tauri/src/bridge/protocol.rs` :

`ClientMessage::Capture` (enum) — ajouter après `kind`:
```rust
        kind: Option<String>,
        fallback_url: Option<String>,
        fallback_kind: Option<String>,
```
`ServerReply::CaptureRequested` — ajouter après `kind` :
```rust
        kind: Option<String>,
        fallback_url: Option<String>,
        fallback_kind: Option<String>,
```
`RawMessage::Capture` — ajouter après le champ `kind` :
```rust
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        fallback_url: Option<String>,
        #[serde(default)]
        fallback_kind: Option<String>,
```
Dans `parse_client_message`, l'arm `Ok(RawMessage::Capture { … })` :
```rust
        Ok(RawMessage::Capture { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind }) => {
            ClientMessage::Capture { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind }
        }
```
Dans `handle_message`, l'arm `ClientMessage::Capture { … }` :
```rust
        ClientMessage::Capture { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind } => {
            if authed {
                (authed, ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind })
            } else {
                (authed, ServerReply::NeedAuth)
            }
        }
```

- [ ] **Step 2: Test de contrat**

Dans `#[cfg(test)] mod tests` de `protocol.rs`, ajouter :
```rust
    #[test]
    fn capture_carries_smart_fallback() {
        let msg = parse_client_message(
            r#"{"type":"capture","url":"https://site/watch","kind":"smart","fallback_url":"https://cdn/x.m3u8","fallback_kind":"hls"}"#,
        );
        let (_authed, reply) = handle_message(true, "secret", msg);
        match reply {
            ServerReply::CaptureRequested { kind, fallback_url, fallback_kind, .. } => {
                assert_eq!(kind.as_deref(), Some("smart"));
                assert_eq!(fallback_url.as_deref(), Some("https://cdn/x.m3u8"));
                assert_eq!(fallback_kind.as_deref(), Some("hls"));
            }
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }
```

- [ ] **Step 3: Lancer le test (échoue à la compilation tant que les champs manquent)**

Run: `cd src-tauri && cargo test capture_carries_smart_fallback`
Expected: après Step 1, PASS.

- [ ] **Step 4: Implémenter `probe_smart` dans `commands.rs`**

Ajouter près de `probe_ytdlp` (qui contient déjà `probe_ytdlp_inner`) :
```rust
/// Bouton vidéo intelligent : yt-dlp d'abord (ré-extrait des URL fraîches, gère les sites
/// à URL signée), puis repli sur le flux HLS/DASH sniffé si yt-dlp ne trouve rien.
/// Émet `media://error` une seule fois si tout échoue.
pub(crate) async fn probe_smart(
    app: &AppHandle,
    page_url: &str,
    fallback_url: Option<String>,
    fallback_kind: Option<String>,
    headers: RequestHeaders,
) -> Result<(), String> {
    let yt = probe_ytdlp_inner(app, page_url, headers.clone()).await;
    if yt.is_ok() {
        return Ok(());
    }
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
    let e = yt.err().unwrap_or_else(|| "aucune vidéo trouvée sur cette page".to_string());
    let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    Err(e)
}
```
(Vérifier les signatures réelles avant de t'appuyer dessus : `probe_ytdlp_inner(app, url, headers)`, `probe_hls_inner(app, url, headers, Option<String>)`, `probe_dash_inner(app, url, headers, Option<String>)` — toutes `pub(crate)`/privées dans `commands.rs`, renvoyant `Result<(), String>`, émettant `media://variants` + surgissant la fenêtre en cas de succès et N'émettant PAS `media://error`. `MediaErrorEvent` et `RequestHeaders` (Clone) sont déjà importés/définis dans `commands.rs`.)

- [ ] **Step 5: Bras `kind=="smart"` dans `server.rs`**

Dans `src-tauri/src/bridge/server.rs`, étendre la destructuration et ajouter le bras. Remplacer :
```rust
            ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind } => {
                let headers = RequestHeaders { referrer, cookie, user_agent };
                if kind.as_deref() == Some("hls") {
```
par :
```rust
            ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind } => {
                let headers = RequestHeaders { referrer, cookie, user_agent };
                if kind.as_deref() == Some("smart") {
                    match commands::probe_smart(&app, &url, fallback_url, fallback_kind, headers).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "smart" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else if kind.as_deref() == Some("hls") {
```
(Le reste de la chaîne `hls`/`dash`/`page`/`else` est inchangé. `filename` reste utilisé par les bras existants → pas de warning « unused ».)

- [ ] **Step 6: Vérifier compilation + tests**

Run: `cd src-tauri && cargo test`
Expected: tauri lib 12 (11 + capture_carries_smart_fallback) + download-core verts, **0 warning**.

- [ ] **Step 7: Commit**

```bash
git checkout phase-4c5-smart-routing
git add src-tauri/src/bridge/protocol.rs src-tauri/src/commands.rs src-tauri/src/bridge/server.rs
git commit -m "feat(bridge): kind=smart (yt-dlp d'abord + repli sniffé) via probe_smart"
```

---

## Task 2: Extension — bouton smart + capture de repli

**Files:**
- Modify: `extension/popup.js`
- Modify: `extension/background.js`
- Modify: `extension/buildCapture.js`

> Pas de runner de test pour l'extension → revue + smoke manuel.

- [ ] **Step 1: `buildCapture.js` — whitelist des 2 champs**

Dans `extension/buildCapture.js`, la boucle des champs :
```js
  for (const key of ["referrer", "cookie", "user_agent", "filename", "kind"]) {
```
devient :
```js
  for (const key of ["referrer", "cookie", "user_agent", "filename", "kind", "fallback_url", "fallback_kind"]) {
```

- [ ] **Step 2: `background.js` — `sendSmart`**

Ajouter, à côté de `sendPage` :
```js
// Bouton vidéo intelligent : envoie l'URL de page (yt-dlp d'abord côté app) + un flux
// HLS/DASH sniffé en repli (kind "smart").
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
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, ...extras, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```
(`isDashUrl` est déjà importé en haut de `background.js`.)

- [ ] **Step 3: `background.js` — handler `smartDownload`**

Dans `chrome.runtime.onMessage.addListener`, ajouter (à côté du handler `sendPage`) :
```js
  if (req.cmd === "smartDownload" && req.tabId != null) {
    (async () => {
      let url, referrer;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        url = tab && tab.url;
        referrer = url;
      } catch (_e) {
        // pas d'onglet
      }
      if (!url) { sendResponse({ ok: false }); return; }
      const streams = detectedByTab.get(req.tabId) || [];
      const fallbackUrl = streams.length > 0 ? streams[0].url : undefined;
      const ok = await sendSmart(url, referrer, fallbackUrl);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
```

- [ ] **Step 4: `background.js` — `flushPending` transmet le repli**

Dans `flushPending`, l'envoi :
```js
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename, kind: it.kind })));
```
devient :
```js
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename, kind: it.kind, fallback_url: it.fallback_url, fallback_kind: it.fallback_kind })));
```
(buildCapture omet les champs falsy → les items non-smart ne sont pas affectés.)

- [ ] **Step 5: `popup.js` — le bouton appelle `smartDownload`**

Remplacer le handler `#downloadVideo` actuel (celui qui interroge `listMedia` et route vers `sendMedia`/`sendPage`) par :
```js
document.getElementById("downloadVideo").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.id) return;
  chrome.runtime.sendMessage({ cmd: "smartDownload", tabId: tab.id }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});
```

- [ ] **Step 6: Revue manuelle (cohérence)**

Vérifier : `detectedByTab` est bien le Map des flux sniffés (présent dans background.js) ; `isDashUrl` importé ; les handlers `sendMedia`/`sendPage`/`listMedia` existants restent (peuvent rester inutilisés depuis le popup, OK) ; `#sendUrl`/`#openOptions`/`#retry` inchangés.

- [ ] **Step 7: Commit**

```bash
git checkout phase-4c5-smart-routing
git add extension/popup.js extension/background.js extension/buildCapture.js
git commit -m "feat(extension): bouton smartDownload (page + flux sniffé en repli, kind smart)"
```

---

## Task 3: Vérification finale

**Files:** aucun.

- [ ] **Step 1: Suites automatiques**

Run: `cd src-tauri && cargo test` ; `cd src-tauri/crates/download-core && cargo test` ; `cd c:/Users/steph/Documents/H/idm-clone && npm test && npm run build`
Expected: tout vert (tauri lib 12), **0 warning rustc**, build OK, Vitest inchangé.

- [ ] **Step 2: Smoke manuel (note réviseur)**

Avec yt-dlp en place : (a) page Pornhub / site à URL HLS signée → bouton → yt-dlp ré-extrait → ✅ (plus de 410) ; (b) YouTube/Facebook → ✅ ; (c) site de niche avec flux sniffé non géré par yt-dlp → repli → ✅ ; (d) page sans vidéo → « aucune vidéo trouvée sur cette page » (une seule bannière).

- [ ] **Step 3: Commit éventuel** (ajustements)

```bash
git checkout phase-4c5-smart-routing
git add -A
git commit -m "chore(phase-4c5): vérification finale routage smart"
```

---

## Notes d'implémentation
- **Ordre étape app/route :** `probe_smart` (Task 1 Step 4) doit exister avant le bras `server.rs` (Step 5) qui l'appelle — même commit, OK.
- **Émission d'erreur unique :** seules les variantes `_inner` sont appelées (elles n'émettent pas d'erreur) ; `probe_smart` émet `media://error` une fois à la fin.
- **DRY/YAGNI :** réutilise les `_inner` existants ; pas de course parallèle.
- **Garde git :** `git checkout phase-4c5-smart-routing` avant chaque commit de subagent.
