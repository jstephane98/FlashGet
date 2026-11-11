//! Real-HTTP integration test: exercises the production `ReqwestFetcher` +
//! `engine::download` end-to-end against a real (loopback) HTTP server.
//!
//! Unlike the unit tests (which use an in-memory FakeFetcher), this drives
//! actual TCP/HTTP networking through reqwest, proving multi-segment download,
//! range handling, and correct offset reassembly on disk.

use download_core::engine::{download, DownloadHandle, Outcome, DEFAULT_SEGMENTS};
use download_core::http::ReqwestFetcher;
use download_core::model::{Progress, RequestHeaders};
use download_core::rate_limiter::RateLimiter;
use std::sync::Arc;
use tempfile::tempdir;
use tiny_http::{Header, Response, Server};

const TOTAL: usize = 5000;

/// Deterministic payload so we can byte-compare what landed on disk.
fn payload() -> Vec<u8> {
    (0..TOTAL).map(|i| (i % 251) as u8).collect()
}

/// Parse a `Range: bytes=START-END` header value into an inclusive (start, end).
/// END is always present for our clients (probe sends `0-0`, segments send
/// explicit `start-end`).
fn parse_range(value: &str) -> Option<(u64, u64)> {
    let spec = value.trim().strip_prefix("bytes=")?;
    let (s, e) = spec.split_once('-')?;
    let start = s.trim().parse::<u64>().ok()?;
    let end = e.trim().parse::<u64>().ok()?;
    Some((start, end))
}

/// Spawn a tiny_http server on an ephemeral loopback port. Returns the bound
/// port. The server runs in a daemon-ish background thread for the lifetime of
/// the test process (we don't bother with clean shutdown — leaking it is fine).
fn start_server() -> u16 {
    let server = Server::http("127.0.0.1:0").expect("bind ephemeral port");
    let port = server.server_addr().to_ip().expect("ip addr").port();

    std::thread::spawn(move || {
        let data = payload();
        let total = data.len() as u64;
        for request in server.incoming_requests() {
            // Find a Range header (case-insensitive field name).
            let range = request
                .headers()
                .iter()
                .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("Range"))
                .and_then(|h| parse_range(h.value.as_str()));

            let response = match range {
                Some((start, end)) => {
                    // Clamp end to the last valid index.
                    let end = end.min(total - 1);
                    let slice = &data[start as usize..=end as usize];
                    let content_range =
                        format!("bytes {start}-{end}/{total}");
                    Response::from_data(slice.to_vec())
                        .with_status_code(206)
                        .with_header(
                            Header::from_bytes(&b"Content-Range"[..], content_range.as_bytes())
                                .unwrap(),
                        )
                        .with_header(
                            Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
                        )
                        .boxed()
                }
                None => Response::from_data(data.clone())
                    .with_status_code(200)
                    .with_header(
                        Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]).unwrap(),
                    )
                    .boxed(),
            };
            let _ = request.respond(response);
        }
    });

    port
}

#[tokio::test(flavor = "multi_thread")]
async fn real_http_multisegment_download_matches_payload() {
    let port = start_server();
    let fetcher = ReqwestFetcher::new();
    let dir = tempdir().unwrap();
    let dest = dir.path().join("file.bin");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();

    let url = format!("http://127.0.0.1:{port}/file");
    let outcome = download(&fetcher, "it1", &url, &dest, DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
        .await
        .expect("download should succeed");

    assert_eq!(outcome, Outcome::Completed);

    let written = std::fs::read(&dest).unwrap();
    assert_eq!(
        written,
        payload(),
        "file on disk must byte-match original payload (multi-segment reassembly)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_range_404_maps_to_not_found() {
    use download_core::http::ByteFetcher;
    use download_core::CoreError;
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let _ = request.respond(Response::from_data(Vec::new()).with_status_code(404));
        }
    });
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/missing.ts");
    let result = fetcher.fetch_range(&url, None, &RequestHeaders::default()).await;
    let err = result.map(|_| ()).unwrap_err();
    assert!(matches!(err, CoreError::NotFound(_)), "attendu NotFound, obtenu {err:?}");
}

/// Statut HTTP d'échec : 5xx → erreur `Http` passagère, 4xx → erreur `Http` définitive.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_range_http_status_maps_to_http_error() {
    use download_core::http::ByteFetcher;
    use download_core::CoreError;
    for (code, transient) in [(503u16, true), (500, true), (403, false)] {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let _ = request.respond(Response::from_data(Vec::new()).with_status_code(code));
            }
        });
        let fetcher = ReqwestFetcher::new();
        let url = format!("http://127.0.0.1:{port}/f.bin");
        let err = fetcher.fetch_range(&url, None, &RequestHeaders::default()).await.map(|_| ()).unwrap_err();
        assert!(matches!(err, CoreError::Http { status, .. } if status == code), "HTTP {code} : obtenu {err:?}");
        assert_eq!(err.is_transient(), transient, "HTTP {code}");
    }
}

/// Serveur qui ignore `Range` (toujours 200 + corps entier) : une plage en milieu de
/// fichier est refusée (le corps ne doit pas être écrit à l'offset) ; une plage depuis 0 passe.
#[tokio::test(flavor = "multi_thread")]
async fn fetch_range_rejects_full_body_for_mid_file_range() {
    use download_core::http::ByteFetcher;
    use download_core::CoreError;
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let _ = request.respond(Response::from_data(vec![1u8; 100]).with_status_code(200));
        }
    });
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/f.bin");
    let err = fetcher.fetch_range(&url, Some((10, 99)), &RequestHeaders::default()).await.map(|_| ()).unwrap_err();
    assert!(matches!(err, CoreError::Unsupported), "obtenu {err:?}");
    assert!(!err.is_transient());
    assert!(fetcher.fetch_range(&url, Some((0, 99)), &RequestHeaders::default()).await.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn real_http_progress_reports_full_size() {
    let port = start_server();
    let fetcher = ReqwestFetcher::new();
    let dir = tempdir().unwrap();
    let dest = dir.path().join("file2.bin");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();

    let url = format!("http://127.0.0.1:{port}/file");
    let outcome = download(&fetcher, "it2", &url, &dest, DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
        .await
        .expect("download should succeed");
    assert_eq!(outcome, Outcome::Completed);

    // Drain progress and assert the final cumulative downloaded_bytes == TOTAL.
    let mut last = 0u64;
    while let Ok(p) = rx.try_recv() {
        last = last.max(p.downloaded_bytes);
    }
    assert_eq!(
        last, TOTAL as u64,
        "final cumulative downloaded_bytes must equal the payload size"
    );
}
