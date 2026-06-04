# Phase 4a — Téléchargement vidéo HLS (VOD) — Design

**Date :** 2026-06-04
**Statut :** Validé (design approuvé en brainstorming) — en attente de relecture utilisateur puis plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phases 1, 2a, 2b, 3a, 3b-1 (cookies/referer/user-agent), 3b-2 (capture auto) — fusionnées dans `main`.

## 1. Objectif et périmètre

**4a livre le téléchargement de vidéos HLS à la demande (VOD).** L'extension détecte les flux `.m3u8` lus dans la page, l'utilisateur choisit une qualité dans FlashGet, puis **notre moteur Rust télécharge les segments** (en réutilisant cookies/referer/user-agent de 3b-1, le rate-limiter, les retries et la pause/reprise) et **ffmpeg embarqué assemble** le résultat en `.mp4` par simple remux (`-c copy`, sans ré-encodage).

**Inclus :**
- HLS **VOD** uniquement (présence d'`EXT-X-ENDLIST`).
- Déchiffrement **AES-128 à clé en clair** (`EXT-X-KEY:METHOD=AES-128`, URI de clé HTTP).
- **Piste audio séparée** (`EXT-X-MEDIA:TYPE=AUDIO`) téléchargée et muxée avec la vidéo.
- **Dialogue de choix de qualité** dans l'app (meilleure variante pré-sélectionnée).
- **ffmpeg embarqué** en sidecar Tauri, cible **Windows seul** pour 4a.
- **Détection via `chrome.webRequest`** (observation) + bouton « Télécharger la vidéo » dans le popup.

**Hors périmètre 4a (→ 4b et au-delà) :**
- Flux **live** (sans `EXT-X-ENDLIST`) : détecté et signalé « non supporté » proprement.
- **DASH** (`.mpd`).
- **DRM** : Widevine, FairPlay, `METHOD=SAMPLE-AES`, `EXT-X-SESSION-KEY` → erreur explicite.
- ffmpeg **multi-plateforme** (Mac/Linux).
- Capture auto silencieuse des vidéos (la détection est passive ; l'envoi reste un clic).

La capture manuelle/auto de fichiers classiques (3a/3b-1/3b-2) reste disponible et inchangée.

## 2. Décisions de conception (validées en brainstorming)

- **Découpage :** 4a = HLS VOD d'abord (format dominant, pose toute l'infra ffmpeg/détection réutilisable) ; DASH + live + DRM en phases ultérieures.
- **Qui télécharge :** notre moteur Rust télécharge chaque segment via le `ByteFetcher` existant ; ffmpeg sert **uniquement** au remux/mux final. (Écarté : « ffmpeg fait tout » via `-i playlist.m3u8`, qui ferait perdre le pilotage progression/throttle/reprise et la transmission fiable des en-têtes.)
- **Détection :** `chrome.webRequest` (observation MV3) repère les `.m3u8` par extension d'URL et/ou Content-Type ; le popup propose un bouton « Télécharger la vidéo ». (Écarté : capture auto silencieuse — intrusive, doublons master/variantes ; content script DOM — fragile sur players obfusqués.)
- **Choix de qualité :** le **cœur Rust parse le master** et liste les variantes ; un **dialogue FlashGet** affiche les choix (l'UI ne fait qu'afficher). (Écarté : parsing en JS dans l'extension ; auto-meilleure-qualité sans demander.)
- **ffmpeg :** binaire **Windows local** committé dans `src-tauri/bin/`, configuré en `externalBin`. (Écarté : multi-plateforme immédiat — non smoke-testable ici ; téléchargement au build — build non hermétique.)
- **Cas HLS gérés :** AES-128 (clé en clair), audio séparé, choix de qualité. Live reporté.
- **Cookies non exposés au frontend :** registre **`pending_media`** côté backend ; l'event ne porte qu'un `probe_id` + la liste affichable.
- **Progression par segments** (la playlist donne le nombre de segments, pas les octets).
- **Trait `Remuxer`** dans le cœur (Tauri-agnostique, testable avec un fake) ; l'impl sidecar ffmpeg vit dans `src-tauri`.

## 3. Architecture & flux

```
Le lecteur de la page charge master.m3u8 + variante.m3u8 + segments
  └─ extension: chrome.webRequest (onBeforeRequest / onHeadersReceived) observe les requêtes
       → URL en .m3u8 OU Content-Type HLS ⇒ mémorise le flux pour cet onglet (Map par tabId)
  └─ popup: section « Vidéo(s) détectée(s) sur cette page » → clic « Télécharger la vidéo »
       → gatherHeaders(url, referrer) (cookies/referer/UA, comme 3b-1)
       → bridge: ClientMessage::Capture{ kind:"hls", url:masterUrl, referrer, cookie, user_agent, filename? }
  └─ server.rs: kind=="hls" ⇒ probe_hls(masterUrl, headers) au lieu d'add_url
       → fetch master.m3u8 (avec en-têtes) → parse:
           - master multi-variantes ⇒ liste { resolution, bandwidth, ~taille estimée, audio_group }
           - media playlist directe (pas de variantes) ⇒ variante unique implicite
       → range PendingMedia{ master_url, headers, variants, file_name } dans AppState (clé = probe_id)
       → emit media://variants{ probe_id, file_name, variants:[{ index, label, resolution, bandwidth, est_size }] }
       → fait surgir la fenêtre
  └─ frontend: HlsVariantDialog affiche les qualités (meilleure pré-sélectionnée)
       → invoke add_hls_download(probe_id, variant_index)
  └─ commands::add_hls_download:
       → récupère PendingMedia, résout la variante choisie (URL media playlist + éventuelle URL audio)
       → choose_file_name(filename suggéré, défaut depuis l'URL) + extension forcée .mp4 + catégorie Video
       → crée Download{ kind:Hls, url:mediaPlaylistUrl, audio_url, status:Queued } → upsert → reevaluate
  └─ launch: si download.kind == Hls ⇒ download_core::hls::download_hls(...) (au lieu d'engine::download)
       1. fetch+parse la media playlist (avec en-têtes) → segments[], key?(AES-128), map?(init fMP4), is_live
          → si is_live (pas d'ENDLIST) ⇒ CoreError « live non supporté »
       2. si EXT-X-KEY: fetch la clé une fois (avec en-têtes) ; refuse METHOD≠AES-128/NONE
       3. pour chaque segment manquant (pause/cancel vérifiés, rate-limiter, retries+backoff):
            fetcher.fetch_range(seg.url, None, headers) → octets
            si chiffré: déchiffre AES-128-CBC (IV explicite ou dérivé du media-sequence)
            écrit dossier temp dest+".fghls/seg00042.part" → rename atomique .ts/.m4s
            émet Progress{ segments_done, segments_total, downloaded_bytes }
       4. (si audio séparé) idem pour la piste audio dans .fghls/audio/
       5. Remuxer.remux({ video, audio?, init? }, dest.mp4):
            ffmpeg -i <video> [-i <audio>] -c copy -movflags +faststart <out.mp4>
       6. supprime le dossier temp ; émet Completed
```

Côté UI, un download HLS apparaît dans la liste comme les autres (badge « HLS », progression « X/N segments »), et bénéficie de pause/reprise/annulation.

## 4. Composants

### 4.1 Extension — logique pure (`extension/media-rules.js`, testable Vitest)
- `isHlsUrl(url: string) -> boolean` : vrai si le **chemin** (query/fragment ignorés) finit par `.m3u8`.
- `isHlsContentType(ct: string) -> boolean` : vrai pour `application/vnd.apple.mpegurl`, `application/x-mpegurl`, `audio/mpegurl`, `audio/x-mpegurl` (insensible à la casse, paramètres `;charset` ignorés).
- `dedupeStreams(list) -> list` : dédoublonne par URL, conserve l'ordre d'apparition.

### 4.2 Extension — `background.js`
- Listeners **`chrome.webRequest.onBeforeRequest`** (filtre URL) et **`onHeadersReceived`** (Content-Type) — mode observation (pas de blocage).
- Maintient `detectedByTab: Map<tabId, [{ url, ts }]>` ; ajoute un flux quand `isHlsUrl(details.url)` ou `isHlsContentType(header)` ; déduplique.
- Purge l'entrée d'un onglet sur `chrome.webRequest.onBeforeNavigate` (navigation principale) et `chrome.tabs.onRemoved`.
- Message popup `{ cmd:"listMedia", tabId }` → renvoie les flux détectés ; `{ cmd:"sendMedia", url, tabId }` → `gatherHeaders` + `buildCapture(url, { kind:"hls", referrer, cookie, user_agent })` → envoi via le socket (file `pending` réutilisée si non connecté).
- Permission **`webRequest`** ajoutée au `manifest.json` (host permission `<all_urls>` déjà présente).

### 4.3 Extension — `popup.js` / `popup.html`
- Nouvelle section « Vidéos sur cette page » : liste les flux détectés de l'onglet actif (via `chrome.tabs.query({active:true})` + `listMedia`), chacun avec un bouton « Télécharger la vidéo ».
- Si aucun flux : message discret « Aucune vidéo détectée ». Le statut de connexion existant reste affiché.

### 4.4 Extension — `buildCapture.js`
- Ajoute `kind` aux clés autorisées de `extras` (avec `referrer`, `cookie`, `user_agent`, `filename`). Défaut implicite `"file"` quand absent. Reste **pur**.

### 4.5 Bridge — `protocol.rs` / `server.rs`
- `ClientMessage::Capture` gagne un champ optionnel `kind: Option<String>` (parsé ; absent ⇒ `"file"`).
- `protocol::handle_message` : pour une capture, le `kind` est propagé dans `ServerReply::CaptureRequested` (nouveau champ `kind`).
- `server.rs` : si `kind == "hls"` ⇒ appelle `commands::probe_hls(app, url, RequestHeaders, filename)` (émet `media://variants`, fait surgir la fenêtre) ; sinon comportement actuel (`add_url` + `download://added`).

### 4.6 Cœur `download-core` — module `hls/` (TDD)

**`hls/playlist.rs` (pur) :**
- `parse_master(text, base_url) -> MasterPlaylist` où
  `MasterPlaylist { variants: Vec<Variant>, audio: Vec<AudioRendition> }`,
  `Variant { uri, bandwidth: u64, resolution: Option<(u32,u32)>, audio_group: Option<String> }`,
  `AudioRendition { group_id, name, uri: Option<String>, default: bool, autoselect: bool }`.
  Parse `EXT-X-STREAM-INF` (BANDWIDTH, RESOLUTION, AUDIO) et `EXT-X-MEDIA:TYPE=AUDIO`.
- `parse_media(text, base_url) -> MediaPlaylist` où
  `MediaPlaylist { segments: Vec<Segment>, key: Option<EncKey>, map: Option<Uri>, is_live: bool, target_duration: f64, media_sequence: u64 }`,
  `Segment { uri, duration: f64, byte_range: Option<(u64,u64)> }`,
  `EncKey { method: KeyMethod, uri: Option<String>, iv: Option<[u8;16]> }`,
  `KeyMethod { None, Aes128, SampleAes, Other(String) }`.
  Parse `EXTINF`, `EXT-X-BYTERANGE`, `EXT-X-KEY`, `EXT-X-MAP` ; `is_live = !contient("EXT-X-ENDLIST")`.
- Résolution d'URI relatives/absolues contre `base_url`.
- `is_master(text) -> bool` (présence d'`EXT-X-STREAM-INF`) pour distinguer master vs media.

**`hls/crypto.rs` (pur) :**
- `decrypt_aes128_cbc(data: &[u8], key: &[u8;16], iv: &[u8;16]) -> Vec<u8>` (crates `aes` + `cbc`, padding PKCS7).
- `iv_for_segment(explicit: Option<[u8;16]>, media_sequence: u64) -> [u8;16]` : IV explicite sinon big-endian du numéro de séquence.

**`hls/select.rs` (pur) :**
- `best_variant(&MasterPlaylist) -> usize` : index du plus haut `bandwidth`.
- `estimated_size(bandwidth: u64, total_duration_secs: f64) -> u64` : `bandwidth * dur / 8` (affiché « ≈ »).
- `pick_audio(&MasterPlaylist, variant) -> Option<&AudioRendition>` : la rendition `default`/`autoselect` du groupe audio de la variante (si URI présente).

**`hls/engine.rs` :**
- `async fn download_hls<F: ByteFetcher, R: Remuxer>(fetcher, remux, id, media_url, audio_url: Option<&str>, dest: &Path, handle: DownloadHandle, limiter, progress, headers) -> Result<Outcome, CoreError>`.
- Réutilise `DownloadHandle`/`Outcome`/`RateLimiter` du moteur existant et le motif retries+backoff par segment.
- **Concaténation côté cœur** (pure I/O, testable) : une fois tous les segments d'une piste téléchargés et déchiffrés, le moteur les concatène **dans l'ordre** en un **fichier unique par piste** dans le temp (`video.ts`/`video.mp4`, init `EXT-X-MAP` préfixé pour le fMP4 ; `audio.<ext>` si piste séparée). Ces fichiers uniques sont ensuite passés au `Remuxer`. ffmpeg ne fait donc que le **remux conteneur**, jamais la concaténation.
- Reprise : re-récupère la playlist, **saute les segments dont le fichier `.ts`/`.m4s` final existe** (écriture `.part` → rename atomique). Pause après segment courant. Annulation : supprime le dossier temp.

### 4.7 Cœur — trait `Remuxer`
- `#[async_trait] pub trait Remuxer: Send + Sync { async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError>; }`
  avec `RemuxInputs { video: PathBuf, audio: Option<PathBuf> }` — **fichiers déjà concaténés** par le moteur (§4.6), un par piste.
- Tests du cœur : `FakeRemuxer` qui écrit un fichier de sortie factice et enregistre les inputs reçus.

### 4.8 `src-tauri` — ffmpeg sidecar & `SidecarRemuxer`
- `tauri.conf.json` : `bundle.externalBin = ["bin/ffmpeg"]` ; dépendance `tauri-plugin-shell` ; binaire `src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe` (build statique LGPL).
- `remux.rs` : `SidecarRemuxer` (impl `Remuxer`) reçoit les **fichiers déjà concaténés** (§4.6), construit les args (`-y -i <video> [-i <audio>] -c copy -movflags +faststart <out>`), lance le sidecar via le shell Tauri, lit le code de sortie ; en cas d'échec ⇒ `CoreError` avec extrait de stderr. Aucune concaténation ici (faite côté cœur) — uniquement le remux conteneur.

### 4.9 `src-tauri` — commandes & launch
- `probe_hls(app, url, headers, filename) -> Result<(), String>` : fetch+parse master (ou media direct), construit la liste de variantes + tailles estimées, range `PendingMedia` (clé `probe_id` = UUID), émet `media://variants`, surgit la fenêtre.
- `pending_media: Mutex<HashMap<String, PendingMedia>>` ajouté à `AppState`.
- `add_hls_download(app, probe_id, variant_index) -> Result<Download, String>` : résout la variante (URL media + audio), nom de fichier `.mp4`, catégorie Video, crée `Download{ kind:Hls, ... }`, upsert, `reevaluate`, retire l'entrée `pending_media`.
- `launch` : `match download.kind { Regular => engine::download(...), Hls => hls::download_hls(..., SidecarRemuxer, ...) }`. Le relais de progression existant transmet aussi `segments_done/total`.

### 4.10 Modèle & storage
- `model.rs` : `enum DownloadKind { Regular, Hls }` (serde `lowercase`, default `Regular`) ; `Download` gagne `kind: DownloadKind` (serde default) et `audio_url: Option<String>` (serde default). `Progress` gagne `segments_done: Option<u32>` et `segments_total: Option<u32>` (serde default `None`).
- `storage.rs` : migrations additives `ALTER TABLE downloads ADD COLUMN kind TEXT`, `ADD COLUMN audio_url TEXT` (motif des phases 2a/2b/3b-1). `upsert`/`list`/`get_by_id` lisent/écrivent ces colonnes.

### 4.11 Frontend (React)
- `types.ts` : mirror `DownloadKind`, `HlsVariant { index, label, resolution, bandwidth, estSize }`, `MediaVariantsEvent { probeId, fileName, variants }`, et champs `segmentsDone/segmentsTotal` de `Progress`, `kind`/`audioUrl` de `Download`.
- `api.ts` : `onMediaVariants(cb)` (écoute `media://variants`), `addHlsDownload(probeId, variantIndex)`.
- `HlsVariantDialog.tsx` : liste les variantes (résolution, bitrate, ≈ taille), meilleure pré-sélectionnée, Valider/Annuler.
- `App.tsx` : écoute `onMediaVariants` → ouvre le dialogue.
- `DownloadRow.tsx` : badge « HLS » si `kind==='hls'` ; progression « X/N segments » quand `segmentsTotal` est défini (barre = done/total), sinon comportement actuel.

## 5. Modèle de progression

La playlist fournit le **nombre de segments et leurs durées, pas les octets** ⇒ taille totale inconnue au départ.
- Pendant le téléchargement : `Progress.segments_done/segments_total` pilotent la barre (`done/total`) et le libellé « X/N segments » ; `downloaded_bytes` s'accumule réellement ; `total_bytes` reste `None`.
- Dans le **dialogue de qualité** : taille estimée « ≈ » = `bandwidth × durée_totale ÷ 8`, indicative.
- Les downloads classiques (Regular) gardent `segments_*` à `None` ⇒ affichage % par octets inchangé.

## 6. Pause / reprise / annulation (HLS)

- **Pause** : arrêt après le segment courant ; les segments faits restent dans `dest+".fghls/"` (écriture `.part` → rename atomique).
- **Reprise** : re-récupère la media playlist, **saute les segments dont le fichier final existe**, télécharge le reste, puis remux. *Limite assumée :* segments à URL/jeton expirable ⇒ erreur reprenable (même limite qu'en 3b-2).
- **Annulation** : supprime le dossier temp + toute sortie partielle.

## 7. Garde-fous, sécurité & limites

- **Live non supporté** : media playlist sans `EXT-X-ENDLIST` ⇒ `CoreError` « Flux live non supporté (à venir) » ; le download passe en Error proprement (pas de boucle infinie).
- **DRM non supporté** : `METHOD=SAMPLE-AES`, `EXT-X-SESSION-KEY`, Widevine/FairPlay ⇒ erreur explicite ; seuls `AES-128` (clé URI HTTP) et `NONE` gérés.
- **Chemins sûrs** : dossier temp sous le dossier de destination ; segments nommés par **index** (jamais par nom distant) ⇒ pas de traversée. La garde anti-traversée d'`add_url`/`choose_file_name` protège le nom final `.mp4`.
- **En-têtes** (cookies/referer/user-agent) appliqués aux requêtes **playlist, clé ET segments**.
- **Échec ffmpeg** ⇒ statut Error + extrait stderr ; dossier temp conservé pour diagnostic (nettoyé au prochain retry réussi ou à l'annulation).
- **Pas de boucle d'auto-capture** : FlashGet récupère via `reqwest` (hors navigateur) ; `webRequest` n'observe que le trafic du navigateur.
- **Permission `webRequest`** : avertissement navigateur assumé (lecture du trafic réseau, sans blocage).

## 8. Stratégie de tests

- **Rust (TDD)** :
  - `playlist.rs` : master (variantes BANDWIDTH/RESOLUTION/AUDIO, renditions audio), media (segments, EXTINF, EXT-X-BYTERANGE, EXT-X-MAP, EXT-X-KEY), URIs relatives/absolues, détection live (absence d'ENDLIST), `is_master`.
  - `crypto.rs` : déchiffrement AES-128-CBC sur vecteurs connus ; IV explicite vs dérivé du media-sequence.
  - `select.rs` : `best_variant` (plus haut bitrate), `estimated_size`, `pick_audio` (default/autoselect).
  - `engine.rs` : `download_hls` contre **FakeFetcher + FakeRemuxer** : N segments téléchargés et concaténés, segment chiffré déchiffré, audio séparé muxé, pause après segment, annulation nettoie le temp, reprise saute les segments existants, live ⇒ erreur.
- **Vitest** : `media-rules` (`isHlsUrl` avec query/sans extension ; `isHlsContentType` casse/charset ; `dedupeStreams`) ; `buildCapture` (inclut `kind`, omet vide).
- **Manuel (smoke-test)** :
  1. HLS VOD **clair** public → détecté dans le popup, dialogue liste les qualités, download → `.mp4` lisible.
  2. HLS **AES-128** → download déchiffré, lisible.
  3. HLS **audio séparé** → la vidéo finale **a du son**.
  4. Flux **live** → message « non supporté » propre.
  5. **Pause/reprise** d'un HLS en cours → reprend sans re-télécharger les segments déjà faits.

## 9. Découpage prévisionnel du plan (à détailler en writing-plans)

1. `hls/playlist.rs` (parser master + media, live detection, byte-range, map, key) — TDD.
2. `hls/crypto.rs` (AES-128-CBC + IV) — TDD.
3. `hls/select.rs` (best_variant, estimated_size, pick_audio) + trait `Remuxer` + `FakeRemuxer` — TDD.
4. `hls/engine.rs` `download_hls` (segments, déchiffrement, audio, pause/cancel/reprise, remux) contre fakes — TDD.
5. `model.rs` (`DownloadKind`, `audio_url`, `Progress` segments) + `storage.rs` migrations — TDD.
6. `src-tauri` : config `externalBin` + `tauri-plugin-shell` + binaire ffmpeg Windows + `SidecarRemuxer`.
7. `src-tauri` : `probe_hls` + `pending_media` + `add_hls_download` + event `media://variants` + branche `launch` (Hls).
8. Bridge : `kind` dans `Capture`/`CaptureRequested` + routage `server.rs`.
9. Extension : `media-rules.js` (TDD) + détection `webRequest` + popup « Vidéos détectées » + `kind` dans `buildCapture`/`sendMedia`.
10. Frontend : `HlsVariantDialog` + `api`/`types` + écoute `media://variants` + badge HLS + progression segments.
11. Vérification : suites Rust + Vitest + smoke-tests (clair / AES-128 / audio séparé / live / pause-reprise).

## 10. Décisions reportées / hors 4a

- **4b et au-delà :** live HLS (enregistrement, arrêt manuel, progression en durée), DASH (`.mpd`, adaptation sets audio/vidéo), DRM (Widevine/FairPlay/SAMPLE-AES), ffmpeg multi-plateforme (Mac/Linux), capture auto silencieuse des vidéos, sous-titres (WebVTT), sélection de la piste audio/langue, qualité par défaut configurable dans les paramètres.
- **Backlog technique antérieur** (2a/2b/3a) toujours ouvert : `get_category_dirs` atomique ; re-catégorisation si le nom final diffère.
