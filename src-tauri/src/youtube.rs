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
        let url: tauri::Url = u.parse().map_err(|e| format!("url: {e}"))?;
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
