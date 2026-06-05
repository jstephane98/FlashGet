# Spec — Phase 4c : extracteur yt-dlp (sites difficiles / YouTube)

**Date :** 2026-06-05
**Branche :** `phase-4c-ytdlp` (empilée sur `phase-4b3-media-filename-title`)
**Statut :** design validé par l'utilisateur, prêt pour le plan d'implémentation.

## 1. Problème / objectif

Le moteur HLS/DASH standard ne capte pas YouTube et les « sites difficiles » (URLs
signées/expirables, paramètre `n` obfusqué, audio/vidéo séparés, protocole SABR/UMP, pas
de manifeste unique). On embarque **yt-dlp en sidecar** (comme ffmpeg) et on route les
URLs de pages vidéo génériques vers lui. yt-dlp gère l'extraction spécifique au site et
reste à jour ; FlashGet l'orchestre et présente l'UI cohérente du reste de l'app.

## 2. Décisions (brainstorm, 2026-06-05)

- **Point d'entrée :** l'utilisateur **colle l'URL de la page** dans « Ajouter une URL ».
  Aucune modification de l'extension navigateur (paste-only).
- **Détection :** si l'URL sondée renvoie une page `text/html` → route yt-dlp ; sinon
  flux régulier. **Générique** (tout site supporté par yt-dlp), pas de liste blanche.
- **Qualité :** **boîte de choix** des formats (sonde `yt-dlp -J`), réutilise
  `HlsVariantDialog` + l'événement `media://variants`.
- **Contrôles :** **pause / reprise / annulation** (reprise via `yt-dlp --continue`).
- **Approche A :** yt-dlp en sidecar piloté côté Tauri, **parseurs purs et testés** dans
  `download-core`. ffmpeg = notre sidecar existant (`--ffmpeg-location`).
- **Binaire :** `yt-dlp-<target-triple>.exe` fourni en sidecar (`externalBin`), comme
  ffmpeg. Auto-update **hors périmètre**.

## 3. Périmètre

### Inclus
- Sidecar yt-dlp + provisioning (`tauri.conf.json`).
- Cœur : `DownloadKind::Ytdlp`, `ProbeResult.content_type`, module pur `ytdlp` (parse
  formats + progression + sélection de format).
- Routage `text/html` → flux yt-dlp dans la commande `add_download`.
- Sonde des formats + boîte de variantes (nom pré-rempli avec le **titre yt-dlp**).
- Moteur de téléchargement yt-dlp (spawn + progression + pause/reprise/annulation).
- Frontend : type `ytdlp`, badge, réutilisation des composants existants.

### Exclus (assumé, backlog)
- Playlists (`--no-playlist` : une vidéo par ajout).
- Bouton de capture dans l'extension navigateur (paste-only pour ce MVP).
- Auto-update de yt-dlp (binaire figé ; peut vieillir).
- Sous-titres.
- **Limiteur de bande passante global non appliqué** à yt-dlp (il a son propre
  `--limit-rate` ; non câblé ici).
- Sélection fine de la langue audio (yt-dlp fusionne `bestaudio`).

## 4. Architecture livrée

### 4.1 Binaire (sidecar)
`src-tauri/tauri.conf.json` : `"externalBin": ["bin/ffmpeg", "bin/yt-dlp"]`. L'utilisateur
place `src-tauri/bin/yt-dlp-<target-triple>.exe`. Appelé via
`app.shell().sidecar("yt-dlp")` (même API que `SidecarRemuxer`).

### 4.2 Cœur — `download-core/src/ytdlp.rs` (pur, testé)

```rust
pub struct YtdlpFormat {
    pub format_id: String,
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub ext: Option<String>,
    pub vcodec: Option<String>,   // "none" => audio-only
    pub acodec: Option<String>,
    pub filesize: Option<u64>,    // filesize ou filesize_approx
    pub tbr: Option<f64>,         // kbit/s
}

/// Parse la sortie `yt-dlp -J` : titre + formats.
pub fn parse_formats(json: &str) -> Result<(String, Vec<YtdlpFormat>), CoreError>;

/// Variante neutre côté cœur (mappée vers le `MediaVariant` Tauri par commands.rs).
pub struct YtdlpVariant {
    pub resolution: Option<String>, // ex. "1920x1080"
    pub bandwidth: u64,             // bit/s (depuis tbr*1000), 0 si inconnu
    pub est_size: Option<u64>,
}

/// Variantes sélectionnables : formats porteurs de vidéo (vcodec != "none"),
/// dédupliqués par hauteur, triés par hauteur décroissante. Renvoie aussi le
/// format_id à mémoriser pour chaque variante.
pub fn select_variants(formats: &[YtdlpFormat]) -> Vec<(String, YtdlpVariant)>;

/// Argument `-f` : fusionne l'audio si le format est vidéo-seul.
/// `"<id>+bestaudio/<id>/best"`.
pub fn build_format_arg(format_id: &str) -> String;

/// Parse une ligne de progression (template à sentinelle, voir 4.5).
/// Renvoie (downloaded_bytes, total_bytes_opt).
pub fn parse_progress(line: &str) -> Option<(u64, Option<u64>)>;
```

`MediaVariant` est défini côté Tauri (`commands.rs`) ; le cœur reste agnostique en
renvoyant `YtdlpVariant`, que `commands.rs` mappe trivialement vers `MediaVariant`
(pas de type partagé déplacé — YAGNI).

### 4.3 Cœur — modèle & http
- `model.rs` : `DownloadKind::Ytdlp` ; `storage.rs` round-trip `"ytdlp"` (+ test).
- `http.rs` : `ProbeResult` gagne `content_type: Option<String>` (lu depuis l'en-tête
  `Content-Type` de la réponse de sonde). Helper pur `is_html(&Option<String>) -> bool`
  (vrai si commence par `text/html`, insensible casse/espaces).

### 4.4 Routage — `commands.rs::add_download`
La commande (champ « Ajouter une URL ») sonde l'URL **une fois** :
- `is_html(probe.content_type)` **vrai** → flux yt-dlp :
  1. `crate::ytdlp::probe_formats(app, url, headers)` → `(title, formats)`.
  2. `select_variants` → variantes ; si vide → `media://error`
     « Aucune vidéo trouvée sur cette page ».
  3. `PendingMedia` étendu : variante `Ytdlp { format_ids: Vec<String>, url: String }`
     (réutilise la map `pending_media` + le TTL existants), `file_name =
     naming::sanitize_title(Some(&title))`.
  4. émet `media://variants { probe_id, file_name, variants }` (même boîte que HLS/DASH).
- sinon → `add_url` régulier. **Le probe est réutilisé** (passé à `add_url` pour éviter
  une double sonde — `add_url` accepte un `Option<ProbeResult>` pré-calculé).

Le bridge navigateur (`add_url` via capture) n'est pas concerné par le routage html.

### 4.5 Coquille Tauri — `src-tauri/src/ytdlp.rs` (mince, non testée)
- `probe_formats(app, url, headers) -> Result<(String, Vec<YtdlpFormat>), String>` :
  spawn `yt-dlp -J --no-playlist --no-warnings` (+ `--add-header` pour
  cookie/referer/user-agent si présents) ; capture stdout ; `ytdlp::parse_formats`.
- `run_download(app, url, format_id, dest, handle, tx)` :
  spawn `yt-dlp -f <build_format_arg(format_id)> --no-playlist --newline
  --ffmpeg-location <ffmpeg sidecar path> --continue
  --progress-template "FGPROG:%(progress.downloaded_bytes)s/%(progress.total_bytes)s"
  -o <dest> <url>` (+ en-têtes). Lit stdout ligne par ligne → `ytdlp::parse_progress`
  → `tx.send(Progress { status: Downloading, .. })`. **Surveille `handle`** : si
  `paused`/`cancelled` → tue l'enfant. Code de sortie 0 → `Ok` (émet `Completed`) ;
  sinon → `Err(tail stderr)`.
  Le chemin du ffmpeg sidecar est résolu via l'API Tauri shell (même résolution que
  `sidecar("ffmpeg")`).

### 4.6 Dispatch — `commands.rs::launch`
Nouveau bras :
```rust
download_core::model::DownloadKind::Ytdlp => {
    let format_id = media_selection.clone().unwrap_or_default();
    crate::ytdlp::run_download(app_for_dl.clone(), &url, &format_id, &dest, handle, tx).await
}
```
Branché sur le **même `tx`** (relais de progression générique inchangé) et le **même
`handle`** (pause/cancel).

### 4.7 Confirmation — `commands.rs::add_media_download`
Gère le `PendingMedia::Ytdlp` : `Download { kind: Ytdlp, url: page_url,
media_selection: Some(format_ids[variant_index]), file_name: ensure_extension(nom_édité,
ext_choisi ou "mp4"), category: Video }`. (L'extension de sortie est `mp4` par défaut ;
le conteneur réel est décidé par yt-dlp/ffmpeg au merge — acceptable pour le MVP.)

### 4.8 Pause / reprise / annulation
Réutilise `DownloadHandle` :
- **Pause** (`pause_download`) → `handle.pause()` → `run_download` tue le process, garde le
  `.part`, statut `Paused`.
- **Reprise** (`start_download` → re-`Queued` → `launch`) → `run_download` relance yt-dlp
  avec `--continue` (reprend le `.part`) + `media_selection` (format) + `url`.
- **Annulation** (`cancel`) → tue + retire la ligne + nettoie le partiel (sémantique
  existante).

### 4.9 Frontend
- `types.ts` : `DownloadKind`/`kind` += `"ytdlp"`.
- `DownloadRow.tsx` : badge « yt-dlp » (ou « site ») à côté du nom, sur le modèle des
  badges HLS/DASH/LIVE existants.
- Réutilise `HlsVariantDialog` (nom éditable pré-rempli) et les lignes de progression.
- Aucune nouvelle commande front : `add_download` (existant) déclenche le flux ;
  `add_media_download` (existant, étendu) confirme.

## 5. Flux de données

```
[Champ "Ajouter une URL"] page URL
   │ add_download → probe (content_type)
   ├── is_html=false → add_url régulier (probe réutilisé)
   └── is_html=true :
        ytdlp::probe_formats (yt-dlp -J)  ──►  parse_formats → (title, formats)
        select_variants → variantes ; PendingMedia::Ytdlp{format_ids,url}, file_name=title
        média://variants  ──►  HlsVariantDialog (nom éditable pré-rempli)
        onConfirm(probe_id, variant, name)
        add_media_download → Download{kind:Ytdlp, url, media_selection=format_id, name.mp4}
        launch → ytdlp::run_download(yt-dlp -f … --continue --ffmpeg-location … )
            stdout → parse_progress → download://progress (Downloading → Completed)
```

## 6. Tests

### Cœur (`download-core`, `#[cfg(test)]`)
- `parse_formats` : JSON `yt-dlp -J` d'exemple → titre + formats ; gère
  `filesize`/`filesize_approx`, `vcodec:"none"`, champs manquants.
- `select_variants` : ne garde que les formats vidéo, déduplique par hauteur, tri
  décroissant, mappe résolution/bitrate/taille.
- `build_format_arg("137")` == `"137+bestaudio/137/best"`.
- `parse_progress("FGPROG:1048576/5242880")` == `Some((1048576, Some(5242880)))` ;
  total `"NA"`/absent → `Some((dl, None))` ; ligne non-FGPROG → `None`.
- `is_html` : `text/html`, `text/html; charset=utf-8`, `TEXT/HTML ` → vrai ;
  `video/mp4`, `application/x-mpegurl`, `None` → faux.
- Storage round-trip `DownloadKind::Ytdlp` (`"ytdlp"`).

### Frontend (Vitest)
- `DownloadRow` rend le badge pour `kind: "ytdlp"`.

### Smoke (`#[ignore]`, réseau + binaire requis)
- `yt-dlp --version` via sidecar renvoie 0 (vérifie le câblage du sidecar). (Un vrai
  download réseau reste manuel.)

### Critères verts
`cargo test` (download-core + tauri lib) ✅, `npm test` (Vitest) ✅, build `tsc + vite`
✅, **0 warning rustc**.

## 7. Gestion d'erreurs
- yt-dlp absent (sidecar introuvable) → erreur claire « yt-dlp introuvable ».
- `yt-dlp -J` échoue / aucune variante vidéo → `media://error` « Aucune vidéo trouvée sur
  cette page ».
- Échec du download → tail des dernières lignes stderr (comme `SidecarRemuxer`).
- Vidéo privée / soumise à l'âge / géo-bloquée → message yt-dlp remonté tel quel.

## 8. Risques / limites (non bloquants)
- **yt-dlp figé peut casser** quand les sites changent (pas d'auto-update au MVP).
- **Limiteur de bande passante global non appliqué** (yt-dlp télécharge lui-même).
- Conteneur de sortie : le nom force `.mp4`, mais yt-dlp peut produire un autre conteneur
  selon les formats ; acceptable au MVP (le merge ffmpeg vise mp4).
- Le titre yt-dlp peut contenir un suffixe ; champ éditable (réutilise 4b-3).
- Reprise `--continue` : fiable sur la plupart des sites ; si un site ne supporte pas la
  reprise, yt-dlp recommence le segment courant (acceptable).

## 9. Garde-fou processus
Implémentation **subagent-driven** : chaque prompt de subagent DOIT commencer par
`git checkout phase-4c-ytdlp` avant tout commit (incident de branche déjà survenu en
4b-2c).
