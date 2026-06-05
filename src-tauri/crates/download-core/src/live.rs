//! Primitives communes aux moteurs d'enregistrement live (HLS et DASH) :
//! cadence de rafraîchissement et sleep interruptible.
use crate::engine::DownloadHandle;
use std::time::Duration;

/// Intervalle entre deux rafraîchissements (playlist HLS / MPD DASH).
#[derive(Debug, Clone, Copy)]
pub enum RefreshInterval {
    /// Production : dérive de la période naturelle du flux (HLS target-duration,
    /// DASH minimumUpdatePeriod). Borne min 1 s, défaut 6 s si absente/nulle.
    Derived,
    /// Tests : intervalle fixe (ex. 0–10 ms pour ne pas dormir).
    Fixed(Duration),
}

impl RefreshInterval {
    pub fn compute(&self, base_secs: f64) -> Duration {
        match self {
            RefreshInterval::Derived => {
                let secs = if base_secs > 0.0 { base_secs } else { 6.0 };
                Duration::from_secs_f64(secs.max(1.0))
            }
            RefreshInterval::Fixed(d) => *d,
        }
    }
}

/// Dort jusqu'à `dur` en vérifiant stop/cancel toutes les 50 ms.
/// Renvoie `true` si interrompu (stop ou cancel demandé) avant l'échéance.
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
        assert_eq!(RefreshInterval::Derived.compute(4.0), Duration::from_secs_f64(4.0));
        assert_eq!(RefreshInterval::Derived.compute(0.0), Duration::from_secs(6));
        assert_eq!(RefreshInterval::Derived.compute(0.3), Duration::from_secs(1));
        assert_eq!(RefreshInterval::Fixed(Duration::from_millis(7)).compute(99.0), Duration::from_millis(7));
    }

    #[tokio::test]
    async fn responsive_sleep_returns_early_on_stop() {
        let h = DownloadHandle::new();
        h.stop();
        assert!(responsive_sleep(Duration::from_secs(60), &h).await);
    }

    #[tokio::test]
    async fn responsive_sleep_completes_when_not_interrupted() {
        let h = DownloadHandle::new();
        assert!(!responsive_sleep(Duration::from_millis(10), &h).await);
    }
}
