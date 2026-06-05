# Spec — Nom de fichier vidéo dérivé du titre de la page

**Date :** 2026-06-05
**Branche :** `phase-4b3-media-filename-title`
**Statut :** design validé par l'utilisateur, prêt pour le plan d'implémentation.

## 1. Problème

Quand une vidéo HLS/DASH est capturée puis lancée, le fichier produit s'appelle
`video.mp4`. Tous les téléchargements vidéo portent donc le même nom : on s'y perd
vite. L'utilisateur veut que le **titre de la vidéo** serve de nom de fichier.

### Origine du `video.mp4`

- Le média n'arrive **que** par la capture de l'extension navigateur :
  `probe_hls` / `probe_dash` sont `pub(crate)` et seul `bridge/server.rs` les appelle.
- L'extension, pour une capture média (`sendMedia`), envoie `url` + `kind` mais
  **aucun** `filename` (cf. `extension/background.js`, `buildCapture(url, {...headers, kind})`).
- Côté Rust, `probe_hls_inner` / `probe_dash_inner` font donc
  `suggested_name.unwrap_or_else(|| "video".to_string())`
  (`src-tauri/src/commands.rs:164` et `:220`), puis `add_media_download` ajoute `.mp4`
  → `video.mp4`.
- Le protocole de capture ne transporte aujourd'hui **aucun titre de page**.

## 2. Décisions (issues du brainstorm)

- **Source du titre :** `document.title` de l'onglet, exposé par `tab.title`
  (déjà accessible via `chrome.tabs.get(tabId)`, le `host_permissions: <all_urls>`
  suffit — aucune permission ajoutée). Pas d'`og:title` (nécessiterait `scripting`).
- **UX :** le titre est **auto-rempli ET éditable** dans la boîte de dialogue des
  variantes, avant le lancement. L'utilisateur garde le contrôle si le titre est moche
  (ex. nettoyer le suffixe « - YouTube »).
- **Transport (Approche A) :** réutiliser le champ `filename` existant du message de
  capture. `probe_hls`/`probe_dash` ne servent que pour du média : leur
  `suggested_name` est donc toujours un *titre*. Aucun changement de protocole
  (`protocol.rs`, `server.rs` inchangés).

## 3. Périmètre

### Inclus
- Récupération de `tab.title` dans l'extension et transmission via `filename` pour les
  captures média (immédiat + file d'attente `pending`/`flushPending`).
- Assainisseur de titre côté cœur Rust (`naming::sanitize_title`).
- Champ de nom éditable dans `HlsVariantDialog`, propagé jusqu'à `add_media_download`.

### Exclus (assumé)
- Nettoyage automatique du suffixe de site (« - YouTube », « | Dailymotion ») :
  l'utilisateur l'enlève dans le champ éditable.
- Déduplication de noms de fichiers identiques : comportement existant inchangé
  (pas de régression introduite, mais pas d'amélioration non plus).
- `og:title` / injection de content-script.
- Live (hors priorité, et la capture live passe par le même chemin de toute façon).

## 4. Architecture livrée

### 4.1 Extension (`extension/background.js`)

- `sendMedia(url, referrer, title)` : nouveau 3ᵉ paramètre. Quand la socket est prête,
  inclut le titre via `buildCapture(url, { ...headers, kind, filename: title })`.
  Sinon, empile `{ url, referrer, kind, filename: title, ts }` dans `pending`.
- Le handler `cmd === "sendMedia"` récupère déjà l'onglet
  (`chrome.tabs.get(req.tabId)`) : on en tire **aussi** `tab.title` et on le passe à
  `sendMedia`.
- `flushPending` : déjà générique sur `it.filename` ; il transmet donc le titre empilé
  sans modification (vérifier que la branche média inclut bien `filename`).
- `buildCapture` (`extension/buildCapture.js`) : **inchangé** — `filename` est déjà
  dans sa liste blanche de champs ; les valeurs vides restent omises.
- `manifest.json` : **inchangé** (aucune permission ajoutée).

### 4.2 Cœur Rust — `download-core/src/naming.rs`

Nouvelle fonction pure et testée :

```rust
/// Transforme un titre de page libre en radical de nom de fichier sûr (sans extension).
/// Vide / inutilisable → "video".
pub fn sanitize_title(raw: Option<&str>) -> String
```

Règles :
1. `None` ou chaîne vide/espaces → `"video"`.
2. Nettoyer les caractères interdits Windows et de contrôle :
   - `< > : " | ? *` et les caractères de contrôle (`\x00..\x1f`) → **supprimés**.
   - `/` et `\` → **remplacés par une espace** (préserve la séparation des mots).
   **Important :** ne PAS passer par `choose_file_name` (qui découpe sur `/` et `\`
   comme un chemin — un titre `AC/DC` deviendrait `DC`). Ici `/` et `\` sont traités
   comme du texte, pas comme des séparateurs de chemin.
3. Réduire les espaces multiples à un seul ; `trim`.
4. Couper les points et espaces de fin (interdits/problématiques sous Windows).
5. Si le radical (insensible à la casse, sans extension) correspond à un nom réservé
   Windows (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`) → préfixer `_`.
6. Plafonner à 120 caractères (sur frontières de caractères, pas d'octets coupés).
7. Si après tout ça la chaîne est vide → `"video"`.

L'extension `.mp4` est ajoutée séparément par `with_extension` (existant), pas ici.

### 4.3 Cœur Rust — `src-tauri/src/commands.rs`

- `probe_hls_inner` / `probe_dash_inner` :
  `let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());`
  en remplacement du `unwrap_or_else(|| "video".to_string())`.
  Le `file_name` (radical sûr, sans extension) est stocké dans `PendingMedia` et émis
  dans `media://variants` comme aujourd'hui.
- `add_media_download(app, probe_id, variant_index, file_name: Option<String>)` :
  nouveau paramètre `file_name` = nom édité par l'utilisateur.
  - Si `Some`, l'assainir aussi via `sanitize_title` (défense en profondeur contre une
    saisie UI hostile), sinon repli sur `pm.file_name`.
  - Puis `with_extension(.., "mp4")` (inchangé).
  - Le reste (résolution variante, dossier de destination, garde `dest_path.parent()`,
    upsert, `reevaluate`) est inchangé.

### 4.4 Frontend

- `api.ts` : `addMediaDownload(probeId, variantIndex, fileName: string)` →
  `invoke("add_media_download", { probeId, variantIndex, fileName })`.
- `components/HlsVariantDialog.tsx` : remplacer le `<p>{event.file_name}</p>` par un
  `<input>` contrôlé :
  - état `name`, initialisé à `event.file_name` dans le `useEffect` existant
    (en même temps que la variante par défaut).
  - suffixe `.mp4` affiché à droite du champ (purement visuel ; non éditable).
  - `onConfirm(event.probe_id, selected, name.trim())`.
  - si le champ est vidé par l'utilisateur, on envoie quand même la valeur ; Rust
    retombe sur `"video"` via `sanitize_title`.
- `App.tsx` : `handleHlsConfirm(probeId, variantIndex, fileName)` →
  `api.addMediaDownload(probeId, variantIndex, fileName)`.

## 5. Flux de données

```
[Page vidéo] tab.title
   │ popup "sendMedia" → background.js: chrome.tabs.get(tabId).title
   ▼
buildCapture(url, { kind, filename: title })  ──WS──►  bridge/server.rs
   ▼
probe_hls/dash(url, headers, suggested_name = title)
   ▼  naming::sanitize_title(title) → radical sûr (ex. "Ma vidéo - YouTube")
PendingMedia.file_name + event media://variants { file_name }
   ▼  (frontend)
HlsVariantDialog: <input value=file_name> .mp4   (éditable)
   ▼  onConfirm(probe_id, variant, editedName)
add_media_download(probe_id, variant, file_name = editedName)
   ▼  sanitize_title(editedName) + with_extension("mp4")
Download.file_name = "Ma vidéo - YouTube.mp4"  →  dossier Vidéos
```

## 6. Tests

### Rust (`naming.rs`, `#[cfg(test)]`)
- `sanitize_title(None)` et `Some("")` / `Some("   ")` → `"video"`.
- Caractères interdits supprimés : `Some("a:b?c*")` → `"abc"`.
- `AC/DC` **non découpé** : `Some("AC/DC - Live")` → `"AC DC - Live"`
  (`/` remplacé par une espace, jamais traité en séparateur de chemin) — test
  anti-régression clé.
- Noms réservés : `Some("CON")` → `"_CON"`.
- Plafond de longueur : titre de 300 caractères → ≤ 120, sur frontière de caractère.
- Unicode conservé : `Some("Café 日本")` reste lisible.
- Points/espaces de fin : `Some("titre...  ")` → `"titre"`.

### Frontend (Vitest, `HlsVariantDialog.test.tsx`)
- Le champ est pré-rempli avec `event.file_name`.
- Édition du champ puis « Télécharger » → `onConfirm` reçoit le nom édité.

### Extension
- Si une suite de tests existe pour `buildCapture`/`background`, ajouter un cas :
  une capture média inclut `filename` quand un titre est fourni. Sinon, couverture
  assurée par revue manuelle (l'extension n'a pas de runner dédié à ce stade).

### Critères verts
- `cargo test` (download-core + tauri lib) ✅, `npm test` (Vitest) ✅,
  build `tsc + vite` ✅, **0 warning rustc**.

## 7. Risques / limites (non bloquants)

- `document.title` contient souvent un suffixe de site ; assumé (champ éditable).
- Si une page n'a pas de titre (rare), `tab.title` peut être vide → repli `"video"`.
- Pas de dédup : deux vidéos au même titre → le second écrit par-dessus / selon le
  comportement d'écriture existant (inchangé, hors périmètre).
- Le plafond de 120 caractères est arbitraire mais sûr pour `MAX_PATH` Windows combiné
  au dossier de destination.

## 8. Garde-fou processus

Implémentation en **subagent-driven** : chaque prompt de subagent DOIT commencer par
`git checkout phase-4b3-media-filename-title` avant tout commit (un subagent a déjà
commité sur la mauvaise branche lors d'une phase précédente).
