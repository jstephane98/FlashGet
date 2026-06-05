# Phase 4b-2b — DASH dynamic live (SegmentTimeline) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enregistrer un flux DASH dynamic live (`MPD@type="dynamic"`, SegmentTimeline) vers un `.mp4`, avec ClearKey, arrêt manuel ou fin de flux.

**Architecture:** Nouveau moteur `dash/live.rs::download_dash_live` parallèle à `hls/live.rs` (Approche A). On extrait d'abord les mécaniques communes : `RefreshInterval`/`responsive_sleep` → `crate::live`, et la finalisation concat+remux → `media_io::finalize_and_remux` (paie la dette « règle de trois »). `download_dash` délègue le dynamic au live ; `probe_dash` accepte le dynamic ; la recovery déchiffre via une clé persistée dans le work dir.

**Tech Stack:** Rust (tokio, async-trait), crate `download-core` + crate Tauri ; ffmpeg sidecar. Pas de changement front.

**Spec :** `docs/superpowers/specs/2026-06-04-phase-4b2b-dash-live-design.md`.

**Conventions de test (projet) :**
- Cœur : `cargo test -p download-core --manifest-path src-tauri/Cargo.toml`
- Ciblé : `... --lib <module>::tests::<nom>`
- Smoke (vrai ffmpeg, `#[ignore]`) : `... --test <fichier> -- --ignored --nocapture`
- Barre projet : **0 warning rustc**. Commits `type(scope): …` + trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

**Branche :** `phase-4b2b-dash-live` (déjà créée, empilée sur `phase-4b2a-hls-live`). Le spec y est déjà commité.

---

## File Structure

- **Modify** `src-tauri/crates/download-core/src/dash/mpd.rs` — `Mpd.minimum_update_period`.
- **Create** `src-tauri/crates/download-core/src/live.rs` — `RefreshInterval` + `responsive_sleep` (déplacés depuis hls/live).
- **Modify** `src-tauri/crates/download-core/src/lib.rs` — `pub mod live;`.
- **Modify** `src-tauri/crates/download-core/src/hls/live.rs` — importer `crate::live::{…}` ; utiliser `finalize_and_remux`.
- **Modify** `src-tauri/crates/download-core/src/media_io.rs` — `finalize_and_remux`.
- **Create** `src-tauri/crates/download-core/src/dash/live.rs` — `download_dash_live`.
- **Modify** `src-tauri/crates/download-core/src/dash/mod.rs` — `pub mod live;`.
- **Modify** `src-tauri/crates/download-core/src/dash/engine.rs` — `download_dash` délègue ; `resolve_key` en `pub(crate)`.
- **Modify** `src-tauri/src/commands.rs` — `probe_dash_inner` accepte dynamic ; recovery lit `clearkey.hex`.
- **Create** `src-tauri/crates/download-core/tests/dash_live_smoke.rs` — smoke offline.

---

## Task 1: `Mpd.minimum_update_period`

**Files:**
- Modify: `src-tauri/crates/download-core/src/dash/mpd.rs`

- [ ] **Step 1: Write the failing test**

In `mpd.rs` `tests` module, add:
```rust
    #[test]
    fn parses_minimum_update_period_for_dynamic() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT2S"><Period/></MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert!(m.dynamic);
        assert_eq!(m.minimum_update_period, Some(2.0));
    }

    #[test]
    fn minimum_update_period_absent_is_none() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT4S"><Period/></MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert_eq!(m.minimum_update_period, None);
    }
```

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::mpd::tests::parses_minimum_update_period_for_dynamic`
Expected: FAIL — no field `minimum_update_period`.

- [ ] **Step 3: Implement**

In `struct Mpd` (around line 75-81), add the field after `duration_secs`:
```rust
    /// MPD@minimumUpdatePeriod en secondes (cadence de refresh du live), si présent.
    pub minimum_update_period: Option<f64>,
```
In `parse_mpd` (around line 87-89), after `let duration_secs = ...;`, add:
```rust
    let minimum_update_period = attr(&root, "minimumUpdatePeriod").and_then(|s| parse_iso8601_duration(&s));
```
In the final `Ok(Mpd { ... })` (around line 123), add `minimum_update_period`:
```rust
    Ok(Mpd { dynamic, duration_secs, minimum_update_period, periods })
```

- [ ] **Step 4: Run + full mpd tests**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::mpd`
Expected: PASS (new tests + existing).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK` (no other `Mpd { … }` literal breaks — there is only one constructor in mpd.rs; the engine builds Mpd only via parse_mpd).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/dash/mpd.rs
git commit -m "$(printf 'feat(core): parse MPD@minimumUpdatePeriod (dynamic refresh cadence)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 2: `crate::live` module (move RefreshInterval + responsive_sleep)

**Files:**
- Create: `src-tauri/crates/download-core/src/live.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs`
- Modify: `src-tauri/crates/download-core/src/hls/live.rs`

- [ ] **Step 1: Create the shared module**

Create `src-tauri/crates/download-core/src/live.rs`:
```rust
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
```

- [ ] **Step 2: Register the module**

In `lib.rs`, add (near the other `pub mod` lines, e.g. after `pub mod media_io;`):
```rust
pub mod live;
```

- [ ] **Step 3: Remove the originals from `hls/live.rs` and import from `crate::live`**

In `hls/live.rs`:
- DELETE the `RefreshInterval` enum + its `impl` (current lines ~19-38) and the `responsive_sleep` fn (current lines ~40-54).
- The module-level `use std::time::Duration;` (line 4) becomes test-only after the move (the engine body no longer names `Duration`). A module-level import used only by `#[cfg(test)]` code warns in a normal build. So MOVE it: delete the top-level `use std::time::Duration;` and add `use std::time::Duration;` INSIDE the `#[cfg(test)] mod tests` block (the tests construct `RefreshInterval::Fixed(Duration::from_millis(0))`). Verify with the 0-warning check in Step 5.
- Add an import so the rest of the file (and tests via `use super::*`) sees the moved items:
```rust
use crate::live::{responsive_sleep, RefreshInterval};
```
- The call site `refresh.compute(vpl.target_duration)` stays unchanged (signature is now `compute(base_secs)` — same call).
- The public re-export: external callers (Tauri `launch`, and Task delegations) referenced `crate::hls::live::RefreshInterval`. To avoid breaking them AND keep a single source of truth, add a re-export in `hls/live.rs`:
```rust
pub use crate::live::RefreshInterval;
```
(So `crate::hls::live::RefreshInterval` still resolves. The `download_hls` delegation in `hls/engine.rs` used `crate::hls::live::RefreshInterval::FromTargetDuration` — that must become `::Derived`; update it in Step 4.)

- [ ] **Step 4: Update the `FromTargetDuration` → `Derived` rename at the call site**

In `hls/engine.rs`, the live delegation passes `crate::hls::live::RefreshInterval::FromTargetDuration`. Change it to `crate::hls::live::RefreshInterval::Derived` (the re-export resolves to `crate::live::RefreshInterval::Derived`).

Search to be safe:
Run: `grep -rn "FromTargetDuration" src-tauri/`
Expected after edits: no matches.

- [ ] **Step 5: Run hls::live + hls::engine + live tests**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib live hls::live hls::engine`
Expected: PASS (the moved `live::tests` 3 tests + all hls::live tests + hls::engine tests).
Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK` (the Tauri crate compiles; if `launch` referenced `RefreshInterval`, it still resolves via the re-export — but `launch` does NOT pass RefreshInterval; only the hls/engine delegation does).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/download-core/src/live.rs src-tauri/crates/download-core/src/lib.rs src-tauri/crates/download-core/src/hls/live.rs src-tauri/crates/download-core/src/hls/engine.rs
git commit -m "$(printf 'refactor(core): move RefreshInterval+responsive_sleep to crate::live (Derived)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 3: `media_io::finalize_and_remux` + convert hls/live

**Files:**
- Modify: `src-tauri/crates/download-core/src/media_io.rs`
- Modify: `src-tauri/crates/download-core/src/hls/live.rs`

- [ ] **Step 1: Write the failing test**

In `media_io.rs` `tests` module, add (the FakeRemuxer concatenates video then audio and records the inputs):
```rust
    #[tokio::test]
    async fn finalize_concats_and_passes_key() {
        use crate::hls::remux::fake::FakeRemuxer;
        let dir = tempdir().unwrap();
        let w = dir.path();
        // 2 segments vidéo + 1 audio, pas d'init.
        std::fs::write(w.join("v0.seg"), b"V0").unwrap();
        std::fs::write(w.join("v1.seg"), b"V1").unwrap();
        std::fs::write(w.join("a0.seg"), b"A0").unwrap();
        let out = w.join("out.mp4");
        let remux = FakeRemuxer::default();
        finalize_and_remux(w, false, Some(false), Some("deadbeef".into()), &remux, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"V0V1A0");
        let calls = remux.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].decryption_key.as_deref(), Some("deadbeef"));
        assert!(calls[0].audio.is_some());
    }

    #[tokio::test]
    async fn finalize_video_only_no_key() {
        use crate::hls::remux::fake::FakeRemuxer;
        let dir = tempdir().unwrap();
        let w = dir.path();
        std::fs::write(w.join("v0.seg"), b"XX").unwrap();
        let out = w.join("out.mp4");
        let remux = FakeRemuxer::default();
        finalize_and_remux(w, false, None, None, &remux, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"XX");
        let calls = remux.calls.lock().unwrap();
        assert!(calls[0].audio.is_none());
        assert!(calls[0].decryption_key.is_none());
    }
```

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib media_io::tests::finalize_concats_and_passes_key`
Expected: FAIL — `finalize_and_remux` does not exist.

- [ ] **Step 3: Implement `finalize_and_remux`**

At the top of `media_io.rs`, add the import:
```rust
use crate::hls::remux::{RemuxInputs, Remuxer};
```
Add the function (after `concat_track`):
```rust
/// Concatène la piste vidéo (et audio si présente) puis remux en `dest` via
/// ffmpeg. `audio_has_init = None` ⇒ pas de piste audio. La concat (IO lourde)
/// tourne sur un thread bloquant.
pub async fn finalize_and_remux<R: Remuxer>(
    work: &Path,
    video_has_init: bool,
    audio_has_init: Option<bool>,
    decryption_key: Option<String>,
    remux: &R,
    dest: &Path,
) -> Result<(), CoreError> {
    let work_v = work.to_path_buf();
    let video = tokio::task::spawn_blocking(move || concat_track(&work_v, "v", video_has_init))
        .await
        .map_err(|e| CoreError::Media(format!("tâche de concaténation interrompue: {e}")))??;
    let audio = if let Some(a_has_init) = audio_has_init {
        let work_a = work.to_path_buf();
        Some(
            tokio::task::spawn_blocking(move || concat_track(&work_a, "a", a_has_init))
                .await
                .map_err(|e| CoreError::Media(format!("tâche de concaténation interrompue: {e}")))??,
        )
    } else {
        None
    };
    remux.remux(RemuxInputs { video, audio, decryption_key }, dest).await
}
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib media_io::tests`
Expected: PASS.

- [ ] **Step 5: Convert `hls/live.rs` finalize to use the helper**

In `hls/live.rs`, replace the finalize block (current lines ~212-225: the `spawn_blocking` concat of v, the audio concat, and the `remux.remux(...)` call) with:
```rust
    // --- finalisation : concat + remux (helper partagé) ---
    crate::media_io::finalize_and_remux(
        &work,
        v.has_init,
        a.has_any.then_some(a.has_init),
        None,
        remux,
        dest,
    )
    .await?;
```
Remove now-unused imports in `hls/live.rs`: `concat_track` (no longer called directly), `RemuxInputs` (no longer constructed here). KEEP `Remuxer` (the generic bound `R: Remuxer` on `download_hls_live` still needs the trait in scope) — verify whether `Remuxer` is still referenced; the `R: Remuxer` bound needs the trait imported. So change `use crate::hls::remux::{RemuxInputs, Remuxer};` to `use crate::hls::remux::Remuxer;`. Adjust the `use crate::media_io::{...}` line to drop `concat_track` if unused (the engine still uses `fetch_to_vec, finish_cancelled, work_dir, write_segment`).

- [ ] **Step 6: Run hls::live + 0 warning**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib hls::live`
Expected: PASS (all 9 hls::live tests still green — finalize behavior identical).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning` (fix any unused-import warning from Step 5).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/download-core/src/media_io.rs src-tauri/crates/download-core/src/hls/live.rs
git commit -m "$(printf 'refactor(core): media_io::finalize_and_remux shared by live engines\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 4: `dash/live.rs::download_dash_live` (core)

**Files:**
- Create: `src-tauri/crates/download-core/src/dash/live.rs`
- Modify: `src-tauri/crates/download-core/src/dash/mod.rs`
- Modify: `src-tauri/crates/download-core/src/dash/engine.rs` (make `resolve_key` `pub(crate)`)

- [ ] **Step 1: Expose `resolve_key` + declare the module**

In `dash/engine.rs`, change `async fn resolve_key<F: ByteFetcher>(` to `pub(crate) async fn resolve_key<F: ByteFetcher>(`.
In `dash/mod.rs`, add:
```rust
pub mod live;
```

- [ ] **Step 2: Write the failing test (capture + dedup + end on static)**

Create `dash/live.rs` with ONLY the test module first won't compile (no `download_dash_live`); instead create the file with the full implementation in Step 4 and the tests here. Write the file's `#[cfg(test)] mod tests` containing:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::engine::DashSelection;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::live::RefreshInterval;
    use crate::model::ProbeResult;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use futures_util::StreamExt;
    use std::collections::HashMap as Map;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    /// MPD dynamic SegmentTimeline ; `media="seg-$Time$.m4s"`. `segs` = couples (t).
    fn mpd(times: &[u64], dynamic: bool) -> String {
        let s_elems: String = times.iter().enumerate().map(|(i, t)| {
            if i == 0 { format!("<S t=\"{t}\" d=\"1\"/>") } else { "<S d=\"1\"/>".to_string() }
        }).collect();
        let typ = if dynamic { "dynamic" } else { "static" };
        let mup = if dynamic { " minimumUpdatePeriod=\"PT1S\"" } else { "" };
        format!(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="{typ}"{mup}>
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Time$.m4s" timescale="1">
      <SegmentTimeline>{s_elems}</SegmentTimeline>
    </SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#)
    }

    /// Fetcher : MPD évolutif (corps successifs) ; segments depuis une map ;
    /// peut poser stop/cancel après N fetchs de segment. `post_resp` pour ClearKey.
    struct EvolvingMpd {
        mpd_url: String,
        mpds: Vec<String>,
        calls: Mutex<usize>,
        segs: Map<String, Vec<u8>>,
        handle: Option<DownloadHandle>,
        stop_after: usize,
        cancel_after: usize,
        seg_fetches: Arc<Mutex<usize>>,
        post_resp: Option<Vec<u8>>,
    }
    #[async_trait]
    impl ByteFetcher for EvolvingMpd {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
        }
        async fn fetch_range(&self, url: &str, _r: Option<(u64, u64)>, _h: &RequestHeaders)
            -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = if url == self.mpd_url {
                let mut n = self.calls.lock().unwrap();
                let b = self.mpds[(*n).min(self.mpds.len() - 1)].clone();
                *n += 1;
                b.into_bytes()
            } else {
                let mut c = self.seg_fetches.lock().unwrap();
                *c += 1;
                if let Some(h) = &self.handle {
                    if self.stop_after != 0 && *c == self.stop_after { h.stop(); }
                    if self.cancel_after != 0 && *c == self.cancel_after { h.cancel(); }
                }
                self.segs.get(url).cloned().unwrap_or_default()
            };
            Ok(stream::iter(vec![Ok(Bytes::from(data))]).boxed())
        }
        async fn post(&self, _u: &str, _b: &[u8], _ct: &str, _h: &RequestHeaders) -> Result<Vec<u8>, CoreError> {
            Ok(self.post_resp.clone().unwrap_or_default())
        }
    }
    fn unlimited() -> Arc<RateLimiter> { Arc::new(RateLimiter::unlimited()) }
    fn sel() -> DashSelection { DashSelection { video_index: 0, audio_lang: None } }

    #[tokio::test]
    async fn captures_dedups_and_ends_on_static() {
        let base = "https://h/p/";
        // refresh 0: t=0,1 ; refresh 1 (slide): t=1,2 ; refresh 2 (static): t=2,3.
        let mpds = vec![mpd(&[0, 1], true), mpd(&[1, 2], true), mpd(&[2, 3], false)];
        let mut segs = Map::new();
        for (t, b) in [(0u64,"AA"),(1,"BB"),(2,"CC"),(3,"DD")] {
            segs.insert(format!("{base}seg-{t}.m4s"), b.as_bytes().to_vec());
        }
        let seg_fetches = Arc::new(Mutex::new(0));
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: seg_fetches.clone(), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D1", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABBCCDD");
        assert_eq!(*seg_fetches.lock().unwrap(), 4, "dédup par URL : 4 fetchs");
        assert!(!dir.path().join("out.mp4.fghls").exists());
    }

    #[tokio::test]
    async fn manual_stop_finalizes_partial() {
        let base = "https://h/p/";
        let mpds = vec![mpd(&[0, 1], true)]; // ne finit jamais
        let mut segs = Map::new();
        for (t, b) in [(0u64,"AA"),(1,"BB")] { segs.insert(format!("{base}seg-{t}.m4s"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: Some(h.clone()), stop_after: 2, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D2", &format!("{base}m.mpd"), &sel(), &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABB");
    }

    #[tokio::test]
    async fn cancel_discards() {
        let base = "https://h/p/";
        let mpds = vec![mpd(&[0, 1], true)];
        let mut segs = Map::new();
        for (t, b) in [(0u64,"AA"),(1,"BB")] { segs.insert(format!("{base}seg-{t}.m4s"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: Some(h.clone()), stop_after: 0, cancel_after: 1, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D3", &format!("{base}m.mpd"), &sel(), &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(!dest.exists());
        assert!(!dir.path().join("out.mp4.fghls").exists());
    }

    #[tokio::test]
    async fn clearkey_resolved_persisted_and_passed() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let base = "https://h/p/";
        // MPD chiffré ClearKey (1 refresh static pour finir vite).
        let key: Vec<u8> = (0u8..16).collect();
        let k_b64 = URL_SAFE_NO_PAD.encode(&key);
        let jwk = format!(r#"{{"keys":[{{"kty":"oct","k":"{k_b64}","kid":"x"}}]}}"#);
        let xml = format!(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" xmlns:cenc="urn:mpeg:cenc:2013" type="static">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" cenc:default_KID="11112222-3333-4444-5555-666677778888"/>
    <ContentProtection schemeIdUri="urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e">
      <dashif:laurl xmlns:dashif="https://dashif.org/CPS">{base}license</dashif:laurl>
    </ContentProtection>
    <SegmentTemplate media="seg-$Time$.m4s" timescale="1"><SegmentTimeline><S t="0" d="1"/></SegmentTimeline></SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#);
        let mut segs = Map::new();
        segs.insert(format!("{base}seg-0.m4s"), b"ENC".to_vec());
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds: vec![xml], calls: Mutex::new(0), segs,
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)),
            post_resp: Some(jwk.into_bytes()),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D4", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        // la clé hex (000102…0f) a été transmise au remux.
        let calls = remux.calls.lock().unwrap();
        assert_eq!(calls[0].decryption_key.as_deref(), Some("000102030405060708090a0b0c0d0e0f"));
    }

    #[tokio::test]
    async fn rejects_number_mode_without_timeline() {
        let base = "https://h/p/";
        // dynamic mais SegmentTemplate $Number$ + duration SANS timeline.
        let xml = format!(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT1S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Number$.m4s" timescale="1" duration="1" startNumber="1"/>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#);
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds: vec![xml], calls: Mutex::new(0), segs: Map::new(),
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = download_dash_live(
            &fetcher, &remux, "D5", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }
}
```

- [ ] **Step 3: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::live --no-run`
Expected: FAIL to compile — `download_dash_live`, `DashLiveTrack`, etc. not defined.

- [ ] **Step 4: Implement `download_dash_live` + `DashLiveTrack`**

At the TOP of `dash/live.rs` (before the test module), write:
```rust
//! Enregistrement DASH dynamic live (SegmentTimeline) : refresh du MPD, capture
//! incrémentale dédupliquée par URL, finalisation concat + remux (ClearKey via
//! ffmpeg). Voir spec 2026-06-04-phase-4b2b-dash-live-design.md.
use crate::dash::engine::{resolve_key, DashSelection};
use crate::dash::mpd::{parse_mpd, Addressing, Mpd};
use crate::dash::segments::{build_segments, SegmentPlan};
use crate::dash::select::{pick_audio, video_variants};
use crate::engine::{DownloadHandle, Outcome};
use crate::hls::remux::Remuxer;
use crate::http::ByteFetcher;
use crate::live::{responsive_sleep, RefreshInterval};
use crate::media_io::{fetch_to_vec, finalize_and_remux, finish_cancelled, work_dir, write_segment};
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::CoreError;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Issue d'une passe de capture d'une piste.
enum Step { Continue, Stopped, Cancelled }

/// État d'une piste (vidéo/audio) entre rafraîchissements.
struct DashLiveTrack {
    prefix: &'static str,
    seen: HashSet<String>, // URLs déjà capturées (dédup)
    counter: usize,        // compteur de capture (nommage <prefix>_<counter>.seg)
    init_done: bool,
    has_init: bool,
    has_any: bool,
}

impl DashLiveTrack {
    fn new(prefix: &'static str) -> Self {
        Self { prefix, seen: HashSet::new(), counter: 0, init_done: false, has_init: false, has_any: false }
    }

    #[allow(clippy::too_many_arguments)]
    async fn capture<F: ByteFetcher>(
        &mut self, plan: &SegmentPlan, fetcher: &F, work: &Path, headers: &RequestHeaders,
        limiter: &Arc<RateLimiter>, handle: &DownloadHandle,
        progress: &mpsc::UnboundedSender<Progress>, id: &str, total_done: &mut u32,
    ) -> Result<Step, CoreError> {
        if !self.init_done {
            if let Some(init) = &plan.init {
                let bytes = match fetch_to_vec(fetcher, &init.url, init.byte_range, headers, limiter, handle).await? {
                    Some(b) => b, None => return Ok(Step::Cancelled),
                };
                std::fs::write(work.join(format!("{}_init.seg", self.prefix)), bytes)?;
                self.has_init = true;
            }
            self.init_done = true;
        }
        for seg in &plan.media {
            if self.seen.contains(&seg.url) { continue; }
            if handle.is_cancelled() { return Ok(Step::Cancelled); }
            if handle.is_stopped() { return Ok(Step::Stopped); }
            let bytes = match fetch_to_vec(fetcher, &seg.url, seg.byte_range, headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Step::Cancelled),
            };
            write_segment(work, self.prefix, self.counter, &bytes)?;
            self.counter += 1;
            self.seen.insert(seg.url.clone());
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

/// Coordonnées + clé résolues une fois au 1ᵉʳ rafraîchissement.
struct Resolved {
    vcoord: crate::dash::select::ReprCoord,
    acoord: Option<crate::dash::select::ReprCoord>,
    key: Option<String>,
}

/// Enregistre un flux DASH dynamic live (SegmentTimeline) vers `dest` (.mp4).
#[allow(clippy::too_many_arguments)]
pub async fn download_dash_live<F: ByteFetcher, R: Remuxer>(
    fetcher: &F, remux: &R, id: &str,
    mpd_url: &str, selection: &DashSelection, dest: &Path,
    handle: DownloadHandle, limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>, headers: &RequestHeaders,
    initial: Option<Mpd>, refresh: RefreshInterval,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    let mut pending = initial;
    let mut resolved: Option<Resolved> = None;
    let mut v = DashLiveTrack::new("v");
    let mut a = DashLiveTrack::new("a");
    let mut total_done: u32 = 0;

    loop {
        let mpd = match pending.take() {
            Some(m) => m,
            None => match fetch_to_vec(fetcher, mpd_url, None, headers, &limiter, &handle).await? {
                Some(b) => parse_mpd(&String::from_utf8_lossy(&b), mpd_url)?,
                None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            },
        };

        // Résolution unique (variantes + garde de mode + clé ClearKey).
        if resolved.is_none() {
            let vlist = video_variants(&mpd);
            let (vcoord, _) = vlist
                .iter()
                .find(|(_, vv)| vv.index == selection.video_index)
                .or_else(|| vlist.first())
                .ok_or_else(|| CoreError::Media("aucune variante vidéo".into()))?;
            let vrepr = &mpd.periods[vcoord.period].adaptation_sets[vcoord.adaptation].representations[vcoord.representation];
            match &vrepr.addressing {
                Addressing::Template(t) if t.timeline.is_some() => {}
                _ => return Err(CoreError::Media("DASH live $Number$/SegmentList non supporté (à venir)".into())),
            }
            let acoord = pick_audio(&mpd, selection.audio_lang.as_deref());
            let key = resolve_key(fetcher, &mpd, vcoord.adaptation, headers).await?;
            if let Some(k) = &key {
                std::fs::write(work.join("clearkey.hex"), k)?;
            }
            resolved = Some(Resolved { vcoord: vcoord.clone(), acoord, key });
        }
        let r = resolved.as_ref().unwrap();

        // Vidéo.
        let vrepr = &mpd.periods[r.vcoord.period].adaptation_sets[r.vcoord.adaptation].representations[r.vcoord.representation];
        let vplan = build_segments(vrepr, &vrepr.base, mpd.duration_secs);
        match v.capture(&vplan, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
            Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            Step::Stopped => break,
            Step::Continue => {}
        }

        // Audio (optionnelle).
        if let Some(ac) = &r.acoord {
            let arepr = &mpd.periods[ac.period].adaptation_sets[ac.adaptation].representations[ac.representation];
            let aplan = build_segments(arepr, &arepr.base, mpd.duration_secs);
            match a.capture(&aplan, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
                Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
                Step::Stopped => break,
                Step::Continue => {}
            }
        }

        if handle.is_stopped() { break; }
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        // Fin de flux : MPD repassé static ou plus de minimumUpdatePeriod.
        if !mpd.dynamic || mpd.minimum_update_period.is_none() { break; }

        let interval = refresh.compute(mpd.minimum_update_period.unwrap_or(0.0));
        responsive_sleep(interval, &handle).await;
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
    }

    // Finalisation : concat + remux (clé ClearKey éventuelle).
    let key = resolved.and_then(|r| r.key);
    finalize_and_remux(&work, v.has_init, a.has_any.then_some(a.has_init), key, remux, dest).await?;

    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
        speed_bps: 0, status: DownloadStatus::Completed,
        segments_done: Some(total_done), segments_total: None,
    });
    Ok(Outcome::Completed)
}
```
NOTE on imports: `ReprCoord` is `crate::dash::select::ReprCoord` (used via full path in `Resolved`). `base64` is a crate dependency available to tests. If `ContentType`/other unused, don't import them. Confirm `pick_audio` returns `Option<ReprCoord>` and `video_variants` returns `Vec<(ReprCoord, DashVariant)>` (read `dash/select.rs`); `resolve_key(fetcher, &mpd, adaptation_index, headers)` returns `Result<Option<String>, CoreError>` (read `dash/engine.rs`). `DashSelection { video_index: u32, audio_lang: Option<String> }`.

- [ ] **Step 5: Run the dash::live tests**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::live`
Expected: PASS (5 tests). If `clearkey_resolved_persisted_and_passed` fails on the key hex, verify `resolve_key`/`parse_jwk_response` produce `000102…0f` for `k = base64url(0..16)`.
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning` (`download_dash_live` is `pub`; if `Resolved`/`DashLiveTrack` trigger no dead-code since used). Add `#[allow(dead_code)]` ONLY if a field is genuinely unused — prefer removing unused fields.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/download-core/src/dash/live.rs src-tauri/crates/download-core/src/dash/mod.rs src-tauri/crates/download-core/src/dash/engine.rs
git commit -m "$(printf 'feat(core): download_dash_live engine (SegmentTimeline, URL dedup, ClearKey)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 5: `download_dash` delegates dynamic to live

**Files:**
- Modify: `src-tauri/crates/download-core/src/dash/engine.rs`

- [ ] **Step 1: Replace the `rejects_dynamic_mpd` test with a delegation test**

In `dash/engine.rs` `tests` module, DELETE `rejects_dynamic_mpd` and add (the existing `MapFetcher` there has a `post_resp` field; a dynamic timeline MPD with a static-on-next would loop — to bound it deterministically with the single-body `MapFetcher`, serve a STATIC SegmentTimeline MPD via a NO-`minimumUpdatePeriod` dynamic body that ends after one pass):
```rust
    #[tokio::test]
    async fn dynamic_mpd_is_recorded_not_rejected() {
        // type=dynamic SANS minimumUpdatePeriod => le moteur live capture la
        // fenêtre puis termine (pas de refresh attendu).
        let mpd = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" mediaPresentationDuration="PT2S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Time$.m4s" timescale="1"><SegmentTimeline><S t="0" d="1"/><S d="1"/></SegmentTimeline></SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#;
        let mut routes = Map::new();
        routes.insert("https://h/p/m.mpd".to_string(), mpd.as_bytes().to_vec());
        routes.insert("https://h/p/seg-0.m4s".to_string(), b"AA".to_vec());
        routes.insert("https://h/p/seg-1.m4s".to_string(), b"BB".to_vec());
        let fetcher = MapFetcher { routes, post_resp: None };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let sel = DashSelection { video_index: 0, audio_lang: None };
        let outcome = download_dash(&fetcher, &remux, "dyn1", "https://h/p/m.mpd", &sel, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default()).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABB");
    }
```

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::engine::tests::dynamic_mpd_is_recorded_not_rejected`
Expected: FAIL — currently `download_dash` returns `Err("flux live DASH non supporté")`.

- [ ] **Step 3: Implement the delegation**

In `dash/engine.rs::download_dash`, replace:
```rust
    if mpd.dynamic {
        return Err(CoreError::Media("flux live DASH non supporté (à venir)".into()));
    }
```
with:
```rust
    if mpd.dynamic {
        return crate::dash::live::download_dash_live(
            fetcher, remux, id, mpd_url, selection, dest,
            handle, limiter, progress, headers,
            Some(mpd), crate::live::RefreshInterval::Derived,
        ).await;
    }
```
`mpd` is moved into the call (the VOD code below uses `mpd` but is unreachable on this branch due to `return`).

- [ ] **Step 4: Run + non-regression**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::engine`
Expected: PASS (VOD `downloads_clear_dash_and_remuxes` + new delegation test).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/dash/engine.rs
git commit -m "$(printf 'feat(core): download_dash delegates dynamic MPDs to download_dash_live\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 6: `probe_dash` accepts dynamic

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Remove the dynamic rejection**

In `commands.rs::probe_dash_inner` (around lines 211-214), delete:
```rust
    if mpd.dynamic {
        return Err("flux live DASH non supporté (à venir)".into());
    }
```
The rest (variant emission via `video_variants`) is unchanged — it works for dynamic MPDs.

- [ ] **Step 2: Compile**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK` (if `mpd` becomes used only for `video_variants`, no warning; the `parse_mpd` result is still used).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(printf 'feat(tauri): probe_dash accepts dynamic MPDs (enables live recording)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 7: Recovery reads `clearkey.hex` + uses `finalize_and_remux`

**Files:**
- Modify: `src-tauri/src/commands.rs` (`recover_interrupted_recordings`)

- [ ] **Step 1: Read the current recovery body**

Read `recover_interrupted_recordings` (around lines 536-640). It computes `v_has_init`, `a_has_init`, `has_audio`, then does two inline `spawn_blocking(concat_track)` calls and `remux.remux(RemuxInputs { ..., decryption_key: None }, &dest)`.

- [ ] **Step 2: Replace the inline concat+remux with the shared helper + key read**

Replace the block from `let work_v = work.clone();` down to the `match remux.remux(inputs, &dest).await {` line's `inputs` construction — i.e. replace the two `spawn_blocking` concat calls AND the `RemuxInputs { video, audio, decryption_key: None }` construction — so the remux uses `finalize_and_remux` with the persisted key. Concretely, replace:
```rust
            let work_v = work.clone();
            let video = match tokio::task::spawn_blocking(move || {
                download_core::media_io::concat_track(&work_v, "v", v_has_init)
            })
            .await
            {
                Ok(Ok(p)) => p,
                _ => {
                    eprintln!("recovery: concat video failed for {id}");
                    return;
                }
            };
            let audio = if has_audio {
                let work_a = work.clone();
                match tokio::task::spawn_blocking(move || {
                    download_core::media_io::concat_track(&work_a, "a", a_has_init)
                })
                .await
                {
                    Ok(Ok(p)) => Some(p),
                    _ => None,
                }
            } else {
                None
            };
            let inputs = download_core::hls::remux::RemuxInputs {
                video,
                audio,
                decryption_key: None,
            };
            use download_core::hls::remux::Remuxer;
            match remux.remux(inputs, &dest).await {
```
with:
```rust
            // Clé ClearKey persistée par le moteur DASH live (None pour HLS / clair).
            let decryption_key = std::fs::read_to_string(work.join("clearkey.hex"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let audio_arg = if has_audio { Some(a_has_init) } else { None };
            match download_core::media_io::finalize_and_remux(&work, v_has_init, audio_arg, decryption_key, &remux, &dest).await {
```
(The closing `{ Ok(()) => { ... } Err(e) => { ... } }` match arms stay as-is. The `use download_core::hls::remux::Remuxer;` line is removed since we no longer call `remux.remux` directly — `finalize_and_remux` takes the `&remux` and needs `R: Remuxer`, satisfied by `SidecarRemuxer`. The `RemuxInputs` import is no longer needed here.)

- [ ] **Step 3: Compile**

Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK`. Remove any now-unused imports (`RemuxInputs`, the inline `Remuxer` use) flagged by the compiler.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(printf 'feat(tauri): recovery decrypts via persisted clearkey.hex (DASH live)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 8: Offline smoke-test (real ffmpeg)

**Files:**
- Create: `src-tauri/crates/download-core/tests/dash_live_smoke.rs`

- [ ] **Step 1: Write the smoke-test**

Model on `tests/dash_clearkey_smoke.rs` and `tests/hls_live_smoke.rs` (CliRemuxer + tiny_http). Create `dash_live_smoke.rs`:
```rust
//! Smoke-test DASH dynamic live OFFLINE (#[ignore], exige le vrai ffmpeg).
//! Un serveur loopback sert un MPD dynamic SegmentTimeline qui évolue puis
//! repasse static ; download_dash_live via ReqwestFetcher + CliRemuxer → .mp4.
//!
//! Rejouer : cargo test -p download-core --test dash_live_smoke -- --ignored --nocapture

use download_core::dash::engine::DashSelection;
use download_core::dash::live::download_dash_live;
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::http::ReqwestFetcher;
use download_core::live::RefreshInterval;
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

/// Génère N segments fMP4 (init + chunks) via le muxer dash de ffmpeg, renvoie
/// (init_bytes, vec_of_segment_bytes).
fn make_fmp4_segments(ffmpeg: &Path, dir: &Path, n: usize) -> (Vec<u8>, Vec<Vec<u8>>) {
    // Encode une courte vidéo fragmentée puis découpe en init + fragments.
    let enc = dir.join("enc.mp4");
    let status = Command::new(ffmpeg).args([
        "-hide_banner","-loglevel","error","-y",
        "-f","lavfi","-i",&format!("testsrc=duration={n}:size=160x120:rate=10"),
        "-c:v","libx264","-pix_fmt","yuv420p","-g","10","-keyint_min","10","-sc_threshold","0",
        "-frag_duration","1000000",
        "-movflags","+frag_keyframe+empty_moov+default_base_moof",
    ]).arg(&enc).status().expect("ffmpeg enc");
    assert!(status.success());
    let data = std::fs::read(&enc).unwrap();
    // Découpe top-level : init = jusqu'au 1er moof ; chaque fragment = moof..moof.
    let mut moofs = Vec::new();
    let mut off = 0usize;
    while off + 8 <= data.len() {
        let size = u32::from_be_bytes(data[off..off+4].try_into().unwrap()) as usize;
        let typ = &data[off+4..off+8];
        let real = if size == 1 { u64::from_be_bytes(data[off+8..off+16].try_into().unwrap()) as usize } else { size };
        if real == 0 { break; }
        if typ == b"moof" { moofs.push(off); }
        off += real;
    }
    let init = data[..moofs[0]].to_vec();
    let mut segs = Vec::new();
    for (i, &s) in moofs.iter().enumerate() {
        let e = moofs.get(i+1).copied().unwrap_or(data.len());
        segs.push(data[s..e].to_vec());
    }
    (init, segs)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "exige le vrai ffmpeg.exe ; lancer avec --ignored"]
async fn dash_live_records_until_static() {
    let ffmpeg = ffmpeg_path();
    assert!(ffmpeg.exists(), "ffmpeg introuvable en {}", ffmpeg.display());
    let dir = tempdir().unwrap();
    let (init, segs) = make_fmp4_segments(&ffmpeg, dir.path(), 4);
    assert!(segs.len() >= 4, "attendu ≥4 fragments, obtenu {}", segs.len());

    // MPD évolutif : fenêtre de 2 segments (t en timescale=1, d=1) qui glisse,
    // puis static. media="seg-$Number$.m4s" avec startNumber qui suit la fenêtre.
    // On référence les segments par $Time$ pour une dédup stable.
    let mk = |start_t: u64, end: bool| {
        let typ = if end { "static" } else { "dynamic" };
        let mup = if end { String::new() } else { " minimumUpdatePeriod=\"PT1S\"".to_string() };
        format!(r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="{typ}"{mup}>
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate initialization="init.mp4" media="seg-$Time$.m4s" timescale="1">
      <SegmentTimeline><S t="{start_t}" d="1"/><S d="1"/></SegmentTimeline>
    </SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#)
    };
    let mpds = vec![mk(0, false), mk(1, false), mk(2, true)];
    let init_srv = init.clone();
    let segs_srv = segs.clone();
    let calls = Arc::new(Mutex::new(0usize));
    let mpds_srv = mpds.clone();

    let server = Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let url = req.url().trim_start_matches('/').to_string();
            let body: Option<Vec<u8>> = if url == "m.mpd" {
                let mut n = calls.lock().unwrap();
                let b = mpds_srv[(*n).min(mpds_srv.len()-1)].clone();
                *n += 1;
                Some(b.into_bytes())
            } else if url == "init.mp4" {
                Some(init_srv.clone())
            } else if let Some(t) = url.strip_prefix("seg-").and_then(|s| s.strip_suffix(".m4s")) {
                // seg-$Time$ avec d=1 → time = index du fragment
                t.parse::<usize>().ok().and_then(|i| segs_srv.get(i).cloned())
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
    let url = format!("http://127.0.0.1:{port}/m.mpd");
    let sel = DashSelection { video_index: 0, audio_lang: None };
    let outcome = download_dash_live(
        &fetcher, &remux, "smk", &url, &sel, &dest,
        DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(),
        None, RefreshInterval::Fixed(Duration::from_millis(10)),
    ).await.expect("dash live ok");
    assert_eq!(outcome, Outcome::Completed);
    assert!(dest.exists());

    let chk = Command::new(&ffmpeg).args(["-hide_banner","-loglevel","error","-i"])
        .arg(&dest).args(["-f","null","-"]).output().unwrap();
    assert!(chk.status.success(), "décodage du .mp4 live échoué: {}", String::from_utf8_lossy(&chk.stderr));
}
```
NOTE: the segment time→index mapping assumes `d=1` per segment so `$Time$` equals the fragment index (0,1,2,3). The window slides `t=0..`, `t=1..`, `t=2..`; dedup-by-URL ensures each `seg-N.m4s` is fetched once. If ffmpeg produces fewer than 4 fragments, lower the window/segment count or raise `duration`. Adjust if `make_fmp4_segments` yields a different count (the assert guards it).

- [ ] **Step 2: Compile (no run)**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test dash_live_smoke --no-run 2>&1 | tail -5`
Expected: compiles.

- [ ] **Step 3: Run the smoke-test**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test dash_live_smoke -- --ignored --nocapture`
Expected: PASS — `.mp4` produced, decodes cleanly. If the decode check is too strict (fMP4 concat across fragments), relax to `chk.status.success()` only (already the case). If it fails because the window/timeline doesn't line up with ffmpeg's fragment count, tune `make_fmp4_segments` and the MPD window per the NOTE; investigate rather than weaken the assertion.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/download-core/tests/dash_live_smoke.rs
git commit -m "$(printf 'test(dash): offline dynamic live smoke-test (SegmentTimeline to static)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 9: Final verification

- [ ] **Step 1: Full Rust suite + 0 warning**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "test result|warning"`
Expected: all `ok`, no `warning`.
Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "test result|warning" | head`
Expected: tauri lib `ok`, no warning.

- [ ] **Step 2: Offline smoke-tests (real ffmpeg)**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test dash_live_smoke --test hls_live_smoke --test dash_clearkey_smoke -- --ignored`
Expected: PASS.

- [ ] **Step 3: Front (non-regression — no front changes this phase)**

Run: `npm test`
Expected: PASS.
Run: `npm run build 2>&1 | tail -3`
Expected: OK.

- [ ] **Step 4: Manual GUI smoke (if a dynamic DASH live source is available)**

Launch app, capture a DASH dynamic live stream, verify: recording row (LIVE + DASH badges, timer, segments), Stop → playable `.mp4`. Test a stream that ends (type→static) for auto-finalize.

- [ ] **Step 5: Merge handled separately** via `finishing-a-development-branch` (out of plan scope).

---

## Self-review (auteur)

- **Couverture spec** : §3 MPD attr → T1 ; §3 probe → T6 ; §4 crate::live → T2 ; §4 finalize_and_remux → T3 ; §5 moteur → T4 ; §5 délégation → T5 ; §6 ClearKey persist+recovery → T4 (write) + T7 (read) ; §7 tests → T1/T3/T4/T8. ✅
- **Pas de placeholder** : code complet à chaque step ; T7/T8 contiennent une étape de lecture/réglage car ils dépendent de structures existantes (recover) ou de la sortie ffmpeg (count de fragments).
- **Cohérence des types** : `RefreshInterval::Derived` (T2) utilisé en T4/T5 ; `finalize_and_remux(work, video_has_init, audio_has_init: Option<bool>, decryption_key, remux, dest)` identique T3/T4/T7 ; `download_dash_live` signature identique T4/T5/T8 ; `DashLiveTrack`/`Resolved`/`Step` internes à dash/live.rs ; `resolve_key` rendu `pub(crate)` en T4 et appelé en T4 ; `Mpd.minimum_update_period` (T1) lu en T4.
- **Risque connu** : T8 dépend du nombre de fragments produits par ffmpeg (garde par assert + note de réglage) ; T2 touche `hls/live.rs` (couvert par ses tests 4b-2a).
