# Spec — Nom de repli yt-dlp (uploader + id quand le titre manque)

**Date :** 2026-06-05
**Branche :** `phase-4c3-ytdlp-name-fallback` (empilée sur `phase-4c2-ytdlp-extension`)
**Statut :** design validé par l'utilisateur, prêt pour le plan.

## 1. Problème
Certains sites (Facebook surtout) ne fournissent pas de `title` via `yt-dlp -J` →
`parse_formats` renvoie un titre vide → `sanitize_title` retombe sur « video » →
fichier générique « video.mp4 ». On veut un nom plus utile (et unique) dans ce cas.

## 2. Décision (brainstorm 2026-06-05)
Quand le `title` manque, replier sur **`uploader - id`** (lisible via le nom de la
page/chaîne + unique grâce à l'id, donc pas de collision entre vidéos sans titre du même
compte). Chaîne de repli : `title` → `uploader - id` → `id` → `""` (→ « video » via
`sanitize_title`).

## 3. Périmètre
### Inclus
- `parse_formats` extrait aussi `id` et `uploader` de la sortie `-J`.
- Fonction pure `pick_name(title, uploader, id)` + tests.
- `parse_formats` renvoie `(pick_name(...), formats)`.

### Exclus (assumé)
- Déduplication globale des noms identiques (inchangée).
- Détection de titres « génériques mais non vides » (ex. yt-dlp renvoie « Video » comme
  vrai titre → respecté tel quel).
- Aucun changement front/UI/commande (le consommateur fait déjà
  `sanitize_title(Some(&name))` + `ensure_extension(.., "mp4")`).

## 4. Architecture (cœur `download-core/src/ytdlp.rs`)
- `RawInfo` gagne `#[serde(default)] id: Option<String>` et
  `#[serde(default)] uploader: Option<String>`.
- Nouvelle fonction pure :
  ```rust
  /// Choisit un nom d'affichage à partir des champs yt-dlp.
  /// title (non vide) → "uploader - id" (si uploader) → id → "".
  pub fn pick_name(title: &str, uploader: Option<&str>, id: Option<&str>) -> String
  ```
  Règles (dans l'ordre), avec `uploader`/`id` considérés « présents » seulement si non
  vides après `trim` :
  1. `title.trim()` non vide → renvoyer le titre trimé.
  2. sinon, `uploader` présent ET `id` présent → `"{uploader} - {id}"`.
  3. sinon, `uploader` présent (id absent) → `"{uploader}"` (PAS de tiret orphelin).
  4. sinon, `id` présent → `id`.
  5. sinon → `""`.
- `parse_formats` calcule `let name = pick_name(&title, info.uploader.as_deref(),
  info.id.as_deref());` et renvoie `(name, formats)` (la 1ʳᵉ valeur du tuple est désormais
  le « nom d'affichage », plus seulement le titre brut).

## 5. Flux
```
yt-dlp -J → parse_formats → pick_name(title, uploader, id) → (name, formats)
  → probe_ytdlp_inner: sanitize_title(Some(&name)) → media://variants (champ éditable)
  → add_media_download → ensure_extension(.., "mp4")
```

## 6. Tests (cœur, `#[cfg(test)]`)
- `pick_name`:
  - `pick_name("Ma vidéo", Some("Chaîne"), Some("id1"))` == `"Ma vidéo"` (titre prioritaire).
  - `pick_name("", Some("Ma Page"), Some("123"))` == `"Ma Page - 123"`.
  - `pick_name("   ", None, Some("123"))` == `"123"` (titre blanc + pas d'uploader).
  - `pick_name("", Some("Ma Page"), None)` == `"Ma Page"`.
  - `pick_name("", None, None)` == `""`.
- `parse_formats` : un JSON sans `title` mais avec `uploader`+`id` → nom == `"uploader - id"`
  (et les formats restent corrects). Les tests existants (titre présent) restent verts.
- Critères : `cargo test` (download-core + tauri) ✅, Vitest inchangé, build, **0 warning**.

## 7. Risques (non bloquants)
- `uploader - id` peut être long pour certains sites ; `sanitize_title` plafonne déjà à
  120 caractères.
- Un site renvoyant un `title` générique non vide (« Video ») n'est pas amélioré
  (hors périmètre ; on ne devine pas la généricité).

## 8. Garde-fou
Implémentation subagent-driven : `git checkout phase-4c3-ytdlp-name-fallback` avant tout
commit de subagent.
