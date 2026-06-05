use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Recording,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DownloadKind {
    #[default]
    Regular,
    Hls,
    Dash,
    Ytdlp,
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
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub cookie: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub kind: DownloadKind,
    #[serde(default)]
    pub audio_url: Option<String>,
    #[serde(default)]
    pub media_selection: Option<String>,
}

/// En-têtes HTTP optionnels transmis par le navigateur pour un téléchargement capturé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestHeaders {
    pub referrer: Option<String>,
    pub cookie: Option<String>,
    pub user_agent: Option<String>,
}

impl Download {
    /// Copie les en-têtes HTTP optionnels associés à ce téléchargement.
    pub fn request_headers(&self) -> RequestHeaders {
        RequestHeaders {
            referrer: self.referrer.clone(),
            cookie: self.cookie.clone(),
            user_agent: self.user_agent.clone(),
        }
    }
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
    #[serde(default)]
    pub segments_done: Option<u32>,
    #[serde(default)]
    pub segments_total: Option<u32>,
}
