//! État de la connexion avec l'extension — calcul PUR (aucune I/O, aucun tauri).

use serde::Serialize;
use std::collections::HashMap;

/// Fenêtre (en secondes) pendant laquelle un refus de jeton reste visible.
pub const REJECTED_WINDOW_SECS: i64 = 120;

/// Un client authentifié (une extension connectée).
#[derive(Debug, Clone, PartialEq)]
pub struct ClientInfo {
    pub browser: Option<String>,
    pub version: Option<String>,
    /// Dernier message reçu, en secondes Unix.
    pub last_seen: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BridgeStatus {
    /// "connected", "waiting", "rejected" ou "inactive".
    pub state: &'static str,
    pub browser: Option<String>,
    pub version: Option<String>,
    pub last_seen: Option<i64>,
    pub clients: usize,
    pub port: u16,
}

/// Résume l'état : serveur arrêté ⇒ inactive ; au moins un client ⇒ connected (le plus
/// récemment vu) ; sinon refus de jeton récent ⇒ rejected ; sinon waiting.
pub fn summarize(
    listening: bool,
    clients: &HashMap<u64, ClientInfo>,
    last_rejected: Option<i64>,
    now: i64,
) -> BridgeStatus {
    let port = crate::bridge::PORT;
    let base = |state| BridgeStatus {
        state,
        browser: None,
        version: None,
        last_seen: None,
        clients: clients.len(),
        port,
    };
    if !listening {
        return base("inactive");
    }
    // Départage déterministe à dernier vu égal : le plus grand id de connexion.
    if let Some((_, c)) = clients.iter().max_by_key(|(id, c)| (c.last_seen, **id)) {
        return BridgeStatus {
            state: "connected",
            browser: c.browser.clone(),
            version: c.version.clone(),
            last_seen: Some(c.last_seen),
            clients: clients.len(),
            port,
        };
    }
    match last_rejected {
        Some(t) if now - t <= REJECTED_WINDOW_SECS => base("rejected"),
        _ => base("waiting"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(browser: &str, last_seen: i64) -> ClientInfo {
        ClientInfo { browser: Some(browser.into()), version: Some("1.0".into()), last_seen }
    }

    #[test]
    fn summarize_inactive_when_not_listening() {
        let mut c = HashMap::new();
        c.insert(1, client("chrome", 10));
        let s = summarize(false, &c, Some(5), 20);
        assert_eq!(s.state, "inactive");
        assert_eq!(s.browser, None);
    }

    #[test]
    fn summarize_waiting() {
        let s = summarize(true, &HashMap::new(), None, 100);
        assert_eq!(s.state, "waiting");
        assert_eq!(s.clients, 0);
        assert_eq!(s.port, crate::bridge::PORT);
    }

    #[test]
    fn summarize_connected_picks_most_recent() {
        let mut c = HashMap::new();
        c.insert(1, client("chrome", 50));
        c.insert(2, client("firefox", 90));
        let s = summarize(true, &c, None, 100);
        assert_eq!(s.state, "connected");
        assert_eq!(s.browser.as_deref(), Some("firefox"));
        assert_eq!(s.last_seen, Some(90));
        assert_eq!(s.clients, 2);
    }

    #[test]
    fn summarize_rejected_within_window_only() {
        let c = HashMap::new();
        assert_eq!(summarize(true, &c, Some(100), 150).state, "rejected");
        assert_eq!(summarize(true, &c, Some(100), 100 + 121).state, "waiting");
    }

    #[test]
    fn summarize_connected_wins_over_rejected() {
        let mut c = HashMap::new();
        c.insert(1, client("chrome", 99));
        assert_eq!(summarize(true, &c, Some(100), 101).state, "connected");
    }

    #[test]
    fn status_after_disconnect() {
        let mut c = HashMap::new();
        c.insert(1, client("chrome", 99));
        assert_eq!(summarize(true, &c, None, 100).state, "connected");
        c.remove(&1);
        assert_eq!(summarize(true, &c, None, 101).state, "waiting");
    }

    #[test]
    fn status_serializes_snake_case() {
        let s = summarize(true, &HashMap::new(), None, 1);
        let v = serde_json::to_value(&s).unwrap();
        for k in ["state", "browser", "version", "last_seen", "clients", "port"] {
            assert!(v.get(k).is_some(), "champ manquant {k}");
        }
    }
}
