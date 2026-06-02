use crate::CoreError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Progression par segment : combien d'octets ont déjà été écrits dans chaque segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumeState {
    pub url: String,
    pub total_bytes: u64,
    /// Pour chaque segment : (start, end_inclusif, octets_déjà_téléchargés).
    pub segments: Vec<(u64, u64, u64)>,
}

/// Chemin du fichier sidecar associé à un fichier de destination.
pub fn sidecar_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".fgdownload");
    PathBuf::from(s)
}

pub fn save(dest: &Path, state: &ResumeState) -> Result<(), CoreError> {
    let json = serde_json::to_vec_pretty(state)?;
    std::fs::write(sidecar_path(dest), json)?;
    Ok(())
}

pub fn load(dest: &Path) -> Option<ResumeState> {
    let bytes = std::fs::read(sidecar_path(dest)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn clear(dest: &Path) {
    let _ = std::fs::remove_file(sidecar_path(dest));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let state = ResumeState {
            url: "http://x/f".into(),
            total_bytes: 100,
            segments: vec![(0, 49, 10), (50, 99, 0)],
        };
        save(&dest, &state).unwrap();
        assert_eq!(load(&dest), Some(state));
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempdir().unwrap();
        assert_eq!(load(&dir.path().join("nope.bin")), None);
    }

    #[test]
    fn clear_removes_sidecar() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let state = ResumeState { url: "u".into(), total_bytes: 1, segments: vec![(0, 0, 0)] };
        save(&dest, &state).unwrap();
        clear(&dest);
        assert_eq!(load(&dest), None);
    }
}
