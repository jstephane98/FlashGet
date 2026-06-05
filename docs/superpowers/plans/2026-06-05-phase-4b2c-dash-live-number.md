# Phase 4b-2c — DASH dynamic live `$Number$` (wall-clock) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enregistrer un flux DASH dynamic live en mode SegmentTemplate `$Number$`+`@duration` (sans timeline), en calculant la fenêtre de segments disponible à l'horloge murale.

**Architecture:** Étendre `download_dash_live` (4b-2b) avec un dispatch de mode : timeline → chemin existant ; `$Number$`+duration → nouvelle `dash/segments.rs::live_number_window` calculée depuis une `Clock` injectable + `availabilityStartTime`/`timeShiftBufferDepth`. Un 404 (segment pas encore publié) est mappé en `CoreError::NotFound` et toléré par `capture`. Tout le reste (boucle, dédup, finalisation, ClearKey, recovery) est réutilisé.

**Tech Stack:** Rust (tokio), crate `download-core`. Pas de dépendance ajoutée (parseur datetime fait main). Pas de changement front/Tauri (hors la délégation qui passe `Clock::System`).

**Spec :** `docs/superpowers/specs/2026-06-05-phase-4b2c-dash-live-number-design.md`.

**Conventions de test :** cœur `cargo test -p download-core --manifest-path src-tauri/Cargo.toml` ; ciblé `... --lib <module>::tests::<nom>` ; intégration `... --test real_http` ; **0 warning rustc** ; commits `type(scope): …` + trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

**Branche :** `phase-4b2c-dash-live-number` (déjà créée, empilée sur `phase-4b2b-dash-live`). Le spec y est déjà commité.

---

## File Structure
- **Modify** `src-tauri/crates/download-core/src/dash/mpd.rs` — `parse_iso8601_datetime`, `days_from_civil`, `Mpd.availability_start_time`, `Mpd.time_shift_buffer_depth`.
- **Modify** `src-tauri/crates/download-core/src/dash/select.rs` — fixture de test `Mpd{}` (2 nouveaux champs `None`).
- **Modify** `src-tauri/crates/download-core/src/live.rs` — `Clock`.
- **Modify** `src-tauri/crates/download-core/src/http.rs` — `fetch_range` mappe 404/410 → `NotFound`.
- **Modify** `src-tauri/crates/download-core/tests/real_http.rs` — test 404→NotFound.
- **Modify** `src-tauri/crates/download-core/src/dash/segments.rs` — `live_number_window`.
- **Modify** `src-tauri/crates/download-core/src/dash/live.rs` — param `clock`, dispatch de mode, tolérance `NotFound`, tests.
- **Modify** `src-tauri/crates/download-core/src/dash/engine.rs` — délègue avec `Clock::System`.
- **Modify** `src-tauri/crates/download-core/tests/dash_live_smoke.rs` — passer `Clock::System` (signature).

---

## Task 1: `Mpd` wall-clock attributes + ISO-8601 datetime parser

**Files:**
- Modify: `src-tauri/crates/download-core/src/dash/mpd.rs`
- Modify: `src-tauri/crates/download-core/src/dash/select.rs` (test fixture)

- [ ] **Step 1: Write the failing tests**

In `mpd.rs` `tests` module, add:
```rust
    #[test]
    fn parses_datetime_to_epoch() {
        assert_eq!(super::parse_iso8601_datetime("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(super::parse_iso8601_datetime("2001-09-09T01:46:40Z"), Some(1_000_000_000.0));
        assert_eq!(super::parse_iso8601_datetime("1970-01-01T00:00:01.5Z"), Some(1.5));
        assert_eq!(super::parse_iso8601_datetime("2023-10-01T12:00:00+02:00"), None); // offset non géré
    }

    #[test]
    fn parses_availability_and_buffer_for_dynamic() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT2S" availabilityStartTime="1970-01-01T00:00:00Z" timeShiftBufferDepth="PT1M"><Period/></MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert_eq!(m.availability_start_time, Some(0.0));
        assert_eq!(m.time_shift_buffer_depth, Some(60.0));
    }

    #[test]
    fn availability_absent_is_none() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static"><Period/></MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert_eq!(m.availability_start_time, None);
        assert_eq!(m.time_shift_buffer_depth, None);
    }
```

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::mpd::tests::parses_datetime_to_epoch`
Expected: FAIL — `parse_iso8601_datetime` undefined.

- [ ] **Step 3: Implement**

In `struct Mpd` (after `minimum_update_period`), add:
```rust
    /// MPD@availabilityStartTime en epoch unix (s) — ancre du live $Number$.
    pub availability_start_time: Option<f64>,
    /// MPD@timeShiftBufferDepth en secondes (profondeur DVR).
    pub time_shift_buffer_depth: Option<f64>,
```
In `parse_mpd`, after the `minimum_update_period` line, add:
```rust
    let availability_start_time = attr(&root, "availabilityStartTime").and_then(|s| parse_iso8601_datetime(&s));
    let time_shift_buffer_depth = attr(&root, "timeShiftBufferDepth").and_then(|s| parse_iso8601_duration(&s));
```
In the `Ok(Mpd { ... })` constructor, add the two fields:
```rust
    Ok(Mpd { dynamic, duration_secs, minimum_update_period, availability_start_time, time_shift_buffer_depth, periods })
```
Add these two free functions near `parse_iso8601_duration` (at file scope, not inside `tests`):
```rust
/// Parse un datetime ISO-8601 UTC (`YYYY-MM-DDTHH:MM:SS(.fff)?(Z)?`) en epoch unix (s).
/// Seul UTC (`Z` ou pas de suffixe) est géré ; un offset signé (`+/-HH:MM`) → None.
fn parse_iso8601_datetime(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    if time.contains('+') || time.contains('-') {
        return None; // offset explicite non géré
    }
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: i64 = dp.next()?.parse().ok()?;
    let da: i64 = dp.next()?.parse().ok()?;
    if dp.next().is_some() { return None; }
    let mut tp = time.split(':');
    let h: f64 = tp.next()?.parse().ok()?;
    let mi: f64 = tp.next()?.parse().ok()?;
    let se: f64 = tp.next().unwrap_or("0").parse().ok()?;
    if tp.next().is_some() { return None; }
    let days = days_from_civil(y, mo, da)?;
    Some(days as f64 * 86400.0 + h * 3600.0 + mi * 60.0 + se)
}

/// Jours depuis 1970-01-01 (algorithme de Howard Hinnant). `m` ∈ [1, 12].
fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146097 + doe - 719468)
}
```

`dash/select.rs` test fixture: the `#[cfg(test)] fn mpd()` builds `Mpd { ... }` directly — add `availability_start_time: None, time_shift_buffer_depth: None,` to it (the compiler will point at the exact literal).

- [ ] **Step 4: Run + full mpd/select tests + build**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::mpd dash::select`
Expected: PASS.
Run: `cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning|error" || echo OK`
Expected: `OK`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/dash/mpd.rs src-tauri/crates/download-core/src/dash/select.rs
git commit -m "$(printf 'feat(core): parse availabilityStartTime/timeShiftBufferDepth (+ ISO datetime)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 2: `crate::live::Clock`

**Files:**
- Modify: `src-tauri/crates/download-core/src/live.rs`

- [ ] **Step 1: Write the failing test**

In `live.rs` `tests` module, add:
```rust
    #[test]
    fn clock_fixed_and_system() {
        assert_eq!(Clock::Fixed(123.5).now_epoch(), 123.5);
        // l'horloge système est postérieure à 2023-01-01 (epoch 1.6726e9).
        assert!(Clock::System.now_epoch() > 1_672_500_000.0);
    }
```

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib live::tests::clock_fixed_and_system`
Expected: FAIL — `Clock` undefined.

- [ ] **Step 3: Implement**

In `live.rs`, add at the top (after the existing `use` lines):
```rust
use std::time::{SystemTime, UNIX_EPOCH};
```
Add (after the `RefreshInterval` impl):
```rust
/// Source d'horloge murale (epoch unix, secondes). Injectable pour les tests.
#[derive(Debug, Clone, Copy)]
pub enum Clock {
    /// Production : `SystemTime::now()`.
    System,
    /// Tests : instant fixe.
    Fixed(f64),
}

impl Clock {
    pub fn now_epoch(&self) -> f64 {
        match self {
            Clock::System => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0),
            Clock::Fixed(t) => *t,
        }
    }
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib live::tests`
Expected: PASS.
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning` (Clock is `pub` → no dead-code even before it's used).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/live.rs
git commit -m "$(printf 'feat(core): injectable Clock (System/Fixed) for wall-clock live\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 3: `fetch_range` maps 404/410 → `CoreError::NotFound`

**Files:**
- Modify: `src-tauri/crates/download-core/src/http.rs`
- Modify: `src-tauri/crates/download-core/tests/real_http.rs`

- [ ] **Step 1: Write the failing test**

In `real_http.rs`, add (a dedicated server that always 404s):
```rust
#[tokio::test(flavor = "multi_thread")]
async fn fetch_range_404_maps_to_not_found() {
    use download_core::http::ByteFetcher;
    use download_core::CoreError;
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let _ = request.respond(Response::from_data(Vec::new()).with_status_code(404));
        }
    });
    let fetcher = ReqwestFetcher::new();
    let url = format!("http://127.0.0.1:{port}/missing.ts");
    let err = fetcher.fetch_range(&url, None, &RequestHeaders::default()).await.unwrap_err();
    assert!(matches!(err, CoreError::NotFound(_)), "attendu NotFound, obtenu {err:?}");
}
```
(`real_http.rs` already imports `tiny_http::{Header, Response, Server}` and `RequestHeaders`; reuse them. Add `use download_core::http::ByteFetcher;` inside the test or at top if not present.)

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test real_http fetch_range_404_maps_to_not_found`
Expected: FAIL — currently a 404 maps to `CoreError::Network`.

- [ ] **Step 3: Implement**

In `http.rs::fetch_range`, replace the line:
```rust
        let resp = resp.error_for_status().map_err(|e| CoreError::Network(describe_reqwest_error(&e)))?;
```
with:
```rust
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
            return Err(CoreError::NotFound(format!("HTTP {} pour {url}", status.as_u16())));
        }
        let resp = resp.error_for_status().map_err(|e| CoreError::Network(describe_reqwest_error(&e)))?;
```

- [ ] **Step 4: Run the test + non-regression**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test real_http`
Expected: PASS (new test + existing real_http tests).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/http.rs src-tauri/crates/download-core/tests/real_http.rs
git commit -m "$(printf 'feat(core): fetch_range maps HTTP 404/410 to CoreError::NotFound\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 4: `dash/segments.rs::live_number_window`

**Files:**
- Modify: `src-tauri/crates/download-core/src/dash/segments.rs`

- [ ] **Step 1: Write the failing tests**

In `segments.rs` `tests` module, add (the `repr(addr)` helper there builds a `Representation` with `base: ""`; pass a base via the `base` arg of `live_number_window`):
```rust
    fn tmpl(duration: u64, start_number: u64) -> SegmentTemplate {
        SegmentTemplate {
            init: Some("init.mp4".into()),
            media: "seg-$Number$.m4s".into(),
            timescale: 1, duration: Some(duration), start_number, timeline: None,
        }
    }

    #[test]
    fn number_window_single_edge_segment_no_buffer() {
        let r = repr(Addressing::Template(tmpl(2, 1)));
        let t = tmpl(2, 1);
        // ast=0, d=2, now=20, margin=2 → elapsed=18, count=9, edge_N=9, span=0.
        let plan = live_number_window(&r, &t, 0.0, None, 20.0, 2.0, "https://h/p/");
        assert_eq!(plan.init.unwrap().url, "https://h/p/init.mp4");
        assert_eq!(plan.media.iter().map(|s| s.url.clone()).collect::<Vec<_>>(),
            vec!["https://h/p/seg-9.m4s"]);
    }

    #[test]
    fn number_window_dvr_span() {
        let r = repr(Addressing::Template(tmpl(2, 1)));
        let t = tmpl(2, 1);
        // tsb=10, d=2 → span=5 ; edge_N=9 → start_N=4 → 4..=9.
        let plan = live_number_window(&r, &t, 0.0, Some(10.0), 20.0, 2.0, "https://h/p/");
        assert_eq!(plan.media.iter().map(|s| s.url.clone()).collect::<Vec<_>>(),
            vec!["https://h/p/seg-4.m4s","https://h/p/seg-5.m4s","https://h/p/seg-6.m4s",
                 "https://h/p/seg-7.m4s","https://h/p/seg-8.m4s","https://h/p/seg-9.m4s"]);
    }

    #[test]
    fn number_window_edge_advances_with_now() {
        let r = repr(Addressing::Template(tmpl(2, 1)));
        let t = tmpl(2, 1);
        let last = |now: f64| live_number_window(&r, &t, 0.0, None, now, 2.0, "https://h/p/")
            .media.last().unwrap().url.clone();
        assert_eq!(last(20.0), "https://h/p/seg-9.m4s");
        assert_eq!(last(22.0), "https://h/p/seg-10.m4s"); // edge avance d'un
    }

    #[test]
    fn number_window_empty_before_first_segment() {
        let r = repr(Addressing::Template(tmpl(2, 1)));
        let t = tmpl(2, 1);
        // now=2.5, margin=2, ast=0 → elapsed=0.5 < d=2 → vide.
        let plan = live_number_window(&r, &t, 0.0, None, 2.5, 2.0, "https://h/p/");
        assert!(plan.media.is_empty());
    }
```

- [ ] **Step 2: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::segments::tests::number_window_single_edge_segment_no_buffer`
Expected: FAIL — `live_number_window` undefined.

- [ ] **Step 3: Implement**

In `segments.rs`, add (after `build_segments`/`build_template`; it reuses the private `expand` + `resolve_uri`):
```rust
/// Fenêtre de segments disponible d'un live `$Number$`+`@duration` à l'instant
/// `now` (epoch s). `ast` = availabilityStartTime, `tsb` = timeShiftBufferDepth (s),
/// `margin` = marge de sécurité (s, typiquement une durée de segment). `base` =
/// BaseURL effective de la Representation. `init` est toujours résolu.
pub fn live_number_window(
    repr: &Representation,
    t: &SegmentTemplate,
    ast: f64,
    tsb: Option<f64>,
    now: f64,
    margin: f64,
    base: &str,
) -> SegmentPlan {
    let init = t.init.as_ref().map(|tpl| SegmentRef {
        url: resolve_uri(base, &expand(tpl, &repr.id, repr.bandwidth, 0, 0)),
        byte_range: None,
    });
    let d = t.duration.unwrap_or(0) as f64 / t.timescale.max(1) as f64;
    let elapsed = now - margin - ast;
    if d <= 0.0 || elapsed < d {
        return SegmentPlan { init, media: Vec::new() };
    }
    let count = (elapsed / d).floor() as u64; // segments pleinement publiés
    let edge_n = t.start_number + count - 1;
    let span = tsb.map(|x| (x / d).floor() as u64).unwrap_or(0);
    let start_n = t.start_number.max(edge_n.saturating_sub(span));
    let media = (start_n..=edge_n)
        .map(|n| {
            let time = (n - t.start_number) * t.duration.unwrap_or(0);
            SegmentRef {
                url: resolve_uri(base, &expand(&t.media, &repr.id, repr.bandwidth, n, time)),
                byte_range: None,
            }
        })
        .collect();
    SegmentPlan { init, media }
}
```
NOTE: `expand`, `resolve_uri`, `SegmentRef`, `SegmentPlan`, `Representation`, `SegmentTemplate` are already in scope in `segments.rs`. Confirm `repr(...)` test helper and `Addressing` import exist in the `tests` module (the existing tests use them).

- [ ] **Step 4: Run the math tests**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::segments`
Expected: PASS (4 new + existing).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning` (`live_number_window` is `pub` → no dead-code warning before the engine uses it).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/download-core/src/dash/segments.rs
git commit -m "$(printf 'feat(core): live_number_window (wall-clock DASH $Number$ availability)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 5: Engine integration — mode dispatch + clock + 404 tolerance

**Files:**
- Modify: `src-tauri/crates/download-core/src/dash/live.rs`

- [ ] **Step 1: Add the `clock` parameter + update existing call sites (compile-only change first)**

Add `clock: Clock` as the LAST parameter of `download_dash_live`:
```rust
    initial: Option<Mpd>, refresh: RefreshInterval, clock: Clock,
) -> Result<Outcome, CoreError> {
```
Add to the imports at top of `dash/live.rs`: `use crate::live::{responsive_sleep, Clock, RefreshInterval};` (extend the existing `use crate::live::{...}`).
Update EVERY existing call site of `download_dash_live` to pass a clock:
- In `dash/live.rs`'s own `tests` module (the 7 calls in `captures_dedups_and_ends_on_static`, `manual_stop_finalizes_partial`, `cancel_discards`, `clearkey_resolved_persisted_and_passed`, `rejects_number_mode_without_timeline`, `captures_separate_audio_track`, `prepends_init_segment`): append `, Clock::Fixed(0.0)` as the last argument (after the `RefreshInterval::Fixed(...)`). Add `use crate::live::Clock;` to the test module imports (or rely on `use super::*` if it re-exports — add it explicitly to be safe).

Don't change behavior yet — this step only threads the parameter.

- [ ] **Step 2: Compile (existing tests still pass)**

Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "error" | head`
Expected: only errors are the OTHER call sites not yet updated (`dash/engine.rs`, `tests/dash_live_smoke.rs`) — those are Task 6 / Step 7 here. To keep this step self-contained, ALSO update them now:
- `dash/engine.rs` delegation: add `, crate::live::Clock::System` as the last arg.
- `tests/dash_live_smoke.rs`: add `, RefreshInterval` already imported? add `download_core::live::Clock` import and pass `, download_core::live::Clock::System` as the last arg of its `download_dash_live(...)` call.
Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::live 2>&1 | grep -E "test result"`
Expected: PASS — existing 7 dash::live tests green with the new param (Fixed clock, timeline mode ignores it). The test `rejects_number_mode_without_timeline` STILL passes for now (we haven't relaxed the guard yet — it's renamed/repurposed in Step 5).

- [ ] **Step 3: Write the failing number-mode integration tests**

Replace the existing `rejects_number_mode_without_timeline` test with the following number-mode tests (the old name asserted `$Number$` is rejected; we now SUPPORT it). Add to the `tests` module:
```rust
    /// MPD dynamic $Number$ (sans timeline). `ast`/`tsb`/segments choisis pour que
    /// quelques numéros soient disponibles à l'instant `now` fixe du test.
    fn mpd_number(start_number: u64, dynamic: bool, ast: &str, tsb: &str) -> String {
        let typ = if dynamic { "dynamic" } else { "static" };
        let mup = if dynamic { " minimumUpdatePeriod=\"PT1S\"" } else { "" };
        format!(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="{typ}"{mup} availabilityStartTime="{ast}" timeShiftBufferDepth="{tsb}">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Number$.m4s" timescale="1" duration="1" startNumber="{start_number}"/>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#)
    }

    #[tokio::test]
    async fn number_mode_captures_window_and_ends_on_static() {
        let base = "https://h/p/";
        // ast=0, d=1, margin=1. À now=5 : elapsed=4, count=4, edge_N=4 (start=1),
        // tsb=10 → span=10 → start_N=1 → segments 1..=4.
        // 1er MPD dynamic puis 2e static → un tour de capture puis fin.
        let mpds = vec![mpd_number(1, true, "1970-01-01T00:00:00Z", "PT10S"),
                        mpd_number(1, false, "1970-01-01T00:00:00Z", "PT10S")];
        let mut segs = Map::new();
        for n in 1..=4 { segs.insert(format!("{base}seg-{n}.m4s"), format!("S{n}").into_bytes()); }
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "N1", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)), Clock::Fixed(5.0),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"S1S2S3S4");
    }

    #[tokio::test]
    async fn number_mode_requires_availability_start_time() {
        let base = "https://h/p/";
        // dynamic $Number$ SANS availabilityStartTime.
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT1S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Number$.m4s" timescale="1" duration="1" startNumber="1"/>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#.to_string();
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds: vec![xml], calls: Mutex::new(0), segs: Map::new(),
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = download_dash_live(
            &fetcher, &remux, "N2", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)), Clock::Fixed(5.0),
        ).await.unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }

    #[tokio::test]
    async fn number_mode_tolerates_404() {
        let base = "https://h/p/";
        // segments 1..=4 attendus ; seg-3 absent → 404 → sauté, les autres capturés.
        let mpds = vec![mpd_number(1, true, "1970-01-01T00:00:00Z", "PT10S"),
                        mpd_number(1, false, "1970-01-01T00:00:00Z", "PT10S")];
        let mut segs = Map::new();
        for n in [1u64, 2, 4] { segs.insert(format!("{base}seg-{n}.m4s"), format!("S{n}").into_bytes()); }
        // seg-3 absent de la map → le fetcher renvoie NotFound (voir EvolvingMpd ci-dessous).
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "N3", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)), Clock::Fixed(5.0),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"S1S2S4"); // seg-3 sauté
    }
```
ALSO update `EvolvingMpd::fetch_range` so a MISSING segment URL returns `NotFound` (today it returns empty bytes). In the `else` branch (segment path), change `self.segs.get(url).cloned().unwrap_or_default()` handling so a miss yields a `NotFound` error stream. Replace the segment-serving part with:
```rust
            } else {
                let mut c = self.seg_fetches.lock().unwrap();
                *c += 1;
                if let Some(h) = &self.handle {
                    if self.stop_after != 0 && *c == self.stop_after { h.stop(); }
                    if self.cancel_after != 0 && *c == self.cancel_after { h.cancel(); }
                }
                match self.segs.get(url).cloned() {
                    Some(d) => d,
                    None => return Err(CoreError::NotFound(format!("missing {url}"))),
                }
            };
```
(Existing tests insert every requested URL into `segs`, so they keep getting bytes; only the new 404 test omits `seg-3`.)

- [ ] **Step 4: Run, verify failure**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::live::tests::number_mode_captures_window_and_ends_on_static`
Expected: FAIL — currently the resolve-once guard rejects `$Number$` (no timeline).

- [ ] **Step 5: Implement the mode dispatch + 404 tolerance**

5a. Add a mode helper + a per-track plan builder. Near the top of `dash/live.rs` (after the `use` lines, before `DashLiveTrack`), add:
```rust
use crate::dash::mpd::Representation;
use crate::dash::segments::live_number_window;

/// Construit le `SegmentPlan` courant d'une piste selon son mode d'adressage live.
fn track_plan(repr: &Representation, mpd: &Mpd, clock: &Clock) -> Result<crate::dash::segments::SegmentPlan, CoreError> {
    match &repr.addressing {
        Addressing::Template(t) if t.timeline.is_some() => {
            Ok(build_segments(repr, &repr.base, mpd.duration_secs))
        }
        Addressing::Template(t) if t.duration.is_some() => {
            let ast = mpd
                .availability_start_time
                .ok_or_else(|| CoreError::Media("availabilityStartTime requis pour DASH live $Number$".into()))?;
            let d = t.duration.unwrap_or(0) as f64 / t.timescale.max(1) as f64;
            Ok(live_number_window(repr, t, ast, mpd.time_shift_buffer_depth, clock.now_epoch(), d, &repr.base))
        }
        _ => Err(CoreError::Media("adressage DASH live non supporté (timeline ou $Number$ requis)".into())),
    }
}
```
(Ensure `build_segments` is imported — it already is. `SegmentPlan` path used fully-qualified to avoid an extra import; or add `use crate::dash::segments::{build_segments, live_number_window, SegmentPlan};`.)

5b. Relax the resolve-once guard. Replace the video guard block (currently `match &vrepr.addressing { Addressing::Template(t) if t.timeline.is_some() => {} _ => return Err(...) }`) with a check that accepts timeline OR `$Number$`-with-AST:
```rust
            match &vrepr.addressing {
                Addressing::Template(t) if t.timeline.is_some() => {}
                Addressing::Template(t) if t.duration.is_some() => {
                    if mpd.availability_start_time.is_none() {
                        return Err(CoreError::Media("availabilityStartTime requis pour DASH live $Number$".into()));
                    }
                }
                _ => return Err(CoreError::Media("adressage DASH live non supporté (timeline ou $Number$ requis)".into())),
            }
```
And the AUDIO guard similarly (replace the `if !matches!(... timeline.is_some()) { return Err(...) }` block):
```rust
            if let Some(ac) = &acoord {
                let arepr = &mpd.periods[ac.period].adaptation_sets[ac.adaptation].representations[ac.representation];
                let ok = matches!(&arepr.addressing, Addressing::Template(t) if t.timeline.is_some() || t.duration.is_some());
                if !ok {
                    return Err(CoreError::Media("adressage DASH live audio non supporté".into()));
                }
            }
```

5c. Replace the two `build_segments(...)` calls in the loop body with `track_plan(...)`:
- video: `let vplan = track_plan(vrepr, &mpd, &clock)?;`
- audio: `let aplan = track_plan(arepr, &mpd, &clock)?;`

5d. Make `DashLiveTrack::capture` tolerate `NotFound`. Replace the segment fetch match:
```rust
            let bytes = match fetch_to_vec(fetcher, &seg.url, seg.byte_range, headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Step::Cancelled),
            };
```
with:
```rust
            let bytes = match fetch_to_vec(fetcher, &seg.url, seg.byte_range, headers, limiter, handle).await {
                Ok(Some(b)) => b,
                Ok(None) => return Ok(Step::Cancelled),
                // Segment pas encore publié / expiré : on saute (sans marquer vu) ;
                // il sera retenté au prochain rafraîchissement.
                Err(CoreError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
```

- [ ] **Step 6: Run all dash::live tests + 0 warning**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib dash::live`
Expected: PASS — the 6 timeline tests (4b-2b) + 3 new number-mode tests (window/ends-on-static, requires-AST, tolerates-404).
Run: `cargo build -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "warning" || echo "0 warning"`
Expected: `0 warning`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/download-core/src/dash/live.rs src-tauri/crates/download-core/src/dash/engine.rs src-tauri/crates/download-core/tests/dash_live_smoke.rs
git commit -m "$(printf 'feat(core): DASH live $Number$ wall-clock mode (clock, dispatch, 404 tolerance)\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```

---

## Task 6: Confirm delegation passes `Clock::System`

(Done as part of Task 5 Step 2 — this task verifies it.)

**Files:**
- Verify: `src-tauri/crates/download-core/src/dash/engine.rs`

- [ ] **Step 1: Verify the delegation**

Run: `grep -n "download_dash_live" src-tauri/crates/download-core/src/dash/engine.rs`
Confirm the call passes `crate::live::Clock::System` as the last argument (after `RefreshInterval::Derived`). If not, add it.

- [ ] **Step 2: Full core suite**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "test result|warning:" | tail -3`
Expected: ok, no warnings.

- [ ] **Step 3: Commit (only if a change was needed)**

```bash
git add src-tauri/crates/download-core/src/dash/engine.rs
git commit -m "$(printf 'feat(core): download_dash delegation passes Clock::System\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>')"
```
(If Task 5 already committed engine.rs, this task is a no-op verification.)

---

## Task 7: Final verification

- [ ] **Step 1: Full Rust suite + 0 warning**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "test result|warning:"`
Expected: all `ok`, no `warning`.
Run: `cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "test result|warning:" | head`
Expected: tauri lib `ok`, no warning.

- [ ] **Step 2: Offline smoke-tests (real ffmpeg) — non-regression**

Run: `cargo test -p download-core --manifest-path src-tauri/Cargo.toml --test dash_live_smoke --test hls_live_smoke --test dash_clearkey_smoke -- --ignored`
Expected: PASS (the 4b-2b DASH-live smoke still green with the new `Clock::System` arg).

- [ ] **Step 3: Front (non-regression — no front changes)**

Run: `npm test`
Expected: PASS.
Run: `npm run build 2>&1 | tail -3`
Expected: OK.

- [ ] **Step 4: Merge handled separately** via `finishing-a-development-branch` (out of plan scope).

---

## Self-review (auteur)

- **Couverture spec** : §3 MPD attrs+datetime → T1 ; §4 Clock → T2, `live_number_window` → T4 ; §5 dispatch+clock → T5, 404→NotFound → T3 (fetch_range) + T5 (capture tolerance), délégation → T5/T6 ; §6 tests → T1/T3/T4/T5. ✅
- **Pas de placeholder** : code complet à chaque step ; T1 met à jour la fixture `select.rs` (le compilateur pointe le literal exact) ; T5 énumère tous les call sites à mettre à jour.
- **Cohérence des types** : `Clock{System|Fixed}` (T2) utilisé en T5/T6 ; `live_number_window(repr, t, ast, tsb, now, margin, base)` identique T4/T5 ; `Mpd.availability_start_time`/`time_shift_buffer_depth` (T1) lus en T4(via T5)/T5 ; `CoreError::NotFound` (T3) toléré en T5 ; `track_plan`/`download_dash_live(.. clock)` signature cohérente T5/T6/smoke.
- **Risque connu** : T5 touche le moteur 4b-2b (couvert par ses 6 tests timeline + smoke) ; le param `clock` ajouté en dernier minimise la churn des call sites.
