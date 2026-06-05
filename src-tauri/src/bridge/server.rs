//! Boucle WebSocket du bridge — fine couche I/O au-dessus de la logique pure `protocol`.
//! Lancée via `tauri::async_runtime::spawn` (JAMAIS `tokio::spawn` direct).

use crate::bridge::protocol::{handle_message, parse_client_message, ServerReply};
use crate::commands;
use crate::state::AppState;
use download_core::model::RequestHeaders;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

/// Port d'écoute loopback par défaut (l'extension est configurable côté options).
pub const PORT: u16 = 15151;

/// Démarre le serveur WebSocket loopback. À appeler depuis le hook `setup`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let addr = format!("127.0.0.1:{PORT}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("bridge: impossible d'écouter sur {addr}: {e}");
                return;
            }
        };
        app.state::<AppState>().bridge_listening.store(true, Ordering::Relaxed);
        eprintln!("bridge: à l'écoute sur ws://{addr}");
        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(handle_conn(app2, stream));
                }
                Err(e) => eprintln!("bridge: accept a échoué: {e}"),
            }
        }
    });
}

/// Gère une connexion : handshake WS, puis boucle lecture → handle_message → action → réponse JSON.
async fn handle_conn(app: AppHandle, stream: TcpStream) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("bridge: handshake WS échoué: {e}");
            return;
        }
    };
    let (mut write, mut read) = ws.split();
    let mut authed = false;
    // Jeton attendu lu une fois à la connexion.
    let expected = app
        .state::<AppState>()
        .storage
        .get_or_create_bridge_token()
        .unwrap_or_default();

    while let Some(Ok(msg)) = read.next().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue, // ignore ping/pong/binary
        };
        let parsed = parse_client_message(&text);
        let (new_authed, reply) = handle_message(authed, &expected, parsed);
        authed = new_authed;

        let out = match reply {
            ServerReply::Authed => serde_json::json!({ "type": "authed" }).to_string(),
            ServerReply::AuthFailed => {
                serde_json::json!({ "type": "error", "error": "auth_failed" }).to_string()
            }
            ServerReply::NeedAuth => {
                serde_json::json!({ "type": "error", "error": "need_auth" }).to_string()
            }
            ServerReply::Error { msg } => {
                serde_json::json!({ "type": "error", "error": msg }).to_string()
            }
            ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind } => {
                let headers = RequestHeaders { referrer, cookie, user_agent };
                if kind.as_deref() == Some("smart") {
                    match commands::probe_smart(&app, &url, filename, fallback_url, fallback_kind, headers).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "smart" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else if kind.as_deref() == Some("hls") {
                    match commands::probe_hls(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "hls" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else if kind.as_deref() == Some("dash") {
                    match commands::probe_dash(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "dash" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else if kind.as_deref() == Some("page") {
                    // Page vidéo générique → extraction yt-dlp (cookies/UA transmis).
                    match commands::probe_ytdlp(&app, &url, headers).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "page" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else {
                    // Capture via le pipeline d'ajout existant, en transmettant les en-têtes du navigateur
                    // et le nom de fichier suggéré par le navigateur.
                    match commands::add_url(&app, &url, "", headers, filename).await {
                        Ok(d) => {
                            // Notifier le frontend pour qu'il rafraîchisse sa liste : sans cet
                            // événement la capture n'apparaît qu'à la complétion (l'UI n'insère
                            // pas les downloads inconnus reçus via `download://progress`).
                            let _ = app.emit("download://added", &d);
                            // Faire surgir la fenêtre de l'app au premier plan (UX type IDM).
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.unminimize();
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                            serde_json::json!({ "type": "ack", "id": d.id }).to_string()
                        }
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                }
            }
        };
        if write.send(Message::Text(out)).await.is_err() {
            break;
        }
    }
}
