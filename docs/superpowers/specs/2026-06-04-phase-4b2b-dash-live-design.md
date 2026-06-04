# Phase 4b-2b — Enregistrement DASH dynamic live (SegmentTimeline) (design)

Date : 2026-06-04. Statut : design validé, prêt pour writing-plans.

Empile sur la phase 4b-2a (enregistrement HLS live, branche `phase-4b2a-hls-live`,
non encore mergée) et réutilise la phase 4b-1 (DASH VOD + ClearKey, mergée dans
`main`). Voir specs `2026-06-04-phase-4b2a-hls-live-design.md` et
`2026-06-04-phase-4b1-dash-vod-design.md`.

## 1. Objectif et périmètre

Permettre d'**enregistrer un flux DASH dynamic live** (`MPD@type="dynamic"`,
adressage **SegmentTimeline**) vers un `.mp4`, là où le moteur le détecte et le
**rejette** aujourd'hui (`download_dash` → `Err("flux live DASH non supporté")`,
et `probe_dash` rejette les MPD dynamic).

Réutilise l'infrastructure d'enregistrement de 4b-2a : `DownloadStatus::Recording`,
le drapeau `stop` sur `DownloadHandle` (Stop=finaliser vs Cancel=jeter), la
finalisation de secours au démarrage, le modèle « capture jusqu'au Stop manuel ou
fin de flux ».

### Dans le périmètre
- DASH **dynamic** en **SegmentTemplate + SegmentTimeline** uniquement.
- **Modèle d'enregistrement** (réutilisé de 4b-2a) : capture jusqu'au **Stop
  manuel** *ou* la **fin de flux**, puis finalisation du `.mp4`.
- **Point de départ** (réutilisé de 4b-2a) : capture toute la fenêtre listée à la
  timeline puis les nouveaux segments à chaque rafraîchissement.
- **ClearKey/CENC** réutilisé de 4b-1 (clé via laurl → `ffmpeg -decryption_key`),
  avec **persistance de la clé** dans le work dir pour la reprise au démarrage.
- Vidéo + audio (AdaptationSets séparés) via la sélection existante.

### Hors périmètre (→ phases ultérieures)
- **DASH dynamic en `$Number$`+`@duration` sans timeline** (mode horloge-murale :
  `availabilityStartTime`, `timeShiftBufferDepth`, UTCTiming, calcul de
  disponibilité) → **4b-2c**.
- Multi-Period dynamic, xlink, sous-titres, sélection langue UI, DRM
  Widevine/FairPlay, ffmpeg multi-plateforme.
- HLS live (déjà couvert par 4b-2a).

## 2. Approche retenue

**Approche A — `dash/live.rs` parallèle, partage minimal des mécaniques
communes.** (Parmi 3 évaluées ; rejet de B « trait générique `LiveSource` » car
les protocoles diffèrent trop — clé de dédup séquence vs temps, chiffrement
segment-level vs ffmpeg-level, détection de fin différente — l'abstraction serait
fuyante et imposerait un refactor risqué du moteur HLS stable ; rejet de C car
elle laisse la dette de finalisation.)

Concrètement :
1. **Déplacer** `RefreshInterval` + `responsive_sleep` de `hls/live.rs` vers un
   nouveau module `crate::live`, renommer `RefreshInterval::FromTargetDuration` →
   `Derived` (neutre). HLS passe `target_duration`, DASH passe
   `minimum_update_period` à `compute(base_secs)`.
2. **Extraire** la séquence concat-pistes + remux en
   `media_io::finalize_and_remux` (paie la dette « règle de trois »), utilisée par
   `hls/live.rs` (conversion), le nouveau `dash/live.rs`, et la recovery Tauri.
3. Écrire `dash/live.rs::download_dash_live` parallèle à `download_hls_live`,
   réutilisant `mpd`/`select`/`segments`/`clearkey`/`media_io`.

## 3. Parser MPD + acceptation du live à la sonde

### `dash/mpd.rs`
Ajouter au parse du root : `minimum_update_period: Option<f64>` (depuis
`MPD@minimumUpdatePeriod`, ISO-8601 via `parse_iso8601_duration`). `struct Mpd`
gagne ce champ ; `dynamic` existe déjà. La **SegmentTimeline** est déjà parsée
(`parse_segment_template` lit `<SegmentTimeline>`/`<S t/d/r>`) et `build_segments`
produit la liste de segments d'une timeline — réutilisés tels quels par
rafraîchissement.

`availabilityStartTime`/`timeShiftBufferDepth` ne sont **pas** ajoutés (inutiles
en mode SegmentTimeline, qui liste explicitement la fenêtre — YAGNI ; ils
arriveront en 4b-2c pour le mode horloge-murale).

### `commands.rs::probe_dash_inner`
Supprimer le rejet `if mpd.dynamic { return Err("non supporté") }`. Un MPD dynamic
émet ses variantes normalement (`video_variants` est indépendant de `dynamic`).
Le flux : `probe_dash` → `add_media_download` → `launch` → `download_dash` → (si
dynamic) délègue à `download_dash_live`. `launch` inchangé (kind=Dash).

## 4. Module live partagé + finalisation (refactor Approche A)

### `crate::live` (nouveau `download-core/src/live.rs`)
Reçoit `RefreshInterval` (variantes `Derived` | `Fixed(Duration)`,
`compute(base_secs: f64) -> Duration`, floor 1 s, défaut 6 s) et
`pub(crate) responsive_sleep(dur, &DownloadHandle) -> bool`, déplacés depuis
`hls/live.rs`. `hls/live.rs` est mis à jour pour importer `crate::live::{…}` et
passer `target_duration` (changement mécanique ; les tests 4b-2a passent au même
endroit, juste l'import/le nom de variante changent).

### `media_io::finalize_and_remux`
```rust
pub async fn finalize_and_remux<R: Remuxer>(
    work: &Path,
    video_has_init: bool,
    audio_has_init: Option<bool>,   // None = pas de piste audio
    decryption_key: Option<String>,
    remux: &R, dest: &Path,
) -> Result<(), CoreError>
```
Fait `spawn_blocking(concat_track(work,"v",video_has_init))` (+ audio si `Some`)
puis `remux.remux(RemuxInputs { video, audio, decryption_key }, dest)`. Sites
convertis : `hls/live.rs`, `dash/live.rs` (nouveau), `recover_interrupted_recordings`
(Tauri). Les moteurs VOD `download_hls`/`download_dash` restent inchangés (backlog
optionnel).

## 5. Moteur `dash/live.rs::download_dash_live`

```rust
#[allow(clippy::too_many_arguments)]
pub async fn download_dash_live<F: ByteFetcher, R: Remuxer>(
    fetcher: &F, remux: &R, id: &str,
    mpd_url: &str, selection: &DashSelection, dest: &Path,
    handle: DownloadHandle, limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>, headers: &RequestHeaders,
    initial: Option<Mpd>, refresh: RefreshInterval,
) -> Result<Outcome, CoreError>
```

### Boucle de rafraîchissement
État conservé : un état **par piste** (vidéo + audio optionnelle) — chacune avec
son `HashSet<String>` des URLs déjà capturées, son drapeau init-fait, et son
**compteur de capture monotone** (pour nommer `<prefix>_<compteur>.seg`) ; plus la
clé ClearKey résolue (commune).

1. Fetch + parse le MPD (utilise `initial` au 1ᵉʳ tour, sinon refetch).
2. **1ᵉʳ tour seulement** : résout la Representation vidéo (via
   `select::video_variants` + `selection.video_index`) et audio (`pick_audio`).
   **Garde de mode** : l'adressage de la Representation vidéo doit être
   `Template` **avec** `timeline` ; sinon `Err("DASH live $Number$/SegmentList
   non supporté (à venir)")`. Résout la **clé ClearKey** une fois
   (`resolve_key`, réutilisé de 4b-1) ; si `Some(hex)`, écrit `<work>/clearkey.hex`.
3. Par piste : `build_segments(repr, &repr.base, 0.0)` sur le MPD rafraîchi →
   init (téléchargé **une fois** via le `HashSet`/drapeau) + segments de la
   fenêtre courante.
4. **Dédup par URL résolue** : pour chaque segment dont l'URL n'est pas dans le
   `HashSet`, télécharge (`fetch_to_vec`, byte-range éventuel) → écrit
   `<prefix>_<compteur>.seg` (compteur de capture monotone, ordre chronologique) →
   insère l'URL dans le set → émet `Recording`. (Pas de déchiffrement ici : CENC
   est géré par ffmpeg au remux, segments stockés tels quels.)
5. Entre segments : `is_cancelled()` → `finish_cancelled` + `Cancelled` ;
   `is_stopped()` → sortir pour finaliser.
6. **Fin de flux** : après capture, si le MPD rafraîchi n'est plus `dynamic`
   (`type="static"`) **ou** `minimum_update_period` est `None` → finalise. Sinon
   `responsive_sleep(refresh.compute(minimum_update_period_secs))` (interruptible)
   et reboucle.
7. **Finalisation** : `media_io::finalize_and_remux(work, v_has_init, audio?, key,
   remux, dest)`. Nettoyage du work dir (supprime aussi `clearkey.hex`), émet
   `Completed`.

### Dédup par URL — justification
Sous une SegmentTimeline glissante, `$Number$` est **instable** (les `<S>` de tête
disparaissent, l'index décale), tandis que l'**URL résolue d'un segment est
stable** (dérivée de `$Time$`/`$Number$` substitué sur la timeline courante). On
dédoublonne donc par URL, robuste quel que soit le template, et on nomme les
fichiers par un compteur de capture monotone (l'ordre de capture = chronologique
car la timeline est ordonnée et on n'ajoute que des segments plus récents).

### `dash/engine.rs::download_dash`
Là où il fait `if mpd.dynamic { return Err(...) }`, il **délègue** à
`download_dash_live(... initial: Some(mpd), RefreshInterval::Derived)` (passe le
MPD déjà parsé). VOD inchangé.

## 6. ClearKey live : persistance + reprise

- **Capture** : la clé ClearKey hex résolue est écrite dans `<work>/clearkey.hex`
  (co-localisée avec les segments chiffrés ; même cycle de vie).
- **Finalisation normale** : la clé en mémoire est passée à `finalize_and_remux` →
  `.mp4` déchiffré ; le work dir (et `clearkey.hex`) sont supprimés.
- **Reprise après crash** (`recover_interrupted_recordings`, Tauri) : lit
  `<work>/clearkey.hex` s'il existe → le passe en `decryption_key` à
  `finalize_and_remux`. Un enregistrement HLS (segments déjà en clair) n'a pas ce
  fichier → clé `None` → comportement 4b-2a inchangé.
- **Sécurité** : ClearKey = clé en clair (pas du DRM réel) ; stockage temporaire
  local supprimé à la fin, cohérent avec le stockage existant de cookies/headers.

## 7. Tests (TDD)

1. **Unitaires `dash/live.rs`** (fake fetcher + MPD évolutif : SegmentTimeline
   glissante puis MPD `static`) : capture+dédup par URL, `stop()`→finalise,
   `cancel()`→jette, fin de flux (static / `minimumUpdatePeriod` absent), ClearKey
   (clé résolue, `clearkey.hex` écrit, `decryption_key` transmis au FakeRemuxer),
   garde de mode (`$Number$`/SegmentList → `Err`). `RefreshInterval::Fixed`.
2. **`mpd.rs`** : parse de `minimumUpdatePeriod`.
3. **`media_io::finalize_and_remux`** : vidéo seule / vidéo+audio, avec/sans clé
   (non-régression du refactor).
4. **Smoke-tests offline** (`#[ignore]`, vrai ffmpeg) : `tiny_http` sert un MPD
   DASH évolutif (timeline glissante → static) + segments fMP4 **continus**
   (générés via le muxer dash de ffmpeg), pilote `download_dash_live` via
   `ReqwestFetcher` + `CliRemuxer` → `.mp4` lisible. Variante **ClearKey**
   (segments CENC chiffrés + serveur de licence JWK, réutilise la fixture 4b-1) →
   mp4 **déchiffré** lisible. + un cas **stop en cours**.
5. **Front** : aucun changement (réutilise la ligne « enregistrement » de 4b-2a —
   badge **LIVE** à côté du badge **DASH**, bouton Stop).

## 8. Découpage en tâches (pour writing-plans)

1. `dash/mpd.rs` : `Mpd.minimum_update_period` (+ test parse).
2. `crate::live` : déplacer `RefreshInterval` (renommer `Derived`) + `responsive_sleep` ;
   mettre à jour `hls/live.rs` pour les importer (tests 4b-2a verts).
3. `media_io::finalize_and_remux` (+ test) ; convertir `hls/live.rs` à l'utiliser.
4. `dash/live.rs::download_dash_live` : boucle, dédup par URL, compteur, garde de
   mode timeline, init-once, ClearKey résolu + `clearkey.hex`, fin de flux,
   finalisation. TDD avec fake fetcher évolutif.
5. `dash/engine.rs::download_dash` délègue le dynamic au live.
6. `commands.rs::probe_dash_inner` : accepter les MPD dynamic.
7. `recover_interrupted_recordings` : lire `<work>/clearkey.hex` → `decryption_key`.
8. Smoke-tests offline (clair + ClearKey + stop).
9. Vérification : suites Rust + Vitest + smoke-tests, 0 warning rustc.

## 9. Décisions / risques

- **`$Number$` sous timeline glissante** : non géré par calcul d'index (instable) ;
  contourné par la dédup-par-URL. Un dynamic en `$Number$`+`@duration` sans
  timeline est explicitement rejeté (→ 4b-2c).
- **Fin de flux** : détectée par `type="static"` ou `minimumUpdatePeriod` absent ;
  un live qui ne signale jamais sa fin se termine par Stop manuel.
- **Trou de capture** si rafraîchissement plus lent que la fenêtre (segment expiré
  avant fetch) : sauté, limite connue (idem HLS).
- **Reprise chiffrée** : dépend de `clearkey.hex` présent ; si la résolution de
  clé a échoué avant écriture, la reprise produira un fichier encore chiffré
  (cas limite, l'enregistrement aurait de toute façon échoué à la capture).
- **Refactor de `hls/live.rs`** (déplacement RefreshInterval + finalize) : couvert
  par les tests 4b-2a (unitaires + smoke).
