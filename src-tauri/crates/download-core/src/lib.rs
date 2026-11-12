pub mod i18n;
pub mod model;
pub mod naming;
pub mod categorizer;
pub mod segments;
pub mod http;
pub mod resume;
pub mod engine;
pub mod storage;
pub mod rate_limiter;
pub mod scheduler;
pub mod hls;
pub mod dash;
pub mod media_io;
pub mod live;
pub mod ytdlp;
pub mod batch;
pub mod cookies;
pub mod planner;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("network error: {0}")]
    Network(String),
    /// Réponse HTTP en échec (4xx/5xx hors 404/410, qui donnent `NotFound`).
    #[error("http error: {message}")]
    Http { status: u16, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("the server does not support the requested operation")]
    Unsupported,
    #[error("download not found: {0}")]
    NotFound(String),
    #[error("media error: {0}")]
    Media(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl CoreError {
    /// Échec passager, qu'une nouvelle tentative peut résoudre : erreur réseau (connexion,
    /// délai dépassé, flux coupé) ou réponse HTTP ≥ 500. Jamais une erreur disque, une
    /// réponse 4xx, une ressource introuvable ni une erreur de média.
    pub fn is_transient(&self) -> bool {
        match self {
            CoreError::Network(_) => true,
            CoreError::Http { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    pub mod fake_fetcher;
}

#[cfg(test)]
mod error_tests {
    use super::CoreError;

    fn http(status: u16) -> CoreError {
        CoreError::Http { status, message: format!("HTTP {status}") }
    }

    #[test]
    fn transient_errors_are_network_and_server_side() {
        assert!(CoreError::Network("[timeout] délai dépassé".into()).is_transient());
        assert!(CoreError::Network("[connexion] refusée".into()).is_transient());
        assert!(http(500).is_transient());
        assert!(http(503).is_transient());
    }

    #[test]
    fn permanent_errors_are_not_transient() {
        assert!(!http(404).is_transient());
        assert!(!http(403).is_transient());
        assert!(!http(400).is_transient());
        assert!(!CoreError::Io(std::io::Error::other("disque plein")).is_transient());
        assert!(!CoreError::NotFound("HTTP 404".into()).is_transient());
        assert!(!CoreError::Media("yt-dlp a échoué".into()).is_transient());
        assert!(!CoreError::Unsupported.is_transient());
        assert!(!CoreError::Storage("verrou".into()).is_transient());
    }
}
