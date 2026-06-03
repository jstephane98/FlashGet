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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    #[default]
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Video,
    Document,
    Program,
    Archive,
    Music,
    #[default]
    Other,
}

/// Réglages persistants de l'application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub max_concurrent: u32,
    /// Vitesse de téléchargement max en octets/seconde (0 = illimité).
    pub max_speed_bps: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { max_concurrent: 3, max_speed_bps: 0 }
    }
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
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub category: Category,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CategoryDirs {
    pub video: Option<String>,
    pub document: Option<String>,
    pub program: Option<String>,
    pub archive: Option<String>,
    pub music: Option<String>,
    pub other: Option<String>,
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
