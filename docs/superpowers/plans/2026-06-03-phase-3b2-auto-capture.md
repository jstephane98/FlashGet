# Phase 3b-2 — Automatic Download Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the extension automatically intercept the browser's downloads (whitelisted file types) and redirect them to FlashGet — reusing 3b-1's cookie forwarding and forwarding the browser's resolved filename.

**Architecture:** A new pure `extension/capture-rules.js` decides whether a download should be captured (`shouldCapture`); the `background.js` `chrome.downloads.onDeterminingFilename` listener gates on `connected`, cancels the browser download, and reuses `sendCapture` (now carrying an optional filename). A small pure Rust `choose_file_name` helper lets `add_url` use the browser's suggested filename when present.

**Tech Stack:** Chromium MV3 (vanilla JS) + Vitest; Rust (download-core) + Tauri 2.

---

## File Structure

**Extension (new):**
- `extension/capture-rules.js` — pure: `extensionOf`, `parseCaptureTypes`, `shouldCapture`, `DEFAULT_CAPTURE_TYPES`.
- `extension/capture-rules.test.js` — Vitest.

**Extension (modified):**
- `extension/buildCapture.js` / `buildCapture.test.js` — allow `filename` extra.
- `extension/background.js` — `onDeterminingFilename` listener, `getConfig` reads auto-capture settings, `sendCapture(url, referrer, filename)`, enriched `pending`, `basename`.
- `extension/manifest.json` — add `downloads` permission.
- `extension/options.html` / `options.js` — auto-capture checkbox + captured-types field + persistence.

**Rust (new):**
- `src-tauri/crates/download-core/src/naming.rs` — pure `choose_file_name`.

**Rust (modified):**
- `src-tauri/crates/download-core/src/lib.rs` — `pub mod naming;`.
- `src-tauri/src/commands.rs` — `add_url` gains `suggested_name`; `add_download` passes `None`.
- `src-tauri/src/bridge/server.rs` — pass captured `filename` to `add_url`.

---

## Task 1: Pure capture rules (`capture-rules.js`)

**Files:**
- Create: `extension/capture-rules.js`
- Test: `extension/capture-rules.test.js`

- [ ] **Step 1: Write the failing tests**

Create `extension/capture-rules.test.js`:

```js
import { describe, it, expect } from "vitest";
import { extensionOf, parseCaptureTypes, shouldCapture, DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";

describe("extensionOf", () => {
  it("extracts the lowercase last extension", () => {
    expect(extensionOf("video.MP4")).toBe("mp4");
    expect(extensionOf("backup.tar.gz")).toBe("gz");
  });
  it("ignores url query/fragment and path", () => {
    expect(extensionOf("https://h/path/file.zip?x=1#y")).toBe("zip");
  });
  it("returns empty when there is no extension", () => {
    expect(extensionOf("https://h/download?id=123")).toBe("");
    expect(extensionOf("noext")).toBe("");
    expect(extensionOf("")).toBe("");
  });
});

describe("parseCaptureTypes", () => {
  it("splits on commas/spaces/newlines, lowercases, strips dots, dedupes", () => {
    expect(parseCaptureTypes("zip, exe,mp4")).toEqual(["zip", "exe", "mp4"]);
    expect(parseCaptureTypes(".pdf .PDF\nrar")).toEqual(["pdf", "rar"]);
    expect(parseCaptureTypes("")).toEqual([]);
  });
  it("the default list is non-empty and includes common types", () => {
    const t = parseCaptureTypes(DEFAULT_CAPTURE_TYPES);
    expect(t).toContain("zip");
    expect(t).toContain("exe");
    expect(t).toContain("mp4");
  });
});

describe("shouldCapture", () => {
  const settings = { autoCapture: true, types: ["zip", "mp4"] };
  it("captures a whitelisted http(s) type", () => {
    expect(shouldCapture({ url: "https://h/f.zip" }, settings)).toBe(true);
  });
  it("uses the filename extension when the url has none", () => {
    expect(shouldCapture({ url: "https://h/download?id=1", filename: "movie.mp4" }, settings)).toBe(true);
  });
  it("rejects non-whitelisted types", () => {
    expect(shouldCapture({ url: "https://h/f.txt" }, settings)).toBe(false);
  });
  it("rejects when auto-capture is off", () => {
    expect(shouldCapture({ url: "https://h/f.zip" }, { autoCapture: false, types: ["zip"] })).toBe(false);
  });
  it("rejects blob/data/non-http schemes", () => {
    expect(shouldCapture({ url: "blob:https://h/abc" }, settings)).toBe(false);
    expect(shouldCapture({ url: "data:text/plain;base64,AAAA" }, settings)).toBe(false);
    expect(shouldCapture({ url: "file:///c:/x.zip" }, settings)).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npx vitest run extension/capture-rules.test.js`
Expected: FAIL — cannot resolve `./capture-rules.js`.

- [ ] **Step 3: Implement the module**

Create `extension/capture-rules.js`:

```js
// Logique pure et testable des règles de capture automatique (aucun chrome.*).

// Liste blanche par défaut (basée sur le catégoriseur du cœur + types IDM courants).
export const DEFAULT_CAPTURE_TYPES =
  "7z, rar, zip, tar, gz, bz2, xz, zst, iso, exe, msi, dmg, pkg, deb, rpm, apk, appimage, " +
  "mp4, mkv, avi, mov, webm, flv, wmv, m4v, mpg, mpeg, mp3, flac, wav, aac, ogg, m4a, wma, opus, " +
  "pdf, doc, docx, xls, xlsx, ppt, pptx, epub";

// Dernière extension en minuscules d'un nom de fichier ou d'une URL (ignore query/fragment
// et le chemin). Sans extension ⇒ "".
export function extensionOf(nameOrUrl) {
  let s = String(nameOrUrl ?? "");
  s = s.split(/[?#]/)[0]; // retirer query/fragment (URL)
  s = s.split(/[/\\]/).pop() || ""; // dernier segment de chemin
  const dot = s.lastIndexOf(".");
  if (dot <= 0 || dot === s.length - 1) return "";
  return s.slice(dot + 1).toLowerCase();
}

// Parse la liste de types : séparée par virgules/espaces/sauts de ligne, minuscules,
// sans point de tête, dédoublonnée, sans vides.
export function parseCaptureTypes(text) {
  const set = new Set();
  for (const raw of String(text ?? "").split(/[\s,]+/)) {
    const t = raw.trim().toLowerCase().replace(/^\.+/, "");
    if (t) set.add(t);
  }
  return [...set];
}

// Décide si un download doit être capturé. PURE — la garde "connected" est gérée par l'appelant.
// item = { url, filename? }, settings = { autoCapture: bool, types: string[] }
export function shouldCapture(item, settings) {
  if (!settings || !settings.autoCapture) return false;
  const url = String((item && item.url) ?? "");
  if (!/^https?:\/\//i.test(url)) return false; // exclut blob:/data:/file:/chrome:
  const ext = extensionOf(item && item.filename) || extensionOf(url);
  if (!ext) return false;
  return settings.types.includes(ext);
}
```

- [ ] **Step 4: Run to verify pass**

Run: `npx vitest run extension/capture-rules.test.js`
Expected: PASS — all tests.

- [ ] **Step 5: Commit**

```bash
git add extension/capture-rules.js extension/capture-rules.test.js
git commit -m "feat(extension): pure capture rules (extensionOf/parseCaptureTypes/shouldCapture) — TDD"
```

---

## Task 2: `buildCapture` allows `filename`

**Files:**
- Modify: `extension/buildCapture.js`
- Modify: `extension/buildCapture.test.js`

- [ ] **Step 1: Write the failing test**

In `extension/buildCapture.test.js`, add (inside the existing `describe("buildCapture", …)`):

```js
  it("includes filename when provided", () => {
    expect(buildCapture("http://x/dl?id=1", { filename: "movie.mp4" })).toEqual({
      type: "capture",
      url: "http://x/dl?id=1",
      filename: "movie.mp4",
    });
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: FAIL — the new test fails (`filename` not copied into the message).

- [ ] **Step 3: Add `filename` to the allowed keys**

In `extension/buildCapture.js`, change the keys loop to include `filename`:

```js
  for (const key of ["referrer", "cookie", "user_agent", "filename"]) {
```

- [ ] **Step 4: Run to verify pass**

Run: `npx vitest run extension/buildCapture.test.js`
Expected: PASS — all buildCapture tests.

- [ ] **Step 5: Commit**

```bash
git add extension/buildCapture.js extension/buildCapture.test.js
git commit -m "feat(extension): buildCapture forwards optional filename (TDD)"
```

---

## Task 3: Backend — `choose_file_name` + forward filename through `add_url`

**Files:**
- Create: `src-tauri/crates/download-core/src/naming.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/bridge/server.rs`

- [ ] **Step 1: Write the failing test (pure helper)**

Create `src-tauri/crates/download-core/src/naming.rs`:

```rust
//! Choix du nom de fichier final pour un téléchargement.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_suggested_when_present() {
        assert_eq!(choose_file_name(Some("video.mp4"), Some("download.bin")), "video.mp4");
    }

    #[test]
    fn takes_basename_of_suggested() {
        assert_eq!(choose_file_name(Some("sub/dir/file.zip"), None), "file.zip");
        assert_eq!(choose_file_name(Some("a\\b\\file.iso"), None), "file.iso");
    }

    #[test]
    fn falls_back_to_probe_when_suggested_empty_or_blank() {
        assert_eq!(choose_file_name(Some(""), Some("fromprobe.iso")), "fromprobe.iso");
        assert_eq!(choose_file_name(Some("   "), Some("x.bin")), "x.bin");
        assert_eq!(choose_file_name(None, Some("p.zip")), "p.zip");
    }

    #[test]
    fn falls_back_to_default_when_nothing_usable() {
        assert_eq!(choose_file_name(None, None), "download.bin");
        assert_eq!(choose_file_name(Some(""), Some("")), "download.bin");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p download-core --lib naming`
Expected: FAIL — `choose_file_name` not found.

- [ ] **Step 3: Implement `choose_file_name`**

In `src-tauri/crates/download-core/src/naming.rs`, add ABOVE the `#[cfg(test)]` block:

```rust
/// Choisit le nom de fichier final : nom suggéré (basename, non vide) sinon nom du probe
/// (non vide) sinon `download.bin`.
pub fn choose_file_name(suggested: Option<&str>, probe: Option<&str>) -> String {
    if let Some(name) = suggested.map(basename).filter(|s| !s.is_empty()) {
        return name;
    }
    if let Some(name) = probe.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        return name;
    }
    "download.bin".to_string()
}

/// Dernier segment de chemin (sépareurs `/` ou `\`), espaces retirés.
fn basename(s: &str) -> String {
    s.rsplit(['/', '\\']).next().unwrap_or(s).trim().to_string()
}
```

Register the module in `src-tauri/crates/download-core/src/lib.rs` by adding `pub mod naming;` next to the other `pub mod …;` declarations.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p download-core --lib naming`
Expected: PASS — 4 tests.

- [ ] **Step 5: Thread `suggested_name` through `add_url` + callers**

In `src-tauri/src/commands.rs`:

Change `add_url`'s signature (add a 5th param):
```rust
pub(crate) async fn add_url(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<Download, String> {
```

Replace the `file_name` line (currently `let file_name = probe.file_name.clone().unwrap_or_else(|| "download.bin".to_string());`) with:
```rust
    let file_name = download_core::naming::choose_file_name(suggested_name.as_deref(), probe.file_name.as_deref());
```

Update the `add_download` command to pass `None`:
```rust
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    add_url(&app, &args.url, &args.dest_dir, RequestHeaders::default(), None).await
}
```

In `src-tauri/src/bridge/server.rs`, update the `CaptureRequested` arm to bind `filename` and pass it (the variant has exactly `url, filename, referrer, cookie, user_agent`, so no `..` needed):
```rust
            ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent } => {
                // Capture via le pipeline d'ajout existant, en transmettant les en-têtes du navigateur
                // et le nom de fichier suggéré par le navigateur.
                let headers = RequestHeaders { referrer, cookie, user_agent };
                match commands::add_url(&app, &url, "", headers, filename).await {
                    Ok(d) => {
                        // Notifier le frontend pour qu'il rafraîchisse sa liste : sans cet
                        // événement la capture n'apparaît qu'à la complétion (l'UI n'insère
                        // pas les downloads inconnus reçus via `download://progress`).
                        let _ = app.emit("download://added", &d);
                        // Faire surgir la fenêtre de l'app au premier plan (UX type IDM).
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

- [ ] **Step 6: Build + full Rust suite**

Run: `cargo build -p tauri-app && cargo test`
Expected: builds clean; all tests pass (incl. `naming` 4).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/download-core/src/naming.rs src-tauri/crates/download-core/src/lib.rs src-tauri/src/commands.rs src-tauri/src/bridge/server.rs
git commit -m "feat(core): choose_file_name + add_url forwards suggested filename from capture (TDD)"
```

---

## Task 4: Extension — intercept downloads + carry filename

**Files:**
- Modify: `extension/manifest.json`
- Modify: `extension/background.js`

- [ ] **Step 1: Add the `downloads` permission**

In `extension/manifest.json`, add `"downloads"` to `permissions`:
```json
  "permissions": ["contextMenus", "storage", "activeTab", "alarms", "cookies", "downloads"],
```

- [ ] **Step 2: Import the capture rules**

In `extension/background.js`, add the import under the existing `buildCapture` import:
```js
import { buildCapture } from "./buildCapture.js";
import { shouldCapture, parseCaptureTypes, DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";
```

- [ ] **Step 3: Read auto-capture settings in `getConfig`**

Replace `getConfig` with:
```js
async function getConfig() {
  const {
    token = "",
    port = DEFAULT_PORT,
    autoCapture = true,
    captureTypes = DEFAULT_CAPTURE_TYPES,
  } = await chrome.storage.local.get(["token", "port", "autoCapture", "captureTypes"]);
  return {
    token,
    port: Number(port) || DEFAULT_PORT,
    autoCapture: autoCapture !== false,
    types: parseCaptureTypes(captureTypes),
  };
}
```

- [ ] **Step 4: Add a `basename` helper + carry `filename` through `sendCapture`/`flushPending`**

In `extension/background.js`, add a `basename` helper near `gatherHeaders`:
```js
// Dernier segment de chemin d'un nom de fichier suggéré par le navigateur.
function basename(name) {
  return String(name || "").split(/[/\\]/).pop() || "";
}
```

Replace `flushPending`'s send line so it carries the queued item's filename:
```js
      const headers = await gatherHeaders(it.url, it.referrer);
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename })));
```

Replace `sendCapture` with a 3-arg version (the optional `filename` is forwarded; existing 2-arg callers pass `undefined`, which `buildCapture` omits):
```js
async function sendCapture(url, referrer, filename) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, filename })));
      return true;
    } catch (_e) {
      return false; // URL invalide
    }
  }
  pending.push({ url, referrer, filename, ts: Date.now() });
  if (pending.length > 20) pending.shift(); // borne anti-accumulation
  connect();
  return true; // acceptée : sera envoyée dès la (re)connexion
}
```
Also update the `pending` comment near the top of the file:
```js
let pending = []; // captures en attente d'une connexion authentifiée : [{ url, referrer, filename, ts }]
```

- [ ] **Step 5: Add the auto-capture listener**

In `extension/background.js`, add this listener (place it after the `chrome.contextMenus.onClicked` listener):
```js
// Capture automatique : intercepte les téléchargements du navigateur (types listés) et
// les redirige vers FlashGet. N'agit QUE si l'app est connectée (sinon on laisse le
// navigateur télécharger pour ne rien perdre).
chrome.downloads.onDeterminingFilename.addListener((item) => {
  (async () => {
    if (!connected) return; // app injoignable : ne pas intercepter
    const cfg = await getConfig();
    const url = item.finalUrl || item.url;
    if (!shouldCapture({ url, filename: item.filename }, { autoCapture: cfg.autoCapture, types: cfg.types })) {
      return; // type non listé / schéma non récupérable : laisser le navigateur
    }
    try {
      await chrome.downloads.cancel(item.id);
      await chrome.downloads.erase({ id: item.id });
    } catch (_e) {
      return; // annulation impossible : ne pas double-télécharger
    }
    sendCapture(url, item.referrer, basename(item.filename));
  })();
  // Listener synchrone : on n'appelle pas suggest() (le navigateur garde son nom par défaut
  // pour les downloads non capturés ; les capturés sont annulés ci-dessus).
});
```

- [ ] **Step 6: Verify syntax + run the suite**

Run: `node --check extension/background.js && npm test`
Expected: syntax OK; all Vitest tests pass (existing + capture-rules + the buildCapture filename test).

- [ ] **Step 7: Commit**

```bash
git add extension/manifest.json extension/background.js
git commit -m "feat(extension): auto-capture browser downloads via onDeterminingFilename (connected-gated, filename forwarded)"
```

---

## Task 5: Options UI — auto-capture toggle + captured types

**Files:**
- Modify: `extension/options.html`
- Modify: `extension/options.js`

- [ ] **Step 1: Add the checkbox + types field to the options page**

In `extension/options.html`, add `textarea` to the width rule and add the two controls before the Save button block. Change the style rule:
```html
      input { width: 100%; box-sizing: border-box; padding: 6px; }
      textarea { width: 100%; box-sizing: border-box; padding: 6px; font-family: inherit; }
```
And insert, after the `port` input and before the `<div>` holding the Save button:
```html
    <label style="display: flex; align-items: center; gap: 6px; margin-top: 14px;">
      <input id="autoCapture" type="checkbox" style="width: auto;" />
      Capture automatique des téléchargements
    </label>
    <label for="captureTypes">Types capturés (extensions, séparées par des virgules)</label>
    <textarea id="captureTypes" rows="4"></textarea>
```

- [ ] **Step 2: Load + save the new settings**

Replace the contents of `extension/options.js` with:
```js
import { DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";

const tokenInput = document.getElementById("token");
const portInput = document.getElementById("port");
const autoCaptureInput = document.getElementById("autoCapture");
const captureTypesInput = document.getElementById("captureTypes");
const saved = document.getElementById("saved");

async function load() {
  const {
    token = "",
    port = 15151,
    autoCapture = true,
    captureTypes = DEFAULT_CAPTURE_TYPES,
  } = await chrome.storage.local.get(["token", "port", "autoCapture", "captureTypes"]);
  tokenInput.value = token;
  portInput.value = port;
  autoCaptureInput.checked = autoCapture !== false;
  captureTypesInput.value = captureTypes;
}

document.getElementById("save").addEventListener("click", async () => {
  await chrome.storage.local.set({
    token: tokenInput.value.trim(),
    port: Number(portInput.value) || 15151,
    autoCapture: autoCaptureInput.checked,
    captureTypes: captureTypesInput.value,
  });
  saved.textContent = "Enregistré ✓";
  setTimeout(() => (saved.textContent = ""), 1500);
  // Forcer une reconnexion avec le nouveau jeton/port.
  chrome.runtime.sendMessage({ cmd: "reconnect" });
});

load();
```

- [ ] **Step 3: Verify the suite still passes**

Run: `npm test`
Expected: all Vitest tests pass (options.js is browser code, not unit-tested; this confirms nothing else broke).

- [ ] **Step 4: Commit**

```bash
git add extension/options.html extension/options.js
git commit -m "feat(extension): options — auto-capture toggle + captured-types list (default on)"
```

---

## Task 6: Verification (suites + manual smoke test)

**Files:** none (verification only).

- [ ] **Step 1: Full Rust suite**

Run: `cargo test`
Expected: all PASS, incl. `download-core` `naming` (4).

- [ ] **Step 2: Vitest + frontend build**

Run: `npm test && npm run build`
Expected: all Vitest tests pass (incl. `capture-rules` + the new `buildCapture` filename test); `tsc` + vite build clean.

- [ ] **Step 3: Tauri build**

Run: `cargo build -p tauri-app`
Expected: clean build.

- [ ] **Step 4: Manual smoke test (run by a human; record outcomes)**

1. `npm run tauri dev`. Reload the extension in `edge://extensions` (the new `downloads` permission requires a reload; accept it). Confirm the badge is green (connected). Open the extension **Options** → confirm "Capture automatique" is **checked** and the types list is prefilled.
2. **Whitelisted type:** click a direct link to a `.zip` (or any listed type). ✅ Expected: the browser does NOT download it; it appears in FlashGet's queue **with the correct filename**, in the right category folder.
3. **Non-whitelisted type:** click a link to a `.txt` (not in the list). ✅ Expected: the **browser** downloads it normally (no interception).
4. **blob:** trigger a client-side "export" (a `blob:` download). ✅ Expected: the browser handles it (FlashGet does not intercept).
5. **App closed:** quit FlashGet, click a `.zip` link. ✅ Expected: the **browser** downloads it (no interception when disconnected).
6. **Toggle off:** in Options, uncheck "Capture automatique", Save. Click a `.zip` link. ✅ Expected: the browser downloads it (auto-capture disabled). Re-check to restore.
7. **Authenticated (regression with 3b-1):** while logged into a site, auto-capture a login-gated file → succeeds (cookies still forwarded).

- [ ] **Step 5: Final commit (only if verification fixes were needed)**

```bash
git add -A
git commit -m "test(3b-2): verification pass — auto-capture works; suites green"
```

---

## Notes for the executor

- **Never run on `main`.** Create branch `phase-3b2-auto-capture` first (`git checkout -b phase-3b2-auto-capture`).
- **`add_url` signature change (Task 3)** adds a 5th param; it breaks `server.rs` and `add_download` — both are updated in the same task, so build at the end of Task 3.
- **`onDeterminingFilename` gotcha:** the listener is synchronous (it does not call `suggest()`); the capture decision + cancel happen in a fire-and-forget async IIFE. For non-captured downloads we simply don't interfere. If, during the smoke test, downloads aren't intercepted at all, fall back to `chrome.downloads.onCreated` (note: `item.filename` may be empty there, so the extension match relies on the URL) — but try `onDeterminingFilename` first.
- **Connected-gate is load-bearing:** never cancel a browser download when `connected` is false (the user would lose it).
- **Reload the extension** in `edge://extensions` after any `extension/` change (and accept the new `downloads` permission once).
- **Double review** the `onDeterminingFilename` listener (cancel/erase/connected ordering) and `choose_file_name` (the load-bearing pieces).
- On merge: `git checkout main && git merge --no-ff phase-3b2-auto-capture`.
