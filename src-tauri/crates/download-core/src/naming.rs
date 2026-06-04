//! Choix du nom de fichier final pour un téléchargement.

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

/// Force l'extension d'un nom de fichier (remplace l'existante, ou l'ajoute).
pub fn with_extension(name: &str, ext: &str) -> String {
    let base = match name.rfind('.') {
        Some(p) if p > 0 => &name[..p],
        _ => name,
    };
    format!("{base}.{ext}")
}

/// Dernier segment de chemin (séparateurs `/` ou `\`), espaces retirés.
fn basename(s: &str) -> String {
    s.rsplit(['/', '\\']).next().unwrap_or(s).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_extension_forces_mp4() {
        assert_eq!(with_extension("video", "mp4"), "video.mp4");
        assert_eq!(with_extension("clip.ts", "mp4"), "clip.mp4");
        assert_eq!(with_extension("a.b.m3u8", "mp4"), "a.b.mp4");
    }

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
        // Nom suggéré réduit à des séparateurs ⇒ basename vide ⇒ on retombe sur le probe.
        assert_eq!(choose_file_name(Some("/"), Some("probe.zip")), "probe.zip");
    }

    #[test]
    fn falls_back_to_default_when_nothing_usable() {
        assert_eq!(choose_file_name(None, None), "download.bin");
        assert_eq!(choose_file_name(Some(""), Some("")), "download.bin");
    }
}
