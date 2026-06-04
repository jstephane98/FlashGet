//! Enregistrement HLS live : boucle de rafraîchissement de la media playlist,
//! capture incrémentale par numéro de séquence absolue, finalisation par
//! concat + remux. Voir spec 2026-06-04-phase-4b2a-hls-live-design.md.
use std::time::Duration;
use crate::engine::DownloadHandle;

/// Intervalle entre deux rafraîchissements de la playlist.
#[derive(Debug, Clone, Copy)]
pub enum RefreshInterval {
    /// Production : dérive de `EXT-X-TARGETDURATION` (borne min 1 s, défaut 6 s).
    FromTargetDuration,
    /// Tests : intervalle fixe (ex. 0–10 ms pour ne pas dormir).
    Fixed(Duration),
}

impl RefreshInterval {
    pub fn compute(&self, target_duration: f64) -> Duration {
        match self {
            RefreshInterval::FromTargetDuration => {
                let secs = if target_duration > 0.0 { target_duration } else { 6.0 };
                Duration::from_secs_f64(secs.max(1.0))
            }
            RefreshInterval::Fixed(d) => *d,
        }
    }
}

/// Dort jusqu'à `dur` en vérifiant stop/cancel toutes les 50 ms.
/// Renvoie `true` si interrompu (stop ou cancel demandé) avant l'échéance.
// Utilisé par download_hls_live (Task 5) ; l'allow tombera quand le moteur l'appellera.
#[allow(dead_code)]
pub(crate) async fn responsive_sleep(dur: Duration, handle: &DownloadHandle) -> bool {
    let tick = Duration::from_millis(50);
    let mut elapsed = Duration::ZERO;
    while elapsed < dur {
        if handle.is_stopped() || handle.is_cancelled() {
            return true;
        }
        let step = tick.min(dur - elapsed);
        tokio::time::sleep(step).await;
        elapsed += step;
    }
    handle.is_stopped() || handle.is_cancelled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_interval_clamps_and_defaults() {
        assert_eq!(RefreshInterval::FromTargetDuration.compute(4.0), Duration::from_secs_f64(4.0));
        assert_eq!(RefreshInterval::FromTargetDuration.compute(0.0), Duration::from_secs(6));
        assert_eq!(RefreshInterval::FromTargetDuration.compute(0.3), Duration::from_secs(1));
        assert_eq!(RefreshInterval::Fixed(Duration::from_millis(7)).compute(99.0), Duration::from_millis(7));
    }

    #[tokio::test]
    async fn responsive_sleep_returns_early_on_stop() {
        let h = DownloadHandle::new();
        h.stop();
        // Doit revenir quasi immédiatement et signaler l'interruption.
        assert!(responsive_sleep(Duration::from_secs(60), &h).await);
    }

    #[tokio::test]
    async fn responsive_sleep_completes_when_not_interrupted() {
        let h = DownloadHandle::new();
        assert!(!responsive_sleep(Duration::from_millis(10), &h).await);
    }
}
