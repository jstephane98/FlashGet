//! Logique pure pour l'extracteur yt-dlp (sidecar piloté côté Tauri).
//! Parse la sortie `yt-dlp -J`, sélectionne les variantes, construit l'argument
//! `-f`, et parse les lignes de progression. Aucune dépendance Tauri/process.

use crate::CoreError;
use serde::Deserialize;

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
}

#[derive(Deserialize)]
struct RawInfo {
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

/// Parse la sortie JSON de `yt-dlp -J` : titre + liste des formats.
pub fn parse_formats(json: &str) -> Result<(String, Vec<YtdlpFormat>), CoreError> {
    let info: RawInfo = serde_json::from_str(json)
        .map_err(|e| CoreError::Media(format!("sortie yt-dlp illisible: {e}")))?;
    let title = info.title.unwrap_or_default();
    let name = pick_name(&title, info.uploader.as_deref(), info.id.as_deref());
    let formats = info
        .formats
        .into_iter()
        .map(|f| YtdlpFormat {
            format_id: f.format_id,
            height: f.height,
            width: f.width,
            ext: f.ext,
            vcodec: f.vcodec,
            acodec: f.acodec,
            filesize: f.filesize.or(f.filesize_approx),
            tbr: f.tbr,
        })
        .collect();
    Ok((name, formats))
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

/// Variante neutre côté cœur (mappée vers `MediaVariant` par la couche Tauri).
#[derive(Debug, Clone, PartialEq)]
pub struct YtdlpVariant {
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

/// Variantes sélectionnables : formats porteurs de vidéo (vcodec présent et != "none"),
/// dédupliqués par hauteur, triés par hauteur décroissante. Renvoie le format_id
/// associé à chaque variante (à mémoriser pour le téléchargement).
pub fn select_variants(formats: &[YtdlpFormat]) -> Vec<(String, YtdlpVariant)> {
    let mut seen_heights: Vec<u32> = Vec::new();
    let mut out: Vec<(String, YtdlpVariant)> = Vec::new();

    let mut video: Vec<&YtdlpFormat> = formats
        .iter()
        .filter(|f| f.vcodec.as_deref().map(|c| c != "none").unwrap_or(false))
        .collect();
    video.sort_by(|a, b| b.height.unwrap_or(0).cmp(&a.height.unwrap_or(0)));

    for f in video {
        // Les formats sans hauteur connue ne sont pas dédupliqués (on les garde tous).
        if let Some(h) = f.height {
            if seen_heights.contains(&h) {
                continue;
            }
            seen_heights.push(h);
        }
        let resolution = match (f.width, f.height) {
            (Some(w), Some(h)) => Some(format!("{w}x{h}")),
            (None, Some(h)) => Some(format!("{h}p")),
            _ => None,
        };
        // tbr est en kbps ; un tbr négatif/NaN (métadonnée corrompue) devient 0.
        let bandwidth = f.tbr.map(|t| (t.max(0.0) * 1000.0) as u64).unwrap_or(0);
        out.push((
            f.format_id.clone(),
            YtdlpVariant { resolution, bandwidth, est_size: f.filesize },
        ));
    }
    out
}

/// Argument `-f` : fusionne l'audio si le format est vidéo-seul, avec repli.
pub fn build_format_arg(format_id: &str) -> String {
    format!("{format_id}+bestaudio/{format_id}/best")
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
    fn parse_formats_extracts_title_and_formats() {
        let (title, formats) = parse_formats(SAMPLE).unwrap();
        assert_eq!(title, "Ma vidéo");
        assert_eq!(formats.len(), 3);
        assert_eq!(formats[0].format_id, "18");
        assert_eq!(formats[0].height, Some(360));
        assert_eq!(formats[1].filesize, Some(5000));
        assert_eq!(formats[2].vcodec.as_deref(), Some("none"));
    }

    #[test]
    fn parse_formats_handles_missing_title_and_bad_json() {
        let (title, _) = parse_formats(r#"{"formats":[]}"#).unwrap();
        assert_eq!(title, "");
        assert!(parse_formats("pas du json").is_err());
    }

    #[test]
    fn select_variants_keeps_video_dedups_sorts() {
        let (_t, formats) = parse_formats(SAMPLE).unwrap();
        let v = select_variants(&formats);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, "137");
        assert_eq!(v[0].1.resolution.as_deref(), Some("1920x1080"));
        assert_eq!(v[0].1.bandwidth, 4_000_000);
        assert_eq!(v[0].1.est_size, Some(5000));
        assert_eq!(v[1].0, "18");
    }

    #[test]
    fn select_variants_dedups_same_height() {
        let (_t, mut formats) = parse_formats(SAMPLE).unwrap();
        formats.push(YtdlpFormat {
            format_id: "299".into(), height: Some(1080), width: Some(1920),
            ext: Some("mp4".into()), vcodec: Some("avc1".into()), acodec: Some("none".into()),
            filesize: Some(6000), tbr: Some(4500.0),
        });
        let v = select_variants(&formats);
        let n_1080 = v.iter().filter(|(_, mv)| mv.resolution.as_deref() == Some("1920x1080")).count();
        assert_eq!(n_1080, 1);
    }

    #[test]
    fn build_format_arg_merges_audio() {
        assert_eq!(build_format_arg("137"), "137+bestaudio/137/best");
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
    fn parse_formats_tolerates_absent_formats_key() {
        let (title, formats) = parse_formats(r#"{"title":"x"}"#).unwrap();
        assert_eq!(title, "x");
        assert_eq!(formats.len(), 0);
    }

    #[test]
    fn select_variants_keeps_heightless_video_with_no_resolution() {
        let formats = vec![YtdlpFormat {
            format_id: "0".into(), height: None, width: None,
            ext: Some("mp4".into()), vcodec: Some("avc1".into()), acodec: Some("none".into()),
            filesize: None, tbr: None,
        }];
        let v = select_variants(&formats);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].1.resolution, None);
        assert_eq!(v[0].1.bandwidth, 0);
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
    fn parse_formats_uses_uploader_id_when_no_title() {
        let json = r#"{
            "id": "987",
            "uploader": "Ma Page FB",
            "formats": [
                {"format_id":"hd","height":720,"width":1280,"ext":"mp4","vcodec":"avc1","acodec":"mp4a","filesize":1000,"tbr":900.0}
            ]
        }"#;
        let (name, formats) = parse_formats(json).unwrap();
        assert_eq!(name, "Ma Page FB - 987");
        assert_eq!(formats.len(), 1);
    }
}
