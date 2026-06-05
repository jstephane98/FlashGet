# Phase 4b-2c — DASH dynamic live `$Number$` (horloge-murale) (design)

Date : 2026-06-05. Statut : design validé, prêt pour writing-plans.

Empile sur 4b-2b (DASH dynamic live SegmentTimeline, branche `phase-4b2b-dash-live`,
non mergée) et réutilise son moteur `download_dash_live`. Voir specs
`2026-06-04-phase-4b2b-dash-live-design.md` et `2026-06-04-phase-4b2a-hls-live-design.md`.

## 1. Objectif et périmètre

Enregistrer un flux **DASH dynamic live en mode `$Number$`+`@duration` sans
SegmentTimeline** — là où 4b-2b le rejette (`download_dash_live` →
`Err("DASH live $Number$/SegmentList non supporté (à venir)")`). Ce mode ne
**liste pas** les segments : il faut **calculer** quels numéros sont disponibles à
l'instant courant depuis l'**horloge murale** (`availabilityStartTime`, `now`,
durée de segment, `startNumber`, `timeShiftBufferDepth`).

### Dans le périmètre
- DASH dynamic, adressage **SegmentTemplate `$Number$` + `@duration` (sans timeline)**.
- **Horloge locale** (`SystemTime::now()`) + **live edge conservateur** (marge =
  une durée de segment) + **tolérance 404** (segment pas encore publié / expiré).
- Modèle d'enregistrement réutilisé de 4b-2a/4b-2b : capture jusqu'au **Stop
  manuel OU fin de flux** (MPD → `static` / `minimumUpdatePeriod` absent) ; départ
  = fenêtre DVR disponible (`timeShiftBufferDepth`) puis nouveaux segments.
- ClearKey, finalisation, recovery, stop/cancel, UI : **réutilisés tels quels** de
  4b-2b (aucun changement).

### Hors périmètre (→ phases ultérieures)
- **UTCTiming** (synchro d'horloge serveur pour corriger le skew) → phase ultérieure.
- Offsets de fuseau explicites dans `availabilityStartTime` (`+HH:MM`) : seul `Z`
  (UTC) est géré (limite mineure ; quasi tous les MPD live utilisent `Z`).
- Multi-Period dynamic, xlink, sous-titres, sélection langue UI, DRM Widevine/FairPlay.
- SegmentTimeline (déjà couvert par 4b-2b), HLS live (4b-2a).

## 2. Approche retenue

**Approche A — étendre `download_dash_live` avec une fonction de fenêtre par
mode** (parmi 3 ; rejet de B « moteur `download_dash_live_number` séparé » car il
duplique la boucle/dédup/finalisation tout juste consolidée ; rejet de C « trait
`SegmentWindow` » car sur-abstraction pour deux cas dans une fonction).

Le seul delta entre les deux modes dynamic est **comment calculer la fenêtre de
segments courante**. On ajoute : une horloge injectable, le parsing des attributs
horloge, une fonction de fenêtre `$Number$` ; le reste du moteur 4b-2b (boucle,
`DashLiveTrack` dédup-URL, finalisation, ClearKey, stop/recovery) est réutilisé.

## 3. Parsing MPD (`dash/mpd.rs`)

Ajouter au parse du root, dans `struct Mpd` (en plus de `minimum_update_period`
de 4b-2b) :
- `availability_start_time: Option<f64>` — epoch unix (s), depuis
  `MPD@availabilityStartTime` (datetime ISO-8601).
- `time_shift_buffer_depth: Option<f64>` — secondes, depuis
  `MPD@timeShiftBufferDepth` (durée ISO, réutilise `parse_iso8601_duration`).

Nouveau parseur **`parse_iso8601_datetime(s) -> Option<f64>`** (sans dépendance,
cohérent avec le `parse_iso8601_duration` fait main) : gère
`YYYY-MM-DDTHH:MM:SS(.fff)?(Z)?` (UTC) via l'algorithme `days_from_civil` (jours
depuis 1970-01-01) → `jours*86400 + h*3600 + m*60 + s` (+ fraction). Les offsets
non-`Z` renvoient `None` ou sont traités comme UTC (limite notée).

`SegmentTemplate` a déjà `duration`/`timescale`/`start_number` — inchangé.

## 4. Horloge injectable + calcul de disponibilité

### `crate::live::Clock`
```rust
#[derive(Debug, Clone, Copy)]
pub enum Clock { System, Fixed(f64) }
impl Clock {
    pub fn now_epoch(&self) -> f64 { /* System → SystemTime::now() depuis UNIX_EPOCH en secs_f64 ; Fixed(t) → t */ }
}
```

### `dash/segments.rs::live_number_window`
```rust
pub fn live_number_window(
    repr: &Representation, t: &SegmentTemplate,
    ast: f64, tsb: Option<f64>, now: f64, margin: f64,
) -> SegmentPlan
```
Avec `d = t.duration / t.timescale` (durée de segment, s) :
- `elapsed = now - margin - ast`. Si `d <= 0` ou `elapsed < d` → `SegmentPlan`
  avec `media` vide (rien encore disponible) ; `init` toujours résolu.
- **live edge conservateur** : `count = floor(elapsed / d)` ;
  `edge_N = t.start_number + count - 1` (dernier numéro pleinement publié ; marge
  `d` ⇒ on ne demande jamais un segment trop récent).
- **fenêtre DVR** : `span = tsb.map(|x| floor(x / d)).unwrap_or(0)` ;
  `start_N = max(t.start_number, edge_N - span)`.
- `media` = pour `N` de `start_N` à `edge_N` : `SegmentRef { url:
  resolve_uri(base, expand($Number$ → N)), byte_range: None }` (réutilise la
  substitution `expand` existante). `init` = `t.init` résolu.

À chaque rafraîchissement, recalcul depuis `now` (qui avance de
`minimum_update_period`) → `edge_N` progresse, nouveaux numéros en tête,
`DashLiveTrack::capture` dédoublonne (URL). Au 1ᵉʳ tour = la fenêtre DVR
(back-buffer). **Marge par défaut = `d`** (une durée de segment).

## 5. Intégration moteur (`dash/live.rs`)

`download_dash_live` gagne un paramètre `clock: Clock`.

**Dispatch de mode** (résolution unique, garde relâchée) — pour la Representation
vidéo et audio :
- `Addressing::Template(t)` avec `t.timeline.is_some()` → **mode timeline** (4b-2b).
- `Addressing::Template(t)` avec `t.duration.is_some()` (sans timeline) → **mode
  `$Number$` horloge-murale** ; exige `mpd.availability_start_time` présent, sinon
  `Err("availabilityStartTime requis pour DASH live $Number$")`.
- sinon → `Err` (adressage live non géré).

Par rafraîchissement, par piste, le `SegmentPlan` est bâti selon le mode :
`build_segments(repr, base, duration_secs)` (timeline) ou
`live_number_window(repr, t, ast, tsb, clock.now_epoch(), d)` (`$Number$`). Le
`DashLiveTrack::capture` existant le consomme (dédup-URL, écriture, ClearKey,
finalisation — tout réutilisé). Boucle/refresh/fin-de-flux/stop/cancel inchangés.

`dash/engine.rs::download_dash` délègue avec `Clock::System`.

### Tolérance 404
`http.rs::fetch_range` mappe un **HTTP 404/410 → `CoreError::NotFound`** (variante
existante) au lieu de `Network`. `DashLiveTrack::capture` traite
`Err(CoreError::NotFound)` (issu de `fetch_to_vec`) comme **« sauter ce segment,
ne pas le marquer vu, continuer »** — retenté au rafraîchissement suivant — au lieu
de tuer l'enregistrement. La marge conservatrice évite les 404 sous horloge
correcte ; cette tolérance couvre le skew résiduel / segment expiré. **Effet sur le
VOD/régulier** : un 404 reste une erreur (variante `NotFound`) → échoue toujours,
pas de régression de comportement. Limite : un segment durablement absent = un
fetch gâché par rafraîchissement (borné).

## 6. Tests (TDD)

1. **`dash/mpd.rs`** : parse `availabilityStartTime` + `timeShiftBufferDepth` ; le
   parseur datetime sur points connus (`1970-01-01T00:00:00Z`→0.0 ;
   `2001-09-09T01:46:40Z`→1_000_000_000.0 ; avec `.fff`).
2. **`dash/segments.rs::live_number_window`** : `tsb=None` → segment de tête
   uniquement ; `tsb` donné → `span` segments ; deux `now` croissants → `edge_N`
   avance d'un (nouveau numéro) ; `elapsed < d` → media vide ; URLs `$Number$`
   correctes (substitution + résolution base).
3. **`dash/live.rs` (intégration, `Clock::Fixed`)** : MPD dynamic `$Number$`
   (`availabilityStartTime` calé pour rendre N segments dispo), un tour puis MPD
   `static` → capture → finalise → `Completed`, octets attendus ; garde relâchée ;
   **tolérance 404** (un segment 404 → sauté, non fatal, `Completed`) ; `$Number$`
   sans `availabilityStartTime` → `Err`.
4. **`tests/real_http.rs` (tiny_http)** : `fetch_range` sur un 404 →
   `CoreError::NotFound`.
5. **Pas de nouveau smoke ffmpeg dédié** : le chemin réel concat+remux du DASH
   dynamic live est déjà validé par `dash_live_smoke` (4b-2b), identique ici ; le
   delta (`$Number$` = calcul de fenêtre) est couvert par les unitaires +
   l'intégration `Clock::Fixed`. Un smoke `Clock::System` serait non déterministe
   (numéros dérivés de l'horloge réelle) — écarté à dessein.
6. **Front** : aucun changement.

## 7. Découpage en tâches (pour writing-plans)

1. `dash/mpd.rs` : `parse_iso8601_datetime` + `Mpd.availability_start_time` +
   `Mpd.time_shift_buffer_depth` (+ tests).
2. `crate::live::Clock` (+ test `Fixed`).
3. `http.rs::fetch_range` : mapper 404/410 → `CoreError::NotFound` (+ test real_http).
4. `dash/segments.rs::live_number_window` (+ tests math).
5. `dash/live.rs` : paramètre `clock`, dispatch de mode (relâche la garde), branche
   `$Number$` via `live_number_window` ; `capture` tolère `NotFound`. TDD intégration.
6. `dash/engine.rs::download_dash` : passe `Clock::System` à la délégation.
7. Vérification : suites Rust + Vitest + smoke 4b-2b inchangés, 0 warning rustc.

## 8. Décisions / risques

- **Skew d'horloge > marge** : sans UTCTiming, un client à l'heure fausse calcule
  un mauvais edge → 404 systématiques → la tolérance 404 saute les segments
  (enregistrement vide plutôt que crash). Compromis accepté (horloge locale) ;
  UTCTiming en phase ultérieure.
- **`timeShiftBufferDepth` absent** : `span=0` → départ au live edge (pas de
  back-buffer), puis suite. Acceptable.
- **`seen` HashSet** : croît sur la durée (déjà documenté en 4b-2b ; même
  trade-off).
- **Réutilisation `download_dash_live`** : la branche timeline (4b-2b) est couverte
  par ses tests ; le dispatch n'altère pas son chemin.
