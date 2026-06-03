# Phase 2a — Moteur de gestion — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ajouter au download manager une file d'attente avec limite de simultanés, démarrage automatique, priorités (Haute/Normale/Basse), et un limiteur de bande passante global — le tout piloté par un scheduler à décision pure, avec réglages persistants et UI associée.

**Architecture:** Un module `scheduler` (fonction pure testable) décide quels téléchargements `Queued` démarrer selon la limite de simultanés et la priorité. Un module `rate_limiter` (token bucket à horloge injectable) plafonne le débit cumulé, injecté dans la boucle de chunks du moteur. La couche Tauri détient un `RateLimiter` et un `reqwest::Client` mutualisés, et ré-évalue la file à chaque événement (ajout, fin, pause, reprise, annulation, changement de réglages).

**Tech Stack:** Rust (tokio, rusqlite, async-trait — déjà présents), React + TypeScript + Tailwind + Vitest. Aucune nouvelle dépendance.

**Spec :** [docs/superpowers/specs/2026-06-03-phase-2a-management-design.md](../specs/2026-06-03-phase-2a-management-design.md).

---

## État de départ (Phase 1, déjà en place)

- `download-core` : `model.rs` (`Download{id,url,file_name,dest_path,total_bytes:Option<u64>,downloaded_bytes:u64,status:DownloadStatus,supports_ranges,error}`, `DownloadStatus{Queued,Downloading,Paused,Completed,Error}` sérialisé en minuscules, `Progress`), `segments.rs`, `http.rs` (`ByteFetcher`, `ReqwestFetcher{client}`), `resume.rs`, `engine.rs` (`download(fetcher:&F, id, url, dest, handle:DownloadHandle, progress) -> Result<Outcome>`, `DownloadHandle`, `Outcome`), `storage.rs` (`Storage{Mutex<Connection>}` : `open/open_in_memory/upsert/list/delete/update_state/mark_error`).
- App Tauri : `state.rs` (`AppState{storage, handles:Mutex<HashMap<String,DownloadHandle>>}`), `commands.rs` (`add_download/list_downloads/start_download/pause_download/cancel_download`), `lib.rs` (setup + `generate_handler!`).
- Frontend : `types.ts`, `api.ts`, `format.ts`, `App.tsx`, `components/{Toolbar,AddUrlDialog,DownloadTable,DownloadRow}.tsx`.

Suite de tests actuelle : `cargo test -p download-core` (24) + `npm test` (3).

## Structure des fichiers (2a)

```
src-tauri/crates/download-core/src/
  rate_limiter.rs   (NOUVEAU) token bucket + Clock injectable
  scheduler.rs      (NOUVEAU) pick_to_start (pur)
  model.rs          (+) Priority, Settings, champ Download.priority
  storage.rs        (+) migration priority/settings, get_by_id, set_priority, get/set_settings, list ORDER BY rowid
  engine.rs         (+) param limiter: Arc<RateLimiter> + acquire() par chunk
  lib.rs            (+) pub mod rate_limiter; pub mod scheduler;
src-tauri/src/
  state.rs          (+) fetcher mutualisé + limiter partagé
  commands.rs       (refactor) launch/reevaluate + nouvelles commandes
  lib.rs            (+) enregistrer les nouvelles commandes, initialiser le limiter
src/
  types.ts          (+) Priority, Settings
  api.ts            (+) setPriority, getSettings, updateSettings, pauseAll, resumeAll
  components/
    PriorityPill.tsx   (NOUVEAU)
    SettingsDialog.tsx (NOUVEAU)
    Toolbar.tsx        (+) Tout/⚙/débit
    DownloadRow.tsx    (+) colonne priorité, libellé "En file (n)"
    DownloadTable.tsx  (+) en-tête Priorité + position file
  App.tsx           (refactor) plus d'auto-start après add, settings, queue
```

---

## Task 1 : `rate_limiter.rs` — token bucket à horloge injectable (TDD)

**Files:**
- Create: `src-tauri/crates/download-core/src/rate_limiter.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs` (ajouter `pub mod rate_limiter;`)

- [ ] **Step 1: Déclarer le module**

Dans `lib.rs`, ajouter à la liste des modules (après `pub mod engine;`) :
```rust
pub mod rate_limiter;
pub mod scheduler;
```
(`scheduler` sera créé en Task 3 ; créer aussi un `scheduler.rs` vide avec `// rempli en Task 3` pour que ça compile.)

- [ ] **Step 2: Écrire le module avec ses tests**

`src-tauri/crates/download-core/src/rate_limiter.rs` :
```rust
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
```

- [ ] **Step 3: Lancer les tests → échec compil du `scheduler` vide possible, puis PASS**

Run:
```bash
cd src-tauri && cargo test -p download-core rate_limiter
```
Expected: PASS (4 tests). Si erreur « module scheduler not found », vérifier que `scheduler.rs` vide existe.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(core): global rate limiter (token bucket) with injectable clock (TDD)"
```

---

## Task 2 : Modèle + persistance — Priority, Settings, migration (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs`
- Modify: `src-tauri/crates/download-core/src/storage.rs`
- Modify: `src-tauri/src/commands.rs` (construction de `Download` dans `add_download`)

- [ ] **Step 1: Étendre le modèle**

Dans `model.rs`, ajouter après l'enum `DownloadStatus` :
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    High,
    #[default]
    Normal,
    Low,
}

/// Réglages persistants de l'application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub max_concurrent: u32,
    /// Vitesse de téléchargement max en octets/seconde (0 = illimité).
    pub max_speed_bps: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { max_concurrent: 3, max_speed_bps: 0 }
    }
}
```

Ajouter le champ `priority` à `Download` (avec `#[serde(default)]` pour rester compatible) :
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub dest_path: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub supports_ranges: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub priority: Priority,
}
```

- [ ] **Step 2: Mettre à jour la construction de `Download` dans `add_download`**

Dans `src-tauri/src/commands.rs`, dans `add_download`, ajouter le champ à la struct littérale `Download { ... }` (juste après `error: None,`) :
```rust
        error: None,
        priority: download_core::model::Priority::Normal,
```

- [ ] **Step 3: Écrire les tests storage (échouent à la compilation tant que l'impl n'est pas faite)**

Dans `storage.rs`, mettre à jour le helper `sample()` du module de tests pour inclure la priorité, et ajouter les nouveaux tests :
```rust
    fn sample(id: &str) -> Download {
        Download {
            id: id.into(),
            url: "http://x/f".into(),
            file_name: "f.bin".into(),
            dest_path: "/tmp/f.bin".into(),
            total_bytes: Some(100),
            downloaded_bytes: 40,
            status: DownloadStatus::Paused,
            supports_ranges: true,
            error: None,
            priority: Priority::Normal,
        }
    }

    #[test]
    fn round_trips_priority() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("p");
        d.priority = Priority::High;
        s.upsert(&d).unwrap();
        assert_eq!(s.list().unwrap()[0].priority, Priority::High);
    }

    #[test]
    fn set_priority_updates_row() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        s.set_priority("a", Priority::Low).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().priority, Priority::Low);
    }

    #[test]
    fn get_by_id_absent_is_none() {
        let s = Storage::open_in_memory().unwrap();
        assert!(s.get_by_id("nope").unwrap().is_none());
    }

    #[test]
    fn settings_default_then_roundtrip() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.get_settings().unwrap(), Settings::default());
        s.set_settings(&Settings { max_concurrent: 5, max_speed_bps: 1024 }).unwrap();
        assert_eq!(s.get_settings().unwrap(), Settings { max_concurrent: 5, max_speed_bps: 1024 });
    }

    #[test]
    fn list_is_ordered_by_insertion() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("first")).unwrap();
        s.upsert(&sample("second")).unwrap();
        let ids: Vec<String> = s.list().unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["first", "second"]);
    }
```

Mettre à jour l'import du module de tests : `use super::*;` suffit (déjà présent). Ajouter `Priority` à l'import en tête de fichier (voir Step 4).

- [ ] **Step 4: Implémenter dans `storage.rs`**

En tête, élargir l'import :
```rust
use crate::model::{Download, DownloadStatus, Priority, Settings};
```

Dans `from_conn`, après le `CREATE TABLE IF NOT EXISTS downloads (...)` (avant le `Ok(...)`), ajouter la migration additive et la table settings :
```rust
        // Migration additive : colonne priority (ignorer l'erreur si déjà présente).
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal'",
            [],
        );
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
```

Mettre à jour `upsert` pour écrire `priority` :
```rust
    pub fn upsert(&self, d: &Download) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9, priority=?10",
            rusqlite::params![
                d.id, d.url, d.file_name, d.dest_path,
                d.total_bytes.map(|v| v as i64),
                d.downloaded_bytes as i64,
                status_to_str(d.status),
                d.supports_ranges as i64,
                d.error,
                priority_to_str(d.priority),
            ],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
```

Remplacer `list` pour lire `priority` et trier par ordre d'insertion :
```rust
    pub fn list(&self) -> Result<Vec<Download>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority FROM downloads ORDER BY rowid")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Download {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    dest_path: row.get(3)?,
                    total_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    downloaded_bytes: row.get::<_, i64>(5)? as u64,
                    status: status_from_str(&row.get::<_, String>(6)?),
                    supports_ranges: row.get::<_, i64>(7)? != 0,
                    error: row.get(8)?,
                    priority: priority_from_str(&row.get::<_, String>(9)?),
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| CoreError::Storage(e.to_string()))?);
        }
        Ok(out)
    }
```

Ajouter les nouvelles méthodes (après `mark_error`) :
```rust
    pub fn get_by_id(&self, id: &str) -> Result<Option<Download>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority FROM downloads WHERE id=?1")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut rows = stmt
            .query_map([id], |row| {
                Ok(Download {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    dest_path: row.get(3)?,
                    total_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    downloaded_bytes: row.get::<_, i64>(5)? as u64,
                    status: status_from_str(&row.get::<_, String>(6)?),
                    supports_ranges: row.get::<_, i64>(7)? != 0,
                    error: row.get(8)?,
                    priority: priority_from_str(&row.get::<_, String>(9)?),
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| CoreError::Storage(e.to_string()))?)),
            None => Ok(None),
        }
    }

    pub fn set_priority(&self, id: &str, priority: Priority) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET priority=?1 WHERE id=?2",
            rusqlite::params![priority_to_str(priority), id],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_settings(&self) -> Result<Settings, CoreError> {
        let conn = self.conn.lock().unwrap();
        let read = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get::<_, String>(0)).ok()
        };
        let mut s = Settings::default();
        if let Some(v) = read("max_concurrent") {
            if let Ok(n) = v.parse::<u32>() { s.max_concurrent = n; }
        }
        if let Some(v) = read("max_speed_bps") {
            if let Ok(n) = v.parse::<u64>() { s.max_speed_bps = n; }
        }
        Ok(s)
    }

    pub fn set_settings(&self, s: &Settings) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut put = |key: &str, val: String| -> Result<(), CoreError> {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
                rusqlite::params![key, val],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;
            Ok(())
        };
        put("max_concurrent", s.max_concurrent.to_string())?;
        put("max_speed_bps", s.max_speed_bps.to_string())?;
        Ok(())
    }
```

Ajouter les helpers de conversion (à côté de `status_to_str`) :
```rust
fn priority_to_str(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

fn priority_from_str(s: &str) -> Priority {
    match s {
        "high" => Priority::High,
        "low" => Priority::Low,
        _ => Priority::Normal,
    }
}
```

- [ ] **Step 5: Lancer les tests → PASS**

Run:
```bash
cd src-tauri && cargo test -p download-core storage
```
Expected: PASS (tests existants + 5 nouveaux). Puis `cargo build` pour confirmer que `commands.rs` compile avec le nouveau champ.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): persist priority + settings, get_by_id, ordered list (TDD)"
```

---

## Task 3 : `scheduler.rs` — décision pure (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/scheduler.rs`

- [ ] **Step 1: Écrire la fonction + tests**

`src-tauri/crates/download-core/src/scheduler.rs` :
```rust
use crate::model::{Download, DownloadStatus, Priority};

/// Étant donné l'ensemble des téléchargements et la limite de simultanés,
/// renvoie les ids des téléchargements `Queued` à démarrer, choisis par
/// priorité décroissante puis par ordre d'arrivée (l'ordre du slice).
/// Idempotent : ne renvoie jamais un id déjà `Downloading` / `Paused` / terminé.
pub fn pick_to_start(downloads: &[Download], max_concurrent: u32) -> Vec<String> {
    let active = downloads
        .iter()
        .filter(|d| d.status == DownloadStatus::Downloading)
        .count() as u32;
    let slots = max_concurrent.saturating_sub(active);
    if slots == 0 {
        return Vec::new();
    }
    let mut queued: Vec<&Download> = downloads
        .iter()
        .filter(|d| d.status == DownloadStatus::Queued)
        .collect();
    // Tri STABLE par rang de priorité : conserve l'ordre d'arrivée à priorité égale.
    queued.sort_by_key(|d| priority_rank(d.priority));
    queued.into_iter().take(slots as usize).map(|d| d.id.clone()).collect()
}

fn priority_rank(p: Priority) -> u8 {
    match p {
        Priority::High => 0,
        Priority::Normal => 1,
        Priority::Low => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dl(id: &str, status: DownloadStatus, priority: Priority) -> Download {
        Download {
            id: id.into(),
            url: "u".into(),
            file_name: "f".into(),
            dest_path: "/f".into(),
            total_bytes: Some(1),
            downloaded_bytes: 0,
            status,
            supports_ranges: true,
            error: None,
            priority,
        }
    }

    #[test]
    fn starts_up_to_the_limit() {
        let items = vec![
            dl("a", DownloadStatus::Queued, Priority::Normal),
            dl("b", DownloadStatus::Queued, Priority::Normal),
            dl("c", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 2), vec!["a", "b"]);
    }

    #[test]
    fn counts_active_against_the_limit() {
        let items = vec![
            dl("x", DownloadStatus::Downloading, Priority::Normal),
            dl("a", DownloadStatus::Queued, Priority::Normal),
            dl("b", DownloadStatus::Queued, Priority::Normal),
        ];
        // 1 actif, limite 2 => 1 slot libre => démarre seulement "a".
        assert_eq!(pick_to_start(&items, 2), vec!["a"]);
    }

    #[test]
    fn higher_priority_first_then_fifo() {
        let items = vec![
            dl("a", DownloadStatus::Queued, Priority::Low),
            dl("b", DownloadStatus::Queued, Priority::High),
            dl("c", DownloadStatus::Queued, Priority::Normal),
            dl("d", DownloadStatus::Queued, Priority::High),
        ];
        // High d'abord (b avant d, FIFO), puis Normal (c), puis Low (a).
        assert_eq!(pick_to_start(&items, 3), vec!["b", "d", "c"]);
    }

    #[test]
    fn ignores_paused_completed_error() {
        let items = vec![
            dl("p", DownloadStatus::Paused, Priority::High),
            dl("done", DownloadStatus::Completed, Priority::High),
            dl("err", DownloadStatus::Error, Priority::High),
            dl("a", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 5), vec!["a"]);
    }

    #[test]
    fn no_slots_returns_empty() {
        let items = vec![
            dl("x", DownloadStatus::Downloading, Priority::Normal),
            dl("y", DownloadStatus::Downloading, Priority::Normal),
            dl("a", DownloadStatus::Queued, Priority::High),
        ];
        assert_eq!(pick_to_start(&items, 2), Vec::<String>::new());
    }
}
```

- [ ] **Step 2: Lancer les tests → PASS**

Run:
```bash
cd src-tauri && cargo test -p download-core scheduler
```
Expected: PASS (5 tests).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(core): scheduler pick_to_start (priority + FIFO) with TDD"
```

---

## Task 4 : Intégrer le limiteur dans le moteur (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/engine.rs`
- Modify: `src-tauri/crates/download-core/tests/real_http.rs`

- [ ] **Step 1: Ajouter le paramètre `limiter` à `download`**

Dans `engine.rs`, changer la signature et l'en-tête d'imports. En haut, ajouter :
```rust
use crate::rate_limiter::RateLimiter;
```
Changer la signature :
```rust
pub async fn download<F: ByteFetcher>(
    fetcher: &F,
    id: &str,
    url: &str,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
) -> Result<Outcome, CoreError> {
```

Dans le **chemin mono-flux** (`if state.segments.is_empty()`), juste après `let chunk = chunk?;` et AVANT `file.write_all(&chunk).await?;`, insérer :
```rust
            limiter.acquire(chunk.len() as u64).await;
```

Dans le **chemin segmenté**, chaque future doit capturer un clone du limiteur. Avant `let fut = async move {`, ajouter :
```rust
        let limiter = limiter.clone();
```
Puis dans la boucle de la future, après `let chunk = chunk?;` et AVANT `file.write_all(&chunk).await?;`, insérer :
```rust
                limiter.acquire(chunk.len() as u64).await;
```

- [ ] **Step 2: Mettre à jour les tests du moteur**

Dans le module `#[cfg(test)] mod tests` d'`engine.rs`, chaque appel `download(&fetcher, "...", "...", &dest, handle, tx)` doit recevoir un limiteur illimité. Ajouter en haut du module de tests :
```rust
    use crate::rate_limiter::RateLimiter;
    fn unlimited() -> std::sync::Arc<RateLimiter> {
        std::sync::Arc::new(RateLimiter::unlimited())
    }
```
Puis dans CHAQUE test, insérer le limiteur en avant-dernier argument. Exemple pour `downloads_full_file_multisegment` :
```rust
        let outcome = download(&fetcher, "id1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx)
            .await
            .unwrap();
```
Faire de même pour `falls_back_to_single_stream_when_no_ranges`, `emits_progress`, `resume_after_pause_completes_file` (les deux appels), et `resume_preserves_partial_bytes` (les deux appels) : insérer `unlimited()` juste avant le `tx`/`tx2`.

- [ ] **Step 3: Mettre à jour le test d'intégration réel**

Dans `tests/real_http.rs`, ajouter l'import et le limiteur. En haut :
```rust
use download_core::rate_limiter::RateLimiter;
use std::sync::Arc;
```
Dans les deux tests, insérer `Arc::new(RateLimiter::unlimited())` juste avant l'argument `tx` de l'appel à `download(...)`.

- [ ] **Step 4: Lancer toute la suite du cœur → PASS**

Run:
```bash
cd src-tauri && cargo test -p download-core
```
Expected: PASS (tous : rate_limiter, scheduler, storage, engine, segments, http, resume, real_http).

- [ ] **Step 5: (TDD) Test que la limite plafonne le débit cumulé**

Ajouter dans le module de tests d'`engine.rs` un test utilisant une horloge contrôlée. Comme le moteur utilise `RateLimiter` avec une horloge réelle par défaut, on injecte une horloge de test via `RateLimiter::with_clock`. Réutiliser une horloge déterministe locale :
```rust
    #[tokio::test]
    async fn throttle_makes_engine_wait() {
        use crate::rate_limiter::Clock;
        use std::sync::Mutex;
        struct Fake { now: Mutex<f64>, slept: Mutex<f64> }
        #[async_trait::async_trait]
        impl Clock for Fake {
            fn now_secs(&self) -> f64 { *self.now.lock().unwrap() }
            async fn sleep(&self, s: f64) {
                *self.now.lock().unwrap() += s;
                *self.slept.lock().unwrap() += s;
            }
        }
        let clock = std::sync::Arc::new(Fake { now: Mutex::new(0.0), slept: Mutex::new(0.0) });
        let limiter = std::sync::Arc::new(RateLimiter::with_clock(1000, clock.clone()));

        let data: Vec<u8> = vec![7u8; 2000]; // 2000 octets à 1000 o/s => ~2 s simulées
        let fetcher = FakeFetcher::new(data, true);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        let (tx, _rx) = mpsc::unbounded_channel();

        download(&fetcher, "t", "http://x/f", &dest, DownloadHandle::new(), limiter, tx)
            .await
            .unwrap();

        let slept = *clock.slept.lock().unwrap();
        assert!(slept >= 1.9, "le moteur aurait dû dormir ~2 s (simulées), got {slept}");
    }
```
Run:
```bash
cd src-tauri && cargo test -p download-core engine::tests::throttle_makes_engine_wait
```
Expected: PASS. (`async-trait` est déjà dépendance du crate.)

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): wire global rate limiter into the engine chunk loops (TDD)"
```

---

## Task 5 : Câblage Tauri — état partagé, ré-évaluation, nouvelles commandes

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/crates/download-core/src/http.rs` (rendre `ReqwestFetcher` clonable)

- [ ] **Step 1: Rendre `ReqwestFetcher` clonable (client mutualisé)**

Dans `http.rs`, ajouter `#[derive(Clone)]` sur la struct :
```rust
#[derive(Clone)]
pub struct ReqwestFetcher {
    client: reqwest::Client,
}
```
(`reqwest::Client` est déjà `Clone` — partage le pool interne.)

- [ ] **Step 2: Étendre `AppState`**

`src-tauri/src/state.rs` :
```rust
use download_core::engine::DownloadHandle;
use download_core::http::ReqwestFetcher;
use download_core::rate_limiter::RateLimiter;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// État global géré par Tauri.
pub struct AppState {
    pub storage: Storage,
    pub handles: Mutex<HashMap<String, DownloadHandle>>,
    /// Fetcher mutualisé (un seul `reqwest::Client` partagé).
    pub fetcher: ReqwestFetcher,
    /// Limiteur de bande passante global partagé.
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(storage: Storage, max_speed_bps: u64) -> Self {
        Self {
            storage,
            handles: Mutex::new(HashMap::new()),
            fetcher: ReqwestFetcher::new(),
            limiter: Arc::new(RateLimiter::new(max_speed_bps)),
        }
    }
}
```

- [ ] **Step 3: Réécrire `commands.rs`**

Remplacer intégralement `src-tauri/src/commands.rs` par :
```rust
use crate::state::AppState;
use download_core::engine::{self, DownloadHandle};
use download_core::http::ByteFetcher;
use download_core::model::{Download, DownloadStatus, Priority, Progress, Settings};
use download_core::scheduler::pick_to_start;
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

#[derive(serde::Deserialize)]
pub struct AddArgs {
    pub url: String,
    pub dest_dir: String,
}

/// Crée un téléchargement (statut Queued), le persiste, puis ré-évalue la file.
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(&args.url))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;
    let file_name = probe.file_name.clone().unwrap_or_else(|| "download.bin".to_string());

    let dest_dir = if args.dest_dir.trim().is_empty() {
        app.path().download_dir().map_err(|e| e.to_string())?
    } else {
        PathBuf::from(&args.dest_dir)
    };
    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("invalid file name derived from URL".to_string());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: args.url.clone(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: probe.total_bytes,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: probe.supports_ranges,
        error: None,
        priority: Priority::Normal,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(download)
}

#[tauri::command]
pub fn list_downloads(state: State<AppState>) -> Result<Vec<Download>, String> {
    state.storage.list().map_err(|e| e.to_string())
}

/// Reprend un téléchargement (le repasse `Queued`) puis ré-évalue.
#[tauri::command]
pub fn start_download(app: AppHandle, id: String) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let d = state.storage.get_by_id(&id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("download not found: {id}"))?;
        // Ne rien faire si déjà actif.
        if state.handles.lock().unwrap().contains_key(&id) {
            return Ok(());
        }
        state.storage.update_state(&id, d.downloaded_bytes, DownloadStatus::Queued)
            .map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn pause_download(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.pause();
    }
    // Le slot se libérera quand la tâche moteur se terminera (elle ré-évalue alors).
    Ok(())
}

#[tauri::command]
pub fn cancel_download(app: AppHandle, id: String) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut handles = state.handles.lock().unwrap();
        if let Some(h) = handles.get(&id) {
            h.cancel();
        }
        handles.remove(&id);
        state.storage.delete(&id).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn set_priority(app: AppHandle, id: String, priority: Priority) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        state.storage.set_priority(&id, priority).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    state.storage.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(app: AppHandle, max_concurrent: u32, max_speed_bps: u64) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let s = Settings { max_concurrent: max_concurrent.max(1), max_speed_bps };
        state.storage.set_settings(&s).map_err(|e| e.to_string())?;
        state.limiter.set_rate(s.max_speed_bps);
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn pause_all(state: State<AppState>) -> Result<(), String> {
    let handles = state.handles.lock().unwrap();
    for h in handles.values() {
        h.pause();
    }
    Ok(())
}

#[tauri::command]
pub fn resume_all(app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let downloads = state.storage.list().map_err(|e| e.to_string())?;
        for d in downloads.iter().filter(|d| d.status == DownloadStatus::Paused) {
            state.storage.update_state(&d.id, d.downloaded_bytes, DownloadStatus::Queued)
                .map_err(|e| e.to_string())?;
        }
    }
    reevaluate(&app);
    Ok(())
}

/// Ré-évalue la file : démarre les `Queued` élus selon la limite + priorités.
/// Atomique vis-à-vis de la map `handles` (verrou tenu pendant pick + launch).
fn reevaluate(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = match state.storage.get_settings() {
        Ok(s) => s,
        Err(e) => { eprintln!("reevaluate: get_settings failed: {e}"); return; }
    };
    let downloads = match state.storage.list() {
        Ok(d) => d,
        Err(e) => { eprintln!("reevaluate: list failed: {e}"); return; }
    };
    let to_start = pick_to_start(&downloads, settings.max_concurrent);
    if to_start.is_empty() {
        return;
    }
    // Verrou tenu pour rendre pick+launch atomique (évite un double-démarrage concurrent).
    let mut handles = state.handles.lock().unwrap();
    for id in to_start {
        if handles.contains_key(&id) {
            continue; // déjà actif
        }
        let download = match downloads.iter().find(|d| d.id == id) {
            Some(d) => d.clone(),
            None => continue,
        };
        let handle = DownloadHandle::new();
        handles.insert(id.clone(), handle.clone());
        // Marquer Downloading tout de suite (source de vérité pour les prochains pick).
        let _ = state.storage.update_state(&id, download.downloaded_bytes, DownloadStatus::Downloading);
        launch(app, download, handle);
    }
}

/// Lance les deux tâches (relais de progression + moteur) pour un téléchargement.
fn launch(app: &AppHandle, download: Download, handle: DownloadHandle) {
    let id = download.id.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    // Relais de progression : vitesse + emit + persistance des états terminaux.
    let app_for_rx = app.clone();
    let id_for_rx = id.clone();
    tokio::spawn(async move {
        let mut last_t = Instant::now();
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;
        while let Some(mut p) = rx.recv().await {
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64();
            if dt >= 0.5 {
                let delta = p.downloaded_bytes.saturating_sub(last_bytes);
                current_speed = (delta as f64 / dt) as u64;
                last_t = now;
                last_bytes = p.downloaded_bytes;
            }
            p.speed_bps = current_speed;
            let _ = app_for_rx.emit("download://progress", &p);
            if matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error) {
                let st = app_for_rx.state::<AppState>();
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
            }
        }
    });

    // Tâche moteur.
    let app_for_dl = app.clone();
    let dest = PathBuf::from(&download.dest_path);
    let url = download.url.clone();
    let id_for_dl = id.clone();
    let handle_for_err = handle.clone();
    let (fetcher, limiter) = {
        let st = app.state::<AppState>();
        (st.fetcher.clone(), st.limiter.clone())
    };
    tokio::spawn(async move {
        match engine::download(&fetcher, &id_for_dl, &url, &dest, handle, limiter, tx).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("download {id_for_dl} failed: {e}");
                let bytes = handle_for_err.downloaded();
                let st = app_for_dl.state::<AppState>();
                if let Err(se) = st.storage.mark_error(&id_for_dl, bytes, &e.to_string()) {
                    eprintln!("failed to persist error for {id_for_dl}: {se}");
                }
                let _ = app_for_dl.emit("download://progress", &Progress {
                    id: id_for_dl.clone(),
                    downloaded_bytes: bytes,
                    total_bytes: None,
                    speed_bps: 0,
                    status: DownloadStatus::Error,
                });
            }
        }
        // Libérer le slot et relancer la file.
        {
            let st = app_for_dl.state::<AppState>();
            st.handles.lock().unwrap().remove(&id_for_dl);
        }
        reevaluate(&app_for_dl);
    });
}
```

- [ ] **Step 4: Mettre à jour `lib.rs`**

Dans `src-tauri/src/lib.rs` : initialiser le débit du limiteur depuis les réglages persistés, et enregistrer les nouvelles commandes.
```rust
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            std::fs::create_dir_all(&dir).ok();
            let storage = Storage::open(&dir.join("downloads.sqlite")).map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let settings = storage.get_settings().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            app.manage(AppState::new(storage, settings.max_speed_bps));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_download,
            commands::list_downloads,
            commands::start_download,
            commands::pause_download,
            commands::cancel_download,
            commands::set_priority,
            commands::get_settings,
            commands::update_settings,
            commands::pause_all,
            commands::resume_all,
        ])
```

- [ ] **Step 5: Compiler**

Run:
```bash
cd src-tauri && cargo build && cargo test -p download-core
```
Expected: build OK ; cœur toujours vert. Corriger tout import inutilisé.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(app): shared client + limiter, queue reevaluation, priority/settings/pause-all commands"
```

---

## Task 6 : Frontend — types & couche API

**Files:**
- Modify: `src/types.ts`, `src/api.ts`

- [ ] **Step 1: Étendre `types.ts`**

Ajouter à `src/types.ts` :
```ts
export type Priority = "high" | "normal" | "low";

export interface Download {
  id: string;
  url: string;
  file_name: string;
  dest_path: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: DownloadStatus;
  supports_ranges: boolean;
  error: string | null;
  priority: Priority;
}

export interface Settings {
  max_concurrent: number;
  max_speed_bps: number;
}
```
(Remplacer l'interface `Download` existante par celle ci-dessus — seul `priority` est ajouté.)

- [ ] **Step 2: Étendre `api.ts`**

Ajouter à `src/api.ts` (après les fonctions existantes) :
```ts
import type { Download, Progress, Priority, Settings } from "./types";

export function setPriority(id: string, priority: Priority): Promise<void> {
  return invoke("set_priority", { id, priority });
}

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export function updateSettings(maxConcurrent: number, maxSpeedBps: number): Promise<void> {
  return invoke("update_settings", { maxConcurrent, maxSpeedBps });
}

export function pauseAll(): Promise<void> {
  return invoke("pause_all");
}

export function resumeAll(): Promise<void> {
  return invoke("resume_all");
}
```
(Veiller à ce que la ligne `import type { Download, Progress } from "./types";` existante inclue bien `Priority, Settings` — fusionner l'import.)

- [ ] **Step 3: Vérifier les types**

Run:
```bash
npx tsc --noEmit
```
Expected: aucune erreur.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ui): TS types + API for priority, settings, pause/resume all"
```

---

## Task 7 : Frontend — composants UI (priorité, réglages, toolbar)

**Files:**
- Create: `src/components/PriorityPill.tsx`, `src/components/SettingsDialog.tsx`
- Modify: `src/components/Toolbar.tsx`, `src/components/DownloadRow.tsx`, `src/components/DownloadTable.tsx`, `src/App.tsx`

- [ ] **Step 1: `PriorityPill.tsx`**

`src/components/PriorityPill.tsx` :
```tsx
import type { Priority } from "../types";

const STYLE: Record<Priority, string> = {
  high: "bg-red-50 text-red-600",
  normal: "bg-indigo-50 text-indigo-600",
  low: "bg-gray-100 text-gray-500",
};
const LABEL: Record<Priority, string> = { high: "HAUTE", normal: "NORMALE", low: "BASSE" };

export function PriorityPill({
  value,
  onChange,
}: {
  value: Priority;
  onChange: (p: Priority) => void;
}) {
  return (
    <select
      className={`text-[10px] font-bold rounded-full px-2 py-0.5 border-0 cursor-pointer ${STYLE[value]}`}
      value={value}
      onChange={(e) => onChange(e.target.value as Priority)}
      title="Priorité"
    >
      <option value="high">{LABEL.high}</option>
      <option value="normal">{LABEL.normal}</option>
      <option value="low">{LABEL.low}</option>
    </select>
  );
}
```

- [ ] **Step 2: `SettingsDialog.tsx`**

`src/components/SettingsDialog.tsx` :
```tsx
import { useEffect, useState } from "react";
import * as api from "../api";

export function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [maxConcurrent, setMaxConcurrent] = useState(3);
  const [speedValue, setSpeedValue] = useState(0); // en Mo/s dans l'UI
  const [unlimited, setUnlimited] = useState(true);

  useEffect(() => {
    if (!open) return;
    api.getSettings().then((s) => {
      setMaxConcurrent(s.max_concurrent);
      if (s.max_speed_bps === 0) {
        setUnlimited(true);
        setSpeedValue(0);
      } else {
        setUnlimited(false);
        setSpeedValue(Math.round((s.max_speed_bps / (1024 * 1024)) * 10) / 10);
      }
    });
  }, [open]);

  if (!open) return null;

  async function save() {
    const bps = unlimited ? 0 : Math.round(speedValue * 1024 * 1024);
    await api.updateSettings(Math.max(1, Math.round(maxConcurrent)), bps);
    onClose();
  }

  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg w-[380px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold px-4 py-3 border-b border-gray-200">⚙ Réglages</h3>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="block text-sm text-gray-600 mb-1.5">Téléchargements simultanés max</label>
          <input
            type="number"
            min={1}
            className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24"
            value={maxConcurrent}
            onChange={(e) => setMaxConcurrent(Number(e.target.value))}
          />
        </div>
        <div className="px-4 py-3">
          <label className="block text-sm text-gray-600 mb-1.5">Vitesse de téléchargement max (Mo/s)</label>
          <div className="flex items-center gap-3">
            <input
              type="number"
              min={0}
              step={0.1}
              disabled={unlimited}
              className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24 disabled:opacity-50"
              value={speedValue}
              onChange={(e) => setSpeedValue(Number(e.target.value))}
            />
            <label className="text-sm flex items-center gap-1">
              <input type="checkbox" checked={unlimited} onChange={(e) => setUnlimited(e.target.checked)} />
              Illimitée
            </label>
          </div>
        </div>
        <div className="flex justify-end gap-2 px-4 py-3">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>Annuler</button>
          <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold" onClick={save}>
            Enregistrer
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: `Toolbar.tsx` (boutons Tout + Réglages)**

Remplacer `src/components/Toolbar.tsx` par :
```tsx
export function Toolbar({
  onAdd,
  onPauseAll,
  onResumeAll,
  onSettings,
}: {
  onAdd: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onSettings: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-white">
      <button
        className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold"
        onClick={onAdd}
      >
        + Ajouter
      </button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onPauseAll}>
        ⏸ Tout
      </button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onResumeAll}>
        ▶ Tout
      </button>
      <div className="flex-1" />
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onSettings} title="Réglages">
        ⚙
      </button>
    </div>
  );
}
```

- [ ] **Step 4: `DownloadRow.tsx` (colonne priorité + position file)**

Remplacer `src/components/DownloadRow.tsx` par :
```tsx
import type { Download, Priority } from "../types";
import { formatBytes, formatSpeed, percent } from "../format";
import { PriorityPill } from "./PriorityPill";

const STATUS_LABEL: Record<string, string> = {
  queued: "En file",
  downloading: "En cours",
  paused: "En pause",
  completed: "Terminé",
  error: "Erreur",
};

export function DownloadRow({
  d,
  speed,
  queuePosition,
  onPause,
  onResume,
  onCancel,
  onPriority,
}: {
  d: Download;
  speed: number;
  queuePosition: number | null;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onPriority: (p: Priority) => void;
}) {
  const pct = percent(d.downloaded_bytes, d.total_bytes);
  const stateLabel =
    d.status === "queued" && queuePosition !== null
      ? `En file (${queuePosition})`
      : `${STATUS_LABEL[d.status]} · ${pct}%`;
  return (
    <div className="flex items-center gap-3 px-3 py-2 border-b border-gray-100 text-sm">
      <div className="flex-[2] min-w-0">
        <div className="font-semibold truncate">{d.file_name}</div>
        <div className="h-1.5 mt-1 rounded bg-gray-200 overflow-hidden">
          <div
            className={`h-full rounded ${d.status === "completed" ? "bg-green-500" : "bg-indigo-600"}`}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>
      <div className="flex-1">{d.total_bytes ? formatBytes(d.total_bytes) : "—"}</div>
      <div className="flex-1"><PriorityPill value={d.priority} onChange={onPriority} /></div>
      <div className="flex-1">{stateLabel}</div>
      <div className="flex-1">{d.status === "downloading" ? formatSpeed(speed) : "—"}</div>
      <div className="flex gap-1">
        {d.status === "downloading" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onPause}>⏸</button>
        ) : d.status !== "completed" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onResume}>▶</button>
        ) : null}
        <button className="px-2 py-1 rounded bg-gray-100" onClick={onCancel}>✕</button>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: `DownloadTable.tsx` (en-tête priorité + calcul des positions de file)**

Remplacer `src/components/DownloadTable.tsx` par :
```tsx
import type { Download, Priority } from "../types";
import { DownloadRow } from "./DownloadRow";

const RANK: Record<Priority, number> = { high: 0, normal: 1, low: 2 };

export function DownloadTable({
  downloads,
  speeds,
  onPause,
  onResume,
  onCancel,
  onPriority,
}: {
  downloads: Download[];
  speeds: Record<string, number>;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onPriority: (id: string, p: Priority) => void;
}) {
  // Position dans la file = rang parmi les "queued", trié par priorité puis ordre d'origine.
  const queued = downloads
    .map((d, i) => ({ d, i }))
    .filter((x) => x.d.status === "queued")
    .sort((a, b) => RANK[a.d.priority] - RANK[b.d.priority] || a.i - b.i);
  const position: Record<string, number> = {};
  queued.forEach((x, idx) => { position[x.d.id] = idx + 1; });

  return (
    <div>
      <div className="flex gap-3 px-3 py-2 text-[10px] uppercase tracking-wide text-gray-400 border-b border-gray-200 bg-gray-50">
        <div className="flex-[2]">Nom</div>
        <div className="flex-1">Taille</div>
        <div className="flex-1">Priorité</div>
        <div className="flex-1">État</div>
        <div className="flex-1">Vitesse</div>
        <div className="w-16" />
      </div>
      {downloads.length === 0 ? (
        <div className="p-8 text-center text-gray-400 text-sm">Aucun téléchargement.</div>
      ) : (
        downloads.map((d) => (
          <DownloadRow
            key={d.id}
            d={d}
            speed={speeds[d.id] ?? 0}
            queuePosition={position[d.id] ?? null}
            onPause={() => onPause(d.id)}
            onResume={() => onResume(d.id)}
            onCancel={() => onCancel(d.id)}
            onPriority={(p) => onPriority(d.id, p)}
          />
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 6: `App.tsx` (plus d'auto-start après add, settings, priorité)**

Remplacer `src/App.tsx` par :
```tsx
import { useEffect, useState } from "react";
import "./index.css";
import type { Download, Priority } from "./types";
import * as api from "./api";
import { Toolbar } from "./components/Toolbar";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { DownloadTable } from "./components/DownloadTable";
import { SettingsDialog } from "./components/SettingsDialog";

const DEST_DIR = ""; // vide => le backend défaut sur le dossier Téléchargements

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setDownloads(await api.listDownloads());
  }

  useEffect(() => {
    refresh();
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      setDownloads((list) =>
        list.map((d) =>
          d.id === p.id
            ? { ...d, downloaded_bytes: p.downloaded_bytes, status: p.status, total_bytes: p.total_bytes }
            : d,
        ),
      );
      // Un changement d'état terminal peut avoir démarré un autre téléchargement : resync.
      if (p.status === "completed" || p.status === "paused" || p.status === "error") {
        refresh();
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  async function handleAdd(url: string) {
    setDialogOpen(false);
    setError(null);
    try {
      await api.addDownload(url, DEST_DIR);
      await refresh(); // le backend démarre automatiquement si un slot est libre
    } catch (e) {
      setError(`Impossible d'ajouter ce lien : ${String(e)}`);
    }
  }

  async function withRefresh(fn: () => Promise<void>) {
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
    await refresh();
  }

  return (
    <div className="h-screen flex flex-col">
      <Toolbar
        onAdd={() => setDialogOpen(true)}
        onPauseAll={() => withRefresh(api.pauseAll)}
        onResumeAll={() => withRefresh(api.resumeAll)}
        onSettings={() => setSettingsOpen(true)}
      />
      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-red-50 border-b border-red-200 text-sm text-red-700">
          <span className="flex-1 break-words">{error}</span>
          <button className="px-2 py-0.5 rounded bg-red-100 text-red-700 shrink-0" onClick={() => setError(null)}>✕</button>
        </div>
      )}
      <div className="flex-1 overflow-auto">
        <DownloadTable
          downloads={downloads}
          speeds={speeds}
          onPause={(id) => withRefresh(() => api.pauseDownload(id))}
          onResume={(id) => withRefresh(() => api.startDownload(id))}
          onCancel={(id) => {
            api.cancelDownload(id);
            setDownloads((l) => l.filter((d) => d.id !== id));
          }}
          onPriority={(id, p: Priority) => withRefresh(() => api.setPriority(id, p))}
        />
      </div>
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
```

- [ ] **Step 7: Build front**

Run:
```bash
npm run build && npm test
```
Expected: build OK, tests de formatage toujours verts.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(ui): priority column, settings dialog, pause/resume all, queue positions"
```

---

## Task 8 : Vérification (build complet + suites + smoke-test manuel)

**Files:** aucun nouveau.

- [ ] **Step 1: Suites automatisées**

Run:
```bash
cd src-tauri && cargo test -p download-core
cd .. && npm test
cd src-tauri && cargo build
cd .. && npm run build
```
Expected: tout PASS, builds OK.

- [ ] **Step 2: Smoke-test manuel (humain)**

Lancer `npm run tauri dev`, puis :
1. Régler « simultanés max » à 1 dans ⚙ Réglages.
2. Ajouter 3 URLs valides : la 1ʳᵉ démarre, les 2 autres affichent « En file (1) / (2) ».
3. Quand la 1ʳᵉ finit, la suivante démarre automatiquement.
4. Passer une file en priorité HAUTE : elle doit passer devant les autres « En file ».
5. Régler la vitesse max à 1 Mo/s : le débit affiché doit se plafonner.
6. ⏸ Tout / ▶ Tout : met tout en pause / relance la file.
7. Remonter « simultanés max » à 3 : plusieurs téléchargements démarrent.

Expected: comportements conformes. Noter tout écart (corriger en TDD côté cœur si logique, ou côté UI).

- [ ] **Step 3: Commit final**

```bash
git add -A
git commit -m "test: verify phase 2a queue, priorities and throttling"
```

---

## Auto-revue (effectuée)

- **Couverture spec :** limite de simultanés + démarrage auto/En file (scheduler Task 3 + reevaluate Task 5), priorités Haute/Normale/Basse (model Task 2, scheduler Task 3, UI Task 7), limiteur global + à chaud (rate_limiter Task 1, engine Task 4, update_settings Task 5), réglages persistants (storage Task 2, settings dialog Task 7), pause tout/reprendre tout (Task 5/7), distinction Paused vs Queued (reevaluate ne touche que les Queued — Task 3/5), migration base (Task 2), client mutualisé + get_by_id (Task 2/5). ✔
- **Hors périmètre (2b) :** catégories, recherche/filtres, historique, limite par téléchargement. ✔
- **Cohérence des types :** `Priority`/`Settings` identiques Rust (`model.rs`) ↔ TS (`types.ts`) ; commandes `set_priority{id,priority}`, `update_settings{maxConcurrent,maxSpeedBps}` ↔ `api.ts` ; `pick_to_start` consommé par `reevaluate` ; signature `download(... , limiter, progress)` mise à jour partout (engine tests + real_http + commands). ✔
- **Pas de placeholder :** chaque step de code est complet. ✔

## Notes d'implémentation / risques

- **Ordre des verrous** : toujours `handles` puis `storage` (jamais l'inverse) — respecté dans `reevaluate`/`cancel`/`launch` ⇒ pas d'interblocage.
- **Atomicité de la ré-évaluation** : le verrou `handles` est tenu pendant pick + launch, et le statut passe `Downloading` immédiatement ⇒ pas de double-démarrage même si plusieurs événements ré-évaluent en rafale.
- **`acquire` ne tient aucun verrou pendant le sommeil** (`take()` relâche le `Mutex` avant l'`await`) ⇒ pas de `MutexGuard` traversant un `.await`.
