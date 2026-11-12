//! Nouvelles tentatives automatiques après un échec passager (réseau, délai dépassé, HTTP 5xx).

use download_core::engine::DownloadHandle;
use download_core::CoreError;
use std::time::Duration;

/// Pas de vérification de la pause / annulation pendant l'attente.
const WAIT_STEP: Duration = Duration::from_millis(250);
/// Plafond du délai entre deux tentatives.
const MAX_DELAY_SECS: u64 = 30;

/// Vrai s'il reste une tentative (`attempt` = tentatives déjà refaites, `max` = réglage
/// `retries`) et que l'erreur est passagère : réseau, délai dépassé ou HTTP ≥ 500.
/// Jamais pour une erreur disque, une réponse 4xx ou une ressource introuvable.
pub fn should_retry(err: &CoreError, attempt: u32, max: u32) -> bool {
    attempt < max && err.is_transient()
}

/// yt-dlp ne dit pas pourquoi il échoue : toute sortie en échec qui n'est ni une pause
/// ni une annulation est retentée (la commande reprend avec `--continue`).
pub fn should_retry_ytdlp(exit_ok: bool, paused_or_cancelled: bool, attempt: u32, max: u32) -> bool {
    attempt < max && !exit_ok && !paused_or_cancelled
}

/// Délai avant la tentative suivante : 2, 4, 8, 16 puis 30 s.
pub fn retry_delay(attempt: u32) -> Duration {
    let secs = 1u64.checked_shl(attempt.saturating_add(1)).unwrap_or(u64::MAX);
    Duration::from_secs(secs.min(MAX_DELAY_SECS))
}

/// Message d'erreur final ; mentionne le nombre total de tentatives s'il y en a eu plusieurs.
pub fn final_error_message(err: &str, attempt: u32) -> String {
    if attempt == 0 {
        err.to_string()
    } else {
        download_core::trf!("{err} (après {} tentatives)", "{err} (after {} attempts)", attempt + 1)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RetryWait {
    Elapsed,
    Paused,
    Cancelled,
}

/// Attend `delay` par pas de 250 ms ; s'arrête dès que le téléchargement est mis en pause
/// ou annulé (l'annulation l'emporte). Aucun verrou n'est tenu.
pub async fn wait_before_retry(handle: &DownloadHandle, delay: Duration) -> RetryWait {
    let mut left = delay;
    loop {
        if handle.is_cancelled() {
            return RetryWait::Cancelled;
        }
        if handle.is_paused() {
            return RetryWait::Paused;
        }
        if left.is_zero() {
            return RetryWait::Elapsed;
        }
        let step = left.min(WAIT_STEP);
        tokio::time::sleep(step).await;
        left -= step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_core::engine::DownloadHandle;
    use download_core::CoreError;
    use std::time::Duration;

    fn timeout() -> CoreError {
        CoreError::Network("[timeout] délai dépassé".into())
    }

    fn http(status: u16) -> CoreError {
        CoreError::Http { status, message: format!("HTTP {status}") }
    }

    #[test]
    fn retry_policy() {
        assert!(should_retry(&timeout(), 0, 3));
        assert!(should_retry(&timeout(), 2, 3));
        assert!(!should_retry(&timeout(), 3, 3));
        assert!(!should_retry(&CoreError::Io(std::io::Error::other("disque plein")), 0, 3));
        assert!(!should_retry(&http(404), 0, 3));
        assert!(should_retry(&http(503), 0, 3));
        assert!(!should_retry(&CoreError::NotFound("HTTP 404".into()), 0, 3));
        for attempt in 0..3 {
            assert!(!should_retry(&timeout(), attempt, 0), "max = 0 : jamais");
            assert!(!should_retry(&http(503), attempt, 0), "max = 0 : jamais");
        }
    }

    #[test]
    fn retry_delays() {
        let secs: Vec<u64> = (0..6).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(secs, vec![2, 4, 8, 16, 30, 30]);
        assert_eq!(retry_delay(u32::MAX), Duration::from_secs(30));
    }

    #[test]
    fn ytdlp_retry_policy() {
        assert!(should_retry_ytdlp(false, false, 0, 3));
        assert!(should_retry_ytdlp(false, false, 2, 3));
        assert!(!should_retry_ytdlp(false, false, 3, 3));
        assert!(!should_retry_ytdlp(true, false, 0, 3), "sortie réussie");
        assert!(!should_retry_ytdlp(false, true, 0, 3), "pause ou annulation");
        assert!(!should_retry_ytdlp(false, false, 0, 0));
    }

    #[test]
    fn final_message_mentions_attempts_only_after_retries() {
        assert_eq!(final_error_message("network error: x", 0), "network error: x");
        assert_eq!(final_error_message("network error: x", 3), "network error: x (après 4 tentatives)");
        assert_eq!(final_error_message("e", 1), "e (après 2 tentatives)");
    }

    #[tokio::test]
    async fn wait_stops_on_pause_or_cancel() {
        let h = DownloadHandle::new();
        assert_eq!(wait_before_retry(&h, Duration::from_millis(10)).await, RetryWait::Elapsed);

        let paused = DownloadHandle::new();
        paused.pause();
        assert_eq!(wait_before_retry(&paused, Duration::from_secs(30)).await, RetryWait::Paused);

        let cancelled = DownloadHandle::new();
        cancelled.pause();
        cancelled.cancel();
        assert_eq!(wait_before_retry(&cancelled, Duration::from_secs(30)).await, RetryWait::Cancelled);

        // Pause posée pendant l'attente : détectée au pas suivant (250 ms), pas au bout des 30 s.
        let late = DownloadHandle::new();
        let late2 = late.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            late2.pause();
        });
        let t0 = std::time::Instant::now();
        assert_eq!(wait_before_retry(&late, Duration::from_secs(30)).await, RetryWait::Paused);
        assert!(t0.elapsed() < Duration::from_secs(2));
    }
}
