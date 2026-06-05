# Phase 4b-2a — Enregistrement HLS live (design)

Date : 2026-06-04. Statut : design validé, prêt pour writing-plans.

Empile sur la Phase 4 vidéo VOD (4a HLS + 4b-1 DASH, mergées dans `main`). Voir
les specs `2026-06-04-phase-4a-hls-vod-design.md` et
`2026-06-04-phase-4b1-dash-vod-design.md`.

## 1. Objectif et périmètre

Permettre d'**enregistrer un flux HLS live** (media playlist sans
`#EXT-X-ENDLIST`) vers un fichier `.mp4`, là où le moteur actuel le détecte et
le **rejette** (`download_hls` → `Err("flux live non supporté (à venir)")`).

**Découpage assumé** (principe acté en 4b-1 : un sous-système indépendant à la
fois, chacun son cycle spec→plan→impl). La roadmap notait « 4b-2 = live +
multi-Period + xlink » : ces morceaux sont indépendants et traités séparément.
Cette phase = **HLS live uniquement (4b-2a)**.

### Dans le périmètre
- HLS **live** : VOD-style capture d'une media playlist qui se rafraîchit.
- **Modèle d'enregistrement** : capture jusqu'au **Stop manuel** *ou* la **fin
  de flux** (`ENDLIST` apparaît / la playlist cesse d'avancer), puis
  finalisation du `.mp4`.
- **Point de départ** : télécharge toute la fenêtre actuellement listée (le
  back-buffer DVR disponible) puis continue avec les nouveaux segments.
- **Chiffrement** : AES-128 (clé en clair), réutilisé de 4a, **rotation de clé**
  gérée (clé par-segment).
- Variantes (master playlist) : réutilise la sélection de qualité 4a. Audio
  séparé (EXT-X-MEDIA) traité symétriquement.
- **Finalisation de secours au redémarrage** si l'app s'est arrêtée pendant un
  enregistrement.

### Hors périmètre (→ phases ultérieures)
- **DASH dynamic live** (calcul de disponibilité à l'horloge murale) → **4b-2b**
  (phase suivante, empilée).
- Pause/reprise d'un live (la fenêtre glisse — non applicable).
- Multi-Period DASH, xlink, sous-titres, sélection de langue UI, SAMPLE-AES,
  DRM (Widevine/FairPlay), ffmpeg multi-plateforme.
- Traitement spécial des discontinuités (codec/résolution changeants) au-delà de
  ce que `ffmpeg -c copy` tolère.

## 2. Approche retenue

**Approche 1 — moteur live dédié réutilisant `media_io`** (parmi 3 évaluées ;
les alternatives « branche live dans `download_hls` » et « abstraction
`SegmentSource` générique » rejetées : la 1ʳᵉ mélange deux flux de contrôle très
différents dans une fonction déjà dense, la 2ᵈ est un refactor prématuré — YAGNI,
l'abstraction se justifiera quand DASH live arrivera en 4b-2b, règle de trois).

Nouveau module `hls/live.rs` avec une fonction `download_hls_live` parallèle à
`download_hls`, réutilisant intégralement `media_io` (work dir, `fetch_to_vec`,
`write_segment`, `concat_track`, `finish_cancelled`, `emit_segments`), la crypto
AES-128 (`crypto.rs`), et le `Remuxer` (ffmpeg). Le chemin VOD n'est pas modifié.

## 3. Cœur : `hls/live.rs::download_hls_live`

Signature (calquée sur `download_hls`) :
```rust
#[allow(clippy::too_many_arguments)]
pub async fn download_hls_live<F: ByteFetcher, R: Remuxer>(
    fetcher: &F, remux: &R, id: &str,
    media_url: &str, audio_url: Option<&str>, dest: &Path,
    handle: DownloadHandle, limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>, headers: &RequestHeaders,
    initial: Option<MediaPlaylist>, // playlist vidéo déjà parsée par download_hls (sinon refetch)
    refresh: RefreshInterval,       // intervalle injectable pour les tests (voir §8)
) -> Result<Outcome, CoreError>
```

### Boucle de rafraîchissement (par piste : vidéo + audio optionnelle)
État conservé entre rafraîchissements : un **curseur de séquence absolue**
`next_seq` par piste, un drapeau « init `EXT-X-MAP` déjà téléchargé », et le
`key_cache: HashMap<String,[u8;16]>`.

À chaque tour :
1. `fetch_to_vec` + `parse_media` de la media playlist (vidéo ; idem audio si
   `audio_url`). Au 1ᵉʳ tour, télécharge l'init `EXT-X-MAP` (`v_init.seg` /
   `a_init.seg`) une seule fois.
2. Refuse tout chiffrement non AES-128 (garde héritée de 4a → `Err`).
3. Pour chaque segment d'index `idx` : **séquence absolue** = `media_sequence +
   idx`. Si `< next_seq` → déjà capturé, skip. Sinon : `fetch_to_vec` →
   (déchiffrement AES-128 si `key`, IV = `iv_for_segment(k.iv, abs_seq)`) →
   `write_segment` sous le nom **`<prefix>_<abs_seq>.seg`** ; `next_seq =
   abs_seq + 1` ; `emit_segments(... Recording)`.
4. Entre chaque segment : `handle.is_cancelled()` → `finish_cancelled` +
   `Cancelled` ; `handle.is_stopped()` → sortir de la boucle pour finaliser.
5. En fin de tour : si la playlist contient `ENDLIST` (`!is_live`) → **fin de
   flux**, sortir pour finaliser. Si `is_stopped()` → finaliser. Sinon
   **attendre** `refresh.next(target_duration)` (sleep interruptible : ticks
   courts qui vérifient `is_stopped`/`is_cancelled` pour la réactivité) puis
   reboucler.

### Finalisation
`concat_track` (vidéo, puis audio si présent) en **ordre numérique** des
fichiers `<prefix>_<seq>.seg` → `remux.remux(RemuxInputs { video, audio,
decryption_key: None }, dest)` (AES-128 déjà déchiffré au niveau segment, comme
le VOD HLS). Nettoyage du work dir, `Progress{status:Completed}`, `Ok(Completed)`.

### Ordre numérique des segments
`concat_track` ordonne aujourd'hui par index/nom. Les séquences live peuvent être
grandes et non nulles. **Décision** : étendre `concat_track` (ou un helper
`media_io`) pour **trier numériquement** les `.seg` d'un préfixe par leur suffixe
entier, plutôt qu'un tri lexical fragile. Couvert par test unitaire.

## 4. Sémantique d'arrêt — trois signaux sur `DownloadHandle`

Ajout d'un troisième drapeau à `DownloadHandle` (aujourd'hui `pause`, `cancel`,
`downloaded`) :
```rust
stop: Arc<AtomicBool>,        // + pub fn stop(&self) / pub fn is_stopped(&self)
```

| Signal | Méthode | Effet live | Effet VOD |
|---|---|---|---|
| **Stop** (nouveau) | `stop()` / `is_stopped()` | sort de la boucle, **remux le partiel → `Completed`** | ignoré (VOD se termine seul) |
| **Cancel** | `cancel()` | **jette** le work dir → `Cancelled` | inchangé |
| **Pause** | `pause()` | **non applicable** (ignoré) | inchangé (reprise VOD) |

Le chemin VOD (`download_hls`, `download_dash`) n'inspecte pas `stop` → intact.

## 5. Détection / dispatch

La détection live se fait **au download**, pas à la sonde (`probe_hls` parse le
master = variantes ; `is_live` vit sur la media playlist).

`download_hls` fetch déjà la media playlist vidéo. Là où il fait aujourd'hui
`return Err("flux live non supporté (à venir)")`, il **délègue à
`download_hls_live`** en lui passant la media playlist déjà parsée (évite un
re-fetch ; `download_hls_live` accepte une playlist initiale optionnelle, sinon
la refetch). VOD et live partagent l'entrée `download_hls` ; **le `launch` Tauri
est inchangé** (`kind=Hls → download_hls`).

`probe_hls` doit produire une variante unique pour une **URL media-playlist
directe** (live sans master) — comportement à confirmer/ajuster (test).

## 6. Statut, commande et persistance (crate Tauri)

### Nouveau statut `DownloadStatus::Recording`
Ajouté à l'enum `model::DownloadStatus` (sérialisé `"recording"`). Lifecycle
distinct (capture non bornée). Transitions : `Recording` → `Completed` (stop
manuel / fin de flux), → `Error` (échec réseau persistant), → supprimé (cancel).

**Pas d'impact ordonnanceur** : les slots se comptent sur `handles.len()` (un
enregistrement garde son handle → compté actif comme un download normal) ;
`scheduler::pick_to_start` inchangé. Vérifier qu'aucun chemin ne filtre les
statuts d'une façon qui exclurait `Recording` des actifs.

### Commande `stop_recording`
```rust
#[tauri::command]
pub fn stop_recording(app: AppHandle, id: String) -> Result<(), String>
// pose handle.stop() sur le handle actif ; le moteur finalise (remux) et émet
// Completed ; le relais de progression persiste Completed ; reevaluate libère
// le slot. No-op si le handle n'existe pas (déjà terminé).
```
Distincte de `cancel_download` (jette le fichier) et `pause_download` (VOD).

### Émission de progression live
`segments_done` croît, `segments_total = None`, `downloaded_bytes` croît,
`status = Recording`. Le relais de progression (`launch`) **persiste désormais
aussi `Recording`** (en plus de Completed/Paused/Error) pour qu'un redémarrage
détecte les enregistrements interrompus (§7).

## 7. Finalisation de secours au redémarrage

Au démarrage de l'app, scanner les lignes en `Recording` (interrompues — la
fenêtre live a glissé, pas de reprise transparente possible) :
- work dir **non vide** → **finalisation de secours** : `concat_track` + remux →
  `.mp4`, statut `Completed`. Sauve une longue capture après un crash, réutilise
  le chemin de finalisation.
- work dir **vide** → la ligne est retirée (rien à sauver).

Implémenté comme une passe de démarrage (dans l'init de l'app, après ouverture du
storage). Nécessite ffmpeg disponible (déjà embarqué). Aucune reprise de capture
n'est tentée.

## 8. Tests (TDD, méthode projet)

1. **Unitaires `download-core` (`hls/live.rs`)** avec un fake fetcher servant une
   media playlist **évolutive** (renvoie des playlists successives : la
   `media-sequence` avance, la fenêtre glisse, puis un dernier tour avec
   `ENDLIST`). Cas :
   - capture la fenêtre initiale puis les nouveaux segments ; **dédup par
     séquence absolue** (un segment déjà capturé n'est pas re-téléchargé).
   - `stop()` mid-capture → finalise → `Completed`, contenu concaténé attendu.
   - `cancel()` → jette le work dir → `Cancelled`.
   - **fin de flux** (`ENDLIST` à un tour ultérieur) → finalise auto.
   - **AES-128 live + rotation de clé** (URI de clé qui change en cours).
   - `concat_track` : ordre **numérique** des `.seg`.
   - **Intervalle de rafraîchissement injectable** : `RefreshInterval` (ou un
     paramètre `Duration`) réglé à ~0–10 ms en test pour ne pas dormir.
2. **Smoke-test offline** (façon 4b-1, `#[ignore]` car ffmpeg réel) — `tiny_http`
   sert une media playlist live qui évolue sur quelques rafraîchissements puis
   ajoute `ENDLIST`, plus des segments TS ; pilote `download_hls_live` via
   `ReqwestFetcher` + un `CliRemuxer` (vrai ffmpeg) → `.mp4` lisible/décodable.
   Un second cas : **stop en cours d'enregistrement** → `.mp4` finalisé non vide.
3. **Front (Vitest)** : rendu de la ligne `recording` (libellé, badge LIVE, bouton
   Stop), helper de minuteur (pur), type `DownloadStatus` étendu.

## 9. Découpage en tâches (pour writing-plans)

1. `DownloadHandle` : drapeau `stop` + `stop()`/`is_stopped()` (+ tests). VOD
   inchangé.
2. `model` : `DownloadStatus::Recording` (+ sérialisation, mises à jour
   exhaustives des `match`).
3. `media_io` : tri **numérique** des `.seg` dans `concat_track` (+ test).
4. `hls/live.rs` : `download_hls_live` — boucle de rafraîchissement, curseur de
   séquence, dédup, AES-128 + rotation, init une fois, sleep interruptible,
   finalisation. TDD avec fake fetcher évolutif + `RefreshInterval` injectable.
5. `hls/engine.rs` : `download_hls` délègue au live au lieu de rejeter (passe la
   playlist déjà parsée).
6. Crate Tauri : commande `stop_recording` ; persistance de `Recording` dans le
   relais de progression ; enregistrement de la commande.
7. Crate Tauri : passe de **finalisation de secours au redémarrage**.
8. `probe_hls` : confirmer/ajuster la gestion d'une URL media-playlist directe.
9. Front : type `DownloadStatus` + `STATUS_LABEL` ; `api.stopRecording` ;
   `DownloadRow` mode enregistrement (libellé MM:SS/segments/taille, barre rouge
   animée, badge LIVE, bouton Stop) ; minuteur local ; câblage `onStop`.
10. Smoke-tests offline (capture jusqu'à `ENDLIST` ; stop en cours).
11. Vérification : suites Rust + Vitest + smoke-tests, 0 warning rustc.

## 10. Décisions / risques

- **Trou dans la fenêtre** si un rafraîchissement est trop lent (segment expiré
  avant fetch) : on saute le trou (irrécupérable). Mitigation : cadence ≤
  `target_duration`, download prompt. Limite connue.
- **Discontinuités** (pubs, changement de résolution) : on s'appuie sur
  `ffmpeg -c copy` ; pas de traitement spécial.
- **Disque non borné** pendant la capture : maîtrisé par l'utilisateur via Stop.
- **Redémarrage** : pas de reprise de capture, seulement finalisation du capturé.
