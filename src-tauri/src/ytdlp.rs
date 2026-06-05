//! Coquille Tauri pour le sidecar yt-dlp : sonde des formats + téléchargement avec
//! progression et pause/reprise/annulation. La logique parseable vit dans
//! `download_core::ytdlp`.

use crate::state::AppState;
use download_core::engine::DownloadHandle;
use download_core::model::{DownloadStatus, Progress, RequestHeaders};
use download_core::CoreError;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::UnboundedSender;

/// Chemin du binaire ffmpeg sidecar (à côté de l'exécutable), pour `--ffmpeg-location`.
/// Cherche un fichier dont le nom commence par "ffmpeg" (gère `ffmpeg.exe` bundlé ET
/// `ffmpeg-<triple>.exe` en dev).
fn ffmpeg_location() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("ffmpeg") && (name.ends_with(".exe") || !name.contains('.')) {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }
    None
}

/// Supprime le fichier final, son `.part`, et les fragments intermédiaires de yt-dlp
/// (formats vidéo/audio séparés avant merge : `<stem>.fNNN.<ext>[.part]`).
fn cleanup_partials(dest: &std::path::Path) {
    let _ = std::fs::remove_file(dest);
    let _ = std::fs::remove_file(format!("{}.part", dest.to_string_lossy()));
    if let (Some(dir), Some(stem)) = (dest.parent(), dest.file_stem().and_then(|s| s.to_str())) {
        let prefix = format!("{stem}.f");
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&prefix) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

/// Après un succès, le conteneur réel produit par yt-dlp peut différer de l'extension
/// `.mp4` demandée (ex. `.mkv` pour certains flux). On retrouve le fichier réel par son
/// radical dans le dossier de destination — via le système de fichiers, donc avec des
/// noms Unicode corrects (immunisé contre l'encodage du stdout de yt-dlp). No-op si le
/// fichier `.mp4` demandé existe déjà (cas courant).
fn reconcile_output(app: &AppHandle, id: &str, dest: &Path) {
    if dest.exists() {
        return;
    }
    let dir = match dest.parent() {
        Some(d) => d,
        None => return,
    };
    let stem = match dest.file_stem() {
        Some(s) => s,
        None => return,
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_stem() == Some(stem) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let dest_str = path.to_string_lossy().to_string();
                let _ = app.state::<AppState>().storage.update_paths(id, name, &dest_str);
            }
            return;
        }
    }
}

/// Arguments d'en-têtes pour yt-dlp. On NE transmet PAS le User-Agent du navigateur :
/// yt-dlp gère son propre client/UA par site, et un UA de navigateur desktop casse
/// l'extraction YouTube (« Requested format is not available »). Cookies + Referer sont
/// conservés (vidéos authentifiées / protection hotlink).
fn header_args(headers: &RequestHeaders) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(r) = &headers.referrer {
        a.push("--add-header".into());
        a.push(format!("Referer:{r}"));
    }
    if let Some(c) = &headers.cookie {
        a.push("--add-header".into());
        a.push(format!("Cookie:{c}"));
    }
    a
}

/// Sonde les formats d'une URL via `yt-dlp -J`. Renvoie (titre, formats).
pub async fn probe_formats(
    app: &AppHandle,
    url: &str,
    headers: &RequestHeaders,
) -> Result<(String, Vec<download_core::ytdlp::YtdlpFormat>), String> {
    let mut args: Vec<String> = vec!["-J".into(), "--no-playlist".into(), "--no-warnings".into()];
    args.extend(header_args(headers));
    args.push(url.to_string());

    let cmd = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| format!("yt-dlp introuvable: {e}"))?
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8");
    let output = cmd.args(args).output().await.map_err(|e| format!("échec exécution yt-dlp: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(format!("yt-dlp -J a échoué: {tail}"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    download_core::ytdlp::parse_formats(&stdout).map_err(|e| e.to_string())
}

/// Télécharge via yt-dlp avec le format choisi, en émettant la progression.
/// Pause → tue le process en gardant le partiel ; annulation → tue + supprime ;
/// succès → émet Completed.
#[allow(clippy::too_many_arguments)]
pub async fn run_download(
    app: AppHandle,
    id: &str,
    url: &str,
    format_id: &str,
    dest: &Path,
    handle: DownloadHandle,
    headers: &RequestHeaders,
    tx: UnboundedSender<Progress>,
) -> Result<(), CoreError> {
    let mut args: Vec<String> = vec![
        "-f".into(),
        download_core::ytdlp::build_format_arg(format_id),
        "--no-playlist".into(),
        "--newline".into(),
        "--continue".into(),
        "--merge-output-format".into(),
        "mp4".into(),
        "--progress-template".into(),
        format!("{}%(progress.downloaded_bytes)s/%(progress.total_bytes)s", download_core::ytdlp::PROGRESS_PREFIX),
        "-o".into(),
        dest.to_string_lossy().to_string(),
    ];
    if let Some(ff) = ffmpeg_location() {
        args.push("--ffmpeg-location".into());
        args.push(ff);
    }
    args.extend(header_args(headers));
    args.push(url.to_string());

    let cmd = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| CoreError::Media(format!("yt-dlp introuvable: {e}")))?
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8");
    let (mut rx, child) = cmd.args(args).spawn().map_err(|e| CoreError::Media(format!("échec lancement yt-dlp: {e}")))?;
    let mut child = Some(child);

    let mut last: u64 = 0;
    let mut total: Option<u64> = None;
    let mut stderr_tail: Vec<String> = Vec::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(300));

    let mk = |dl, total, status| Progress {
        id: id.to_string(),
        downloaded_bytes: dl,
        total_bytes: total,
        speed_bps: 0,
        status,
        segments_done: None,
        segments_total: None,
    };

    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Some(CommandEvent::Stdout(bytes)) => {
                        let line = String::from_utf8_lossy(&bytes);
                        if let Some((dl, t)) = download_core::ytdlp::parse_progress(&line) {
                            handle.add_downloaded(dl.saturating_sub(last)); // compteur absolu -> delta
                            last = dl;
                            if t.is_some() { total = t; }
                            let _ = tx.send(mk(dl, total, DownloadStatus::Downloading));
                        }
                    }
                    Some(CommandEvent::Stderr(bytes)) => {
                        let l = String::from_utf8_lossy(&bytes).to_string();
                        stderr_tail.push(l);
                        if stderr_tail.len() > 10 { stderr_tail.remove(0); }
                    }
                    Some(CommandEvent::Terminated(payload)) => {
                        let code = payload.code.unwrap_or(-1);
                        if code == 0 {
                            reconcile_output(&app, id, dest);
                            let _ = tx.send(mk(last, total, DownloadStatus::Completed));
                            return Ok(());
                        }
                        let tail = stderr_tail.join("\n");
                        return Err(CoreError::Media(format!("yt-dlp a échoué (code {code}): {tail}")));
                    }
                    Some(_) => {}
                    None => {
                        return Err(CoreError::Media(
                            "yt-dlp s'est fermé sans signal de terminaison".to_string(),
                        ));
                    }
                }
            }
            _ = ticker.tick() => {
                if handle.is_cancelled() {
                    if let Some(c) = child.take() { let _ = c.kill(); }
                    cleanup_partials(dest);
                    return Ok(());
                }
                if handle.is_paused() {
                    if let Some(c) = child.take() { let _ = c.kill(); }
                    let _ = tx.send(mk(last, total, DownloadStatus::Paused));
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_core::model::RequestHeaders;

    #[test]
    fn header_args_forwards_cookie_referer_not_user_agent() {
        let h = RequestHeaders {
            referrer: Some("https://www.youtube.com/watch?v=x".into()),
            cookie: Some("VISITOR_INFO=abc".into()),
            user_agent: Some("Mozilla/5.0 (Windows NT 10.0) Chrome/123".into()),
        };
        let args = header_args(&h);
        // Le User-Agent du navigateur casse l'extraction YouTube de yt-dlp → non transmis.
        assert!(!args.iter().any(|a| a == "--user-agent"), "ne doit pas passer --user-agent");
        assert!(!args.iter().any(|a| a.contains("Mozilla")), "l'UA navigateur ne doit pas fuiter");
        // Cookies + Referer conservés (auth / hotlink).
        assert!(args.iter().any(|a| a == "Cookie:VISITOR_INFO=abc"), "Cookie conservé");
        assert!(args.iter().any(|a| a == "Referer:https://www.youtube.com/watch?v=x"), "Referer conservé");
    }
}
