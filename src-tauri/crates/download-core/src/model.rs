use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    /// Ajouté mais pas démarré : n'entre pas dans la file tant que l'utilisateur ne le lance pas.
    Held,
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

/// Conduite quand le fichier de destination existe déjà.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    #[default]
    Rename,
    Overwrite,
}

/// Format de la conversion audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    #[default]
    Mp3,
    M4a,
    Opus,
}

/// Réglages persistants de l'application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Settings {
    pub max_concurrent: u32,
    /// Vitesse de téléchargement max en octets/seconde (0 = illimité).
    pub max_speed_bps: u64,
    /// Connexions (segments) par fichier, 1 à 32.
    pub connections: u32,
    /// Nouvelles tentatives automatiques, 0 à 10.
    pub retries: u32,
    pub auto_resume: bool,
    pub conflict: ConflictPolicy,
    pub close_to_tray: bool,
    pub notify_complete: bool,
    pub notify_error: bool,
    pub video_max_height: u32,
    pub audio_format: AudioFormat,
    /// 0 = qualité maximale.
    pub audio_kbps: u32,
    pub sub_langs: Vec<String>,
    /// Met yt-dlp à jour automatiquement.
    pub ytdlp_auto_update: bool,
    /// Vérifie la disponibilité d'une nouvelle version de l'application.
    pub app_update_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            max_speed_bps: 0,
            connections: 8,
            retries: 3,
            auto_resume: true,
            conflict: ConflictPolicy::Rename,
            close_to_tray: true,
            notify_complete: true,
            notify_error: true,
            video_max_height: 1080,
            audio_format: AudioFormat::Mp3,
            audio_kbps: 0,
            sub_langs: vec!["fr".to_string(), "en".to_string()],
            ytdlp_auto_update: true,
            app_update_check: true,
        }
    }
}

/// Valide les réglages ; le message d'erreur est destiné à l'utilisateur.
pub fn validate_settings(s: &Settings) -> Result<(), String> {
    if !(1..=10).contains(&s.max_concurrent) {
        return Err(crate::i18n::tr("Téléchargements simultanés : entre 1 et 10", "Simultaneous downloads: between 1 and 10").into());
    }
    if !(1..=32).contains(&s.connections) {
        return Err(crate::i18n::tr("Connexions par fichier : entre 1 et 32", "Connections per file: between 1 and 32").into());
    }
    if s.retries > 10 {
        return Err("Nouvelles tentatives : entre 0 et 10".into());
    }
    if ![2160, 1440, 1080, 720, 480, 360].contains(&s.video_max_height) {
        return Err(crate::i18n::tr("Qualité vidéo inconnue", "Unknown video quality").into());
    }
    if ![0, 128, 192, 256, 320].contains(&s.audio_kbps) {
        return Err(crate::i18n::tr("Débit audio inconnu", "Unknown audio bitrate").into());
    }
    let langs_ok = !s.sub_langs.is_empty()
        && s.sub_langs.len() <= 8
        && s.sub_langs.iter().all(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_lowercase()));
    if !langs_ok {
        return Err(crate::i18n::tr("Langues de sous-titres invalides", "Invalid subtitle languages").into());
    }
    Ok(())
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
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub duration_s: Option<u64>,
    #[serde(default)]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    /// Rang dans la file (départage à priorité égale). None = pas encore classé.
    #[serde(default)]
    pub queue_rank: Option<i64>,
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
    pub content_type: Option<String>,
    /// Code HTTP de la réponse de sonde (200, 206, 404…).
    pub status: u16,
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
    /// Fraction terminée (0.0..=1.0) de chaque segment HTTP, dans l'ordre. `None` si non segmenté.
    #[serde(default)]
    pub parts: Option<Vec<f32>>,
}
