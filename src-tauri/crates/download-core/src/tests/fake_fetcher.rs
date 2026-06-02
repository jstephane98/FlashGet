//! Fetcher en mémoire pour les tests du moteur. Honore les Range inclusifs.
use crate::http::ByteFetcher;
use crate::model::ProbeResult;
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
}

impl FakeFetcher {
    pub fn new(data: Vec<u8>, supports_ranges: bool) -> Self {
        Self { data, supports_ranges, chunk_size: 7 }
    }
}

#[async_trait]
impl ByteFetcher for FakeFetcher {
    async fn probe(&self, _url: &str) -> Result<ProbeResult, CoreError> {
        Ok(ProbeResult {
            total_bytes: Some(self.data.len() as u64),
            supports_ranges: self.supports_ranges,
            file_name: Some("test.bin".to_string()),
        })
    }

    async fn fetch_range(
        &self,
        _url: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        let slice: Vec<u8> = match range {
            Some((start, end)) => {
                let start = start as usize;
                let end = (end as usize + 1).min(self.data.len());
                self.data[start..end].to_vec()
            }
            None => self.data.clone(),
        };
        let chunks: Vec<Result<Bytes, CoreError>> = slice
            .chunks(self.chunk_size.max(1))
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        Ok(stream::iter(chunks).boxed())
    }
}
