use download_core::engine::DownloadHandle;
use download_core::http::ReqwestFetcher;
use download_core::rate_limiter::RateLimiter;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
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
    /// Extensions authentifiées (clé : id de connexion).
    pub bridge_clients: Mutex<HashMap<u64, crate::bridge::status::ClientInfo>>,
    /// Dernier refus de jeton (secondes Unix).
    pub bridge_rejected_at: Mutex<Option<i64>>,
    /// Compteur d'ids de connexion du bridge.
    pub bridge_next_conn: AtomicU64,
    /// Diffusion du message `config` (JSON complet) aux extensions authentifiées.
    pub bridge_config: tokio::sync::broadcast::Sender<String>,
    /// Sérialise les écritures de la liste des sites ignorés.
    pub sites_lock: Mutex<()>,
    /// Liens analysés (page Nouveau / captures) en attente de confirmation (probe_id → PendingLink).
    pub pending_links: Mutex<HashMap<String, crate::link::PendingLink>>,
    /// Contexte des captures échouées (capture_id → FailedCapture), pour « Réessayer ».
    pub failed_captures: Mutex<HashMap<String, crate::link::FailedCapture>>,
    /// Planificateur et suivi de fin de file. Verrou feuille (voir `queue.rs`).
    pub planner: Mutex<crate::queue::PlannerState>,
    /// Version de yt-dlp, mise en cache pour la session (succès seulement).
    pub ytdlp_version: Mutex<Option<String>>,
    /// `true` pendant une mise à jour de yt-dlp (refuse un second appel concurrent).
    pub ytdlp_updating: AtomicBool,
    /// `Update` trouvée par la dernière vérification de FlashGet.
    pub checked_update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// `true` de la mise en pause avant installation jusqu'à son échec : plus aucun lancement.
    pub app_installing: AtomicBool,
    /// `true` pendant un téléchargement ou une installation de FlashGet.
    pub app_update_busy: AtomicBool,
    /// Mise à jour de FlashGet téléchargée (et signée), prête à installer sur clic.
    pub pending_update: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
}

impl AppState {
    pub fn new(storage: Storage, max_speed_bps: u64) -> Self {
        Self {
            storage,
            handles: Mutex::new(HashMap::new()),
            fetcher: ReqwestFetcher::new(),
            limiter: Arc::new(RateLimiter::new(max_speed_bps)),
            bridge_listening: Arc::new(AtomicBool::new(false)),
            bridge_clients: Mutex::new(HashMap::new()),
            bridge_rejected_at: Mutex::new(None),
            bridge_next_conn: AtomicU64::new(1),
            bridge_config: tokio::sync::broadcast::channel(16).0,
            sites_lock: Mutex::new(()),
            pending_links: Mutex::new(HashMap::new()),
            failed_captures: Mutex::new(HashMap::new()),
            planner: Mutex::new(crate::queue::PlannerState::default()),
            ytdlp_version: Mutex::new(None),
            ytdlp_updating: AtomicBool::new(false),
            checked_update: Mutex::new(None),
            pending_update: Mutex::new(None),
            app_installing: AtomicBool::new(false),
            app_update_busy: AtomicBool::new(false),
        }
    }
}
