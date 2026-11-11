//! Mise à jour de yt-dlp : vérification de la dernière version, téléchargement contrôlé par
//! somme SHA-256, installation dans le dossier de données.

use crate::state::AppState;
use crate::ytdlp::parse_version;
use crate::ytdlp_bin::{data_bin_dir, resolve_override};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::path::Path;
use std::sync::atomic::Ordering as AtomicOrdering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::ShellExt;

const LAST_CHECK_KEY: &str = "ytdlp_update.last_check";
const API_LATEST: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
const DOWNLOAD_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/download";
const INVALID: &str = "fichier téléchargé invalide";
const CHECK_INTERVAL_SECS: i64 = 86_400;

#[derive(Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    /// `yt-dlp.exe` a été remplacé ; la chaîne est la nouvelle version.
    Replaced(String),
    /// La cible est verrouillée : la nouvelle version attend en `yt-dlp.next.exe`.
    Pending(String),
}

#[derive(Serialize)]
pub struct YtdlpUpdateInfo {
    pub current: Option<String>,
    pub latest: String,
    pub available: bool,
    pub last_check: Option<i64>,
}

/// Composants numériques séparés par `.` ; un composant absent ou illisible vaut 0.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |v: &str| -> Vec<u64> { v.trim().split('.').map(|c| c.parse().unwrap_or(0)).collect() };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let o = pa.get(i).copied().unwrap_or(0).cmp(&pb.get(i).copied().unwrap_or(0));
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Somme (hex minuscules) de `file` dans un fichier `<hex>  <nom>` (format `sha256sum`).
pub fn parse_sha256sums(text: &str, file: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (sum, name) = (it.next()?, it.next()?);
        (name.trim_start_matches('*') == file).then(|| sum.to_lowercase())
    })
}

pub fn latest_tag(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get("tag_name")?.as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

/// La copie du dossier de données est périmée si le yt-dlp fourni est au moins aussi récent.
/// Une version inconnue : on ne supprime rien.
pub fn should_drop_data_copy(bundled: Option<&str>, data: Option<&str>) -> bool {
    match (bundled, data) {
        (Some(b), Some(d)) => compare_versions(b, d) != Ordering::Less,
        _ => false,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Écrit `bytes` dans `dir/yt-dlp.download`, vérifie la somme, puis `run_version`, et seulement
/// ensuite remplace `yt-dlp.exe` (ou, si la cible est verrouillée, dépose `yt-dlp.next.exe`).
/// En cas d'échec de la vérification, le fichier temporaire est supprimé et rien d'autre ne bouge.
pub fn install_from_bytes(
    dir: &Path,
    bytes: &[u8],
    expected_sha: &str,
    run_version: impl Fn(&Path) -> Option<String>,
) -> Result<InstallOutcome, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let download = dir.join("yt-dlp.download");
    // `fs::write` referme le fichier avant le renommage.
    std::fs::write(&download, bytes).map_err(|e| e.to_string())?;
    let verified = if hex(&Sha256::digest(bytes)).eq_ignore_ascii_case(expected_sha.trim()) {
        run_version(&download)
    } else {
        None
    };
    let Some(version) = verified else {
        let _ = std::fs::remove_file(&download);
        return Err(INVALID.to_string());
    };
    if std::fs::rename(&download, dir.join("yt-dlp.exe")).is_ok() {
        return Ok(InstallOutcome::Replaced(version));
    }
    let next = dir.join("yt-dlp.next.exe");
    let _ = std::fs::remove_file(&next);
    if let Err(e) = std::fs::rename(&download, &next) {
        let _ = std::fs::remove_file(&download);
        return Err(e.to_string());
    }
    Ok(InstallOutcome::Pending(version))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn read_last_check(app: &AppHandle) -> Option<i64> {
    app.state::<AppState>().storage.get_pref(LAST_CHECK_KEY).ok().flatten()?.parse().ok()
}

fn record_last_check(app: &AppHandle) {
    let _ = app.state::<AppState>().storage.set_pref(LAST_CHECK_KEY, &now_secs().to_string());
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("FlashGet")
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

/// Version affichée par `path --version`.
async fn exe_version(app: &AppHandle, path: &Path) -> Option<String> {
    let out = app.shell().command(path).args(["--version"]).output().await.ok()?;
    out.status.success().then(|| parse_version(&String::from_utf8_lossy(&out.stdout))).flatten()
}

async fn sidecar_version(app: &AppHandle) -> Option<String> {
    let out = app.shell().sidecar("yt-dlp").ok()?.args(["--version"]).output().await.ok()?;
    out.status.success().then(|| parse_version(&String::from_utf8_lossy(&out.stdout))).flatten()
}

async fn fetch_latest(client: &reqwest::Client) -> Result<String, String> {
    const NET: &str = "Impossible de joindre GitHub";
    let resp = client.get(API_LATEST).send().await.map_err(|_| NET.to_string())?;
    if !resp.status().is_success() {
        return Err(NET.to_string());
    }
    let body = resp.text().await.map_err(|_| NET.to_string())?;
    latest_tag(&body).ok_or_else(|| NET.to_string())
}

#[tauri::command]
pub async fn check_ytdlp_update(app: AppHandle) -> Result<YtdlpUpdateInfo, String> {
    let data_path = data_bin_dir(&app).and_then(|d| resolve_override(&d));
    let current;
    if let Some(path) = data_path {
        let data_v = exe_version(&app, &path).await;
        let bundled_v = sidecar_version(&app).await;
        if should_drop_data_copy(bundled_v.as_deref(), data_v.as_deref()) && std::fs::remove_file(&path).is_ok() {
            if let Ok(mut g) = app.state::<AppState>().ytdlp_version.lock() {
                *g = None;
            }
            current = bundled_v;
        } else {
            current = data_v.or(bundled_v);
        }
    } else {
        current = sidecar_version(&app).await;
    }
    let latest = fetch_latest(&http_client()?).await?;
    record_last_check(&app);
    let available = current.as_deref().map_or(true, |c| compare_versions(c, &latest) == Ordering::Less);
    Ok(YtdlpUpdateInfo { current, latest, available, last_check: read_last_check(&app) })
}

#[tauri::command]
pub fn get_ytdlp_last_check(app: AppHandle) -> Option<i64> {
    read_last_check(&app)
}

/// Remet `ytdlp_updating` à faux quelle que soit la sortie.
struct UpdatingGuard<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for UpdatingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, AtomicOrdering::SeqCst);
    }
}

#[tauri::command]
pub async fn update_ytdlp(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    if state.ytdlp_updating.swap(true, AtomicOrdering::SeqCst) {
        return Err("Mise à jour déjà en cours".into());
    }
    let _guard = UpdatingGuard(&state.ytdlp_updating);

    const NET: &str = "Impossible de joindre GitHub";
    let client = http_client()?;
    let tag = fetch_latest(&client).await?;
    let sums = client.get(format!("{DOWNLOAD_BASE}/{tag}/SHA2-256SUMS")).send().await.map_err(|_| NET.to_string())?;
    if !sums.status().is_success() {
        return Err(NET.into());
    }
    let sums = sums.text().await.map_err(|_| NET.to_string())?;
    let expected = parse_sha256sums(&sums, "yt-dlp.exe").ok_or_else(|| INVALID.to_string())?;

    let mut resp = client.get(format!("{DOWNLOAD_BASE}/{tag}/yt-dlp.exe")).send().await.map_err(|_| NET.to_string())?;
    if !resp.status().is_success() {
        return Err(NET.into());
    }
    let total = resp.content_length();
    let mut bytes: Vec<u8> = Vec::with_capacity(total.unwrap_or(0).min(512 * 1024 * 1024) as usize);
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    while let Some(chunk) = resp.chunk().await.map_err(|_| NET.to_string())? {
        bytes.extend_from_slice(&chunk);
        if last_emit.elapsed() >= Duration::from_millis(100) {
            last_emit = Instant::now();
            let _ = app.emit("ytdlp-update-progress", serde_json::json!({ "downloaded": bytes.len(), "total": total }));
        }
    }
    let _ = app.emit("ytdlp-update-progress", serde_json::json!({ "downloaded": bytes.len(), "total": total }));
    // Un fichier tronqué est écarté ici : la somme ne correspondrait de toute façon pas.
    if total.is_some_and(|t| t != bytes.len() as u64) {
        return Err(INVALID.into());
    }

    let dir = data_bin_dir(&app).ok_or_else(|| "dossier de données introuvable".to_string())?;
    // `install_from_bytes` est synchrone ; la vérification `--version` passe par le plugin shell
    // (asynchrone), pilotée depuis un thread bloquant avec `block_on`.
    let app2 = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        install_from_bytes(&dir, &bytes, &expected, |p| tauri::async_runtime::block_on(exe_version(&app2, p)))
    })
    .await
    .map_err(|e| e.to_string())??;

    let version = match outcome {
        InstallOutcome::Replaced(v) | InstallOutcome::Pending(v) => v,
    };
    if let Ok(mut g) = state.ytdlp_version.lock() {
        *g = None;
    }
    record_last_check(&app);
    Ok(version)
}

/// Vérification quotidienne 10 s après l'ouverture ; n'ouvre jamais de message en cas d'échec.
pub fn start_ytdlp_auto_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if read_last_check(&app).is_some_and(|t| now_secs() - t < CHECK_INTERVAL_SECS) {
            return;
        }
        let info = match check_ytdlp_update(app.clone()).await {
            Ok(i) => i,
            Err(e) => return eprintln!("vérification de yt-dlp : {e}"),
        };
        if !info.available {
            return;
        }
        let auto = app.state::<AppState>().storage.get_settings().map(|s| s.ytdlp_auto_update).unwrap_or(true);
        if auto {
            match update_ytdlp(app.clone()).await {
                Ok(version) => {
                    let _ = app.emit("ytdlp-updated", serde_json::json!({ "version": version }));
                }
                Err(e) => eprintln!("mise à jour de yt-dlp : {e}"),
            }
        } else {
            let _ = app.emit("ytdlp-update-available", serde_json::json!({ "current": info.current, "latest": info.latest }));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("flashget-ytdlp-upd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn compare_orders_versions() {
        assert_eq!(compare_versions("2026.10.30", "2026.11.08"), Ordering::Less);
        assert_eq!(compare_versions("2026.11.08", "2026.11.08.1"), Ordering::Less);
        assert_eq!(compare_versions("2026.11.08", "2026.11.08"), Ordering::Equal);
        assert_eq!(compare_versions("2026.11.10", "2026.11.9"), Ordering::Greater);
    }

    #[test]
    fn sums_pick_exact_file() {
        let t = "aaaa1111  yt-dlp\nBBBB2222  yt-dlp.exe\ncccc3333  yt-dlp_x86.exe\n";
        assert_eq!(parse_sha256sums(t, "yt-dlp.exe"), Some("bbbb2222".into()));
        assert_eq!(parse_sha256sums(t, "absent.exe"), None);
    }

    #[test]
    fn latest_tag_reads_tag_name() {
        assert_eq!(latest_tag(r#"{"tag_name":"2026.11.08","x":1}"#), Some("2026.11.08".into()));
        assert_eq!(latest_tag("pas du json"), None);
        assert_eq!(latest_tag("{}"), None);
    }

    #[test]
    fn install_rejects_bad_checksum() {
        let d = tmp("badsum");
        std::fs::write(d.join("yt-dlp.exe"), b"old").unwrap();
        let r = install_from_bytes(&d, b"new", &sha(b"autre"), |_| Some("1".into()));
        assert_eq!(r, Err("fichier téléchargé invalide".to_string()));
        assert_eq!(std::fs::read(d.join("yt-dlp.exe")).unwrap(), b"old");
        assert!(!d.join("yt-dlp.download").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_rejects_unrunnable() {
        let d = tmp("unrunnable");
        std::fs::write(d.join("yt-dlp.exe"), b"old").unwrap();
        let r = install_from_bytes(&d, b"new", &sha(b"new"), |_| None);
        assert_eq!(r, Err("fichier téléchargé invalide".to_string()));
        assert_eq!(std::fs::read(d.join("yt-dlp.exe")).unwrap(), b"old");
        assert!(!d.join("yt-dlp.download").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_replaces() {
        let d = tmp("replace");
        std::fs::write(d.join("yt-dlp.exe"), b"old").unwrap();
        let upper = sha(b"new").to_uppercase();
        let r = install_from_bytes(&d, b"new", &upper, |p| {
            assert!(p.ends_with("yt-dlp.download"));
            Some("2026.11.08".into())
        });
        assert_eq!(r, Ok(InstallOutcome::Replaced("2026.11.08".into())));
        assert_eq!(std::fs::read(d.join("yt-dlp.exe")).unwrap(), b"new");
        assert!(!d.join("yt-dlp.download").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_falls_back_to_next_when_locked() {
        let d = tmp("locked");
        let target = d.join("yt-dlp.exe");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(d.join("yt-dlp.next.exe"), b"older").unwrap();
        #[cfg(windows)]
        let _lock = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new().read(true).share_mode(0).open(&target).unwrap()
        };
        #[cfg(not(windows))]
        {
            // Cible non remplaçable : un dossier à la place du fichier.
            std::fs::remove_file(&target).unwrap();
            std::fs::create_dir(&target).unwrap();
        }
        let r = install_from_bytes(&d, b"new", &sha(b"new"), |_| Some("2026.11.08".into()));
        assert_eq!(r, Ok(InstallOutcome::Pending("2026.11.08".into())));
        assert_eq!(std::fs::read(d.join("yt-dlp.next.exe")).unwrap(), b"new");
        assert!(!d.join("yt-dlp.download").exists());
        #[cfg(windows)]
        {
            drop(_lock);
            assert_eq!(std::fs::read(&target).unwrap(), b"old");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn stale_data_copy_removed() {
        assert!(should_drop_data_copy(Some("2026.11.08"), Some("2026.10.30")));
        assert!(should_drop_data_copy(Some("2026.11.08"), Some("2026.11.08")));
        assert!(!should_drop_data_copy(Some("2026.10.30"), Some("2026.11.08")));
        assert!(!should_drop_data_copy(None, Some("2026.11.08")));
        assert!(!should_drop_data_copy(Some("2026.11.08"), None));
    }
}
