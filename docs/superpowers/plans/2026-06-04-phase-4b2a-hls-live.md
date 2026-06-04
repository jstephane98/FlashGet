# Phase 4b-2a — HLS Live Recording Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enregistrer un flux HLS live (media playlist sans `#EXT-X-ENDLIST`) vers un `.mp4`, avec arrêt manuel ou sur fin de flux.

**Architecture:** Nouveau moteur `hls/live.rs::download_hls_live` (boucle de rafraîchissement, curseur de séquence absolue, finalisation par concat+remux) réutilisant `media_io`/`crypto`/`Remuxer`. `download_hls` délègue au live au lieu de le rejeter. Côté Tauri : `DownloadStatus::Recording`, 3ᵉ drapeau `stop` sur `DownloadHandle`, commande `stop_recording`, finalisation de secours au démarrage. Côté front : ligne `DownloadRow` en mode enregistrement.

**Tech Stack:** Rust (tokio, async-trait), crate `download-core` (cœur testable sans Tauri) + crate Tauri ; React/TS + Vitest ; ffmpeg sidecar.

**Spec :** `docs/superpowers/specs/2026-06-04-phase-4b2a-hls-live-design.md`.

**Convention de test (projet) :**
- Tests Rust cœur : `cargo test -p download-core --manifest-path src-tauri/Cargo.toml`
- Test ciblé : `... -p download-core --lib <module>::tests::<nom>`
- Smoke offline (vrai ffmpeg, `#[ignore]`) : `... --test <fichier> -- --ignored --nocapture`
- Front : `npm test` (Vitest)
- Barre projet : **0 warning rustc**. Commits fréquents, messages `type(scope): …` + trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

## File Structure

- **Create** `src-tauri/crates/download-core/src/hls/live.rs` — moteur live (`download_hls_live`, `RefreshInterval`, helpers).
- **Modify** `src-tauri/crates/download-core/src/hls/mod.rs` — déclarer `pub mod live;`.
- **Modify** `src-tauri/crates/download-core/src/engine.rs` — `DownloadHandle` : drapeau `stop`.
- **Modify** `src-tauri/crates/download-core/src/model.rs` — `DownloadStatus::Recording`.
- **Modify** `src-tauri/crates/download-core/src/storage.rs` — round-trip statut `recording`.
- **Modify** `src-tauri/crates/download-core/src/scheduler.rs` — compter `Recording` comme actif.
- **Modify** `src-tauri/crates/download-core/src/media_io.rs` — `concat_track` tri **numérique**.
- **Modify** `src-tauri/crates/download-core/src/hls/engine.rs` — `download_hls` délègue au live.
- **Create** `src-tauri/crates/download-core/tests/hls_live_smoke.rs` — smoke offline.
- **Modify** `src-tauri/src/commands.rs` — `stop_recording`, persistance `Recording` (relais), recovery.
- **Modify** `src-tauri/src/lib.rs` — enregistrer `stop_recording`, appeler la passe de recovery.
- **Modify** `src-tauri/src/state.rs` (si besoin d'un helper) — n/a a priori.
- **Modify** front : `src/api.ts` (ou équivalent), `src/components/DownloadRow.tsx`, type `DownloadStatus`, `src/App.tsx`/`DownloadTable.tsx`, + test Vitest.

---

## Task 1: `DownloadHandle` — drapeau `stop`

**Files:**
- Modify: `src-tauri/crates/download-core/src/engine.rs:16-45`
- Test: même fichier, module `tests`.

- [ ] **Step 1: Écrire le test d'échec**

Ajouter dans le module `#[cfg(test)] mod tests` de `engine.rs` :

```rust
#[test]
fn stop_flag_independent_from_cancel_and_pause() {
    let h = DownloadHandle::new();
    assert!(!h.is_stopped());
    h.stop();
    assert!(h.is_stopped());
    // stop n'affecte ni cancel ni pause
    assert!(!h.is_cancelled());
    assert!(!h.is_paused());
    // un clone partage le drapeau
    let h2 = h.clone();
    assert!(h2.is_stopped());
}
```

- [ ] **Step 2: Lancer le test, vérifier l'échec**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib engine::tests::stop_flag_independent_from_cancel_and_pause`
Expected: FAIL — `no method named 'stop'` / `is_stopped`.

- [ ] **Step 3: Implémenter le drapeau**

Dans `struct DownloadHandle` (après `cancel: Arc<AtomicBool>,`) ajouter :
```rust
    stop: Arc<AtomicBool>,
```
Dans `impl DownloadHandle`, ajouter (après `cancel`) :
```rust
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
```
`#[derive(Clone, Default)]` est conservé : `Arc<AtomicBool>::default()` = `false`.

- [ ] **Step 4: Lancer le test, vérifier le succès**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib engine::tests::stop_flag_independent_from_cancel_and_pause`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/engine.rs
git commit -m "$(printf 'feat(core): stop flag on DownloadHandle (finalize vs cancel)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 2: `DownloadStatus::Recording`

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs:3-11`
- Modify: `src-tauri/crates/download-core/src/storage.rs:314-332`
- Modify: `src-tauri/crates/download-core/src/scheduler.rs:7-11`
- Test: `storage.rs` et `scheduler.rs` modules `tests`.

- [ ] **Step 1: Écrire les tests d'échec**

Dans `storage.rs` module `tests`, ajouter :
```rust
#[test]
fn status_recording_round_trips() {
    assert_eq!(super::status_from_str(super::status_to_str(DownloadStatus::Recording)), DownloadStatus::Recording);
    assert_eq!(super::status_to_str(DownloadStatus::Recording), "recording");
}
```
Dans `scheduler.rs` module `tests`, ajouter :
```rust
#[test]
fn recording_counts_as_active_slot() {
    // 1 enregistrement actif + 1 en file, max=1 => rien à démarrer.
    let downloads = vec![
        dl("rec", DownloadStatus::Recording, Priority::Normal),
        dl("q", DownloadStatus::Queued, Priority::Normal),
    ];
    assert!(pick_to_start(&downloads, 1).is_empty());
}
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib status_recording_round_trips recording_counts_as_active_slot`
Expected: FAIL — variant `Recording` inexistant.

- [ ] **Step 3: Implémenter**

`model.rs`, enum `DownloadStatus` — ajouter `Recording` après `Downloading` :
```rust
pub enum DownloadStatus {
    Queued,
    Downloading,
    Recording,
    Paused,
    Completed,
    Error,
}
```
`storage.rs::status_to_str` — ajouter le bras (avant `Completed`) :
```rust
        DownloadStatus::Recording => "recording",
```
`storage.rs::status_from_str` — ajouter (avant `_ =>`) :
```rust
        "recording" => DownloadStatus::Recording,
```
`scheduler.rs::pick_to_start`, filtre `active` (lignes 8-11) — compter aussi `Recording` :
```rust
    let active = downloads
        .iter()
        .filter(|d| matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Recording))
        .count() as u32;
```

- [ ] **Step 4: Lancer, vérifier le succès + compilation globale**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib status_recording_round_trips recording_counts_as_active_slot`
Expected: PASS.
Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK` (aucun `match` non exhaustif ailleurs ; le crate Tauri compile encore — voir Task 7 pour l'usage UI).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/model.rs src-tauri/crates/download-core/src/storage.rs src-tauri/crates/download-core/src/scheduler.rs
git commit -m "$(printf 'feat(core): DownloadStatus::Recording (live capture state)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 3: `concat_track` — tri numérique des segments

Permet des numéros de séquence live > 99999 (le tri lexical casse au passage 99999→100000).

**Files:**
- Modify: `src-tauri/crates/download-core/src/media_io.rs:56-83`
- Test: même fichier, nouveau module `tests`.

- [ ] **Step 1: Écrire le test d'échec**

Ajouter en bas de `media_io.rs` :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn concat_orders_segments_numerically_not_lexically() {
        let dir = tempdir().unwrap();
        let w = dir.path();
        // 99999 puis 100000 : ordre numérique correct = 99999 avant 100000,
        // alors que le tri lexical placerait "v100000" avant "v99999".
        std::fs::write(w.join("v99999.seg"), b"A").unwrap();
        std::fs::write(w.join("v100000.seg"), b"B").unwrap();
        let out = concat_track(w, "v", false).unwrap();
        assert_eq!(std::fs::read(out).unwrap(), b"AB");
    }
}
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib media_io::tests::concat_orders_segments_numerically_not_lexically`
Expected: FAIL — résultat `BA` (tri lexical).

- [ ] **Step 3: Implémenter le tri numérique**

Dans `concat_track`, remplacer `segs.sort();` par un tri sur le suffixe entier. Remplacer la collecte + tri (lignes ~68-77) par :
```rust
    let mut segs: Vec<(u64, PathBuf)> = std::fs::read_dir(work)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            if !(name.starts_with(prefix) && name.ends_with(".seg") && !name.contains("_init")) {
                return None;
            }
            // <prefix><digits>.seg → digits
            let digits = &name[prefix.len()..name.len() - 4];
            let seq: u64 = digits.parse().ok()?;
            Some((seq, p))
        })
        .collect();
    segs.sort_by_key(|(seq, _)| *seq);
    for (_, s) in segs {
        out.write_all(&std::fs::read(s)?)?;
    }
```
Mettre à jour le commentaire au-dessus :
```rust
    // Tri NUMÉRIQUE par numéro de séquence (suffixe entier) : robuste quelle que
    // soit la largeur (VOD padde sur 5 ; live peut dépasser 99999 segments).
```

- [ ] **Step 4: Lancer le test + non-régression VOD**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib media_io`
Expected: PASS.
Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::engine::tests dash::engine::tests`
Expected: PASS (VOD utilise toujours le pad 5, l'ordre est identique).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/media_io.rs
git commit -m "$(printf 'fix(core): concat_track orders segments numerically (live seq > 99999)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 4: `hls/live.rs` — `RefreshInterval` + `responsive_sleep`

Briques utilitaires testables avant le moteur.

**Files:**
- Create: `src-tauri/crates/download-core/src/hls/live.rs`
- Modify: `src-tauri/crates/download-core/src/hls/mod.rs`

- [ ] **Step 1: Déclarer le module**

Dans `hls/mod.rs`, ajouter (avec les autres `pub mod`) :
```rust
pub mod live;
```

- [ ] **Step 2: Écrire le fichier avec les helpers + tests**

Créer `hls/live.rs` avec, pour l'instant, les helpers et leurs tests :
```rust
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
```

- [ ] **Step 3: Lancer les tests**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::live::tests`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/download-core/src/hls/live.rs src-tauri/crates/download-core/src/hls/mod.rs
git commit -m "$(printf 'feat(core): hls live refresh-interval + interruptible sleep helpers\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 5: `hls/live.rs` — `download_hls_live` (cœur)

**Files:**
- Modify: `src-tauri/crates/download-core/src/hls/live.rs`

- [ ] **Step 1: Écrire le test d'échec — capture + dédup + fin de flux**

Ajouter au module `tests` de `live.rs` (et les imports en tête de module : voir Step 3) :
```rust
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::http::ByteFetcher;
    use crate::model::{ProbeResult, Progress, RequestHeaders};
    use crate::rate_limiter::RateLimiter;
    use crate::engine::Outcome;
    use crate::CoreError;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use futures_util::StreamExt;
    use std::collections::HashMap as Map;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    /// Fetcher où l'URL de playlist renvoie des corps SUCCESSIFS (fenêtre qui
    /// glisse), et les segments/clés viennent d'une map fixe. Peut poser
    /// stop/cancel après un nombre donné de fetchs de segment.
    struct EvolvingFetcher {
        playlist_url: String,
        playlists: Vec<String>,
        calls: Mutex<usize>,
        segs: Map<String, Vec<u8>>,
        handle: Option<DownloadHandle>,
        stop_after_segs: usize, // 0 = jamais
        cancel_after_segs: usize,
        seg_fetches: Arc<Mutex<usize>>,
    }
    #[async_trait]
    impl ByteFetcher for EvolvingFetcher {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
        }
        async fn fetch_range(&self, url: &str, _r: Option<(u64, u64)>, _h: &RequestHeaders)
            -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = if url == self.playlist_url {
                let mut n = self.calls.lock().unwrap();
                let body = self.playlists[(*n).min(self.playlists.len() - 1)].clone();
                *n += 1;
                body.into_bytes()
            } else {
                let d = self.segs.get(url).cloned().unwrap_or_default();
                let mut c = self.seg_fetches.lock().unwrap();
                *c += 1;
                if let Some(h) = &self.handle {
                    if self.stop_after_segs != 0 && *c == self.stop_after_segs { h.stop(); }
                    if self.cancel_after_segs != 0 && *c == self.cancel_after_segs { h.cancel(); }
                }
                d
            };
            Ok(stream::iter(vec![Ok(Bytes::from(data))]).boxed())
        }
    }

    fn pl(seq: u64, segs: &[&str], endlist: bool) -> String {
        let mut s = format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{seq}\n");
        for u in segs { s.push_str(&format!("#EXTINF:1.0,\n{u}\n")); }
        if endlist { s.push_str("#EXT-X-ENDLIST\n"); }
        s
    }
    fn unlimited() -> Arc<RateLimiter> { Arc::new(RateLimiter::unlimited()) }

    #[tokio::test]
    async fn captures_window_dedups_and_finalizes_on_endlist() {
        let base = "https://h/p/";
        let playlists = vec![
            pl(0, &["s0.ts", "s1.ts"], false),
            pl(1, &["s1.ts", "s2.ts"], false),       // fenêtre glisse : s1 déjà capturé
            pl(2, &["s2.ts", "s3.ts"], true),         // ENDLIST
        ];
        let mut segs = Map::new();
        for (n, b) in [("s0.ts","AA"),("s1.ts","BB"),("s2.ts","CC"),("s3.ts","DD")] {
            segs.insert(format!("{base}{n}"), b.as_bytes().to_vec());
        }
        let seg_fetches = Arc::new(Mutex::new(0));
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists, calls: Mutex::new(0),
            segs, handle: None, stop_after_segs: 0, cancel_after_segs: 0, seg_fetches: seg_fetches.clone(),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();

        let outcome = download_hls_live(
            &fetcher, &remux, "L1", &format!("{base}live.m3u8"), None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();

        assert_eq!(outcome, Outcome::Completed);
        // s0..s3 capturés une fois chacun, dans l'ordre.
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABBCCDD");
        assert_eq!(*seg_fetches.lock().unwrap(), 4, "aucun segment re-téléchargé");
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists(), "work dir nettoyé");
    }
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::live::tests::captures_window_dedups_and_finalizes_on_endlist`
Expected: FAIL — `download_hls_live` n'existe pas.

- [ ] **Step 3: Implémenter `download_hls_live` + structures internes**

En tête de `live.rs`, compléter les `use` :
```rust
use crate::hls::crypto::{decrypt_aes128_cbc, iv_for_segment};
use crate::hls::playlist::{parse_media, KeyMethod, MediaPlaylist};
use crate::hls::remux::{RemuxInputs, Remuxer};
use crate::http::ByteFetcher;
use crate::media_io::{concat_track, fetch_to_vec, finish_cancelled, work_dir, write_segment};
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::engine::Outcome;
use crate::CoreError;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
```

Ajouter (après les helpers, avant `#[cfg(test)]`) :
```rust
/// Issue d'une passe de capture d'une piste sur un rafraîchissement.
enum Step { Continue, Stopped, Cancelled }

/// État persistant d'une piste (vidéo ou audio) entre rafraîchissements.
struct LiveTrack {
    prefix: &'static str,
    next_seq: u64,           // prochaine séquence absolue à capturer
    init_handled: bool,      // EXT-X-MAP déjà traité
    has_init: bool,          // un init existe (pour la concat)
    has_any: bool,           // au moins un segment capturé
    key_cache: HashMap<String, [u8; 16]>,
}

impl LiveTrack {
    fn new(prefix: &'static str) -> Self {
        Self { prefix, next_seq: 0, init_handled: false, has_init: false, has_any: false, key_cache: HashMap::new() }
    }

    /// Capture les segments de `pl` dont la séquence absolue ≥ `next_seq`.
    #[allow(clippy::too_many_arguments)]
    async fn capture<F: ByteFetcher>(
        &mut self, pl: &MediaPlaylist, fetcher: &F, work: &Path, headers: &RequestHeaders,
        limiter: &Arc<RateLimiter>, handle: &DownloadHandle,
        progress: &mpsc::UnboundedSender<Progress>, id: &str, total_done: &mut u32,
    ) -> Result<Step, CoreError> {
        if !self.init_handled {
            if let Some(map_uri) = &pl.map {
                let init = match fetch_to_vec(fetcher, map_uri, None, headers, limiter, handle).await? {
                    Some(b) => b, None => return Ok(Step::Cancelled),
                };
                std::fs::write(work.join(format!("{}_init.seg", self.prefix)), init)?;
                self.has_init = true;
            }
            self.init_handled = true;
        }
        for (idx, seg) in pl.segments.iter().enumerate() {
            let abs = pl.media_sequence + idx as u64;
            if abs < self.next_seq { continue; }
            if handle.is_cancelled() { return Ok(Step::Cancelled); }
            if handle.is_stopped() { return Ok(Step::Stopped); }
            let raw = match fetch_to_vec(fetcher, &seg.uri, seg.byte_range, headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Step::Cancelled),
            };
            let bytes = if let Some(k) = &seg.key {
                let uri = k.uri.as_ref().ok_or_else(|| CoreError::Media("clé AES-128 sans URI".into()))?;
                let key = match self.key_cache.get(uri) {
                    Some(k) => *k,
                    None => {
                        let kb = match fetch_to_vec(fetcher, uri, None, headers, limiter, handle).await? {
                            Some(b) => b, None => return Ok(Step::Cancelled),
                        };
                        if kb.len() != 16 { return Err(CoreError::Media("clé AES-128 de taille invalide".into())); }
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(&kb);
                        self.key_cache.insert(uri.clone(), arr);
                        arr
                    }
                };
                let iv = iv_for_segment(k.iv, abs);
                decrypt_aes128_cbc(&raw, &key, &iv)?
            } else { raw };
            write_segment(work, self.prefix, abs as usize, &bytes)?;
            self.next_seq = abs + 1;
            self.has_any = true;
            *total_done += 1;
            let _ = progress.send(Progress {
                id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
                speed_bps: 0, status: DownloadStatus::Recording,
                segments_done: Some(*total_done), segments_total: None,
            });
        }
        Ok(Step::Continue)
    }
}

/// Refuse tout chiffrement non AES-128 (garde héritée du VOD HLS).
fn reject_unsupported_crypto(pl: &MediaPlaylist) -> Result<(), CoreError> {
    for seg in &pl.segments {
        if let Some(k) = &seg.key {
            if k.method != KeyMethod::Aes128 {
                return Err(CoreError::Media("chiffrement non supporté (DRM/SAMPLE-AES)".into()));
            }
        }
    }
    Ok(())
}

/// Enregistre un flux HLS live vers `dest` (.mp4). Capture la fenêtre disponible
/// puis les nouveaux segments à chaque rafraîchissement, jusqu'à `stop()` ou la
/// fin de flux (`ENDLIST`). `cancel()` jette la capture.
#[allow(clippy::too_many_arguments)]
pub async fn download_hls_live<F: ByteFetcher, R: Remuxer>(
    fetcher: &F, remux: &R, id: &str,
    media_url: &str, audio_url: Option<&str>, dest: &Path,
    handle: DownloadHandle, limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>, headers: &RequestHeaders,
    initial: Option<MediaPlaylist>, refresh: RefreshInterval,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    let mut v = LiveTrack::new("v");
    let mut a = LiveTrack::new("a");
    let mut total_done: u32 = 0;
    let mut pending_video = initial;

    loop {
        // --- piste vidéo ---
        let vpl = match pending_video.take() {
            Some(pl) => pl,
            None => match fetch_to_vec(fetcher, media_url, None, headers, &limiter, &handle).await? {
                Some(b) => parse_media(&String::from_utf8_lossy(&b), media_url),
                None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            },
        };
        reject_unsupported_crypto(&vpl)?;
        match v.capture(&vpl, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
            Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            Step::Stopped => break,
            Step::Continue => {}
        }
        let video_ended = !vpl.is_live;

        // --- piste audio (optionnelle) ---
        let mut audio_ended = true;
        if let Some(au) = audio_url {
            let apl = match fetch_to_vec(fetcher, au, None, headers, &limiter, &handle).await? {
                Some(b) => parse_media(&String::from_utf8_lossy(&b), au),
                None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            };
            reject_unsupported_crypto(&apl)?;
            match a.capture(&apl, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
                Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
                Step::Stopped => break,
                Step::Continue => {}
            }
            audio_ended = !apl.is_live;
        }

        if handle.is_stopped() { break; }
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        if video_ended && audio_ended { break; } // fin de flux sur toutes les pistes

        let interval = refresh.compute(vpl.target_duration);
        responsive_sleep(interval, &handle).await;
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        // stop : la boucle reprend et capturera une dernière fois si la playlist a
        // bougé, puis cassera au `is_stopped()` ci-dessus — acceptable.
    }

    // --- finalisation : concat + remux ---
    let work_v = work.clone();
    let v_has_init = v.has_init;
    let video_cat = tokio::task::spawn_blocking(move || concat_track(&work_v, "v", v_has_init))
        .await
        .map_err(|e| CoreError::Media(format!("tâche de concaténation interrompue: {e}")))??;
    let audio_cat = if a.has_any {
        let work_a = work.clone();
        let a_has_init = a.has_init;
        Some(tokio::task::spawn_blocking(move || concat_track(&work_a, "a", a_has_init))
            .await
            .map_err(|e| CoreError::Media(format!("tâche de concaténation interrompue: {e}")))??)
    } else { None };
    remux.remux(RemuxInputs { video: video_cat, audio: audio_cat, decryption_key: None }, dest).await?;

    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
        speed_bps: 0, status: DownloadStatus::Completed,
        segments_done: Some(total_done), segments_total: None,
    });
    Ok(Outcome::Completed)
}
```

- [ ] **Step 4: Lancer le test de capture**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::live::tests::captures_window_dedups_and_finalizes_on_endlist`
Expected: PASS.

- [ ] **Step 5: Ajouter les tests stop / cancel**

Ajouter au module `tests` :
```rust
    #[tokio::test]
    async fn manual_stop_finalizes_partial() {
        let base = "https://h/p/";
        // Playlist live qui ne finit jamais ; stop posé après 2 segments.
        let playlists = vec![pl(0, &["s0.ts", "s1.ts"], false)];
        let mut segs = Map::new();
        for (n, b) in [("s0.ts","AA"),("s1.ts","BB")] { segs.insert(format!("{base}{n}"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists, calls: Mutex::new(0),
            segs, handle: Some(h.clone()), stop_after_segs: 2, cancel_after_segs: 0,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L2", &format!("{base}live.m3u8"), None, &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABB");
    }

    #[tokio::test]
    async fn cancel_discards_work_dir() {
        let base = "https://h/p/";
        let playlists = vec![pl(0, &["s0.ts", "s1.ts"], false)];
        let mut segs = Map::new();
        for (n, b) in [("s0.ts","AA"),("s1.ts","BB")] { segs.insert(format!("{base}{n}"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists, calls: Mutex::new(0),
            segs, handle: Some(h.clone()), stop_after_segs: 0, cancel_after_segs: 1,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L3", &format!("{base}live.m3u8"), None, &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(!dest.exists());
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists());
    }
```

- [ ] **Step 6: Lancer stop/cancel**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::live::tests`
Expected: PASS (capture + stop + cancel + helpers).

- [ ] **Step 7: Ajouter le test AES-128 + rotation de clé**

Ajouter au module `tests` (chiffre avec la même primitive que le moteur pour rester cohérent) :
```rust
    #[tokio::test]
    async fn decrypts_aes128_live_with_key_rotation() {
        use crate::hls::crypto::iv_for_segment;
        use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        type Enc = cbc::Encryptor<aes::Aes128>;
        let base = "https://h/p/";
        let key0 = [1u8; 16];
        let key1 = [2u8; 16];
        // playlist : s0 sous key0 puis s1 sous key1 (rotation), ENDLIST.
        let body = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"{base}k0.bin\"\n#EXTINF:1.0,\ns0.ts\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"{base}k1.bin\"\n#EXTINF:1.0,\ns1.ts\n#EXT-X-ENDLIST\n");
        // Chiffre avec PKCS7 (decrypt_aes128_cbc retire le PKCS7), IV dérivé de
        // la séquence absolue comme le fait le moteur.
        let enc = |plain: &[u8], key: &[u8; 16], seq: u64| {
            let iv = iv_for_segment(None, seq);
            Enc::new(key.into(), (&iv).into()).encrypt_padded_vec_mut::<Pkcs7>(plain)
        };
        let mut segs = Map::new();
        segs.insert(format!("{base}live.m3u8"), body.into_bytes());
        segs.insert(format!("{base}k0.bin"), key0.to_vec());
        segs.insert(format!("{base}k1.bin"), key1.to_vec());
        segs.insert(format!("{base}s0.ts"), enc(b"HELLO-VIDEO-0", &key0, 0));
        segs.insert(format!("{base}s1.ts"), enc(b"HELLO-VIDEO-1", &key1, 1));
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"),
            playlists: vec![String::from_utf8(segs[&format!("{base}live.m3u8")].clone()).unwrap()],
            calls: Mutex::new(0), segs, handle: None, stop_after_segs: 0, cancel_after_segs: 0,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L4", &format!("{base}live.m3u8"), None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"HELLO-VIDEO-0HELLO-VIDEO-1");
    }
```
Note : `playlists` ne contient que ce body (la playlist contient déjà ENDLIST, donc un seul tour). On reconstruit la String depuis les octets pour éviter de dupliquer le corps.

- [ ] **Step 8: Lancer + vérifier 0 warning sur le crate**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::live`
Expected: PASS (toutes).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning`.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/crates/download-core/src/hls/live.rs
git commit -m "$(printf 'feat(core): download_hls_live engine (refresh loop, dedup, AES-128, finalize)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 6: `download_hls` délègue au live

**Files:**
- Modify: `src-tauri/crates/download-core/src/hls/engine.rs:42-45`

- [ ] **Step 1: Mettre à jour le test `rejects_live_stream` → délègue**

Dans `hls/engine.rs` tests, remplacer `rejects_live_stream` par un test qui prouve la délégation (live capté → Completed avec une playlist live + ENDLIST servie au 1ᵉʳ tour) :
```rust
    #[tokio::test]
    async fn live_playlist_is_recorded_not_rejected() {
        // Playlist live mais déjà terminée (ENDLIST) : un seul tour, finalise.
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXTINF:1.0,\nseg0.ts\n#EXTINF:1.0,\nseg1.ts\n"; // pas d'ENDLIST au 1er fetch
        let media_end = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXTINF:1.0,\nseg0.ts\n#EXTINF:1.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
        // MapFetcher renvoie toujours le même corps : on sert la version ENDLIST
        // pour borner la boucle en un tour.
        let mut routes = Map::new();
        routes.insert("https://h/p/index.m3u8".to_string(), media_end.as_bytes().to_vec());
        routes.insert("https://h/p/seg0.ts".to_string(), b"AAAA".to_vec());
        routes.insert("https://h/p/seg1.ts".to_string(), b"BBBB".to_vec());
        let _ = media; // version live non-ENDLIST documentée mais non servie ici
        let fetcher = MapFetcher { routes };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls(
            &fetcher, &remux, "idlive", "https://h/p/index.m3u8", None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AAAABBBB");
    }
```

- [ ] **Step 2: Lancer, vérifier l'échec**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::engine::tests::live_playlist_is_recorded_not_rejected`
Expected: FAIL — actuellement `download_hls` renvoie `Err("flux live…")`.

- [ ] **Step 3: Implémenter la délégation**

Dans `hls/engine.rs`, remplacer (lignes 43-45) :
```rust
    if video_pl.is_live {
        return Err(CoreError::Media("flux live non supporté (à venir)".into()));
    }
```
par :
```rust
    if video_pl.is_live {
        // Live : délègue au moteur d'enregistrement (passe la playlist déjà parsée).
        return crate::hls::live::download_hls_live(
            fetcher, remux, id, media_url, audio_url, dest,
            handle, limiter, progress, headers,
            Some(video_pl), crate::hls::live::RefreshInterval::FromTargetDuration,
        ).await;
    }
```
Note : `video_pl` est déplacé dans l'appel ; il n'est plus utilisé après dans le chemin VOD (le `return` court-circuite). Vérifier qu'aucune ligne VOD en aval ne référence `video_pl` avant ce point — elle est consommée juste après son parse, OK.

- [ ] **Step 4: Lancer le test + non-régression VOD**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::engine`
Expected: PASS (VOD `downloads_clear_vod_and_remuxes` + nouveau test live).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/hls/engine.rs
git commit -m "$(printf 'feat(core): download_hls delegates live streams to download_hls_live\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 7: Tauri — commande `stop_recording` + persistance `Recording`

**Files:**
- Modify: `src-tauri/src/commands.rs` (nouvelle commande + relais)
- Modify: `src-tauri/src/lib.rs:25-45` (enregistrer la commande)

- [ ] **Step 1: Ajouter la commande `stop_recording`**

Dans `commands.rs`, après `pause_download` (vers ligne 367), ajouter :
```rust
/// Arrête (finalise) un enregistrement live : pose le drapeau `stop` ; le moteur
/// remux le partiel et émet `Completed`. No-op si le handle n'est plus actif.
#[tauri::command]
pub fn stop_recording(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.stop();
    }
    Ok(())
}
```

- [ ] **Step 2: Persister `Recording` (sur changement) dans le relais de progression**

Dans `commands.rs::launch`, le relais (boucle `while let Some(mut p) = rx.recv().await`), remplacer le bloc de persistance terminale (ligne ~545) pour persister aussi l'entrée en `Recording`. Avant la boucle, ajouter un suivi du dernier statut persisté :
```rust
        let mut last_persisted: Option<DownloadStatus> = None;
```
Puis remplacer :
```rust
            if matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error) {
                let st = app_for_rx.state::<AppState>();
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
            }
```
par :
```rust
            let persist = matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error)
                || (p.status == DownloadStatus::Recording && last_persisted != Some(DownloadStatus::Recording));
            if persist {
                let st = app_for_rx.state::<AppState>();
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
                last_persisted = Some(p.status);
            }
```
(Le statut `Recording` n'est persisté qu'une fois à l'entrée, pas à chaque tick.)

- [ ] **Step 3: Enregistrer la commande**

Dans `lib.rs`, ajouter dans `generate_handler![ … ]` (après `commands::pause_download,`) :
```rust
            commands::stop_recording,
```

- [ ] **Step 4: Compiler le crate Tauri (vérification ; pas de test unitaire ici)**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(printf 'feat(tauri): stop_recording command + persist Recording status on entry\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 8: Tauri — finalisation de secours au redémarrage

Au démarrage, les lignes restées `Recording` (interrompues) sont finalisées (concat+remux du capturé) ou retirées si rien n'a été capturé.

**Files:**
- Modify: `src-tauri/src/commands.rs` (fonction `recover_interrupted_recordings`)
- Modify: `src-tauri/src/lib.rs:16-24` (appel dans `.setup`)

- [ ] **Step 1: Écrire la fonction de recovery**

Dans `commands.rs`, ajouter une fonction publique (appelée au setup) :
```rust
/// Au démarrage : finalise les enregistrements live interrompus (statut
/// `Recording` persisté mais aucune tâche moteur active). work dir non vide →
/// concat + remux → `Completed` ; vide → la ligne est retirée.
pub fn recover_interrupted_recordings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let downloads = match state.storage.list() {
        Ok(d) => d,
        Err(e) => { eprintln!("recovery: list failed: {e}"); return; }
    };
    for d in downloads.into_iter().filter(|d| d.status == DownloadStatus::Recording) {
        let dest = PathBuf::from(&d.dest_path);
        let work = PathBuf::from(format!("{}.fghls", d.dest_path));
        let has_segments = std::fs::read_dir(&work)
            .map(|rd| rd.filter_map(|e| e.ok()).any(|e| {
                e.file_name().to_str().map(|n| n.ends_with(".seg") && !n.contains("_init")).unwrap_or(false)
            }))
            .unwrap_or(false);
        if !has_segments {
            let _ = std::fs::remove_dir_all(&work);
            let _ = state.storage.delete(&d.id);
            continue;
        }
        // Finalise de façon synchrone (au démarrage, hors UI critique).
        let app2 = app.clone();
        let id = d.id.clone();
        tauri::async_runtime::spawn(async move {
            let st = app2.state::<AppState>();
            let remux = crate::remux::SidecarRemuxer::new(app2.clone());
            let v_has_init = work.join("v_init.seg").exists();
            let a_has_init = work.join("a_init.seg").exists();
            let has_audio = std::fs::read_dir(&work).map(|rd| rd.filter_map(|e| e.ok())
                .any(|e| e.file_name().to_str().map(|n| n.starts_with('a') && n.ends_with(".seg")).unwrap_or(false)))
                .unwrap_or(false);
            let work_v = work.clone();
            let video = match tokio::task::spawn_blocking(move || download_core::media_io::concat_track(&work_v, "v", v_has_init)).await {
                Ok(Ok(p)) => p, _ => { eprintln!("recovery: concat video failed for {id}"); return; }
            };
            let audio = if has_audio {
                let work_a = work.clone();
                match tokio::task::spawn_blocking(move || download_core::media_io::concat_track(&work_a, "a", a_has_init)).await {
                    Ok(Ok(p)) => Some(p), _ => None,
                }
            } else { None };
            let inputs = download_core::hls::remux::RemuxInputs { video, audio, decryption_key: None };
            use download_core::hls::remux::Remuxer;
            match remux.remux(inputs, &dest).await {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(&work);
                    let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                    let _ = st.storage.update_state(&id, bytes, DownloadStatus::Completed);
                    let _ = app2.emit("download://progress", &Progress {
                        id: id.clone(), downloaded_bytes: bytes, total_bytes: None, speed_bps: 0,
                        status: DownloadStatus::Completed, segments_done: None, segments_total: None,
                    });
                }
                Err(e) => {
                    eprintln!("recovery: remux failed for {id}: {e}");
                    let _ = st.storage.mark_error(&id, 0, "finalisation de l'enregistrement échouée");
                }
            }
        });
    }
}
```
Note : `media_io` doit être public sur le crate (`pub mod media_io;` — déjà le cas, cf. `lib.rs`). Vérifier que `concat_track` et `RemuxInputs`/`Remuxer` sont accessibles (publics).

- [ ] **Step 2: Appeler au setup**

Dans `lib.rs`, dans la closure `.setup`, après `bridge::start(app.handle().clone());` :
```rust
            commands::recover_interrupted_recordings(app.handle());
```

- [ ] **Step 3: Compiler**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK`. (Si un import manque — `Emitter` pour `emit`, déjà importé ; `PathBuf` déjà importé.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "$(printf 'feat(tauri): finalize interrupted live recordings on startup\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 9: `probe_hls` — URL media-playlist directe

Confirmer qu'une URL pointant directement une media playlist live (sans master) produit une variante unique exploitable.

**Files:**
- Modify (si besoin) : `src-tauri/src/commands.rs` (`probe_hls`)
- Inspection : `src-tauri/crates/download-core/src/hls/playlist.rs` (`parse_master`)

- [ ] **Step 1: Inspecter le comportement actuel**

Run: `grep -n "fn probe_hls" src-tauri/src/commands.rs` puis lire la fonction.
Run: `grep -n "fn parse_master" src-tauri/crates/download-core/src/hls/playlist.rs` puis lire.
Vérifier : si le corps n'a pas de `#EXT-X-STREAM-INF`, `parse_master` renvoie-t-il une liste vide (→ `probe_hls` doit alors traiter l'URL elle-même comme variante unique) ?

- [ ] **Step 2: Si une media-playlist directe donne 0 variante, ajouter le repli**

Dans `probe_hls`, si `master.variants.is_empty()` mais que le corps ressemble à une media playlist (contient `#EXTINF`), émettre une variante unique pointant l'URL fournie. Code de repli (à insérer après le parse master, adapter aux structures exactes lues au Step 1) :
```rust
    if variants.is_empty() && body.contains("#EXTINF") {
        // Media playlist directe (live ou VOD sans master) : une seule "variante".
        variants.push(MediaVariant {
            url: source_url.clone(),
            // champs hauteur/bandwidth inconnus : valeurs neutres
            height: None, bandwidth: 0, audio_url: None,
        });
    }
```
(Si `probe_hls` produit déjà une variante pour ce cas, ne rien changer et documenter via un commentaire.)

- [ ] **Step 3: Compiler + tester**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(printf 'fix(tauri): probe_hls yields a single variant for direct media playlists\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 10: Front — ligne d'enregistrement

**Files:**
- Modify: type `DownloadStatus` (chercher : `grep -rn "completed\|downloading" src/*.ts src/types* 2>/dev/null`)
- Modify: `src/components/DownloadRow.tsx`
- Modify: `src/api.ts` (ou fichier d'invoke) — `stopRecording`
- Modify: `src/App.tsx` / `src/components/DownloadTable.tsx` — câblage `onStop`
- Test: `src/components/DownloadRow.test.tsx` (créer) ou test existant

- [ ] **Step 1: Localiser les types et le câblage**

Run: `grep -rn "\"downloading\"\|'downloading'\|STATUS_LABEL\|isSegmented" src --include=*.ts --include=*.tsx`
Identifier : l'union de type `DownloadStatus`, `STATUS_LABEL`, le composant `DownloadRow`, et où `onPause`/`onCancel` sont fournis.

- [ ] **Step 2: Écrire le test Vitest d'échec**

Créer `src/components/DownloadRow.test.tsx` (adapter les props au composant réel lu au Step 1) :
```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { DownloadRow } from "./DownloadRow";

const base = {
  id: "1", url: "u", file_name: "live.m3u8", dest_path: "/x/live.mp4",
  total_bytes: null, downloaded_bytes: 50_000_000, status: "recording",
  supports_ranges: false, error: null, priority: "normal", category: "video",
  kind: "hls",
} as any;

describe("DownloadRow recording", () => {
  it("shows recording label, LIVE badge and a Stop button", () => {
    const onStop = vi.fn();
    render(
      <DownloadRow d={base} speed={1_200_000} seg={{ done: 134, total: 0 }}
        queuePosition={null} onPause={() => {}} onResume={() => {}}
        onCancel={() => {}} onStop={onStop} onOpenFile={() => {}}
        onOpenFolder={() => {}} onRemove={() => {}} />
    );
    expect(screen.getByText(/Enregistrement/)).toBeTruthy();
    expect(screen.getByText("LIVE")).toBeTruthy();
    const stop = screen.getByTitle(/Arrêter/);
    stop.click();
    expect(onStop).toHaveBeenCalledTimes(1);
  });
});
```
Note : si le projet n'a pas `@testing-library/react`, adapter au harnais Vitest existant (lire un autre `*.test.tsx`/`*.test.ts` pour le style). S'il n'y a pas de test de composant, créer un test pur sur un helper `recordingLabel(d, seg, elapsedSec)` extrait (préférable : voir Step 3).

- [ ] **Step 3: Implémenter le mode enregistrement**

Type `DownloadStatus` : ajouter `"recording"` à l'union. `STATUS_LABEL` : ajouter `recording: "Enregistrement"`.

Dans `DownloadRow.tsx` :
1. Ajouter la prop `onStop: () => void;`.
2. Minuteur local : au montage/quand `status==="recording"`, stocker `recordingStartRef` (première observation) et un `useState` tick par seconde :
```tsx
const startRef = useRef<number | null>(null);
const [, force] = useState(0);
useEffect(() => {
  if (d.status !== "recording") { startRef.current = null; return; }
  if (startRef.current === null) startRef.current = Date.now();
  const t = setInterval(() => force((n) => n + 1), 1000);
  return () => clearInterval(t);
}, [d.status]);
const elapsed = startRef.current ? Math.floor((Date.now() - startRef.current) / 1000) : 0;
const mmss = `${String(Math.floor(elapsed / 60)).padStart(2, "0")}:${String(elapsed % 60).padStart(2, "0")}`;
```
3. Libellé : quand `status==="recording"` →
```tsx
`Enregistrement · ${mmss} · ${seg?.done ?? 0} segments · ${formatBytes(d.downloaded_bytes)}`
```
(réutiliser le formateur d'octets existant ; sinon afficher `downloaded_bytes`).
4. Barre : si `recording`, rendre une barre rouge animée indéterminée + pastille `● REC` (au lieu de la barre `%`). Ex. : `<div className="h-full w-1/3 rounded bg-red-500 animate-pulse" />`.
5. Badge : ajouter `<span className="… bg-red-600 text-white">LIVE</span>` à côté du badge HLS quand `recording`.
6. Actions : quand `recording`, afficher un bouton `⏹` `title="Arrêter (finalise)"` `onClick={onStop}` à la place de `⏸`. Conserver `✕` (cancel).

- [ ] **Step 4: Câbler `onStop` + l'API**

`api.ts` : ajouter
```ts
export const stopRecording = (id: string) => invoke("stop_recording", { id });
```
`App.tsx`/`DownloadTable.tsx` : passer `onStop={() => stopRecording(d.id)}` à `DownloadRow`.

- [ ] **Step 5: Lancer les tests front**

Run: `npm test`
Expected: PASS (nouveau test recording + non-régression).

- [ ] **Step 6: Build front (type-check)**

Run: `npm run build 2>&1 | tail -5`
Expected: build OK (tsc + vite), aucune erreur de type.

- [ ] **Step 7: Commit**

```bash
git add src/
git commit -m "$(printf 'feat(frontend): recording-mode DownloadRow (LIVE badge, timer, Stop)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 11: Smoke-test offline (vrai ffmpeg)

**Files:**
- Create: `src-tauri/crates/download-core/tests/hls_live_smoke.rs`

- [ ] **Step 1: Écrire le smoke-test**

S'inspirer de `tests/dash_clearkey_smoke.rs` (CliRemuxer + tiny_http). Créer `hls_live_smoke.rs` :
```rust
//! Smoke-test HLS live OFFLINE (#[ignore], exige le vrai ffmpeg).
//! Un serveur loopback sert une media playlist live qui évolue sur quelques
//! rafraîchissements puis ajoute ENDLIST, plus des segments TS générés par
//! ffmpeg. download_hls_live via ReqwestFetcher + CliRemuxer → .mp4 lisible.
//!
//! Rejouer : cargo test -p download-core --test hls_live_smoke -- --ignored --nocapture

use download_core::hls::live::{download_hls_live, RefreshInterval};
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::http::ReqwestFetcher;
use download_core::model::{Progress, RequestHeaders};
use download_core::rate_limiter::RateLimiter;
use download_core::engine::{DownloadHandle, Outcome};
use download_core::CoreError;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use tiny_http::{Response, Server};

fn ffmpeg_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bin/ffmpeg-x86_64-pc-windows-msvc.exe")
}

struct CliRemuxer { ffmpeg: PathBuf }
#[async_trait::async_trait]
impl Remuxer for CliRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into(), "-i".into(), inputs.video.to_string_lossy().into()];
        args.extend(["-c".into(), "copy".into(), "-movflags".into(), "+faststart".into(), output.to_string_lossy().into()]);
        let out = Command::new(&self.ffmpeg).args(&args).output()
            .map_err(|e| CoreError::Media(format!("ffmpeg: {e}")))?;
        if !out.status.success() {
            return Err(CoreError::Media(format!("ffmpeg a échoué: {}", String::from_utf8_lossy(&out.stderr))));
        }
        Ok(())
    }
}

/// Génère N petits segments TS (mpegts) via ffmpeg.
fn make_ts_segments(ffmpeg: &Path, dir: &Path, n: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for i in 0..n {
        let p = dir.join(format!("seg{i}.ts"));
        let status = Command::new(ffmpeg).args([
            "-hide_banner","-loglevel","error","-y",
            "-f","lavfi","-i",&format!("testsrc=duration=1:size=160x120:rate=10"),
            "-c:v","libx264","-pix_fmt","yuv420p","-f","mpegts",
        ]).arg(&p).status().expect("ffmpeg seg");
        assert!(status.success());
        out.push(std::fs::read(&p).unwrap());
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "exige le vrai ffmpeg.exe ; lancer avec --ignored"]
async fn hls_live_records_until_endlist() {
    let ffmpeg = ffmpeg_path();
    assert!(ffmpeg.exists(), "ffmpeg introuvable en {}", ffmpeg.display());
    let dir = tempdir().unwrap();
    let segs = make_ts_segments(&ffmpeg, dir.path(), 4);

    // Playlists successives : fenêtre de 2 segments qui glisse, puis ENDLIST.
    let mk = |seq: usize, idxs: &[usize], end: bool| {
        let mut s = format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{seq}\n");
        for &i in idxs { s.push_str(&format!("#EXTINF:1.0,\nseg{i}.ts\n")); }
        if end { s.push_str("#EXT-X-ENDLIST\n"); }
        s.into_bytes()
    };
    let playlists = Arc::new(vec![
        mk(0, &[0,1], false),
        mk(1, &[1,2], false),
        mk(2, &[2,3], true),
    ]);
    let calls = Arc::new(Mutex::new(0usize));

    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let segs_srv = segs.clone();
    let pl_srv = playlists.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let url = req.url().trim_start_matches('/').to_string();
            let body: Option<Vec<u8>> = if url == "live.m3u8" {
                let mut n = calls.lock().unwrap();
                let b = pl_srv[(*n).min(pl_srv.len()-1)].clone();
                *n += 1;
                Some(b)
            } else if let Some(i) = url.strip_prefix("seg").and_then(|s| s.strip_suffix(".ts")) {
                i.parse::<usize>().ok().and_then(|i| segs_srv.get(i).cloned())
            } else { None };
            let _ = req.respond(match body {
                Some(b) => Response::from_data(b).with_status_code(200),
                None => Response::from_data(Vec::new()).with_status_code(404),
            });
        }
    });

    let fetcher = ReqwestFetcher::new();
    let remux = CliRemuxer { ffmpeg: ffmpeg.clone() };
    let dest = dir.path().join("out.mp4");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let url = format!("http://127.0.0.1:{port}/live.m3u8");
    let outcome = download_hls_live(
        &fetcher, &remux, "smk", &url, None, &dest,
        DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(),
        None, RefreshInterval::Fixed(Duration::from_millis(10)),
    ).await.expect("live record ok");
    assert_eq!(outcome, Outcome::Completed);
    assert!(dest.exists());

    // Décodage propre = .mp4 valide assemblé depuis les 4 segments.
    let chk = Command::new(&ffmpeg).args(["-hide_banner","-loglevel","error","-i"])
        .arg(&dest).args(["-f","null","-"]).output().unwrap();
    assert!(chk.status.success() && chk.stderr.is_empty(),
        "décodage du .mp4 live échoué: {}", String::from_utf8_lossy(&chk.stderr));
}
```

- [ ] **Step 2: Compiler (sans lancer)**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test hls_live_smoke --no-run 2>&1 | tail -5`
Expected: compile OK.

- [ ] **Step 3: Lancer le smoke-test (ffmpeg réel)**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test hls_live_smoke -- --ignored --nocapture`
Expected: PASS — `.mp4` produit, décodage sans erreur.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/download-core/tests/hls_live_smoke.rs
git commit -m "$(printf 'test(hls): offline live recording smoke-test (refresh loop to ENDLIST)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 12: Vérification finale

- [ ] **Step 1: Suite Rust complète + 0 warning**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "test result|warning"`
Expected: tous `ok`, aucun `warning`.
Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "test result|warning" | head`
Expected: lib tauri `ok`, aucun warning.

- [ ] **Step 2: Smoke-tests offline (vrai ffmpeg)**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test hls_live_smoke --test dash_clearkey_smoke -- --ignored`
Expected: PASS.

- [ ] **Step 3: Front**

Run: `npm test`
Expected: PASS.
Run: `npm run build 2>&1 | tail -3`
Expected: OK.

- [ ] **Step 4: Smoke GUI manuel (si flux live dispo)**

Lancer l'app, capturer un flux HLS live de test (IPTV/HLS live public), vérifier : ligne en mode Enregistrement (minuteur, segments, taille), bouton Stop → `.mp4` lisible. Tester aussi un flux avec `ENDLIST` (fin auto).

- [ ] **Step 5: Le merge est géré séparément** via `finishing-a-development-branch` (hors plan).

---

## Self-review (auteur)

- **Couverture spec** : §3 moteur → T4/T5 ; §4 stop/cancel → T1/T5 ; §5 dispatch → T6 ; §6 statut/commande → T2/T7 ; §7 recovery → T8 ; §5 probe media-directe → T9 ; UI §Section 3 → T10 ; tests §8 → T5/T10/T11. ✅
- **Pas de placeholder** : code complet fourni à chaque step ; T9/T10 contiennent une étape d'inspection préalable car les noms exacts (structs `probe_hls`, props front) doivent être lus dans le code — le repli/code est fourni, à adapter aux noms réels.
- **Cohérence des types** : `RefreshInterval` (T4) réutilisé en T5/T6/T11 ; `download_hls_live` signature identique partout ; `LiveTrack`/`Step` internes à `live.rs` ; `DownloadStatus::Recording` (T2) utilisé en T5/T7/T8/T10 ; `stop()`/`is_stopped()` (T1) utilisés en T5/T7.
- **Risque connu** : T9/T10 dépendent de structures existantes non recopiées ici (probe_hls, MediaVariant, composant DownloadRow exact) → étape d'inspection en tête de tâche.
