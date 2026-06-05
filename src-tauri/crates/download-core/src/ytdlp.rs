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

/// Parse la sortie JSON de `yt-dlp -J` : titre + liste des formats.
pub fn parse_formats(json: &str) -> Result<(String, Vec<YtdlpFormat>), CoreError> {
    let info: RawInfo = serde_json::from_str(json)
        .map_err(|e| CoreError::Media(format!("sortie yt-dlp illisible: {e}")))?;
    let title = info.title.unwrap_or_default();
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
    Ok((title, formats))
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
}
