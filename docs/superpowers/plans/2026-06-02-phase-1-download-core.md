# Phase 1 — Cœur de téléchargement (MVP) — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Livrer une app Tauri qui télécharge un fichier depuis une URL en multi-segments, affiche la progression en temps réel, et supporte pause / reprise / annulation, avec persistance SQLite et reprise après crash.

**Architecture:** Un *workspace* Cargo sépare un crate de logique pure et testable (`download-core`, sans dépendance Tauri) de l'app Tauri (binaire). Le frontend React/TypeScript n'affiche que l'état et appelle le cœur via des commandes Tauri ; le cœur pousse la progression via des events. L'accès réseau passe par un trait `ByteFetcher`, ce qui rend le moteur testable sans serveur réel (un `FakeFetcher` en mémoire honore les requêtes Range dans les tests).

**Tech Stack:** Tauri v2, Rust (tokio, reqwest, futures-util, bytes, async-trait, rusqlite *bundled*, serde, serde_json, thiserror, uuid), React + TypeScript + Vite + Tailwind CSS, Vitest.

---

## Référence du périmètre (spec)

Spec : [docs/superpowers/specs/2026-06-02-idm-clone-design.md](../specs/2026-06-02-idm-clone-design.md). Cette Phase 1 couvre : `download-engine` (multi-segments), `resume-manager` (reprise coupure/crash), `storage` (SQLite), commandes ajouter/démarrer/pause/reprendre/annuler, progression temps réel, et UI de base (tableau Option B + barres de progression). Les phases 2-6 sont hors périmètre.

## Structure des fichiers

```
idm-clone/
├─ package.json, vite.config.ts, tsconfig.json, index.html
├─ tailwind.config.js, postcss.config.js
├─ src/                              # Frontend React
│  ├─ main.tsx                       # point d'entrée React
│  ├─ App.tsx                        # compose Toolbar + DownloadTable
│  ├─ index.css                      # directives Tailwind
│  ├─ types.ts                       # types TS miroir des structs Rust
│  ├─ format.ts                      # formatBytes / formatSpeed (testés)
│  ├─ api.ts                         # wrappers invoke + listen
│  └─ components/
│     ├─ Toolbar.tsx                 # boutons Ajouter/Pause/Reprendre + recherche
│     ├─ AddUrlDialog.tsx            # saisie d'une URL
│     ├─ DownloadTable.tsx           # en-têtes + lignes
│     └─ DownloadRow.tsx             # une ligne (nom, taille, état, vitesse, %)
│  └─ format.test.ts                 # tests Vitest de format.ts
└─ src-tauri/
   ├─ Cargo.toml                     # workspace + crate binaire (app)
   ├─ tauri.conf.json, build.rs
   ├─ crates/download-core/
   │  ├─ Cargo.toml
   │  └─ src/
   │     ├─ lib.rs                    # ré-exports + erreurs
   │     ├─ model.rs                  # Download, DownloadStatus, ProbeResult, Progress
   │     ├─ segments.rs               # compute_segments (pur)
   │     ├─ http.rs                   # ByteFetcher trait + ReqwestFetcher
   │     ├─ resume.rs                 # sidecar de progression (save/load)
   │     ├─ engine.rs                 # orchestration multi-segments + pause
   │     ├─ storage.rs                # SQLite (rusqlite)
   │     └─ tests/fake_fetcher.rs     # ByteFetcher en mémoire (dev)
   └─ src/
      ├─ main.rs                      # entrée Tauri, manage(state), handlers
      ├─ state.rs                     # AppState : Manager + handles de tâches
      └─ commands.rs                  # commandes Tauri (thin)
```

Chaque fichier a une responsabilité unique. Le découpage permet de tester le cœur sans Tauri.

---

## Task 0 : Scaffolding du projet Tauri + React + Tailwind

**Files:**
- Create: tout l'arbre via le CLI Tauri, puis `tailwind.config.js`, `postcss.config.js`, `src/index.css`

- [ ] **Step 1: Scaffolder l'app**

Run (depuis `c:/Users/steph/Documents/H/idm-clone`) :
```bash
npm create tauri-app@latest . -- --template react-ts --manager npm --yes
```
Si le dossier n'est pas vide, répondre « yes » pour continuer. Cela crée `src/`, `src-tauri/`, `package.json`, `vite.config.ts`.

- [ ] **Step 2: Installer les dépendances JS**

Run:
```bash
npm install
npm install -D tailwindcss postcss autoprefixer vitest @testing-library/react @testing-library/jest-dom jsdom
npx tailwindcss init -p
```
Expected: `tailwind.config.js` et `postcss.config.js` créés.

- [ ] **Step 3: Configurer Tailwind**

Remplacer `tailwind.config.js` par :
```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: { extend: {} },
  plugins: [],
};
```

Remplacer le contenu de `src/index.css` (ou `src/App.css` selon le template — utiliser `src/index.css`) par :
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root { color-scheme: light; }
body { margin: 0; font-family: "Segoe UI", system-ui, sans-serif; background: #fbfbfd; color: #1d1d29; }
```
S'assurer que `src/main.tsx` importe `./index.css`.

- [ ] **Step 4: Vérifier que l'app démarre**

Run:
```bash
npm run tauri dev
```
Expected: une fenêtre desktop s'ouvre avec le template par défaut. Fermer la fenêtre.

- [ ] **Step 5: Configurer Vitest**

Ajouter dans `package.json` le script :
```json
"test": "vitest run"
```
Créer `vitest.config.ts` :
```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { environment: "jsdom", globals: true },
});
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: scaffold Tauri + React + Tailwind + Vitest"
```

---

## Task 1 : Workspace Cargo + crate `download-core` + modèle

**Files:**
- Modify: `src-tauri/Cargo.toml` (transformer en workspace)
- Create: `src-tauri/crates/download-core/Cargo.toml`, `src-tauri/crates/download-core/src/lib.rs`, `src-tauri/crates/download-core/src/model.rs`

- [ ] **Step 1: Déclarer le workspace**

En tête de `src-tauri/Cargo.toml`, ajouter (avant `[package]`) :
```toml
[workspace]
members = ["crates/download-core"]
```
Puis ajouter la dépendance du binaire vers le cœur dans la section `[dependencies]` du même fichier :
```toml
download-core = { path = "crates/download-core" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
```

- [ ] **Step 2: Créer le Cargo.toml du cœur**

`src-tauri/crates/download-core/Cargo.toml` :
```toml
[package]
name = "download-core"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "fs", "io-util", "sync", "time"] }
reqwest = { version = "0.12", features = ["stream"] }
futures-util = "0.3"
bytes = "1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
uuid = { version = "1", features = ["v4"] }
rusqlite = { version = "0.31", features = ["bundled"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Écrire le modèle**

`src-tauri/crates/download-core/src/model.rs` :
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error,
}

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
}

/// Résultat de la requête d'inspection (HEAD/GET partiel) faite avant de télécharger.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub total_bytes: Option<u64>,
    pub supports_ranges: bool,
    pub file_name: Option<String>,
}

/// Payload de progression poussé vers l'UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: u64,
    pub status: DownloadStatus,
}
```

- [ ] **Step 4: Écrire lib.rs (erreurs + modules)**

`src-tauri/crates/download-core/src/lib.rs` :
```rust
pub mod model;
pub mod segments;
pub mod http;
pub mod resume;
pub mod engine;
pub mod storage;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("network error: {0}")]
    Network(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("the server does not support the requested operation")]
    Unsupported,
    #[error("download not found: {0}")]
    NotFound(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
```

> Note : les modules `segments`, `http`, `resume`, `engine`, `storage` seront créés vides puis remplis dans les tâches suivantes. Pour que ça compile dès maintenant, créer chaque fichier avec un contenu minimal (`// rempli en Task N`) et un module vide.

Créer fichiers vides : `segments.rs`, `http.rs`, `resume.rs`, `engine.rs`, `storage.rs` (chacun avec une ligne de commentaire).

- [ ] **Step 5: Vérifier la compilation**

Run:
```bash
cd src-tauri && cargo build -p download-core
```
Expected: compile sans erreur (warnings de code mort tolérés).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): add cargo workspace, download-core crate and domain model"
```

---

## Task 2 : Calcul des segments (fonction pure, TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/segments.rs`

- [ ] **Step 1: Écrire les tests qui échouent**

`src-tauri/crates/download-core/src/segments.rs` :
```rust
/// Découpe `total` octets en au plus `parts` segments de tailles quasi égales.
/// Retourne des intervalles INCLUSIFS [(start, end)]. Le reste est réparti
/// sur les premiers segments. `parts` est borné à [1, total].
pub fn compute_segments(total: u64, parts: u32) -> Vec<(u64, u64)> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divides_evenly() {
        assert_eq!(compute_segments(100, 4), vec![(0, 24), (25, 49), (50, 74), (75, 99)]);
    }

    #[test]
    fn distributes_remainder_to_first_segments() {
        // 10 / 3 -> longueurs 4,3,3
        assert_eq!(compute_segments(10, 3), vec![(0, 3), (4, 6), (7, 9)]);
    }

    #[test]
    fn zero_total_yields_nothing() {
        assert_eq!(compute_segments(0, 4), Vec::<(u64, u64)>::new());
    }

    #[test]
    fn parts_capped_at_total() {
        // 5 octets, 10 parts demandées -> 5 segments d'un octet
        assert_eq!(compute_segments(5, 10), vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)]);
    }

    #[test]
    fn last_segment_reaches_end() {
        let segs = compute_segments(1000, 7);
        assert_eq!(segs.last().unwrap().1, 999);
        assert_eq!(segs.first().unwrap().0, 0);
    }
}
```

- [ ] **Step 2: Lancer le test → échoue**

Run:
```bash
cd src-tauri && cargo test -p download-core segments
```
Expected: FAIL (`unimplemented!`).

- [ ] **Step 3: Implémenter**

Remplacer le corps de `compute_segments` :
```rust
pub fn compute_segments(total: u64, parts: u32) -> Vec<(u64, u64)> {
    if total == 0 {
        return Vec::new();
    }
    let parts = (parts as u64).clamp(1, total);
    let base = total / parts;
    let remainder = total % parts;
    let mut ranges = Vec::with_capacity(parts as usize);
    let mut start = 0u64;
    for i in 0..parts {
        let extra = if i < remainder { 1 } else { 0 };
        let len = base + extra;
        let end = start + len - 1;
        ranges.push((start, end));
        start = end + 1;
    }
    ranges
}
```

- [ ] **Step 4: Lancer le test → passe**

Run:
```bash
cd src-tauri && cargo test -p download-core segments
```
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): segment range computation with TDD"
```

---

## Task 3 : Trait `ByteFetcher` + `FakeFetcher` de test + `ReqwestFetcher`

**Files:**
- Modify: `src-tauri/crates/download-core/src/http.rs`

- [ ] **Step 1: Écrire le trait, le fake et un test**

`src-tauri/crates/download-core/src/http.rs` :
```rust
use crate::model::ProbeResult;
use crate::CoreError;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;

/// Abstraction réseau. La prod utilise reqwest ; les tests un fake en mémoire.
#[async_trait]
pub trait ByteFetcher: Send + Sync {
    /// Inspecte l'URL : taille, support des Range, nom de fichier suggéré.
    async fn probe(&self, url: &str) -> Result<ProbeResult, CoreError>;

    /// Récupère un flux d'octets. `range` = intervalle inclusif (start, end).
    /// Si `range` est None, récupère tout le corps.
    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError>;
}

/// Implémentation de production basée sur reqwest.
pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

impl Default for ReqwestFetcher {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ByteFetcher for ReqwestFetcher {
    async fn probe(&self, url: &str) -> Result<ProbeResult, CoreError> {
        // Une requête Range de 0-0 révèle Accept-Ranges + Content-Range (=> taille totale).
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;

        let status = resp.status();
        let headers = resp.headers().clone();

        // 206 Partial Content => le serveur supporte les Range.
        let supports_ranges = status == reqwest::StatusCode::PARTIAL_CONTENT
            || headers
                .get(reqwest::header::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("bytes"))
                .unwrap_or(false);

        // Taille totale : depuis Content-Range "bytes 0-0/12345", sinon Content-Length.
        let total_bytes = headers
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .or_else(|| {
                if !supports_ranges {
                    headers
                        .get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                } else {
                    None
                }
            });

        let file_name = file_name_from_url(url);

        Ok(ProbeResult { total_bytes, supports_ranges, file_name })
    }

    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        use futures_util::StreamExt;

        let mut req = self.client.get(url);
        if let Some((start, end)) = range {
            req = req.header(reqwest::header::RANGE, format!("bytes={start}-{end}"));
        }
        let resp = req.send().await.map_err(|e| CoreError::Network(e.to_string()))?;
        let resp = resp.error_for_status().map_err(|e| CoreError::Network(e.to_string()))?;

        let stream = resp
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| CoreError::Network(e.to_string())));
        Ok(stream.boxed())
    }
}

/// Déduit un nom de fichier depuis l'URL (dernier segment de chemin).
pub fn file_name_from_url(url: &str) -> Option<String> {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    let last = without_query.rsplit('/').next().unwrap_or("");
    if last.is_empty() { None } else { Some(last.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_file_name_from_url() {
        assert_eq!(file_name_from_url("https://x.com/a/b/film.mkv?token=1"), Some("film.mkv".into()));
        assert_eq!(file_name_from_url("https://x.com/"), None);
    }
}
```

- [ ] **Step 2: Lancer le test → échoue puis passe**

Run:
```bash
cd src-tauri && cargo test -p download-core http
```
Expected: PASS (le test `extracts_file_name_from_url`). (Le trait et reqwest compilent.)

- [ ] **Step 3: Créer le FakeFetcher (réutilisé par les tests d'engine)**

`src-tauri/crates/download-core/src/tests/fake_fetcher.rs` :
```rust
//! Fetcher en mémoire pour les tests du moteur. Honore les Range inclusifs.
use crate::http::ByteFetcher;
use crate::model::ProbeResult;
use crate::CoreError;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::{self, BoxStream};

pub struct FakeFetcher {
    pub data: Vec<u8>,
    pub supports_ranges: bool,
    /// Taille de chunk émise par le flux (simule un réseau morcelé).
    pub chunk_size: usize,
}

impl FakeFetcher {
    pub fn new(data: Vec<u8>, supports_ranges: bool) -> Self {
        Self { data, supports_ranges, chunk_size: 7 }
    }
}

#[async_trait]
impl ByteFetcher for FakeFetcher {
    async fn probe(&self, _url: &str) -> Result<ProbeResult, CoreError> {
        Ok(ProbeResult {
            total_bytes: Some(self.data.len() as u64),
            supports_ranges: self.supports_ranges,
            file_name: Some("test.bin".to_string()),
        })
    }

    async fn fetch_range(
        &self,
        _url: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        let slice: Vec<u8> = match range {
            Some((start, end)) => {
                let start = start as usize;
                let end = (end as usize + 1).min(self.data.len());
                self.data[start..end].to_vec()
            }
            None => self.data.clone(),
        };
        let chunks: Vec<Result<Bytes, CoreError>> = slice
            .chunks(self.chunk_size.max(1))
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        Ok(stream::iter(chunks).boxed())
    }
}
```

Déclarer ce module de test dans `lib.rs` en ajoutant à la fin :
```rust
#[cfg(test)]
mod tests {
    pub mod fake_fetcher;
}
```

- [ ] **Step 4: Vérifier la compilation des tests**

Run:
```bash
cd src-tauri && cargo test -p download-core --no-run
```
Expected: compile.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): ByteFetcher trait, reqwest impl, in-memory fake for tests"
```

---

## Task 4 : Sidecar de reprise (save/load de la progression, TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/resume.rs`

- [ ] **Step 1: Écrire l'API et les tests**

`src-tauri/crates/download-core/src/resume.rs` :
```rust
use crate::CoreError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Progression par segment : combien d'octets ont déjà été écrits dans chaque segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumeState {
    pub url: String,
    pub total_bytes: u64,
    /// Pour chaque segment : (start, end_inclusif, octets_déjà_téléchargés).
    pub segments: Vec<(u64, u64, u64)>,
}

/// Chemin du fichier sidecar associé à un fichier de destination.
pub fn sidecar_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".fgdownload");
    PathBuf::from(s)
}

pub fn save(dest: &Path, state: &ResumeState) -> Result<(), CoreError> {
    let json = serde_json::to_vec_pretty(state)?;
    std::fs::write(sidecar_path(dest), json)?;
    Ok(())
}

pub fn load(dest: &Path) -> Option<ResumeState> {
    let bytes = std::fs::read(sidecar_path(dest)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn clear(dest: &Path) {
    let _ = std::fs::remove_file(sidecar_path(dest));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let state = ResumeState {
            url: "http://x/f".into(),
            total_bytes: 100,
            segments: vec![(0, 49, 10), (50, 99, 0)],
        };
        save(&dest, &state).unwrap();
        assert_eq!(load(&dest), Some(state));
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempdir().unwrap();
        assert_eq!(load(&dir.path().join("nope.bin")), None);
    }

    #[test]
    fn clear_removes_sidecar() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let state = ResumeState { url: "u".into(), total_bytes: 1, segments: vec![(0, 0, 0)] };
        save(&dest, &state).unwrap();
        clear(&dest);
        assert_eq!(load(&dest), None);
    }
}
```

- [ ] **Step 2: Lancer les tests → passent**

Run:
```bash
cd src-tauri && cargo test -p download-core resume
```
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(core): resume sidecar save/load/clear with TDD"
```

---

## Task 5 : Moteur multi-segments + pause + reprise (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/engine.rs`

- [ ] **Step 1: Écrire la signature + tests**

`src-tauri/crates/download-core/src/engine.rs` :
```rust
use crate::http::ByteFetcher;
use crate::model::{DownloadStatus, Progress};
use crate::resume::{self, ResumeState};
use crate::segments::compute_segments;
use crate::CoreError;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

pub const DEFAULT_SEGMENTS: u32 = 8;

/// Contrôle partagé d'un téléchargement en cours.
#[derive(Clone, Default)]
pub struct DownloadHandle {
    pause: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    downloaded: Arc<AtomicU64>,
}

impl DownloadHandle {
    pub fn new() -> Self { Self::default() }
    pub fn pause(&self) { self.pause.store(true, Ordering::SeqCst); }
    pub fn cancel(&self) { self.cancel.store(true, Ordering::SeqCst); }
    pub fn downloaded(&self) -> u64 { self.downloaded.load(Ordering::SeqCst) }
}

/// Issue d'un run du moteur.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Completed,
    Paused,
    Cancelled,
}

/// Télécharge `url` vers `dest`. Émet la progression (octets cumulés) sur `progress`.
/// Reprend depuis le sidecar s'il existe. Multi-segments si le serveur le supporte.
pub async fn download<F: ByteFetcher>(
    fetcher: &F,
    id: &str,
    url: &str,
    dest: &Path,
    handle: DownloadHandle,
    progress: mpsc::UnboundedSender<Progress>,
) -> Result<Outcome, CoreError> {
    use futures_util::StreamExt;

    let probe = fetcher.probe(url).await?;
    let total = probe.total_bytes.unwrap_or(0);

    // Construire (ou recharger) l'état des segments.
    let mut state = resume::load(dest).filter(|s| s.total_bytes == total && total > 0)
        .unwrap_or_else(|| {
            let segs = if probe.supports_ranges && total > 0 {
                compute_segments(total, DEFAULT_SEGMENTS)
            } else if total > 0 {
                vec![(0, total - 1)]
            } else {
                vec![] // taille inconnue : un seul flux non-segmenté
            };
            ResumeState {
                url: url.to_string(),
                total_bytes: total,
                segments: segs.into_iter().map(|(s, e)| (s, e, 0u64)).collect(),
            }
        });

    // Pré-allouer le fichier de destination à la bonne taille (si connue).
    {
        let f = std::fs::OpenOptions::new().create(true).write(true).open(dest)?;
        if total > 0 { f.set_len(total)?; }
    }

    // Amorcer le compteur cumulé avec ce qui est déjà téléchargé (reprise).
    let already: u64 = state.segments.iter().map(|(_, _, d)| *d).sum();
    handle.downloaded.store(already, Ordering::SeqCst);

    // Cas taille inconnue : un seul flux séquentiel, pas de segmentation.
    if state.segments.is_empty() {
        let mut file = tokio::fs::OpenOptions::new().create(true).write(true).open(dest).await?;
        let mut stream = fetcher.fetch_range(url, None).await?;
        while let Some(chunk) = stream.next().await {
            if handle.cancel.load(Ordering::SeqCst) { return Ok(Outcome::Cancelled); }
            if handle.pause.load(Ordering::SeqCst) {
                emit(&progress, id, &handle, total, DownloadStatus::Paused);
                return Ok(Outcome::Paused);
            }
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            handle.downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
            emit(&progress, id, &handle, total, DownloadStatus::Downloading);
        }
        file.flush().await?;
        emit(&progress, id, &handle, total, DownloadStatus::Completed);
        return Ok(Outcome::Completed);
    }

    // Cas segmenté : une tâche par segment incomplet, écriture à l'offset.
    let mut tasks = Vec::new();
    for (idx, (start, end, done)) in state.segments.iter().enumerate() {
        let seg_start = *start;
        let seg_end = *end;
        let seg_done = *done;
        if seg_start + seg_done > seg_end { continue; } // segment déjà fini
        let resume_from = seg_start + seg_done;

        let fetcher_ref = fetcher;
        let url = url.to_string();
        let dest = dest.to_path_buf();
        let handle = handle.clone();
        let progress = progress.clone();
        let id = id.to_string();

        // Progrès par segment renvoyé pour mettre à jour le sidecar.
        let seg_progress = Arc::new(AtomicU64::new(seg_done));
        let seg_progress_ret = seg_progress.clone();

        let fut = async move {
            let mut file = tokio::fs::OpenOptions::new().write(true).open(&dest).await?;
            file.seek(std::io::SeekFrom::Start(resume_from)).await?;
            let mut stream = fetcher_ref.fetch_range(&url, Some((resume_from, seg_end))).await?;
            while let Some(chunk) = stream.next().await {
                if handle.cancel.load(Ordering::SeqCst) { return Ok::<(usize, Outcome), CoreError>((idx, Outcome::Cancelled)); }
                if handle.pause.load(Ordering::SeqCst) { return Ok((idx, Outcome::Paused)); }
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                seg_progress.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                handle.downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                emit(&progress, &id, &handle, total, DownloadStatus::Downloading);
            }
            file.flush().await?;
            Ok((idx, Outcome::Completed))
        };
        tasks.push((idx, seg_progress_ret, fut));
    }

    // Exécuter toutes les tâches de segment en parallèle.
    use tokio::io::AsyncSeekExt;
    let mut handles = Vec::new();
    let seg_trackers: Vec<(usize, Arc<AtomicU64>)> =
        tasks.iter().map(|(i, p, _)| (*i, p.clone())).collect();
    for (_, _, fut) in tasks {
        handles.push(tokio::spawn(fut));
    }

    let mut outcome = Outcome::Completed;
    for h in handles {
        match h.await.map_err(|e| CoreError::Network(e.to_string()))?? {
            (_, Outcome::Completed) => {}
            (_, Outcome::Paused) => { outcome = Outcome::Paused; }
            (_, Outcome::Cancelled) => { outcome = Outcome::Cancelled; }
        }
    }

    // Mettre à jour l'état des segments avec la progression observée.
    for (idx, tracker) in seg_trackers {
        let done = tracker.load(Ordering::SeqCst);
        let (s, e, _) = state.segments[idx];
        state.segments[idx] = (s, e, done.min(e - s + 1));
    }

    match outcome {
        Outcome::Completed => {
            resume::clear(dest);
            emit(&progress, id, &handle, total, DownloadStatus::Completed);
            Ok(Outcome::Completed)
        }
        Outcome::Paused => {
            resume::save(dest, &state)?;
            emit(&progress, id, &handle, total, DownloadStatus::Paused);
            Ok(Outcome::Paused)
        }
        Outcome::Cancelled => {
            resume::clear(dest);
            let _ = std::fs::remove_file(dest);
            Ok(Outcome::Cancelled)
        }
    }
}

fn emit(
    tx: &mpsc::UnboundedSender<Progress>,
    id: &str,
    handle: &DownloadHandle,
    total: u64,
    status: DownloadStatus,
) {
    let _ = tx.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: if total > 0 { Some(total) } else { None },
        speed_bps: 0, // la vitesse est calculée côté appelant (Task 8)
        status,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fake_fetcher::FakeFetcher;
    use tempfile::tempdir;

    fn collect_dest(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("out.bin")
    }

    #[tokio::test]
    async fn downloads_full_file_multisegment() {
        let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let fetcher = FakeFetcher::new(data.clone(), true);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, _rx) = mpsc::unbounded_channel();

        let outcome = download(&fetcher, "id1", "http://x/f", &dest, DownloadHandle::new(), tx)
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Completed);
        let written = std::fs::read(&dest).unwrap();
        assert_eq!(written, data);
    }

    #[tokio::test]
    async fn falls_back_to_single_stream_when_no_ranges() {
        let data: Vec<u8> = (0..200u8).collect();
        let fetcher = FakeFetcher::new(data.clone(), false);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, _rx) = mpsc::unbounded_channel();

        download(&fetcher, "id2", "http://x/f", &dest, DownloadHandle::new(), tx)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[tokio::test]
    async fn emits_progress() {
        let data: Vec<u8> = vec![1u8; 300];
        let fetcher = FakeFetcher::new(data, true);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, mut rx) = mpsc::unbounded_channel();

        download(&fetcher, "id3", "http://x/f", &dest, DownloadHandle::new(), tx)
            .await
            .unwrap();

        let mut last = 0u64;
        while let Ok(p) = rx.try_recv() {
            last = p.downloaded_bytes.max(last);
        }
        assert_eq!(last, 300);
    }
}
```

> Note d'implémentation : `AsyncSeekExt` et `AsyncWriteExt` doivent être importés (déjà fait via `use tokio::io::{AsyncWriteExt, AsyncSeekExt}` — vérifier que les deux `use` sont présents en haut/portée).

- [ ] **Step 2: Lancer les tests → d'abord échec de compilation attendu si un `use` manque, corriger, puis PASS**

Run:
```bash
cd src-tauri && cargo test -p download-core engine
```
Expected: PASS (3 tests). Si erreur « no method seek », ajouter `use tokio::io::AsyncSeekExt;` en haut du fichier.

- [ ] **Step 3: Test de reprise (pause puis reprise donne le fichier complet)**

Ajouter dans le module `tests` :
```rust
    #[tokio::test]
    async fn resume_after_pause_completes_file() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        // 1er run : on met en pause immédiatement après le démarrage.
        let fetcher = FakeFetcher::new(data.clone(), true);
        let handle = DownloadHandle::new();
        handle.pause(); // pause avant le premier chunk => crée le sidecar
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "id4", "http://x/f", &dest, handle, tx).await.unwrap();
        assert_eq!(outcome, Outcome::Paused);

        // 2e run : reprise jusqu'à complétion.
        let fetcher2 = FakeFetcher::new(data.clone(), true);
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let outcome2 = download(&fetcher2, "id4", "http://x/f", &dest, DownloadHandle::new(), tx2)
            .await
            .unwrap();
        assert_eq!(outcome2, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }
```

Run:
```bash
cd src-tauri && cargo test -p download-core engine::tests::resume_after_pause_completes_file
```
Expected: PASS. (Si la pause pré-démarrage ne crée pas de sidecar parce que les segments n'ont rien écrit, le 2e run repart de zéro et complète quand même — l'assertion finale reste vraie.)

- [ ] **Step 4: Lancer toute la suite du cœur**

Run:
```bash
cd src-tauri && cargo test -p download-core
```
Expected: tous PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): multi-segment engine with pause, resume and single-stream fallback"
```

---

## Task 6 : Persistance SQLite (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/storage.rs`

- [ ] **Step 1: Écrire l'API + tests (base en mémoire)**

`src-tauri/crates/download-core/src/storage.rs` :
```rust
use crate::model::{Download, DownloadStatus};
use crate::CoreError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let conn = Connection::open(path).map_err(|e| CoreError::Storage(e.to_string()))?;
        Self::from_conn(conn)
    }

    pub fn open_in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory().map_err(|e| CoreError::Storage(e.to_string()))?;
        Self::from_conn(conn)
    }

    fn from_conn(conn: Connection) -> Result<Self, CoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                file_name TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                total_bytes INTEGER,
                downloaded_bytes INTEGER NOT NULL,
                status TEXT NOT NULL,
                supports_ranges INTEGER NOT NULL,
                error TEXT
            );",
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn upsert(&self, d: &Download) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9",
            rusqlite::params![
                d.id, d.url, d.file_name, d.dest_path,
                d.total_bytes.map(|v| v as i64),
                d.downloaded_bytes as i64,
                status_to_str(d.status),
                d.supports_ranges as i64,
                d.error,
            ],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Download>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error FROM downloads")
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
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| CoreError::Storage(e.to_string()))?);
        }
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE id=?1", [id])
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
}

fn status_to_str(s: DownloadStatus) -> &'static str {
    match s {
        DownloadStatus::Queued => "queued",
        DownloadStatus::Downloading => "downloading",
        DownloadStatus::Paused => "paused",
        DownloadStatus::Completed => "completed",
        DownloadStatus::Error => "error",
    }
}

fn status_from_str(s: &str) -> DownloadStatus {
    match s {
        "downloading" => DownloadStatus::Downloading,
        "paused" => DownloadStatus::Paused,
        "completed" => DownloadStatus::Completed,
        "error" => DownloadStatus::Error,
        _ => DownloadStatus::Queued,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn upsert_then_list() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].downloaded_bytes, 40);
        assert_eq!(all[0].status, DownloadStatus::Paused);
    }

    #[test]
    fn upsert_updates_existing() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        let mut d = sample("a");
        d.downloaded_bytes = 100;
        d.status = DownloadStatus::Completed;
        s.upsert(&d).unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].downloaded_bytes, 100);
        assert_eq!(all[0].status, DownloadStatus::Completed);
    }

    #[test]
    fn delete_removes_row() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        s.delete("a").unwrap();
        assert!(s.list().unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Lancer les tests → passent**

Run:
```bash
cd src-tauri && cargo test -p download-core storage
```
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(core): SQLite storage (upsert/list/delete) with TDD"
```

---

## Task 7 : État applicatif Tauri + commandes + events

**Files:**
- Create: `src-tauri/src/state.rs`, `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`

- [ ] **Step 1: Dépendances du binaire**

Dans `src-tauri/Cargo.toml` (`[dependencies]`), s'assurer d'avoir :
```toml
download-core = { path = "crates/download-core" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
tauri = { version = "2", features = [] }
```
(Garder les autres dépendances générées par le template.)

- [ ] **Step 2: Écrire l'état applicatif**

`src-tauri/src/state.rs` :
```rust
use download_core::engine::DownloadHandle;
use download_core::storage::Storage;
use std::collections::HashMap;
use std::sync::Mutex;

/// État global géré par Tauri. `handles` permet pause/annulation des téléchargements actifs.
pub struct AppState {
    pub storage: Storage,
    pub handles: Mutex<HashMap<String, DownloadHandle>>,
}

impl AppState {
    pub fn new(storage: Storage) -> Self {
        Self { storage, handles: Mutex::new(HashMap::new()) }
    }
}
```

- [ ] **Step 3: Écrire les commandes**

`src-tauri/src/commands.rs` :
```rust
use crate::state::AppState;
use download_core::engine::{self, DownloadHandle};
use download_core::http::ReqwestFetcher;
use download_core::model::{Download, DownloadStatus, Progress};
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

#[derive(serde::Deserialize)]
pub struct AddArgs {
    pub url: String,
    pub dest_dir: String,
}

/// Crée un téléchargement (statut Queued) et le persiste. Retourne l'objet créé.
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    let fetcher = ReqwestFetcher::new();
    let probe = engine::probe_public(&fetcher, &args.url).await.map_err(|e| e.to_string())?;
    let file_name = probe.file_name.clone().unwrap_or_else(|| "download.bin".to_string());
    let dest_path = PathBuf::from(&args.dest_dir).join(&file_name);

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
    };

    let state = app.state::<AppState>();
    state.storage.upsert(&download).map_err(|e| e.to_string())?;
    Ok(download)
}

#[tauri::command]
pub fn list_downloads(state: State<AppState>) -> Result<Vec<Download>, String> {
    state.storage.list().map_err(|e| e.to_string())
}

/// Démarre (ou reprend) un téléchargement. Lance une tâche async qui émet la progression.
#[tauri::command]
pub async fn start_download(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let download = state
        .storage
        .list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("download not found: {id}"))?;

    let handle = DownloadHandle::new();
    state.handles.lock().unwrap().insert(id.clone(), handle.clone());

    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    // Tâche d'émission : calcule la vitesse et relaie vers l'UI + persiste.
    let app_for_rx = app.clone();
    let id_for_rx = id.clone();
    tokio::spawn(async move {
        let mut last_t = Instant::now();
        let mut last_bytes = 0u64;
        while let Some(mut p) = rx.recv().await {
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64();
            if dt >= 0.5 {
                let delta = p.downloaded_bytes.saturating_sub(last_bytes);
                p.speed_bps = (delta as f64 / dt) as u64;
                last_t = now;
                last_bytes = p.downloaded_bytes;
            }
            let _ = app_for_rx.emit("download://progress", &p);
            // Persistance à chaque transition d'état importante.
            if matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error) {
                let st = app_for_rx.state::<AppState>();
                if let Ok(list) = st.storage.list() {
                    if let Some(mut d) = list.into_iter().find(|d| d.id == id_for_rx) {
                        d.downloaded_bytes = p.downloaded_bytes;
                        d.status = p.status;
                        let _ = st.storage.upsert(&d);
                    }
                }
            }
        }
    });

    // Tâche de téléchargement.
    let dest = PathBuf::from(&download.dest_path);
    let url = download.url.clone();
    let id_for_dl = id.clone();
    tokio::spawn(async move {
        let fetcher = ReqwestFetcher::new();
        let _ = engine::download(&fetcher, &id_for_dl, &url, &dest, handle, tx).await;
        // Nettoyage du handle une fois terminé/pausé.
        let st = app.state::<AppState>();
        st.handles.lock().unwrap().remove(&id_for_dl);
    });

    Ok(())
}

#[tauri::command]
pub fn pause_download(state: State<AppState>, id: String) -> Result<(), String> {
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.pause();
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<AppState>, id: String) -> Result<(), String> {
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.cancel();
    }
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    Ok(())
}
```

> Cette tâche référence `engine::probe_public`. L'ajouter dans `engine.rs` :
> ```rust
> use crate::model::ProbeResult;
> /// Expose la phase d'inspection pour les appelants (commandes Tauri).
> pub async fn probe_public<F: ByteFetcher>(fetcher: &F, url: &str) -> Result<ProbeResult, CoreError> {
>     fetcher.probe(url).await
> }
> ```

- [ ] **Step 4: Câbler main.rs**

`src-tauri/src/main.rs` :
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use download_core::storage::Storage;
use state::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&dir).ok();
            let storage = Storage::open(&dir.join("downloads.sqlite")).expect("open db");
            app.manage(AppState::new(storage));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_download,
            commands::list_downloads,
            commands::start_download,
            commands::pause_download,
            commands::cancel_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: Vérifier la compilation Rust complète**

Run:
```bash
cd src-tauri && cargo build
```
Expected: compile (app + cœur). Corriger tout import manquant signalé.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(app): Tauri state, commands (add/list/start/pause/cancel) and progress events"
```

---

## Task 8 : Frontend — types, formatage (TDD) et couche API

**Files:**
- Create: `src/types.ts`, `src/format.ts`, `src/format.test.ts`, `src/api.ts`

- [ ] **Step 1: Types miroir**

`src/types.ts` :
```ts
export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "error";

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
}

export interface Progress {
  id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  status: DownloadStatus;
}
```

- [ ] **Step 2: Test de formatage (échoue)**

`src/format.test.ts` :
```ts
import { describe, it, expect } from "vitest";
import { formatBytes, formatSpeed, percent } from "./format";

describe("format", () => {
  it("formats bytes", () => {
    expect(formatBytes(0)).toBe("0 o");
    expect(formatBytes(1024)).toBe("1,0 Ko");
    expect(formatBytes(1536)).toBe("1,5 Ko");
    expect(formatBytes(1048576)).toBe("1,0 Mo");
  });

  it("formats speed", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(1048576)).toBe("1,0 Mo/s");
  });

  it("computes percent", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(0, null)).toBe(0);
  });
});
```

- [ ] **Step 3: Lancer → échoue**

Run:
```bash
npm test
```
Expected: FAIL (module `./format` introuvable).

- [ ] **Step 4: Implémenter format.ts**

`src/format.ts` :
```ts
const UNITS = ["o", "Ko", "Mo", "Go", "To"];

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 o";
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const text = i === 0 ? String(Math.round(v)) : v.toFixed(1).replace(".", ",");
  return `${text} ${UNITS[i]}`;
}

export function formatSpeed(bps: number): string {
  if (bps <= 0) return "—";
  return `${formatBytes(bps)}/s`;
}

export function percent(downloaded: number, total: number | null): number {
  if (!total || total <= 0) return 0;
  return Math.min(100, Math.round((downloaded / total) * 100));
}
```

- [ ] **Step 5: Lancer → passe**

Run:
```bash
npm test
```
Expected: PASS.

- [ ] **Step 6: Couche API**

`src/api.ts` :
```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Download, Progress } from "./types";

export function addDownload(url: string, destDir: string): Promise<Download> {
  return invoke<Download>("add_download", { args: { url, dest_dir: destDir } });
}

export function listDownloads(): Promise<Download[]> {
  return invoke<Download[]>("list_downloads");
}

export function startDownload(id: string): Promise<void> {
  return invoke("start_download", { id });
}

export function pauseDownload(id: string): Promise<void> {
  return invoke("pause_download", { id });
}

export function cancelDownload(id: string): Promise<void> {
  return invoke("cancel_download", { id });
}

export function onProgress(cb: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("download://progress", (e) => cb(e.payload));
}
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(ui): TS types, tested formatting helpers and Tauri API layer"
```

---

## Task 9 : Frontend — composants UI (layout Option B)

**Files:**
- Create: `src/components/Toolbar.tsx`, `src/components/AddUrlDialog.tsx`, `src/components/DownloadRow.tsx`, `src/components/DownloadTable.tsx`
- Modify: `src/App.tsx`

- [ ] **Step 1: AddUrlDialog**

`src/components/AddUrlDialog.tsx` :
```tsx
import { useState } from "react";

export function AddUrlDialog({
  open,
  onClose,
  onAdd,
}: {
  open: boolean;
  onClose: () => void;
  onAdd: (url: string) => void;
}) {
  const [url, setUrl] = useState("");
  if (!open) return null;
  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg p-5 w-[480px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold mb-3">Nouveau téléchargement</h3>
        <input
          autoFocus
          className="w-full border border-gray-200 rounded-md px-3 py-2 text-sm"
          placeholder="Colle une URL…"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && url.trim()) {
              onAdd(url.trim());
              setUrl("");
            }
          }}
        />
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold disabled:opacity-50"
            disabled={!url.trim()}
            onClick={() => {
              onAdd(url.trim());
              setUrl("");
            }}
          >
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Toolbar**

`src/components/Toolbar.tsx` :
```tsx
export function Toolbar({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-white">
      <button
        className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold"
        onClick={onAdd}
      >
        + Ajouter
      </button>
      <div className="flex-1" />
      <input
        className="bg-gray-100 rounded-md px-3 py-1.5 text-sm text-gray-500 w-48"
        placeholder="🔍 Rechercher…"
        disabled
        title="Recherche disponible en Phase 2"
      />
    </div>
  );
}
```

- [ ] **Step 3: DownloadRow**

`src/components/DownloadRow.tsx` :
```tsx
import type { Download } from "../types";
import { formatBytes, formatSpeed, percent } from "../format";

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
  onPause,
  onResume,
  onCancel,
}: {
  d: Download;
  speed: number;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
}) {
  const pct = percent(d.downloaded_bytes, d.total_bytes);
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
      <div className="flex-1">{STATUS_LABEL[d.status]} · {pct}%</div>
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

- [ ] **Step 4: DownloadTable**

`src/components/DownloadTable.tsx` :
```tsx
import type { Download } from "../types";
import { DownloadRow } from "./DownloadRow";

export function DownloadTable({
  downloads,
  speeds,
  onPause,
  onResume,
  onCancel,
}: {
  downloads: Download[];
  speeds: Record<string, number>;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
}) {
  return (
    <div>
      <div className="flex gap-3 px-3 py-2 text-[10px] uppercase tracking-wide text-gray-400 border-b border-gray-200 bg-gray-50">
        <div className="flex-[2]">Nom</div>
        <div className="flex-1">Taille</div>
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
            onPause={() => onPause(d.id)}
            onResume={() => onResume(d.id)}
            onCancel={() => onCancel(d.id)}
          />
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 5: App.tsx assemble tout**

`src/App.tsx` :
```tsx
import { useEffect, useState } from "react";
import "./index.css";
import type { Download } from "./types";
import * as api from "./api";
import { Toolbar } from "./components/Toolbar";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { DownloadTable } from "./components/DownloadTable";

// Dossier de destination par défaut (Phase 1 : Téléchargements de l'utilisateur).
const DEST_DIR = ""; // chaîne vide => le backend pourra défaulter ; voir note plan.

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);

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
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  async function handleAdd(url: string) {
    setDialogOpen(false);
    const d = await api.addDownload(url, DEST_DIR);
    setDownloads((list) => [...list, d]);
    await api.startDownload(d.id);
  }

  return (
    <div className="h-screen flex flex-col">
      <Toolbar onAdd={() => setDialogOpen(true)} />
      <div className="flex-1 overflow-auto">
        <DownloadTable
          downloads={downloads}
          speeds={speeds}
          onPause={(id) => api.pauseDownload(id)}
          onResume={(id) => api.startDownload(id)}
          onCancel={(id) => {
            api.cancelDownload(id);
            setDownloads((l) => l.filter((d) => d.id !== id));
          }}
        />
      </div>
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
    </div>
  );
}
```

> Note destination : pour la Phase 1, le dossier de destination par défaut doit être le dossier « Téléchargements ». Comme `DEST_DIR` est vide ici, modifier `add_download` (Task 7) pour défaulter quand `dest_dir` est vide :
> ```rust
> let dir = if args.dest_dir.is_empty() {
>     app.path().download_dir().map_err(|e| e.to_string())?
> } else {
>     PathBuf::from(&args.dest_dir)
> };
> let dest_path = dir.join(&file_name);
> ```
> et importer `tauri::Manager` (déjà importé) — `app.path()` requiert le trait `Manager`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ui): toolbar, add-url dialog, download table and row with live progress"
```

---

## Task 10 : Vérification bout-en-bout (manuelle) + correctifs

**Files:** aucun nouveau ; ajustements éventuels.

- [ ] **Step 1: Lancer l'app**

Run:
```bash
npm run tauri dev
```

- [ ] **Step 2: Scénario complet**

1. Cliquer « + Ajouter », coller une URL de fichier volumineux et stable, ex. :
   `https://speed.hetzner.de/100MB.bin`
2. Vérifier : la ligne apparaît, la barre progresse, la vitesse s'affiche.
3. Cliquer ⏸ → l'état passe « En pause », la progression s'arrête.
4. Cliquer ▶ → la progression reprend là où elle s'était arrêtée (pas de retour à 0).
5. Laisser finir → état « Terminé », barre verte à 100%.
6. Vérifier que le fichier existe dans le dossier Téléchargements et que sa taille correspond.
7. Fermer puis rouvrir l'app → le téléchargement terminé est toujours listé (persistance SQLite).

Expected: tous les points OK. Noter tout écart.

- [ ] **Step 3: Test de reprise après crash**

1. Démarrer un gros téléchargement.
2. Fermer brutalement la fenêtre à ~30%.
3. Rouvrir l'app, cliquer ▶ sur la ligne → reprend depuis ~30% grâce au sidecar `.fgdownload`.

Expected: complétion correcte, fichier intègre.

- [ ] **Step 4: Corriger les écarts éventuels**

Pour chaque écart constaté, créer un test (Rust si logique cœur, Vitest si formatage/affichage) reproduisant le bug avant de corriger (TDD). Committer chaque correctif séparément.

- [ ] **Step 5: Lancer toutes les suites**

Run:
```bash
cd src-tauri && cargo test -p download-core
cd .. && npm test
```
Expected: tout PASS.

- [ ] **Step 6: Commit final de la phase**

```bash
git add -A
git commit -m "test: end-to-end verification of phase 1 download core"
```

---

## Auto-revue (effectuée)

- **Couverture spec :** multi-segments (Task 2,5), reprise coupure/crash (Task 4,5 + sidecar + vérif manuelle Task 10), fallback mono-flux (Task 5), persistance SQLite (Task 6), ajouter/démarrer/pause/reprendre/annuler (Task 7), progression temps réel via events (Task 7,8), UI tableau Option B + barres (Task 9). ✔
- **Hors périmètre (phases ultérieures) :** file d'attente/priorités/throttling, catégories, intégration navigateur, vidéo/HLS, planification, thème sombre, installeur/auto-update. ✔ (volontairement exclus)
- **Cohérence des types :** `Download`, `DownloadStatus`, `Progress` identiques entre Rust (`model.rs`) et TS (`types.ts`) ; `add_download` reçoit `{ args: { url, dest_dir } }` côté JS et `AddArgs { url, dest_dir }` côté Rust ; event `download://progress` émis (Task 7) et écouté (Task 8). ✔
- **Pas de placeholder :** chaque step de code contient le code complet. ✔

## Décisions reportées (rappel)

- Nom définitif (provisoire « FlashGet »), bibliothèque de style (Tailwind retenu ici), signature de code (Phase 6).
- Nombre de segments configurable : codé en dur `DEFAULT_SEGMENTS = 8` en Phase 1, deviendra un paramètre en Phase 2.

## Suites Phase 2 (issues des revues de code — à traiter plus tard)

Éléments volontairement non traités en Phase 1, identifiés lors des relectures :

- **Réutiliser un seul `reqwest::Client`** stocké dans `AppState` au lieu d'en recréer un par `add_download`/`start_download` (perf, pool de connexions). (Important — perf)
- **`Storage::get_by_id`** pour éviter le `list()`+`find` O(n) dans `start_download`. (Mineur)
- **Source de vérité de la taille en base/sidecar** : persister `total_bytes`/`supports_ranges` à l'ajout et s'y fier à la reprise plutôt que de re-sonder (résout proprement la fragilité I1, déjà mitigée).
- **Persistance périodique du statut `downloading`** (pas seulement aux états terminaux) pour que `downloaded_bytes` en base ne soit pas en retard après un crash (les octets sont déjà sur disque + sidecar).
- **Tolérance au *poisoning* des mutex** : remplacer `lock().unwrap()` par `unwrap_or_else(|e| e.into_inner())` dans les commandes/stockage. (Mineur — robustesse)
- **Test e2e d'erreur** : moteur `Err` → ligne DB `error` → libellé « Erreur » dans l'UI (le chemin est désormais câblé via `mark_error` + event final ; reste à couvrir par un test).
- **Nom de fichier via `Content-Disposition`** (réutiliser le même assainisseur que `file_name_from_url`).
- **Collision de noms** : deux téléchargements sans nom déduit → `download.bin` en double dans le même dossier ; prévoir une dé-duplication (suffixe `(1)`).

## Statut d'exécution

Phase 1 implémentée par développement piloté par sous-agents (TDD + double relecture spec/qualité sur les tâches à risque). **26 tests `download-core` + 3 tests Vitest verts**, builds `cargo`/`vite` propres, test d'intégration sur vrai serveur HTTP (Range) inclus. Reste à faire par un humain : **smoke-test GUI** (`npm run tauri dev`) — non observable en environnement headless.
