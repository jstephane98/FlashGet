use crate::model::ProbeResult;
use crate::CoreError;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;

/// Abstraction réseau. La prod utilise reqwest ; les tests un fake en mémoire.
#[async_trait]
pub trait ByteFetcher: Send + Sync {
    /// Inspecte l'URL : taille, support des Range, nom de fichier suggéré.
    async fn probe(&self, url: &str) -> Result<ProbeResult, CoreError>;

    /// Récupère un flux d'octets. `range` = intervalle inclusif (start, end).
    /// Si `range` est None, récupère tout le corps.
    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError>;
}

/// Implémentation de production basée sur reqwest.
pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

impl Default for ReqwestFetcher {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ByteFetcher for ReqwestFetcher {
    async fn probe(&self, url: &str) -> Result<ProbeResult, CoreError> {
        // Une requête Range de 0-0 révèle Accept-Ranges + Content-Range (=> taille totale).
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();

        // 206 Partial Content => le serveur supporte les Range.
        let supports_ranges = status == reqwest::StatusCode::PARTIAL_CONTENT
            || headers
                .get(reqwest::header::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("bytes"))
                .unwrap_or(false);

        // Taille totale : depuis Content-Range "bytes 0-0/12345", sinon Content-Length.
        let total_bytes = headers
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .or_else(|| {
                if !supports_ranges {
                    headers
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                } else {
                    None
                }
            });

        let file_name = file_name_from_url(url);

        Ok(ProbeResult { total_bytes, supports_ranges, file_name })
    }

    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        use futures_util::StreamExt;

        let mut req = self.client.get(url);
        if let Some((start, end)) = range {
            req = req.header(reqwest::header::RANGE, format!("bytes={start}-{end}"));
        }
        let resp = req.send().await.map_err(|e| CoreError::Network(e.to_string()))?;
        let resp = resp.error_for_status().map_err(|e| CoreError::Network(e.to_string()))?;

        let stream = resp
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| CoreError::Network(e.to_string())));
        Ok(stream.boxed())
    }
}

/// Déduit un nom de fichier depuis l'URL (dernier segment de chemin), assaini.
pub fn file_name_from_url(url: &str) -> Option<String> {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    let last = without_query.rsplit('/').next().unwrap_or("").trim();
    let candidate = std::path::Path::new(last)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." {
        return None;
    }
    // neutralize any residual separators / drive markers
    let cleaned: String = candidate
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        None
    } else {
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_file_name_from_url() {
        assert_eq!(file_name_from_url("https://x.com/a/b/film.mkv?token=1"), Some("film.mkv".into()));
        assert_eq!(file_name_from_url("https://x.com/"), None);
        // traversal segment is rejected
        assert_eq!(file_name_from_url("https://x.com/a/.."), None);
        // percent-encoded traversal stays a literal (not decoded) segment — fine
        assert_eq!(file_name_from_url("https://x.com/a/%2e%2e"), Some("%2e%2e".into()));
    }
}
