use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub dest_path: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub supports_ranges: bool,
    pub error: Option<String>,
}

/// Résultat de la requête d'inspection (HEAD/GET partiel) faite avant de télécharger.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub total_bytes: Option<u64>,
    pub supports_ranges: bool,
    pub file_name: Option<String>,
}

/// Payload de progression poussé vers l'UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: u64,
    pub status: DownloadStatus,
}
