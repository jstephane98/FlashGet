# Phase 4c — Extracteur yt-dlp — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **GARDE-FOU GIT (obligatoire) :** chaque subagent DOIT exécuter `git checkout phase-4c-ytdlp` avant tout commit. Un subagent a déjà commité sur la mauvaise branche lors d'une phase précédente.

**Goal:** Permettre de coller l'URL d'une page vidéo (YouTube, etc.) dans « Ajouter une URL » et la télécharger via un sidecar yt-dlp, avec choix de qualité et pause/reprise.

**Architecture:** yt-dlp embarqué en sidecar Tauri (comme ffmpeg), piloté par une coquille mince côté `src-tauri` ; toute la logique parseable est en fonctions pures testées dans `download-core`. Réutilise `HlsVariantDialog`, l'événement `media://variants`, le `DownloadHandle` et le relais de progression de `launch()`. ffmpeg sidecar sert au muxage de yt-dlp.

**Tech Stack:** Rust (`download-core` + crate Tauri), `tauri-plugin-shell` (sidecar), React/TS + Vitest, yt-dlp.

**Spec :** `docs/superpowers/specs/2026-06-05-phase-4c-ytdlp-design.md`

---

## File Structure

- `src-tauri/crates/download-core/src/ytdlp.rs` — **créer** : module pur (parse formats/progress, sélection, build `-f`). Tests inclus.
- `src-tauri/crates/download-core/src/lib.rs` — **modifier** : `pub mod ytdlp;`.
- `src-tauri/crates/download-core/src/model.rs` — **modifier** : `DownloadKind::Ytdlp` + `ProbeResult.content_type`.
- `src-tauri/crates/download-core/src/storage.rs` — **modifier** : round-trip `"ytdlp"`.
- `src-tauri/crates/download-core/src/http.rs` — **modifier** : renseigner `content_type` + helper `is_html`.
- 5 sites stub `ProbeResult` (dash/live, dash/engine, hls/live, hls/engine ×2) + `tests/fake_fetcher.rs` — **modifier** : ajouter `content_type: None`.
- `src-tauri/src/ytdlp.rs` — **créer** : runner sidecar (`probe_formats`, `run_download`).
- `src-tauri/src/lib.rs` — **modifier** : `mod ytdlp;`.
- `src-tauri/tauri.conf.json` — **modifier** : `externalBin` += `bin/yt-dlp`.
- `src-tauri/src/state.rs` — **modifier** : `MediaProbe::Ytdlp(Vec<String>)`.
- `src-tauri/src/commands.rs` — **modifier** : routage `add_download`, `probe_ytdlp_inner`, bras `add_media_download`, bras `launch`.
- `src/types.ts`, `src/components/DownloadRow.tsx`, `src/components/DownloadRow.test.tsx` — **modifier** : type + badge `ytdlp`.

---

## Task 1: Cœur — `parse_formats` (TDD)

**Files:**
- Create: `src-tauri/crates/download-core/src/ytdlp.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs`

- [ ] **Step 1: Déclarer le module**

Dans `src-tauri/crates/download-core/src/lib.rs`, ajouter la ligne `pub mod ytdlp;` à côté des autres `pub mod` (ex. après `pub mod naming;`).

- [ ] **Step 2: Écrire le module avec un test qui échoue**

Créer `src-tauri/crates/download-core/src/ytdlp.rs` :

```rust
//! Logique pure pour l'extracteur yt-dlp (sidecar piloté côté Tauri).
//! Parse la sortie `yt-dlp -J`, sélectionne les variantes, construit l'argument
//! `-f`, et parse les lignes de progression. Aucune dépendance Tauri/process.

use crate::CoreError;
use serde::Deserialize;

/// Un format yt-dlp pertinent extrait de `yt-dlp -J`.
#[derive(Debug, Clone, PartialEq)]
pub struct YtdlpFormat {
    pub format_id: String,
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub ext: Option<String>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub tbr: Option<f64>,
}

#[derive(Deserialize)]
struct RawInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    formats: Vec<RawFormat>,
}

#[derive(Deserialize)]
struct RawFormat {
    format_id: String,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    ext: Option<String>,
    #[serde(default)]
    vcodec: Option<String>,
    #[serde(default)]
    acodec: Option<String>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    filesize_approx: Option<u64>,
    #[serde(default)]
    tbr: Option<f64>,
}

/// Parse la sortie JSON de `yt-dlp -J` : titre + liste des formats.
pub fn parse_formats(json: &str) -> Result<(String, Vec<YtdlpFormat>), CoreError> {
    let info: RawInfo = serde_json::from_str(json)
        .map_err(|e| CoreError::Media(format!("sortie yt-dlp illisible: {e}")))?;
    let title = info.title.unwrap_or_default();
    let formats = info
        .formats
        .into_iter()
        .map(|f| YtdlpFormat {
            format_id: f.format_id,
            height: f.height,
            width: f.width,
            ext: f.ext,
            vcodec: f.vcodec,
            acodec: f.acodec,
            filesize: f.filesize.or(f.filesize_approx),
            tbr: f.tbr,
        })
        .collect();
    Ok((title, formats))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "title": "Ma vidéo",
        "formats": [
            {"format_id":"18","height":360,"width":640,"ext":"mp4","vcodec":"avc1","acodec":"mp4a","filesize":1000,"tbr":500.0},
            {"format_id":"137","height":1080,"width":1920,"ext":"mp4","vcodec":"avc1","acodec":"none","filesize_approx":5000,"tbr":4000.0},
            {"format_id":"140","ext":"m4a","vcodec":"none","acodec":"mp4a","tbr":128.0}
        ]
    }"#;

    #[test]
    fn parse_formats_extracts_title_and_formats() {
        let (title, formats) = parse_formats(SAMPLE).unwrap();
        assert_eq!(title, "Ma vidéo");
        assert_eq!(formats.len(), 3);
        assert_eq!(formats[0].format_id, "18");
        assert_eq!(formats[0].height, Some(360));
        // filesize_approx pris quand filesize absent
        assert_eq!(formats[1].filesize, Some(5000));
        // audio-seul
        assert_eq!(formats[2].vcodec.as_deref(), Some("none"));
    }

    #[test]
    fn parse_formats_handles_missing_title_and_bad_json() {
        let (title, _) = parse_formats(r#"{"formats":[]}"#).unwrap();
        assert_eq!(title, "");
        assert!(parse_formats("pas du json").is_err());
    }
}
```

- [ ] **Step 3: Lancer le test pour vérifier l'échec puis le succès**

Run: `cd src-tauri/crates/download-core && cargo test ytdlp::tests::parse_formats`
Expected : compile puis PASS (2 tests). Si `serde_json` n'est pas déjà une dépendance de `download-core`, l'ajouter dans `src-tauri/crates/download-core/Cargo.toml` (`serde_json = "1"`) — vérifier d'abord avec `grep serde_json src-tauri/crates/download-core/Cargo.toml`.

- [ ] **Step 4: Vérifier la non-régression**

Run: `cd src-tauri/crates/download-core && cargo test`
Expected : tout vert, **0 warning**.

- [ ] **Step 5: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/crates/download-core/src/ytdlp.rs src-tauri/crates/download-core/src/lib.rs src-tauri/crates/download-core/Cargo.toml
git commit -m "feat(core): ytdlp::parse_formats (parse yt-dlp -J: titre + formats)"
```

---

## Task 2: Cœur — `select_variants` + `build_format_arg` (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/ytdlp.rs`

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter dans `mod tests` de `ytdlp.rs` :

```rust
    #[test]
    fn select_variants_keeps_video_dedups_sorts() {
        let (_t, formats) = parse_formats(SAMPLE).unwrap();
        let v = select_variants(&formats);
        // 18 (360p, vidéo) et 137 (1080p, vidéo) gardés ; 140 (audio-seul) écarté.
        assert_eq!(v.len(), 2);
        // tri par hauteur décroissante : 1080p d'abord.
        assert_eq!(v[0].0, "137");
        assert_eq!(v[0].1.resolution.as_deref(), Some("1920x1080"));
        assert_eq!(v[0].1.bandwidth, 4_000_000); // tbr 4000 kbit/s -> 4_000_000 bit/s
        assert_eq!(v[0].1.est_size, Some(5000));
        assert_eq!(v[1].0, "18");
    }

    #[test]
    fn select_variants_dedups_same_height() {
        let (_t, mut formats) = parse_formats(SAMPLE).unwrap();
        // doublon 1080p : ne garder qu'une variante par hauteur.
        formats.push(YtdlpFormat {
            format_id: "299".into(), height: Some(1080), width: Some(1920),
            ext: Some("mp4".into()), vcodec: Some("avc1".into()), acodec: Some("none".into()),
            filesize: Some(6000), tbr: Some(4500.0),
        });
        let v = select_variants(&formats);
        let n_1080 = v.iter().filter(|(_, mv)| mv.resolution.as_deref() == Some("1920x1080")).count();
        assert_eq!(n_1080, 1);
    }

    #[test]
    fn build_format_arg_merges_audio() {
        assert_eq!(build_format_arg("137"), "137+bestaudio/137/best");
    }
```

- [ ] **Step 2: Lancer pour vérifier l'échec**

Run: `cd src-tauri/crates/download-core && cargo test ytdlp::tests::select_variants`
Expected: FAIL — `cannot find function select_variants`.

- [ ] **Step 3: Implémenter**

Ajouter dans `ytdlp.rs` (après `parse_formats`) :

```rust
/// Variante neutre côté cœur (mappée vers `MediaVariant` par la couche Tauri).
#[derive(Debug, Clone, PartialEq)]
pub struct YtdlpVariant {
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

/// Variantes sélectionnables : formats porteurs de vidéo (vcodec présent et != "none"),
/// dédupliqués par hauteur, triés par hauteur décroissante. Renvoie le format_id
/// associé à chaque variante (à mémoriser pour le téléchargement).
pub fn select_variants(formats: &[YtdlpFormat]) -> Vec<(String, YtdlpVariant)> {
    let mut seen_heights: Vec<u32> = Vec::new();
    let mut out: Vec<(String, YtdlpVariant)> = Vec::new();

    let mut video: Vec<&YtdlpFormat> = formats
        .iter()
        .filter(|f| f.vcodec.as_deref().map(|c| c != "none").unwrap_or(false))
        .collect();
    // tri par hauteur décroissante (None en dernier).
    video.sort_by(|a, b| b.height.unwrap_or(0).cmp(&a.height.unwrap_or(0)));

    for f in video {
        if let Some(h) = f.height {
            if seen_heights.contains(&h) {
                continue;
            }
            seen_heights.push(h);
        }
        let resolution = match (f.width, f.height) {
            (Some(w), Some(h)) => Some(format!("{w}x{h}")),
            (None, Some(h)) => Some(format!("{h}p")),
            _ => None,
        };
        let bandwidth = f.tbr.map(|t| (t * 1000.0) as u64).unwrap_or(0);
        out.push((
            f.format_id.clone(),
            YtdlpVariant { resolution, bandwidth, est_size: f.filesize },
        ));
    }
    out
}

/// Argument `-f` : fusionne l'audio si le format est vidéo-seul, avec repli.
pub fn build_format_arg(format_id: &str) -> String {
    format!("{format_id}+bestaudio/{format_id}/best")
}
```

- [ ] **Step 4: Lancer pour vérifier le succès + non-régression**

Run: `cd src-tauri/crates/download-core && cargo test ytdlp` puis `cargo test`
Expected: PASS, **0 warning**.

- [ ] **Step 5: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/crates/download-core/src/ytdlp.rs
git commit -m "feat(core): ytdlp::select_variants + build_format_arg"
```

---

## Task 3: Cœur — `parse_progress` (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/ytdlp.rs`

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter dans `mod tests` :

```rust
    #[test]
    fn parse_progress_reads_sentinel_line() {
        assert_eq!(parse_progress("FGPROG:1048576/5242880"), Some((1048576, Some(5242880))));
    }

    #[test]
    fn parse_progress_total_unknown() {
        // yt-dlp émet "NA" quand le total est inconnu.
        assert_eq!(parse_progress("FGPROG:1048576/NA"), Some((1048576, None)));
        assert_eq!(parse_progress("FGPROG:2048/"), Some((2048, None)));
    }

    #[test]
    fn parse_progress_ignores_other_lines() {
        assert_eq!(parse_progress("[download] Destination: x.mp4"), None);
        assert_eq!(parse_progress("FGPROG:bad"), None);
    }
```

- [ ] **Step 2: Lancer pour vérifier l'échec**

Run: `cd src-tauri/crates/download-core && cargo test ytdlp::tests::parse_progress`
Expected: FAIL — `cannot find function parse_progress`.

- [ ] **Step 3: Implémenter**

Ajouter dans `ytdlp.rs` :

```rust
/// Préfixe de progression émis via `--progress-template` (voir couche Tauri).
pub const PROGRESS_PREFIX: &str = "FGPROG:";

/// Parse une ligne de progression `FGPROG:<downloaded>/<total>`.
/// `<total>` peut être `NA` ou vide (total inconnu → `None`).
/// Renvoie `None` pour toute autre ligne.
pub fn parse_progress(line: &str) -> Option<(u64, Option<u64>)> {
    let rest = line.trim().strip_prefix(PROGRESS_PREFIX)?;
    let (dl, total) = rest.split_once('/')?;
    let downloaded: u64 = dl.trim().parse().ok()?;
    let total = total.trim().parse::<u64>().ok();
    Some((downloaded, total))
}
```

- [ ] **Step 4: Lancer pour vérifier le succès + non-régression**

Run: `cd src-tauri/crates/download-core && cargo test ytdlp` puis `cargo test`
Expected: PASS, **0 warning**.

- [ ] **Step 5: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/crates/download-core/src/ytdlp.rs
git commit -m "feat(core): ytdlp::parse_progress (ligne sentinelle FGPROG)"
```

---

## Task 4: Cœur — `DownloadKind::Ytdlp` (modèle + storage)

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs` (enum `DownloadKind`)
- Modify: `src-tauri/crates/download-core/src/storage.rs` (`kind_to_str`/`kind_from_str` + test)

- [ ] **Step 1: Ajouter le variant au modèle**

Dans `model.rs`, l'enum `DownloadKind` devient :

```rust
pub enum DownloadKind {
    #[default]
    Regular,
    Hls,
    Dash,
    Ytdlp,
}
```

- [ ] **Step 2: Round-trip storage**

Dans `storage.rs`, `kind_to_str` :

```rust
fn kind_to_str(k: DownloadKind) -> &'static str {
    match k {
        DownloadKind::Regular => "regular",
        DownloadKind::Hls => "hls",
        DownloadKind::Dash => "dash",
        DownloadKind::Ytdlp => "ytdlp",
    }
}
```

et `kind_from_str` :

```rust
fn kind_from_str(s: &str) -> DownloadKind {
    match s {
        "hls" => DownloadKind::Hls,
        "dash" => DownloadKind::Dash,
        "ytdlp" => DownloadKind::Ytdlp,
        _ => DownloadKind::Regular,
    }
}
```

- [ ] **Step 3: Test round-trip**

Dans le `mod tests` de `storage.rs`, à côté des tests `kind` existants (vers la ligne 635-666), ajouter :

```rust
    #[test]
    fn kind_ytdlp_roundtrips() {
        let (s, mut d) = (test_storage(), sample_download());
        d.kind = DownloadKind::Ytdlp;
        s.upsert(&d).unwrap();
        let got = s.get_by_id(&d.id).unwrap().unwrap();
        assert_eq!(got.kind, DownloadKind::Ytdlp);
    }
```

> Note : adapter `test_storage()` / `sample_download()` aux helpers réellement utilisés par les tests voisins (lire les tests `kind` existants vers la ligne 630-666 pour copier le même style de construction).

- [ ] **Step 4: Vérifier**

Run: `cd src-tauri/crates/download-core && cargo test` puis `cd src-tauri && cargo test`
Expected: PASS, **0 warning** (le `match` exhaustif force la mise à jour partout ; corriger tout `match kind` non exhaustif signalé par le compilateur dans le cœur).

- [ ] **Step 5: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/crates/download-core/src/model.rs src-tauri/crates/download-core/src/storage.rs
git commit -m "feat(core): DownloadKind::Ytdlp (modèle + round-trip storage)"
```

---

## Task 5: Cœur — `ProbeResult.content_type` + `http::is_html` (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs` (`ProbeResult`)
- Modify: `src-tauri/crates/download-core/src/http.rs` (renseigne le champ + `is_html`)
- Modify: 5 sites stub + `tests/fake_fetcher.rs`

- [ ] **Step 1: Écrire le test `is_html` qui échoue**

Dans `http.rs`, dans le `#[cfg(test)] mod tests` (à côté de `extracts_file_name_from_url`), ajouter :

```rust
    #[test]
    fn is_html_detects_html_content_type() {
        assert!(is_html(&Some("text/html".to_string())));
        assert!(is_html(&Some("text/html; charset=utf-8".to_string())));
        assert!(is_html(&Some("  TEXT/HTML ".to_string())));
        assert!(!is_html(&Some("video/mp4".to_string())));
        assert!(!is_html(&Some("application/x-mpegurl".to_string())));
        assert!(!is_html(&None));
    }
```

- [ ] **Step 2: Lancer pour vérifier l'échec**

Run: `cd src-tauri/crates/download-core && cargo test is_html`
Expected: FAIL — `cannot find function is_html` + erreurs de champ manquant.

- [ ] **Step 3: Ajouter le champ au modèle**

Dans `model.rs`, `ProbeResult` devient :

```rust
pub struct ProbeResult {
    pub total_bytes: Option<u64>,
    pub supports_ranges: bool,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
}
```

- [ ] **Step 4: Renseigner le champ dans `http.rs` + ajouter `is_html`**

Dans `http.rs`, juste après `let headers = resp.headers().clone();` (vers la ligne 71), lire le content-type une fois (les `headers` sont déjà clonés) :

```rust
        let content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
```

Puis la construction finale (ligne 100) devient :

```rust
        Ok(ProbeResult { total_bytes, supports_ranges, file_name, content_type })
```

Ajouter la fonction publique (niveau module, hors `impl`) :

```rust
/// Vrai si le content-type désigne une page HTML (insensible casse/espaces).
pub fn is_html(content_type: &Option<String>) -> bool {
    content_type
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase().starts_with("text/html"))
        .unwrap_or(false)
}
```

- [ ] **Step 5: Mettre à jour les 5 sites stub identiques**

Remplacer (replace_all) l'expression exacte :

```rust
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
```

par :

```rust
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None, content_type: None })
```

dans : `dash/live.rs:254`, `dash/engine.rs:225`, `hls/live.rs:231`, `hls/engine.rs:255`, `hls/engine.rs:321`.

- [ ] **Step 6: Mettre à jour `tests/fake_fetcher.rs`**

Le bloc `ProbeResult { … file_name: Some("test.bin".to_string()) }` (ligne 28-36) doit ajouter `content_type: None,` après `file_name: Some(...)`.

- [ ] **Step 7: Vérifier**

Run: `cd src-tauri/crates/download-core && cargo test` puis `cd src-tauri && cargo test`
Expected: tout vert (le nouveau test `is_html` inclus), **0 warning**.

- [ ] **Step 8: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/crates/download-core/src
git commit -m "feat(core): ProbeResult.content_type + http::is_html"
```

---

## Task 6: Tauri — runner sidecar `src-tauri/src/ytdlp.rs`

**Files:**
- Create: `src-tauri/src/ytdlp.rs`
- Modify: `src-tauri/src/lib.rs` (`mod ytdlp;`)
- Modify: `src-tauri/tauri.conf.json` (`externalBin`)

> Pas de test unitaire (process/sidecar, comme `SidecarRemuxer`) : vérification par `cargo build` ; un smoke `#[ignore]` arrive en Task 10.

- [ ] **Step 1: Déclarer le sidecar**

Dans `src-tauri/tauri.conf.json`, remplacer :

```json
    "externalBin": ["bin/ffmpeg"],
```

par :

```json
    "externalBin": ["bin/ffmpeg", "bin/yt-dlp"],
```

(L'utilisateur place `src-tauri/bin/yt-dlp-<target-triple>.exe`, comme ffmpeg.)

- [ ] **Step 2: Déclarer le module**

Dans `src-tauri/src/lib.rs`, ajouter `mod ytdlp;` à côté de `mod remux;`.

- [ ] **Step 3: Écrire le runner**

Créer `src-tauri/src/ytdlp.rs` :

```rust
//! Coquille Tauri pour le sidecar yt-dlp : sonde des formats + téléchargement avec
//! progression et pause/reprise/annulation. La logique parseable vit dans
//! `download_core::ytdlp`.

use download_core::engine::DownloadHandle;
use download_core::model::{DownloadStatus, Progress, RequestHeaders};
use download_core::CoreError;
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc::UnboundedSender;

/// Chemin du binaire ffmpeg sidecar (à côté de l'exécutable), pour `--ffmpeg-location`.
/// Cherche un fichier dont le nom commence par "ffmpeg" (gère `ffmpeg.exe` bundlé ET
/// `ffmpeg-<triple>.exe` en dev).
fn ffmpeg_location() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("ffmpeg") && (name.ends_with(".exe") || !name.contains('.')) {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }
    None
}

fn header_args(headers: &RequestHeaders) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(ua) = &headers.user_agent {
        a.push("--user-agent".into());
        a.push(ua.clone());
    }
    if let Some(r) = &headers.referrer {
        a.push("--add-header".into());
        a.push(format!("Referer:{r}"));
    }
    if let Some(c) = &headers.cookie {
        a.push("--add-header".into());
        a.push(format!("Cookie:{c}"));
    }
    a
}

/// Sonde les formats d'une URL via `yt-dlp -J`. Renvoie (titre, formats).
pub async fn probe_formats(
    app: &AppHandle,
    url: &str,
    headers: &RequestHeaders,
) -> Result<(String, Vec<download_core::ytdlp::YtdlpFormat>), String> {
    let mut args: Vec<String> = vec![
        "-J".into(),
        "--no-playlist".into(),
        "--no-warnings".into(),
    ];
    args.extend(header_args(headers));
    args.push(url.to_string());

    let cmd = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| format!("yt-dlp introuvable: {e}"))?;
    let output = cmd
        .args(args)
        .output()
        .await
        .map_err(|e| format!("échec exécution yt-dlp: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ");
        return Err(format!("yt-dlp -J a échoué: {tail}"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    download_core::ytdlp::parse_formats(&stdout).map_err(|e| e.to_string())
}

/// Télécharge via yt-dlp avec le format choisi, en émettant la progression.
/// Pause → tue le process en gardant le partiel ; annulation → tue + supprime ;
/// succès → émet Completed.
pub async fn run_download(
    app: AppHandle,
    id: &str,
    url: &str,
    format_id: &str,
    dest: &Path,
    handle: DownloadHandle,
    headers: &RequestHeaders,
    tx: UnboundedSender<Progress>,
) -> Result<(), CoreError> {
    let mut args: Vec<String> = vec![
        "-f".into(),
        download_core::ytdlp::build_format_arg(format_id),
        "--no-playlist".into(),
        "--newline".into(),
        "--continue".into(),
        "--progress-template".into(),
        format!("{}%(progress.downloaded_bytes)s/%(progress.total_bytes)s", download_core::ytdlp::PROGRESS_PREFIX),
        "-o".into(),
        dest.to_string_lossy().to_string(),
    ];
    if let Some(ff) = ffmpeg_location() {
        args.push("--ffmpeg-location".into());
        args.push(ff);
    }
    args.extend(header_args(headers));
    args.push(url.to_string());

    let cmd = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| CoreError::Media(format!("yt-dlp introuvable: {e}")))?;
    let (mut rx, child) = cmd
        .args(args)
        .spawn()
        .map_err(|e| CoreError::Media(format!("échec lancement yt-dlp: {e}")))?;
    let mut child = Some(child);

    let mut last: u64 = 0;
    let mut total: Option<u64> = None;
    let mut stderr_tail: Vec<String> = Vec::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(300));

    let mk = |dl, total, status| Progress {
        id: id.to_string(),
        downloaded_bytes: dl,
        total_bytes: total,
        speed_bps: 0,
        status,
        segments_done: None,
        segments_total: None,
    };

    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Some(CommandEvent::Stdout(bytes)) => {
                        let line = String::from_utf8_lossy(&bytes);
                        if let Some((dl, t)) = download_core::ytdlp::parse_progress(&line) {
                            handle.add_downloaded(dl.saturating_sub(last)); // compteur absolu -> delta
                            last = dl;
                            if t.is_some() { total = t; }
                            let _ = tx.send(mk(dl, total, DownloadStatus::Downloading));
                        }
                    }
                    Some(CommandEvent::Stderr(bytes)) => {
                        let l = String::from_utf8_lossy(&bytes).to_string();
                        stderr_tail.push(l);
                        if stderr_tail.len() > 10 { stderr_tail.remove(0); }
                    }
                    Some(CommandEvent::Terminated(payload)) => {
                        let code = payload.code.unwrap_or(-1);
                        if code == 0 {
                            let _ = tx.send(mk(last, total, DownloadStatus::Completed));
                            return Ok(());
                        }
                        let tail = stderr_tail.join("\n");
                        return Err(CoreError::Media(format!("yt-dlp a échoué (code {code}): {tail}")));
                    }
                    Some(_) => {}
                    None => return Ok(()),
                }
            }
            _ = ticker.tick() => {
                if handle.is_cancelled() {
                    if let Some(c) = child.take() { let _ = c.kill(); }
                    let _ = std::fs::remove_file(dest);
                    let _ = std::fs::remove_file(format!("{}.part", dest.to_string_lossy()));
                    return Ok(());
                }
                if handle.is_paused() {
                    if let Some(c) = child.take() { let _ = c.kill(); }
                    let _ = tx.send(mk(last, total, DownloadStatus::Paused));
                    return Ok(());
                }
            }
        }
    }
}
```

> `DownloadHandle::add_downloaded(&self, n: u64)` est la méthode réelle (incrémente le compteur ; `engine.rs:49`). Comme `parse_progress` renvoie un cumul absolu, on ajoute le delta `dl - last` (déjà fait dans le code ci-dessus).

- [ ] **Step 4: Vérifier la compilation**

Run: `cd src-tauri && cargo build`
Expected: compile sans erreur. (Si `CommandEvent`/`spawn`/`kill` diffèrent de l'API de la version de `tauri-plugin-shell` du projet, ajuster d'après la doc de la version verrouillée dans `Cargo.lock` — ne pas changer la version.)

- [ ] **Step 5: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/src/ytdlp.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat(tauri): runner sidecar yt-dlp (probe_formats + run_download)"
```

---

## Task 7: Tauri — routage `text/html` → flux yt-dlp

**Files:**
- Modify: `src-tauri/src/state.rs` (`MediaProbe::Ytdlp`)
- Modify: `src-tauri/src/commands.rs` (`add_download` + `probe_ytdlp_inner`)

- [ ] **Step 1: Ajouter le variant de probe**

Dans `state.rs`, l'enum `MediaProbe` devient :

```rust
pub enum MediaProbe {
    Hls(download_core::hls::playlist::MasterPlaylist),
    Dash(Mpd),
    /// Liste des format_id yt-dlp, parallèle aux index de variantes.
    Ytdlp(Vec<String>),
}
```

- [ ] **Step 2: Router dans `add_download`**

Dans `commands.rs`, remplacer la commande `add_download` (lignes ~79-82) :

```rust
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    add_url(&app, &args.url, &args.dest_dir, RequestHeaders::default(), None).await
}
```

par une version qui sonde d'abord et route les pages html vers yt-dlp :

```rust
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    let headers = RequestHeaders::default();
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(&args.url, &headers))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;

    if download_core::http::is_html(&probe.content_type) {
        // Page web → tenter l'extraction yt-dlp (n'aboutit pas à un Download ici :
        // émet media://variants, l'utilisateur choisit, add_media_download confirme).
        probe_ytdlp(&app, &args.url, headers).await?;
        return Err("__media_pending__".to_string());
    }
    add_url_with_probe(&app, &args.url, &args.dest_dir, headers, None, Some(probe)).await
}
```

> Note d'intégration UX : `add_download` renvoyait un `Download`. Pour le flux yt-dlp il n'y a pas encore de download (l'utilisateur doit choisir une variante). On signale ce cas au frontend par l'erreur sentinelle `"__media_pending__"`, que `App.tsx` doit **ignorer silencieusement** (ne pas afficher de bannière) — la boîte `media://variants` prendra le relais. Modifier `App.tsx` en Task 9.

- [ ] **Step 3: Factoriser `add_url` pour réutiliser le probe**

Pour éviter une double sonde, ajouter une variante de `add_url` qui accepte un probe pré-calculé, et faire de l'`add_url` existant un simple wrapper. Remplacer la signature de `add_url` (ligne 23) en ajoutant une fonction :

```rust
pub(crate) async fn add_url(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<Download, String> {
    add_url_with_probe(app, url, dest_dir, headers, suggested_name, None).await
}

pub(crate) async fn add_url_with_probe(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders, suggested_name: Option<String>, probe: Option<download_core::model::ProbeResult>) -> Result<Download, String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = match probe {
        Some(p) => p,
        None => tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(url, &headers))
            .await
            .map_err(|_| "timeout while probing URL".to_string())?
            .map_err(|e| e.to_string())?,
    };
    // ... (le reste du corps actuel de add_url, inchangé, à partir de `let file_name = ...`)
}
```

(Déplacer tout le corps existant de `add_url` — de `let file_name = download_core::naming::choose_file_name(...)` jusqu'au `Ok(download)` — dans `add_url_with_probe`, en supprimant l'ancienne sonde interne désormais portée par le paramètre.)

- [ ] **Step 4: Écrire `probe_ytdlp`**

Ajouter dans `commands.rs` (à côté de `probe_dash`) :

```rust
/// Sonde une page via yt-dlp ; émet media://variants ou media://error.
pub(crate) async fn probe_ytdlp(app: &AppHandle, url: &str, headers: RequestHeaders) -> Result<(), String> {
    let result = probe_ytdlp_inner(app, url, headers).await;
    if let Err(e) = &result {
        let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
        if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    }
    result
}

async fn probe_ytdlp_inner(app: &AppHandle, url: &str, headers: RequestHeaders) -> Result<(), String> {
    let (title, formats) = crate::ytdlp::probe_formats(app, url, &headers).await?;
    let selected = download_core::ytdlp::select_variants(&formats);
    if selected.is_empty() {
        return Err("Aucune vidéo trouvée sur cette page".into());
    }
    let format_ids: Vec<String> = selected.iter().map(|(id, _)| id.clone()).collect();
    let variants: Vec<MediaVariant> = selected
        .iter()
        .enumerate()
        .map(|(i, (_, v))| MediaVariant {
            index: i as u32,
            resolution: v.resolution.clone(),
            bandwidth: v.bandwidth,
            est_size: v.est_size,
        })
        .collect();

    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = download_core::naming::sanitize_title(Some(&title));
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let ttl = std::time::Duration::from_secs(30 * 60);
        pending.retain(|_, pm| pm.created.elapsed() < ttl);
        pending.insert(probe_id.clone(), PendingMedia {
            probe: MediaProbe::Ytdlp(format_ids),
            headers,
            file_name: file_name.clone(),
            source_url: url.to_string(),
            created: std::time::Instant::now(),
        });
    }
    let _ = app.emit("media://variants", MediaVariantsEvent { probe_id, file_name, variants });
    if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    Ok(())
}
```

- [ ] **Step 5: Vérifier la compilation + non-régression**

Run: `cd src-tauri && cargo test`
Expected: compile + tauri lib 9 + download-core verts, **0 warning**.

- [ ] **Step 6: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/src/state.rs src-tauri/src/commands.rs
git commit -m "feat(tauri): routage page text/html -> probe yt-dlp (media://variants)"
```

---

## Task 8: Tauri — confirmation + dispatch yt-dlp

**Files:**
- Modify: `src-tauri/src/commands.rs` (`add_media_download` + `launch`)

- [ ] **Step 1: Bras Ytdlp dans `add_media_download`**

Dans `add_media_download`, l'`enum Built` (ligne 259) devient :

```rust
    enum Built { Hls { url: String, audio_url: Option<String> }, Dash { mpd_url: String, selection: String }, Ytdlp { url: String, format_id: String } }
```

Dans le `match &pm.probe` (lignes 267-279), ajouter le bras :

```rust
            MediaProbe::Ytdlp(format_ids) => {
                let format_id = format_ids.get(variant_index as usize).ok_or_else(|| "variante invalide".to_string())?.clone();
                Built::Ytdlp { url: pm.source_url.clone(), format_id }
            }
```

Dans le `match built` (lignes 298-301), ajouter :

```rust
        Built::Ytdlp { url, format_id } => (DownloadKind::Ytdlp, url, None, Some(format_id)),
```

(Le tuple est `(kind, url, audio_url, media_selection)` ; pour yt-dlp `media_selection` = le format_id brut, pas du JSON.)

- [ ] **Step 2: Bras Ytdlp dans `launch`**

Dans `launch` (le `match kind`, lignes 669-700), ajouter après le bras `Dash` :

```rust
            download_core::model::DownloadKind::Ytdlp => {
                let format_id = media_selection.clone().unwrap_or_default();
                crate::ytdlp::run_download(app_for_dl.clone(), &id_for_dl, &url, &format_id, &dest, handle, &headers, tx).await
            }
```

- [ ] **Step 3: Vérifier**

Run: `cd src-tauri && cargo test`
Expected: compile + tout vert, **0 warning**. (Le `match` de `add_media_download` et celui de `launch` sont désormais exhaustifs.)

- [ ] **Step 4: Commit**

```bash
git checkout phase-4c-ytdlp
git add src-tauri/src/commands.rs
git commit -m "feat(tauri): add_media_download + launch gèrent DownloadKind::Ytdlp"
```

---

## Task 9: Frontend — type, badge, sentinelle media-pending (TDD)

**Files:**
- Modify: `src/types.ts`
- Modify: `src/components/DownloadRow.tsx`
- Modify: `src/components/DownloadRow.test.tsx`
- Modify: `src/App.tsx` (ignorer l'erreur sentinelle)

- [ ] **Step 1: Écrire le test de badge qui échoue**

Dans `src/components/DownloadRow.test.tsx`, ajouter un test (en réutilisant `renderRow`/`makeDownload` existants) :

```tsx
  it("affiche le badge yt-dlp pour kind ytdlp", () => {
    renderRow({ kind: "ytdlp" });
    expect(screen.getByText("yt-dlp")).toBeTruthy();
  });
```

- [ ] **Step 2: Lancer pour vérifier l'échec**

Run: `npm test -- DownloadRow`
Expected: FAIL — le texte "yt-dlp" est introuvable.

- [ ] **Step 3: Étendre le type**

Dans `src/types.ts`, trouver le type `kind` / `DownloadKind` (valeurs `"regular" | "hls" | "dash"`) et ajouter `"ytdlp"`.

- [ ] **Step 4: Ajouter le badge**

Dans `src/components/DownloadRow.tsx`, repérer le rendu des badges HLS/DASH existants (chercher `"HLS"`/`"DASH"`) et ajouter, sur le même modèle, un badge pour `d.kind === "ytdlp"` affichant le texte `yt-dlp` (réutiliser les classes Tailwind du badge voisin pour rester cohérent).

- [ ] **Step 5: Ignorer la sentinelle media-pending dans `App.tsx`**

Le flux yt-dlp via « Ajouter une URL » fait que `add_download` rejette avec `"__media_pending__"` (la boîte de variantes prend le relais). Là où `App.tsx` appelle `api.addDownload(...)` (chercher `addDownload`), entourer d'un `catch` qui **n'affiche pas** d'erreur si le message contient `"__media_pending__"` :

```tsx
    try {
      await api.addDownload(url, destDir);
      await refresh();
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("__media_pending__")) {
        setError(`Impossible d'ajouter : ${msg}`);
      }
    }
```

(Adapter aux noms réels des variables/fonctions du composant ; le point clé est le filtrage de la sentinelle.)

- [ ] **Step 6: Lancer pour vérifier le succès + non-régression + build**

Run: `npm test -- DownloadRow` puis `npm test` puis `npm run build`
Expected: PASS (nouveau test inclus), build tsc+vite OK.

- [ ] **Step 7: Commit**

```bash
git checkout phase-4c-ytdlp
git add src/types.ts src/components/DownloadRow.tsx src/components/DownloadRow.test.tsx src/App.tsx
git commit -m "feat(frontend): kind ytdlp (badge) + ignore sentinelle media-pending"
```

---

## Task 10: Vérification finale + smoke `#[ignore]`

**Files:**
- Create/Modify: `src-tauri/crates/download-core/tests/` (optionnel) ou test côté Tauri pour le smoke.

> Le smoke réel exige le binaire yt-dlp + réseau ; il est `#[ignore]` (comme les smoke ffmpeg). Comme le sidecar n'est invocable que via `AppHandle` (Tauri), un vrai smoke automatisé est lourd : on se limite à **documenter** la vérification manuelle et à confirmer les suites automatiques.

- [ ] **Step 1: Suites complètes**

Run: `cd src-tauri/crates/download-core && cargo test` ; `cd src-tauri && cargo test` ; `cd c:/Users/steph/Documents/H/idm-clone && npm test && npm run build`
Expected: tout vert, **0 warning rustc**.

- [ ] **Step 2: Smoke manuel (note pour le réviseur)**

Placer `src-tauri/bin/yt-dlp-<target-triple>.exe`, lancer l'app (`npm run tauri dev`), coller une URL YouTube dans « Ajouter une URL » : vérifier que la boîte de variantes s'ouvre (nom pré-rempli = titre), choisir une qualité, vérifier la progression puis le fichier final lisible. Tester pause→reprise (`--continue`) et annulation. Tester une URL de page sans vidéo (→ « Aucune vidéo trouvée sur cette page »).

- [ ] **Step 3: Commit éventuel**

```bash
git checkout phase-4c-ytdlp
git add -A
git commit -m "chore(phase-4c): vérification finale extracteur yt-dlp"
```

---

## Notes d'implémentation

- **DRY :** parseurs uniques dans `download_core::ytdlp` ; `add_url_with_probe` évite la double sonde.
- **YAGNI :** pas de playlists, pas d'auto-update yt-dlp, pas de sous-titres, pas de limiteur sur yt-dlp.
- **Mapping Tauri :** la commande `add_download` existe déjà (front inchangé pour l'entrée) ; `add_media_download` reçoit déjà `fileName` (Task 4b-3).
- **Garde git :** `git checkout phase-4c-ytdlp` avant chaque commit de subagent.
- **Vérifs d'API à faire par l'implémenteur (ne pas inventer) :** nom exact de la méthode compteur de `DownloadHandle` (`engine.rs` ~L50) ; API `tauri-plugin-shell` (`CommandEvent`, `spawn`, `CommandChild::kill`) selon la version de `Cargo.lock` ; présence de `serde_json` dans `download-core/Cargo.toml`.
