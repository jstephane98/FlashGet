# Phase 3a — Browser Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the end-to-end manual-capture pipe — a local WebSocket server inside the FlashGet app plus a Chromium MV3 extension that sends links (right-click + popup) to the app, which downloads them through the existing add pipeline.

**Architecture:** A new Rust `bridge` module splits **pure, unit-tested protocol logic** (`parse_client_message` + `handle_message`) from a thin async I/O loop (`tokio-tungstenite`) launched via `tauri::async_runtime::spawn`. On an authenticated `capture` the loop calls a refactored, reusable `commands::add_url` helper (the same probe → categorize → category-dir → queue path that `add_download` already uses). A mandatory token (persisted in the `settings` table, paired by manual copy-paste) gates every capture. A new `extension/` folder holds the MV3 extension with a pure `buildCapture` helper tested under Vitest.

**Tech Stack:** Rust + Tauri 2, `tokio-tungstenite` (new dep), `futures-util` (new dep), `rusqlite` (existing), `uuid` (existing); React + TypeScript + Tailwind; Chromium MV3 (vanilla JS ES modules); Vitest.

---

## File Structure

**Rust (new):**
- `src-tauri/src/bridge/mod.rs` — module root: re-exports protocol items, `PORT` const, `start(app)` entry point.
- `src-tauri/src/bridge/protocol.rs` — **pure** protocol: `ClientMessage`, `parse_client_message`, `ServerReply`, `handle_message`. No tokio, no tauri. Fully TDD'd.
- `src-tauri/src/bridge/server.rs` — thin WS loop (`tokio-tungstenite`), binds loopback, dispatches to `handle_message`, performs captures via `commands::add_url`, replies with JSON.

**Rust (modified):**
- `src-tauri/src/commands.rs` — extract `add_url` helper; add `get_bridge_info` + `regenerate_bridge_token` commands.
- `src-tauri/src/state.rs` — add `bridge_listening: Arc<AtomicBool>` field.
- `src-tauri/src/lib.rs` — `mod bridge;`, register 2 new commands, call `bridge::start(...)` in setup.
- `src-tauri/Cargo.toml` — add `tokio-tungstenite`, `futures-util`.
- `src-tauri/crates/download-core/src/storage.rs` — add `get_or_create_bridge_token` + `regenerate_bridge_token`.

**Frontend (modified):**
- `src/types.ts` — add `BridgeInfo`.
- `src/api.ts` — add `getBridgeInfo`, `regenerateBridgeToken`.
- `src/components/SettingsDialog.tsx` — add "🧩 Extension navigateur" section.

**Extension (new folder `extension/`):**
- `extension/manifest.json` — MV3 manifest.
- `extension/buildCapture.js` — pure helper (ES module).
- `extension/buildCapture.test.js` — Vitest test.
- `extension/background.js` — service worker: WS connect + reconnect + auth + capture + context menu.
- `extension/popup.html` / `extension/popup.js` — status + paste-URL + send-current-tab.
- `extension/options.html` / `extension/options.js` — token + port stored in `chrome.storage`.
- `extension/README.md` — load-unpacked instructions.

---

## Task 1: Pure bridge protocol (`parse_client_message` + `handle_message`)

**Files:**
- Create: `src-tauri/src/bridge/protocol.rs`
- Create: `src-tauri/src/bridge/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod bridge;`)
- Test: inline `#[cfg(test)] mod tests` in `protocol.rs`

- [ ] **Step 1: Create the module so the crate compiles**

Create `src-tauri/src/bridge/mod.rs`:

```rust
pub mod protocol;

pub use protocol::{handle_message, parse_client_message, ClientMessage, ServerReply};
```

Add `mod bridge;` to `src-tauri/src/lib.rs` directly under the existing `mod commands;` / `mod state;` lines (top of file):

```rust
mod bridge;
mod commands;
mod state;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/bridge/protocol.rs` with ONLY the tests first (types/functions come next step):

```rust
//! Logique de protocole du bridge navigateur — PURE et testable (aucune I/O, aucun tauri).
//! La boucle WebSocket (server.rs) est une fine couche qui appelle ces fonctions.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_before_auth_is_refused() {
        let msg = parse_client_message(r#"{"type":"capture","url":"http://x/f.zip"}"#);
        let (authed, reply) = handle_message(false, "secret", msg);
        assert!(!authed, "doit rester non authentifié");
        assert!(matches!(reply, ServerReply::NeedAuth));
    }

    #[test]
    fn auth_with_good_token_succeeds() {
        let msg = parse_client_message(r#"{"type":"auth","token":"secret"}"#);
        let (authed, reply) = handle_message(false, "secret", msg);
        assert!(authed);
        assert!(matches!(reply, ServerReply::Authed));
    }

    #[test]
    fn auth_with_bad_token_fails_and_stays_unauthed() {
        let msg = parse_client_message(r#"{"type":"auth","token":"wrong"}"#);
        let (authed, reply) = handle_message(false, "secret", msg);
        assert!(!authed);
        assert!(matches!(reply, ServerReply::AuthFailed));
    }

    #[test]
    fn capture_after_auth_yields_capture_requested() {
        let msg = parse_client_message(r#"{"type":"capture","url":"http://x/f.zip"}"#);
        let (authed, reply) = handle_message(true, "secret", msg);
        assert!(authed, "reste authentifié");
        match reply {
            ServerReply::CaptureRequested { url, .. } => assert_eq!(url, "http://x/f.zip"),
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_error() {
        let msg = parse_client_message("not json at all");
        assert!(matches!(msg, ClientMessage::Unknown));
        let (authed, reply) = handle_message(true, "secret", msg);
        assert!(authed);
        assert!(matches!(reply, ServerReply::Error { .. }));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail (won't compile)**

Run: `cargo test -p tauri-app --lib bridge::protocol`
Expected: FAIL — `cannot find function parse_client_message` / `ClientMessage` not defined.

- [ ] **Step 4: Write the minimal implementation**

Insert ABOVE the `#[cfg(test)]` block in `src-tauri/src/bridge/protocol.rs`:

```rust
use serde::Deserialize;

/// Message reçu du client (extension). `Unknown` = JSON invalide ou type inconnu.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Auth { token: String },
    Capture { url: String, filename: Option<String>, referrer: Option<String> },
    Unknown,
}

/// Réponse logique du serveur. `CaptureRequested` signale à la couche I/O de lancer le téléchargement.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerReply {
    Authed,
    AuthFailed,
    NeedAuth,
    CaptureRequested { url: String, filename: Option<String>, referrer: Option<String> },
    Error { msg: String },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RawMessage {
    Auth { token: String },
    Capture { url: String, #[serde(default)] filename: Option<String>, #[serde(default)] referrer: Option<String> },
}

/// Parse un texte JSON en `ClientMessage`. Tout échec ⇒ `Unknown`.
pub fn parse_client_message(text: &str) -> ClientMessage {
    match serde_json::from_str::<RawMessage>(text) {
        Ok(RawMessage::Auth { token }) => ClientMessage::Auth { token },
        Ok(RawMessage::Capture { url, filename, referrer }) => {
            ClientMessage::Capture { url, filename, referrer }
        }
        Err(_) => ClientMessage::Unknown,
    }
}

/// Transition pure : (déjà authentifié, jeton attendu, message) -> (nouvel état authentifié, réponse).
/// Règle clé : une `Capture` reçue avant un `Auth` valide ⇒ `NeedAuth` (jamais de capture).
pub fn handle_message(
    authed: bool,
    expected_token: &str,
    msg: ClientMessage,
) -> (bool, ServerReply) {
    match msg {
        ClientMessage::Auth { token } => {
            if !expected_token.is_empty() && token == expected_token {
                (true, ServerReply::Authed)
            } else {
                (false, ServerReply::AuthFailed)
            }
        }
        ClientMessage::Capture { url, filename, referrer } => {
            if authed {
                (authed, ServerReply::CaptureRequested { url, filename, referrer })
            } else {
                (authed, ServerReply::NeedAuth)
            }
        }
        ClientMessage::Unknown => (authed, ServerReply::Error { msg: "message invalide".into() }),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p tauri-app --lib bridge::protocol`
Expected: PASS — 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/bridge/ src-tauri/src/lib.rs
git commit -m "feat(bridge): pure protocol parse_client_message + handle_message (TDD)"
```

---

## Task 2: Bridge token in storage (`get_or_create_bridge_token` / `regenerate_bridge_token`)

**Files:**
- Modify: `src-tauri/crates/download-core/src/storage.rs`
- Test: inline `#[cfg(test)] mod tests` in `storage.rs` (append to existing block)

- [ ] **Step 1: Write the failing tests**

In `src-tauri/crates/download-core/src/storage.rs`, inside the existing `#[cfg(test)] mod tests { ... }` block (near the other `#[test]` fns), add:

```rust
    #[test]
    fn bridge_token_is_stable_and_non_empty() {
        let s = Storage::open_in_memory().unwrap();
        let a = s.get_or_create_bridge_token().unwrap();
        let b = s.get_or_create_bridge_token().unwrap();
        assert!(!a.is_empty());
        assert_eq!(a, b, "le jeton doit être stable entre appels");
    }

    #[test]
    fn regenerate_bridge_token_changes_value() {
        let s = Storage::open_in_memory().unwrap();
        let a = s.get_or_create_bridge_token().unwrap();
        let b = s.regenerate_bridge_token().unwrap();
        assert_ne!(a, b, "régénérer doit changer le jeton");
        assert!(!b.is_empty());
        // Le nouveau jeton est désormais celui qui persiste.
        assert_eq!(b, s.get_or_create_bridge_token().unwrap());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p download-core bridge_token`
Expected: FAIL — `no method named get_or_create_bridge_token`.

- [ ] **Step 3: Write the minimal implementation**

In `src-tauri/crates/download-core/src/storage.rs`, add these two methods inside `impl Storage { ... }` (place them right after `set_category_dir`, near line 213):

```rust
    /// Jeton du bridge navigateur (persisté en `settings`, clé `bridge.token`).
    /// Généré fort au premier appel, puis stable.
    pub fn get_or_create_bridge_token(&self) -> Result<String, CoreError> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key='bridge.token'", [], |r| r.get::<_, String>(0))
            .ok()
            .filter(|s| !s.is_empty());
        if let Some(tok) = existing {
            return Ok(tok);
        }
        let token = new_bridge_token();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('bridge.token', ?1) ON CONFLICT(key) DO UPDATE SET value=?1",
            [token.as_str()],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(token)
    }

    /// Régénère et persiste un nouveau jeton ; renvoie la nouvelle valeur.
    pub fn regenerate_bridge_token(&self) -> Result<String, CoreError> {
        let token = new_bridge_token();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('bridge.token', ?1) ON CONFLICT(key) DO UPDATE SET value=?1",
            [token.as_str()],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(token)
    }
```

Add this free function at the bottom of the file, OUTSIDE `impl Storage` and OUTSIDE the `#[cfg(test)]` block (e.g. just above `mod tests`):

```rust
/// Jeton aléatoire fort : deux UUID v4 concaténés en hexadécimal (64 caractères).
fn new_bridge_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
```

> Note: `uuid` (v4) is already a dependency of `download-core` — no Cargo.toml change needed. Verify with `Grep "uuid" src-tauri/crates/download-core/Cargo.toml` if unsure.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p download-core bridge_token`
Expected: PASS — both tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/storage.rs
git commit -m "feat(storage): bridge token get_or_create + regenerate (TDD)"
```

---

## Task 3: WS server, reusable `add_url`, commands & startup wiring

This task has little pure logic to TDD (the protocol is already covered by Task 1). It refactors `add_download` into a reusable helper, adds the `tokio-tungstenite` loop, two Tauri commands, the `bridge_listening` flag, and the startup spawn. Verification is `cargo build` + `cargo test` (protocol tests must still pass) + the manual smoke test in Task 6.

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/bridge/mod.rs`
- Create: `src-tauri/src/bridge/server.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add WebSocket dependencies and the tokio `net` feature**

In `src-tauri/Cargo.toml`, under `[dependencies]`, add the two new deps after the `uuid` line:

```toml
tokio-tungstenite = "0.24"
futures-util = "0.3"
```

And add the `net` feature to the existing `tokio` line (required for `TcpListener`/`TcpStream`):

```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "net"] }
```

- [ ] **Step 2: Verify it builds with the new deps**

Run: `cargo build -p tauri-app`
Expected: builds (downloads `tokio-tungstenite`, `futures-util`). No code uses them yet.

- [ ] **Step 3: Refactor `add_download` to expose a reusable `add_url`**

In `src-tauri/src/commands.rs`, replace the whole `add_download` function (lines 18–67) with a thin command that delegates to a new `pub(crate)` helper holding the moved body:

```rust
/// Crée un téléchargement (statut Queued), le persiste, puis ré-évalue la file.
/// Logique réutilisable par la commande Tauri ET par le bridge navigateur.
pub(crate) async fn add_url(app: &AppHandle, url: &str, dest_dir: &str) -> Result<Download, String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(url))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;
    let file_name = probe.file_name.clone().unwrap_or_else(|| "download.bin".to_string());

    let category = download_core::categorizer::categorize(&file_name);

    // Dossier : argument explicite > dossier configuré pour la catégorie > dossier Téléchargements.
    let dest_dir = if !dest_dir.trim().is_empty() {
        PathBuf::from(dest_dir)
    } else {
        let configured = { app.state::<AppState>().storage.category_dir(category).map_err(|e| e.to_string())? };
        match configured {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;

    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("invalid file name derived from URL".to_string());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: probe.total_bytes,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: probe.supports_ranges,
        error: None,
        priority: Priority::Normal,
        category,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    reevaluate(app);
    Ok(download)
}

/// Crée un téléchargement (statut Queued), le persiste, puis ré-évalue la file.
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    add_url(&app, &args.url, &args.dest_dir).await
}
```

> The `reevaluate(app)` call now takes `app: &AppHandle` directly (the helper already has a reference). `reevaluate`'s signature `fn reevaluate(app: &AppHandle)` is unchanged.

- [ ] **Step 4: Add the `bridge_listening` flag to AppState**

In `src-tauri/src/state.rs`, update imports and the struct + constructor:

```rust
use download_core::engine::DownloadHandle;
use download_core::http::ReqwestFetcher;
use download_core::rate_limiter::RateLimiter;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// État global géré par Tauri.
pub struct AppState {
    pub storage: Storage,
    pub handles: Mutex<HashMap<String, DownloadHandle>>,
    /// Fetcher mutualisé (un seul `reqwest::Client` partagé).
    pub fetcher: ReqwestFetcher,
    /// Limiteur de bande passante global partagé.
    pub limiter: Arc<RateLimiter>,
    /// `true` une fois le serveur WebSocket du bridge à l'écoute sur la loopback.
    pub bridge_listening: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(storage: Storage, max_speed_bps: u64) -> Self {
        Self {
            storage,
            handles: Mutex::new(HashMap::new()),
            fetcher: ReqwestFetcher::new(),
            limiter: Arc::new(RateLimiter::new(max_speed_bps)),
            bridge_listening: Arc::new(AtomicBool::new(false)),
        }
    }
}
```

- [ ] **Step 5: Add the two bridge Tauri commands**

At the END of `src-tauri/src/commands.rs`, append:

```rust
#[derive(serde::Serialize)]
pub struct BridgeInfo {
    pub token: String,
    pub port: u16,
    pub listening: bool,
}

/// Renvoie le jeton (créé si absent), le port d'écoute et l'état du serveur local.
#[tauri::command]
pub fn get_bridge_info(state: State<AppState>) -> Result<BridgeInfo, String> {
    let token = state.storage.get_or_create_bridge_token().map_err(|e| e.to_string())?;
    Ok(BridgeInfo {
        token,
        port: crate::bridge::PORT,
        listening: state.bridge_listening.load(std::sync::atomic::Ordering::Relaxed),
    })
}

/// Régénère le jeton du bridge et renvoie la nouvelle valeur.
#[tauri::command]
pub fn regenerate_bridge_token(state: State<AppState>) -> Result<String, String> {
    state.storage.regenerate_bridge_token().map_err(|e| e.to_string())
}
```

- [ ] **Step 6: Write the WS server loop**

Create `src-tauri/src/bridge/server.rs`:

```rust
//! Boucle WebSocket du bridge — fine couche I/O au-dessus de la logique pure `protocol`.
//! Lancée via `tauri::async_runtime::spawn` (JAMAIS `tokio::spawn` direct).

use crate::bridge::protocol::{handle_message, parse_client_message, ServerReply};
use crate::commands;
use crate::state::AppState;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

/// Port d'écoute loopback par défaut (l'extension est configurable côté options).
pub const PORT: u16 = 15151;

/// Démarre le serveur WebSocket loopback. À appeler depuis le hook `setup`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let addr = format!("127.0.0.1:{PORT}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("bridge: impossible d'écouter sur {addr}: {e}");
                return;
            }
        };
        app.state::<AppState>().bridge_listening.store(true, Ordering::Relaxed);
        eprintln!("bridge: à l'écoute sur ws://{addr}");
        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(handle_conn(app2, stream));
                }
                Err(e) => eprintln!("bridge: accept a échoué: {e}"),
            }
        }
    });
}

/// Gère une connexion : handshake WS, puis boucle lecture → handle_message → action → réponse JSON.
async fn handle_conn(app: AppHandle, stream: TcpStream) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("bridge: handshake WS échoué: {e}");
            return;
        }
    };
    let (mut write, mut read) = ws.split();
    let mut authed = false;
    // Jeton attendu lu une fois à la connexion.
    let expected = app
        .state::<AppState>()
        .storage
        .get_or_create_bridge_token()
        .unwrap_or_default();

    while let Some(Ok(msg)) = read.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue, // ignore ping/pong/binary
        };
        let parsed = parse_client_message(&text);
        let (new_authed, reply) = handle_message(authed, &expected, parsed);
        authed = new_authed;

        let out = match reply {
            ServerReply::Authed => serde_json::json!({ "type": "authed" }).to_string(),
            ServerReply::AuthFailed => {
                serde_json::json!({ "type": "error", "error": "auth_failed" }).to_string()
            }
            ServerReply::NeedAuth => {
                serde_json::json!({ "type": "error", "error": "need_auth" }).to_string()
            }
            ServerReply::Error { msg } => {
                serde_json::json!({ "type": "error", "error": msg }).to_string()
            }
            ServerReply::CaptureRequested { url, .. } => {
                // Capture via le pipeline d'ajout existant (dossier vide ⇒ dossier de catégorie).
                match commands::add_url(&app, &url, "").await {
                    Ok(d) => serde_json::json!({ "type": "ack", "id": d.id }).to_string(),
                    Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                }
            }
        };
        if write.send(Message::Text(out)).await.is_err() {
            break;
        }
    }
}
```

- [ ] **Step 7: Export server items from the module root**

Replace `src-tauri/src/bridge/mod.rs` with:

```rust
pub mod protocol;
pub mod server;

pub use protocol::{handle_message, parse_client_message, ClientMessage, ServerReply};
pub use server::{start, PORT};
```

- [ ] **Step 8: Register commands and start the bridge in `lib.rs`**

In `src-tauri/src/lib.rs`, add the two commands to the `generate_handler!` list (after `commands::open_folder,`):

```rust
            commands::open_file,
            commands::open_folder,
            commands::get_bridge_info,
            commands::regenerate_bridge_token,
        ])
```

And in the `.setup(|app| { ... })` hook, after `app.manage(AppState::new(...));` and before `Ok(())`, add:

```rust
            app.manage(AppState::new(storage, settings.max_speed_bps));
            bridge::start(app.handle().clone());
            Ok(())
```

- [ ] **Step 9: Build and run the full Rust test suite**

Run: `cargo build -p tauri-app && cargo test`
Expected: builds clean; all existing tests + Task 1/Task 2 tests PASS.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/
git commit -m "feat(bridge): loopback WS server, reusable add_url, bridge commands + startup wiring"
```

---

## Task 4: App UI — "Extension navigateur" settings section

**Files:**
- Modify: `src/types.ts`
- Modify: `src/api.ts`
- Modify: `src/components/SettingsDialog.tsx`

- [ ] **Step 1: Add the `BridgeInfo` type**

In `src/types.ts`, append:

```ts
export interface BridgeInfo {
  token: string;
  port: number;
  listening: boolean;
}
```

- [ ] **Step 2: Add the API bindings**

In `src/api.ts`, add (keep the existing `invoke` import and `BridgeInfo` to the type import line from `./types`):

```ts
export function getBridgeInfo(): Promise<BridgeInfo> {
  return invoke<BridgeInfo>("get_bridge_info");
}

export function regenerateBridgeToken(): Promise<string> {
  return invoke<string>("regenerate_bridge_token");
}
```

> Ensure `BridgeInfo` is added to the `import type { ... } from "./types";` line at the top of `api.ts`.

- [ ] **Step 3: Wire the section into SettingsDialog**

In `src/components/SettingsDialog.tsx`:

Add state + load inside the component, alongside the existing `dirs` state:

```tsx
  const [bridge, setBridge] = useState<BridgeInfo | null>(null);
  const [copied, setCopied] = useState(false);
```

Add to the existing `useEffect(() => { if (!open) return; ... }, [open])` body (after `api.getCategoryDirs().then(setDirs);`):

```tsx
    api.getBridgeInfo().then(setBridge);
```

Add these handlers inside the component (next to `pickDir`):

```tsx
  async function copyToken() {
    if (!bridge) return;
    await navigator.clipboard.writeText(bridge.token);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  async function regenToken() {
    const token = await api.regenerateBridgeToken();
    setBridge((b) => (b ? { ...b, token } : b));
  }
```

Add the JSX section just before the footer (`<div className="flex justify-end ...">`):

```tsx
        <div className="px-4 py-3 border-t border-gray-100">
          <div className="font-medium mb-2">🧩 Extension navigateur</div>
          <div className="flex items-center gap-2 mb-2 text-sm">
            <span
              className={
                "inline-block w-2 h-2 rounded-full " +
                (bridge?.listening ? "bg-green-500" : "bg-gray-400")
              }
            />
            <span className="text-gray-600">
              {bridge?.listening
                ? `Serveur local à l'écoute (port ${bridge.port})`
                : "Serveur local indisponible"}
            </span>
          </div>
          <label className="block text-xs text-gray-500 mb-1">
            Jeton d'appairage (à coller dans les options de l'extension)
          </label>
          <div className="flex items-center gap-2">
            <code className="flex-1 truncate bg-gray-100 rounded px-2 py-1 text-xs">
              {bridge?.token ?? "…"}
            </code>
            <button
              className="text-xs border rounded px-2 py-1 hover:bg-gray-50"
              onClick={copyToken}
            >
              {copied ? "Copié ✓" : "Copier"}
            </button>
            <button
              className="text-xs border rounded px-2 py-1 hover:bg-gray-50"
              onClick={regenToken}
            >
              Régénérer
            </button>
          </div>
        </div>
```

Add `BridgeInfo` to the `import type { ... } from "../types";` line at the top of the file.

- [ ] **Step 4: Verify the frontend compiles and tests pass**

Run: `npm run build` (or `npx tsc --noEmit`) then `npm test`
Expected: TypeScript compiles with no errors; existing Vitest suite still passes.

- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/api.ts src/components/SettingsDialog.tsx
git commit -m "feat(ui): browser-extension settings section (token copy/regenerate + server status)"
```

---

## Task 5: Chromium MV3 extension

**Files (all new, under `extension/`):**
- Create: `extension/buildCapture.js`
- Create: `extension/buildCapture.test.js`
- Create: `extension/manifest.json`
- Create: `extension/background.js`
- Create: `extension/popup.html`, `extension/popup.js`
- Create: `extension/options.html`, `extension/options.js`
- Create: `extension/README.md`

- [ ] **Step 1: Write the failing test for the pure helper**

Create `extension/buildCapture.test.js`:

```js
import { describe, it, expect } from "vitest";
import { buildCapture } from "./buildCapture.js";

describe("buildCapture", () => {
  it("builds a capture message for a valid url", () => {
    expect(buildCapture("http://x/file.zip")).toEqual({
      type: "capture",
      url: "http://x/file.zip",
    });
  });

  it("trims surrounding whitespace", () => {
    expect(buildCapture("  http://x/f.bin  ")).toEqual({
      type: "capture",
      url: "http://x/f.bin",
    });
  });

  it("throws on an empty url", () => {
    expect(() => buildCapture("   ")).toThrow();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: FAIL — cannot resolve `./buildCapture.js`.

- [ ] **Step 3: Write the helper**

Create `extension/buildCapture.js`:

```js
// Logique pure et testable : construit le message de capture WS à partir d'une URL.
export function buildCapture(url) {
  const trimmed = (url ?? "").trim();
  if (!trimmed) {
    throw new Error("URL vide");
  }
  return { type: "capture", url: trimmed };
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: PASS — 3 tests pass.

- [ ] **Step 5: Write the manifest**

Create `extension/manifest.json`:

```json
{
  "manifest_version": 3,
  "name": "FlashGet — Capture",
  "version": "0.1.0",
  "description": "Envoie des liens à l'application FlashGet (capture manuelle).",
  "permissions": ["contextMenus", "storage", "activeTab"],
  "host_permissions": ["ws://127.0.0.1/*"],
  "background": { "service_worker": "background.js", "type": "module" },
  "action": { "default_popup": "popup.html", "default_title": "FlashGet" },
  "options_page": "options.html"
}
```

- [ ] **Step 6: Write the service worker**

Create `extension/background.js`:

```js
import { buildCapture } from "./buildCapture.js";

const DEFAULT_PORT = 15151;
let socket = null;
let connected = false;
let reconnectTimer = null;

async function getConfig() {
  const { token = "", port = DEFAULT_PORT } = await chrome.storage.local.get(["token", "port"]);
  return { token, port: Number(port) || DEFAULT_PORT };
}

function setConnected(value) {
  connected = value;
  // Pastille verte/grise sur l'icône.
  chrome.action.setBadgeText({ text: value ? "●" : "" });
  chrome.action.setBadgeBackgroundColor({ color: value ? "#22c55e" : "#9ca3af" });
}

async function connect() {
  const { token, port } = await getConfig();
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    return;
  }
  try {
    socket = new WebSocket(`ws://127.0.0.1:${port}`);
  } catch (_e) {
    scheduleReconnect();
    return;
  }
  socket.onopen = () => {
    socket.send(JSON.stringify({ type: "auth", token }));
  };
  socket.onmessage = (event) => {
    let msg;
    try {
      msg = JSON.parse(event.data);
    } catch (_e) {
      return;
    }
    if (msg.type === "authed") {
      setConnected(true);
    } else if (msg.type === "error" && (msg.error === "auth_failed" || msg.error === "need_auth")) {
      setConnected(false);
    }
  };
  socket.onclose = () => {
    setConnected(false);
    scheduleReconnect();
  };
  socket.onerror = () => {
    setConnected(false);
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, 3000);
}

// Envoie une capture ; (re)connecte si nécessaire. Renvoie true si l'envoi a pu partir.
async function sendCapture(url) {
  if (!socket || socket.readyState !== WebSocket.OPEN || !connected) {
    connect();
    return false;
  }
  socket.send(JSON.stringify(buildCapture(url)));
  return true;
}

// Menu contextuel sur liens/images/vidéos/audio.
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "flashget-capture",
    title: "Télécharger avec FlashGet",
    contexts: ["link", "image", "video", "audio"],
  });
});

chrome.contextMenus.onClicked.addListener((info) => {
  const url = info.linkUrl || info.srcUrl || info.pageUrl;
  if (url) sendCapture(url);
});

// Messages depuis le popup.
chrome.runtime.onMessage.addListener((req, _sender, sendResponse) => {
  if (req.cmd === "status") {
    sendResponse({ connected });
    return false;
  }
  if (req.cmd === "send" && req.url) {
    sendCapture(req.url).then((ok) => sendResponse({ ok }));
    return true; // réponse asynchrone
  }
  if (req.cmd === "reconnect") {
    connect();
    sendResponse({ ok: true });
    return false;
  }
  return false;
});

// Connexion initiale au démarrage du service worker.
connect();
```

- [ ] **Step 7: Write the popup**

Create `extension/popup.html`:

```html
<!doctype html>
<html lang="fr">
  <head>
    <meta charset="utf-8" />
    <style>
      body { font-family: system-ui, sans-serif; width: 260px; margin: 0; padding: 12px; }
      .status { display: flex; align-items: center; gap: 6px; font-size: 13px; margin-bottom: 10px; }
      .dot { width: 8px; height: 8px; border-radius: 50%; }
      .on { background: #22c55e; } .off { background: #9ca3af; }
      input { width: 100%; box-sizing: border-box; padding: 6px; margin-bottom: 8px; }
      button { width: 100%; padding: 6px; margin-bottom: 6px; cursor: pointer; }
      .muted { font-size: 12px; color: #6b7280; }
      .hidden { display: none; }
      #toast { font-size: 12px; color: #16a34a; }
    </style>
  </head>
  <body>
    <div class="status">
      <span id="dot" class="dot off"></span>
      <span id="statusText">…</span>
    </div>

    <div id="connectedView" class="hidden">
      <input id="url" type="text" placeholder="Coller une URL…" />
      <button id="sendUrl">Envoyer à FlashGet</button>
      <button id="sendTab">Envoyer l'onglet courant</button>
      <div id="toast"></div>
    </div>

    <div id="disconnectedView" class="hidden">
      <p class="muted">FlashGet non détecté. Lancez l'application.</p>
      <button id="openOptions">Options</button>
      <button id="retry">Réessayer</button>
    </div>

    <script src="popup.js" type="module"></script>
  </body>
</html>
```

Create `extension/popup.js`:

```js
const dot = document.getElementById("dot");
const statusText = document.getElementById("statusText");
const connectedView = document.getElementById("connectedView");
const disconnectedView = document.getElementById("disconnectedView");
const toast = document.getElementById("toast");

function render(connected) {
  dot.className = "dot " + (connected ? "on" : "off");
  statusText.textContent = connected ? "Connecté" : "Non détecté";
  connectedView.classList.toggle("hidden", !connected);
  disconnectedView.classList.toggle("hidden", connected);
}

function refresh() {
  chrome.runtime.sendMessage({ cmd: "status" }, (res) => {
    render(Boolean(res && res.connected));
  });
}

function flash(text) {
  toast.textContent = text;
  setTimeout(() => (toast.textContent = ""), 1500);
}

document.getElementById("sendUrl").addEventListener("click", () => {
  const url = document.getElementById("url").value;
  if (!url.trim()) return;
  chrome.runtime.sendMessage({ cmd: "send", url }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("sendTab").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.url) return;
  chrome.runtime.sendMessage({ cmd: "send", url: tab.url }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("openOptions").addEventListener("click", () => {
  chrome.runtime.openOptionsPage();
});

document.getElementById("retry").addEventListener("click", () => {
  chrome.runtime.sendMessage({ cmd: "reconnect" }, () => setTimeout(refresh, 500));
});

refresh();
```

- [ ] **Step 8: Write the options page**

Create `extension/options.html`:

```html
<!doctype html>
<html lang="fr">
  <head>
    <meta charset="utf-8" />
    <style>
      body { font-family: system-ui, sans-serif; max-width: 420px; margin: 24px auto; padding: 0 16px; }
      label { display: block; font-size: 13px; margin: 12px 0 4px; }
      input { width: 100%; box-sizing: border-box; padding: 6px; }
      button { margin-top: 16px; padding: 6px 14px; cursor: pointer; }
      #saved { color: #16a34a; font-size: 13px; margin-left: 8px; }
    </style>
  </head>
  <body>
    <h3>Options FlashGet</h3>
    <label for="token">Jeton d'appairage (copié depuis l'application)</label>
    <input id="token" type="text" placeholder="Coller le jeton…" />
    <label for="port">Port</label>
    <input id="port" type="number" value="15151" />
    <div>
      <button id="save">Enregistrer</button>
      <span id="saved"></span>
    </div>
    <script src="options.js" type="module"></script>
  </body>
</html>
```

Create `extension/options.js`:

```js
const tokenInput = document.getElementById("token");
const portInput = document.getElementById("port");
const saved = document.getElementById("saved");

async function load() {
  const { token = "", port = 15151 } = await chrome.storage.local.get(["token", "port"]);
  tokenInput.value = token;
  portInput.value = port;
}

document.getElementById("save").addEventListener("click", async () => {
  await chrome.storage.local.set({
    token: tokenInput.value.trim(),
    port: Number(portInput.value) || 15151,
  });
  saved.textContent = "Enregistré ✓";
  setTimeout(() => (saved.textContent = ""), 1500);
  // Forcer une reconnexion avec le nouveau jeton/port.
  chrome.runtime.sendMessage({ cmd: "reconnect" });
});

load();
```

- [ ] **Step 9: Write the load instructions**

Create `extension/README.md`:

```markdown
# Extension FlashGet (Chromium MV3)

Capture manuelle de liens vers l'application FlashGet.

## Charger en non empaqueté (Chrome / Edge)

1. Lancer l'application FlashGet (le serveur local écoute sur `127.0.0.1:15151`).
2. Ouvrir `chrome://extensions` (ou `edge://extensions`).
3. Activer le **mode développeur**.
4. **Charger l'extension non empaquetée** → sélectionner ce dossier `extension/`.
5. Dans l'application : Réglages → 🧩 Extension navigateur → **Copier** le jeton.
6. Dans l'extension : **Options** → coller le jeton → Enregistrer.
7. La pastille de l'icône passe au vert (connecté).

## Usage

- **Clic droit** sur un lien/image/vidéo/audio → « Télécharger avec FlashGet ».
- **Popup** : coller une URL, ou « Envoyer l'onglet courant ».
```

- [ ] **Step 10: Run the extension test suite**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add extension/
git commit -m "feat(extension): Chromium MV3 manual-capture extension (background/popup/options + buildCapture TDD)"
```

---

## Task 6: Verification (suites + manual smoke test)

**Files:** none (verification only).

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test`
Expected: all tests PASS, including `bridge::protocol` (5) and storage `bridge_token` (2).

- [ ] **Step 2: Run the full frontend + extension test suite**

Run: `npm test`
Expected: all Vitest tests PASS (existing + `extension/buildCapture.test.js`).

- [ ] **Step 3: Type-check / build the frontend**

Run: `npm run build`
Expected: builds with no TypeScript errors.

- [ ] **Step 4: Build the Tauri app**

Run: `cargo build -p tauri-app`
Expected: clean build.

- [ ] **Step 5: Manual smoke test (documented, run by a human)**

Record the outcome of each in the PR/commit description:

1. Launch the app (`npm run tauri dev`). Open Réglages → confirm "🧩 Extension navigateur" shows a green dot, "Serveur local à l'écoute (port 15151)", and a 64-char token. Click **Copier**.
2. Load `extension/` unpacked in Chrome (see `extension/README.md`). Open Options, paste the token, Enregistrer. Confirm the icon badge turns green.
3. Right-click a direct file link (e.g. a `.zip`) → "Télécharger avec FlashGet". Confirm a new download appears in the app's queue, in the correct category folder.
4. Open the popup, paste a URL, "Envoyer à FlashGet" → confirm "Envoyé ✓" and the download appears.
5. Quit the app. Open the popup → confirm it shows "● Non détecté" with the Options / Réessayer buttons.
6. Relaunch the app → confirm the extension reconnects automatically (badge green again) within a few seconds.
7. In Réglages, click **Régénérer** the token. Confirm the extension (still holding the old token) drops to disconnected; paste the new token in Options → reconnects.

- [ ] **Step 6: Final commit (if any verification fixes were needed)**

```bash
git add -A
git commit -m "test(3a): verification pass — Rust + Vitest suites green, smoke test recorded"
```

---

## Notes for the executor

- **Never run on `main`.** Create branch `phase-3a-browser-bridge` first (the subagent-driven / executing-plans skill handles this; if executing manually: `git checkout -b phase-3a-browser-bridge`).
- **Double review** the `bridge` module (protocol + server) and the `add_url` refactor — these are the load-bearing pieces. The refactor must not change `add_download`'s external behavior.
- The WS loop (`server.rs`) is intentionally a thin, untested-by-unit-tests layer; all branching logic lives in the pure `protocol.rs` (Task 1) and is verified by the manual smoke test (Task 6).
- `tokio-tungstenite` version `0.24` is the assumption; if the lockfile resolves a different compatible minor, that's fine — adjust only if the build fails.
- Out of scope for 3a (do NOT implement): cookies/referrer forwarding, automatic capture/interception, Firefox, HLS/DASH. `referrer`/`filename` are parsed into `ClientMessage::Capture` for forward-compat but deliberately ignored by `server.rs` in 3a.
- On merge: `git checkout main && git merge --no-ff phase-3a-browser-bridge`.
