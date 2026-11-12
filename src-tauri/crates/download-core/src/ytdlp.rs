//! Logique pure pour l'extracteur yt-dlp (sidecar piloté côté Tauri).
//! Parse la sortie `yt-dlp -J`, construit les formats proposés et les arguments
//! de téléchargement, et parse les lignes de progression. Aucune dépendance Tauri/process.

use crate::model::AudioFormat;
use crate::CoreError;
use serde::{Deserialize, Serialize};

/// Un format yt-dlp pertinent extrait de `yt-dlp -J`.
#[derive(Debug, Clone, PartialEq)]
pub struct YtdlpFormat {
    pub format_id: String,
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub ext: Option<String>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub tbr: Option<f64>,
    pub fps: Option<f32>,
    pub abr: Option<f64>,
}

#[derive(Deserialize)]
struct RawInfo {
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    upload_date: Option<String>,
    #[serde(default)]
    subtitles: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    formats: Vec<RawFormat>,
}

#[derive(Deserialize)]
struct RawFormat {
    format_id: String,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    ext: Option<String>,
    #[serde(default)]
    vcodec: Option<String>,
    #[serde(default)]
    acodec: Option<String>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    filesize_approx: Option<u64>,
    #[serde(default)]
    tbr: Option<f64>,
    #[serde(default)]
    fps: Option<f32>,
    #[serde(default)]
    abr: Option<f64>,
}

impl From<RawFormat> for YtdlpFormat {
    fn from(f: RawFormat) -> Self {
        YtdlpFormat {
            format_id: f.format_id,
            height: f.height,
            width: f.width,
            ext: f.ext,
            vcodec: f.vcodec,
            acodec: f.acodec,
            filesize: f.filesize.or(f.filesize_approx),
            tbr: f.tbr,
            fps: f.fps,
            abr: f.abr,
        }
    }
}

/// Choisit un nom d'affichage à partir des champs yt-dlp.
/// Ordre : `title` (non vide, trimé) → `"uploader - id"` → `"uploader"` → `id` → `""`.
/// `uploader`/`id` ne comptent que s'ils sont non vides après `trim`.
pub fn pick_name(title: &str, uploader: Option<&str>, id: Option<&str>) -> String {
    let t = title.trim();
    if !t.is_empty() {
        return t.to_string();
    }
    let up = uploader.map(str::trim).filter(|s| !s.is_empty());
    let id = id.map(str::trim).filter(|s| !s.is_empty());
    match (up, id) {
        (Some(up), Some(id)) => format!("{up} - {id}"),
        (Some(up), None) => up.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => String::new(),
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FormatKind {
    Av,
    Video,
    Audio,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MediaFormat {
    pub index: u32,
    pub kind: FormatKind,
    pub format_id: String,
    pub height: Option<u32>,
    pub label: String,
    pub container: Option<String>,
    pub codecs: Option<String>,
    pub fps: Option<f32>,
    pub est_size: Option<u64>,
    pub recommended: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct MediaInfo {
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub duration_s: Option<u64>,
    pub upload_date: Option<String>,
    pub subtitle_langs: Vec<String>,
    pub can_cut: bool,
}

/// Parse `yt-dlp -J` : métadonnées (`MediaInfo`) + formats bruts.
pub fn parse_info(json: &str) -> Result<(MediaInfo, Vec<YtdlpFormat>), CoreError> {
    let info: RawInfo = serde_json::from_str(json)
        .map_err(|e| CoreError::Media(format!("sortie yt-dlp illisible: {e}")))?;
    let title = pick_name(
        info.title.as_deref().unwrap_or(""),
        info.uploader.as_deref(),
        info.id.as_deref(),
    );
    let upload_date = info.upload_date.as_deref().and_then(|d| {
        let d = d.trim();
        (d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()))
            .then(|| format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]))
    });
    let mut subtitle_langs: Vec<String> = info
        .subtitles
        .map(|m| m.into_keys().filter(|k| k != "live_chat").collect())
        .unwrap_or_default();
    subtitle_langs.sort();
    let media = MediaInfo {
        title,
        uploader: info.uploader.filter(|u| !u.trim().is_empty()),
        thumbnail: info.thumbnail.filter(|t| !t.is_empty()),
        duration_s: info.duration.filter(|d| *d >= 0.0).map(|d| d.round() as u64),
        upload_date,
        subtitle_langs,
        can_cut: true,
    };
    let formats = info.formats.into_iter().map(YtdlpFormat::from).collect();
    Ok((media, formats))
}

/// Libellé court d'un codec yt-dlp.
pub fn codec_label(codec: &str) -> String {
    let c = codec.trim();
    if c.starts_with("avc1") {
        "H.264".into()
    } else if c == "vp9" || c.starts_with("vp09") {
        "VP9".into()
    } else if c.starts_with("av01") {
        "AV1".into()
    } else if c.starts_with("hev1") || c.starts_with("hvc1") {
        "H.265".into()
    } else if c.starts_with("mp4a") {
        "AAC".into()
    } else if c == "opus" {
        "Opus".into()
    } else if c == "vorbis" {
        "Vorbis".into()
    } else {
        c.split('.').next().unwrap_or("").to_uppercase()
    }
}

fn has_codec(c: &Option<String>) -> bool {
    c.as_deref().map(|c| c != "none" && !c.is_empty()).unwrap_or(false)
}

fn abr_of(f: &YtdlpFormat) -> f64 {
    f.abr.or(f.tbr).unwrap_or(0.0)
}

/// Identifiant de la ligne synthétique « conversion audio » (format des réglages).
pub const CONVERT_FORMAT_ID: &str = "convert";

/// Préférences des réglages qui orientent les formats proposés.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaPrefs {
    /// Hauteur vidéo recommandée maximale (ex. 1080).
    pub max_height: u32,
    pub audio_format: AudioFormat,
    /// Débit de la conversion en kb/s (0 = meilleure qualité).
    pub audio_kbps: u32,
}

impl Default for MediaPrefs {
    fn default() -> Self {
        MediaPrefs { max_height: 1080, audio_format: AudioFormat::Mp3, audio_kbps: 0 }
    }
}

/// Extension (et `--audio-format` yt-dlp) d'un format de conversion.
pub fn audio_format_ext(f: AudioFormat) -> &'static str {
    match f {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::M4a => "m4a",
        AudioFormat::Opus => "opus",
    }
}

fn audio_format_label(f: AudioFormat) -> &'static str {
    match f {
        AudioFormat::Mp3 => "MP3",
        AudioFormat::M4a => "M4A",
        AudioFormat::Opus => "Opus",
    }
}

/// Liste ordonnée des formats proposés : Av, puis Video, puis Audio (conversion d'abord).
/// Recommandé : la plus grande hauteur <= `prefs.max_height`, à défaut la plus petite.
pub fn build_media_formats(formats: &[YtdlpFormat], prefs: &MediaPrefs) -> Vec<MediaFormat> {
    // Audio seul, meilleur abr en premier.
    let audio_only: Vec<&YtdlpFormat> = formats
        .iter()
        .filter(|f| has_codec(&f.acodec) && !has_codec(&f.vcodec))
        .collect();
    let best_audio = audio_only
        .iter()
        .copied()
        .max_by(|a, b| abr_of(a).partial_cmp(&abr_of(b)).unwrap_or(std::cmp::Ordering::Equal));

    // Un format vidéo par hauteur : préférence avc1, puis tbr maximal.
    let mut best_by_height: Vec<(u32, &YtdlpFormat)> = Vec::new();
    for f in formats.iter().filter(|f| has_codec(&f.vcodec)) {
        let Some(h) = f.height else { continue };
        let rank = |f: &YtdlpFormat| {
            (
                f.vcodec.as_deref().unwrap_or("").starts_with("avc1"),
                f.tbr.unwrap_or(0.0),
            )
        };
        match best_by_height.iter_mut().find(|(bh, _)| *bh == h) {
            Some(slot) => {
                let (a_avc, a_tbr) = rank(f);
                let (b_avc, b_tbr) = rank(slot.1);
                if (a_avc, a_tbr) > (b_avc, b_tbr) {
                    slot.1 = f;
                }
            }
            None => best_by_height.push((h, f)),
        }
    }
    best_by_height.sort_by(|a, b| b.0.cmp(&a.0));
    let rec_height = best_by_height
        .iter()
        .map(|(h, _)| *h)
        .find(|h| *h <= prefs.max_height)
        .or_else(|| best_by_height.last().map(|(h, _)| *h));

    let mut out: Vec<MediaFormat> = Vec::new();
    let mut push = |out: &mut Vec<MediaFormat>, mut m: MediaFormat| {
        m.index = out.len() as u32;
        out.push(m);
    };

    for (h, f) in &best_by_height {
        let vlabel = f.vcodec.as_deref().map(codec_label).unwrap_or_default();
        let codecs = match best_audio.and_then(|a| a.acodec.as_deref()) {
            Some(ac) => format!("{vlabel} + {}", codec_label(ac)),
            None => vlabel,
        };
        let est_size = match (f.filesize, best_audio.and_then(|a| a.filesize)) {
            (Some(v), Some(a)) => Some(v + a),
            _ => None,
        };
        push(&mut out, MediaFormat {
            index: 0,
            kind: FormatKind::Av,
            format_id: f.format_id.clone(),
            height: Some(*h),
            label: format!("{h}p"),
            container: f.ext.as_ref().map(|e| e.to_uppercase()),
            codecs: Some(codecs),
            fps: f.fps,
            est_size,
            recommended: rec_height == Some(*h),
        });
    }
    for (h, f) in &best_by_height {
        push(&mut out, MediaFormat {
            index: 0,
            kind: FormatKind::Video,
            format_id: f.format_id.clone(),
            height: Some(*h),
            label: format!("{h}p"),
            container: f.ext.as_ref().map(|e| e.to_uppercase()),
            codecs: f.vcodec.as_deref().map(codec_label),
            fps: f.fps,
            est_size: f.filesize,
            recommended: rec_height == Some(*h),
        });
    }
    // Aucune hauteur connue mais au moins un flux vidéo (vcodec absent/inconnu compris) :
    // une seule ligne « Meilleure qualité » (`bestvideo` → `-f bestvideo+bestaudio/best`).
    if best_by_height.is_empty() && formats.iter().any(|f| f.vcodec.as_deref() != Some("none")) {
        push(&mut out, MediaFormat {
            index: 0,
            kind: FormatKind::Av,
            format_id: "bestvideo".into(),
            height: None,
            label: crate::i18n::tr("Meilleure qualité", "Best quality").into(),
            container: None,
            codecs: None,
            fps: None,
            est_size: None,
            recommended: true,
        });
    }

    let conv = audio_format_label(prefs.audio_format);
    push(&mut out, MediaFormat {
        index: 0,
        kind: FormatKind::Audio,
        format_id: CONVERT_FORMAT_ID.into(),
        height: None,
        label: if prefs.audio_kbps > 0 { format!("{conv} · {} kb/s", prefs.audio_kbps) } else { conv.into() },
        container: Some(audio_format_ext(prefs.audio_format).to_uppercase()),
        codecs: Some(format!("{conv} (conversion)")),
        fps: None,
        est_size: None,
        recommended: true,
    });
    // Meilleur format audio par extension, dans l'ordre de première apparition.
    let mut by_ext: Vec<(String, &YtdlpFormat)> = Vec::new();
    for f in &audio_only {
        let ext = f.ext.clone().unwrap_or_default();
        match by_ext.iter_mut().find(|(e, _)| *e == ext) {
            Some(slot) => {
                if abr_of(f) > abr_of(slot.1) {
                    slot.1 = f;
                }
            }
            None => by_ext.push((ext, f)),
        }
    }
    for (ext, f) in by_ext {
        let up = ext.to_uppercase();
        push(&mut out, MediaFormat {
            index: 0,
            kind: FormatKind::Audio,
            format_id: f.format_id.clone(),
            height: None,
            label: format!("{up} · {} kb/s", abr_of(f).round() as u64),
            container: Some(up),
            codecs: f.acodec.as_deref().map(codec_label),
            fps: None,
            est_size: f.filesize,
            recommended: false,
        });
    }
    out
}

/// Préfixe de progression émis via `--progress-template` (voir couche Tauri).
pub const PROGRESS_PREFIX: &str = "FGPROG:";

/// Parse une ligne de progression `FGPROG:<downloaded>/<total>`.
/// `<total>` peut être `NA` ou vide (total inconnu → `None`).
/// Renvoie `None` pour toute autre ligne.
pub fn parse_progress(line: &str) -> Option<(u64, Option<u64>)> {
    let rest = line.trim().strip_prefix(PROGRESS_PREFIX)?;
    let (dl, total) = rest.split_once('/')?;
    let downloaded: u64 = dl.trim().parse().ok()?;
    let total = total.trim().parse::<u64>().ok();
    Some((downloaded, total))
}

/// Mode de téléchargement d'une tâche yt-dlp.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobMode {
    #[default]
    Av,
    Video,
    Audio,
}

/// Tâche yt-dlp paramétrée, sérialisée dans `Download.media_selection`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct YtdlpJob {
    pub format_id: String,
    #[serde(default)]
    pub mode: JobMode,
    #[serde(default)]
    pub time_range: Option<String>,
    #[serde(default)]
    pub subs: Option<Vec<String>>,
    /// Conversion audio (`-x`) ; le nom date de l'époque où seul le MP3 existait.
    #[serde(default)]
    pub convert_mp3: bool,
    /// Format de conversion (`mp3`, `m4a`, `opus`) ; absent (anciennes tâches) = `mp3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_format: Option<String>,
    /// Débit de conversion en kb/s ; absent ou 0 = meilleure qualité.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_kbps: Option<u32>,
}

/// Langues de sous-titres préférées par défaut.
pub const PREFERRED_SUB_LANGS: &[&str] = &["fr", "en"];

/// Lit `Download.media_selection` (compatible avec les anciennes lignes sans `mode`, et
/// les très anciennes qui ne stockaient que l'identifiant de format : `137`, `bestaudio`
/// ou `"137"`).
pub fn job_from_selection(json: Option<&str>) -> YtdlpJob {
    let default = || YtdlpJob { format_id: "bestvideo".into(), mode: JobMode::Av, ..Default::default() };
    let Some(raw) = json.map(str::trim).filter(|r| !r.is_empty()) else { return default() };
    let v = serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
    let bare = |id: String| YtdlpJob { format_id: id, ..default() };
    let mut job = match &v {
        serde_json::Value::Object(_) => match serde_json::from_value::<YtdlpJob>(v.clone()) {
            Ok(job) => job,
            Err(_) => return default(),
        },
        serde_json::Value::String(s) if !s.trim().is_empty() => bare(s.trim().to_string()),
        serde_json::Value::Number(n) => bare(n.to_string()),
        _ => return default(),
    };
    if v.get("mode").is_none() && job.format_id == "bestaudio" {
        job.mode = JobMode::Audio;
        job.convert_mp3 = true;
    }
    job
}

/// Options yt-dlp (format, sortie, sous-titres, découpe) pour une tâche.
pub fn job_args(job: &YtdlpJob) -> Vec<String> {
    let id = &job.format_id;
    let mut a: Vec<String> = Vec::new();
    let mut push = |xs: &[&str]| a.extend(xs.iter().map(|s| s.to_string()));
    match job.mode {
        JobMode::Av => {
            if id == "bestvideo" {
                push(&["-f", "bestvideo+bestaudio/best"]);
            } else {
                push(&["-f", &format!("{id}+bestaudio/{id}/best")]);
            }
            push(&["--merge-output-format", "mp4"]);
        }
        JobMode::Video => push(&["-f", id, "--remux-video", "mp4"]),
        // `/best` : certains sites n'offrent que des formats combinés (pas d'audio seul).
        JobMode::Audio if job.convert_mp3 => {
            // Liste blanche : une valeur inconnue (ligne modifiée à la main) retombe sur mp3.
            let fmt = match job.audio_format.as_deref() {
                Some(f @ ("mp3" | "m4a" | "opus")) => f,
                _ => "mp3",
            };
            let quality = match job.audio_kbps {
                Some(k) if k > 0 => format!("{k}K"),
                _ => "0".into(),
            };
            push(&["-f", "bestaudio/best", "-x", "--audio-format", fmt, "--audio-quality", &quality]);
        }
        JobMode::Audio => push(&["-f", id]),
    }
    if let Some(subs) = job.subs.as_ref().filter(|s| !s.is_empty()) {
        push(&["--write-subs", "--sub-langs", &subs.join(","), "--embed-subs"]);
    }
    if let Some(range) = &job.time_range {
        push(&["--download-sections", range]);
    }
    a
}

/// Langues préférées présentes (dans l'ordre des préférées) ; à défaut la première disponible.
pub fn pick_sub_langs<S: AsRef<str>>(available: &[String], preferred: &[S]) -> Vec<String> {
    let picked: Vec<String> = preferred
        .iter()
        .map(|p| p.as_ref())
        .filter(|p| available.iter().any(|a| a == p))
        .map(|p| p.to_string())
        .collect();
    if !picked.is_empty() {
        return picked;
    }
    available.first().cloned().into_iter().collect()
}

/// `"90"`, `"1:30"`, `"01:02:03"` → secondes.
pub fn parse_clock(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut nums = Vec::new();
    for p in &parts {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        nums.push(p.parse::<u64>().ok()?);
    }
    if nums.len() > 1 && nums[1..].iter().any(|n| *n >= 60) {
        return None;
    }
    Some(nums.iter().fold(0, |acc, n| acc * 60 + n))
}

/// Section yt-dlp `*début-fin` en secondes entières.
pub fn time_range(start_s: u64, end_s: Option<u64>) -> String {
    match end_s {
        Some(e) => format!("*{start_s}-{e}"),
        None => format!("*{start_s}-inf"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "title": "Ma vidéo",
        "formats": [
            {"format_id":"18","height":360,"width":640,"ext":"mp4","vcodec":"avc1","acodec":"mp4a","filesize":1000,"tbr":500.0},
            {"format_id":"137","height":1080,"width":1920,"ext":"mp4","vcodec":"avc1","acodec":"none","filesize_approx":5000,"tbr":4000.0},
            {"format_id":"140","ext":"m4a","vcodec":"none","acodec":"mp4a","tbr":128.0}
        ]
    }"#;

    #[test]
    fn parse_info_reads_formats_and_filesize_approx() {
        let (info, formats) = parse_info(SAMPLE).unwrap();
        assert_eq!(info.title, "Ma vidéo");
        assert_eq!(formats.len(), 3);
        assert_eq!(formats[0].format_id, "18");
        assert_eq!(formats[0].height, Some(360));
        assert_eq!(formats[1].filesize, Some(5000));
        assert_eq!(formats[2].vcodec.as_deref(), Some("none"));
    }

    #[test]
    fn parse_info_handles_missing_title_and_bad_json() {
        let (info, _) = parse_info(r#"{"formats":[]}"#).unwrap();
        assert_eq!(info.title, "");
        assert!(parse_info("pas du json").is_err());
    }

    #[test]
    fn parse_progress_reads_sentinel_line() {
        assert_eq!(parse_progress("FGPROG:1048576/5242880"), Some((1048576, Some(5242880))));
    }

    #[test]
    fn parse_progress_total_unknown() {
        assert_eq!(parse_progress("FGPROG:1048576/NA"), Some((1048576, None)));
        assert_eq!(parse_progress("FGPROG:2048/"), Some((2048, None)));
    }

    #[test]
    fn parse_progress_ignores_other_lines() {
        assert_eq!(parse_progress("[download] Destination: x.mp4"), None);
        assert_eq!(parse_progress("FGPROG:bad"), None);
    }

    #[test]
    fn parse_info_tolerates_absent_formats_key() {
        let (info, formats) = parse_info(r#"{"title":"x"}"#).unwrap();
        assert_eq!(info.title, "x");
        assert_eq!(formats.len(), 0);
    }

    const INFO_SAMPLE: &str = r#"{
        "title": "Ma vidéo", "uploader": "Chaîne", "thumbnail": "https://img/x.jpg",
        "duration": 768.4, "upload_date": "20261102",
        "subtitles": {"fr": [{"ext":"vtt"}], "en": [{"ext":"vtt"}]},
        "formats": [
            {"format_id":"137","height":1080,"ext":"mp4","vcodec":"avc1.640028","acodec":"none","filesize":1000000,"tbr":4000.0,"fps":60},
            {"format_id":"248","height":1080,"ext":"webm","vcodec":"vp9","acodec":"none","tbr":5000.0},
            {"format_id":"22","height":720,"ext":"mp4","vcodec":"avc1.64001F","acodec":"none","filesize":500000},
            {"format_id":"313","height":2160,"ext":"webm","vcodec":"vp9","acodec":"none","filesize":4000000},
            {"format_id":"140","ext":"m4a","vcodec":"none","acodec":"mp4a.40.2","abr":129,"filesize":100000},
            {"format_id":"251","ext":"webm","vcodec":"none","acodec":"opus","abr":160,"filesize":120000},
            {"format_id":"250","ext":"webm","vcodec":"none","acodec":"opus","abr":70}
        ]
    }"#;

    #[test]
    fn parse_info_reads_metadata() {
        let (info, f) = parse_info(INFO_SAMPLE).unwrap();
        assert_eq!(info.title, "Ma vidéo");
        assert_eq!(info.duration_s, Some(768));
        assert_eq!(info.upload_date.as_deref(), Some("2026-11-02"));
        assert_eq!(info.subtitle_langs, vec!["en", "fr"]);
        assert!(info.can_cut && info.thumbnail.is_some() && f.len() == 7);
    }

    #[test]
    fn av_formats_one_per_height_prefer_h264() {
        let (_, f) = parse_info(INFO_SAMPLE).unwrap();
        let av: Vec<_> = build_media_formats(&f, &MediaPrefs::default()).into_iter().filter(|m| m.kind == FormatKind::Av).collect();
        assert_eq!(av.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["2160p", "1080p", "720p"]);
        let p1080 = &av[1];
        assert_eq!(p1080.format_id, "137");
        assert_eq!(p1080.codecs.as_deref(), Some("H.264 + Opus"));
        assert_eq!(p1080.est_size, Some(1_120_000));
        assert_eq!(p1080.fps, Some(60.0));
        assert!(p1080.recommended && !av[0].recommended);
    }

    #[test]
    fn video_formats_follow_av_with_same_heights() {
        let (_, f) = parse_info(INFO_SAMPLE).unwrap();
        let v: Vec<_> = build_media_formats(&f, &MediaPrefs::default()).into_iter().filter(|m| m.kind == FormatKind::Video).collect();
        assert_eq!(v.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["2160p", "1080p", "720p"]);
        assert_eq!(v[1].codecs.as_deref(), Some("H.264"));
        assert_eq!(v[1].est_size, Some(1_000_000));
        assert!(v[1].recommended && !v[0].recommended);
    }

    #[test]
    fn audio_formats_convert_first_then_native_by_ext() {
        let (_, f) = parse_info(INFO_SAMPLE).unwrap();
        let au: Vec<_> = build_media_formats(&f, &MediaPrefs::default()).into_iter().filter(|m| m.kind == FormatKind::Audio).collect();
        assert_eq!(au[0].format_id, "convert");
        assert!(au[0].recommended);
        assert_eq!(au.iter().skip(1).map(|m| m.label.as_str()).collect::<Vec<_>>(), ["M4A · 129 kb/s", "WEBM · 160 kb/s"]);
    }

    fn heightless(id: &str, vcodec: Option<&str>, acodec: Option<&str>, ext: &str) -> YtdlpFormat {
        YtdlpFormat {
            format_id: id.into(), height: None, width: None,
            ext: Some(ext.into()), vcodec: vcodec.map(String::from), acodec: acodec.map(String::from),
            filesize: None, tbr: None, fps: None, abr: None,
        }
    }

    #[test]
    fn heightless_video_gets_one_best_quality_row() {
        let f = build_media_formats(&[
            heightless("0", Some("avc1"), Some("mp4a"), "mp4"),
            heightless("1", Some("avc1"), Some("none"), "mp4"),
        ], &MediaPrefs::default());
        let av: Vec<_> = f.iter().filter(|m| m.kind == FormatKind::Av).collect();
        assert_eq!(av.len(), 1);
        let row = av[0];
        assert_eq!(row.format_id, "bestvideo");
        assert_eq!(row.label, "Meilleure qualité");
        assert_eq!((row.height, row.container.as_deref(), row.codecs.as_deref(), row.fps, row.est_size), (None, None, None, None, None));
        assert!(row.recommended);
        assert_eq!(row.index, 0);
        assert!(!f.iter().any(|m| m.kind == FormatKind::Video));
        // Les lignes audio suivent, indexes = positions.
        assert_eq!(f[1].format_id, "convert");
        for (i, m) in f.iter().enumerate() {
            assert_eq!(m.index as usize, i);
        }
        // vcodec absent (extracteur générique) = vidéo aussi.
        let g = build_media_formats(&[heightless("direct", None, None, "mp4")], &MediaPrefs::default());
        assert_eq!(g[0].format_id, "bestvideo");
    }

    #[test]
    fn storyboards_alone_give_no_video_row() {
        let f = build_media_formats(&[
            heightless("sb0", Some("none"), Some("none"), "mhtml"),
            heightless("sb1", Some("none"), Some("none"), "mhtml"),
        ], &MediaPrefs::default());
        assert!(!f.iter().any(|m| matches!(m.kind, FormatKind::Av | FormatKind::Video)));
        assert!(!f.iter().any(|m| m.format_id != CONVERT_FORMAT_ID));
        // Rien du tout → pas de ligne vidéo non plus.
        assert!(!build_media_formats(&[], &MediaPrefs::default()).iter().any(|m| m.kind == FormatKind::Av));
        // Des hauteurs connues → pas de ligne « Meilleure qualité ».
        let (_, info) = parse_info(INFO_SAMPLE).unwrap();
        assert!(!build_media_formats(&info, &MediaPrefs::default()).iter().any(|m| m.format_id == "bestvideo"));
    }

    #[test]
    fn indexes_are_positions() {
        let (_, f) = parse_info(INFO_SAMPLE).unwrap();
        for (i, m) in build_media_formats(&f, &MediaPrefs::default()).iter().enumerate() {
            assert_eq!(m.index as usize, i);
        }
    }

    #[test]
    fn codec_labels() {
        assert_eq!(codec_label("avc1.640028"), "H.264");
        assert_eq!(codec_label("vp09.00.40.08"), "VP9");
        assert_eq!(codec_label("av01.0.08M.08"), "AV1");
        assert_eq!(codec_label("hev1.1.6"), "H.265");
        assert_eq!(codec_label("mp4a.40.2"), "AAC");
        assert_eq!(codec_label("opus"), "Opus");
        assert_eq!(codec_label("vorbis"), "Vorbis");
        assert_eq!(codec_label("flac"), "FLAC");
    }

    #[test]
    fn pick_name_prefers_title() {
        assert_eq!(pick_name("Ma vidéo", Some("Chaîne"), Some("id1")), "Ma vidéo");
        assert_eq!(pick_name("  Ma vidéo  ", None, None), "Ma vidéo");
    }

    #[test]
    fn pick_name_falls_back_to_uploader_and_id() {
        assert_eq!(pick_name("", Some("Ma Page"), Some("123")), "Ma Page - 123");
        assert_eq!(pick_name("   ", Some("Ma Page"), Some("123")), "Ma Page - 123");
    }

    #[test]
    fn pick_name_uploader_only_has_no_orphan_dash() {
        assert_eq!(pick_name("", Some("Ma Page"), None), "Ma Page");
        assert_eq!(pick_name("", Some("  "), Some("123")), "123");
    }

    #[test]
    fn pick_name_id_only_then_empty() {
        assert_eq!(pick_name("", None, Some("123")), "123");
        assert_eq!(pick_name("", None, None), "");
        assert_eq!(pick_name("", Some(""), None), "");
    }

    #[test]
    fn parse_info_uses_uploader_id_when_no_title() {
        let json = r#"{
            "id": "987",
            "uploader": "Ma Page FB",
            "formats": [
                {"format_id":"hd","height":720,"width":1280,"ext":"mp4","vcodec":"avc1","acodec":"mp4a","filesize":1000,"tbr":900.0}
            ]
        }"#;
        let (info, formats) = parse_info(json).unwrap();
        assert_eq!(info.title, "Ma Page FB - 987");
        assert_eq!(formats.len(), 1);
    }

    #[test] fn legacy_bestaudio_selection_is_mp3() { let j = job_from_selection(Some(r#"{"format_id":"bestaudio","time_range":null}"#)); assert_eq!((j.mode, j.convert_mp3), (JobMode::Audio, true)); }
    #[test] fn legacy_video_selection_is_av() { let j = job_from_selection(Some(r#"{"format_id":"137","time_range":"*0-30"}"#)); assert_eq!(j.mode, JobMode::Av); assert_eq!(j.time_range.as_deref(), Some("*0-30")); }
    #[test] fn missing_selection_is_best_av() { assert_eq!(job_from_selection(None).format_id, "bestvideo"); assert_eq!(job_from_selection(Some("garbage")).mode, JobMode::Av); }
    #[test] fn args_legacy_bestvideo() {
        let j = job_from_selection(Some(r#"{"format_id":"bestvideo","time_range":null}"#));
        assert_eq!(job_args(&j), ["-f","bestvideo+bestaudio/best","--merge-output-format","mp4"]);
    }
    #[test] fn args_av_with_subs_and_cut() {
        let j = YtdlpJob { format_id: "137".into(), mode: JobMode::Av, time_range: Some("*90-165".into()), subs: Some(vec!["fr".into(), "en".into()]), convert_mp3: false, ..Default::default() };
        assert_eq!(job_args(&j), ["-f","137+bestaudio/137/best","--merge-output-format","mp4","--write-subs","--sub-langs","fr,en","--embed-subs","--download-sections","*90-165"]);
    }
    #[test] fn args_video_only() {
        let j = YtdlpJob { format_id: "137".into(), mode: JobMode::Video, time_range: None, subs: None, convert_mp3: false, ..Default::default() };
        assert_eq!(job_args(&j), ["-f","137","--remux-video","mp4"]);
    }
    #[test] fn args_mp3() {
        let j = YtdlpJob { format_id: "bestaudio".into(), mode: JobMode::Audio, time_range: None, subs: Some(vec![]), convert_mp3: true, ..Default::default() };
        assert_eq!(job_args(&j), ["-f","bestaudio/best","-x","--audio-format","mp3","--audio-quality","0"]);
    }
    #[test] fn legacy_bare_format_id_selection() {
        // Nombre JSON nu.
        let j = job_from_selection(Some("137"));
        assert_eq!((j.format_id.as_str(), j.mode, j.convert_mp3), ("137", JobMode::Av, false));
        // Chaîne non JSON.
        let j = job_from_selection(Some("bestaudio"));
        assert_eq!((j.format_id.as_str(), j.mode, j.convert_mp3), ("bestaudio", JobMode::Audio, true));
        // Chaîne JSON.
        let j = job_from_selection(Some(r#""137""#));
        assert_eq!((j.format_id.as_str(), j.mode, j.convert_mp3), ("137", JobMode::Av, false));
        let j = job_from_selection(Some(r#""bestaudio""#));
        assert_eq!((j.mode, j.convert_mp3), (JobMode::Audio, true));
        // Vide : meilleure qualité.
        assert_eq!(job_from_selection(Some("  ")).format_id, "bestvideo");
    }
    #[test] fn args_native_audio() {
        let j = YtdlpJob { format_id: "140".into(), mode: JobMode::Audio, time_range: None, subs: None, convert_mp3: false, ..Default::default() };
        assert_eq!(job_args(&j), ["-f","140"]);
    }
    #[test] fn sub_langs() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick_sub_langs(&a(&["de","en","fr"]), PREFERRED_SUB_LANGS), ["fr","en"]);
        assert_eq!(pick_sub_langs(&a(&["de","it"]), PREFERRED_SUB_LANGS), ["de"]);
        assert!(pick_sub_langs(&[], PREFERRED_SUB_LANGS).is_empty());
    }
    #[test] fn clock() { assert_eq!(parse_clock("90"), Some(90)); assert_eq!(parse_clock("1:30"), Some(90)); assert_eq!(parse_clock("01:02:03"), Some(3723)); assert_eq!(parse_clock("1:75"), None); assert_eq!(parse_clock(""), None); assert_eq!(parse_clock("abc"), None); }
    #[test] fn range() { assert_eq!(time_range(90, Some(165)), "*90-165"); assert_eq!(time_range(0, None), "*0-inf"); }
    fn tall(id: &str, h: u32) -> YtdlpFormat {
        YtdlpFormat { height: Some(h), ..heightless(id, Some("avc1"), Some("none"), "mp4") }
    }
    fn prefs(max_height: u32) -> MediaPrefs {
        MediaPrefs { max_height, ..MediaPrefs::default() }
    }
    fn recommended_av(f: &[YtdlpFormat], p: &MediaPrefs) -> Vec<String> {
        build_media_formats(f, p).into_iter().filter(|m| m.kind == FormatKind::Av && m.recommended).map(|m| m.label).collect()
    }

    #[test]
    fn recommended_respects_max_height() {
        let (_, f) = parse_info(INFO_SAMPLE).unwrap();
        assert_eq!(recommended_av(&f, &prefs(720)), ["720p"]);
        assert_eq!(recommended_av(&f, &prefs(2160)), ["2160p"]);
        assert_eq!(recommended_av(&f, &prefs(1080)), ["1080p"]);
        // Entre deux hauteurs : la plus grande en dessous.
        assert_eq!(recommended_av(&f, &prefs(1440)), ["1080p"]);
        // Toutes au-dessus du plafond : la plus petite disponible.
        let big = [tall("a", 2160), tall("b", 1440)];
        assert_eq!(recommended_av(&big, &prefs(1080)), ["1440p"]);
        let v: Vec<_> = build_media_formats(&big, &prefs(1080)).into_iter().filter(|m| m.kind == FormatKind::Video && m.recommended).map(|m| m.label).collect();
        assert_eq!(v, ["1440p"]);
    }

    #[test]
    fn convert_row_follows_audio_prefs() {
        let (_, f) = parse_info(INFO_SAMPLE).unwrap();
        let first_audio = |p: &MediaPrefs| build_media_formats(&f, p).into_iter().find(|m| m.kind == FormatKind::Audio).unwrap();
        let m4a = first_audio(&MediaPrefs { audio_format: AudioFormat::M4a, audio_kbps: 256, ..MediaPrefs::default() });
        assert_eq!(m4a.format_id, CONVERT_FORMAT_ID);
        assert_eq!(m4a.format_id, "convert");
        assert_eq!(m4a.label, "M4A · 256 kb/s");
        assert_eq!(m4a.container.as_deref(), Some("M4A"));
        assert!(m4a.recommended);
        let opus = first_audio(&MediaPrefs { audio_format: AudioFormat::Opus, audio_kbps: 0, ..MediaPrefs::default() });
        assert_eq!((opus.label.as_str(), opus.container.as_deref()), ("Opus", Some("OPUS")));
        let mp3 = first_audio(&MediaPrefs::default());
        assert_eq!((mp3.label.as_str(), mp3.container.as_deref(), mp3.codecs.as_deref()), ("MP3", Some("MP3"), Some("MP3 (conversion)")));
    }

    #[test]
    fn job_args_audio_formats() {
        let conv = |fmt: Option<&str>, kbps: Option<u32>| YtdlpJob {
            format_id: "bestaudio".into(), mode: JobMode::Audio, time_range: None, subs: None, convert_mp3: true,
            audio_format: fmt.map(String::from), audio_kbps: kbps,
        };
        assert_eq!(job_args(&conv(Some("opus"), Some(0))), ["-f","bestaudio/best","-x","--audio-format","opus","--audio-quality","0"]);
        assert_eq!(job_args(&conv(Some("m4a"), Some(256))), ["-f","bestaudio/best","-x","--audio-format","m4a","--audio-quality","256K"]);
        // Valeur inconnue : mp3.
        assert_eq!(job_args(&conv(Some("wav; rm"), None))[4], "mp3");
        // Ancienne tâche : `convert_mp3` sans format ni débit → mp3, qualité 0.
        let old = job_from_selection(Some(r#"{"format_id":"bestaudio","mode":"audio","time_range":null,"subs":null,"convert_mp3":true}"#));
        assert_eq!((old.audio_format.as_deref(), old.audio_kbps), (None, None));
        assert_eq!(job_args(&old), ["-f","bestaudio/best","-x","--audio-format","mp3","--audio-quality","0"]);
        // Les champs absents ne sont pas sérialisés (lignes compatibles avec l'ancien format).
        let s = serde_json::to_string(&old).unwrap();
        assert!(!s.contains("audio_format") && !s.contains("audio_kbps"), "{s}");
    }

    #[test]
    fn sub_langs_from_settings() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick_sub_langs(&a(&["en","de"]), &a(&["de"])), ["de"]);
        assert_eq!(pick_sub_langs(&a(&["en","de"]), &a(&["it","en","de"])), ["en","de"]);
    }

    #[test] fn parse_info_excludes_live_chat() {
        let j = r#"{"title":"t","subtitles":{"fr":[{"ext":"vtt"}],"live_chat":[{"ext":"json"}]},"formats":[]}"#;
        let (info, _) = parse_info(j).unwrap();
        assert_eq!(info.subtitle_langs, vec!["fr"]);
    }
}
