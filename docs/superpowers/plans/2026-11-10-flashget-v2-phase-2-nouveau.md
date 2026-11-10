# FlashGet v2 — Phase 2 : page « Nouveau » — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** remplacer les boîtes « Ajouter un lien », « Ajout groupé » et « Choix de variante » par la page « Nouveau » de la maquette 02 (option 2). On y analyse un lien, on choisit Vidéo + audio / Audio seul / Vidéo seule et le format, les sous-titres, la découpe, le dossier et le nom. Les boutons « Télécharger » et « Ajouter à la file » sont distincts, et les liens capturés par l'extension arrivent sur cette page.

**Architecture :** l'analyse devient une **commande qui répond directement** (`probe_link`), au lieu de passer par des événements globaux et une fausse erreur `__media_pending__`. Son résultat (`LinkProbe`) décrit soit un fichier simple, soit un média (infos + liste de formats). Une seconde commande, `confirm_link`, crée le téléchargement à partir de ce résultat et du choix de l'utilisateur. L'extension utilise la même analyse et en envoie le résultat à la page par l'événement `link://probed`. Un nouvel état `held` (« en attente de la file ») permet d'ajouter sans démarrer.

**Tech Stack :** Tauri 2, React 19, TypeScript 5.8, Tailwind 3, Vitest + Testing Library, Rust (`download-core`, `rusqlite`, `serde_json`), yt-dlp (binaire embarqué), `@tauri-apps/plugin-dialog` (déjà installé).

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md), §4.4, §7.1, §7.4, §8.5. Maquette de référence : [`../specs/2026-11-10-flashget-v2-maquettes/02-ajout-lien.html`](../specs/2026-11-10-flashget-v2-maquettes/02-ajout-lien.html), **option 2** et le bloc « fichier simple ».

**Feuille de route :** [`2026-11-10-flashget-v2-feuille-de-route.md`](2026-11-10-flashget-v2-feuille-de-route.md) · Phase précédente : [`2026-11-10-flashget-v2-phase-1-coque.md`](2026-11-10-flashget-v2-phase-1-coque.md)

## Global Constraints

- Aucun code, composant ou ressource repris de NeoDLP ; aucune mention d'un tiers (spec §2).
- Aucune capacité actuelle ne doit disparaître (spec §2) : choix de variante HLS/DASH/yt-dlp, découpe d'extrait, extraction MP3, ajout groupé (modes direct / vidéo / audio, playlists), cookies et connexion YouTube, en-têtes Referer/Cookie des captures, capture par l'extension.
- « Télécharger » démarre dès qu'une place se libère ; « Ajouter à la file » met l'élément **en attente** (état `held`) : il ne démarre que sur action de l'utilisateur, ou du planificateur en phase 4 (spec §7.1).
- La case « Démarrer tout de suite » n'existe pas (spec §8.5).
- Un lien capturé par l'extension ouvre la page « Nouveau », avec FlashGet au premier plan et le lien prérempli ; **aucune capture ne démarre de téléchargement sans confirmation** (spec §7.4).
- Tout texte visible est en français et passe par `t()` (`src/i18n/fr.ts`) ; JetBrains Mono pour les chiffres, tailles et durées.
- Couleurs uniquement via les jetons Tailwind existants (`bg`, `panel`, `field`, `surface`, `border`, `line`, `ink*`, `accent*`, `accent-fg`, `cta`, `cta-ink`, `ok`, `err`), lisibles dans les deux thèmes.
- `cargo` sur cette machine : toujours `-j 2` (sinon manque de mémoire).
- Travail sur la branche `flashget-v2-phase-2`, un commit par tâche.

## Review Focus

1. **Lien qui n'est ni un fichier ni une vidéo reconnue** (page web sans vidéo, site non pris en charge, lien mort) : la page affiche l'erreur et un bouton « Réessayer », sans rester bloquée sur le chargement ; rien n'est ajouté. Tests : Task 5 `probe_link_reports_error_for_page_without_video` (via `classify_probe`), Task 8 `useLinkProbe › erreur puis nouvelle analyse`.
2. **Fichier du même nom déjà présent sur le disque, ou déjà prévu par un autre téléchargement** : le nouveau reçoit `nom (2).ext`, jamais d'écrasement. Test : Task 4 `unique_file_name_*`.
3. **Deux analyses qui se chevauchent** (l'utilisateur colle un second lien avant la fin de la première analyse) : seul le résultat du dernier lien s'affiche. Test : Task 8 `useLinkProbe › ignore un résultat périmé`.
4. **Découpe invalide** (fin avant début, au-delà de la durée, texte non conforme) : message sous les champs et boutons désactivés. Tests : Task 8 `validateCut`, Task 9 `MediaPanel › découpe invalide`.
5. **MP3 rangé dans Musique, avec la catégorie Musique** (aujourd'hui la catégorie est « Vidéo » même pour un MP3) : le téléchargement audio a `category = music` et le dossier Musique. Test : Task 6 `confirm_audio_mp3_is_music`.

---

## Décisions techniques (prises pour ce plan)

- **Formats yt-dlp** : « Vidéo + audio » et « Vidéo seule » proposent une ligne par hauteur. À hauteur égale, on préfère H.264 (`avc1`), puis le débit le plus élevé. Le format **recommandé** est la plus grande hauteur ≤ 1080. « Audio seul » propose d'abord « MP3 (conversion) », qui est le recommandé, puis les formats audio natifs, un par extension, avec le meilleur débit. La sortie vidéo reste un `.mp4` (fusion actuelle) ; un audio natif garde son extension (`.m4a`, `.opus`, `.webm`).
- **HLS / DASH** : seulement « Vidéo + audio », une ligne par variante ; la découpe n'est proposée que pour yt-dlp (`can_cut`).
- **Sous-titres** : langues préférées `fr` puis `en`, parmi celles disponibles ; à défaut, la première disponible. Intégrés au fichier (`--embed-subs`). La liste des langues deviendra un réglage en phase 5.
- **Nommage** : si le fichier existe déjà, ou si un autre téléchargement vise le même chemin, on ajoute ` (2)`, ` (3)`…, comme le futur réglage « Renommer » de la phase 5, qui sera le comportement par défaut.
- **Après confirmation**, la page se vide et l'application passe sur « Téléchargements ».
- **Capture de l'extension pendant une saisie** : la nouvelle capture remplace le contenu de la page, puisque c'est l'action la plus récente de l'utilisateur.
- **Nombre de connexions** (spec §4.4, bloc « fichier simple ») : non affiché en phase 2, car le moteur utilise encore une valeur fixe (8). Le réglage « Connexions par fichier » de la phase 5 l'ajoutera, ici et dans les Paramètres.
- **Découpe** : dans `MediaChoice`, une fin vide signifie « jusqu'à la fin » ; la page la transmet comme `end: null` dans `ConfirmLink`.

## Fichiers

| Fichier | Rôle |
|---|---|
| `download-core/src/ytdlp.rs` | `MediaInfo`, `MediaFormat`, `parse_info`, `build_media_formats`, `YtdlpJob`, `job_args`, `pick_sub_langs`, `parse_clock`, `time_range` |
| `download-core/src/model.rs`, `storage.rs`, `scheduler.rs` | état `Held`, colonnes `thumbnail_url` et `duration_s` |
| `src-tauri/src/dest.rs` (nouveau) | `resolve_dest_dir`, `unique_file_name` |
| `src-tauri/src/link.rs` (nouveau) | `LinkProbe`, `PendingLink`, `probe_link`, `confirm_link`, événements `link://*` |
| `src-tauri/src/commands.rs`, `ytdlp.rs`, `state.rs`, `bridge/server.rs`, `lib.rs` | branchement, suppression de l'ancien chemin |
| `src/api.ts`, `src/types.ts` | types et appels |
| `src/newlink/selection.ts`, `src/newlink/useLinkProbe.ts` (nouveaux) | logique pure et état de l'analyse |
| `src/components/newlink/*.tsx` (nouveaux) | `FormatList`, `MediaPanel`, `FilePanel`, `DestinationFields`, `BatchPanel` |
| `src/pages/NewPage.tsx` (nouveau) | la page |
| `src/components/downloads/DownloadCard.tsx`, `src/downloads/tabs.ts`, `src/App.tsx` | vignette, état `held`, intégration |
| supprimés (Task 11) | `AddUrlDialog.tsx`, `AddBatchDialog.tsx` (+test), `HlsVariantDialog.tsx` (+test), `ProbingOverlay.tsx`, commandes `add_download` / `add_media_download`, événements `media://*` |

---

### Task 1 : Infos et formats yt-dlp

**Files :**
- Modify : `src-tauri/crates/download-core/src/ytdlp.rs` (types, parsing, tests)

**Interfaces :**
- Produces (tous `#[derive(Serialize, Clone, Debug, PartialEq)]`, `serde(rename_all = "snake_case")` sur les enums) :
  - `pub enum FormatKind { Av, Video, Audio }` → `"av" | "video" | "audio"`.
  - `pub struct MediaFormat { pub index: u32, pub kind: FormatKind, pub format_id: String, pub height: Option<u32>, pub label: String, pub container: Option<String>, pub codecs: Option<String>, pub fps: Option<f32>, pub est_size: Option<u64>, pub recommended: bool }`
  - `pub struct MediaInfo { pub title: String, pub uploader: Option<String>, pub thumbnail: Option<String>, pub duration_s: Option<u64>, pub upload_date: Option<String>, pub subtitle_langs: Vec<String>, pub can_cut: bool }` (+ `Default`). `upload_date` est au format `AAAA-MM-JJ`.
  - `YtdlpFormat` gagne `fps: Option<f32>` et `abr: Option<f64>`.
  - `pub fn parse_info(json: &str) -> Result<(MediaInfo, Vec<YtdlpFormat>), CoreError>` : `title` = `pick_name(...)` actuel ; `thumbnail` = champ `thumbnail` ; `duration` arrondi ; `upload_date` `"20261102"` → `"2026-11-02"` ; `subtitle_langs` = clés de `subtitles` (sous-titres manuels uniquement), triées ; `can_cut = true`.
  - `pub fn codec_label(codec: &str) -> String` : `avc1*`→`H.264`, `vp9`/`vp09*`→`VP9`, `av01*`→`AV1`, `hev1*`/`hvc1*`→`H.265`, `mp4a*`→`AAC`, `opus`→`Opus`, `vorbis`→`Vorbis`, sinon la valeur en majuscules avant le premier `.`.
  - `pub fn build_media_formats(formats: &[YtdlpFormat]) -> Vec<MediaFormat>` : ordre **Av** (hauteur décroissante), puis **Video** (mêmes hauteurs), puis **Audio** (MP3 d'abord). `index` = position dans ce vecteur.
    - Av / Video : une entrée par hauteur parmi les formats `vcodec != "none"` qui ont une hauteur, avec la préférence `avc1` puis `tbr` maximal. `format_id` = id du format vidéo ; `label` = `"{h}p"` ; `container` = extension en majuscules ; `fps` = celui du format.
    - Av : `codecs` = `"{vidéo} + {audio}"`, où l'audio est le meilleur `abr` parmi les formats audio seuls ; `est_size` = taille vidéo + taille de ce meilleur audio (`None` si l'une manque).
    - Video : `codecs` = codec vidéo, `est_size` = taille vidéo.
    - `recommended` = la plus grande hauteur ≤ 1080, dans Av et dans Video.
    - Audio : d'abord `{ format_id: "mp3", label: "MP3", container: Some("MP3"), codecs: Some("MP3 (conversion)"), est_size: None, recommended: true }`, puis un format natif par extension (`acodec != "none" && vcodec == "none"`, meilleur `abr`), avec `label` = `"{EXT} · {abr arrondi} kb/s"`, `container` = extension en majuscules et `codecs` = `codec_label(acodec)`.
- `select_variants` et `parse_formats` restent tels quels pour l'instant ; la Task 11 les supprime.

- [ ] **Step 1 : Tests qui échouent.** Ajouter un fixture JSON `const SAMPLE: &str` (en ligne dans le module de tests) avec : `title`, `uploader`, `thumbnail`, `duration: 768.4`, `upload_date: "20261102"`, `subtitles: {"en": [...], "fr": [...]}`, et les formats suivants :
  - `137` : 1080, avc1, mp4, filesize 1 000 000, tbr 4000, fps 60 ;
  - `248` : 1080, vp9, webm, tbr 5000 ;
  - `22` : 720, avc1, mp4, filesize 500 000 ;
  - `313` : 2160, vp9, webm, filesize 4 000 000 ;
  - `140` : audio m4a, mp4a, abr 129, filesize 100 000 ;
  - `251` : audio webm, opus, abr 160, filesize 120 000 ;
  - `250` : audio webm, opus, abr 70.

```rust
#[test] fn parse_info_reads_metadata() {
    let (info, f) = parse_info(SAMPLE).unwrap();
    assert_eq!(info.duration_s, Some(768));
    assert_eq!(info.upload_date.as_deref(), Some("2026-11-02"));
    assert_eq!(info.subtitle_langs, vec!["en", "fr"]);
    assert!(info.can_cut && info.thumbnail.is_some() && f.len() == 7);
}
#[test] fn av_formats_one_per_height_prefer_h264() {
    let (_, f) = parse_info(SAMPLE).unwrap();
    let av: Vec<_> = build_media_formats(&f).into_iter().filter(|m| m.kind == FormatKind::Av).collect();
    assert_eq!(av.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["2160p", "1080p", "720p"]);
    let p1080 = &av[1];
    assert_eq!(p1080.format_id, "137");
    assert_eq!(p1080.codecs.as_deref(), Some("H.264 + Opus")); // meilleur audio = 251 (abr 160)
    assert_eq!(p1080.est_size, Some(1_120_000));
    assert!(p1080.recommended && !av[0].recommended);
}
#[test] fn audio_formats_mp3_first_then_native_by_ext() {
    let (_, f) = parse_info(SAMPLE).unwrap();
    let au: Vec<_> = build_media_formats(&f).into_iter().filter(|m| m.kind == FormatKind::Audio).collect();
    assert_eq!(au[0].format_id, "mp3"); assert!(au[0].recommended);
    assert_eq!(au.iter().skip(1).map(|m| m.label.as_str()).collect::<Vec<_>>(), ["M4A · 129 kb/s", "WEBM · 160 kb/s"]);
}
#[test] fn indexes_are_positions() { /* build_media_formats(..).iter().enumerate() : index == position */ }
#[test] fn codec_labels() { assert_eq!(codec_label("avc1.640028"), "H.264"); assert_eq!(codec_label("vp09.00.40.08"), "VP9"); assert_eq!(codec_label("mp4a.40.2"), "AAC"); assert_eq!(codec_label("opus"), "Opus"); }
```

- [ ] **Step 2 : Vérifier l'échec** — `cargo test --manifest-path src-tauri/Cargo.toml -p download-core -j 2 ytdlp` → erreurs de compilation.
- [ ] **Step 3 : Implémenter** dans `ytdlp.rs` : `RawInfo` lit aussi `thumbnail`, `duration`, `upload_date` et `subtitles` (`Option<HashMap<String, serde_json::Value>>`), et les formats lisent `fps` et `abr`.
- [ ] **Step 4 : Vérifier** — `cargo test --manifest-path src-tauri/Cargo.toml --workspace -j 2` → tout `ok`.
- [ ] **Step 5 : Commit** — `feat(core): infos et formats détaillés yt-dlp (MediaInfo, MediaFormat)`

---

### Task 2 : Tâche yt-dlp paramétrée (mode, sous-titres, découpe)

**Files :**
- Modify : `src-tauri/crates/download-core/src/ytdlp.rs`, `src-tauri/src/ytdlp.rs` (`run_download` / `run_yt`), `src-tauri/src/commands.rs` (`launch` pour `Ytdlp`, `add_ytdlp_queued`)

**Interfaces :**
- Produces (download-core) :
  - `#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)] #[serde(rename_all = "snake_case")] pub enum JobMode { #[default] Av, Video, Audio }`
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct YtdlpJob { pub format_id: String, #[serde(default)] pub mode: JobMode, #[serde(default)] pub time_range: Option<String>, #[serde(default)] pub subs: Option<Vec<String>>, #[serde(default)] pub convert_mp3: bool }`
  - `pub fn job_from_selection(json: Option<&str>) -> YtdlpJob`. C'est la lecture de `Download.media_selection`, compatible avec les lignes existantes : `None` ou JSON illisible → `{ format_id: "bestvideo", mode: Av, … }`. Un JSON **sans** champ `mode` dont `format_id == "bestaudio"` → `mode: Audio, convert_mp3: true` ; sans `mode` sinon → `Av`.
  - `pub fn job_args(job: &YtdlpJob) -> Vec<String>` : l'option `-f`, puis les options de sortie, dans cet ordre.
    - `Av` : `-f "{id}+bestaudio/{id}/best"`, puis `--merge-output-format mp4` ; si `format_id == "bestvideo"` : `-f "bestvideo+bestaudio/best"`.
    - `Video` : `-f "{id}"`, puis `--remux-video mp4`.
    - `Audio` avec `convert_mp3` : `-f bestaudio`, puis `-x --audio-format mp3 --audio-quality 0`.
    - `Audio` sans conversion : `-f "{id}"`.
    - Puis, si `subs` n'est pas vide : `--write-subs --sub-langs {l1,l2} --embed-subs`.
    - Puis, si `time_range` : `--download-sections {range}`.
  - `pub fn pick_sub_langs(available: &[String], preferred: &[&str]) -> Vec<String>` : les préférées présentes, dans l'ordre des préférées ; à défaut, la première disponible ; vide si rien n'est disponible.
  - `pub const PREFERRED_SUB_LANGS: &[&str] = &["fr", "en"];`
  - `pub fn parse_clock(s: &str) -> Option<u64>` : `"90"`, `"1:30"`, `"01:02:03"` → secondes ; `None` si vide, invalide, ou si minutes ou secondes ≥ 60 dans les formes à deux-points.
  - `pub fn time_range(start_s: u64, end_s: Option<u64>) -> String` → `"*{start}-{end|inf}"`, en secondes entières (ex. `"*90-165"`, `"*0-inf"`).
- `src-tauri/src/ytdlp.rs::run_download` reçoit un `&YtdlpJob` à la place de `format_id` et `time_range`. `run_yt` construit `job_args(job)` au lieu de ses options `-f`, `-x` et `--merge-output-format` actuelles ; tout le reste (progression, `-o`, ffmpeg, cookies, `--no-playlist`, `--continue`, `--js-runtimes node`) est inchangé. `launch` lit le job avec `job_from_selection(d.media_selection.as_deref())`. `add_ytdlp_queued` (ajout groupé) écrit désormais un `YtdlpJob` complet : vidéo `{ format_id: "bestvideo", mode: Av }`, audio `{ format_id: "bestaudio", mode: Audio, convert_mp3: true }`.

- [ ] **Step 1 : Tests qui échouent** (download-core)

```rust
#[test] fn legacy_bestaudio_selection_is_mp3() { let j = job_from_selection(Some(r#"{"format_id":"bestaudio","time_range":null}"#)); assert_eq!((j.mode, j.convert_mp3), (JobMode::Audio, true)); }
#[test] fn legacy_video_selection_is_av() { let j = job_from_selection(Some(r#"{"format_id":"137","time_range":"*0-30"}"#)); assert_eq!(j.mode, JobMode::Av); assert_eq!(j.time_range.as_deref(), Some("*0-30")); }
#[test] fn missing_selection_is_best_av() { assert_eq!(job_from_selection(None).format_id, "bestvideo"); }
#[test] fn args_av_with_subs_and_cut() {
    let j = YtdlpJob { format_id: "137".into(), mode: JobMode::Av, time_range: Some("*90-165".into()), subs: Some(vec!["fr".into(), "en".into()]), convert_mp3: false };
    assert_eq!(job_args(&j), ["-f","137+bestaudio/137/best","--merge-output-format","mp4","--write-subs","--sub-langs","fr,en","--embed-subs","--download-sections","*90-165"]);
}
#[test] fn args_video_only() { /* ["-f","137","--remux-video","mp4"] */ }
#[test] fn args_mp3() { /* ["-f","bestaudio","-x","--audio-format","mp3","--audio-quality","0"] */ }
#[test] fn args_native_audio() { /* format_id "140", mode Audio, convert false → ["-f","140"] */ }
#[test] fn sub_langs() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(pick_sub_langs(&a(&["de","en","fr"]), PREFERRED_SUB_LANGS), ["fr","en"]);
    assert_eq!(pick_sub_langs(&a(&["de","it"]), PREFERRED_SUB_LANGS), ["de"]);
    assert!(pick_sub_langs(&[], PREFERRED_SUB_LANGS).is_empty());
}
#[test] fn clock() { assert_eq!(parse_clock("90"), Some(90)); assert_eq!(parse_clock("1:30"), Some(90)); assert_eq!(parse_clock("01:02:03"), Some(3723)); assert_eq!(parse_clock("1:75"), None); assert_eq!(parse_clock(""), None); assert_eq!(parse_clock("abc"), None); }
#[test] fn range() { assert_eq!(time_range(90, Some(165)), "*90-165"); assert_eq!(time_range(0, None), "*0-inf"); }
```

- [ ] **Step 2 : Vérifier l'échec** — `cargo test … -p download-core -j 2 ytdlp` → FAIL.
- [ ] **Step 3 : Implémenter**, puis adapter `src-tauri/src/ytdlp.rs` et `commands.rs`. Le téléchargement d'une ligne existante (base de données de la phase 1) doit produire exactement les mêmes options qu'avant pour `bestaudio` et pour un `format_id` vidéo : c'est ce que garantissent les tests de compatibilité.
- [ ] **Step 4 : Vérifier** — `cargo test … --workspace -j 2` → tout `ok`.
- [ ] **Step 5 : Commit** — `feat(core): tâche yt-dlp paramétrée (mode, sous-titres, découpe)`

---

### Task 3 : État « en attente de la file », vignette et durée

**Files :**
- Modify : `download-core/src/model.rs`, `storage.rs` (+tests), `scheduler.rs` (+test), chaque littéral `Download { … }` (≈ 9 sites : `commands.rs`, tests de `scheduler.rs` et `storage.rs`, `download-core/tests/*`), `src-tauri/src/commands.rs` (`start_download`)

**Interfaces :**
- Produces :
  - `DownloadStatus::Held` (serde `"held"`).
  - `Download` gagne `#[serde(default)] pub thumbnail_url: Option<String>` et `#[serde(default)] pub duration_s: Option<u64>`.
  - Stockage : `ALTER TABLE downloads ADD COLUMN thumbnail_url TEXT` et `ADD COLUMN duration_s INTEGER`, sur le modèle existant (erreur ignorée au redémarrage). `upsert`, `list` et `get_by_id` lisent et écrivent ces deux colonnes, et le statut `"held"`.
  - `scheduler::pick_to_start` ne démarre jamais un `Held`. C'est déjà le cas puisqu'il ne retient que `Queued`, et un test le verrouille.
  - `start_download` sur un `Held` le passe en `Queued` puis appelle `reevaluate` (le code actuel le fait déjà pour tout statut non terminé ; un test de commande n'est pas possible, on le vérifie à la Task 11). `resume_all` ne touche **pas** aux `Held`.

- [ ] **Step 1 : Tests qui échouent**

```rust
// storage.rs
#[test] fn held_and_media_fields_roundtrip() {
    let s = Storage::open_in_memory().unwrap();
    let mut d = sample("h1"); d.status = DownloadStatus::Held; d.thumbnail_url = Some("https://i.ytimg.com/x.jpg".into()); d.duration_s = Some(768);
    s.upsert(&d).unwrap();
    let back = s.get_by_id("h1").unwrap().unwrap();
    assert_eq!((back.status, back.thumbnail_url, back.duration_s), (DownloadStatus::Held, Some("https://i.ytimg.com/x.jpg".into()), Some(768)));
}
// scheduler.rs
#[test] fn never_starts_held() { /* [Held, Queued], max 3 → seul le Queued est renvoyé */ }
```
(`sample(id)` = l'aide de test existante du module, ou à créer si elle n'existe pas.)

- [ ] **Step 2 : Vérifier l'échec.** Côté TS, ajouter dans la même tâche `"held"` à `DownloadStatus`, et `thumbnail_url?: string | null` et `duration_s?: number | null` à `Download` (`src/types.ts`), puis vérifier `npx tsc --noEmit`.
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2`, `npm test`, `npx tsc --noEmit` → tout passe.
- [ ] **Step 5 : Commit** — `feat(core): état held, vignette et durée des téléchargements`

---

### Task 4 : Dossier de destination et noms uniques

**Files :**
- Create : `src-tauri/src/dest.rs` (+ tests en ligne)
- Modify : `src-tauri/src/commands.rs` (les 4 copies du choix de dossier), `src-tauri/src/lib.rs` (`mod dest;`)

**Interfaces :**
- Produces :
  - `pub fn resolve_dest_dir(explicit: Option<&str>, category_dir: Option<String>, downloads_dir: Option<PathBuf>) -> Result<PathBuf, String>` : un `explicit` non vide l'emporte, sinon le dossier de la catégorie, sinon `downloads_dir`, sinon `Err("aucun dossier de destination")`. C'est une fonction pure ; un adaptateur `pub fn dest_dir_for(app: &AppHandle, explicit: Option<&str>, category: Category) -> Result<PathBuf, String>` lit le stockage et `app.path().download_dir()`, puis appelle `create_dir_all`.
  - `pub fn unique_file_name(dir: &Path, name: &str, taken: &[String]) -> String` : renvoie `name` si `dir/name` n'existe pas et n'est pas dans `taken` (liste des `dest_path` des téléchargements existants). Sinon `"{stem} ({n}){.ext}"` avec le plus petit `n ≥ 2` libre. Les noms sans extension sont gérés.
- Les 4 sites de `commands.rs` (`add_url_with_probe`, `add_url_no_probe`, `add_ytdlp_queued`, `add_media_download`) utilisent `dest_dir_for` et `unique_file_name` (avec `taken` lu depuis `storage.list()`). `add_ytdlp_queued` respecte désormais le `dest_dir` de l'ajout groupé, s'il est fourni.

- [ ] **Step 1 : Tests qui échouent** (dans `dest.rs`, avec `tempfile` déjà présent en dépendance de développement de download-core ; l'ajouter aux `[dev-dependencies]` de `src-tauri` s'il n'y est pas)

```rust
#[test] fn explicit_dir_wins() { assert_eq!(resolve_dest_dir(Some("D:/X"), Some("D:/Cat".into()), Some("D:/Dl".into())).unwrap(), PathBuf::from("D:/X")); }
#[test] fn blank_explicit_falls_back_to_category_then_downloads() { /* Some("  ") + cat → cat ; None + None + dl → dl ; tout None → Err */ }
#[test] fn unique_file_name_keeps_free_name() { /* dossier vide → "a.mp4" */ }
#[test] fn unique_file_name_skips_existing_file() { /* crée a.mp4 et "a (2).mp4" → "a (3).mp4" */ }
#[test] fn unique_file_name_skips_planned_paths() { /* taken = [dir/a.mp4] (n'existe pas sur le disque) → "a (2).mp4" */ }
#[test] fn unique_file_name_without_extension() { /* "README" existant → "README (2)" */ }
```

- [ ] **Step 2 : Vérifier l'échec** — `cargo test … -p tauri-app -j 2 dest` → FAIL.
- [ ] **Step 3 : Implémenter**, puis remplacer les 4 copies.
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2` → tout `ok`.
- [ ] **Step 5 : Commit** — `refactor: dossier de destination partagé et noms uniques`

---

### Task 5 : Analyse d'un lien (`probe_link`) et captures de l'extension

**Files :**
- Create : `src-tauri/src/link.rs`
- Modify : `src-tauri/src/state.rs` (`pending_links`), `src-tauri/src/commands.rs` (les sondes HLS/DASH/yt-dlp exposent leur résultat au lieu d'émettre), `src-tauri/src/bridge/server.rs`, `src-tauri/src/lib.rs`

**Interfaces :**
- Produces (sérialisé vers le front) :

```rust
#[derive(Serialize, Clone, Debug)] #[serde(rename_all = "snake_case")]
pub enum MediaSource { Hls, Dash, Ytdlp }

#[derive(Serialize, Clone, Debug)] #[serde(tag = "type", rename_all = "snake_case")]
pub enum LinkProbe {
    File { probe_id: String, url: String, file_name: String, total_bytes: Option<u64>, supports_ranges: bool, category: Category, default_dir: String },
    Media { probe_id: String, url: String, source: MediaSource, info: MediaInfo, formats: Vec<MediaFormat>, file_name: String, video_dir: String, music_dir: String },
}
```

  - `pub struct PendingLink { pub probe: LinkProbe, pub headers: RequestHeaders, pub hls: Option<MasterPlaylist>, pub dash: Option<Mpd>, pub created: Instant }`, stocké dans `AppState.pending_links: Mutex<HashMap<String, PendingLink>>`. On évince à chaque insertion ce qui a plus de 30 min (même règle qu'aujourd'hui). `pending_media` disparaît à la Task 11.
  - `pub async fn analyze(app: &AppHandle, url: &str, headers: RequestHeaders, hint: ProbeHint) -> Result<LinkProbe, String>`, avec `pub enum ProbeHint { Auto, Hls, Dash, Page, Smart { fallback_url: Option<String>, fallback_kind: Option<String> } }`.
    - `Auto` : sonde HTTP ; si la réponse est du HTML, on passe à yt-dlp. Une URL en `.m3u8` ou un content-type HLS donne HLS ; `.mpd` donne DASH ; sinon c'est un fichier simple.
    - Les autres indices reprennent la logique actuelle de `probe_hls` / `probe_dash` / `probe_ytdlp` / `probe_smart`.
    - Pour yt-dlp : `parse_info` + `build_media_formats` (Task 1), avec les en-têtes et le repli sur les cookies YouTube conservés. Pour HLS/DASH : un `MediaFormat` `Av` par variante (`format_id` = index de la variante en texte, `label` = `"{h}p"`, `est_size` comme aujourd'hui, `recommended` = la variante de plus haut débit), avec `MediaInfo { title: file_name, can_cut: false, .. }`.
    - Les dossiers par défaut viennent de `dest_dir_for(app, None, …)` (Task 4) : catégorie du fichier, Vidéo et Musique.
    - Aucun formats, ou une erreur yt-dlp → `Err` avec un message en français (`"Aucune vidéo trouvée sur cette page."`, `"Lien inaccessible : {détail}"`).
  - `pub fn classify_probe(url: &str, content_type: Option<&str>) -> ProbeRoute` avec `pub enum ProbeRoute { File, Hls, Dash, Page }` : c'est la partie pure de `Auto`, testée.
  - Commande `#[tauri::command] pub async fn probe_link(app: AppHandle, url: String) -> Result<LinkProbe, String>` : `Auto`, sans en-têtes, puis enregistrement dans `pending_links`. **N'émet aucun événement et ne met pas la fenêtre au premier plan.**
  - Événements pour l'extension : `link://probing` `{ url: String }`, `link://probed` `LinkProbe`, `link://error` `{ url: String, message: String }`.
  - `bridge/server.rs` : **toute** capture authentifiée (y compris un fichier direct, sans `kind`) émet `link://probing`, met la fenêtre au premier plan, lance `analyze` avec ses en-têtes et l'indice correspondant à son `kind`, enregistre le résultat, puis émet `link://probed` (ou `link://error`). La réponse WebSocket reste `{"type":"ack", …}` / `{"type":"error", …}`. **Plus aucun `add_url` ni téléchargement démarré depuis une capture.**

- [ ] **Step 1 : Tests qui échouent** (purs, dans `link.rs`)

```rust
#[test] fn classify_html_is_page() { assert_eq!(classify_probe("https://www.youtube.com/watch?v=x", Some("text/html; charset=utf-8")), ProbeRoute::Page); }
#[test] fn classify_m3u8_is_hls() { assert_eq!(classify_probe("https://cdn.x/live/master.m3u8?t=1", Some("application/octet-stream")), ProbeRoute::Hls); assert_eq!(classify_probe("https://x/a", Some("application/vnd.apple.mpegurl")), ProbeRoute::Hls); }
#[test] fn classify_mpd_is_dash() { assert_eq!(classify_probe("https://x/manifest.mpd", None), ProbeRoute::Dash); }
#[test] fn classify_binary_is_file() { assert_eq!(classify_probe("https://releases.ubuntu.com/a.iso", Some("application/octet-stream")), ProbeRoute::File); }
#[test] fn link_probe_serializes_with_type_tag() { /* serde_json::to_value(LinkProbe::File{..})["type"] == "file" ; Media → "media", source "ytdlp" */ }
#[test] fn probe_link_reports_error_for_page_without_video() { /* fonction pure `media_or_error(info, formats: Vec<MediaFormat>) -> Result<(MediaInfo, Vec<MediaFormat>), String>` : formats vides → Err("Aucune vidéo trouvée sur cette page.") */ }
```

- [ ] **Step 2 : Vérifier l'échec.**
- [ ] **Step 3 : Implémenter**, puis enregistrer `probe_link` dans `generate_handler!`. Les anciennes commandes et les événements `media://*` restent en place jusqu'à la Task 11 ; l'ancien front ne reçoit donc plus les captures de l'extension entre les Tasks 5 et 11, ce qui est accepté.
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2` → tout `ok`.
- [ ] **Step 5 : Commit** — `feat: analyse de lien probe_link et captures de l'extension vers la page Nouveau`

---

### Task 6 : Confirmation (`confirm_link`) et ajout groupé en attente

**Files :**
- Modify : `src-tauri/src/link.rs`, `src-tauri/src/commands.rs` (`AddBatchArgs.hold`, `add_ytdlp_queued`, `add_url_no_probe`), `src-tauri/src/lib.rs`

**Interfaces :**
- Produces :

```rust
#[derive(Deserialize, Debug)] pub struct TimeCut { pub start: String, pub end: Option<String> }
#[derive(Deserialize, Debug)] pub struct ConfirmLink {
    pub probe_id: String,
    pub format_index: Option<u32>,   // obligatoire pour un média
    pub subtitles: bool,
    pub cut: Option<TimeCut>,        // yt-dlp seulement
    pub dest_dir: Option<String>,
    pub file_name: String,           // AVEC l'extension, telle qu'affichée à l'utilisateur
    pub start_now: bool,             // false = Held
}
```

  - `pub fn plan_download(probe: &LinkProbe, req: &ConfirmLink, now_id: String) -> Result<PlannedDownload, String>`, fonction **pure** testée, où `pub struct PlannedDownload { pub download: Download, pub category_for_dir: Category }`.
    - Pour un fichier : `kind: Regular`, la catégorie de la sonde, `total_bytes` et `supports_ranges` de la sonde.
    - Pour un média : on trouve `formats[format_index]` (absent ou hors bornes → `Err("Format introuvable")`). Pour yt-dlp, on construit un `YtdlpJob` : mode selon `kind` ; MP3 → `format_id "bestaudio"` + `convert_mp3` ; `subs` = `pick_sub_langs(info.subtitle_langs, PREFERRED_SUB_LANGS)` si `subtitles` ; la découpe via `parse_clock` + `time_range`. Une découpe invalide (`start` illisible, `end ≤ start`, ou `end > duration_s`) → `Err("Découpe invalide")`, et une découpe demandée sur HLS/DASH → `Err("Découpe non disponible pour ce flux")`. `media_selection = serde_json::to_string(&job)`.
    - Pour HLS : `url` = URI de la variante, `audio_url` choisi comme dans l'actuel `add_media_download`. Pour DASH : `media_selection = {"video": idx, "audio": null}`.
    - Catégorie : **Music** pour tout format `Audio` (MP3 compris), Video sinon.
    - `thumbnail_url` et `duration_s` viennent de `info`.
    - `status` : `Queued` si `start_now`, sinon `Held`.
    - `file_name` : `req.file_name` passé par `sanitize_title`, avec l'extension obligatoire selon le format (`.mp3` pour MP3, extension native pour un audio natif, `.mp4` sinon) imposée par `ensure_extension`.
    - En-têtes : ceux de la sonde.
  - Commande `#[tauri::command] pub async fn confirm_link(app: AppHandle, req: ConfirmLink) -> Result<Download, String>`.
    - Lit la sonde (sans la retirer) puis appelle `plan_download`.
    - Construit `dest_path` avec `dest_dir_for` et `unique_file_name` (Task 4).
    - Fait l'`upsert`, puis **retire la sonde** (seulement en cas de succès), puis appelle `reevaluate` si `start_now`.
    - Sonde inconnue ou expirée → `Err("Analyse expirée, relance l'analyse.")`.
  - `AddBatchArgs` gagne `#[serde(default)] pub hold: bool`. En mode `hold`, tous les téléchargements créés par le lot sont `Held`, et `reevaluate` n'est appelé que si `!hold`.

- [ ] **Step 1 : Tests qui échouent** (purs, `link.rs`, avec des sondes construites à la main)

```rust
#[test] fn confirm_file_queued_or_held() { /* File + start_now true → Queued, kind Regular ; false → Held */ }
#[test] fn confirm_av_builds_job() { /* Media ytdlp, format Av "137", subtitles true, info.subtitle_langs ["en","fr"] → job { mode Av, subs ["fr","en"] }, category Video, nom finit par ".mp4" */ }
#[test] fn confirm_audio_mp3_is_music() { /* format Audio "mp3" → job { format_id "bestaudio", convert_mp3 true }, category Music, ".mp3" */ }
#[test] fn confirm_native_audio_keeps_ext() { /* format Audio "140" container "M4A" → ".m4a", convert_mp3 false */ }
#[test] fn confirm_cut_valid_and_invalid() { /* cut 1:30→2:45 avec duration 768 → "*90-165" ; end 0:10 < start → Err ; end 20:00 > durée → Err ; cut sur source Hls → Err */ }
#[test] fn confirm_media_requires_format() { /* format_index None ou 99 → Err("Format introuvable") */ }
#[test] fn confirm_copies_thumbnail_and_duration() { /* … */ }
```

- [ ] **Step 2 : Vérifier l'échec.**
- [ ] **Step 3 : Implémenter**, puis enregistrer `confirm_link`.
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 2` → tout `ok`.
- [ ] **Step 5 : Commit** — `feat: confirm_link (format, sous-titres, découpe, file d'attente) et lot en attente`

---

### Task 7 : API front, vignettes et état « en attente » dans les listes

**Files :**
- Modify : `src/api.ts`, `src/components/downloads/DownloadCard.tsx` (+test), `src/downloads/tabs.ts` (+test), `src/App.tsx` (calcul `queued` de la barre d'état uniquement), `src/i18n/fr.ts`

**Interfaces :**
- Produces (TS, miroir exact du Rust) :
  - `type FormatKind = "av" | "video" | "audio"`, `interface MediaFormat { index: number; kind: FormatKind; format_id: string; height: number | null; label: string; container: string | null; codecs: string | null; fps: number | null; est_size: number | null; recommended: boolean }`, `interface MediaInfo { title: string; uploader: string | null; thumbnail: string | null; duration_s: number | null; upload_date: string | null; subtitle_langs: string[]; can_cut: boolean }`.
  - `type LinkProbe = { type: "file"; probe_id: string; url: string; file_name: string; total_bytes: number | null; supports_ranges: boolean; category: Category; default_dir: string } | { type: "media"; probe_id: string; url: string; source: "hls" | "dash" | "ytdlp"; info: MediaInfo; formats: MediaFormat[]; file_name: string; video_dir: string; music_dir: string }`.
  - `interface ConfirmLink { probe_id: string; format_index: number | null; subtitles: boolean; cut: { start: string; end: string | null } | null; dest_dir: string | null; file_name: string; start_now: boolean }`.
  - `probeLink(url: string): Promise<LinkProbe>` → `invoke("probe_link", { url })` ; `confirmLink(req: ConfirmLink): Promise<Download>` → `invoke("confirm_link", { req })` ; `onLinkProbing(cb: (e: { url: string }) => void)`, `onLinkProbed(cb: (p: LinkProbe) => void)`, `onLinkError(cb: (e: { url: string; message: string }) => void)` → `listen` sur `link://probing|probed|error` ; `addBatchDownloads(text, destDir, mode, hold = false)` envoie `{ args: { text, dest_dir, mode, hold } }`.
- `DownloadCard` :
  - Si `d.thumbnail_url` est présent, la vignette est une `<img>` (`object-cover`, `alt=""`, `loading="lazy"`) ; en cas d'erreur de chargement (`onError`), retour à l'icône de catégorie.
  - Si `d.duration_s` est présent, la durée s'affiche en étiquette (`formatElapsed`), comme dans la maquette 01.
  - Un `held` affiche `t("dl.held")` = `"en attente de la file"` dans la méta et à droite, avec la barre en ton `queued`, et le bouton principal Reprendre → `onResume` (qui appelle `start_download`).
- `tabs.ts` : `held` compte dans `all` (et seulement là).
- `App.tsx` : le `queued` de la barre d'état = `queued` + `held`.

- [ ] **Step 1 : Tests qui échouent** — `DownloadCard.test.tsx` : `vignette image quand thumbnail_url`, `retour à l'icône si l'image échoue` (déclencher `fireEvent.error`), `durée affichée`, `held : libellé et bouton Reprendre → onResume`. `tabs.test.ts` : un `held` est compté dans `all`, pas dans `active`.
- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/components/downloads src/downloads`.
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — `npm test`, `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(ui): API page Nouveau, vignettes et état en attente`

---

### Task 8 : Logique de la page Nouveau

**Files :**
- Create : `src/newlink/selection.ts`, `src/newlink/selection.test.ts`, `src/newlink/useLinkProbe.ts`, `src/newlink/useLinkProbe.test.tsx`

**Interfaces :**
- Produces (`selection.ts`, pur) :
  - `availableKinds(formats: MediaFormat[]): FormatKind[]`, dans l'ordre `av`, `audio`, `video`, sans les types vides (ordre des onglets de la maquette).
  - `formatsOf(formats, kind): MediaFormat[]` ; `defaultFormat(formats, kind): MediaFormat | undefined` (le recommandé, sinon le premier).
  - `outputExtension(f: MediaFormat): string` : `".mp3"` si `format_id === "mp3"` ; extension native en minuscules si `kind === "audio"` ; `".mp4"` sinon.
  - `withExtension(stem: string, ext: string): string` : remplace une extension connue (`.mp4`, `.mp3`, `.m4a`, `.webm`, `.opus`) ou en ajoute une.
  - `parseClock(s: string): number | null` : même règle que le Rust `parse_clock`.
  - `validateCut(start: string, end: string, durationS: number | null): "cut.badStart" | "cut.badEnd" | "cut.endBeforeStart" | "cut.beyondEnd" | null` (`end` vide = jusqu'à la fin).
  - `formatDuration(s: number): string` → `formatElapsed`.
- Produces (`useLinkProbe.ts`) :
  - `type ProbeState = { status: "idle" } | { status: "probing"; url: string } | { status: "error"; url: string; message: string } | { status: "ready"; probe: LinkProbe }`
  - `useLinkProbe(): { state: ProbeState; analyze(url: string): void; showProbing(url: string): void; load(probe: LinkProbe): void; fail(url: string, message: string): void; reset(): void }`
    - `analyze` appelle `api.probeLink`. Chaque appel reçoit un jeton : un résultat dont le jeton n'est plus le dernier est **ignoré**.
    - Délai de 90 s → `fail(url, t("new.timeout"))`.
    - `analyze` d'une URL vide ou qui ne commence pas par `http://` ou `https://` → `fail(url, t("new.badUrl"))` sans appel.
    - `load`, `showProbing` et `fail` servent aux événements de l'extension et invalident l'analyse en cours.
- Textes : `"new.timeout": "L'analyse a pris trop de temps. Réessaie."`, `"new.badUrl": "Colle un lien qui commence par http:// ou https://"`, `"cut.badStart": "Début invalide (ex. 1:30)"`, `"cut.badEnd": "Fin invalide (ex. 2:45)"`, `"cut.endBeforeStart": "La fin doit être après le début"`, `"cut.beyondEnd": "La fin dépasse la durée de la vidéo"`.

- [ ] **Step 1 : Tests qui échouent**

```ts
// selection.test.ts
it("onglets dans l'ordre et sans vides", () => { expect(availableKinds([av, au])).toEqual(["av", "audio"]); });
it("format par défaut = recommandé", () => { expect(defaultFormat([a720, a1080rec], "av")?.label).toBe("1080p"); });
it("extensions de sortie", () => { expect(outputExtension(mp3)).toBe(".mp3"); expect(outputExtension(m4a)).toBe(".m4a"); expect(outputExtension(v1080)).toBe(".mp4"); });
it("withExtension remplace ou ajoute", () => { expect(withExtension("Clip.mp4", ".mp3")).toBe("Clip.mp3"); expect(withExtension("Clip", ".mp4")).toBe("Clip.mp4"); expect(withExtension("v1.2 final", ".mp4")).toBe("v1.2 final.mp4"); });
it("validateCut", () => {
  expect(validateCut("1:30", "2:45", 768)).toBeNull();
  expect(validateCut("1:30", "", 768)).toBeNull();
  expect(validateCut("x", "", 768)).toBe("cut.badStart");
  expect(validateCut("2:00", "1:00", 768)).toBe("cut.endBeforeStart");
  expect(validateCut("0:00", "20:00", 768)).toBe("cut.beyondEnd");
});
// useLinkProbe.test.tsx (vi.mock("../api"), fake timers)
it("analyse puis prêt", async () => { /* probeLink résout un File → state.status "ready" */ });
it("ignore un résultat périmé", async () => { /* analyze(A) lent, analyze(B) rapide → reste sur B même quand A résout ensuite */ });
it("erreur puis nouvelle analyse", async () => { /* probeLink rejette "Aucune vidéo trouvée sur cette page." → status error avec ce message ; analyze à nouveau → probing */ });
it("délai dépassé", async () => { /* promesse jamais résolue, avancer de 90 s → error "L'analyse a pris trop de temps. Réessaie." */ });
it("URL invalide sans appel", () => { /* analyze("ftp://x") → error badUrl, probeLink non appelé */ });
it("load depuis l'extension remplace une analyse en cours", async () => { /* … */ });
```

- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/newlink`.
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — `npm test`, `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(ui): logique de la page Nouveau (sélection, découpe, analyse)`

---

### Task 9 : Panneaux média et fichier

**Files :**
- Create : `src/components/newlink/FormatList.tsx`, `MediaPanel.tsx`, `FilePanel.tsx`, `DestinationFields.tsx`, `newlink.test.tsx`
- Modify : `src/i18n/fr.ts`

**Interfaces :**
- Consumes : Task 7 (types), Task 8 (`selection.ts`).
- Produces :
  - `FormatList({ formats: MediaFormat[]; selected: number; onSelect(index: number): void })` : une ligne par format (bouton radio, `label` en mono gras, `container · codecs` tronqué, badge `t("new.recommended")` si recommandé, taille estimée `formatBytes` ou `—`, `fps` arrondi suivi de `" i/s"` si présent). La ligne sélectionnée est bordée d'`accent`, comme dans la maquette.
  - `DestinationFields({ dir: string; fileName: string; onDir(d: string): void; onFileName(n: string): void })` : deux champs (« Enregistrer dans » avec un bouton « Parcourir » qui appelle `open({ directory: true, defaultPath: dir })` de `@tauri-apps/plugin-dialog` et ignore l'annulation, et « Nom du fichier »).
  - `type MediaChoice = { formatIndex: number; subtitles: boolean; cut: { start: string; end: string } | null; dir: string; fileName: string }`
  - `MediaPanel({ probe: Extract<LinkProbe, { type: "media" }>; busy: boolean; onConfirm(choice: MediaChoice, startNow: boolean): void })`.
    - Disposition de la maquette 02, option 2.
    - **Gauche** :
      - aperçu (`info.thumbnail` en image, sinon un fond dégradé avec l'icône lecture) avec la durée en étiquette ;
      - titre, puis `uploader · site` (`hostOf(probe.url)`) et date ;
      - badges : nombre de formats, et `t("new.subsAvailable", { langs })` si des sous-titres existent ;
      - options : interrupteur « Intégrer les sous-titres » (désactivé sans sous-titres disponibles), interrupteur « Découper un extrait » (affiché seulement si `can_cut`), qui révèle deux champs Début / Fin avec le message `validateCut` en `text-err`.
    - **Droite** :
      - onglets `availableKinds`, avec les libellés « Vidéo + audio », « Audio seul » et « Vidéo seule » ;
      - `FormatList` des formats de l'onglet ;
      - `DestinationFields` : le dossier par défaut est `music_dir` pour l'onglet audio, `video_dir` sinon, tant que l'utilisateur n'a pas choisi de dossier lui-même. Le nom vaut `withExtension(probe.file_name, outputExtension(format))` et suit le format choisi, tant que l'utilisateur ne l'a pas modifié lui-même ;
      - boutons « Ajouter à la file » (secondaire, `startNow` false) et « Télécharger · {taille} » (`cta`, `startNow` true ; sans taille : « Télécharger »).
    - Les deux boutons sont désactivés si `busy`, si la découpe est invalide, ou si le nom est vide.
    - Changer d'onglet sélectionne `defaultFormat` de cet onglet.
  - `FilePanel({ probe: Extract<LinkProbe, { type: "file" }>; busy: boolean; onConfirm(choice: { dir: string; fileName: string }, startNow: boolean): void })` : bloc « fichier simple » de la maquette (icône de catégorie colorée, nom, `taille · catégorie · reprise possible ✓/✗ · accélération ✓/✗`), `DestinationFields` et les deux mêmes boutons. Le nombre de connexions n'est pas réglable avant la phase 5 et n'est donc pas affiché ici.
- Textes à ajouter : `new.tabAv` « Vidéo + audio », `new.tabAudio` « Audio seul », `new.tabVideo` « Vidéo seule », `new.recommended` « Recommandé », `new.formats` « {n} formats », `new.subsAvailable` « Sous-titres {langs} », `new.optSubs` « Intégrer les sous-titres », `new.optCut` « Découper un extrait », `new.cutStart` « Début », `new.cutEnd` « Fin », `new.cutEndPh` « fin », `new.saveIn` « Enregistrer dans », `new.browse` « Parcourir », `new.fileName` « Nom du fichier », `new.addToQueue` « Ajouter à la file », `new.download` « Télécharger », `new.downloadSize` « Télécharger · {size} », `new.resumable` « reprise possible », `new.accelerated` « accélération », `new.options` « Options ».

- [ ] **Step 1 : Tests qui échouent** (`newlink.test.tsx`, `vi.mock("@tauri-apps/plugin-dialog")`)

```ts
it("FormatList : badge recommandé et sélection", () => { /* clic sur 720p → onSelect(index de 720p) */ });
it("MediaPanel : onglets disponibles seulement", () => { /* formats av + audio → "Vidéo + audio" et "Audio seul", pas "Vidéo seule" */ });
it("MediaPanel : passer en Audio seul choisit MP3, nom en .mp3 et dossier Musique", () => { /* … */ });
it("MediaPanel : Télécharger envoie le choix avec startNow=true", () => { /* onConfirm({formatIndex, subtitles:false, cut:null, dir:video_dir, fileName:"Concert Live.mp4"}, true) */ });
it("MediaPanel : Ajouter à la file envoie startNow=false", () => { /* … */ });
it("MediaPanel : découpe invalide affiche le message et désactive les boutons", () => { /* début "2:00", fin "1:00" */ });
it("MediaPanel : pas d'option découpe si can_cut=false", () => { /* … */ });
it("MediaPanel : nom modifié à la main conservé au changement de format", () => { /* … */ });
it("FilePanel : infos et confirmation", () => { /* "6,1 Go", "reprise possible", onConfirm({dir, fileName}, true) */ });
it("Parcourir remplace le dossier, annuler ne change rien", async () => { /* open résout "D:/X" puis null */ });
```

- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/components/newlink`.
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — `npm test`, `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(ui): panneaux média et fichier de la page Nouveau`

---

### Task 10 : Page Nouveau et ajout groupé

**Files :**
- Create : `src/components/newlink/BatchPanel.tsx`, `src/pages/NewPage.tsx`, `src/pages/NewPage.test.tsx`
- Modify : `src/i18n/fr.ts`

**Interfaces :**
- Consumes : Tasks 7 à 9.
- Produces :
  - `type BatchMode = "direct" | "video" | "audio"` (déplacé ici depuis `AddBatchDialog`).
  - `BatchPanel({ busy: boolean; onSubmit(text: string, mode: BatchMode, hold: boolean): void })` : zone de texte (un lien par ligne), sélecteur de mode avec les libellés actuels (`Fichiers directs`, `Vidéo (MP4, meilleure qualité)`, `Audio uniquement (MP3)`), et les boutons « Ajouter à la file » / « Télécharger ». Ctrl+Entrée = Télécharger.
  - `type ExternalEvent = { seq: number } & ({ kind: "probing"; url: string } | { kind: "probed"; probe: LinkProbe } | { kind: "error"; url: string; message: string })`
  - `NewPage({ external: ExternalEvent | null; startInBatch: boolean; onDone(info?: string): void })`. Disposition de la maquette 02, option 2 :
    - **barre du haut** : champ de lien (bordure `accent` au focus) avec un bouton Coller (`navigator.clipboard.readText()`, erreur ignorée). Entrée lance l'analyse, et un collage d'URL dans le champ aussi. À côté, le bouton « Plusieurs liens », qui bascule vers `BatchPanel` ;
    - **sous la barre** :
      - au repos, un texte d'aide `t("new.hint")` ;
      - pendant l'analyse, un indicateur `t("new.probing")` et un bouton « Annuler » (→ `reset`) ;
      - en cas d'erreur, le message, et « Réessayer » qui relance `analyze(url)` ;
      - une fois prêt, `MediaPanel` ou `FilePanel`.
    - **confirmation** : `api.confirmLink(...)`. En cas de succès, `reset()` puis `onDone()`. En cas d'erreur, le message s'affiche au-dessus des boutons et le choix est conservé.
    - **lot** : `api.addBatchDownloads(text, "", mode, hold)`, puis `onDone(résumé)`, avec le même texte de résumé qu'aujourd'hui (`"{n} ajouté(s) · …"`).
    - **événement externe** : `external` (nouveau `seq`) appelle `showProbing` / `load` / `fail`, et quitte le mode lot.
- Textes : `new.title` « Nouveau téléchargement », `new.urlPh` « Colle un lien (vidéo, page, fichier)… », `new.paste` « Coller », `new.batch` « Plusieurs liens », `new.single` « Un seul lien », `new.hint` « Colle un lien : FlashGet détecte s'il s'agit d'une vidéo ou d'un fichier. », `new.probing` « Recherche des formats disponibles… », `new.cancel` « Annuler », `new.retry` « Réessayer », `new.batchPh` « Un lien par ligne… », `new.batchMode` « Type », `new.batchDirect` « Fichiers directs », `new.batchVideo` « Vidéo (MP4, meilleure qualité) », `new.batchAudio` « Audio uniquement (MP3) ».

- [ ] **Step 1 : Tests qui échouent** (`NewPage.test.tsx`, `vi.mock("../api")`)

```ts
it("Entrée lance l'analyse et affiche le panneau média", async () => { /* … */ });
it("erreur d'analyse : message et Réessayer relance", async () => { /* … */ });
it("Télécharger appelle confirmLink avec start_now=true puis onDone", async () => {
  /* expect(api.confirmLink).toHaveBeenCalledWith({ probe_id: "p1", format_index: 1, subtitles: false, cut: null, dest_dir: "D:/Vidéos", file_name: "Concert Live.mp4", start_now: true }) */
});
it("échec de confirmLink : message et choix conservé", async () => { /* … */ });
it("événement externe probed remplace la page", async () => { /* rerender avec external {seq:1, kind:"probed", probe} → panneau affiché sans appel à probeLink */ });
it("Plusieurs liens : addBatchDownloads(text, '', 'audio', true) puis onDone(résumé)", async () => { /* … */ });
it("startInBatch ouvre directement le lot", () => { /* … */ });
```

- [ ] **Step 2 : Vérifier l'échec.**
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 : Vérifier** — `npm test`, `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(ui): page Nouveau et ajout groupé`

---

### Task 11 : Intégration et suppression de l'ancien chemin

**Files :**
- Modify : `src/App.tsx`, `src/pages/DownloadsPage.tsx` (rien à changer si les props restent), `src-tauri/src/commands.rs`, `src-tauri/src/state.rs`, `src-tauri/src/lib.rs`, `src/api.ts`, `download-core/src/ytdlp.rs`
- Delete : `src/components/AddUrlDialog.tsx`, `AddBatchDialog.tsx`, `AddBatchDialog.test.tsx`, `HlsVariantDialog.tsx`, `HlsVariantDialog.test.tsx`, `ProbingOverlay.tsx`

**Interfaces :**
- `App.tsx` :
  - `go("new")` passe maintenant sur la page Nouveau (`setPage("new")`) : le menu met « Nouveau » en surbrillance.
  - « Ajouter un lien » de la page Téléchargements fait `go("new")` ; « Plusieurs liens » fait `go("new")` avec `startInBatch = true`.
  - `onLinkProbing`, `onLinkProbed` et `onLinkError` passent la page sur Nouveau et transmettent un `ExternalEvent` avec un `seq` incrémenté. La fenêtre est déjà mise au premier plan côté Rust.
  - `onDone(info)` passe sur « Téléchargements », appelle `refresh()`, et affiche `info` dans le bandeau d'info s'il est fourni.
  - On supprime de `App` tout l'état et les minuteries de l'ancien chemin (`dialogOpen`, `batchOpen`, `mediaEvent`, `adding`, `addError`, `addingRef`, `probeTimeoutRef`, `extProbing`, `extProbeTimeoutRef`, `handleAdd`, `handleAddBatch`, `handleHlsConfirm`, et les abonnements `media://*`).
- Rust : supprimer les commandes `add_download` et `add_media_download`, les événements `media://variants|error|probing`, `pending_media` / `PendingMedia` / `MediaProbe`, `MediaVariant` / `MediaVariantsEvent`, et `select_variants` / `parse_formats` / `YtdlpVariant` s'ils ne servent plus ailleurs (vérifier avec grep). Garder `add_url` seulement s'il a encore un appelant, sinon le supprimer.
- `api.ts` : supprimer `addDownload`, `addMediaDownload`, `onMediaVariants`, `onMediaError`, `onMediaProbing`, `HlsVariant`, `MediaVariantsEvent` et `MediaErrorEvent`.

- [ ] **Step 1 : Réécrire `App.tsx`** selon ce qui précède, et faire les suppressions.
- [ ] **Step 2 : Vérifier** — `npm test`, `npx tsc --noEmit`, `npm run build`, `cargo test … --workspace -j 2`, `cargo build … -j 2` → tout passe. Le grep de `media://`, `__media_pending__`, `add_media_download` et `HlsVariantDialog` dans `src/` et `src-tauri/src/` ne renvoie rien.
- [ ] **Step 3 : Commit** — `feat(ui): page Nouveau branchée, ancien chemin d'ajout supprimé`
- [ ] **Step 4 : Vérification manuelle** (par le propriétaire, avec `npm run tauri dev` ; quitter d'abord tout FlashGet déjà lancé) :
  1. Nouveau → coller un lien YouTube : aperçu, titre, durée, onglets, 1080p recommandé. Passer en « Audio seul » : MP3, nom en `.mp3`, dossier Musique. « Télécharger » : la page Téléchargements montre la vignette, et le fichier final est bien un MP3.
  2. Même lien en 720p avec sous-titres et découpe de 0:10 à 0:40 : le fichier final dure environ 30 s et contient des sous-titres.
  3. Lien d'ISO : bloc « fichier simple », « Ajouter à la file » : la ligne affiche « en attente de la file » et ne démarre pas. Reprendre la démarre.
  4. Relancer le même ISO : le nouveau fichier s'appelle `… (2).iso`.
  5. Lien de page sans vidéo : message d'erreur, puis Réessayer.
  6. Extension : un clic sur un fichier ou une vidéo dans le navigateur met FlashGet au premier plan, sur Nouveau, prérempli ; **rien ne démarre** avant confirmation.
  7. Plusieurs liens en mode Audio avec une playlist, puis « Ajouter à la file » : tous les éléments sont « en attente ».
  8. Un téléchargement yt-dlp ajouté en phase 1 (ancienne base) se reprend normalement.
- [ ] **Step 5 : Fin de phase** — `superpowers:finishing-a-development-branch`.
