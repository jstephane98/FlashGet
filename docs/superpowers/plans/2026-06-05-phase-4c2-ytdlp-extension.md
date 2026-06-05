# Bouton extension yt-dlp (page → yt-dlp) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **GARDE-FOU GIT (obligatoire) :** chaque subagent DOIT exécuter `git checkout phase-4c2-ytdlp-extension` avant tout commit.

**Goal:** Ajouter un bouton « Télécharger la vidéo de cette page » dans le popup de l'extension qui envoie la page courante vers le flux yt-dlp (avec cookies du navigateur).

**Architecture:** Le bouton envoie une capture avec `kind: "page"` (Approche A) ; le bridge route `kind == "page"` vers `commands::probe_ytdlp` (déjà existant), qui réutilise la boîte de variantes et `DownloadKind::Ytdlp`. Aucun changement côté app/cœur. Les en-têtes (cookies/referer/UA) transitent jusqu'à yt-dlp → vidéos authentifiées.

**Tech Stack:** Extension Chromium MV3 (JS), bridge WebSocket (Rust/Tauri), `probe_ytdlp` (déjà en place).

**Spec :** `docs/superpowers/specs/2026-06-05-phase-4c2-ytdlp-extension-design.md`

---

## File Structure

- `src-tauri/src/bridge/protocol.rs` — **modifier** : test de contrat `capture_carries_kind_page`.
- `src-tauri/src/bridge/server.rs` — **modifier** : bras de routage `kind == "page"` → `probe_ytdlp`.
- `extension/popup.html` — **modifier** : bouton `#sendPage`.
- `extension/popup.js` — **modifier** : handler du bouton.
- `extension/background.js` — **modifier** : `sendPage` + handler `cmd: "sendPage"`.

---

## Task 1: Bridge — routage `kind == "page"` → yt-dlp

**Files:**
- Modify: `src-tauri/src/bridge/protocol.rs` (test)
- Modify: `src-tauri/src/bridge/server.rs` (routage)

- [ ] **Step 1: Test de contrat (le protocole porte `kind: "page"`)**

Dans `src-tauri/src/bridge/protocol.rs`, dans `#[cfg(test)] mod tests` (à côté de `capture_carries_kind_hls`), ajouter :

```rust
    #[test]
    fn capture_carries_kind_page() {
        let msg = parse_client_message(
            r#"{"type":"capture","url":"https://site/watch?v=1","kind":"page"}"#,
        );
        let (_authed, reply) = handle_message(true, "secret", msg);
        match reply {
            ServerReply::CaptureRequested { kind, .. } => assert_eq!(kind.as_deref(), Some("page")),
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }
```

- [ ] **Step 2: Lancer le test**

Run: `cd src-tauri && cargo test capture_carries_kind_page`
Expected: **PASS immédiatement** — le protocole transporte déjà `kind` de façon générique (`Option<String>`), donc aucun code de protocole n'est à modifier. Ce test verrouille le contrat pour « page ».

- [ ] **Step 3: Ajouter le bras de routage dans `server.rs`**

Dans `src-tauri/src/bridge/server.rs`, le handler `ServerReply::CaptureRequested` a une chaîne `if kind=="hls" {…} else if kind=="dash" {…} else {add_url}`. Insérer un bras **entre** le bras `dash` et le `else` final. Remplacer :

```rust
                } else if kind.as_deref() == Some("dash") {
                    match commands::probe_dash(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "dash" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else {
```

par :

```rust
                } else if kind.as_deref() == Some("dash") {
                    match commands::probe_dash(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "dash" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else if kind.as_deref() == Some("page") {
                    // Page vidéo générique → extraction yt-dlp (cookies/UA transmis).
                    match commands::probe_ytdlp(&app, &url, headers).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "page" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else {
```

(Note : `probe_ytdlp` ne prend pas `filename` — le titre vient de `yt-dlp -J`. `filename` reste utilisé par le bras `else` (add_url), donc pas de warning « unused ».)

- [ ] **Step 4: Vérifier compilation + tests**

Run: `cd src-tauri && cargo test`
Expected: tauri lib **10** (9 + `capture_carries_kind_page`) + download-core verts, **0 warning**.

- [ ] **Step 5: Commit**

```bash
git checkout phase-4c2-ytdlp-extension
git add src-tauri/src/bridge/protocol.rs src-tauri/src/bridge/server.rs
git commit -m "feat(bridge): route les captures kind=\"page\" vers probe_ytdlp"
```

---

## Task 2: Extension — fonction `sendPage` + handler

**Files:**
- Modify: `extension/background.js`

> Pas de runner de test pour l'extension : vérification par revue + smoke manuel.

- [ ] **Step 1: Ajouter `sendPage`**

Dans `extension/background.js`, à côté de `sendMedia`, ajouter :

```js
// Envoie la page courante vers l'extracteur yt-dlp (kind "page").
async function sendPage(url, referrer) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind: "page" })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, kind: "page", ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```

- [ ] **Step 2: Ajouter le handler `cmd: "sendPage"`**

Dans le `chrome.runtime.onMessage.addListener`, à côté du handler `sendMedia`, ajouter :

```js
  if (req.cmd === "sendPage" && req.url) {
    (async () => {
      let referrer;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        referrer = tab && tab.url;
      } catch (_e) {
        // pas de referer
      }
      const ok = await sendPage(req.url, referrer);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
```

- [ ] **Step 3: Revue manuelle**

Vérifier : `buildCapture` (`extension/buildCapture.js`) inclut `kind` dans sa whitelist (déjà le cas) ; `flushPending` transmet déjà `kind` (déjà le cas) → un item `pending` avec `kind:"page"` partira correctement à la reconnexion. Aucune modification de `buildCapture.js`/`flushPending` requise.

- [ ] **Step 4: Commit**

```bash
git checkout phase-4c2-ytdlp-extension
git add extension/background.js
git commit -m "feat(extension): sendPage (capture kind=page vers yt-dlp)"
```

---

## Task 3: Extension — bouton dans le popup

**Files:**
- Modify: `extension/popup.html`
- Modify: `extension/popup.js`

- [ ] **Step 1: Ajouter le bouton**

Dans `extension/popup.html`, dans `#connectedView`, juste après le bouton `#sendTab` :

```html
      <button id="sendTab">Envoyer l'onglet courant</button>
      <button id="sendPage">Télécharger la vidéo de cette page</button>
```

(Insérer la ligne `#sendPage` ; garder `#sendTab` tel quel.)

- [ ] **Step 2: Ajouter le handler du clic**

Dans `extension/popup.js`, après le handler de `#sendTab` (le `document.getElementById("sendTab").addEventListener(...)`), ajouter :

```js
document.getElementById("sendPage").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.url) return;
  chrome.runtime.sendMessage({ cmd: "sendPage", url: tab.url, tabId: tab.id }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});
```

- [ ] **Step 3: Revue manuelle**

Vérifier que `flash` et `chrome.tabs`/`chrome.runtime` sont déjà utilisés dans `popup.js` (oui) ; le bouton est dans `#connectedView` donc visible seulement quand l'app est connectée.

- [ ] **Step 4: Commit**

```bash
git checkout phase-4c2-ytdlp-extension
git add extension/popup.html extension/popup.js
git commit -m "feat(extension): bouton popup \"Télécharger la vidéo de cette page\""
```

---

## Task 4: Vérification finale

**Files:** aucun (vérification globale).

- [ ] **Step 1: Suites automatiques**

Run: `cd src-tauri && cargo test` ; `cd src-tauri/crates/download-core && cargo test` ; `cd c:/Users/steph/Documents/H/idm-clone && npm test && npm run build`
Expected: tout vert (tauri lib **10**), **0 warning rustc**, build OK. (Le front n'est pas modifié — Vitest inchangé.)

- [ ] **Step 2: Smoke manuel (note pour le réviseur)**

Recharger l'extension dans le navigateur, ouvrir une page vidéo (ex. YouTube), ouvrir le popup → cliquer « Télécharger la vidéo de cette page » : vérifier que l'app fait surgir la boîte de variantes (titre pré-rempli), choisir une qualité, vérifier le téléchargement. Tester aussi : une vidéo nécessitant une connexion (les cookies du navigateur doivent permettre l'extraction) ; une page sans vidéo (→ bannière « aucune vidéo trouvée sur cette page »).

- [ ] **Step 3: Commit éventuel** (si ajustements)

```bash
git checkout phase-4c2-ytdlp-extension
git add -A
git commit -m "chore(phase-4c2): vérification finale bouton extension yt-dlp"
```

---

## Notes d'implémentation

- **DRY :** `sendPage` calque `sendMedia` ; le routage réutilise `probe_ytdlp` sans dupliquer la logique.
- **YAGNI :** pas de détection auto de page vidéo, pas de menu contextuel, pas de badge.
- **Sécurité :** la capture reste soumise à l'auth du bridge (jeton) ; `handle_message` refuse toute capture avant `Auth` (inchangé).
- **Garde git :** `git checkout phase-4c2-ytdlp-extension` avant chaque commit de subagent.
