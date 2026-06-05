# Téléchargement par lots (batch) — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Permettre de coller plusieurs liens (un par ligne) et de tout ajouter d'un coup dans la file existante, sans sonder à l'ajout.

**Architecture:** Logique pure de parsing/filtrage/dédup dans le cœur Rust testable (`download_core::batch`). Commande Tauri fine `add_batch_downloads` qui boucle, crée chaque `Download` en `Queued` sans sonde (le moteur sonde au démarrage), puis appelle `reevaluate()` une seule fois. Frontend : bouton « + Lot » + boîte textarea + bandeau de feedback. Le scheduler existant (`pick_to_start`, `max_concurrent`) démarre les téléchargements par paquets — rien à changer côté file.

**Tech Stack :** Rust (crate `download-core`, Tauri v2), React 19 + TypeScript, Vitest + @testing-library/react.

**Branche de travail :** `phase-batch-download` (déjà créée, contient la spec). Tous les commits de ce plan vont sur cette branche. **Garde subagent : `git checkout phase-batch-download` avant chaque commit.**

**Référence spec :** `docs/superpowers/specs/2026-06-05-batch-download-design.md`

**Note sur les tests Tauri :** il n'existe pas de harnais pour tester une `#[tauri::command]` (besoin d'un `AppHandle` réel) ; comme `add_url_with_probe`, la commande et son helper sont de la colle non testée unitairement. **Toute la logique testable vit dans `parse_batch`** (cœur pur). La commande est vérifiée par compilation + clippy + smoke-test manuel.

---

## Structure des fichiers

**Créés :**
- `src-tauri/crates/download-core/src/batch.rs` — parsing pur (`parse_batch`, `BatchParse`) + tests.
- `src/components/AddBatchDialog.tsx` — boîte de dialogue textarea.
- `src/components/AddBatchDialog.test.tsx` — test du composant.

**Modifiés :**
- `src-tauri/crates/download-core/src/lib.rs` — déclarer `pub mod batch;`.
- `src-tauri/src/commands.rs` — `AddBatchArgs`, `BatchSummary`, helper `add_url_no_probe`, commande `add_batch_downloads`.
- `src-tauri/src/lib.rs` — enregistrer `commands::add_batch_downloads` dans `invoke_handler`.
- `src/api.ts` — `BatchSummary` + `addBatchDownloads`.
- `src/components/Toolbar.tsx` — prop `onAddBatch` + bouton « + Lot ».
- `src/App.tsx` — état `batchOpen`/`info`, handler `handleAddBatch`, bandeau de feedback, câblage de la boîte.

---

## Task 1 : Cœur — `parse_batch` (parsing/filtrage/dédup purs)

**Files:**
- Create: `src-tauri/crates/download-core/src/batch.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs:1-15` (ajouter la déclaration de module)
- Test: dans `src-tauri/crates/download-core/src/batch.rs` (module `#[cfg(test)]`)

- [ ] **Step 1 : Déclarer le module + écrire le fichier avec un stub et tous les tests**

Dans `src-tauri/crates/download-core/src/lib.rs`, ajouter après la ligne `pub mod ytdlp;` (ligne 15) :

```rust
pub mod batch;
```

Créer `src-tauri/crates/download-core/src/batch.rs` avec le stub (implémentation volontairement vide pour faire échouer les tests) et la suite de tests complète :

```rust
//! Parsing d'un collage multi-lignes (une URL par ligne) en liste d'URLs à télécharger en lot.

use std::collections::HashSet;

/// Résultat du parsing d'un texte collé.
#[derive(Debug, Default, PartialEq)]
pub struct BatchParse {
    /// URLs gardées, ordre d'apparition conservé, dédupliquées.
    pub urls: Vec<String>,
    /// Lignes non vides qui ne sont pas des URLs `http(s)://`.
    pub skipped_invalid: usize,
    /// Occurrences d'URLs `http(s)` répétées (au-delà de la première).
    pub skipped_duplicate: usize,
}

/// Vrai si `s` commence par `http://` ou `https://` (schéma insensible à la casse).
fn is_http_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Découpe `text` en lignes, `trim` chacune, ignore les lignes vides (non comptées),
/// garde uniquement les URLs `http(s)://` (sinon `skipped_invalid`), et déduplique
/// dans le lot en conservant l'ordre (répétitions → `skipped_duplicate`).
pub fn parse_batch(_text: &str) -> BatchParse {
    BatchParse::default() // STUB — remplacé au Step 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_empty_result() {
        assert_eq!(parse_batch(""), BatchParse::default());
    }

    #[test]
    fn blank_lines_are_ignored_not_counted() {
        let r = parse_batch("\n  \n\t\n");
        assert!(r.urls.is_empty());
        assert_eq!(r.skipped_invalid, 0);
        assert_eq!(r.skipped_duplicate, 0);
    }

    #[test]
    fn keeps_http_and_https_in_order() {
        let r = parse_batch("https://a.com/1.zip\nhttp://b.com/2.iso");
        assert_eq!(r.urls, vec!["https://a.com/1.zip", "http://b.com/2.iso"]);
        assert_eq!(r.skipped_invalid, 0);
        assert_eq!(r.skipped_duplicate, 0);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let r = parse_batch("   https://a.com/1.zip  \n\t http://b.com/2.iso\t");
        assert_eq!(r.urls, vec!["https://a.com/1.zip", "http://b.com/2.iso"]);
    }

    #[test]
    fn handles_crlf_line_endings() {
        let r = parse_batch("https://a.com/1.zip\r\nhttps://b.com/2.zip\r\n");
        assert_eq!(r.urls, vec!["https://a.com/1.zip", "https://b.com/2.zip"]);
    }

    #[test]
    fn non_http_lines_counted_invalid() {
        let r = parse_batch("ftp://x.com/f\nwww.y.com/z\njuste du texte\nhttps://ok.com/a.zip");
        assert_eq!(r.urls, vec!["https://ok.com/a.zip"]);
        assert_eq!(r.skipped_invalid, 3);
        assert_eq!(r.skipped_duplicate, 0);
    }

    #[test]
    fn duplicates_are_deduped_and_counted() {
        let r = parse_batch("https://a.com/1.zip\nhttps://a.com/1.zip\nhttps://a.com/1.zip");
        assert_eq!(r.urls, vec!["https://a.com/1.zip"]);
        assert_eq!(r.skipped_duplicate, 2);
        assert_eq!(r.skipped_invalid, 0);
    }

    #[test]
    fn uppercase_scheme_is_kept() {
        let r = parse_batch("HTTPS://a.com/1.zip\nHttp://b.com/2.zip");
        assert_eq!(r.urls, vec!["HTTPS://a.com/1.zip", "Http://b.com/2.zip"]);
        assert_eq!(r.skipped_invalid, 0);
    }

    #[test]
    fn mixed_realistic_input() {
        let text = "\
https://site.com/a.zip
   https://site.com/b.iso
https://site.com/a.zip
pas une url
ftp://nope.com/x

http://site.com/c.bin
";
        let r = parse_batch(text);
        assert_eq!(
            r.urls,
            vec![
                "https://site.com/a.zip",
                "https://site.com/b.iso",
                "http://site.com/c.bin",
            ]
        );
        assert_eq!(r.skipped_duplicate, 1); // le 2e a.zip
        assert_eq!(r.skipped_invalid, 2);   // "pas une url" + ftp
    }
}
```

- [ ] **Step 2 : Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core batch::`
Expected: ÉCHEC — la plupart des assertions échouent (le stub renvoie toujours un résultat vide, ex. `keeps_http_and_https_in_order` : `left: []`, `right: [...]`).

- [ ] **Step 3 : Implémenter `parse_batch`**

Remplacer le corps stub de `parse_batch` par :

```rust
pub fn parse_batch(text: &str) -> BatchParse {
    let mut out = BatchParse::default();
    let mut seen: HashSet<&str> = HashSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue; // ligne vide : ignorée, non comptée
        }
        if !is_http_url(line) {
            out.skipped_invalid += 1;
            continue;
        }
        if seen.insert(line) {
            out.urls.push(line.to_string());
        } else {
            out.skipped_duplicate += 1;
        }
    }
    out
}
```

- [ ] **Step 4 : Lancer les tests pour vérifier qu'ils passent**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core batch::`
Expected: PASS (9 tests verts).

- [ ] **Step 5 : Commit**

```bash
git checkout phase-batch-download
git add src-tauri/crates/download-core/src/batch.rs src-tauri/crates/download-core/src/lib.rs
git commit -m "feat(core): parse_batch — découpe/filtre http(s)/dédup pour l'ajout en lot

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2 : Commande Tauri `add_batch_downloads` + helper sans sonde

**Files:**
- Modify: `src-tauri/src/commands.rs` (ajouter structs + helper + commande, après `add_url_with_probe`/`add_download`, ~ligne 104)
- Modify: `src-tauri/src/lib.rs:27-48` (enregistrer la commande)

- [ ] **Step 1 : Ajouter les structs, le helper et la commande dans `commands.rs`**

Insérer juste après la fonction `add_download` (après la ligne 104) :

```rust
#[derive(serde::Deserialize)]
pub struct AddBatchArgs {
    pub text: String,
    pub dest_dir: String,
}

#[derive(serde::Serialize)]
pub struct BatchSummary {
    pub added: usize,
    pub skipped_invalid: usize,
    pub skipped_duplicate: usize,
    pub failed: usize,
}

/// Crée un `Download` en file SANS sonder l'URL (le moteur sondera au démarrage).
/// Le nom vient de l'URL ; taille/segments inconnus. Ne ré-évalue PAS la file
/// (l'appelant le fait une seule fois après le lot).
pub(crate) async fn add_url_no_probe(
    app: &AppHandle,
    url: &str,
    dest_dir: &str,
    headers: RequestHeaders,
) -> Result<Download, String> {
    let file_name = download_core::naming::choose_file_name(
        download_core::http::file_name_from_url(url).as_deref(),
        None,
    );
    let category = download_core::categorizer::categorize(&file_name);

    // Dossier : argument explicite > dossier configuré pour la catégorie > dossier Téléchargements.
    let dest_dir = if !dest_dir.trim().is_empty() {
        PathBuf::from(dest_dir)
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
        url: url.to_string(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
        kind: DownloadKind::Regular,
        audio_url: None,
        media_selection: None,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    Ok(download)
}

/// Ajoute en lot : parse le texte (une URL par ligne), crée chaque téléchargement
/// en `Queued` sans sonder, puis ré-évalue la file une seule fois. Une ligne en échec
/// (ex. dossier inaccessible) n'interrompt pas les suivantes.
#[tauri::command]
pub async fn add_batch_downloads(app: AppHandle, args: AddBatchArgs) -> Result<BatchSummary, String> {
    let parsed = download_core::batch::parse_batch(&args.text);
    let mut added = 0usize;
    let mut failed = 0usize;
    for url in &parsed.urls {
        match add_url_no_probe(&app, url, &args.dest_dir, RequestHeaders::default()).await {
            Ok(_) => added += 1,
            Err(e) => {
                eprintln!("batch: échec ajout {url}: {e}");
                failed += 1;
            }
        }
    }
    reevaluate(&app);
    Ok(BatchSummary {
        added,
        skipped_invalid: parsed.skipped_invalid,
        skipped_duplicate: parsed.skipped_duplicate,
        failed,
    })
}
```

- [ ] **Step 2 : Enregistrer la commande dans `invoke_handler`**

Dans `src-tauri/src/lib.rs`, ajouter une ligne après `commands::add_download,` (ligne 28) :

```rust
            commands::add_batch_downloads,
```

- [ ] **Step 3 : Vérifier la compilation**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compilation réussie (`Finished`), aucune erreur.

- [ ] **Step 4 : Vérifier tests + lint (0 warning)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: tous les tests passent (cœur + tauri), dont les 9 de `batch::`.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: aucun warning (le projet maintient 0 warning).

- [ ] **Step 5 : Commit**

```bash
git checkout phase-batch-download
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): commande add_batch_downloads (ajout en lot sans sonde, Queued)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3 : API frontend `addBatchDownloads`

**Files:**
- Modify: `src/api.ts` (ajouter le type + la fonction, à la suite de `addDownload`, ~ligne 36)

- [ ] **Step 1 : Ajouter le type `BatchSummary` et la fonction `addBatchDownloads`**

Dans `src/api.ts`, insérer juste après la fonction `addDownload` (après la ligne 36) :

```ts
export interface BatchSummary {
  added: number;
  skipped_invalid: number;
  skipped_duplicate: number;
  failed: number;
}

export function addBatchDownloads(text: string, destDir: string): Promise<BatchSummary> {
  return invoke<BatchSummary>("add_batch_downloads", { args: { text, dest_dir: destDir } });
}
```

- [ ] **Step 2 : Vérifier la compilation TypeScript**

Run: `npx tsc --noEmit`
Expected: aucune erreur de type.

- [ ] **Step 3 : Commit**

```bash
git checkout phase-batch-download
git add src/api.ts
git commit -m "feat(api): addBatchDownloads + type BatchSummary

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4 : Composant `AddBatchDialog` (TDD)

**Files:**
- Create: `src/components/AddBatchDialog.tsx`
- Test: `src/components/AddBatchDialog.test.tsx`

- [ ] **Step 1 : Écrire le test (qui échoue car le composant n'existe pas)**

Créer `src/components/AddBatchDialog.test.tsx` :

```tsx
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { AddBatchDialog } from "./AddBatchDialog";

afterEach(cleanup);

describe("AddBatchDialog", () => {
  it("ne rend rien quand fermé", () => {
    render(<AddBatchDialog open={false} onClose={vi.fn()} onAdd={vi.fn()} />);
    expect(screen.queryByLabelText("Liens à télécharger")).toBeNull();
  });

  it("transmet le texte brut multi-lignes à onAdd", () => {
    const onAdd = vi.fn();
    render(<AddBatchDialog open onClose={vi.fn()} onAdd={onAdd} />);
    const ta = screen.getByLabelText("Liens à télécharger");
    fireEvent.change(ta, { target: { value: "https://a.com/1.zip\nhttps://a.com/2.zip" } });
    fireEvent.click(screen.getByText("Ajouter"));
    expect(onAdd).toHaveBeenCalledWith("https://a.com/1.zip\nhttps://a.com/2.zip");
  });

  it("désactive Ajouter quand le texte est vide", () => {
    render(<AddBatchDialog open onClose={vi.fn()} onAdd={vi.fn()} />);
    const btn = screen.getByText("Ajouter") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });
});
```

- [ ] **Step 2 : Lancer le test pour vérifier qu'il échoue**

Run: `npx vitest run src/components/AddBatchDialog.test.tsx`
Expected: ÉCHEC — résolution impossible de `./AddBatchDialog` (le composant n'existe pas).

- [ ] **Step 3 : Implémenter le composant**

Créer `src/components/AddBatchDialog.tsx` :

```tsx
import { useState } from "react";

export function AddBatchDialog({
  open,
  onClose,
  onAdd,
}: {
  open: boolean;
  onClose: () => void;
  onAdd: (text: string) => void;
}) {
  const [text, setText] = useState("");
  if (!open) return null;

  function submit() {
    if (!text.trim()) return;
    onAdd(text);
    setText("");
  }

  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg p-5 w-[560px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold mb-1">Ajouter plusieurs liens</h3>
        <p className="text-xs text-gray-500 mb-3">
          Un lien par ligne. Seuls les liens http(s) directs sont ajoutés.
        </p>
        <textarea
          autoFocus
          aria-label="Liens à télécharger"
          className="w-full h-48 border border-gray-200 rounded-md px-3 py-2 text-sm font-mono resize-none"
          placeholder={"https://exemple.com/fichier1.zip\nhttps://exemple.com/fichier2.iso"}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submit();
          }}
        />
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold disabled:opacity-50"
            disabled={!text.trim()}
            onClick={submit}
          >
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4 : Lancer le test pour vérifier qu'il passe**

Run: `npx vitest run src/components/AddBatchDialog.test.tsx`
Expected: PASS (3 tests verts).

- [ ] **Step 5 : Commit**

```bash
git checkout phase-batch-download
git add src/components/AddBatchDialog.tsx src/components/AddBatchDialog.test.tsx
git commit -m "feat(ui): AddBatchDialog — boîte textarea multi-liens

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5 : Câblage Toolbar « + Lot » + App (handler + bandeau de feedback)

**Files:**
- Modify: `src/components/Toolbar.tsx` (prop `onAddBatch` + bouton)
- Modify: `src/App.tsx` (import, états `batchOpen`/`info`, handler, bandeau, câblage)

- [ ] **Step 1 : Ajouter la prop et le bouton dans `Toolbar.tsx`**

Dans la liste des props (après `onAdd: () => void;`, ligne 8) ajouter :

```tsx
  onAddBatch: () => void;
```

Et dans la destructuration des paramètres (après `onAdd,`, ligne 8 de la signature) ajouter `onAddBatch,`. Concrètement, le bloc props devient :

```tsx
export function Toolbar({
  query,
  onQuery,
  status,
  onStatus,
  onAdd,
  onAddBatch,
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
  onAddBatch: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onClearCompleted: () => void;
  onSettings: () => void;
}) {
```

Ajouter le bouton juste après le bouton « + Ajouter » (après la ligne 26) :

```tsx
      <button className="px-3 py-1.5 text-sm rounded-md bg-indigo-50 text-indigo-700 font-semibold border border-indigo-200" onClick={onAddBatch}>+ Lot</button>
```

- [ ] **Step 2 : Câbler dans `App.tsx`**

(a) Ajouter l'import (après la ligne 8 `import { AddUrlDialog } ...`) :

```tsx
import { AddBatchDialog } from "./components/AddBatchDialog";
```

(b) Ajouter les états (après la ligne 19 `const [dialogOpen, setDialogOpen] = useState(false);`) :

```tsx
  const [batchOpen, setBatchOpen] = useState(false);
  const [info, setInfo] = useState<string | null>(null);
```

(c) Ajouter le handler juste après `handleAdd` (après la ligne 77) :

```tsx
  async function handleAddBatch(text: string) {
    setBatchOpen(false);
    setError(null);
    setInfo(null);
    try {
      const s = await api.addBatchDownloads(text, DEST_DIR);
      await refresh();
      const parts = [`${s.added} ajouté(s)`];
      if (s.skipped_duplicate) parts.push(`${s.skipped_duplicate} doublon(s)`);
      if (s.skipped_invalid) parts.push(`${s.skipped_invalid} ignoré(s)`);
      if (s.failed) parts.push(`${s.failed} échec(s)`);
      setInfo(parts.join(" · "));
    } catch (e) {
      setError(`Échec de l'ajout en lot : ${String(e)}`);
    }
  }
```

(d) Passer la prop à `Toolbar` (après `onAdd={() => setDialogOpen(true)}`, ligne 91) :

```tsx
        onAddBatch={() => setBatchOpen(true)}
```

(e) Ajouter le bandeau d'info juste après le bloc bandeau d'erreur (après la ligne 103, le `)}` qui ferme `{error && (...)}`) :

```tsx
      {info && (
        <div className="flex items-start gap-2 px-3 py-2 bg-indigo-50 border-b border-indigo-200 text-sm text-indigo-700">
          <span className="flex-1 break-words">{info}</span>
          <button className="px-2 py-0.5 rounded bg-indigo-100 text-indigo-700 shrink-0" onClick={() => setInfo(null)}>✕</button>
        </div>
      )}
```

(f) Monter la boîte juste après `<AddUrlDialog .../>` (après la ligne 120) :

```tsx
      <AddBatchDialog open={batchOpen} onClose={() => setBatchOpen(false)} onAdd={handleAddBatch} />
```

- [ ] **Step 3 : Vérifier compilation + suite de tests frontend**

Run: `npx tsc --noEmit`
Expected: aucune erreur de type.

Run: `npm test`
Expected: toute la suite Vitest passe (incl. les 3 tests d'`AddBatchDialog`).

- [ ] **Step 4 : Commit**

```bash
git checkout phase-batch-download
git add src/components/Toolbar.tsx src/App.tsx
git commit -m "feat(ui): bouton « + Lot » + bandeau de feedback (N ajoutés · M doublons · X ignorés)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6 : Vérification finale + smoke-test manuel

**Files:** aucun (vérification).

- [ ] **Step 1 : Suite Rust complète + lint**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: tout vert (cœur + tauri).

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: 0 warning.

- [ ] **Step 2 : Build + suite frontend complète**

Run: `npm run build`
Expected: `tsc` + `vite build` réussissent.

Run: `npm test`
Expected: toute la suite Vitest verte.

- [ ] **Step 3 : Smoke-test manuel (GUI)**

Lancer l'app : `npm run tauri dev`

Scénario :
1. Cliquer « + Lot ». La boîte textarea s'ouvre.
2. Coller 4 lignes : deux URLs directes valides distinctes (ex. deux `.zip`/`.exe` directs joignables), la première URL **répétée** une fois, et une ligne invalide (ex. `pas une url`).
3. Valider « Ajouter ».
4. Vérifier le bandeau : « 2 ajouté(s) · 1 doublon(s) · 1 ignoré(s) ».
5. Vérifier dans la liste que 2 téléchargements apparaissent (en `Queued` puis `Downloading` selon `max_concurrent`), nom dérivé de l'URL, et qu'ils se lancent sans avoir eu de sonde à l'ajout.
6. Mettre `max_concurrent` à 1 dans les réglages, recoller un lot de 3 liens : vérifier qu'1 seul démarre et que les 2 autres restent en `Queued` puis s'enchaînent.

Expected: comptes du bandeau corrects, la file se remplit et démarre par paquets via le scheduler existant, aucune ligne invalide/doublon ne bloque les autres.

- [ ] **Step 4 : Terminaison de branche**

Une fois le smoke-test validé, utiliser la skill `superpowers:finishing-a-development-branch` pour décider de l'intégration (merge `--no-ff` dans `main` selon la convention du projet).

---

## Auto-revue du plan (couverture spec)

- **Décision 1 (fichiers directs only, pas de yt-dlp)** → `add_url_no_probe` force `kind: Regular` ; aucun routage HTML (Task 2). ✓
- **Décision 2 (UI app principale, bouton « + Lot »)** → Task 5. ✓
- **Décision 3 (ajout rapide sans sonde)** → `add_url_no_probe` sans `probe`, `total_bytes: None`, `supports_ranges: false` (Task 2) ; le moteur sonde au démarrage (`engine.rs:77`, inchangé). ✓
- **Décision 4 (validation http(s)://)** → `is_http_url` dans `parse_batch` (Task 1) + tests. ✓
- **Décision 5 (dédup dans le lot)** → `HashSet` dans `parse_batch` (Task 1) + test `duplicates_are_deduped_and_counted`. ✓
- **`reevaluate` une seule fois** → boucle puis `reevaluate(&app)` (Task 2). ✓
- **Feedback « N ajoutés · M ignorés · X doublons »** → bandeau info (Task 5). ✓
- **Réutilisation file/scheduler/nommage/catégorisation** → aucun changement de ces modules ; helper réutilise `choose_file_name`/`file_name_from_url`/`categorize`/`category_dir` (Task 2). ✓
- **Tests** : logique pure couverte par 9 tests `parse_batch` + 3 tests `AddBatchDialog`. Commande Tauri = colle non testée unitairement (pas de harnais `AppHandle` dans le projet ; même statut que `add_url_with_probe`), couverte par compilation/clippy/smoke. Écart au texte de la spec (qui évoquait un test Tauri `FakeFetcher`) **assumé et documenté** car non idiomatique ici et la commande n'utilise pas le fetcher. ✓
