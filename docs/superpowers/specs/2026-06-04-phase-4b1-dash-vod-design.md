# Phase 4b-1 — Téléchargement vidéo DASH (VOD) — Design

**Date :** 2026-06-04
**Statut :** Validé (design approuvé en brainstorming) — en attente de relecture utilisateur puis plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phases 1, 2a, 2b, 3 (intégration navigateur), **4a (HLS VOD)** — 4a est sur la branche `phase-4a-hls-vod` (implémentée, pas encore mergée ; 4b-1 se construit dessus).

## 1. Objectif et périmètre

**4b-1 livre le téléchargement de vidéos DASH à la demande (VOD).** L'extension détecte les manifests `.mpd`, l'utilisateur choisit une qualité dans FlashGet, puis **notre moteur Rust télécharge les segments** (en réutilisant le rate-limiter, les retries, la pause/reprise, les requêtes Range et le pattern dossier-temp/`.seg` de 4a) et **ffmpeg embarqué assemble** le résultat en `.mp4` — et, pour les flux CENC ClearKey, **déchiffre** au passage via `-decryption_key`.

**Inclus :**
- DASH **VOD** uniquement (`MPD@type="static"`).
- Adressage des segments : **SegmentTemplate `$Number$`**, **SegmentTemplate + SegmentTimeline**, **SegmentList**, **SegmentBase (fichier unique + `sidx`)**.
- **AdaptationSets vidéo et audio séparés** : la vidéo et l'audio choisis sont téléchargés et muxés ensemble.
- **Dialogue de choix de qualité** dans l'app (réutilise celui de 4a).
- **ClearKey / CENC** : segments chiffrés téléchargés tels quels, **déchiffrés et remuxés par ffmpeg** (`-decryption_key`), la clé étant obtenue depuis la `ContentProtection` ClearKey du MPD ou sa `laurl` (licence W3C ClearKey → JWK).
- ffmpeg embarqué **Windows seul** (acquis de 4a).

**Hors périmètre 4b-1 (→ ultérieur) :**
- Flux **live** : `MPD@type="dynamic"` détecté et rejeté proprement (→ 4b-2).
- **DRM** : Widevine, FairPlay, SAMPLE-AES → erreur explicite.
- **Multi-Period** : seule la **première Period statique** est traitée (limite assumée, signalée).
- **xlink** (Period distante), sous-titres / sélection de langue (add-ons ultérieurs), ffmpeg multi-plateforme.

La capture de fichiers classiques (P3) et le HLS VOD (4a) restent inchangés.

## 2. Décisions de conception (validées en brainstorming)

- **Découpage de la « Phase 4b » :** elle empile plusieurs sous-systèmes indépendants (DASH, live, DRM, sous-titres, langues, ffmpeg multi-plateforme). On les traite **un par un**, chacun son cycle spec→plan→implémentation. **4b-1 = DASH VOD d'abord** (réutilisation maximale de 4a, format le plus demandé après HLS).
- **Qui télécharge / qui déchiffre :** notre moteur Rust télécharge chaque segment via le `ByteFetcher` existant ; **ffmpeg fait le remux ET le déchiffrement CENC** (`-decryption_key`). (Écarté : implémenter le déchiffrement CENC sample-level en Rust — parsing des boîtes `senc`/`saiz`/`saio` —, beaucoup plus lourd.)
- **DRM hors périmètre, ClearKey OK :** Widevine/FairPlay/SAMPLE-AES restent exclus ; seul le chiffrement à **clé récupérable** (ClearKey/CENC AES-CTR, analogue de l'AES-128 déjà fait en HLS) est géré.
- **Acquisition de clé ClearKey :** depuis la `ContentProtection` du MPD **ou** sa `laurl` (requête de licence W3C ClearKey renvoyant un JWK).
- **Détection :** `chrome.webRequest` (observation), comme 4a — par extension d'URL `.mpd` et/ou `Content-Type: application/dash+xml`.
- **Réutilisation de l'acquis 4a** plutôt que duplication : voir §4 (Remuxer étendu, `pending_media` généralisé, dialogue qualité réutilisé, helpers de téléchargement de segments factorisés).

## 3. Architecture & flux

```
Le lecteur charge manifest.mpd + segments
  └─ extension: chrome.webRequest observe ; URL .mpd OU Content-Type application/dash+xml ⇒ mémorise le flux (Map par tabId)
  └─ popup « Vidéo(s) détectée(s) » → clic « Télécharger la vidéo »
       → gatherHeaders + Capture{ kind:"dash", url:mpdUrl, referrer, cookie, user_agent }
  └─ bridge: kind=="dash" ⇒ probe_dash(mpdUrl, headers)
       → fetch+parse MPD :
           - rejette MPD dynamic (live) ⇒ media://error
           - liste les Representations vidéo de la 1ère Period (résolution, bitrate, ~taille estimée)
       → range PendingMedia{ payload:Dash(Mpd), headers, file_name } dans AppState (clé = probe_id)
       → emit media://variants{ probe_id, file_name, variants } ; surgit la fenêtre
  └─ frontend: HlsVariantDialog (réutilisé) affiche les qualités (meilleure pré-sélectionnée)
       → invoke add_media_download(probe_id, variant_index)
  └─ commands::add_media_download:
       → dispatch sur le kind du PendingMedia : Dash ⇒ résout la Representation vidéo choisie + Representation audio (default/lang)
       → nom .mp4, catégorie Video, crée Download{ kind:Dash, url:mpd_url, media_selection:{video,audio} } → upsert → reevaluate
  └─ launch: kind==Dash ⇒ download_core::dash::engine::download_dash(...)
       1. re-fetch+parse MPD ; résout les Representations video/audio depuis media_selection
       2. construit la liste de segments par piste selon l'adressage (init + médias ; Range pour SegmentBase/byterange)
       3. télécharge chaque segment (pause/cancel, rate-limiter, retries), .part→rename .seg, dans dest+".fghls/"
       4. concatène par piste (init préfixé) → video.cat / audio.cat
       5. si CENC: récupère la clé ClearKey (ContentProtection ou laurl→JWK)
       6. Remuxer.remux({ video, audio, decryption_key }) → dest.mp4 (ffmpeg -decryption_key … -c copy)
       7. nettoie le temp ; émet Completed
```

Côté UI, un download DASH apparaît comme les autres (badge « DASH », progression « X/N segments »), avec pause/reprise/annulation.

## 4. Réutilisation & généralisation de l'acquis 4a

### 4.1 `Remuxer` — clé de déchiffrement optionnelle
- `RemuxInputs` gagne `decryption_key: Option<String>` (clé hex CENC). HLS passe `None` (comportement inchangé).
- `SidecarRemuxer` (crate Tauri) : quand `decryption_key` est présent, préfixe **`-decryption_key <hex>`** avant chaque `-i` chiffré (option d'entrée ffmpeg). Sinon args inchangés (`-i … -c copy -movflags +faststart`).
- Le `FakeRemuxer` de test enregistre aussi `decryption_key` pour les assertions.

### 4.2 `media_io.rs` — helpers partagés (refactor)
Extraire de `hls/engine.rs` vers un nouveau module **`download-core/src/media_io.rs`** les fonctions communes, réutilisées par **hls** et **dash** :
- `work_dir(dest) -> PathBuf` (dossier `dest+".fghls"`).
- `fetch_to_vec(fetcher, url, range, headers, limiter, handle) -> Result<Option<Vec<u8>>>` (lecture complète rate-limitée, `None` = annulé).
- `write_segment(work, prefix, idx, bytes)` (écrit `.part` → rename atomique `.seg`).
- `concat_track(work, prefix, has_init) -> PathBuf` (concatène init + `.seg` triés).
- `emit_segments(tx, id, handle, done, total, status)`.
- `finish_cancelled(work)`.
`hls/engine.rs` est adapté pour consommer ces helpers (comportement identique, tests HLS toujours verts). Pas d'autre refactor.

### 4.3 `pending_media` généralisé + commande unique
- `PendingMedia` (state.rs) porte désormais un payload tagué :
  `enum MediaProbe { Hls(download_core::hls::playlist::MasterPlaylist), Dash(download_core::dash::mpd::Mpd) }`,
  `struct PendingMedia { probe: MediaProbe, headers, file_name, created }`.
- Une **seule** commande `#[tauri::command] add_media_download(probe_id, variant_index) -> Download` dispatche sur le payload : `Hls ⇒` logique 4a (media playlist url + audio url) ; `Dash ⇒` résout la Representation vidéo `variant_index` + Representation audio, persiste la sélection. **Remplace `add_hls_download`** (le front bascule `addHlsDownload` → `addMediaDownload`).
- `probe_hls`/`probe_dash` rangent chacun le bon `MediaProbe` puis émettent le même `media://variants`.

### 4.4 `DownloadKind::Dash` + `media_selection`
- `model::DownloadKind` gagne `Dash`. `launch` (commands.rs) : `Dash ⇒ dash::engine::download_dash(...)`.
- `model::Download` gagne `media_selection: Option<String>` (JSON kind-spécifique pour la reprise). Pour DASH : `{"video":"<reprId>","audio":"<reprId|null>"}`. Pour HLS : `null` (l'`url`+`audio_url` de 4a suffisent). Migration additive `ALTER TABLE downloads ADD COLUMN media_selection TEXT`.

### 4.5 Extension & frontend
- `extension/media-rules.js` : ajoute `isDashUrl(url)` (chemin `.mpd`) et `isDashContentType(ct)` (`application/dash+xml`). Le listener webRequest enregistre HLS **et** DASH (la détection par onglet et le popup sont déjà génériques en 4a).
- `extension/buildCapture.js` : `kind` accepte déjà n'importe quelle valeur ; le popup envoie `kind:"dash"` pour un `.mpd` (l'envoi `sendMedia` peut déduire le kind via `isDashUrl`/`isHlsUrl`).
- `extension/background.js` : `sendMedia(url, referrer)` détermine `kind` (`isDashUrl(url) ? "dash" : "hls"`).
- `bridge/server.rs` : route `kind=="dash"` ⇒ `commands::probe_dash` (comme `kind=="hls"` ⇒ `probe_hls`).
- Frontend : `api.addMediaDownload(probeId, variantIndex)` (remplace `addHlsDownload`) ; `DownloadRow` affiche un badge « DASH » quand `kind==='dash'` (badge HLS conservé).

## 5. Composants — module `download-core/dash/` (TDD)

### 5.1 `dash/mpd.rs` (pur)
- `parse_mpd(xml, base_url) -> Result<Mpd, CoreError>` via **`roxmltree`** (DOM lecture seule, léger).
- `Mpd { dynamic: bool, periods: Vec<Period> }` ; `Period { adaptation_sets: Vec<AdaptationSet> }` ;
  `AdaptationSet { content_type: ContentType (Video|Audio|Other), lang: Option<String>, representations: Vec<Representation>, content_protection: Option<ContentProtection> }` ;
  `Representation { id, bandwidth: u64, width: Option<u32>, height: Option<u32>, mime: Option<String>, addressing: Addressing }` ;
  `enum Addressing { Template(SegmentTemplate), List(SegmentList), Base(SegmentBase) }` ;
  `ContentProtection { default_kid: Option<String>, clearkey_laurl: Option<String>, scheme: ProtScheme }`.
- `dynamic = MPD@type=="dynamic"`. Résolution des `BaseURL` (Period/AdaptationSet/Representation) contre `base_url`. Hérite SegmentTemplate au niveau AdaptationSet ou Representation.

### 5.2 `dash/sidx.rs` (pur)
- `parse_sidx(bytes, sidx_offset) -> Vec<(u64, u64)>` : parse la boîte `sidx` (Segment Index) d'un fMP4 → liste des sous-segments `(start_absolu, end_inclusif)`. (Pour SegmentBase : on télécharge d'abord l'`Initialization`+`indexRange` pour lire le `sidx`, puis on en déduit les Range des médias.)

### 5.3 `dash/segments.rs` (pur)
- `build_segments(repr, addressing, mpd_base) -> SegmentPlan` où `SegmentPlan { init: Option<SegmentRef>, media: Vec<SegmentRef> }` et `SegmentRef { url, byte_range: Option<(u64,u64)> }`.
- Substitution de template : `$RepresentationID$`, `$Number$`, `$Number%0Nd$`, `$Time$`, `$Bandwidth$`. Expansion SegmentTimeline (`S@t/d/r`, `r=-1` exclu — VOD borné). SegmentList → URLs explicites. SegmentBase → `init` (indexRange) + médias issus du `sidx` (renseignés à l'exécution une fois le `sidx` lu).

### 5.4 `dash/clearkey.rs` (pur + orchestration réseau dans l'engine)
- Pur : `build_license_request(kid_hex) -> String` (JSON `{kids:[base64url(kid)],type:"temporary"}`) ; `parse_jwk_response(json) -> Result<String /*key hex*/, CoreError>` (extrait `keys[0].k` base64url → hex).
- **Extension `ByteFetcher` :** la requête de licence W3C ClearKey est un **POST** (corps JSON), non couvert par `probe`/`fetch_range`. On ajoute une méthode `async fn post(&self, url, body: &[u8], content_type: &str, headers: &RequestHeaders) -> Result<Vec<u8>, CoreError>` au trait `ByteFetcher`, **avec une impl par défaut** renvoyant `CoreError::Media("POST non supporté par ce fetcher")`. Seuls `ReqwestFetcher` (prod, via `reqwest`) et le fetcher de test DASH la surchargent — les fetchers HLS existants (`FakeFetcher`, `MapFetcher`) restent inchangés. C'est la seule extension du trait.
- Orchestration (engine) : si `ContentProtection` ClearKey avec `laurl` ⇒ `fetcher.post(laurl, build_license_request(kid), "application/json", headers)` ⇒ `parse_jwk_response` ⇒ clé hex. Si clé directement dans le MPD ⇒ l'utiliser. Si chiffré sans ClearKey exploitable ⇒ `CoreError::Media("DRM non supporté (ClearKey requis)")`.

### 5.5 `dash/select.rs` (pur)
- `best_video(mpd) -> (period_idx, adaptation_idx, repr_idx)` : plus haut `bandwidth` parmi les AdaptationSets vidéo de la 1ère Period.
- `pick_audio(mpd, lang: Option<&str>) -> Option<(…)>` : AdaptationSet audio par langue demandée sinon défaut/premier.
- `estimated_size(bandwidth, total_duration)` : réutilise la formule 4a ; `total_duration` depuis `MPD@mediaPresentationDuration` (ISO-8601).
- `variants(mpd) -> Vec<MediaVariant>` : la liste affichable (réutilise la forme `HlsVariant` de 4a, renommée `MediaVariant` côté backend/serde).

### 5.6 `dash/engine.rs`
- `download_dash<F: ByteFetcher, R: Remuxer>(fetcher, remux, id, mpd_url, selection, dest, handle, limiter, progress, headers) -> Result<Outcome, CoreError>`.
- Réutilise `media_io` (§4.2), `DownloadHandle`/`Outcome`, `RateLimiter`, le pattern reprise (`.seg` présents). Étapes : parse MPD → résout video/audio → SegmentBase: lit le `sidx` → build segments → télécharge init+médias par piste → concat → (clé ClearKey) → remux. Rejette `dynamic`.

## 6. Modèle de progression / reprise / annulation

Identiques à 4a (réutilisés) :
- Progression par **compteur de segments** : `Progress.segments_done/segments_total` (vidéo + audio), `total_bytes = None`.
- Taille estimée « ≈ » dans le dialogue (bitrate × durée ÷ 8).
- **Pause** : arrêt après segment courant, `.seg` conservés. **Reprise** : re-parse MPD, saute les `.seg` présents. **Annulation** : supprime `dest+".fghls"`.
- *Limite assumée :* URLs de segments à jeton expirable ⇒ erreur reprenable (comme 4a).

## 7. Garde-fous, sécurité & limites

- **Live (dynamic) non supporté** : `MPD@type="dynamic"` ⇒ `CoreError::Media("flux live DASH non supporté (à venir)")`.
- **DRM non supporté** : ContentProtection sans ClearKey exploitable (Widevine/FairPlay pssh, SAMPLE-AES) ⇒ erreur explicite.
- **Multi-Period** : seule la 1ère Period statique est traitée (limite signalée) ; xlink différé.
- **Chemins sûrs** : dossier temp sous la destination, segments nommés par **index** (anti-traversée, réutilisé) ; nom final `.mp4` via le helper basename de 4a.
- **En-têtes** (cookies/referer/user-agent) appliqués aux requêtes **MPD, licence ClearKey, init ET segments**.
- **Échec ffmpeg** (y compris clé invalide) ⇒ statut Error + extrait stderr.
- **Pas de boucle d'auto-capture** : FlashGet récupère via `reqwest` (hors navigateur).
- `roxmltree` est un parseur lecture seule sans exécution d'entités externes (pas d'XXE).

## 8. Stratégie de tests

- **Rust (TDD)** :
  - `mpd.rs` : chaque mode d'adressage (Template `$Number$`, Template+Timeline, List, Base) ; vidéo+audio AdaptationSets ; héritage SegmentTemplate ; BaseURL ; ContentProtection/ClearKey (default_KID, laurl) ; `static` vs `dynamic` ; résolution/bandwidth/durée.
  - `sidx.rs` : parse un `sidx` connu → sous-segments attendus.
  - `segments.rs` : substitutions (`$Number$`, `$Number%05d$`, `$Time$`, `$RepresentationID$`, `$Bandwidth$`), expansion Timeline (`t/d/r`), List, Base+sidx.
  - `clearkey.rs` : `build_license_request` (base64url du KID), `parse_jwk_response` (JWK→hex).
  - `select.rs` : `best_video`, `pick_audio` (langue/défaut), `estimated_size`, parse durée ISO-8601.
  - `engine.rs` : `download_dash` contre **fake fetcher + fake remuxer** — clair (concat+remux), chiffré (clé passée au remux via `decryption_key`), reprise (skip `.seg`), rejet `dynamic`, SegmentBase (sidx→ranges).
- **Vitest** : `isDashUrl` (chemin `.mpd`, query ignorée), `isDashContentType` (`application/dash+xml`, casse/charset), `sendMedia` choisit `kind` selon l'URL ; `buildCapture` avec `kind:"dash"`.
- **Manuel (smoke-test)** :
  1. DASH VOD **clair** (DASH-IF test vectors) couvrant Template/Timeline/List/Base → `.mp4` lisible avec **son**.
  2. DASH **ClearKey** (vecteur DASH-IF) → `.mp4` déchiffré lisible.
  3. MPD **dynamic** (live) → erreur « non supporté » propre.
  4. **Pause/reprise** d'un DASH en cours → reprend sans re-télécharger les segments faits.

## 9. Découpage prévisionnel du plan (à détailler en writing-plans)

1. deps `roxmltree` + squelette module `dash/`.
2. `dash/mpd.rs` (parser : Periods, AdaptationSets video/audio, Representations, les 4 adressages, ContentProtection/ClearKey, static/dynamic) — TDD.
3. `dash/sidx.rs` (parse `sidx`) — TDD.
4. `dash/segments.rs` (build segments : templates, timeline, list, base) — TDD.
5. `dash/select.rs` (best_video, pick_audio, estimated_size, durée ISO-8601, variants) — TDD.
6. `ByteFetcher::post` (méthode par défaut + override `ReqwestFetcher`) puis `dash/clearkey.rs` (build request, parse JWK) — TDD.
7. Refactor `download-core/media_io.rs` partagé + adaptation de `hls/engine.rs` (tests HLS verts) — TDD.
8. `Remuxer`/`RemuxInputs` `decryption_key` + `SidecarRemuxer` (Tauri) + `FakeRemuxer`.
9. `dash/engine.rs` `download_dash` (incl. SegmentBase sidx, ClearKey, reprise) contre fakes — TDD.
10. model `DownloadKind::Dash` + `media_selection` + migration storage — TDD.
11. Généralisation `pending_media` (`MediaProbe`) + `add_media_download` (remplace add_hls_download) + `probe_dash` + branche `launch` Dash.
12. Bridge route `kind=="dash"`.
13. Extension : `isDashUrl`/`isDashContentType` (Vitest) + `sendMedia` choisit le kind.
14. Frontend : `addMediaDownload` (remplace addHlsDownload) + badge DASH.
15. Vérification : suites Rust + Vitest + smoke-tests (clair/ClearKey/live/pause-reprise).

## 10. Décisions reportées / hors 4b-1

- **4b-2 et au-delà :** live (DASH dynamic + HLS live), multi-Period DASH + xlink, DRM (Widevine/FairPlay/SAMPLE-AES), SAMPLE-AES HLS, sous-titres (WebVTT pour HLS et DASH), sélection de piste audio/langue dans l'UI, ffmpeg multi-plateforme (Mac/Linux), qualité par défaut configurable.
- **Backlog technique antérieur** (4a, reporté) : byterange HLS à offset implicite peu testé ; compteur d'octets cosmétique à la reprise. (TTL `pending_media` et `media://error` déjà traités dans 4a.)
