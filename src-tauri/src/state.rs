use download_core::engine::DownloadHandle;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::Mutex;

/// État global géré par Tauri. `handles` permet pause/annulation des téléchargements actifs.
pub struct AppState {
    pub storage: Storage,
    pub handles: Mutex<HashMap<String, DownloadHandle>>,
}

impl AppState {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            handles: Mutex::new(HashMap::new()),
        }
    }
}
