# Phase 2b — Organisation (catégories, recherche/filtres, historique) — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Catégoriser automatiquement les téléchargements par extension et les router vers un dossier de destination par catégorie, plus recherche/filtres (nom, catégorie, statut) côté frontend et actions d'historique (ouvrir fichier/dossier, retirer de la liste, vider les terminés).

**Architecture:** Un `categorizer` pur (Rust, testable) mappe l'extension → `Category`. Les dossiers par catégorie sont persistés dans la table `settings`. À l'ajout, le backend route directement vers le dossier de la catégorie. Le filtrage (recherche/catégorie/statut) est purement frontend via un helper testé. Les actions fichier/dossier passent par le plugin `opener` (déjà présent) ; le sélecteur de dossier par le plugin `dialog` (ajouté).

**Tech Stack:** Rust (rusqlite, tauri-plugin-opener — présents ; **ajout** tauri-plugin-dialog), React + TS + Tailwind + Vitest, **ajout** `@tauri-apps/plugin-dialog`.

**Spec :** [docs/superpowers/specs/2026-06-03-phase-2b-organization-design.md](../specs/2026-06-03-phase-2b-organization-design.md).

---

## État de départ (Phase 2a, en place)

- `download-core` : `model.rs` (`Download{...,priority:Priority}`, `DownloadStatus`, `Priority`, `Settings`), `categorizer.rs` **absent**, `scheduler.rs`, `rate_limiter.rs`, `storage.rs` (`open/open_in_memory/upsert/list/delete/update_state/mark_error/get_by_id/set_priority/get_settings/set_settings/mark_active`, table `settings` clé/valeur, `list ORDER BY rowid`), `engine.rs`.
- App : `state.rs` (`AppState{storage,handles,fetcher,limiter}`), `commands.rs` (add/list/start/pause/cancel/set_priority/get_settings/update_settings/pause_all/resume_all + privés `reevaluate`/`launch` ; **les tâches sont lancées via `tauri::async_runtime::spawn`**), `lib.rs` (plugin opener + 10 commandes).
- Front : `types.ts` (`DownloadStatus,Download,Progress,Priority,Settings`), `api.ts`, `format.ts`, `App.tsx`, `components/{Toolbar,AddUrlDialog,DownloadTable,DownloadRow,PriorityPill,SettingsDialog}.tsx`.
- Tests : `cargo test -p download-core` (39 unit + 2 intégration) + `npm test` (3 Vitest).

**Rappel impératif :** toute nouvelle commande qui lance une tâche async doit utiliser `tauri::async_runtime::spawn` (jamais `tokio::spawn`). Les commandes de 2b ne lancent PAS de tâches (elles sont synchrones ou async simples), donc pas de spawn ici.

## Structure des fichiers (2b)

```
src-tauri/crates/download-core/src/
  categorizer.rs   (NOUVEAU) categorize(file_name) -> Category (pur)
  model.rs         (+) enum Category, CategoryDirs, champ Download.category
  storage.rs       (+) migration category, round-trip, category_dir/get_category_dirs/set_category_dir, clear_completed
  lib.rs           (+) pub mod categorizer;
src-tauri/
  Cargo.toml       (+) tauri-plugin-dialog = "2"
  capabilities/default.json (+) "dialog:default"
  src/
    commands.rs    (+) routage par catégorie dans add_download ; get_category_dirs/set_category_dir/remove_from_list/clear_completed/open_file/open_folder
    lib.rs         (+) plugin dialog ; enregistrer les 6 nouvelles commandes
src/
  types.ts         (+) Category, Download.category, CategoryDirs
  api.ts           (+) getCategoryDirs/setCategoryDir/removeFromList/clearCompleted/openFile/openFolder
  filter.ts        (NOUVEAU) filterDownloads (pur, testé)
  filter.test.ts   (NOUVEAU)
  components/
    CategoryPill.tsx (NOUVEAU)
    CategoryTabs.tsx (NOUVEAU)
    RowMenu.tsx      (NOUVEAU)
    Toolbar.tsx      (+) recherche + filtre statut
    DownloadRow.tsx  (+) colonne catégorie + menu ⋯
    DownloadTable.tsx(+) en-tête Cat. + actions menu
    SettingsDialog.tsx (+) section Dossiers par catégorie (+ dialogue)
  App.tsx          (+) état des filtres + filterDownloads + actions
  package.json     (+) @tauri-apps/plugin-dialog
```

---

## Task 1 : `categorizer.rs` + enum `Category` (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs`, `src-tauri/crates/download-core/src/lib.rs`
- Create: `src-tauri/crates/download-core/src/categorizer.rs`

- [ ] **Step 1: Ajouter l'enum `Category` au modèle**

Dans `model.rs`, après l'enum `Priority`, ajouter :
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Video,
    Document,
    Program,
    Archive,
    Music,
    #[default]
    Other,
}
```

- [ ] **Step 2: Déclarer le module**

Dans `lib.rs`, ajouter à la liste des modules : `pub mod categorizer;`.

- [ ] **Step 3: Écrire `categorizer.rs` avec ses tests**

`src-tauri/crates/download-core/src/categorizer.rs` :
```rust
use crate::model::Category;

/// Détermine la catégorie d'un fichier d'après son extension (insensible à la casse).
/// Sans extension ou extension inconnue ⇒ `Other`. Ne regarde que la dernière extension
/// (`archive.tar.gz` ⇒ `gz` ⇒ Archive).
pub fn categorize(file_name: &str) -> Category {
    let ext = match file_name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => return Category::Other,
    };
    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "mpg" | "mpeg" | "ts" => {
            Category::Video
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "odt" | "ods" | "csv"
        | "epub" | "rtf" => Category::Document,
        "exe" | "msi" | "dmg" | "pkg" | "deb" | "rpm" | "apk" | "appimage" => Category::Program,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" => Category::Archive,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" => Category::Music,
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_category() {
        assert_eq!(categorize("film.mp4"), Category::Video);
        assert_eq!(categorize("rapport.pdf"), Category::Document);
        assert_eq!(categorize("setup.exe"), Category::Program);
        assert_eq!(categorize("data.zip"), Category::Archive);
        assert_eq!(categorize("song.mp3"), Category::Music);
    }

    #[test]
    fn unknown_and_missing_extension_is_other() {
        assert_eq!(categorize("file.xyz"), Category::Other);
        assert_eq!(categorize("noext"), Category::Other);
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(categorize("MOVIE.MP4"), Category::Video);
        assert_eq!(categorize("Archive.ZIP"), Category::Archive);
    }

    #[test]
    fn uses_last_extension_for_multi_dot() {
        assert_eq!(categorize("backup.tar.gz"), Category::Archive);
    }
}
```

- [ ] **Step 4: Lancer les tests → PASS**

Run:
```bash
cd src-tauri && cargo test -p download-core categorizer
```
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): file categorizer + Category enum (TDD)"
```

---

## Task 2 : Persistance — champ `category`, dossiers par catégorie, clear_completed (TDD)

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs`, `src-tauri/crates/download-core/src/storage.rs`, `src-tauri/src/commands.rs`

- [ ] **Step 1: Ajouter `category` à `Download` + `CategoryDirs`**

Dans `model.rs`, ajouter le champ à `Download` (après `priority`) :
```rust
    #[serde(default)]
    pub category: Category,
```
Et ajouter la structure des dossiers par catégorie :
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CategoryDirs {
    pub video: Option<String>,
    pub document: Option<String>,
    pub program: Option<String>,
    pub archive: Option<String>,
    pub music: Option<String>,
    pub other: Option<String>,
}
```

- [ ] **Step 2: Mettre à jour la construction de `Download` dans `add_download`**

Dans `src-tauri/src/commands.rs`, dans `add_download`, après `priority: Priority::Normal,` ajouter :
```rust
        category: download_core::categorizer::categorize(&file_name),
```
(Le routage du dossier vient en Task 3 ; ici on persiste juste la catégorie, la destination reste le dossier Téléchargements.)

- [ ] **Step 3: Écrire les tests storage**

Dans `storage.rs`, mettre à jour `sample()` (ajouter `category: Category::Other,` à la fin de la struct) et ajouter :
```rust
    #[test]
    fn round_trips_category() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("c");
        d.category = Category::Video;
        s.upsert(&d).unwrap();
        assert_eq!(s.list().unwrap()[0].category, Category::Video);
        assert_eq!(s.get_by_id("c").unwrap().unwrap().category, Category::Video);
    }

    #[test]
    fn category_dir_roundtrip_and_default_none() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.category_dir(Category::Video).unwrap(), None);
        s.set_category_dir(Category::Video, Some("C:/Vids".to_string())).unwrap();
        assert_eq!(s.category_dir(Category::Video).unwrap(), Some("C:/Vids".to_string()));
        // Chaîne vide ⇒ None (revient au défaut).
        s.set_category_dir(Category::Video, None).unwrap();
        assert_eq!(s.category_dir(Category::Video).unwrap(), None);
    }

    #[test]
    fn get_category_dirs_reflects_sets() {
        let s = Storage::open_in_memory().unwrap();
        s.set_category_dir(Category::Music, Some("C:/Music".to_string())).unwrap();
        let dirs = s.get_category_dirs().unwrap();
        assert_eq!(dirs.music, Some("C:/Music".to_string()));
        assert_eq!(dirs.video, None);
    }

    #[test]
    fn clear_completed_removes_only_completed() {
        let s = Storage::open_in_memory().unwrap();
        let mut a = sample("a");
        a.status = DownloadStatus::Completed;
        s.upsert(&a).unwrap();
        let mut b = sample("b");
        b.status = DownloadStatus::Downloading;
        s.upsert(&b).unwrap();
        s.clear_completed().unwrap();
        let ids: Vec<String> = s.list().unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["b"]);
    }
```

- [ ] **Step 4: Implémenter dans `storage.rs`**

Élargir l'import en tête : `use crate::model::{Download, DownloadStatus, Priority, Settings, Category, CategoryDirs};`

Dans `from_conn`, après la migration `priority` existante, ajouter la migration `category` :
```rust
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN category TEXT NOT NULL DEFAULT 'other'",
            [],
        );
```

Dans `upsert`, ajouter la colonne `category` (paramètre `?11`) :
```rust
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9, priority=?10, category=?11",
```
et ajouter `category_to_str(d.category),` à la fin des `params!`.

Dans `list` ET `get_by_id`, ajouter `category` au `SELECT` (en dernière colonne, index 10) et au mapping :
```rust
            "SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category FROM downloads ORDER BY rowid"
```
(pour `get_by_id`, garder `WHERE id=?1` à la place de `ORDER BY rowid`) ; et dans le `Download { ... }` du `query_map`, ajouter :
```rust
                    category: category_from_str(&row.get::<_, String>(10)?),
```

Ajouter les méthodes (après `set_priority`) :
```rust
    pub fn category_dir(&self, category: Category) -> Result<Option<String>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let key = format!("dest.{}", category_to_str(category));
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key.as_str()], |r| r.get::<_, String>(0))
            .ok()
            .filter(|s| !s.is_empty()))
    }

    pub fn get_category_dirs(&self) -> Result<CategoryDirs, CoreError> {
        Ok(CategoryDirs {
            video: self.category_dir(Category::Video)?,
            document: self.category_dir(Category::Document)?,
            program: self.category_dir(Category::Program)?,
            archive: self.category_dir(Category::Archive)?,
            music: self.category_dir(Category::Music)?,
            other: self.category_dir(Category::Other)?,
        })
    }

    pub fn set_category_dir(&self, category: Category, path: Option<String>) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let key = format!("dest.{}", category_to_str(category));
        let val = path.unwrap_or_default();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params![key, val],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn clear_completed(&self) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE status='completed'", [])
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
```

Ajouter les helpers (à côté de `priority_to_str`) :
```rust
fn category_to_str(c: Category) -> &'static str {
    match c {
        Category::Video => "video",
        Category::Document => "document",
        Category::Program => "program",
        Category::Archive => "archive",
        Category::Music => "music",
        Category::Other => "other",
    }
}

fn category_from_str(s: &str) -> Category {
    match s {
        "video" => Category::Video,
        "document" => Category::Document,
        "program" => Category::Program,
        "archive" => Category::Archive,
        "music" => Category::Music,
        _ => Category::Other,
    }
}
```

- [ ] **Step 5: Lancer les tests → PASS, puis build workspace**

Run:
```bash
cd src-tauri && cargo test -p download-core storage && cargo build
```
Expected: storage tests PASS (existants + 4 nouveaux) ; build OK (commands.rs compile avec le champ `category`).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(core): persist category, per-category dirs, clear_completed (TDD)"
```

---

## Task 3 : Routage par catégorie + commandes (dossiers, historique, ouverture)

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/capabilities/default.json`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1: Ajouter le plugin dialog (Rust)**

Dans `src-tauri/Cargo.toml`, section `[dependencies]`, ajouter :
```toml
tauri-plugin-dialog = "2"
```

- [ ] **Step 2: Capability dialog**

Dans `src-tauri/capabilities/default.json`, ajouter `"dialog:default"` au tableau `permissions` :
```json
  "permissions": [
    "core:default",
    "opener:default",
    "dialog:default"
  ]
```

- [ ] **Step 3: Routage par catégorie dans `add_download`**

Dans `commands.rs`, remplacer le bloc de calcul de `dest_dir` (et la récupération du fetcher) par une version qui route selon la catégorie. La fonction devient :
```rust
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(&args.url))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;
    let file_name = probe.file_name.clone().unwrap_or_else(|| "download.bin".to_string());
    let category = download_core::categorizer::categorize(&file_name);

    // Dossier : argument explicite > dossier configuré pour la catégorie > dossier Téléchargements.
    let dest_dir = if !args.dest_dir.trim().is_empty() {
        PathBuf::from(&args.dest_dir)
    } else {
        let configured = { app.state::<AppState>().storage.category_dir(category).map_err(|e| e.to_string())? };
        match configured {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;

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
        category,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(download)
}
```

- [ ] **Step 4: Nouvelles commandes**

Élargir l'import du modèle en tête de `commands.rs` : ajouter `Category, CategoryDirs` :
```rust
use download_core::model::{Download, DownloadStatus, Priority, Progress, Settings, Category, CategoryDirs};
```
Ajouter en haut l'import du trait opener :
```rust
use tauri_plugin_opener::OpenerExt;
```
Ajouter ces commandes (n'importe où parmi les autres `#[tauri::command]`) :
```rust
#[tauri::command]
pub fn get_category_dirs(state: State<AppState>) -> Result<CategoryDirs, String> {
    state.storage.get_category_dirs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_category_dir(state: State<AppState>, category: Category, path: Option<String>) -> Result<(), String> {
    let path = path.filter(|p| !p.trim().is_empty());
    state.storage.set_category_dir(category, path).map_err(|e| e.to_string())
}

/// Retire l'entrée de la liste SANS supprimer le fichier (distinct d'`annuler`).
/// Refuse si le téléchargement est encore actif.
#[tauri::command]
pub fn remove_from_list(state: State<AppState>, id: String) -> Result<(), String> {
    if state.handles.lock().unwrap().contains_key(&id) {
        return Err("arrêtez le téléchargement avant de le retirer".to_string());
    }
    state.storage.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_completed(state: State<AppState>) -> Result<(), String> {
    state.storage.clear_completed().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_file(app: AppHandle, id: String) -> Result<(), String> {
    let d = app.state::<AppState>().storage.get_by_id(&id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    app.opener().open_path(d.dest_path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_folder(app: AppHandle, id: String) -> Result<(), String> {
    let d = app.state::<AppState>().storage.get_by_id(&id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    app.opener().reveal_item_in_dir(&d.dest_path).map_err(|e| e.to_string())
}
```
> Note API opener : si le compilateur refuse `None::<&str>` pour `open_path` ou la signature de `reveal_item_in_dir`, vérifier la version installée de `tauri-plugin-opener` (`cargo doc -p tauri-plugin-opener --open` ou la source) et adapter le type de l'argument `with` / le type du chemin. Le comportement visé : `open_file` ouvre le fichier avec l'appli système ; `open_folder` révèle le fichier dans l'explorateur. Ne pas changer les noms de commandes.

- [ ] **Step 5: Enregistrer le plugin + les commandes dans `lib.rs`**

Dans `src-tauri/src/lib.rs`, ajouter le plugin dialog après le plugin opener :
```rust
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
```
Et ajouter les 6 commandes au `generate_handler!` :
```rust
            commands::pause_all,
            commands::resume_all,
            commands::get_category_dirs,
            commands::set_category_dir,
            commands::remove_from_list,
            commands::clear_completed,
            commands::open_file,
            commands::open_folder,
```

- [ ] **Step 6: Build + tests cœur**

Run:
```bash
cd src-tauri && cargo build && cargo test -p download-core
```
Expected: compile (app + cœur), cœur vert. Corriger tout import inutilisé / signature opener.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(app): category routing + dialog plugin + history/open commands"
```

---

## Task 4 : Frontend — types & API

**Files:**
- Modify: `src/types.ts`, `src/api.ts`, `package.json` (via npm)

- [ ] **Step 1: Installer le plugin dialog (JS)**

Run (depuis la racine du projet) :
```bash
npm install @tauri-apps/plugin-dialog
```

- [ ] **Step 2: Étendre `types.ts`**

Ajouter à `src/types.ts` :
```ts
export type Category = "video" | "document" | "program" | "archive" | "music" | "other";

export interface CategoryDirs {
  video: string | null;
  document: string | null;
  program: string | null;
  archive: string | null;
  music: string | null;
  other: string | null;
}
```
Et ajouter le champ à l'interface `Download` (après `priority`) :
```ts
  category: Category;
```

- [ ] **Step 3: Étendre `api.ts`**

Élargir l'import de types en tête : `import type { Download, Progress, Priority, Settings, Category, CategoryDirs } from "./types";`
Ajouter à la fin de `src/api.ts` :
```ts
export function getCategoryDirs(): Promise<CategoryDirs> {
  return invoke<CategoryDirs>("get_category_dirs");
}

export function setCategoryDir(category: Category, path: string | null): Promise<void> {
  return invoke("set_category_dir", { category, path });
}

export function removeFromList(id: string): Promise<void> {
  return invoke("remove_from_list", { id });
}

export function clearCompleted(): Promise<void> {
  return invoke("clear_completed");
}

export function openFile(id: string): Promise<void> {
  return invoke("open_file", { id });
}

export function openFolder(id: string): Promise<void> {
  return invoke("open_folder", { id });
}
```

- [ ] **Step 4: Vérifier les types**

Run:
```bash
npx tsc --noEmit
```
Expected: aucune erreur.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ui): TS types + API for categories, dirs and history actions"
```

---

## Task 5 : Frontend — helper de filtrage pur (TDD)

**Files:**
- Create: `src/filter.ts`, `src/filter.test.ts`

- [ ] **Step 1: Écrire le test (échoue)**

`src/filter.test.ts` :
```ts
import { describe, it, expect } from "vitest";
import { filterDownloads } from "./filter";
import type { Download } from "./types";

function dl(partial: Partial<Download>): Download {
  return {
    id: "x", url: "u", file_name: "file.bin", dest_path: "/x", total_bytes: 1,
    downloaded_bytes: 0, status: "queued", supports_ranges: true, error: null,
    priority: "normal", category: "other", ...partial,
  };
}

describe("filterDownloads", () => {
  const list: Download[] = [
    dl({ id: "a", file_name: "Movie.mp4", category: "video", status: "completed" }),
    dl({ id: "b", file_name: "report.pdf", category: "document", status: "downloading" }),
    dl({ id: "c", file_name: "movie-trailer.mkv", category: "video", status: "queued" }),
  ];

  it("returns all with empty filters", () => {
    expect(filterDownloads(list, { query: "", category: "all", status: "all" }).map(d => d.id)).toEqual(["a", "b", "c"]);
  });
  it("filters by name (case-insensitive substring)", () => {
    expect(filterDownloads(list, { query: "movie", category: "all", status: "all" }).map(d => d.id)).toEqual(["a", "c"]);
  });
  it("filters by category", () => {
    expect(filterDownloads(list, { query: "", category: "video", status: "all" }).map(d => d.id)).toEqual(["a", "c"]);
  });
  it("filters by status", () => {
    expect(filterDownloads(list, { query: "", category: "all", status: "downloading" }).map(d => d.id)).toEqual(["b"]);
  });
  it("combines filters", () => {
    expect(filterDownloads(list, { query: "movie", category: "video", status: "queued" }).map(d => d.id)).toEqual(["c"]);
  });
});
```

- [ ] **Step 2: Lancer → échoue**

Run: `npm test`
Expected: FAIL (module `./filter` introuvable).

- [ ] **Step 3: Implémenter `filter.ts`**

`src/filter.ts` :
```ts
import type { Download, Category, DownloadStatus } from "./types";

export interface Filters {
  query: string;
  category: Category | "all";
  status: DownloadStatus | "all";
}

export function filterDownloads(list: Download[], f: Filters): Download[] {
  const q = f.query.trim().toLowerCase();
  return list.filter(
    (d) =>
      (f.category === "all" || d.category === f.category) &&
      (f.status === "all" || d.status === f.status) &&
      (q === "" || d.file_name.toLowerCase().includes(q)),
  );
}
```

- [ ] **Step 4: Lancer → PASS**

Run: `npm test`
Expected: PASS (les 3 tests format + 5 filter).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ui): pure filterDownloads helper (TDD)"
```

---

## Task 6 : Frontend — composants UI (catégories, recherche/filtre, menu d'actions)

**Files:**
- Create: `src/components/CategoryPill.tsx`, `src/components/CategoryTabs.tsx`, `src/components/RowMenu.tsx`
- Modify: `src/components/Toolbar.tsx`, `src/components/DownloadRow.tsx`, `src/components/DownloadTable.tsx`, `src/components/SettingsDialog.tsx`, `src/App.tsx`

- [ ] **Step 1: `CategoryPill.tsx`**

`src/components/CategoryPill.tsx` :
```tsx
import type { Category } from "../types";

const LABEL: Record<Category, string> = {
  video: "🎬 Vidéos",
  document: "📄 Docs",
  program: "⚙ Prog.",
  archive: "🗜 Archives",
  music: "🎵 Musique",
  other: "Autre",
};

export function CategoryPill({ value }: { value: Category }) {
  return <span className="text-[10px] text-gray-500 bg-gray-100 rounded px-1.5 py-0.5">{LABEL[value]}</span>;
}
```

- [ ] **Step 2: `CategoryTabs.tsx`**

`src/components/CategoryTabs.tsx` :
```tsx
import type { Category } from "../types";

const TABS: { key: Category | "all"; label: string }[] = [
  { key: "all", label: "Tous" },
  { key: "video", label: "🎬 Vidéos" },
  { key: "document", label: "📄 Documents" },
  { key: "program", label: "⚙ Programmes" },
  { key: "archive", label: "🗜 Archives" },
  { key: "music", label: "🎵 Musique" },
  { key: "other", label: "Autre" },
];

export function CategoryTabs({
  active,
  onChange,
}: {
  active: Category | "all";
  onChange: (c: Category | "all") => void;
}) {
  return (
    <div className="flex gap-1 px-3 pt-2 bg-gray-50 border-b border-gray-200 text-[11px]">
      {TABS.map((t) => (
        <button
          key={t.key}
          onClick={() => onChange(t.key)}
          className={`px-3 py-1.5 rounded-t-md ${
            active === t.key ? "bg-white text-indigo-600 font-semibold border border-gray-200 border-b-white -mb-px" : "text-gray-500"
          }`}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
```

- [ ] **Step 3: `RowMenu.tsx`**

`src/components/RowMenu.tsx` :
```tsx
import { useState } from "react";

export function RowMenu({
  canOpen,
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  canOpen: boolean;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button className="px-2 py-1 rounded bg-gray-100" onClick={() => setOpen((o) => !o)} title="Actions">⋯</button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-8 z-20 w-44 bg-white border border-gray-200 rounded-lg shadow-lg text-sm overflow-hidden">
            <button
              className="block w-full text-left px-3 py-2 hover:bg-gray-50 disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFile(); }}
            >📂 Ouvrir le fichier</button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-gray-50 disabled:opacity-40"
              disabled={!canOpen}
              onClick={() => { setOpen(false); onOpenFolder(); }}
            >🗂 Ouvrir le dossier</button>
            <button
              className="block w-full text-left px-3 py-2 hover:bg-gray-50 border-t border-gray-100"
              onClick={() => { setOpen(false); onRemove(); }}
            >🗑 Retirer de la liste</button>
          </div>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 4: `Toolbar.tsx` (recherche + filtre statut + vider terminés)**

Remplacer `src/components/Toolbar.tsx` par :
```tsx
import type { DownloadStatus } from "../types";

export function Toolbar({
  query,
  onQuery,
  status,
  onStatus,
  onAdd,
  onPauseAll,
  onResumeAll,
  onClearCompleted,
  onSettings,
}: {
  query: string;
  onQuery: (q: string) => void;
  status: DownloadStatus | "all";
  onStatus: (s: DownloadStatus | "all") => void;
  onAdd: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onClearCompleted: () => void;
  onSettings: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-200 bg-white">
      <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold" onClick={onAdd}>+ Ajouter</button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onPauseAll}>⏸ Tout</button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onResumeAll}>▶ Tout</button>
      <input
        className="flex-1 bg-gray-100 rounded-md px-3 py-1.5 text-sm"
        placeholder="🔍 Rechercher par nom…"
        value={query}
        onChange={(e) => onQuery(e.target.value)}
      />
      <select
        className="border border-gray-200 rounded-md px-2 py-1.5 text-sm bg-white"
        value={status}
        onChange={(e) => onStatus(e.target.value as DownloadStatus | "all")}
      >
        <option value="all">Tous les statuts</option>
        <option value="downloading">En cours</option>
        <option value="queued">En file</option>
        <option value="paused">En pause</option>
        <option value="completed">Terminé</option>
        <option value="error">Erreur</option>
      </select>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onClearCompleted} title="Vider les terminés">🧹</button>
      <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100 text-gray-600" onClick={onSettings} title="Réglages">⚙</button>
    </div>
  );
}
```

- [ ] **Step 5: `DownloadRow.tsx` (colonne catégorie + menu)**

Remplacer `src/components/DownloadRow.tsx` par :
```tsx
import type { Download, Priority } from "../types";
import { formatBytes, formatSpeed, percent } from "../format";
import { PriorityPill } from "./PriorityPill";
import { CategoryPill } from "./CategoryPill";
import { RowMenu } from "./RowMenu";

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
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  d: Download;
  speed: number;
  queuePosition: number | null;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onPriority: (p: Priority) => void;
  onOpenFile: () => void;
  onOpenFolder: () => void;
  onRemove: () => void;
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
          <div className={`h-full rounded ${d.status === "completed" ? "bg-green-500" : "bg-indigo-600"}`} style={{ width: `${pct}%` }} />
        </div>
      </div>
      <div className="flex-1"><CategoryPill value={d.category} /></div>
      <div className="flex-1"><PriorityPill value={d.priority} onChange={onPriority} /></div>
      <div className="flex-1">{stateLabel}</div>
      <div className="flex-1">{d.status === "downloading" ? formatSpeed(speed) : "—"}</div>
      <div className="flex gap-1 items-center">
        {d.status === "downloading" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onPause}>⏸</button>
        ) : d.status !== "completed" ? (
          <button className="px-2 py-1 rounded bg-gray-100" onClick={onResume}>▶</button>
        ) : null}
        <button className="px-2 py-1 rounded bg-gray-100" onClick={onCancel} title="Annuler (efface le fichier)">✕</button>
        <RowMenu canOpen={d.status === "completed"} onOpenFile={onOpenFile} onOpenFolder={onOpenFolder} onRemove={onRemove} />
      </div>
    </div>
  );
}
```

- [ ] **Step 6: `DownloadTable.tsx` (en-tête Cat. + transmission des actions)**

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
  onOpenFile,
  onOpenFolder,
  onRemove,
}: {
  downloads: Download[];
  speeds: Record<string, number>;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onPriority: (id: string, p: Priority) => void;
  onOpenFile: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onRemove: (id: string) => void;
}) {
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
        <div className="flex-1">Cat.</div>
        <div className="flex-1">Priorité</div>
        <div className="flex-1">État</div>
        <div className="flex-1">Vitesse</div>
        <div className="w-24" />
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
            onOpenFile={() => onOpenFile(d.id)}
            onOpenFolder={() => onOpenFolder(d.id)}
            onRemove={() => onRemove(d.id)}
          />
        ))
      )}
    </div>
  );
}
```

- [ ] **Step 7: `SettingsDialog.tsx` (section Dossiers par catégorie)**

Remplacer `src/components/SettingsDialog.tsx` par :
```tsx
import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../api";
import type { Category, CategoryDirs } from "../types";

const CATS: { key: Category; label: string }[] = [
  { key: "video", label: "🎬 Vidéos" },
  { key: "document", label: "📄 Documents" },
  { key: "program", label: "⚙ Programmes" },
  { key: "archive", label: "🗜 Archives" },
  { key: "music", label: "🎵 Musique" },
  { key: "other", label: "Autre" },
];

export function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [maxConcurrent, setMaxConcurrent] = useState(3);
  const [speedValue, setSpeedValue] = useState(0);
  const [unlimited, setUnlimited] = useState(true);
  const [dirs, setDirs] = useState<CategoryDirs | null>(null);

  useEffect(() => {
    if (!open) return;
    api.getSettings().then((s) => {
      setMaxConcurrent(s.max_concurrent);
      if (s.max_speed_bps === 0) { setUnlimited(true); setSpeedValue(0); }
      else { setUnlimited(false); setSpeedValue(Math.round((s.max_speed_bps / (1024 * 1024)) * 10) / 10); }
    });
    api.getCategoryDirs().then(setDirs);
  }, [open]);

  if (!open) return null;

  async function pickDir(cat: Category) {
    const selected = await openDialog({ directory: true });
    if (typeof selected === "string") {
      await api.setCategoryDir(cat, selected);
      setDirs(await api.getCategoryDirs());
    }
  }

  async function clearDir(cat: Category) {
    await api.setCategoryDir(cat, null);
    setDirs(await api.getCategoryDirs());
  }

  async function save() {
    const bps = unlimited ? 0 : Math.round(speedValue * 1024 * 1024);
    await api.updateSettings(Math.max(1, Math.round(maxConcurrent)), bps);
    onClose();
  }

  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg w-[460px] max-h-[80vh] overflow-auto" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold px-4 py-3 border-b border-gray-200">⚙ Réglages</h3>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="block text-sm text-gray-600 mb-1.5">Téléchargements simultanés max</label>
          <input type="number" min={1} className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24"
            value={maxConcurrent} onChange={(e) => setMaxConcurrent(Number(e.target.value))} />
        </div>
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="block text-sm text-gray-600 mb-1.5">Vitesse de téléchargement max (Mo/s)</label>
          <div className="flex items-center gap-3">
            <input type="number" min={0} step={0.1} disabled={unlimited}
              className="border border-gray-200 rounded-md px-2 py-1 text-sm w-24 disabled:opacity-50"
              value={speedValue} onChange={(e) => setSpeedValue(Number(e.target.value))} />
            <label className="text-sm flex items-center gap-1">
              <input type="checkbox" checked={unlimited} onChange={(e) => setUnlimited(e.target.checked)} /> Illimitée
            </label>
          </div>
        </div>
        <div className="px-4 py-3">
          <div className="text-sm font-medium text-gray-700 mb-2">Dossiers par catégorie</div>
          <div className="text-[11px] text-gray-400 mb-2">Vide = dossier Téléchargements par défaut.</div>
          {CATS.map((c) => (
            <div key={c.key} className="flex items-center gap-2 mb-1.5 text-sm">
              <span className="w-28 text-gray-600">{c.label}</span>
              <span className="flex-1 truncate bg-gray-100 rounded px-2 py-1 text-xs text-gray-500">
                {(dirs && dirs[c.key]) || "(Téléchargements)"}
              </span>
              <button className="px-2 py-1 rounded bg-gray-100 text-xs" onClick={() => pickDir(c.key)}>Parcourir…</button>
              {dirs && dirs[c.key] && (
                <button className="px-2 py-1 rounded bg-gray-100 text-xs" onClick={() => clearDir(c.key)}>✕</button>
              )}
            </div>
          ))}
        </div>
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-gray-100">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>Fermer</button>
          <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold" onClick={save}>Enregistrer</button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 8: `App.tsx` (filtres + actions)**

Remplacer `src/App.tsx` par :
```tsx
import { useEffect, useState } from "react";
import "./index.css";
import type { Download, Priority, Category, DownloadStatus } from "./types";
import * as api from "./api";
import { filterDownloads } from "./filter";
import { Toolbar } from "./components/Toolbar";
import { CategoryTabs } from "./components/CategoryTabs";
import { AddUrlDialog } from "./components/AddUrlDialog";
import { DownloadTable } from "./components/DownloadTable";
import { SettingsDialog } from "./components/SettingsDialog";

const DEST_DIR = "";

export default function App() {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const [dialogOpen, setDialogOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [status, setStatus] = useState<DownloadStatus | "all">("all");

  async function refresh() {
    setDownloads(await api.listDownloads());
  }

  useEffect(() => {
    refresh();
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      setDownloads((list) =>
        list.map((d) =>
          d.id === p.id ? { ...d, downloaded_bytes: p.downloaded_bytes, status: p.status, total_bytes: p.total_bytes } : d,
        ),
      );
      if (p.status === "completed" || p.status === "paused" || p.status === "error") refresh();
    });
    return () => { un.then((f) => f()); };
  }, []);

  async function handleAdd(url: string) {
    setDialogOpen(false);
    setError(null);
    try {
      await api.addDownload(url, DEST_DIR);
      await refresh();
    } catch (e) {
      setError(`Impossible d'ajouter ce lien : ${String(e)}`);
    }
  }

  async function withRefresh(fn: () => Promise<void>) {
    try { await fn(); } catch (e) { setError(String(e)); }
    await refresh();
  }

  const visible = filterDownloads(downloads, { query, category, status });

  return (
    <div className="h-screen flex flex-col">
      <Toolbar
        query={query} onQuery={setQuery}
        status={status} onStatus={setStatus}
        onAdd={() => setDialogOpen(true)}
        onPauseAll={() => withRefresh(api.pauseAll)}
        onResumeAll={() => withRefresh(api.resumeAll)}
        onClearCompleted={() => withRefresh(api.clearCompleted)}
        onSettings={() => setSettingsOpen(true)}
      />
      <CategoryTabs active={category} onChange={setCategory} />
      {error && (
        <div className="flex items-start gap-2 px-3 py-2 bg-red-50 border-b border-red-200 text-sm text-red-700">
          <span className="flex-1 break-words">{error}</span>
          <button className="px-2 py-0.5 rounded bg-red-100 text-red-700 shrink-0" onClick={() => setError(null)}>✕</button>
        </div>
      )}
      <div className="flex-1 overflow-auto">
        <DownloadTable
          downloads={visible}
          speeds={speeds}
          onPause={(id) => withRefresh(() => api.pauseDownload(id))}
          onResume={(id) => withRefresh(() => api.startDownload(id))}
          onCancel={(id) => { api.cancelDownload(id); setDownloads((l) => l.filter((d) => d.id !== id)); }}
          onPriority={(id, p: Priority) => withRefresh(() => api.setPriority(id, p))}
          onOpenFile={(id) => withRefresh(() => api.openFile(id))}
          onOpenFolder={(id) => withRefresh(() => api.openFolder(id))}
          onRemove={(id) => { api.removeFromList(id).catch((e) => setError(String(e))); setDownloads((l) => l.filter((d) => d.id !== id)); }}
        />
      </div>
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
```

- [ ] **Step 9: Build + tests**

Run:
```bash
npm run build && npm test
```
Expected: build OK ; tests verts (format + filter).

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(ui): category tabs/column, search + status filter, row action menu, per-category dirs"
```

---

## Task 7 : Vérification (build complet + suites + smoke-test manuel)

**Files:** aucun nouveau.

- [ ] **Step 1: Suites automatisées**

Run:
```bash
cd src-tauri && cargo test -p download-core && cargo build
cd .. && npm test && npm run build
```
Expected: tout PASS, builds OK.

- [ ] **Step 2: Smoke-test manuel (humain)**

Lancer `npm run tauri dev`, puis :
1. Ouvre ⚙ Réglages → définis un dossier pour « Vidéos » (Parcourir…). Enregistre.
2. Ajoute une URL de fichier `.mp4` (ou `.mkv`) → il doit se télécharger dans le dossier Vidéos choisi, pastille « 🎬 Vidéos ».
3. Ajoute un `.zip` / `.pdf` → catégorie Archives / Documents, dossier Téléchargements (non configuré).
4. Onglets : clique « Vidéos » → seule la vidéo s'affiche. Recherche par nom. Filtre statut « Terminé ».
5. Sur un terminé : ⋯ → « Ouvrir le dossier » (l'explorateur s'ouvre), « Ouvrir le fichier ».
6. ⋯ → « Retirer de la liste » → la ligne disparaît mais le fichier reste sur le disque (vérifie). « Annuler » (✕) sur un autre → fichier supprimé.
7. 🧹 « Vider les terminés » → les terminés disparaissent de la liste (fichiers conservés).

Expected: comportements conformes. Noter tout écart (corriger en TDD côté cœur si logique, ou côté UI).

- [ ] **Step 3: Commit final**

```bash
git add -A
git commit -m "test: verify phase 2b categories, filters and history actions"
```

---

## Auto-revue (effectuée)

- **Couverture spec :** catégoriseur (Task 1) ; champ `category` + dossiers par catégorie + clear_completed (Task 2) ; routage à l'ajout + commandes ouvrir/retirer/vider + plugin dialog (Task 3) ; types/API (Task 4) ; filtrage pur testé (Task 5) ; onglets + colonne + recherche + filtre statut + menu d'actions + dossiers dans Réglages (Task 6) ; vérif (Task 7). ✔
- **Hors périmètre :** catégories perso, déplacement post-complétion (exclus). ✔
- **Cohérence des types :** `Category` (Rust lowercase) ↔ TS `"video"|...` ; `CategoryDirs` champs identiques ; `set_category_dir{category,path}`, `remove_from_list{id}`, `open_file/open_folder{id}` ↔ `api.ts` ; `filterDownloads` consommé par `App.tsx`. ✔
- **Pas de placeholder :** code complet à chaque step. ✔
- **Rappel respecté :** aucune nouvelle commande ne fait `tokio::spawn` (les commandes 2b ne lancent pas de tâches). ✔

## Notes / risques

- **API opener** (`open_path`/`reveal_item_in_dir`) : signatures à confirmer contre la version installée de `tauri-plugin-opener` ; adapter le type de l'argument `with` si besoin (Task 3, Step 4 note).
- **Permission dialog** : sans `"dialog:default"` dans `capabilities/default.json`, le bouton « Parcourir… » échouera silencieusement — bien l'ajouter (Task 3, Step 2).
- **Reste backlog 2a** (non traité ici) : fast-path lock-free du limiteur en illimité ; persistance périodique du statut `downloading` ; test e2e d'erreur.
