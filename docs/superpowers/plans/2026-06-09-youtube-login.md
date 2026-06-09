# Connexion YouTube intégrée — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps en cases à cocher.

**Goal:** Permettre une connexion YouTube dans l'app (webview persistant) qui exporte un `cookies.txt` Netscape passé à yt-dlp (`--cookies`), avec repli `--cookies-from-browser`, pour que coller un lien YouTube suffise.

**Architecture:** Cœur pur testable (`download-core/cookies.rs` : format Netscape + sélection de source). Glue Tauri (`youtube.rs` : fenêtre de login, lecture du magasin WebView2 via `cookies_for_url`, écriture du fichier, commandes). Intégration dans le runner yt-dlp. Section « Compte YouTube » dans les Réglages.

**Tech Stack :** Rust (download-core, Tauri 2.11 webview cookies), React/TS, yt-dlp sidecar.

**Branche :** `phase-youtube-login` (depuis `main`). Garde `git checkout phase-youtube-login` avant chaque commit.

**Référence spec :** `docs/superpowers/specs/2026-06-09-youtube-login-design.md`

---

## Task 1 : Cœur — `download-core/src/cookies.rs` (Netscape + sélection de source)

**Files:**
- Create: `src-tauri/crates/download-core/src/cookies.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs` (ajouter `pub mod cookies;`)

- [ ] **Step 1 : Déclarer le module + écrire le fichier (stub + tests)**

Dans `lib.rs`, ajouter après `pub mod batch;` : `pub mod cookies;`.

Créer `src-tauri/crates/download-core/src/cookies.rs` avec le stub (fonctions `unimplemented`/vides pour faire échouer) et les tests :
```rust
//! Cookies pour yt-dlp : sérialisation Netscape + choix de la source (fichier / navigateur).

/// Un cookie prêt à sérialiser au format Netscape.
#[derive(Debug, Clone, PartialEq)]
pub struct NetscapeCookie {
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<i64>, // epoch (s) ; None = cookie de session
    pub name: String,
    pub value: String,
}

/// Sérialise des cookies au format Netscape (lu par yt-dlp/curl).
/// httpOnly → domaine préfixé `#HttpOnly_` (convention reconnue par yt-dlp).
pub fn to_netscape(cookies: &[NetscapeCookie]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in cookies {
        let include_sub = if c.domain.starts_with('.') { "TRUE" } else { "FALSE" };
        let secure = if c.secure { "TRUE" } else { "FALSE" };
        let expiry = c.expires.unwrap_or(0);
        let domain = if c.http_only { format!("#HttpOnly_{}", c.domain) } else { c.domain.clone() };
        out.push_str(&format!(
            "{domain}\t{include_sub}\t{}\t{secure}\t{expiry}\t{}\t{}\n",
            c.path, c.name, c.value
        ));
    }
    out
}

/// Vrai si l'hôte de `url` appartient à YouTube/Google (on n'attache les cookies que là).
pub fn cookies_apply_to(url: &str) -> bool {
    let host = url
        .split("://").nth(1).unwrap_or(url)
        .split('/').next().unwrap_or("")
        .rsplit('@').next().unwrap_or("")
        .split(':').next().unwrap_or("")
        .to_ascii_lowercase();
    const DOMAINS: [&str; 5] = ["youtube.com", "youtu.be", "googlevideo.com", "google.com", "ytimg.com"];
    DOMAINS.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Source de cookies choisie pour un run yt-dlp.
#[derive(Debug, Clone, PartialEq)]
pub enum CookieSource {
    File,
    Browser(String),
    None,
}

/// Décide la source : fichier cookies.txt si présent, sinon repli navigateur, sinon rien.
/// N'attache rien hors domaines YouTube/Google.
pub fn choose_cookie_source(url: &str, has_file: bool, browser_fallback: Option<&str>) -> CookieSource {
    if !cookies_apply_to(url) {
        return CookieSource::None;
    }
    if has_file {
        return CookieSource::File;
    }
    match browser_fallback {
        Some(b) if !b.is_empty() => CookieSource::Browser(b.to_string()),
        _ => CookieSource::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ck(name: &str, val: &str, domain: &str, http_only: bool, expires: Option<i64>) -> NetscapeCookie {
        NetscapeCookie { domain: domain.into(), path: "/".into(), secure: true, http_only, expires, name: name.into(), value: val.into() }
    }

    #[test]
    fn netscape_header_and_line() {
        let s = to_netscape(&[ck("SID", "abc", ".youtube.com", false, Some(1700000000))]);
        assert!(s.starts_with("# Netscape HTTP Cookie File\n"));
        assert!(s.contains(".youtube.com\tTRUE\t/\tTRUE\t1700000000\tSID\tabc\n"));
    }

    #[test]
    fn netscape_httponly_prefix() {
        let s = to_netscape(&[ck("__Secure-3PSID", "x", ".youtube.com", true, Some(1))]);
        assert!(s.contains("#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t1\t__Secure-3PSID\tx\n"));
    }

    #[test]
    fn netscape_session_cookie_expiry_zero_and_host_only() {
        let s = to_netscape(&[ck("VISITOR", "y", "www.youtube.com", false, None)]);
        assert!(s.contains("www.youtube.com\tFALSE\t/\tTRUE\t0\tVISITOR\ty\n"));
    }

    #[test]
    fn apply_to_youtube_google_only() {
        assert!(cookies_apply_to("https://www.youtube.com/watch?v=x"));
        assert!(cookies_apply_to("https://youtu.be/abc"));
        assert!(cookies_apply_to("https://r1---sn-x.googlevideo.com/videoplayback"));
        assert!(cookies_apply_to("https://accounts.google.com/x"));
        assert!(!cookies_apply_to("https://example.com/f.zip"));
        assert!(!cookies_apply_to("https://notyoutube.com.evil.com/x"));
    }

    #[test]
    fn choose_source_branches() {
        assert_eq!(choose_cookie_source("https://youtube.com/x", true, None), CookieSource::File);
        assert_eq!(choose_cookie_source("https://youtube.com/x", false, Some("chrome")), CookieSource::Browser("chrome".into()));
        assert_eq!(choose_cookie_source("https://youtube.com/x", false, Some("")), CookieSource::None);
        assert_eq!(choose_cookie_source("https://youtube.com/x", false, None), CookieSource::None);
        assert_eq!(choose_cookie_source("https://example.com/x", true, Some("chrome")), CookieSource::None);
    }
}
```
Pour le **stub** au Step 1, remplace temporairement les 3 corps de fonction par `unimplemented!()` (et `to_netscape` par `String::new()`) afin que les tests échouent ; au Step 3 tu mets le vrai code ci-dessus.

- [ ] **Step 2 :** `cargo test --manifest-path src-tauri/Cargo.toml -p download-core cookies::` → ÉCHEC (stub).
- [ ] **Step 3 :** Mettre les vrais corps (code ci-dessus).
- [ ] **Step 4 :** `cargo test --manifest-path src-tauri/Cargo.toml -p download-core cookies::` → PASS (5 tests).
- [ ] **Step 5 : Commit**
```bash
git checkout phase-youtube-login
git add src-tauri/crates/download-core/src/cookies.rs src-tauri/crates/download-core/src/lib.rs
git commit -m "feat(core): cookies.rs — format Netscape + choix de source pour yt-dlp"
```

---

## Task 2 : Réglage `cookies_from_browser` (storage)

**Files:** Modify: `src-tauri/crates/download-core/src/storage.rs`

- [ ] **Step 1 : Écrire le test (dans le module `#[cfg(test)]` de storage.rs)**

Repérer un test storage existant (ex. `category_dir_roundtrip_and_default_none`) pour le motif d'ouverture d'une `Storage` en mémoire/tempfile. Ajouter :
```rust
#[test]
fn cookies_from_browser_roundtrip_and_default_none() {
    let s = open_temp(); // utiliser le même helper que les autres tests storage
    assert_eq!(s.get_cookies_from_browser().unwrap(), None);
    s.set_cookies_from_browser(Some("chrome".into())).unwrap();
    assert_eq!(s.get_cookies_from_browser().unwrap(), Some("chrome".into()));
    s.set_cookies_from_browser(None).unwrap();
    assert_eq!(s.get_cookies_from_browser().unwrap(), None);
}
```
(Si le helper d'ouverture s'appelle autrement dans ce fichier, l'utiliser à la place de `open_temp()`.)

- [ ] **Step 2 :** `cargo test --manifest-path src-tauri/Cargo.toml -p download-core cookies_from_browser` → ÉCHEC (méthodes absentes).

- [ ] **Step 3 : Ajouter les méthodes** dans `impl Storage` (à côté de `category_dir`), motif clé-valeur identique :
```rust
/// Repli yt-dlp `--cookies-from-browser` (clé `cookies_from_browser`). Vide = None.
pub fn get_cookies_from_browser(&self) -> Result<Option<String>, CoreError> {
    let conn = self.conn.lock().unwrap();
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key=?1", ["cookies_from_browser"], |r| r.get::<_, String>(0))
        .ok()
        .filter(|s| !s.is_empty()))
}

pub fn set_cookies_from_browser(&self, value: Option<String>) -> Result<(), CoreError> {
    let conn = self.conn.lock().unwrap();
    let val = value.unwrap_or_default();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
        rusqlite::params!["cookies_from_browser", val],
    )
    .map_err(|e| CoreError::Storage(e.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4 :** `cargo test --manifest-path src-tauri/Cargo.toml -p download-core cookies_from_browser` → PASS.
- [ ] **Step 5 : Commit**
```bash
git checkout phase-youtube-login
git add src-tauri/crates/download-core/src/storage.rs
git commit -m "feat(core): réglage cookies_from_browser (get/set) dans storage"
```

---

## Task 3 : Glue Tauri — `src-tauri/src/youtube.rs` + commandes

**Files:**
- Create: `src-tauri/src/youtube.rs`
- Modify: `src-tauri/src/lib.rs` (déclarer `mod youtube;` + enregistrer les commandes)

- [ ] **Step 1 : Créer `src-tauri/src/youtube.rs`**
```rust
//! Connexion YouTube intégrée : fenêtre de login + export des cookies (incl. httpOnly)
//! vers un cookies.txt Netscape consommé par yt-dlp.
use crate::state::AppState;
use download_core::cookies::{self, CookieSource, NetscapeCookie};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const UA_CHROME: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const SESSION_COOKIES: [&str; 3] = ["__Secure-3PSID", "SAPISID", "SID"];

fn cookies_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("youtube-cookies.txt"))
}

/// Lit les cookies YouTube/Google du magasin webview et (ré)écrit le cookies.txt.
/// Écrit UNIQUEMENT si un cookie de session est présent (ne jamais écraser un fichier
/// valide par du vide si la lecture échoue). Renvoie `true` si connecté (fichier écrit).
pub fn sync_cookies(app: &AppHandle) -> Result<bool, String> {
    let win = app
        .get_webview_window("yt-login")
        .or_else(|| app.get_webview_window("main"))
        .ok_or("aucune fenêtre webview")?;
    let urls = [
        "https://www.youtube.com",
        "https://youtube.com",
        "https://accounts.google.com",
        "https://www.google.com",
    ];
    let mut out: Vec<NetscapeCookie> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for u in urls {
        let url = u.parse().map_err(|e| format!("url: {e}"))?;
        if let Ok(list) = win.cookies_for_url(url) {
            for c in list {
                let domain = c.domain().unwrap_or("").to_string();
                if !seen.insert((c.name().to_string(), domain.clone())) {
                    continue;
                }
                out.push(NetscapeCookie {
                    domain,
                    path: c.path().unwrap_or("/").to_string(),
                    secure: c.secure().unwrap_or(false),
                    http_only: c.http_only().unwrap_or(false),
                    expires: c.expires_datetime().map(|dt| dt.unix_timestamp()),
                    name: c.name().to_string(),
                    value: c.value().to_string(),
                });
            }
        }
    }
    let connected = out.iter().any(|c| SESSION_COOKIES.contains(&c.name.as_str()));
    if !connected {
        return Ok(false);
    }
    let path = cookies_path(app).ok_or("dossier de données indisponible")?;
    std::fs::write(&path, cookies::to_netscape(&out)).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Args cookies pour un run yt-dlp (re-synchro best-effort avant). Vide si non YouTube.
pub fn cookie_args(app: &AppHandle, url: &str) -> Vec<String> {
    if !cookies::cookies_apply_to(url) {
        return Vec::new();
    }
    let _ = sync_cookies(app); // best-effort ; n'écrase pas si pas de session
    let path = cookies_path(app);
    let has_file = path.as_ref().map(|p| p.exists()).unwrap_or(false);
    let browser = app.state::<AppState>().storage.get_cookies_from_browser().ok().flatten();
    match cookies::choose_cookie_source(url, has_file, browser.as_deref()) {
        CookieSource::File => vec!["--cookies".into(), path.unwrap().to_string_lossy().to_string()],
        CookieSource::Browser(b) => vec!["--cookies-from-browser".into(), b],
        CookieSource::None => Vec::new(),
    }
}

fn open_login(app: &AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("yt-login") {
        let _ = w.set_focus();
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(
        app,
        "yt-login",
        WebviewUrl::External("https://www.youtube.com/".parse().map_err(|e| format!("{e}"))?),
    )
    .title("Connexion YouTube")
    .inner_size(980.0, 720.0)
    .user_agent(UA_CHROME)
    .build()
    .map_err(|e| e.to_string())?;
    let app2 = app.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::CloseRequested { .. } = ev {
            let _ = sync_cookies(&app2); // yt-login encore vivant → lit ses cookies
            let _ = app2.emit("youtube://status", ());
        }
    });
    Ok(())
}

#[tauri::command]
pub fn youtube_login(app: AppHandle) -> Result<(), String> {
    open_login(&app)
}

#[tauri::command]
pub fn youtube_status(app: AppHandle) -> Result<bool, String> {
    Ok(cookies_path(&app).map(|p| p.exists()).unwrap_or(false))
}

#[tauri::command]
pub fn youtube_logout(app: AppHandle) -> Result<(), String> {
    if let Some(p) = cookies_path(&app) {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

#[tauri::command]
pub fn get_cookies_from_browser(app: AppHandle) -> Result<Option<String>, String> {
    app.state::<AppState>().storage.get_cookies_from_browser().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_cookies_from_browser(app: AppHandle, value: Option<String>) -> Result<(), String> {
    app.state::<AppState>().storage.set_cookies_from_browser(value).map_err(|e| e.to_string())
}
```

- [ ] **Step 2 : `src-tauri/src/lib.rs`** — ajouter `mod youtube;` (avec les autres `mod`) et enregistrer dans `tauri::generate_handler![ ... ]` :
```rust
            youtube::youtube_login,
            youtube::youtube_status,
            youtube::youtube_logout,
            youtube::get_cookies_from_browser,
            youtube::set_cookies_from_browser,
```

- [ ] **Step 3 : Compiler.**
Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: succès. Si `expires_datetime`/`cookies_for_url` diffèrent dans la version exacte de `tauri`/`cookie`, lire la signature dans le crate (`~/.cargo/registry/src/*/tauri-2.11.2/src/webview/webview_window.rs`) et adapter MINIMALEMENT (même intention : lire name/value/domain/path/secure/httpOnly/expiry). Reporter toute adaptation.

- [ ] **Step 4 : Commit**
```bash
git checkout phase-youtube-login
git add src-tauri/src/youtube.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): fenêtre de login YouTube + export cookies.txt + commandes"
```

---

## Task 4 : Intégration dans le runner yt-dlp

**Files:** Modify: `src-tauri/src/ytdlp.rs`

- [ ] **Step 1 : Ajouter un helper `referer_args`** (Referer seul, sans Cookie) près de `header_args` :
```rust
/// Referer seul (utilisé quand les cookies passent par --cookies, pas par --add-header Cookie:).
fn referer_args(headers: &RequestHeaders) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(r) = &headers.referrer {
        a.push("--add-header".into());
        a.push(format!("Referer:{r}"));
    }
    a
}
```

- [ ] **Step 2 : Dans `probe_formats`** remplacer `args.extend(header_args(headers));` (la ligne avant `args.push(url...)`) par :
```rust
    let yt = crate::youtube::cookie_args(app, url);
    if yt.is_empty() {
        args.extend(header_args(headers));
    } else {
        args.extend(yt);
        args.extend(referer_args(headers));
    }
```
(`probe_formats` a `app: &AppHandle` → passer `app`.)

- [ ] **Step 3 : Dans `run_download`** remplacer le `args.extend(header_args(headers));` de la même façon, mais `run_download` a `app: AppHandle` (propriété) → passer `&app` :
```rust
    let yt = crate::youtube::cookie_args(&app, url);
    if yt.is_empty() {
        args.extend(header_args(headers));
    } else {
        args.extend(yt);
        args.extend(referer_args(headers));
    }
```

- [ ] **Step 4 : Compiler + tests.**
Run: `cargo build --manifest-path src-tauri/Cargo.toml` → succès.
Run: `cargo test --manifest-path src-tauri/Cargo.toml` → tests tauri verts (dont `header_args_forwards_cookie_referer_not_user_agent` inchangé).
Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core` → download-core vert (cookies:: inclus).

- [ ] **Step 5 : Commit**
```bash
git checkout phase-youtube-login
git add src-tauri/src/ytdlp.rs
git commit -m "feat(ytdlp): cookies YouTube via --cookies/--cookies-from-browser (sinon en-tête Cookie)"
```

---

## Task 5 : Frontend — API + section « Compte YouTube » dans les Réglages

**Files:** Modify: `src/api.ts`, `src/components/SettingsDialog.tsx`

- [ ] **Step 1 : `src/api.ts`** — ajouter à la fin :
```ts
export function youtubeLogin(): Promise<void> {
  return invoke("youtube_login");
}
export function youtubeStatus(): Promise<boolean> {
  return invoke<boolean>("youtube_status");
}
export function youtubeLogout(): Promise<void> {
  return invoke("youtube_logout");
}
export function getCookiesFromBrowser(): Promise<string | null> {
  return invoke<string | null>("get_cookies_from_browser");
}
export function setCookiesFromBrowser(value: string | null): Promise<void> {
  return invoke("set_cookies_from_browser", { value });
}
export function onYoutubeStatus(cb: () => void): Promise<UnlistenFn> {
  return listen("youtube://status", () => cb());
}
```
(`listen`/`UnlistenFn` sont déjà importés en tête de `api.ts`.)

- [ ] **Step 2 : `src/components/SettingsDialog.tsx`** — section « Compte YouTube » (thème cockpit + icône Lucide).
(a) Imports : ajouter `Youtube` (ou `UserRound`) à l'import lucide-react existant ; ajouter les états :
```tsx
  const [ytConnected, setYtConnected] = useState(false);
  const [ytBrowser, setYtBrowser] = useState<string>("");
```
(b) Dans le `useEffect(open)` (chargement) :
```tsx
    api.youtubeStatus().then(setYtConnected);
    api.getCookiesFromBrowser().then((b) => setYtBrowser(b ?? ""));
```
(c) Encore dans le `useEffect`, s'abonner au rafraîchissement après login (et nettoyer) :
```tsx
    const unsub = api.onYoutubeStatus(() => api.youtubeStatus().then(setYtConnected));
    return () => { unsub.then((f) => f()); };
```
(Si le `useEffect` a déjà un cleanup, fusionner le `unsub.then((f)=>f())` dedans.)
(d) Handlers :
```tsx
  async function ytLogin() { await api.youtubeLogin(); }
  async function ytLogout() { await api.youtubeLogout(); setYtConnected(false); }
  async function ytSetBrowser(v: string) { setYtBrowser(v); await api.setCookiesFromBrowser(v || null); }
```
(e) Bloc JSX (placer après la section « Extension navigateur ») :
```tsx
        <div className="px-4 py-3 border-t border-border">
          <div className="font-medium text-ink mb-2 inline-flex items-center gap-2"><Youtube size={16} /> Compte YouTube</div>
          <div className="flex items-center gap-2 mb-2 text-sm">
            <span className={"inline-block w-2 h-2 rounded-full " + (ytConnected ? "bg-ok" : "bg-ink-mute")} />
            <span className="text-ink-soft">{ytConnected ? "Connecté" : "Non connecté"}</span>
            <span className="flex-1" />
            {ytConnected ? (
              <button className="text-xs border border-border rounded px-2 py-1 bg-surface text-ink-soft" onClick={ytLogout}>Se déconnecter</button>
            ) : (
              <button className="text-xs rounded px-2 py-1 cta-grad font-semibold" onClick={ytLogin}>Se connecter à YouTube</button>
            )}
          </div>
          <label className="block text-xs text-ink-soft mb-1">Repli : cookies du navigateur</label>
          <select className="bg-bg text-ink border border-border rounded-md px-2 py-1.5 text-sm" value={ytBrowser} onChange={(e) => ytSetBrowser(e.target.value)}>
            <option value="">Aucun</option>
            <option value="chrome">Chrome</option>
            <option value="edge">Edge</option>
            <option value="firefox">Firefox</option>
          </select>
        </div>
```

- [ ] **Step 3 : Vérifier.**
Run: `npx tsc --noEmit` → aucune erreur.
Run: `npm test` → suite verte (aucun test ne couvre SettingsDialog).

- [ ] **Step 4 : Commit**
```bash
git checkout phase-youtube-login
git add src/api.ts src/components/SettingsDialog.tsx
git commit -m "feat(ui): section « Compte YouTube » dans les Réglages (connexion + repli navigateur)"
```

---

## Task 6 : Vérification + smoke + merge

**Files:** aucun.

- [ ] **Step 1 :** `cargo test --manifest-path src-tauri/Cargo.toml` + `-p download-core` + `npx tsc --noEmit` + `npm test` → tout vert. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets 2>&1 | grep -E "youtube.rs|cookies.rs|ytdlp.rs"` → pas de nouveau warning (les 13 préexistants download-core ignorés).
- [ ] **Step 2 : Smoke manuel** (`npm run tauri dev`) :
  1. Réglages → « Se connecter à YouTube » → la fenêtre s'ouvre sur youtube.com ; se connecter ; fermer la fenêtre.
  2. Le statut passe à **Connecté** (point vert) ; vérifier l'existence de `youtube-cookies.txt` dans le dossier de données de l'app (`%APPDATA%/com.steph.tauri-app/` ou via app_data_dir).
  3. Coller un lien YouTube (vidéo nécessitant la connexion : âge/membre/privée) → le téléchargement se lance via yt-dlp (le `cookies.txt` est passé).
  4. « Se déconnecter » → `youtube-cookies.txt` supprimé, statut Non connecté.
  5. (Repli) Mettre « Repli navigateur = Chrome », se déconnecter → un lien YouTube utilise `--cookies-from-browser chrome` (nécessite Chrome connecté à YouTube).
- [ ] **Step 3 :** (optionnel) `npm run tauri build` pour un installateur à jour.
- [ ] **Step 4 :** Terminer via `superpowers:finishing-a-development-branch` (merge `--no-ff` dans `main`).

---

## Auto-revue (couverture spec)

- `to_netscape` (httpOnly `#HttpOnly_`, session→0, secure, include-sub) → Task 1 + tests. ✓
- `cookies_apply_to` (YouTube/Google) → Task 1 + tests. ✓
- `choose_cookie_source` (File/Browser/None) → Task 1 + tests. ✓
- Réglage `cookies_from_browser` persisté → Task 2 + test. ✓
- Fenêtre login (UA Chrome, persistante) + sync à la fermeture (lit le webview yt-login) → Task 3. ✓
- Export cookies httpOnly via `cookies_for_url`/`expires_datetime` ; écrit seulement si session présente (anti-écrasement) → Task 3. ✓
- Commandes login/status/logout/get/set + enregistrement → Task 3. ✓
- yt-dlp `--cookies`/`--cookies-from-browser`, et pas de `Cookie:` quand --cookies est utilisé (Referer conservé) → Task 4. ✓
- Re-synchro avant chaque run YouTube (best-effort, sans écraser) → Task 3 (`cookie_args` appelle `sync_cookies`). ✓
- UI section « Compte YouTube » (statut, login/logout, repli navigateur) + event refresh → Task 5. ✓
- Sécurité (cookies.txt effacé à la déconnexion) → Task 3 (`youtube_logout`). ✓
- Limites (blocage Google, magasin partagé) : mitigées (UA Chrome ; sync lit yt-login à la fermeture, fallback main ; écrit seulement si session) → Tasks 3. ✓
