//! Ce qui change d'un système à l'autre : noms des binaires yt-dlp et droits d'exécution.

/// Copie de yt-dlp installée par la mise à jour, dans `{app_data}/bin`.
#[cfg(windows)]
pub const YTDLP_FILE: &str = "yt-dlp.exe";
#[cfg(not(windows))]
pub const YTDLP_FILE: &str = "yt-dlp";

/// Nouvelle version en attente (la copie courante était verrouillée).
#[cfg(windows)]
pub const YTDLP_NEXT: &str = "yt-dlp.next.exe";
#[cfg(not(windows))]
pub const YTDLP_NEXT: &str = "yt-dlp.next";

/// Fichier temporaire du téléchargement.
#[cfg(windows)]
pub const YTDLP_DOWNLOAD: &str = "yt-dlp.download.exe";
#[cfg(not(windows))]
pub const YTDLP_DOWNLOAD: &str = "yt-dlp.download";

/// Nom du binaire autonome dans les publications de yt-dlp sur GitHub.
#[cfg(windows)]
pub const YTDLP_ASSET: &str = "yt-dlp.exe";
#[cfg(target_os = "linux")]
pub const YTDLP_ASSET: &str = "yt-dlp_linux";
#[cfg(target_os = "macos")]
pub const YTDLP_ASSET: &str = "yt-dlp_macos";

/// Rend un binaire téléchargé exécutable (sans effet sous Windows).
pub fn make_executable(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_coherents() {
        assert!(YTDLP_NEXT.starts_with("yt-dlp.next"));
        assert!(YTDLP_DOWNLOAD.starts_with("yt-dlp.download"));
        assert!(YTDLP_ASSET.starts_with("yt-dlp"));
        #[cfg(windows)]
        assert!(YTDLP_FILE.ends_with(".exe") && YTDLP_NEXT.ends_with(".exe") && YTDLP_DOWNLOAD.ends_with(".exe"));
        #[cfg(not(windows))]
        assert!(!YTDLP_FILE.contains('.'));
    }

    #[test]
    fn rend_executable() {
        let p = std::env::temp_dir().join(format!("flashget-exec-{}", std::process::id()));
        std::fs::write(&p, b"x").unwrap();
        make_executable(&p).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let _ = std::fs::remove_file(&p);
    }
}
