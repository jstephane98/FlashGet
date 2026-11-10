//! Fetcher en mémoire pour les tests du moteur. Honore les Range inclusifs.
use crate::http::ByteFetcher;
use crate::model::{ProbeResult, RequestHeaders};
use crate::CoreError;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;

pub struct FakeFetcher {
    pub data: Vec<u8>,
    pub supports_ranges: bool,
    /// Taille de chunk émise par le flux (simule un réseau morcelé).
    pub chunk_size: usize,
    /// Si false, `probe` ne révèle pas la taille (total_bytes = None).
    pub probe_reports_size: bool,
    /// Si présent, chaque flux s'interrompt sur une erreur réseau à cet offset absolu
    /// (les octets avant l'offset sont livrés).
    pub fail_at: Option<u64>,
}

impl FakeFetcher {
    pub fn new(data: Vec<u8>, supports_ranges: bool) -> Self {
        Self { data, supports_ranges, chunk_size: 7, probe_reports_size: true, fail_at: None }
    }
}

#[async_trait]
impl ByteFetcher for FakeFetcher {
    async fn probe(&self, _url: &str, _headers: &RequestHeaders) -> Result<ProbeResult, CoreError> {
        Ok(ProbeResult {
            total_bytes: if self.probe_reports_size {
                Some(self.data.len() as u64)
            } else {
                None
            },
            supports_ranges: self.supports_ranges,
            file_name: Some("test.bin".to_string()),
            content_type: None,
            status: 200,
        })
    }

    async fn fetch_range(
        &self,
        _url: &str,
        range: Option<(u64, u64)>,
        _headers: &RequestHeaders,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        let (start, end) = match range {
            Some((start, end)) => (start as usize, (end as usize + 1).min(self.data.len())),
            None => (0, self.data.len()),
        };
        // Coupure simulée : on ne livre que jusqu'à `fail_at`, puis une erreur réseau.
        let cut = self.fail_at.map(|f| (f as usize).clamp(start, end.max(start)));
        let deliver_end = cut.unwrap_or(end).min(end);
        let slice = &self.data[start..deliver_end.max(start)];
        let mut chunks: Vec<Result<Bytes, CoreError>> = slice
            .chunks(self.chunk_size.max(1))
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        if cut.is_some_and(|c| c < end) {
            chunks.push(Err(CoreError::Network("connexion interrompue".into())));
        }
        Ok(stream::iter(chunks).boxed())
    }
}
