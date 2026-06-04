use download_core::dash::mpd::Mpd;
use download_core::engine::DownloadHandle;
use download_core::http::ReqwestFetcher;
use download_core::model::RequestHeaders;
use download_core::rate_limiter::RateLimiter;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// État global géré par Tauri.
pub struct AppState {
    pub storage: Storage,
    pub handles: Mutex<HashMap<String, DownloadHandle>>,
    /// Fetcher mutualisé (un seul `reqwest::Client` partagé).
    pub fetcher: ReqwestFetcher,
    /// Limiteur de bande passante global partagé.
    pub limiter: Arc<RateLimiter>,
    /// `true` une fois le serveur WebSocket du bridge à l'écoute sur la loopback.
    pub bridge_listening: Arc<AtomicBool>,
    /// Probes HLS en attente d'un choix de variante (probe_id → PendingMedia).
    pub pending_media: Mutex<HashMap<String, PendingMedia>>,
}

impl AppState {
    pub fn new(storage: Storage, max_speed_bps: u64) -> Self {
        Self {
            storage,
            handles: Mutex::new(HashMap::new()),
            fetcher: ReqwestFetcher::new(),
            limiter: Arc::new(RateLimiter::new(max_speed_bps)),
            bridge_listening: Arc::new(AtomicBool::new(false)),
            pending_media: Mutex::new(HashMap::new()),
        }
    }
}

/// Probe média (HLS ou DASH) selon le type de flux sondé.
pub enum MediaProbe {
    Hls(download_core::hls::playlist::MasterPlaylist),
    Dash(Mpd),
}

/// Probe média (HLS ou DASH) en attente d'un choix de variante par l'utilisateur.
pub struct PendingMedia {
    pub probe: MediaProbe,
    pub headers: RequestHeaders,
    pub file_name: String,
    /// URL source (master HLS ou MPD DASH), nécessaire pour reconstruire un download DASH.
    pub source_url: String,
    /// Horodatage d'insertion, pour l'éviction TTL des probes jamais confirmés.
    pub created: Instant,
}
