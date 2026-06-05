# Téléchargement par lots (batch) — Design

**Date :** 2026-06-05
**Statut :** validé (design approuvé par l'utilisateur)
**Module :** FlashGet (clone d'IDM, Tauri v2 — Rust + React/TS)

## Intention

Permettre de **coller plusieurs liens, un par ligne**, et de **tout ajouter d'un coup**
dans la file existante. Les téléchargements s'empilent en `Queued` et sont démarrés par
paquets par le scheduler déjà en place (limite `max_concurrent` + priorités). Le cœur du
travail est donc **UI + une commande qui boucle**, pas une nouvelle infra de file.

## Décisions (issues du brainstorm)

1. **Portée des liens : fichiers directs uniquement.** Pas de routage yt-dlp / boîte de
   variantes en lot. Une page HTML collée par erreur sera traitée comme un fichier direct.
2. **Emplacement UI : app principale.** Nouveau bouton « + Lot » dans la barre d'actions
   (`Toolbar`), à côté de « + Ajouter ». L'extension Chromium n'est pas modifiée.
3. **Sonde : ajout rapide, sonde différée.** On n'effectue **aucune** sonde HEAD à l'ajout :
   tout est créé en `Queued` instantanément. Le moteur (`engine::download`) re-sonde déjà
   l'URL au démarrage (`engine.rs:77`) → taille/segments/`Accept-Ranges` récupérés au lancement.
4. **Validation de ligne : `http(s)://` seulement.** Après `trim`, une ligne n'est gardée que
   si elle commence par `http://` ou `https://`. Les lignes vides sont ignorées sans être
   comptées ; les autres lignes non vides comptent comme « ignorées ».
5. **Déduplication : dans le lot seulement.** Doublons exacts à l'intérieur du texte collé
   retirés (ordre conservé). Pas de comparaison à la file existante (on peut re-télécharger
   un lien déjà présent).

## Architecture

Suit le pattern du projet : **logique pure dans le cœur Rust testable, Tauri = colle fine,
frontend = affichage seul.**

### 1. Cœur Rust — nouveau module `src-tauri/crates/download-core/src/batch.rs`

```rust
pub struct BatchParse {
    pub urls: Vec<String>,        // lignes gardées, ordre conservé, dédupliquées
    pub skipped_invalid: usize,   // lignes non vides qui ne sont pas http(s)://
    pub skipped_duplicate: usize, // occurrences http(s) répétées (au-delà de la 1re)
}

pub fn parse_batch(text: &str) -> BatchParse;
```

Comportement de `parse_batch` :
- Découpe par lignes (`lines()` gère `\n` et `\r\n`).
- `trim` chaque ligne ; ignore les lignes vides (non comptées).
- Garde une ligne seulement si elle commence par `http://` ou `https://`
  (insensible à la casse du schéma). Sinon → `skipped_invalid += 1`.
- Dédup exacte des URLs gardées (égalité de chaîne après `trim`), ordre d'apparition
  conservé ; chaque répétition → `skipped_duplicate += 1`.

Déclarer le module dans `lib.rs` (`pub mod batch;`).

**Tests unitaires (`cargo test`) :** texte vide ; mélange valide/invalide ; doublons exacts ;
espaces autour des URLs ; fins de ligne CRLF ; lignes vides intercalées ; schéma `HTTP://`
en majuscules ; ligne `ftp://`/texte brut comptée comme invalide.

### 2. Commande Tauri — `src-tauri/src/commands.rs`

```rust
#[derive(serde::Deserialize)]
pub struct AddBatchArgs { pub text: String, pub dest_dir: String }

#[derive(serde::Serialize)]
pub struct BatchSummary {
    pub added: usize,
    pub skipped_invalid: usize,
    pub skipped_duplicate: usize,
    pub failed: usize,            // créations échouées (ex. dossier inaccessible)
}

#[tauri::command]
pub async fn add_batch_downloads(app: AppHandle, args: AddBatchArgs)
    -> Result<BatchSummary, String>;
```

Helper interne `add_url_no_probe(app, url, dest_dir, headers) -> Result<Download, String>` :
reflète `add_url_with_probe` **sans la sonde**.
- `file_name` via `download_core::http::file_name_from_url(url)` avec repli (ex. `download`)
  passé à `naming::choose_file_name`.
- `total_bytes: None`, `supports_ranges: false`, `downloaded_bytes: 0`,
  `status: Queued`, `kind: Regular`, `priority: Normal`.
- Catégorie déduite du nom (`categorizer::categorize`) ; dossier de destination résolu comme
  dans `add_url_with_probe` (argument explicite > dossier de catégorie > dossier Téléchargements).
- `upsert` ; **ne pas** appeler `reevaluate` (la boucle s'en charge une seule fois).

`add_batch_downloads` :
1. `let parsed = download_core::batch::parse_batch(&args.text);`
2. Boucle sur `parsed.urls` : chaque création réussie → `added += 1` ; chaque échec →
   `failed += 1` (n'interrompt **pas** les suivantes).
3. `reevaluate(&app)` **une seule fois** après la boucle.
4. Retourne `BatchSummary { added, skipped_invalid: parsed.skipped_invalid,
   skipped_duplicate: parsed.skipped_duplicate, failed }`.

Enregistrer la commande dans le `invoke_handler` (`lib.rs`/`main.rs`).

**Note Tauri :** ne pas spawn manuellement ici ; `reevaluate` utilise déjà
`tauri::async_runtime::spawn` en interne.

**Test Tauri :** avec `FakeFetcher`, appeler `add_batch_downloads` sur un texte mêlant URLs
valides, doublons et lignes invalides ; vérifier les compteurs du `BatchSummary` et que les
`Download` créés sont en `Queued`/`Regular`.

### 3. Frontend (React/TS)

**`src/components/AddBatchDialog.tsx`** (nouveau) — calqué sur `AddUrlDialog.tsx` :
- Grand `<textarea>` (placeholder « Colle une URL par ligne… »).
- Bouton « Ajouter » (désactivé si texte vide après `trim`) ; Échap ferme ; clic hors-boîte ferme.
- `onAdd(text)` envoie le **texte brut** (le parsing vit dans le cœur Rust), puis ferme et vide.

**`src/components/Toolbar.tsx`** — nouveau bouton **« + Lot »** juste après « + Ajouter »,
nouvelle prop `onAddBatch`.

**`src/App.tsx`** :
- État `batchOpen`.
- `handleAddBatch(text)` : `await api.addBatchDownloads(text, DEST_DIR)`, puis `refresh()`,
  puis affiche un **bandeau d'info** (réutilise la zone du bandeau d'erreur, style « info ») :
  « N ajouté(s) · M ignoré(s) · X doublon(s) » (mentionner `failed` s'il est > 0).
- Branche `<AddBatchDialog open={batchOpen} .../>` et `onAddBatch={() => setBatchOpen(true)}`.

**`src/api.ts`** :
```ts
export interface BatchSummary {
  added: number; skipped_invalid: number; skipped_duplicate: number; failed: number;
}
export function addBatchDownloads(text: string, destDir: string): Promise<BatchSummary> {
  return invoke<BatchSummary>("add_batch_downloads", { args: { text, dest_dir: destDir } });
}
```

**Test Vitest :** `AddBatchDialog` rend le textarea et, à la validation, appelle `onAdd`
avec le texte brut saisi.

## Flux de données

1. Utilisateur clique « + Lot » → `AddBatchDialog` s'ouvre.
2. Il colle N lignes, valide → `api.addBatchDownloads(text, "")`.
3. Commande Tauri : `parse_batch` filtre/dédup → boucle `add_url_no_probe` (tout en `Queued`)
   → `reevaluate` une fois → renvoie `BatchSummary`.
4. `reevaluate` + scheduler démarrent jusqu'à `max_concurrent` téléchargements ; le reste
   reste en `Queued` et démarre au fur et à mesure que des slots se libèrent.
5. Chaque moteur sonde son URL au démarrage (taille/segments) ; une URL morte passe en
   `Error` dans la liste sans bloquer les autres.
6. Le frontend `refresh()` et affiche le bandeau de feedback.

## Gestion des erreurs

- **Ligne invalide / vide** : filtrée par `parse_batch`, comptée (`skipped_invalid`) ou ignorée
  (vide), jamais bloquante.
- **Échec de création** (ex. dossier de destination inaccessible) : `failed += 1`, la boucle
  continue.
- **URL morte / 404 / page HTML** : se manifeste au démarrage via le moteur (statut `Error`
  par téléchargement), comme un ajout unitaire. Aucune sonde à l'ajout.

## Limites assumées

- **Nom de fichier dérivé de l'URL** uniquement (pas de `Content-Disposition` en lot) →
  la catégorie est déduite de l'extension présente dans l'URL. Acceptable pour des fichiers
  directs.
- **Pas de routage yt-dlp en lot** : une page vidéo collée téléchargera le HTML brut. Le lot
  est destiné aux fichiers directs (décision 1).

## Hors périmètre (non fait ici)

- Champ multi-lignes dans le popup de l'extension Chromium (éventuel second temps).
- Choix de qualité / variantes par vidéo en lot.
- Déduplication contre la file existante.
- Sonde à l'ajout / feedback détaillé par ligne avec nom et taille.

## Réutilisation de l'existant

- File / scheduler : `scheduler::pick_to_start`, `reevaluate()`, `max_concurrent` — **inchangés**.
- Nommage : `naming::choose_file_name`, `http::file_name_from_url` — réutilisés.
- Catégorisation / dossiers : `categorizer::categorize`, `storage.category_dir` — réutilisés.
- Modèle : `Download` / `DownloadStatus::Queued` / `DownloadKind::Regular` — réutilisés.
