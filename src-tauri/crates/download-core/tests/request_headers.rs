//! Vérifie que `ReqwestFetcher` envoie bien Cookie/Referer/User-Agent quand fournis.
use download_core::http::{ByteFetcher, ReqwestFetcher};
use download_core::model::RequestHeaders;
use std::sync::{Arc, Mutex};
use tiny_http::{Response, Server};

/// Serveur loopback qui enregistre les en-têtes de la 1re requête puis répond 200.
fn start_recording_server(seen: Arc<Mutex<Vec<String>>>) -> u16 {
    let server = Server::http("127.0.0.1:0").expect("bind ephemeral port");
    let port = server.server_addr().to_ip().expect("ip addr").port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let lines: Vec<String> = request
                .headers()
                .iter()
                .map(|h| format!("{}: {}", h.field.as_str().as_str(), h.value.as_str()))
                .collect();
            *seen.lock().unwrap() = lines;
            let _ = request.respond(Response::from_string("ok"));
            break;
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_forwards_cookie_referer_user_agent() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let port = start_recording_server(seen.clone());
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/file.bin");
    let headers = RequestHeaders {
        referrer: Some("http://example.com/page".to_string()),
        cookie: Some("sid=abc123; theme=dark".to_string()),
        user_agent: Some("Mozilla/5.0 (TestBrowser)".to_string()),
    };

    let _ = fetcher.probe(&url, &headers).await.expect("probe should succeed");

    let joined = seen.lock().unwrap().join("\n").to_lowercase();
    assert!(joined.contains("referer: http://example.com/page"), "referer manquant dans: {joined}");
    assert!(joined.contains("cookie: sid=abc123; theme=dark"), "cookie manquant dans: {joined}");
    assert!(joined.contains("user-agent: mozilla/5.0 (testbrowser)"), "user-agent manquant dans: {joined}");
}

#[tokio::test(flavor = "multi_thread")]
async fn probe_without_headers_sends_no_cookie() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let port = start_recording_server(seen.clone());
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/file.bin");

    let _ = fetcher.probe(&url, &RequestHeaders::default()).await.expect("probe ok");

    let joined = seen.lock().unwrap().join("\n").to_lowercase();
    assert!(!joined.contains("cookie:"), "aucun cookie ne doit être envoyé: {joined}");
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_range_forwards_headers() {
    // Parité avec probe : le chemin fetch_range (exécuté N fois par téléchargement
    // multi-segments) doit aussi transmettre Cookie/Referer/User-Agent.
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let port = start_recording_server(seen.clone());
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/file.bin");
    let headers = RequestHeaders {
        referrer: Some("http://example.com/ref".to_string()),
        cookie: Some("session=xyz".to_string()),
        user_agent: Some("UA/test".to_string()),
    };

    let _ = fetcher
        .fetch_range(&url, Some((0, 10)), &headers)
        .await
        .expect("fetch_range should succeed");

    let joined = seen.lock().unwrap().join("\n").to_lowercase();
    assert!(joined.contains("referer: http://example.com/ref"), "referer manquant: {joined}");
    assert!(joined.contains("cookie: session=xyz"), "cookie manquant: {joined}");
    assert!(joined.contains("user-agent: ua/test"), "user-agent manquant: {joined}");
}
