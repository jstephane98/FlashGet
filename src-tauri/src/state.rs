use download_core::engine::DownloadHandle;
use download_core::http::ReqwestFetcher;
use download_core::rate_limiter::RateLimiter;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

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
}

impl AppState {
    pub fn new(storage: Storage, max_speed_bps: u64) -> Self {
        Self {
            storage,
            handles: Mutex::new(HashMap::new()),
            fetcher: ReqwestFetcher::new(),
            limiter: Arc::new(RateLimiter::new(max_speed_bps)),
            bridge_listening: Arc::new(AtomicBool::new(false)),
        }
    }
}
