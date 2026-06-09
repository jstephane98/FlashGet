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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("network error: {0}")]
    Network(String),
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

pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    pub mod fake_fetcher;
}
