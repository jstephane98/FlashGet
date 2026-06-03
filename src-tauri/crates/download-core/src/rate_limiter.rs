use async_trait::async_trait;
use std::sync::Mutex;

/// Horloge injectable : la prod mesure le temps réel et dort réellement ;
/// les tests avancent un compteur sans attendre (déterministe).
#[async_trait]
pub trait Clock: Send + Sync {
    fn now_secs(&self) -> f64;
    async fn sleep(&self, secs: f64);
}

/// Horloge réelle basée sur un `Instant` monotone.
pub struct RealClock {
    start: std::time::Instant,
}

impl RealClock {
    pub fn new() -> Self {
        Self { start: std::time::Instant::now() }
    }
}

impl Default for RealClock {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Clock for RealClock {
    fn now_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
    async fn sleep(&self, secs: f64) {
        if secs > 0.0 {
            tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
        }
    }
}

struct Bucket {
    rate: f64,   // octets/seconde ; 0 => illimité
    tokens: f64, // budget courant (peut être négatif = dette)
    cap: f64,    // plafond de budget (borne le burst)
    last: f64,   // dernier instant de recharge (secondes)
}

/// Token bucket partagé : `acquire(n)` ne rend la main qu'une fois que `n`
/// octets de budget sont disponibles (en dormant la dette éventuelle).
pub struct RateLimiter {
    inner: Mutex<Bucket>,
    clock: std::sync::Arc<dyn Clock>,
}

impl RateLimiter {
    pub fn new(max_bps: u64) -> Self {
        Self::with_clock(max_bps, std::sync::Arc::new(RealClock::new()))
    }

    pub fn unlimited() -> Self {
        Self::new(0)
    }

    pub fn with_clock(max_bps: u64, clock: std::sync::Arc<dyn Clock>) -> Self {
        let rate = max_bps as f64;
        let now = clock.now_secs();
        Self {
            inner: Mutex::new(Bucket { rate, tokens: 0.0, cap: rate.max(1.0), last: now }),
            clock,
        }
    }

    /// Ajuste le débit à chaud (0 => illimité).
    pub fn set_rate(&self, max_bps: u64) {
        let mut b = self.inner.lock().unwrap();
        b.rate = max_bps as f64;
        b.cap = b.rate.max(1.0);
        if b.tokens > b.cap {
            b.tokens = b.cap;
        }
    }

    /// Calcul pur : recharge le bucket selon le temps écoulé, retire `n` octets,
    /// et renvoie le nombre de secondes à attendre (0 si budget suffisant ou illimité).
    /// Ne tient AUCUN verrou pendant l'attente (l'appelant dort après).
    ///
    /// Modèle de dette assumé : sous N appelants concurrents, `tokens` peut devenir
    /// très négatif et chaque appelant dort sa propre part de la dette. Le débit
    /// *moyen agrégé* reste borné à `rate` ; `cap` ne borne que l'accumulation à
    /// l'inactivité (le burst), pas la profondeur de dette sous charge concurrente.
    #[must_use]
    fn take(&self, n: u64) -> f64 {
        let mut b = self.inner.lock().unwrap();
        if b.rate <= 0.0 {
            return 0.0; // illimité : pass-through
        }
        let now = self.clock.now_secs();
        let elapsed = (now - b.last).max(0.0);
        b.tokens = (b.tokens + elapsed * b.rate).min(b.cap);
        b.last = now;
        b.tokens -= n as f64;
        if b.tokens < 0.0 {
            -b.tokens / b.rate
        } else {
            0.0
        }
    }

    /// Attend (si nécessaire) l'autorisation de consommer `n` octets.
    pub async fn acquire(&self, n: u64) {
        let wait = self.take(n); // verrou relâché à la fin de take()
        if wait > 0.0 {
            self.clock.sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Horloge de test : `now` n'avance que via `sleep`/`advance` (pas de temps réel).
    struct TestClock {
        now: Mutex<f64>,
        slept: Mutex<f64>,
    }
    impl TestClock {
        fn new() -> Arc<Self> {
            Arc::new(Self { now: Mutex::new(0.0), slept: Mutex::new(0.0) })
        }
        fn advance(&self, secs: f64) {
            *self.now.lock().unwrap() += secs;
        }
        fn slept(&self) -> f64 {
            *self.slept.lock().unwrap()
        }
    }
    #[async_trait]
    impl Clock for TestClock {
        fn now_secs(&self) -> f64 {
            *self.now.lock().unwrap()
        }
        async fn sleep(&self, secs: f64) {
            *self.now.lock().unwrap() += secs;
            *self.slept.lock().unwrap() += secs;
        }
    }

    #[test]
    fn unlimited_never_waits() {
        let rl = RateLimiter::unlimited();
        assert_eq!(rl.take(1_000_000), 0.0);
    }

    #[test]
    fn accumulates_debt_at_fixed_rate() {
        let clock = TestClock::new();
        let rl = RateLimiter::with_clock(1000, clock.clone());
        // À t=0, budget 0 : retirer 500 => dette 500 => attente 0,5 s.
        assert!((rl.take(500) - 0.5).abs() < 1e-9);
        // Toujours t=0 : dette cumulée 1000 => attente 1,0 s.
        assert!((rl.take(500) - 1.0).abs() < 1e-9);
        // Avance d'1 s : recharge 1000 octets => dette épongée.
        clock.advance(1.0);
        assert!((rl.take(500) - 0.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn acquire_bounds_average_rate() {
        let clock = TestClock::new();
        let rl = RateLimiter::with_clock(1000, clock.clone());
        // Consommer 1000 octets à 1000 o/s doit faire dormir ~1 s au total.
        for _ in 0..10 {
            rl.acquire(100).await;
        }
        assert!((clock.slept() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn set_rate_switches_to_unlimited() {
        let clock = TestClock::new();
        let rl = RateLimiter::with_clock(1000, clock.clone());
        rl.set_rate(0);
        assert_eq!(rl.take(10_000), 0.0);
    }
}
