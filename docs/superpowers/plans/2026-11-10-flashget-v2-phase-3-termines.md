# FlashGet v2 — Phase 3 : bibliothèque « Terminés » — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** remplacer la page « Terminés » provisoire par la bibliothèque de la maquette 04. Elle se présente en **liste groupée par jour** avec un **panneau de détail** à droite (lecteur intégré, infos, actions), ou en **grille de vignettes**, avec une bascule entre les deux (liste par défaut). Elle propose des filtres par catégorie et une recherche, ainsi que « Retirer de la liste » ou « Supprimer aussi le fichier » (envoyé à la corbeille).

**Architecture :** le moteur horodate chaque téléchargement : création, premier démarrage, fin. Trois commandes s'ajoutent :
- `get_file_info` dit si le fichier existe encore, sa taille, et s'il est lisible ;
- `allow_playback` autorise la lecture d'**un seul** fichier par le protocole d'actifs de Tauri, dont la portée est vide par défaut ;
- `delete_download` retire de la liste et, si on le demande, envoie le fichier à la corbeille.

Côté interface, des fonctions pures regroupent et filtrent les éléments, et des composants séparés affichent la liste, la grille, le panneau de détail et la confirmation de suppression.

**Tech Stack :** Tauri 2 (protocole d'actifs, `convertFileSrc`), React 19, TypeScript, Tailwind, Vitest, Rust (`rusqlite`, crate `trash`).

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md), §4.5, §8.1, §8.2. Maquette : [`../specs/2026-11-10-flashget-v2-maquettes/04-termines-file-planif.html`](../specs/2026-11-10-flashget-v2-maquettes/04-termines-file-planif.html), présentations 1 et 2.

**Feuille de route :** [`2026-11-10-flashget-v2-feuille-de-route.md`](2026-11-10-flashget-v2-feuille-de-route.md)

## Global Constraints

- Aucun code repris de NeoDLP, aucune mention d'un tiers (spec §2).
- Aucune capacité actuelle ne doit disparaître : ouvrir le fichier, ouvrir le dossier, retirer de la liste, vider la liste des terminés, recherche par nom.
- Bouton de bascule **Liste / Grille**, **Liste par défaut** ; le choix est mémorisé (spec §4.5).
- « Supprimer » propose « Retirer de la liste » (le fichier reste) et « Supprimer aussi le fichier » (**envoyé à la corbeille de Windows, jamais effacé définitivement**) — spec §8.2.
- Tout texte visible en français via `t()` ; chiffres, tailles et durées en JetBrains Mono ; couleurs via les jetons Tailwind existants, lisibles dans les deux thèmes.
- **Sécurité** : le protocole d'actifs n'autorise **aucun** fichier par défaut (`scope: []`). Seul le fichier d'un téléchargement terminé, demandé explicitement par `allow_playback`, est autorisé.
- `cargo` : toujours `-j 2` sur cette machine.
- Branche `flashget-v2-phase-3`, un commit par tâche.

## Review Focus

1. **Fichier déplacé ou supprimé hors de FlashGet** : le panneau affiche « Fichier introuvable », sans lecteur cassé ; « Ouvrir » est désactivé, « Dossier » ouvre le dossier parent. Tests : Task 2 `file_info_missing`, Task 5 `DetailPanel › fichier introuvable`.
2. **Format que le lecteur ne sait pas lire** (MKV, AVI, HEVC…) : pas de lecteur, une grande icône et « Ouvrir » dans le lecteur de Windows. Si la lecture échoue malgré tout (`onError`), le message `t("lib.cantPlay")` s'affiche. Tests : Task 2 `playable_kinds`, Task 5 `DetailPanel › erreur de lecture`.
3. **Corbeille indisponible** (fichier verrouillé, lecteur réseau) : l'erreur est affichée et l'élément reste dans la liste. Test : Task 2 `delete_keeps_row_when_trash_fails`.
4. **Ancien téléchargement sans date de fin** (base des phases 1 et 2) : il apparaît dans le groupe « Plus ancien », jamais « Invalid Date » ni « NaN ». Test : Task 3 `groupByDay › sans date`.
5. **Supprimer l'élément sélectionné** : la sélection passe à l'élément suivant (ou au précédent s'il était le dernier) ; le panneau ne reste jamais sur un élément disparu. Test : Task 6 `CompletedPage › sélection après suppression`.

---

## Décisions techniques

- **Dates** : colonnes `created_at`, `started_at` et `completed_at` (secondes Unix, nullables).
  - `created_at` est posée à la création.
  - `started_at` est posée au premier passage en « téléchargement », sans être écrasée ensuite.
  - `completed_at` est posée au passage en « terminé ».
  - La « durée du téléchargement » affichée vaut `completed_at − started_at`, pauses comprises.
- **Lisible dans le lecteur intégré** : vidéo `mp4`, `webm`, `m4v` ; audio `mp3`, `m4a`, `aac`, `ogg`, `opus`, `wav`, `flac`. Tout le reste (MKV, AVI…) affiche une icône et passe par « Ouvrir ».
- **Format affiché** : extension en majuscules, plus la qualité si on la connaît (hauteur du format yt-dlp), par exemple « MP4 · 1080p ». Sinon, l'extension seule.
- **« Re-télécharger »** ouvre la page Nouveau et relance l'analyse du lien source (`d.url`). On repasse par le choix du format, ce qui évite de recréer un téléchargement en double sans le vouloir.
- **« Copier le lien »** utilise `navigator.clipboard.writeText(d.url)` et affiche « Lien copié » dans le bandeau d'info.
- **Groupes** : « Aujourd'hui », « Hier », puis une date (`3 nov. 2026`, en français), et enfin « Plus ancien » pour les éléments sans `completed_at`. Le plus récent est en premier.
- **Catégories** : puces « Tout · n », puis une puce par catégorie présente (Vidéos, Musique, Documents, Programmes, Archives, Autres) avec son compteur. Une catégorie absente n'a pas de puce.
- **Vue mémorisée** : préférence `ui.completedView`, valeurs `list` ou `grid`, ajoutée à la liste blanche de `prefs.rs`.
- **« Vider la liste »** reste dans l'en-tête, en bouton secondaire : il retire les terminés de la liste sans toucher aux fichiers, comme aujourd'hui.

## Fichiers

| Fichier | Rôle |
|---|---|
| `download-core/src/model.rs`, `storage.rs` | colonnes et horodatage |
| `src-tauri/src/library.rs` (nouveau) | `FileInfo`, `playable_kind`, `get_file_info`, `allow_playback`, `delete_download` |
| `src-tauri/src/prefs.rs`, `tauri.conf.json`, `Cargo.toml`, `lib.rs` | préférence, protocole d'actifs, crate `trash` |
| `src/api.ts`, `src/types.ts` | types et appels |
| `src/library/library.ts` (nouveau) | regroupement, filtres, comptes, format et durées |
| `src/components/library/*.tsx` (nouveaux) | `CategoryChips`, `ViewToggle`, `LibraryList`, `LibraryGrid`, `DetailPanel`, `DeleteDialog` |
| `src/pages/CompletedPage.tsx` | réécrite |
| `src/pages/NewPage.tsx`, `src/App.tsx` | « Re-télécharger », copie du lien, suppression |

---

### Task 1 : Horodatage des téléchargements

**Files :** `download-core/src/model.rs`, `storage.rs` (+tests), chaque littéral `Download { … }` (≈ 7 sites dans `src-tauri/src/commands.rs`, `src-tauri/src/link.rs`, et les tests de `storage.rs` et `scheduler.rs`), `src/types.ts`.

**Interfaces :**
- `Download` gagne `#[serde(default)] pub created_at: Option<i64>`, `started_at: Option<i64>` et `completed_at: Option<i64>`, en secondes Unix.
- Stockage :
  - On ajoute trois `ALTER TABLE … ADD COLUMN … INTEGER`, sur le modèle existant (erreur ignorée au redémarrage).
  - `upsert`, `list` et `get_by_id` lisent et écrivent ces colonnes.
  - `upsert` pose `created_at = now` si la valeur reçue est `None`. Les sites de création n'ont donc rien à faire, mais ils peuvent passer `None`.
  - `mark_active` pose `started_at = COALESCE(started_at, now)`.
  - `update_state(.., Completed)` et `mark_completed` (s'il existe) posent `completed_at = COALESCE(completed_at, now)`.
  - Aucune autre transition ne touche aux dates.
  - `now` vient de `pub fn now_secs() -> i64` (`SystemTime` → secondes), défini dans `storage.rs`.
- TS : les mêmes trois champs, en `?: number | null`, sur `Download`.

- [ ] **Step 1 : Tests qui échouent** (`storage.rs`)

```rust
#[test] fn upsert_sets_created_at_once() { /* upsert sans created_at → Some(_) ; second upsert avec la valeur lue → inchangée */ }
#[test] fn mark_active_sets_started_at_once() { /* mark_active puis lecture → Some(t1) ; nouveau mark_active → toujours t1 */ }
#[test] fn completion_sets_completed_at() { /* update_state(Completed) → completed_at Some ; update_state(Paused) avant → None */ }
#[test] fn legacy_rows_have_no_dates() { /* ligne insérée en SQL brut sans les colonnes → les trois champs None, pas de panique */ }
```

- [ ] **Step 2 : Vérifier l'échec** — `cargo test … -p download-core -j 2 storage` → FAIL.
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2`, `npm test`, `npx tsc --noEmit` → tout passe.
- [ ] **Step 5 : Commit** — `feat(core): horodatage création, démarrage et fin des téléchargements`

---

### Task 2 : Infos fichier, lecture et suppression

**Files :** create `src-tauri/src/library.rs` (+tests) ; modify `src-tauri/src/lib.rs`, `src-tauri/src/prefs.rs` (+test), `src-tauri/Cargo.toml` (`tauri` feature `protocol-asset`, dépendance `trash = "5"`), `src-tauri/tauri.conf.json` (`app.security.assetProtocol = { "enable": true, "scope": [] }`).

**Interfaces :**
- `#[derive(Serialize, Clone, Copy, Debug, PartialEq)] #[serde(rename_all = "snake_case")] pub enum Playable { Video, Audio }`
- `pub fn playable_kind(path: &Path) -> Option<Playable>` : extension sans tenir compte de la casse. Vidéo : `mp4`, `webm`, `m4v`. Audio : `mp3`, `m4a`, `aac`, `ogg`, `opus`, `wav`, `flac`. Sinon `None`.
- `#[derive(Serialize)] pub struct FileInfo { pub exists: bool, pub size: Option<u64>, pub playable: Option<Playable> }`
- `pub fn file_info(path: &Path) -> FileInfo` (pure, testée avec `tempfile`) ; commande `#[tauri::command] pub fn get_file_info(app: AppHandle, id: String) -> Result<FileInfo, String>`.
- `#[tauri::command] pub fn allow_playback(app: AppHandle, id: String) -> Result<String, String>`.
  - Refus si le téléchargement n'est pas `Completed`, si le fichier n'existe pas, ou si `playable_kind` vaut `None`.
  - Sinon : `app.asset_protocol_scope().allow_file(&path)`, puis renvoie `dest_path`. Le front appelle ensuite `convertFileSrc(path)`.
- `pub fn delete_entry(path: &Path, delete_file: bool, trash: impl Fn(&Path) -> Result<(), String>) -> Result<(), String>` : fonction pure.
  - Si `delete_file` et que le fichier existe, appelle `trash(path)` ; une erreur est renvoyée telle quelle, préfixée de `"Impossible d'envoyer le fichier à la corbeille : "`.
  - Un fichier absent n'est pas une erreur.
- Commande `#[tauri::command] pub fn delete_download(app: AppHandle, id: String, delete_file: bool) -> Result<(), String>`.
  - Refus si le téléchargement est actif (même message que `remove_from_list`).
  - Puis `delete_entry(path, delete_file, |p| trash::delete(p).map_err(|e| e.to_string()))`.
  - Puis `storage.delete(id)`, **seulement si** l'étape précédente a réussi.
- `prefs.rs` : `UI_PREFS` gagne `("ui.completedView", &["list", "grid"])`.

- [ ] **Step 1 : Tests qui échouent**

```rust
#[test] fn playable_kinds() { assert_eq!(playable_kind(Path::new("a.MP4")), Some(Playable::Video)); assert_eq!(playable_kind(Path::new("a.opus")), Some(Playable::Audio)); assert_eq!(playable_kind(Path::new("a.mkv")), None); assert_eq!(playable_kind(Path::new("a")), None); }
#[test] fn file_info_existing() { /* fichier de 5 octets "x.mp3" → exists, size Some(5), playable Audio */ }
#[test] fn file_info_missing() { /* chemin absent → exists false, size None, playable None */ }
#[test] fn delete_entry_list_only_never_trashes() { /* delete_file false → la closure n'est pas appelée */ }
#[test] fn delete_entry_trashes_existing_file() { /* closure appelée une fois avec le chemin */ }
#[test] fn delete_entry_missing_file_is_ok() { /* fichier absent + delete_file → Ok sans appel */ }
#[test] fn delete_keeps_row_when_trash_fails() { /* closure Err("verrouillé") → Err commençant par "Impossible d'envoyer le fichier à la corbeille" */ }
// prefs.rs
#[test] fn completed_view_pref() { assert!(validate("ui.completedView", Some("grid")).is_ok()); assert!(validate("ui.completedView", Some("table")).is_err()); }
```

- [ ] **Step 2 : Vérifier l'échec.**
- [ ] **Step 3 : Implémenter**, puis enregistrer les 3 commandes dans `generate_handler!`.
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2` et `cargo build … -j 2` (le protocole d'actifs doit compiler).
- [ ] **Step 5 : Commit** — `feat: infos fichier, lecture autorisée par fichier et suppression vers la corbeille`

---

### Task 3 : Logique de la bibliothèque

**Files :** create `src/library/library.ts`, `src/library/library.test.ts` ; modify `src/api.ts`, `src/i18n/fr.ts`.

**Interfaces :**
- `api.ts` :
  - `type Playable = "video" | "audio"` ;
  - `interface FileInfo { exists: boolean; size: number | null; playable: Playable | null }` ;
  - `getFileInfo(id): Promise<FileInfo>` → `invoke("get_file_info", { id })` ;
  - `allowPlayback(id): Promise<string>` → `invoke("allow_playback", { id })` ;
  - `deleteDownload(id, deleteFile: boolean): Promise<void>` → `invoke("delete_download", { id, deleteFile })`.
- `library.ts` (pur ; `now` est passé en paramètre pour les tests) :
  - `type DayGroup = { key: string; label: string; items: Download[] }`
  - `groupByDay(items: Download[], now: Date): DayGroup[]` :
    - ne garde que les éléments `completed` ;
    - les trie par `completed_at` décroissant (à égalité, ordre inverse de la liste) ;
    - libellés `t("lib.today")`, `t("lib.yesterday")`, sinon `toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric" })` ;
    - les éléments sans `completed_at` vont en dernier, dans un groupe `t("lib.older")` ;
    - le calcul des jours se fait en **heure locale**.
  - `categoryCounts(items: Download[]): { category: Category; count: number }[]` : catégories présentes parmi les terminés, dans l'ordre vidéo, musique, document, programme, archive, autre.
  - `filterLibrary(items: Download[], category: Category | "all", query: string): Download[]` : ne garde que les terminés, filtre par catégorie et par nom (sans tenir compte de la casse).
  - `formatLabel(d: Download): string` : extension du `file_name` en majuscules, plus `" · {h}p"` si `media_selection` contient un `format_id` dont la hauteur est connue. Phase 3 : la hauteur n'est pas stockée, donc on affiche seulement l'extension (YAGNI ; le libellé est prévu pour l'accueillir plus tard).
  - `downloadDuration(d: Download): number | null` : `completed_at − started_at` si les deux existent et que la différence est ≥ 0, sinon `null`.
  - `formatDay(ts: number): string` (date et heure, ex. `"aujourd'hui à 14:32"` via `lib.todayAt`, sinon `"3 nov. 2026 à 14:32"`).
- Textes :

| Clé | Texte |
|---|---|
| `lib.today` | Aujourd'hui |
| `lib.yesterday` | Hier |
| `lib.older` | Plus ancien |
| `lib.todayAt` | aujourd'hui à {time} |
| `lib.dateAt` | {date} à {time} |
| `lib.all` | Tout · {n} |
| `catp.video` | Vidéos · {n} |
| `catp.music` | Musique · {n} |
| `catp.document` | Documents · {n} |
| `catp.program` | Programmes · {n} |
| `catp.archive` | Archives · {n} |
| `catp.other` | Autres · {n} |

- [ ] **Step 1 : Tests qui échouent**

```ts
const now = new Date(2026, 10, 10, 15, 0); // 10 nov. 2026 15:00 local
it("groupByDay : aujourd'hui, hier, date, plus ancien", () => {
  const g = groupByDay([c("a", at(2026,10,10,14,32)), c("b", at(2026,10,9,22,10)), c("c", at(2026,10,3,9,0)), c("d", null)], now);
  expect(g.map(x => x.label)).toEqual(["Aujourd'hui", "Hier", "3 nov. 2026", "Plus ancien"]);
});
it("groupByDay › sans date : jamais Invalid Date", () => { expect(JSON.stringify(groupByDay([c("d", null)], now))).not.toMatch(/Invalid|NaN/); });
it("groupByDay ignore les non terminés et trie du plus récent au plus ancien", () => { /* … */ });
it("categoryCounts ne liste que les catégories présentes", () => { /* … */ });
it("filterLibrary par catégorie et par nom", () => { /* … */ });
it("downloadDuration", () => { expect(downloadDuration(d({ started_at: 100, completed_at: 267 }))).toBe(167); expect(downloadDuration(d({ started_at: null, completed_at: 267 }))).toBeNull(); expect(downloadDuration(d({ started_at: 300, completed_at: 267 }))).toBeNull(); });
it("formatLabel", () => { expect(formatLabel(d({ file_name: "Concert.mp4" }))).toBe("MP4"); expect(formatLabel(d({ file_name: "README" }))).toBe(""); });
```

- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test` et `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(ui): logique de la bibliothèque (jours, catégories, durées)`

---

### Task 4 : Liste, grille, puces et bascule

**Files :** create `src/components/library/CategoryChips.tsx`, `ViewToggle.tsx`, `LibraryList.tsx`, `LibraryGrid.tsx`, `library.test.tsx` ; modify `src/i18n/fr.ts`.

**Interfaces :**
- `CategoryChips({ items: Download[]; value: Category | "all"; onChange(v): void })` : puces arrondies, l'active sur `surface` ; une icône `CategoryIcon` par catégorie.
- `type LibraryView = "list" | "grid"` ; `ViewToggle({ value: LibraryView; onChange(v): void })` : deux boutons icône (`Rows3`, `LayoutGrid`), `aria-label` `t("lib.viewList")` / `t("lib.viewGrid")`, `aria-pressed` sur le bouton actif.
- `LibraryList({ groups: DayGroup[]; selectedId: string | null; onSelect(id): void })` :
  - un en-tête de groupe (petites capitales, `ink-mute`) ;
  - une ligne par élément : vignette 64×38 (`thumbnail_url` ou tuile de catégorie, avec la durée en étiquette) ;
  - nom tronqué, puis méta (badge `formatLabel`, `hostOf(url)`), taille (`formatBytes(total_bytes ?? downloaded_bytes)`) et heure de fin `HH:MM` (ou `—`) ;
  - la ligne sélectionnée est sur fond accent pâle bordé d'accent (maquette 04) ;
  - les lignes sont des `button`.
- `LibraryGrid({ items: Download[]; selectedId; onSelect })` : grille `repeat(auto-fill, minmax(190px, 1fr))` ; carte = vignette 16:9 (même règle), nom, puis badge, taille et date courte.
- Textes : `lib.viewList` « Liste », `lib.viewGrid` « Grille ».

- [ ] **Step 1 : Tests qui échouent** :
  - puces : « Tout · 3 », « Vidéos · 2 », et un clic qui appelle `onChange("video")` ;
  - bascule : `aria-pressed` correct et clic sur « Grille » qui appelle `onChange("grid")` ;
  - liste : en-têtes de groupes, clic sur une ligne qui appelle `onSelect(id)`, ligne sélectionnée marquée `aria-current` ;
  - grille : la vignette `img` quand `thumbnail_url` est présent, la tuile d'icône sinon.
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): liste groupée, grille, filtres de catégorie et bascule`

---

### Task 5 : Panneau de détail et confirmation de suppression

**Files :** create `src/components/library/DetailPanel.tsx`, `DeleteDialog.tsx`, `detail.test.tsx` ; modify `src/i18n/fr.ts`.

**Interfaces :**
- `DetailPanel({ d: Download; onOpenFile(): void; onOpenFolder(): void; onCopyLink(): void; onRedownload(): void; onDelete(): void })`, largeur 320 px, maquette 04.
  - Au montage, et à chaque changement de `d.id`, appelle `api.getFileInfo(d.id)`.
    - **Fichier lisible** : appelle `api.allowPlayback(d.id)`, puis affiche `<video controls>` ou `<audio controls>` avec `src={convertFileSrc(path)}` (`@tauri-apps/api/core`). Pour l'audio, la vignette s'affiche au-dessus si elle existe.
    - **`onError` sur le média** : remplace le lecteur par `t("lib.cantPlay")`.
    - **Fichier introuvable** : `t("lib.missing")` à la place du lecteur, et « Ouvrir » désactivé.
    - **Fichier non lisible** : grande tuile `CategoryIcon`.
  - Titre (2 lignes au plus), puis une grille de libellés et valeurs :
    - Format = `formatLabel` ;
    - Taille = taille réelle `FileInfo.size`, sinon `total_bytes`, sinon `—` ;
    - Durée = `formatElapsed(duration_s)` ;
    - Terminé = `formatDay(completed_at)`, suivi de `· {formatElapsed(downloadDuration)}` si elle est connue ;
    - Dossier = dossier de `dest_path`, en mono, tronqué au début ;
    - Source = `url` tronquée, en `accent-fg`.
  - Actions :
    - **Ouvrir** (`cta`) ;
    - **Dossier** ;
    - **Copier le lien** ;
    - **Re-télécharger** ;
    - **Supprimer…** (style `err` discret).
- `DeleteDialog({ name: string; busy: boolean; error: string | null; onRemove(): void; onDeleteFile(): void; onCancel(): void })` : fenêtre modale.
  - Titre `t("lib.deleteTitle", { name })`.
  - Texte d'explication `t("lib.deleteHint")`.
  - Trois boutons : « Retirer de la liste », « Supprimer aussi le fichier » (en `err`) et « Annuler ».
  - L'erreur s'affiche au-dessus des boutons.
  - Échap ferme la fenêtre.
- Textes :

| Clé | Texte |
|---|---|
| `lib.format` | Format |
| `lib.size` | Taille |
| `lib.duration` | Durée |
| `lib.finished` | Terminé |
| `lib.folder` | Dossier |
| `lib.source` | Source |
| `lib.open` | Ouvrir |
| `lib.openFolder` | Dossier |
| `lib.copyLink` | Copier le lien |
| `lib.redownload` | Re-télécharger |
| `lib.delete` | Supprimer… |
| `lib.missing` | Fichier introuvable (déplacé ou supprimé) |
| `lib.cantPlay` | Lecture impossible ici — utilise « Ouvrir » |
| `lib.deleteTitle` | Supprimer « {name} » ? |
| `lib.deleteHint` | Le fichier peut rester sur le disque, ou partir à la corbeille de Windows. |
| `lib.removeOnly` | Retirer de la liste |
| `lib.deleteFile` | Supprimer aussi le fichier |
| `lib.cancel` | Annuler |
| `lib.copied` | Lien copié |

- [ ] **Step 1 : Tests qui échouent** (`vi.mock("../../api")` et `vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => "asset://" + p }))`) :
  - `vidéo lisible : balise video avec la source autorisée` ;
  - `audio : balise audio et vignette` ;
  - `fichier introuvable : message et Ouvrir désactivé` ;
  - `non lisible : pas de lecteur` ;
  - `erreur de lecture` (déclencher `fireEvent.error` sur la `video`) ;
  - `infos : taille réelle, durée, terminé avec durée du téléchargement` ;
  - `chaque action appelle son callback` ;
  - `DeleteDialog : trois boutons, erreur affichée, Échap annule`.
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): panneau de détail avec lecteur et confirmation de suppression`

---

### Task 6 : Page Terminés et intégration

**Files :** rewrite `src/pages/CompletedPage.tsx` (+ `src/pages/CompletedPage.test.tsx`, et retrait des cas « Terminés » de `pages.test.tsx` qui ne s'appliquent plus) ; modify `src/pages/NewPage.tsx` (+test), `src/App.tsx`.

**Interfaces :**
- `CompletedPage({ downloads: Download[]; onClear(): void; onOpenFile(id): void; onOpenFolder(id): void; onCopyLink(d: Download): void; onRedownload(url: string): void; onDeleted(): void })`.
  - **En-tête** :
    - titre ;
    - recherche ;
    - `ViewToggle` : valeur initiale lue avec `api.getUiPref("ui.completedView")`, `"list"` par défaut, enregistrée par `setUiPref` à chaque changement ;
    - bouton « Vider la liste ».
  - **Sous l'en-tête** : `CategoryChips`.
  - **Vue Liste** : `LibraryList` sur `groupByDay(filterLibrary(...), new Date())`, avec `DetailPanel` à droite.
  - **Vue Grille** : `LibraryGrid` ; le `DetailPanel` s'affiche à droite quand un élément est sélectionné (clic), et une croix le ferme.
  - **Sélection** :
    - en liste, le premier élément est sélectionné par défaut ;
    - si l'élément sélectionné disparaît (supprimé, filtré), la sélection passe au suivant dans l'ordre affiché, sinon au précédent, sinon à aucun.
  - **Vide** : `t("done.empty")`.
  - **Suppression** : `DeleteDialog`. « Retirer » appelle `api.deleteDownload(id, false)`, « Supprimer aussi le fichier » appelle `api.deleteDownload(id, true)`. En cas de succès, la fenêtre se ferme puis `onDeleted()` est appelé. En cas d'échec, l'erreur s'affiche dans la fenêtre et l'élément reste.
- `NewPage` : `ExternalEvent` gagne `{ kind: "analyze"; url: string }`, qui remplit le champ de lien et appelle `analyze(url)`.
- `App.tsx` :
  - `onRedownload(url)` fait `setPage("new")` et `showExternal({ kind: "analyze", url })` ;
  - `onCopyLink(d)` copie avec `navigator.clipboard.writeText(d.url)`, puis bandeau d'info `t("lib.copied")` ;
  - `onDeleted` appelle `refresh()`.

- [ ] **Step 1 : Tests qui échouent** (`CompletedPage.test.tsx`, avec api mockée) :
  - `liste par défaut avec panneau sur le premier élément` ;
  - `bascule en grille enregistrée (setUiPref ui.completedView grid)` ;
  - `préférence grid au démarrage` ;
  - `filtre catégorie et recherche` ;
  - `sélection après suppression` : supprimer l'élément sélectionné passe la sélection au suivant, puis au précédent quand c'était le dernier ;
  - `Supprimer aussi le fichier appelle deleteDownload(id, true) puis onDeleted` ;
  - `échec de suppression : erreur affichée, élément conservé` ;
  - `Re-télécharger appelle onRedownload(url)`.

  Dans `NewPage.test.tsx`, ajouter `external analyze remplit le champ et lance l'analyse`.
- [ ] **Step 2 : Vérifier l'échec.**
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — `npm test`, `npx tsc --noEmit`, `npm run build`, cargo workspace `-j 2`, `cargo build -j 2` → tout passe.
- [ ] **Step 5 : Commit** — `feat(ui): bibliothèque Terminés (liste, grille, détail, suppression)`
- [ ] **Step 6 : Vérification manuelle** (par le propriétaire, `npm run tauri dev`, après avoir quitté tout FlashGet lancé) :
  1. « Terminés » : groupes Aujourd'hui, Hier… ; les anciens éléments sont dans « Plus ancien ». Un MP4 se lit dans le panneau, un MP3 aussi ; un MKV affiche l'icône et « Ouvrir » fonctionne.
  2. Bascule en grille, fermer FlashGet puis le rouvrir : la grille est conservée.
  3. Supprimer → « Supprimer aussi le fichier » : le fichier est dans la Corbeille de Windows et l'élément disparaît. « Retirer de la liste » : le fichier reste sur le disque.
  4. Renommer un fichier terminé dans l'Explorateur : le panneau affiche « Fichier introuvable ».
  5. Re-télécharger : la page Nouveau s'ouvre et analyse le lien. Copier le lien : le bandeau affiche « Lien copié » et le collage fonctionne.
- [ ] **Step 7 : Fin de phase** — `superpowers:finishing-a-development-branch`.
