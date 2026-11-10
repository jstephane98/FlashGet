use crate::state::AppState;
use download_core::engine::{self, DownloadHandle};
use download_core::hls::playlist::{is_master, parse_master, parse_media, MasterPlaylist, Variant};
use download_core::hls::select::{best_variant, estimated_size};
use download_core::http::ByteFetcher;
use download_core::model::{Download, DownloadKind, DownloadStatus, Priority, Progress, RequestHeaders, Settings, Category, CategoryDirs};
use download_core::scheduler::pick_to_start;
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::mpsc;

#[derive(serde::Deserialize)]
pub struct AddBatchArgs {
    pub text: String,
    pub dest_dir: String,
    #[serde(default)]
    pub mode: Option<String>,
    /// `true` = les téléchargements du lot sont créés `Held` (ajoutés sans démarrer).
    #[serde(default)]
    pub hold: bool,
}

/// Statut initial d'un téléchargement ajouté : `Held` (en attente) ou `Queued`.
fn initial_status(hold: bool) -> DownloadStatus {
    if hold { DownloadStatus::Held } else { DownloadStatus::Queued }
}

#[derive(serde::Serialize)]
pub struct BatchSummary {
    pub added: usize,
    pub skipped_invalid: usize,
    pub skipped_duplicate: usize,
    pub failed: usize,
}

/// Crée un `Download` (`Queued`, ou `Held` si `hold`) SANS sonder l'URL (le moteur sondera
/// au démarrage). Le nom vient de l'URL ; taille/segments inconnus. Ne ré-évalue PAS la file
/// (l'appelant le fait une seule fois après le lot).
pub(crate) async fn add_url_no_probe(
    app: &AppHandle,
    url: &str,
    dest_dir: &str,
    headers: RequestHeaders,
    hold: bool,
) -> Result<Download, String> {
    let file_name = download_core::naming::choose_file_name(
        None,
        download_core::http::file_name_from_url(url).as_deref(),
    );
    let category = download_core::categorizer::categorize(&file_name);

    // Dossier : argument explicite > dossier configuré pour la catégorie > dossier Téléchargements.
    let dest_dir = crate::dest::dest_dir_for(app, Some(dest_dir), category)?;
    let file_name = crate::dest::unique_file_name(&dest_dir, &file_name, &crate::dest::taken_paths(app));
    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("invalid file name derived from URL".to_string());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None,
        downloaded_bytes: 0,
        status: initial_status(hold),
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
        kind: DownloadKind::Regular,
        audio_url: None,
        media_selection: None,
        thumbnail_url: None,
        duration_s: None,
        created_at: None,
        started_at: None,
        completed_at: None,
        queue_rank: None,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    Ok(download)
}

/// Crée un téléchargement yt-dlp (vidéo meilleure qualité ou audio MP3), `Queued` ou `Held` si `hold`.
async fn add_ytdlp_queued(app: &AppHandle, url: &str, title: &str, audio: bool, dest_dir: Option<&str>, hold: bool) -> Result<Download, String> {
    let stem = download_core::naming::sanitize_title(Some(title));
    let stem = if stem.trim().is_empty() { "video".to_string() } else { stem };
    let ext = if audio { "mp3" } else { "mp4" };
    let file_name = download_core::naming::ensure_extension(&stem, ext);
    let category = if audio { Category::Music } else { Category::Video };
    let dest_dir = crate::dest::dest_dir_for(app, dest_dir, category)?;
    let file_name = crate::dest::unique_file_name(&dest_dir, &file_name, &crate::dest::taken_paths(app));
    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("nom de fichier invalide".into());
    }
    let job = if audio {
        download_core::ytdlp::YtdlpJob { format_id: "bestaudio".into(), mode: download_core::ytdlp::JobMode::Audio, time_range: None, subs: None, convert_mp3: true }
    } else {
        download_core::ytdlp::YtdlpJob { format_id: "bestvideo".into(), mode: download_core::ytdlp::JobMode::Av, time_range: None, subs: None, convert_mp3: false }
    };
    let sel = serde_json::to_string(&job).map_err(|e| e.to_string())?;
    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None,
        downloaded_bytes: 0,
        status: initial_status(hold),
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category,
        referrer: None,
        cookie: None,
        user_agent: None,
        kind: DownloadKind::Ytdlp,
        audio_url: None,
        media_selection: Some(sel),
        thumbnail_url: None,
        duration_s: None,
        created_at: None,
        started_at: None,
        completed_at: None,
        queue_rank: None,
    };
    app.state::<AppState>().storage.upsert(&download).map_err(|e| e.to_string())?;
    Ok(download)
}

/// Ajoute en lot : parse le texte (une URL par ligne), crée chaque téléchargement
/// en `Queued` sans sonder, puis ré-évalue la file une seule fois. Avec `hold`, tout
/// est créé `Held` et la file n'est pas ré-évaluée. Une ligne en échec
/// (ex. dossier inaccessible) n'interrompt pas les suivantes.
/// `mode` : `None`/`"direct"` = fichiers directs ; `"video"` ou `"audio"` = via yt-dlp
/// (les playlists sont développées en une entrée par vidéo).
#[tauri::command]
pub async fn add_batch_downloads(app: AppHandle, args: AddBatchArgs) -> Result<BatchSummary, String> {
    let parsed = download_core::batch::parse_batch(&args.text);
    let mut added = 0usize;
    let mut failed = 0usize;
    let mode = args.mode.as_deref().unwrap_or("direct");
    for url in &parsed.urls {
        if mode == "video" || mode == "audio" {
            match crate::ytdlp::list_entries(&app, url).await {
                Ok(entries) => {
                    for (u, t) in entries {
                        match add_ytdlp_queued(&app, &u, &t, mode == "audio", Some(&args.dest_dir), args.hold).await {
                            Ok(_) => added += 1,
                            Err(e) => {
                                eprintln!("batch: échec ajout {u}: {e}");
                                failed += 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("batch: échec lecture {url}: {e}");
                    failed += 1;
                }
            }
            continue;
        }
        match add_url_no_probe(&app, url, &args.dest_dir, RequestHeaders::default(), args.hold).await {
            Ok(_) => added += 1,
            Err(e) => {
                eprintln!("batch: échec ajout {url}: {e}");
                failed += 1;
            }
        }
    }
    if !args.hold {
        reevaluate(&app);
    } else {
        // Dans une plage active, le planificateur lâche tout de suite les `held` ajoutés.
        crate::queue::planner_tick(&app);
    }
    Ok(BatchSummary {
        added,
        skipped_invalid: parsed.skipped_invalid,
        skipped_duplicate: parsed.skipped_duplicate,
        failed,
    })
}

#[derive(serde::Serialize, Clone)]
pub struct MediaVariant {
    pub index: u32,
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

/// Sonde un master/media HLS SANS effet de bord : playlist maître (une seule variante
/// synthétique pour une media playlist) + variantes affichables (éventuellement vide).
pub(crate) async fn hls_variants(
    app: &AppHandle,
    url: &str,
    headers: &RequestHeaders,
) -> Result<(MasterPlaylist, Vec<MediaVariant>), String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let text = tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, url, headers))
        .await
        .map_err(|_| "délai dépassé lors de la récupération de la playlist".to_string())?
        .map_err(|e| e.to_string())?;

    let (master, total_duration) = if is_master(&text) {
        let m = parse_master(&text, url);
        let dur = if let Some(v) = m.variants.get(best_variant(&m)) {
            match tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, &v.uri, headers)).await {
                Ok(Ok(t)) => parse_media(&t, &v.uri).total_duration(),
                _ => 0.0,
            }
        } else {
            0.0
        };
        (m, dur)
    } else {
        let media = parse_media(&text, url);
        let dur = media.total_duration();
        let m = MasterPlaylist {
            variants: vec![Variant {
                uri: url.to_string(),
                bandwidth: 0,
                resolution: None,
                audio_group: None,
            }],
            audio: vec![],
        };
        (m, dur)
    };

    let variants: Vec<MediaVariant> = master
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| MediaVariant {
            index: i as u32,
            resolution: v.resolution.map(|(w, h)| format!("{w}x{h}")),
            bandwidth: v.bandwidth,
            est_size: if v.bandwidth > 0 && total_duration > 0.0 {
                Some(estimated_size(v.bandwidth, total_duration))
            } else {
                None
            },
        })
        .collect();
    Ok((master, variants))
}

/// Sonde un MPD DASH SANS effet de bord : MPD parsé + variantes vidéo (éventuellement vide).
pub(crate) async fn dash_variants(
    app: &AppHandle,
    url: &str,
    headers: &RequestHeaders,
) -> Result<(download_core::dash::mpd::Mpd, Vec<MediaVariant>), String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let text = tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, url, headers))
        .await
        .map_err(|_| "délai dépassé lors de la récupération du MPD".to_string())?
        .map_err(|e| e.to_string())?;
    let mpd = download_core::dash::mpd::parse_mpd(&text, url).map_err(|e| e.to_string())?;
    let variants: Vec<MediaVariant> = download_core::dash::select::video_variants(&mpd).iter().map(|(_, v)| MediaVariant {
        index: v.index, resolution: v.resolution.clone(), bandwidth: v.bandwidth, est_size: v.est_size,
    }).collect();
    Ok((mpd, variants))
}

/// Lit entièrement une ressource texte (playlist) via le fetcher.
async fn fetch_text<F: ByteFetcher>(fetcher: &F, url: &str, headers: &RequestHeaders) -> Result<String, download_core::CoreError> {
    use futures_util::StreamExt;
    let mut stream = fetcher.fetch_range(url, None, headers).await?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk?);
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[tauri::command]
pub fn list_downloads(state: State<AppState>) -> Result<Vec<Download>, String> {
    state.storage.list().map_err(|e| e.to_string())
}

/// Reprend un téléchargement (le repasse `Queued`) puis ré-évalue.
#[tauri::command]
pub fn start_download(app: AppHandle, id: String) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let d = state.storage.get_by_id(&id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("download not found: {id}"))?;
        // Ne pas reprendre un téléchargement déjà terminé.
        if d.status == DownloadStatus::Completed {
            return Ok(());
        }
        // Ne rien faire si déjà actif.
        if state.handles.lock().unwrap().contains_key(&id) {
            return Ok(());
        }
        state.storage.update_state(&id, d.downloaded_bytes, DownloadStatus::Queued)
            .map_err(|e| e.to_string())?;
    }
    reevaluate_with(&app, Some(&id));
    Ok(())
}

#[tauri::command]
pub fn pause_download(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.pause();
    }
    // Le slot se libérera quand la tâche moteur se terminera (elle ré-évalue alors).
    Ok(())
}

/// Arrête (finalise) un enregistrement live : pose le drapeau `stop` ; le moteur
/// remux le partiel et émet `Completed`. No-op si le handle n'est plus actif.
#[tauri::command]
pub fn stop_recording(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_download(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let was_active = {
        let mut handles = state.handles.lock().unwrap();
        if let Some(h) = handles.get(&id) {
            h.cancel();
        }
        handles.remove(&id).is_some()
    };
    // Non actif => aucune tâche moteur ne supprimera le fichier partiel : on le fait ici.
    if !was_active {
        if let Ok(Some(d)) = state.storage.get_by_id(&id) {
            let p = std::path::PathBuf::from(&d.dest_path);
            let _ = std::fs::remove_file(&p);
            download_core::resume::clear(&p);
            let _ = std::fs::remove_dir_all(format!("{}.fghls", d.dest_path));
        }
    }
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    crate::queue::reset_run_if_idle(&state);
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn set_priority(app: AppHandle, id: String, priority: Priority) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        state.storage.set_priority(&id, priority).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    state.storage.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(app: AppHandle, max_concurrent: u32, max_speed_bps: u64) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let s = Settings { max_concurrent: max_concurrent.max(1), max_speed_bps };
        state.storage.set_settings(&s).map_err(|e| e.to_string())?;
        state.limiter.set_rate(s.max_speed_bps);
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn pause_all(state: State<AppState>) -> Result<(), String> {
    crate::queue::pause_active(&state);
    Ok(())
}

#[tauri::command]
pub fn resume_all(app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let downloads = state.storage.list().map_err(|e| e.to_string())?;
        for d in downloads.iter().filter(|d| d.status == DownloadStatus::Paused) {
            state.storage.update_state(&d.id, d.downloaded_bytes, DownloadStatus::Queued)
                .map_err(|e| e.to_string())?;
        }
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn get_category_dirs(state: State<AppState>) -> Result<CategoryDirs, String> {
    state.storage.get_category_dirs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_category_dir(state: State<AppState>, category: Category, path: Option<String>) -> Result<(), String> {
    let path = path.filter(|p| !p.trim().is_empty());
    state.storage.set_category_dir(category, path).map_err(|e| e.to_string())
}

/// Retire l'entrée de la liste SANS supprimer le fichier (distinct d'`annuler`).
/// Refuse si le téléchargement est encore actif.
#[tauri::command]
pub fn remove_from_list(state: State<AppState>, id: String) -> Result<(), String> {
    if state.handles.lock().unwrap().contains_key(&id) {
        return Err("arrêtez le téléchargement avant de le retirer".to_string());
    }
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    crate::queue::reset_run_if_idle(&state);
    Ok(())
}

#[tauri::command]
pub fn clear_completed(state: State<AppState>) -> Result<(), String> {
    state.storage.clear_completed().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_file(app: AppHandle, id: String) -> Result<(), String> {
    let d = app.state::<AppState>().storage.get_by_id(&id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    app.opener().open_path(d.dest_path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_folder(app: AppHandle, id: String) -> Result<(), String> {
    let d = app.state::<AppState>().storage.get_by_id(&id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    match folder_target(std::path::Path::new(&d.dest_path)) {
        FolderTarget::Reveal(p) => app.opener().reveal_item_in_dir(&p).map_err(|e| e.to_string()),
        FolderTarget::Open(parent) => app.opener().open_path(parent.to_string_lossy(), None::<&str>).map_err(|e| e.to_string()),
        FolderTarget::Missing(p) => app.opener().reveal_item_in_dir(&p).map_err(|e| e.to_string()),
    }
}

/// Que faire pour « Ouvrir le dossier » : révéler le fichier s'il existe,
/// sinon ouvrir son dossier parent (fichier pas encore créé : file, HLS, .part...).
#[derive(Debug, PartialEq)]
enum FolderTarget {
    Reveal(PathBuf),
    Open(PathBuf),
    /// Ni le fichier ni son parent n'existent : on garde l'erreur d'origine.
    Missing(PathBuf),
}

fn folder_target(dest: &std::path::Path) -> FolderTarget {
    if dest.exists() {
        return FolderTarget::Reveal(dest.to_path_buf());
    }
    match dest.parent() {
        Some(parent) if parent.is_dir() => FolderTarget::Open(parent.to_path_buf()),
        _ => FolderTarget::Missing(dest.to_path_buf()),
    }
}

#[cfg(test)]
mod batch_hold_tests {
    use super::*;

    #[test]
    fn batch_hold_flag_defaults_off_and_sets_held() {
        let a: AddBatchArgs = serde_json::from_value(serde_json::json!({ "text": "x", "dest_dir": "" })).unwrap();
        assert!(!a.hold);
        let a: AddBatchArgs = serde_json::from_value(serde_json::json!({ "text": "x", "dest_dir": "", "hold": true })).unwrap();
        assert!(a.hold);
        assert_eq!(initial_status(true), DownloadStatus::Held);
        assert_eq!(initial_status(false), DownloadStatus::Queued);
    }
}

#[cfg(test)]
mod folder_target_tests {
    use super::*;

    #[test]
    fn reveals_existing_file() {
        let dir = std::env::temp_dir().join(format!("fg_ft_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(folder_target(&f), FolderTarget::Reveal(f.clone()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn opens_parent_when_file_missing() {
        let dir = std::env::temp_dir();
        let f = dir.join("fg_ft_does_not_exist.part");
        assert_eq!(folder_target(&f), FolderTarget::Open(dir.clone()));
        let g = dir.join("fg_ft_no_dir").join("x.bin");
        assert!(matches!(folder_target(&g), FolderTarget::Missing(_)));
    }
}

/// Ré-évalue la file : démarre les `Queued` élus selon la limite + priorités.
/// Atomique vis-à-vis de la map `handles` (verrou tenu pendant pick + launch).
pub(crate) fn reevaluate(app: &AppHandle) {
    reevaluate_with(app, None);
}

/// Comme `reevaluate`, mais `forced` (démarrage manuel) passe devant la file,
/// même suspendue.
pub(crate) fn reevaluate_with(app: &AppHandle, forced: Option<&str>) {
    let state = app.state::<AppState>();
    let settings = match state.storage.get_settings() {
        Ok(s) => s,
        Err(e) => { eprintln!("reevaluate: get_settings failed: {e}"); return; }
    };
    let downloads = match state.storage.list() {
        Ok(d) => d,
        Err(e) => { eprintln!("reevaluate: list failed: {e}"); return; }
    };
    let paused = state.storage.queue_paused().unwrap_or(false);
    let to_start = pick_to_start(&downloads, settings.max_concurrent, paused, forced);
    if to_start.is_empty() {
        return;
    }
    // Verrou tenu pour rendre pick+launch atomique (évite un double-démarrage concurrent).
    let mut handles = state.handles.lock().unwrap();
    let mut free = settings.max_concurrent.saturating_sub(handles.len() as u32);
    for id in to_start {
        if free == 0 {
            break;
        }
        if handles.contains_key(&id) {
            continue; // déjà actif
        }
        let download = match downloads.iter().find(|d| d.id == id) {
            Some(d) => d.clone(),
            None => continue,
        };
        // Marquer Downloading conditionnellement — ne jamais écraser un statut terminal ni
        // un `held` posé entre la lecture de la liste et ce verrou (sortie de plage) : dans
        // ce cas on ne lance pas. Une erreur de stockage garde l'ancien comportement (lancer).
        if let Ok(false) = state.storage.mark_active(&id, download.downloaded_bytes) {
            continue;
        }
        let handle = DownloadHandle::new();
        handles.insert(id.clone(), handle.clone());
        // Début (ou poursuite) d'une exécution de la file. Ordre des verrous : handles → planner.
        crate::queue::on_started(app);
        launch(app, download, handle);
        free -= 1;
    }
}

/// Au démarrage : finalise les enregistrements live interrompus (statut
/// `Recording` persisté mais aucune tâche moteur active). work dir non vide →
/// concat + remux → `Completed` ; vide → la ligne est retirée.
pub fn recover_interrupted_recordings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let downloads = match state.storage.list() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("recovery: list failed: {e}");
            return;
        }
    };
    for d in downloads.into_iter().filter(|d| d.status == DownloadStatus::Recording) {
        let dest = PathBuf::from(&d.dest_path);
        let work = PathBuf::from(format!("{}.fghls", d.dest_path));
        let has_segments = std::fs::read_dir(&work)
            .map(|rd| {
                rd.filter_map(|e| e.ok()).any(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.ends_with(".seg") && !n.contains("_init"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !has_segments {
            let _ = std::fs::remove_dir_all(&work);
            let _ = state.storage.delete(&d.id);
            continue;
        }
        // Finalise de façon asynchrone (au démarrage, hors UI critique).
        let app2 = app.clone();
        let id = d.id.clone();
        tauri::async_runtime::spawn(async move {
            let st = app2.state::<AppState>();
            let remux = crate::remux::SidecarRemuxer::new(app2.clone());
            let v_has_init = work.join("v_init.seg").exists();
            let a_has_init = work.join("a_init.seg").exists();
            let has_audio = std::fs::read_dir(&work)
                .map(|rd| {
                    rd.filter_map(|e| e.ok()).any(|e| {
                        e.file_name()
                            .to_str()
                            .map(|n| n.starts_with('a') && n.ends_with(".seg") && !n.contains("_init"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            // Clé ClearKey persistée par le moteur DASH live (None pour HLS / clair).
            let decryption_key = std::fs::read_to_string(work.join("clearkey.hex"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let audio_arg = if has_audio { Some(a_has_init) } else { None };
            match download_core::media_io::finalize_and_remux(&work, v_has_init, audio_arg, decryption_key, &remux, &dest).await {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(&work);
                    let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                    let _ = st.storage.update_state(&id, bytes, DownloadStatus::Completed);
                    let _ = app2.emit(
                        "download://progress",
                        &Progress {
                            id: id.clone(),
                            downloaded_bytes: bytes,
                            total_bytes: None,
                            speed_bps: 0,
                            status: DownloadStatus::Completed,
                            segments_done: None,
                            segments_total: None, parts: None,
                        },
                    );
                }
                Err(e) => {
                    eprintln!("recovery: remux failed for {id}: {e}");
                    let _ = st
                        .storage
                        .mark_error(&id, 0, "finalisation de l'enregistrement échouée");
                }
            }
        });
    }
}

/// Ramène la fenêtre principale au premier plan (montre + désiconifie + focus).
pub(crate) fn focus_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Émet une notif système de fin de téléchargement (Completed/Error) si le réglage
/// `notify_on_complete` est activé. Le corps de la notif = nom du fichier (repli : id).
fn notify_terminal(app: &AppHandle, status: DownloadStatus, id: &str) {
    let st = app.state::<AppState>();
    if !st.storage.get_notify_on_complete().unwrap_or(true) {
        return;
    }
    let file_name = st
        .storage
        .get_by_id(id)
        .ok()
        .flatten()
        .map(|d| d.file_name)
        .unwrap_or_else(|| id.to_string());
    let (title, body) = match status {
        DownloadStatus::Completed => ("Téléchargement terminé", file_name),
        _ => ("Échec du téléchargement", file_name),
    };

    #[cfg(target_os = "windows")]
    {
        // Émission native : permet de gérer le clic (on_activated) — absent du plugin desktop.
        // app_id = identifiant du config = AUMID du raccourci créé par l'installateur.
        // NB : le callback se déclenche tant que l'app tourne (cas normal : elle vient d'émettre
        // la notif). Un clic depuis le Centre de notifications après fermeture de l'app ne fait rien
        // (aucun serveur COM d'activation enregistré) — limite acceptée.
        let app_id = app.config().identifier.clone();
        let handle = app.clone();
        if let Err(e) = tauri_winrt_notification::Toast::new(&app_id)
            .title(title)
            .text1(&body)
            .on_activated(move |_action| {
                // on_activated tourne sur le thread COM/WinRT, pas le thread principal Tauri :
                // marshaler la manipulation de fenêtre sur le thread principal.
                let h = handle.clone();
                let _ = handle.run_on_main_thread(move || focus_main_window(&h));
                Ok(())
            })
            .show()
        {
            eprintln!("notif Windows échouée: {e}");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title(title).body(&body).show();
    }
}

/// Lance les deux tâches (relais de progression + moteur) pour un téléchargement.
fn launch(app: &AppHandle, download: Download, handle: DownloadHandle) {
    let id = download.id.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    // Relais de progression : vitesse + emit + persistance des états terminaux.
    let app_for_rx = app.clone();
    let id_for_rx = id.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_t = Instant::now();
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;
        let mut last_persisted: Option<DownloadStatus> = None;
        while let Some(mut p) = rx.recv().await {
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64();
            if dt >= 0.5 {
                let delta = p.downloaded_bytes.saturating_sub(last_bytes);
                current_speed = (delta as f64 / dt) as u64;
                last_t = now;
                last_bytes = p.downloaded_bytes;
            }
            p.speed_bps = current_speed;
            let persist = matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error)
                || (p.status == DownloadStatus::Recording && last_persisted != Some(DownloadStatus::Recording));
            if persist {
                // Pause demandée par la sortie de plage du planificateur : l'élément retourne
                // en attente (`Held`) au lieu de `Paused`. Nettoyé aussi sur Completed/Error.
                if p.status != DownloadStatus::Recording
                    && crate::queue::take_hold_on_pause(&app_for_rx, &id_for_rx)
                    && p.status == DownloadStatus::Paused
                {
                    p.status = DownloadStatus::Held;
                }
                let st = app_for_rx.state::<AppState>();
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
                // Notif de fin via le canal de progression (Completed). Une seule fois par
                // transition terminale : la garde `last_persisted != Some(p.status)` est évaluée
                // AVANT la réaffectation de `last_persisted` ci-dessous. Les échecs (Error) ne
                // passent pas par ce canal : ils sont notifiés depuis le bras Err de la tâche moteur.
                if matches!(p.status, DownloadStatus::Completed | DownloadStatus::Error)
                    && last_persisted != Some(p.status)
                {
                    notify_terminal(&app_for_rx, p.status, &id_for_rx);
                }
                let first = last_persisted != Some(p.status);
                last_persisted = Some(p.status);
                // Suivi de fin de file, APRÈS la persistance (le comptage lit le statut à jour).
                crate::queue::on_terminal_persisted(&app_for_rx, p.status, first);
            }
            // Émission APRÈS la persistance des états terminaux : un refresh() côté UI
            // déclenché par l'évènement lit ainsi le statut et completed_at à jour.
            let _ = app_for_rx.emit("download://progress", &p);
        }
    });

    // Tâche moteur.
    let app_for_dl = app.clone();
    let dest = PathBuf::from(&download.dest_path);
    let url = download.url.clone();
    let headers = download.request_headers();
    let id_for_dl = id.clone();
    let handle_for_err = handle.clone();
    let kind = download.kind;
    let audio_url = download.audio_url.clone();
    let media_selection = download.media_selection.clone();
    let (fetcher, limiter) = {
        let st = app.state::<AppState>();
        (st.fetcher.clone(), st.limiter.clone())
    };
    tauri::async_runtime::spawn(async move {
        let result = match kind {
            download_core::model::DownloadKind::Regular => {
                engine::download(&fetcher, &id_for_dl, &url, &dest, handle, limiter, tx, &headers).await
            }
            download_core::model::DownloadKind::Hls => {
                let remux = crate::remux::SidecarRemuxer::new(app_for_dl.clone());
                download_core::hls::engine::download_hls(
                    &fetcher,
                    &remux,
                    &id_for_dl,
                    &url,
                    audio_url.as_deref(),
                    &dest,
                    handle,
                    limiter,
                    tx,
                    &headers,
                )
                .await
            }
            download_core::model::DownloadKind::Dash => {
                let remux = crate::remux::SidecarRemuxer::new(app_for_dl.clone());
                let selection = match media_selection.as_deref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
                    Some(v) => download_core::dash::engine::DashSelection {
                        video_index: v.get("video").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        audio_lang: v.get("audio").and_then(|x| x.as_str()).map(String::from),
                    },
                    None => download_core::dash::engine::DashSelection { video_index: 0, audio_lang: None },
                };
                download_core::dash::engine::download_dash(&fetcher, &remux, &id_for_dl, &url, &selection, &dest, handle, limiter, tx, &headers).await
            }
            download_core::model::DownloadKind::Ytdlp => {
                let job = download_core::ytdlp::job_from_selection(media_selection.as_deref());
                crate::ytdlp::run_download(app_for_dl.clone(), &id_for_dl, &url, &job, &dest, handle, &headers, tx).await
                    .map(|()| download_core::engine::Outcome::Completed)
            }
        };
        match result {
            Ok(_) => {}
            Err(e) => {
                eprintln!("download {id_for_dl} failed: {e}");
                let bytes = handle_for_err.downloaded();
                let st = app_for_dl.state::<AppState>();
                if let Err(se) = st.storage.mark_error(&id_for_dl, bytes, &e.to_string()) {
                    eprintln!("failed to persist error for {id_for_dl}: {se}");
                }
                let _ = app_for_dl.emit("download://progress", &Progress {
                    id: id_for_dl.clone(),
                    downloaded_bytes: bytes,
                    total_bytes: None,
                    speed_bps: 0,
                    status: DownloadStatus::Error,
                    segments_done: None,
                    segments_total: None, parts: None,
                });
                notify_terminal(&app_for_dl, DownloadStatus::Error, &id_for_dl);
                crate::queue::take_hold_on_pause(&app_for_dl, &id_for_dl);
                crate::queue::on_terminal_persisted(&app_for_dl, DownloadStatus::Error, true);
            }
        }
        // Libérer le slot et relancer la file.
        {
            let st = app_for_dl.state::<AppState>();
            st.handles.lock().unwrap().remove(&id_for_dl);
        }
        reevaluate(&app_for_dl);
    });
}

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

/// Indique si l'app est inscrite au démarrage de Windows (lecture du registre via le plugin).
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Active/désactive le démarrage avec Windows. Le plugin enregistre la clé `Run`
/// avec l'argument `--hidden` (configuré à l'init), pour un démarrage caché dans le tray.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}
