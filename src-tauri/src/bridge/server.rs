//! Boucle WebSocket du bridge — fine couche I/O au-dessus de la logique pure `protocol`.
//! Lancée via `tauri::async_runtime::spawn` (JAMAIS `tokio::spawn` direct).

use crate::bridge::protocol::{handle_message, parse_client_message, ClientMessage, ServerReply};
use crate::bridge::status::{summarize, BridgeStatus, ClientInfo};
use crate::link;
use crate::state::AppState;
use download_core::model::RequestHeaders;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

/// Port d'écoute loopback par défaut (l'extension est configurable côté options).
pub const PORT: u16 = 15151;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Calcule l'état courant. Les verrous sont pris un à un et relâchés avant `summarize`.
pub fn current_status(app: &AppHandle) -> BridgeStatus {
    let st = app.state::<AppState>();
    let listening = st.bridge_listening.load(Ordering::Relaxed);
    let rejected = *st.bridge_rejected_at.lock().unwrap();
    let clients = st.bridge_clients.lock().unwrap().clone();
    summarize(listening, &clients, rejected, now_secs())
}

/// Émet `bridge-status` (aucun jeton dans le payload).
pub fn emit_status(app: &AppHandle) {
    let _ = app.emit("bridge-status", current_status(app));
}

/// Retire le client à la fermeture de la connexion, quelle qu'en soit la cause.
struct ConnGuard {
    app: AppHandle,
    id: u64,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let removed = self
            .app
            .state::<AppState>()
            .bridge_clients
            .lock()
            .unwrap()
            .remove(&self.id)
            .is_some();
        if removed {
            emit_status(&self.app);
        }
    }
}

/// Démarre le serveur WebSocket loopback. À appeler depuis le hook `setup`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let addr = format!("127.0.0.1:{PORT}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("bridge: impossible d'écouter sur {addr}: {e}");
                emit_status(&app);
                return;
            }
        };
        app.state::<AppState>().bridge_listening.store(true, Ordering::Relaxed);
        emit_status(&app);
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
    let conn_id = app.state::<AppState>().bridge_next_conn.fetch_add(1, Ordering::Relaxed);
    let _guard = ConnGuard { app: app.clone(), id: conn_id };
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
        let auth_info = match &parsed {
            ClientMessage::Auth { browser, version, .. } => Some((browser.clone(), version.clone())),
            _ => None,
        };
        let (new_authed, reply) = handle_message(authed, &expected, parsed);
        authed = new_authed;
        let now = now_secs();
        match &reply {
            ServerReply::Authed => {
                let (browser, version) = auth_info.unwrap_or((None, None));
                app.state::<AppState>()
                    .bridge_clients
                    .lock()
                    .unwrap()
                    .insert(conn_id, ClientInfo { browser, version, last_seen: now });
                emit_status(&app);
            }
            ServerReply::AuthFailed => {
                *app.state::<AppState>().bridge_rejected_at.lock().unwrap() = Some(now);
                app.state::<AppState>().bridge_clients.lock().unwrap().remove(&conn_id);
                emit_status(&app);
            }
            _ if authed => {
                if let Some(c) = app.state::<AppState>().bridge_clients.lock().unwrap().get_mut(&conn_id) {
                    c.last_seen = now;
                }
            }
            _ => {}
        }

        let out = match reply {
            ServerReply::Authed => serde_json::json!({ "type": "authed" }).to_string(),
            ServerReply::AuthFailed => {
                serde_json::json!({ "type": "error", "error": "auth_failed" }).to_string()
            }
            ServerReply::Pong => serde_json::json!({ "type": "pong" }).to_string(),
            ServerReply::NeedAuth => {
                serde_json::json!({ "type": "error", "error": "need_auth" }).to_string()
            }
            ServerReply::Error { msg } => {
                serde_json::json!({ "type": "error", "error": msg }).to_string()
            }
            ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind, fallback_url, fallback_kind } => {
                // Toute capture (fichier direct sans `kind` compris) passe par l'analyse de lien :
                // `link://probing` + fenêtre au premier plan, puis `link://probed` / `link://error`.
                // L'utilisateur confirme sur la page Nouveau : aucun téléchargement n'est créé ici.
                // Comme avant, la réponse WS part une fois l'analyse terminée.
                let headers = RequestHeaders { referrer, cookie, user_agent };
                let ack_kind = kind.clone().unwrap_or_else(|| "file".to_string());
                let hint = link::hint_for_kind(kind.as_deref(), fallback_url, fallback_kind);
                match link::capture(&app, &url, headers, hint, filename).await {
                    Ok(p) => serde_json::json!({ "type": "ack", "kind": ack_kind, "probe_id": p.probe_id() }).to_string(),
                    Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                }
            }
        };
        if write.send(Message::Text(out)).await.is_err() {
            break;
        }
    }
}
