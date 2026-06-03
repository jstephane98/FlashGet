# Phase 3b-1 — Cookies/Referer/User-Agent Forwarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Forward the browser's cookies, referer, and user-agent for a captured link to the app, and make the Rust download engine apply them on every HTTP request, so downloads behind authentication work.

**Architecture:** A `RequestHeaders` struct travels from the extension's capture message → bridge → `add_url` → the persisted `Download` (3 new columns) → `engine::download` → `ByteFetcher::probe`/`fetch_range`, where `ReqwestFetcher` sets `Cookie`/`Referer`/`User-Agent` per request (shared client preserved). The extension reads cookies via `chrome.cookies` (`<all_urls>` + `cookies` permission).

**Tech Stack:** Rust (download-core: rusqlite, reqwest, tokio, tiny_http tests), Tauri 2, tokio-tungstenite; Chromium MV3 (vanilla JS) + Vitest; React/TypeScript.

---

## File Structure

**Rust (modified):**
- `src-tauri/crates/download-core/src/model.rs` — add `RequestHeaders` struct + 3 `Download` fields + `Download::request_headers()`.
- `src-tauri/crates/download-core/src/storage.rs` — 3-column migration + upsert/list/get_by_id round-trip.
- `src-tauri/crates/download-core/src/http.rs` — `ByteFetcher` signature change + `ReqwestFetcher` applies headers + `apply_headers` helper.
- `src-tauri/crates/download-core/src/tests/fake_fetcher.rs` — new signature (ignores headers).
- `src-tauri/crates/download-core/src/engine.rs` — thread `&RequestHeaders` through `download()` + update its tests.
- `src-tauri/crates/download-core/tests/real_http.rs` — update `download()` call sites.
- `src-tauri/crates/download-core/tests/request_headers.rs` — **new** tiny_http test asserting headers on the wire.
- `src-tauri/src/bridge/protocol.rs` — `cookie`/`user_agent` in `Capture`/`CaptureRequested`.
- `src-tauri/src/commands.rs` — `add_url` gains a `RequestHeaders` param; `launch` passes the download's headers.
- `src-tauri/src/bridge/server.rs` — pass captured headers to `add_url`.
- `src/types.ts` — optional `referrer`/`cookie`/`user_agent` on `Download`.

**Extension (modified):**
- `extension/manifest.json` — add `cookies` permission + `<all_urls>` host permission.
- `extension/buildCapture.js` — `buildCapture(url, extras)`.
- `extension/buildCapture.test.js` — extras tests.
- `extension/background.js` — `gatherHeaders`, `sendCapture(url, referrer)`, enriched `pending`.

---

## Task 1: `RequestHeaders` + `Download` header fields + storage migration

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs`
- Modify: `src-tauri/crates/download-core/src/storage.rs`
- Modify: `src-tauri/src/commands.rs` (Download literal in `add_url` — placeholder Nones)
- Test: inline `#[cfg(test)] mod tests` in `storage.rs`

- [ ] **Step 1: Write the failing round-trip test**

In `src-tauri/crates/download-core/src/storage.rs`, inside the existing `#[cfg(test)] mod tests { ... }` block, add:

```rust
    #[test]
    fn upsert_then_list_roundtrips_headers() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("h1");
        d.referrer = Some("http://example.com/page".into());
        d.cookie = Some("sid=abc; theme=dark".into());
        d.user_agent = Some("Mozilla/5.0 (Test)".into());
        s.upsert(&d).unwrap();

        let got = s.get_by_id("h1").unwrap().unwrap();
        assert_eq!(got.referrer.as_deref(), Some("http://example.com/page"));
        assert_eq!(got.cookie.as_deref(), Some("sid=abc; theme=dark"));
        assert_eq!(got.user_agent.as_deref(), Some("Mozilla/5.0 (Test)"));

        // Absents => NULL => None.
        let plain = sample("h2");
        s.upsert(&plain).unwrap();
        let got2 = s.get_by_id("h2").unwrap().unwrap();
        assert_eq!(got2.referrer, None);
        assert_eq!(got2.cookie, None);
        assert_eq!(got2.user_agent, None);
    }
```

- [ ] **Step 2: Run the test to verify it fails (won't compile)**

Run: `cargo test -p download-core upsert_then_list_roundtrips_headers`
Expected: FAIL — `no field 'referrer' on type Download` (fields don't exist yet).

- [ ] **Step 3: Add `RequestHeaders` + `Download` fields + method**

In `src-tauri/crates/download-core/src/model.rs`, add the 3 fields to `Download` (after `category`):

```rust
    #[serde(default)]
    pub category: Category,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub cookie: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
}
```

Add the `RequestHeaders` struct and a `Download` method (after the `Download` struct definition):

```rust
/// En-têtes HTTP optionnels transmis par le navigateur pour un téléchargement capturé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestHeaders {
    pub referrer: Option<String>,
    pub cookie: Option<String>,
    pub user_agent: Option<String>,
}

impl Download {
    /// Copie les en-têtes HTTP optionnels associés à ce téléchargement.
    pub fn request_headers(&self) -> RequestHeaders {
        RequestHeaders {
            referrer: self.referrer.clone(),
            cookie: self.cookie.clone(),
            user_agent: self.user_agent.clone(),
        }
    }
}
```

- [ ] **Step 4: Add the storage migration + round-trip columns**

In `src-tauri/crates/download-core/src/storage.rs`, in `from_conn`, after the `category` ALTER (around line 49), add 3 nullable columns:

```rust
        // Migration additive : en-têtes optionnels (3b-1). Nullable, ignorer si déjà présent.
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN referrer TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN cookie TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN user_agent TEXT", []);
```

Replace the `upsert` SQL + params to include the 3 columns:

```rust
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9, priority=?10, category=?11,
                referrer=?12, cookie=?13, user_agent=?14",
            rusqlite::params![
                d.id, d.url, d.file_name, d.dest_path,
                d.total_bytes.map(|v| v as i64),
                d.downloaded_bytes as i64,
                status_to_str(d.status),
                d.supports_ranges as i64,
                d.error,
                priority_to_str(d.priority),
                category_to_str(d.category),
                d.referrer,
                d.cookie,
                d.user_agent,
            ],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
```

In `list`, change the SELECT and the row mapping:

```rust
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent FROM downloads ORDER BY rowid")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Download {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    dest_path: row.get(3)?,
                    total_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    downloaded_bytes: row.get::<_, i64>(5)? as u64,
                    status: status_from_str(&row.get::<_, String>(6)?),
                    supports_ranges: row.get::<_, i64>(7)? != 0,
                    error: row.get(8)?,
                    priority: priority_from_str(&row.get::<_, String>(9)?),
                    category: category_from_str(&row.get::<_, String>(10)?),
                    referrer: row.get(11)?,
                    cookie: row.get(12)?,
                    user_agent: row.get(13)?,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
```

In `get_by_id`, change the SELECT and its row mapping identically (the `prepare(...)` SELECT string and the `Ok(Download { ... })` closure both need the 3 columns appended exactly as in `list`):

```rust
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent FROM downloads WHERE id=?1")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
```
and in its `query_map` closure add the same three lines before the closing `})`:
```rust
                    referrer: row.get(11)?,
                    cookie: row.get(12)?,
                    user_agent: row.get(13)?,
```

- [ ] **Step 5: Update the storage test `sample()` and the `add_url` Download literal so the crate compiles**

In `storage.rs` tests, update `sample(...)` to include the new fields (find the `Download { ... }` it returns and add before the closing brace):

```rust
            category: Category::Other,
            referrer: None,
            cookie: None,
            user_agent: None,
        }
```

In `src-tauri/src/commands.rs`, the `Download { ... }` literal inside `add_url` must compile — add placeholder Nones (Task 4 wires them to real headers):

```rust
        priority: Priority::Normal,
        category,
        referrer: None,
        cookie: None,
        user_agent: None,
    };
```

> If `cargo build` reports any other `Download { ... }` literal missing fields, add `referrer: None, cookie: None, user_agent: None` there too — the compiler will list every site.

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p download-core upsert_then_list_roundtrips_headers && cargo build -p tauri-app`
Expected: test PASS; `tauri-app` builds.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/download-core/src/model.rs src-tauri/crates/download-core/src/storage.rs src-tauri/src/commands.rs
git commit -m "feat(core): RequestHeaders + Download header fields + storage migration (TDD)"
```

---

## Task 2: `ByteFetcher` carries headers; `ReqwestFetcher` sends them

**Files:**
- Modify: `src-tauri/crates/download-core/src/http.rs`
- Modify: `src-tauri/crates/download-core/src/tests/fake_fetcher.rs`
- Modify: `src-tauri/crates/download-core/src/engine.rs` (signature + internals + tests)
- Modify: `src-tauri/crates/download-core/tests/real_http.rs` (call sites)
- Modify: `src-tauri/src/commands.rs` (`launch` call site)
- Test: `src-tauri/crates/download-core/tests/request_headers.rs` (new)

- [ ] **Step 1: Write the failing on-the-wire header test**

Create `src-tauri/crates/download-core/tests/request_headers.rs`:

```rust
//! Vérifie que `ReqwestFetcher` envoie bien Cookie/Referer/User-Agent quand fournis.
use download_core::http::{ByteFetcher, ReqwestFetcher};
use download_core::model::RequestHeaders;
use std::sync::{Arc, Mutex};
use tiny_http::{Response, Server};

/// Serveur loopback qui enregistre les en-têtes de la 1re requête puis répond 200.
fn start_recording_server(seen: Arc<Mutex<Vec<String>>>) -> u16 {
    let server = Server::http("127.0.0.1:0").expect("bind ephemeral port");
    let port = server.server_addr().to_ip().expect("ip addr").port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let lines: Vec<String> = request
                .headers()
                .iter()
                .map(|h| format!("{}: {}", h.field.as_str().as_str(), h.value.as_str()))
                .collect();
            *seen.lock().unwrap() = lines;
            let _ = request.respond(Response::from_string("ok"));
            break;
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_forwards_cookie_referer_user_agent() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let port = start_recording_server(seen.clone());
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/file.bin");
    let headers = RequestHeaders {
        referrer: Some("http://example.com/page".to_string()),
        cookie: Some("sid=abc123; theme=dark".to_string()),
        user_agent: Some("Mozilla/5.0 (TestBrowser)".to_string()),
    };

    let _ = fetcher.probe(&url, &headers).await.expect("probe should succeed");

    let joined = seen.lock().unwrap().join("\n").to_lowercase();
    assert!(joined.contains("referer: http://example.com/page"), "referer manquant dans: {joined}");
    assert!(joined.contains("cookie: sid=abc123; theme=dark"), "cookie manquant dans: {joined}");
    assert!(joined.contains("user-agent: mozilla/5.0 (testbrowser)"), "user-agent manquant dans: {joined}");
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_without_headers_sends_no_cookie() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let port = start_recording_server(seen.clone());
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/file.bin");

    let _ = fetcher.probe(&url, &RequestHeaders::default()).await.expect("probe ok");

    let joined = seen.lock().unwrap().join("\n").to_lowercase();
    assert!(!joined.contains("cookie:"), "aucun cookie ne doit être envoyé: {joined}");
}
```

- [ ] **Step 2: Run the test to verify it fails (won't compile)**

Run: `cargo test -p download-core --test request_headers`
Expected: FAIL — `probe` takes 1 argument, not 2 (signature not changed yet).

- [ ] **Step 3: Change the `ByteFetcher` trait + `ReqwestFetcher` to apply headers**

In `src-tauri/crates/download-core/src/http.rs`, update the import and the trait:

```rust
use crate::model::{ProbeResult, RequestHeaders};
```

```rust
#[async_trait]
pub trait ByteFetcher: Send + Sync {
    /// Inspecte l'URL : taille, support des Range, nom de fichier suggéré.
    async fn probe(&self, url: &str, headers: &RequestHeaders) -> Result<ProbeResult, CoreError>;

    /// Récupère un flux d'octets. `range` = intervalle inclusif (start, end).
    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
        headers: &RequestHeaders,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError>;
}
```

Add a helper (free function at the bottom of `http.rs`, near `file_name_from_url`):

```rust
/// Pose Referer/Cookie/User-Agent sur la requête quand ils sont présents.
/// Le User-Agent par requête écrase celui par défaut du client.
fn apply_headers(mut req: reqwest::RequestBuilder, headers: &RequestHeaders) -> reqwest::RequestBuilder {
    if let Some(r) = &headers.referrer {
        req = req.header(reqwest::header::REFERER, r);
    }
    if let Some(c) = &headers.cookie {
        req = req.header(reqwest::header::COOKIE, c);
    }
    if let Some(ua) = &headers.user_agent {
        req = req.header(reqwest::header::USER_AGENT, ua);
    }
    req
}
```

Update `ReqwestFetcher::probe` to take + apply headers (replace **only** the request-build + `.send()` lines; keep the rest — `status`, `headers` clone, `total_bytes`, `file_name` — unchanged). Note: the existing line `let headers = resp.headers().clone();` later in `probe` intentionally shadows the new `headers` parameter — this is fine because the parameter is only used *before* `.send()`; do not rename it.

```rust
    async fn probe(&self, url: &str, headers: &RequestHeaders) -> Result<ProbeResult, CoreError> {
        let req = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0");
        let resp = apply_headers(req, headers)
            .send()
            .await
            .map_err(|e| CoreError::Network(describe_reqwest_error(&e)))?;
        // ... reste inchangé (status/headers/total_bytes/file_name) ...
```

Update `ReqwestFetcher::fetch_range` signature + request build:

```rust
    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
        headers: &RequestHeaders,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        use futures_util::StreamExt;

        let mut req = self.client.get(url);
        if let Some((start, end)) = range {
            req = req.header(reqwest::header::RANGE, format!("bytes={start}-{end}"));
        }
        let resp = apply_headers(req, headers)
            .send()
            .await
            .map_err(|e| CoreError::Network(describe_reqwest_error(&e)))?;
        // ... reste inchangé (error_for_status, bytes_stream) ...
```

- [ ] **Step 4: Update `FakeFetcher` to the new signature (ignores headers)**

In `src-tauri/crates/download-core/src/tests/fake_fetcher.rs`, update the import and both methods:

```rust
use crate::model::{ProbeResult, RequestHeaders};
```
```rust
    async fn probe(&self, _url: &str, _headers: &RequestHeaders) -> Result<ProbeResult, CoreError> {
        // ... corps inchangé ...
    }

    async fn fetch_range(
        &self,
        _url: &str,
        range: Option<(u64, u64)>,
        _headers: &RequestHeaders,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        // ... corps inchangé ...
    }
```

- [ ] **Step 5: Thread `&RequestHeaders` through `engine::download`**

In `src-tauri/crates/download-core/src/engine.rs`, add the import and the parameter:

```rust
use crate::model::{DownloadStatus, Progress, RequestHeaders};
```

Change the signature (add `headers` as the last parameter):

```rust
pub async fn download<F: ByteFetcher>(
    fetcher: &F,
    id: &str,
    url: &str,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
) -> Result<Outcome, CoreError> {
```

Update the probe call:
```rust
    let probe = fetcher.probe(url, headers).await?;
```

Update the single-stream `fetch_range` call:
```rust
        let mut stream = fetcher.fetch_range(url, None, headers).await?;
```

In the segmented branch, capture a copy of the reference alongside `fetcher_ref` (just after `let fetcher_ref = fetcher;`):
```rust
        let fetcher_ref = fetcher;
        let headers_ref = headers;
```
and update the segment `fetch_range` call:
```rust
            let mut stream = fetcher_ref
                .fetch_range(&url_owned, Some((resume_from, seg_end)), headers_ref)
                .await?;
```

- [ ] **Step 6: Update `engine.rs` test call sites**

In the `#[cfg(test)] mod tests` of `engine.rs`, add the import:
```rust
    use crate::model::RequestHeaders;
```
Append `, &RequestHeaders::default()` as the last argument to **every** `download(...)` call in this test module (there are 10: in `downloads_full_file_multisegment`, `falls_back_to_single_stream_when_no_ranges`, `emits_progress`, `resume_after_pause_completes_file` ×2, `resume_preserves_partial_bytes` ×2, `throttle_makes_engine_wait`, `resume_trusts_sidecar_when_probe_size_unknown` ×2). Example:
```rust
        let outcome = download(&fetcher, "id1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();
```

- [ ] **Step 7: Update the `real_http.rs` integration test call sites**

In `src-tauri/crates/download-core/tests/real_http.rs`, add the import and `, &RequestHeaders::default()` to both `download(...)` calls:
```rust
use download_core::model::{Progress, RequestHeaders};
```
```rust
    let outcome = download(&fetcher, "it1", &url, &dest, DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default())
        .await
        .expect("download should succeed");
```
(and the same addition to the `"it2"` call in `real_http_progress_reports_full_size`).

- [ ] **Step 8: Update the real caller `launch` in `commands.rs`**

In `src-tauri/src/commands.rs`, in `launch`, capture the download's headers before the engine task and pass them. After the existing clones near the top of `launch` (e.g. after `let url = download.url.clone();`), add:
```rust
    let headers = download.request_headers();
```
and update the `engine::download` call inside the `tauri::async_runtime::spawn(async move { ... })`:
```rust
        match engine::download(&fetcher, &id_for_dl, &url, &dest, handle, limiter, tx, &headers).await {
```
(Ensure `headers` is captured by the `async move` block — declaring it before the `spawn` is sufficient; the closure moves it in.)

- [ ] **Step 9: Run the header test + full core suite + app build**

Run: `cargo test -p download-core && cargo build -p tauri-app`
Expected: `request_headers` PASS (2 tests), all existing core tests PASS, `tauri-app` builds.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/crates/download-core/src/http.rs src-tauri/crates/download-core/src/tests/fake_fetcher.rs src-tauri/crates/download-core/src/engine.rs src-tauri/crates/download-core/tests/real_http.rs src-tauri/crates/download-core/tests/request_headers.rs src-tauri/src/commands.rs
git commit -m "feat(core): ByteFetcher carries RequestHeaders; ReqwestFetcher sends Cookie/Referer/User-Agent (TDD)"
```

---

## Task 3: Bridge protocol carries `cookie` + `user_agent`

**Files:**
- Modify: `src-tauri/src/bridge/protocol.rs`
- Test: inline `#[cfg(test)] mod tests` in `protocol.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/bridge/protocol.rs`, inside `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn capture_carries_cookie_and_user_agent() {
        let msg = parse_client_message(
            r#"{"type":"capture","url":"http://x/f.zip","referrer":"http://x/page","cookie":"sid=1","user_agent":"UA/1"}"#,
        );
        let (_authed, reply) = handle_message(true, "secret", msg);
        match reply {
            ServerReply::CaptureRequested { url, referrer, cookie, user_agent, .. } => {
                assert_eq!(url, "http://x/f.zip");
                assert_eq!(referrer.as_deref(), Some("http://x/page"));
                assert_eq!(cookie.as_deref(), Some("sid=1"));
                assert_eq!(user_agent.as_deref(), Some("UA/1"));
            }
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p tauri-app --lib bridge::protocol::tests::capture_carries_cookie_and_user_agent`
Expected: FAIL — `CaptureRequested` has no field `cookie`.

- [ ] **Step 3: Add the fields to the protocol types + parse + handle**

In `src-tauri/src/bridge/protocol.rs`:

`ClientMessage::Capture` — add the two fields:
```rust
    Capture {
        url: String,
        filename: Option<String>,
        referrer: Option<String>,
        cookie: Option<String>,
        user_agent: Option<String>,
    },
```

`ServerReply::CaptureRequested` — add the two fields:
```rust
    CaptureRequested {
        url: String,
        filename: Option<String>,
        referrer: Option<String>,
        cookie: Option<String>,
        user_agent: Option<String>,
    },
```

`RawMessage::Capture` — add the two `#[serde(default)]` fields:
```rust
    Capture {
        url: String,
        #[serde(default)]
        filename: Option<String>,
        #[serde(default)]
        referrer: Option<String>,
        #[serde(default)]
        cookie: Option<String>,
        #[serde(default)]
        user_agent: Option<String>,
    },
```

`parse_client_message` — map the new fields:
```rust
        Ok(RawMessage::Capture { url, filename, referrer, cookie, user_agent }) => {
            ClientMessage::Capture { url, filename, referrer, cookie, user_agent }
        }
```

`handle_message` — pass them through:
```rust
        ClientMessage::Capture { url, filename, referrer, cookie, user_agent } => {
            if authed {
                (authed, ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent })
            } else {
                (authed, ServerReply::NeedAuth)
            }
        }
```

- [ ] **Step 4: Run the protocol tests to verify they pass**

Run: `cargo test -p tauri-app --lib bridge::protocol`
Expected: PASS — all protocol tests (existing + the new one).

> Note: `server.rs` still matches `ServerReply::CaptureRequested { url, .. }`, so it keeps compiling; Task 5 wires the new fields.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bridge/protocol.rs
git commit -m "feat(bridge): protocol carries cookie + user_agent on capture (TDD)"
```

---

## Task 4: `add_url` accepts `RequestHeaders`; UI add passes empty; TS type

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/types.ts`

- [ ] **Step 1: Change `add_url` to accept headers and build the Download with them**

In `src-tauri/src/commands.rs`, update the import to include `RequestHeaders`:
```rust
use download_core::model::{Download, DownloadStatus, Priority, Progress, Settings, Category, CategoryDirs, RequestHeaders};
```

Change `add_url`'s signature and the Download literal (replace the placeholder Nones from Task 1):
```rust
pub(crate) async fn add_url(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders) -> Result<Download, String> {
```
```rust
        priority: Priority::Normal,
        category,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
    };
```

- [ ] **Step 2: Update the `add_download` Tauri command to pass empty headers**

```rust
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    add_url(&app, &args.url, &args.dest_dir, RequestHeaders::default()).await
}
```

- [ ] **Step 3: Add the optional fields to the TS `Download` type**

In `src/types.ts`, in the `Download` interface, add (kept optional so existing literals/tests don't break):
```ts
  referrer?: string | null;
  cookie?: string | null;
  user_agent?: string | null;
```

- [ ] **Step 4: Build to verify everything compiles**

Run: `cargo build -p tauri-app && npx tsc --noEmit`
Expected: Rust builds; TypeScript has no errors.

> `bridge/server.rs` still calls `add_url(&app, &url, "")` with 3 args at this point — it will FAIL to build until Task 5. If you are running tasks strictly one-by-one, expect `cargo build` to error here on the `server.rs` call site; that is fixed in Task 5. To keep this task self-contained, do Step 4's `cargo build` AFTER Task 5, or temporarily pass `RequestHeaders::default()` in `server.rs` now. **Recommended:** treat Tasks 4 and 5 as a pair and run the build at the end of Task 5.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src/types.ts
git commit -m "feat(app): add_url accepts RequestHeaders; UI add passes empty; TS Download type"
```

---

## Task 5: Bridge passes captured headers to `add_url`

**Files:**
- Modify: `src-tauri/src/bridge/server.rs`

- [ ] **Step 1: Pass the captured headers into the pipeline**

In `src-tauri/src/bridge/server.rs`, add the import:
```rust
use download_core::model::RequestHeaders;
```

Update the `CaptureRequested` arm to destructure the headers and build `RequestHeaders` (the `..` ignores `filename`):
```rust
            ServerReply::CaptureRequested { url, referrer, cookie, user_agent, .. } => {
                // Capture via le pipeline d'ajout existant, en transmettant les en-têtes du navigateur.
                let headers = RequestHeaders { referrer, cookie, user_agent };
                match commands::add_url(&app, &url, "", headers).await {
                    Ok(d) => {
                        let _ = app.emit("download://added", &d);
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.unminimize();
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                        serde_json::json!({ "type": "ack", "id": d.id }).to_string()
                    }
                    Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                }
            }
```

- [ ] **Step 2: Build + run the full Rust suite**

Run: `cargo build -p tauri-app && cargo test`
Expected: builds clean; all tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/bridge/server.rs
git commit -m "feat(bridge): forward captured cookie/referer/user-agent to add_url"
```

---

## Task 6: Extension reads cookies and sends enriched captures

**Files:**
- Modify: `extension/manifest.json`
- Modify: `extension/buildCapture.js`
- Modify: `extension/buildCapture.test.js`
- Modify: `extension/background.js`

- [ ] **Step 1: Write the failing `buildCapture` extras tests**

In `extension/buildCapture.test.js`, add (keep the existing 3 tests — they call `buildCapture(url)` with no extras and must still pass):

```js
  it("includes referrer, cookie and user_agent when provided", () => {
    expect(
      buildCapture("http://x/f.zip", {
        referrer: "http://x/page",
        cookie: "sid=1; a=2",
        user_agent: "UA/1",
      }),
    ).toEqual({
      type: "capture",
      url: "http://x/f.zip",
      referrer: "http://x/page",
      cookie: "sid=1; a=2",
      user_agent: "UA/1",
    });
  });

  it("omits empty / missing extras", () => {
    expect(buildCapture("http://x/f.zip", { referrer: "", cookie: undefined })).toEqual({
      type: "capture",
      url: "http://x/f.zip",
    });
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: FAIL — the new tests fail (extras not yet supported).

- [ ] **Step 3: Implement `buildCapture(url, extras)`**

Replace `extension/buildCapture.js`:

```js
// Logique pure et testable : construit le message de capture WS à partir d'une URL
// et d'en-têtes optionnels (referrer / cookie / user_agent). Les champs vides sont omis.
export function buildCapture(url, extras = {}) {
  const trimmed = (url ?? "").trim();
  if (!trimmed) {
    throw new Error("URL vide");
  }
  const msg = { type: "capture", url: trimmed };
  for (const key of ["referrer", "cookie", "user_agent"]) {
    const value = extras[key];
    if (value) {
      msg[key] = value;
    }
  }
  return msg;
}
```

- [ ] **Step 4: Run to verify pass**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: PASS — all 5 tests.

- [ ] **Step 5: Add the `cookies` permission + `<all_urls>` host permission**

In `extension/manifest.json`:
```json
  "permissions": ["contextMenus", "storage", "activeTab", "alarms", "cookies"],
  "host_permissions": ["ws://127.0.0.1/*", "<all_urls>"],
```

- [ ] **Step 6: Gather headers and enrich captures in `background.js`**

In `extension/background.js`:

(a) Add a header-gathering helper (place it near `buildCapture` usage, after the `pending`/`flushPending` block):
```js
// Collecte les en-têtes navigateur pour une URL capturée : cookies (incl. httpOnly via
// l'API chrome.cookies), referer (page d'origine), user-agent.
async function gatherHeaders(url, referrer) {
  let cookie = "";
  try {
    const cookies = await chrome.cookies.getAll({ url });
    cookie = cookies.map((c) => `${c.name}=${c.value}`).join("; ");
  } catch (_e) {
    // pas d'accès cookies pour cette URL : on envoie sans
  }
  return {
    referrer: referrer || undefined,
    cookie: cookie || undefined,
    user_agent: navigator.userAgent,
  };
}
```

(b) Replace `flushPending` (now gathers fresh headers per queued item):
```js
async function flushPending() {
  if (!socket || socket.readyState !== WebSocket.OPEN || !connected) return;
  const now = Date.now();
  const items = pending;
  pending = [];
  for (const it of items) {
    if (now - it.ts > PENDING_TTL_MS) continue;
    try {
      const headers = await gatherHeaders(it.url, it.referrer);
      socket.send(JSON.stringify(buildCapture(it.url, headers)));
    } catch (_e) {
      // URL invalide : ignorée
    }
  }
}
```

(c) Replace `sendCapture` (takes a `referrer`, gathers headers, queues `{url, referrer, ts}`):
```js
async function sendCapture(url, referrer) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, headers)));
      return true;
    } catch (_e) {
      return false; // URL invalide
    }
  }
  pending.push({ url, referrer, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```

(d) Update the context-menu handler to pass the page as referer:
```js
chrome.contextMenus.onClicked.addListener((info) => {
  const url = info.linkUrl || info.srcUrl || info.pageUrl;
  if (url) sendCapture(url, info.pageUrl);
});
```

(e) Update the popup `send` message handler to resolve the active tab as referer:
```js
  if (req.cmd === "send" && req.url) {
    (async () => {
      let referrer;
      try {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        referrer = tab && tab.url;
      } catch (_e) {
        // pas d'onglet : referer absent
      }
      const ok = await sendCapture(req.url, referrer);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
```

- [ ] **Step 7: Verify syntax + run the full Vitest suite**

Run: `node --check extension/background.js && npm test`
Expected: `background.js` syntax OK; all Vitest tests pass (existing + 2 new buildCapture).

- [ ] **Step 8: Commit**

```bash
git add extension/manifest.json extension/buildCapture.js extension/buildCapture.test.js extension/background.js
git commit -m "feat(extension): forward cookies/referer/user-agent on capture (cookies + <all_urls>)"
```

---

## Task 7: Verification (suites + manual smoke test)

**Files:** none (verification only).

- [ ] **Step 1: Full Rust suite**

Run: `cargo test`
Expected: all PASS, including `download-core` `upsert_then_list_roundtrips_headers`, `request_headers` (2), and `tauri-app` `bridge::protocol` (incl. the new capture-headers test).

- [ ] **Step 2: Frontend + extension suite + typecheck + build**

Run: `npm test && npm run build`
Expected: all Vitest tests pass (incl. 5 `buildCapture`); `tsc` + vite build clean.

- [ ] **Step 3: Tauri build**

Run: `cargo build -p tauri-app`
Expected: clean build.

- [ ] **Step 4: Manual smoke test (run by a human; record outcomes)**

1. `npm run tauri dev`. In Réglages → 🧩, copy the token. Reload the extension in `edge://extensions` (the new `cookies` + `<all_urls>` permissions require a reload; accept them). Badge turns green.
2. **Authenticated link:** log into a site that gates a file behind login (session cookie present in the browser). Right-click the file link → "Télécharger avec FlashGet". Confirm the download **succeeds** in the app (it would 401/403 or get an HTML login page without cookies).
3. **Public link (regression):** right-click a public direct file (e.g. a `.zip` on a CDN) → confirm it still downloads correctly.
4. **Referer/UA:** capture a link on a host using hotlink protection (referer check) → confirm it succeeds.
5. **Resume across restart:** pause an authenticated download, quit + relaunch the app, resume → confirm it continues (headers were persisted).

- [ ] **Step 5: Final commit (only if verification fixes were needed)**

```bash
git add -A
git commit -m "test(3b-1): verification pass — auth downloads work; suites green"
```

---

## Notes for the executor

- **Never run on `main`.** Create branch `phase-3b1-cookies-referer` first (`git checkout -b phase-3b1-cookies-referer`).
- **Tasks 4 and 5 are a pair** for compilability: `add_url`'s new 4th argument makes `server.rs` fail to build until Task 5 updates the call site. Run the full `cargo build`/`cargo test` at the end of Task 5.
- **Signature-change blast radius (Task 2):** changing `ByteFetcher::probe`/`fetch_range` and `engine::download` breaks every impl and call site at once — the crate won't compile until all are updated. The compiler lists each site; the known ones are: `ReqwestFetcher` (http.rs), `FakeFetcher` (fake_fetcher.rs), `engine::download` internals, 10 calls in engine.rs tests, 2 calls in real_http.rs, 1 call in commands.rs `launch`.
- **Privacy:** cookies are persisted in plaintext in the local SQLite and serialized to the app's own renderer — accepted per the spec (§5). Do not log cookie values.
- **Double review** the `ByteFetcher`/engine threading (Task 2) and the storage migration (Task 1) — the load-bearing pieces.
- On merge: `git checkout main && git merge --no-ff phase-3b1-cookies-referer`.
