//! Logique de protocole du bridge navigateur — PURE et testable (aucune I/O, aucun tauri).
//! La boucle WebSocket (server.rs) est une fine couche qui appelle ces fonctions.

use serde::Deserialize;

/// Message reçu du client (extension). `Unknown` = JSON invalide ou type inconnu.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Auth { token: String },
    Capture {
        url: String,
        filename: Option<String>,
        referrer: Option<String>,
        cookie: Option<String>,
        user_agent: Option<String>,
        kind: Option<String>,
    },
    Unknown,
}

/// Réponse logique du serveur. `CaptureRequested` signale à la couche I/O de lancer le téléchargement.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerReply {
    Authed,
    AuthFailed,
    NeedAuth,
    CaptureRequested {
        url: String,
        filename: Option<String>,
        referrer: Option<String>,
        cookie: Option<String>,
        user_agent: Option<String>,
        kind: Option<String>,
    },
    Error { msg: String },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RawMessage {
    Auth { token: String },
    Capture {
        url: String,
        #[serde(default)]
        filename: Option<String>,
        #[serde(default)]
        referrer: Option<String>,
        #[serde(default)]
        cookie: Option<String>,
        #[serde(default)]
        user_agent: Option<String>,
        #[serde(default)]
        kind: Option<String>,
    },
}

/// Parse un texte JSON en `ClientMessage`. Tout échec ⇒ `Unknown`.
pub fn parse_client_message(text: &str) -> ClientMessage {
    match serde_json::from_str::<RawMessage>(text) {
        Ok(RawMessage::Auth { token }) => ClientMessage::Auth { token },
        Ok(RawMessage::Capture { url, filename, referrer, cookie, user_agent, kind }) => {
            ClientMessage::Capture { url, filename, referrer, cookie, user_agent, kind }
        }
        Err(_) => ClientMessage::Unknown,
    }
}

/// Transition pure : (déjà authentifié, jeton attendu, message) -> (nouvel état authentifié, réponse).
/// Règle clé : une `Capture` reçue avant un `Auth` valide ⇒ `NeedAuth` (jamais de capture).
pub fn handle_message(
    authed: bool,
    expected_token: &str,
    msg: ClientMessage,
) -> (bool, ServerReply) {
    match msg {
        ClientMessage::Auth { token } => {
            if !expected_token.is_empty() && token == expected_token {
                (true, ServerReply::Authed)
            } else {
                (false, ServerReply::AuthFailed)
            }
        }
        ClientMessage::Capture { url, filename, referrer, cookie, user_agent, kind } => {
            if authed {
                (authed, ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind })
            } else {
                (authed, ServerReply::NeedAuth)
            }
        }
        ClientMessage::Unknown => (authed, ServerReply::Error { msg: "invalid_message".into() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_before_auth_is_refused() {
        let msg = parse_client_message(r#"{"type":"capture","url":"http://x/f.zip"}"#);
        let (authed, reply) = handle_message(false, "secret", msg);
        assert!(!authed, "doit rester non authentifié");
        assert!(matches!(reply, ServerReply::NeedAuth));
    }

    #[test]
    fn auth_with_good_token_succeeds() {
        let msg = parse_client_message(r#"{"type":"auth","token":"secret"}"#);
        let (authed, reply) = handle_message(false, "secret", msg);
        assert!(authed);
        assert!(matches!(reply, ServerReply::Authed));
    }

    #[test]
    fn auth_with_bad_token_fails_and_stays_unauthed() {
        let msg = parse_client_message(r#"{"type":"auth","token":"wrong"}"#);
        let (authed, reply) = handle_message(false, "secret", msg);
        assert!(!authed);
        assert!(matches!(reply, ServerReply::AuthFailed));
    }

    #[test]
    fn capture_after_auth_yields_capture_requested() {
        let msg = parse_client_message(r#"{"type":"capture","url":"http://x/f.zip"}"#);
        let (authed, reply) = handle_message(true, "secret", msg);
        assert!(authed, "reste authentifié");
        match reply {
            ServerReply::CaptureRequested { url, .. } => assert_eq!(url, "http://x/f.zip"),
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_error() {
        let msg = parse_client_message("not json at all");
        assert!(matches!(msg, ClientMessage::Unknown));
        let (authed, reply) = handle_message(true, "secret", msg);
        assert!(authed);
        assert!(matches!(reply, ServerReply::Error { .. }));
    }

    #[test]
    fn auth_with_empty_expected_token_is_refused() {
        // Sécurité : un serveur sans jeton configuré ne doit jamais authentifier.
        let msg = parse_client_message(r#"{"type":"auth","token":""}"#);
        let (authed, reply) = handle_message(false, "", msg);
        assert!(!authed);
        assert!(matches!(reply, ServerReply::AuthFailed));
    }

    #[test]
    fn reauth_with_valid_token_stays_authed() {
        // Une ré-authentification valide alors qu'on est déjà authentifié reste authentifiée.
        let msg = parse_client_message(r#"{"type":"auth","token":"secret"}"#);
        let (authed, reply) = handle_message(true, "secret", msg);
        assert!(authed);
        assert!(matches!(reply, ServerReply::Authed));
    }

    #[test]
    fn capture_carries_cookie_and_user_agent() {
        let msg = parse_client_message(
            r#"{"type":"capture","url":"http://x/f.zip","referrer":"http://x/page","cookie":"sid=1","user_agent":"UA/1"}"#,
        );
        let (_authed, reply) = handle_message(true, "secret", msg);
        match reply {
            ServerReply::CaptureRequested { url, referrer, cookie, user_agent, .. } => {
                assert_eq!(url, "http://x/f.zip");
                assert_eq!(referrer.as_deref(), Some("http://x/page"));
                assert_eq!(cookie.as_deref(), Some("sid=1"));
                assert_eq!(user_agent.as_deref(), Some("UA/1"));
            }
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }

    #[test]
    fn capture_carries_kind_hls() {
        let msg = parse_client_message(
            r#"{"type":"capture","url":"http://x/m.m3u8","kind":"hls"}"#,
        );
        let (_authed, reply) = handle_message(true, "secret", msg);
        match reply {
            ServerReply::CaptureRequested { kind, .. } => assert_eq!(kind.as_deref(), Some("hls")),
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }
}
