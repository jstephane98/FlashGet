# Nom de repli yt-dlp (uploader + id) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **GARDE-FOU GIT (obligatoire) :** chaque subagent DOIT exécuter `git checkout phase-4c3-ytdlp-name-fallback` avant tout commit.

**Goal:** Quand `yt-dlp -J` ne fournit pas de titre (ex. Facebook), nommer le fichier `uploader - id` (lisible + unique) au lieu de `video.mp4`.

**Architecture:** Logique pure dans `download-core/src/ytdlp.rs` : `parse_formats` extrait aussi `id`/`uploader`, et une nouvelle fonction `pick_name` choisit le meilleur nom (title → "uploader - id" → "uploader" → id → ""). Aucun changement consommateur (probe_ytdlp_inner fait déjà `sanitize_title` + `ensure_extension`).

**Tech Stack:** Rust (crate `download-core`), `serde`/`serde_json`.

**Spec :** `docs/superpowers/specs/2026-06-05-phase-4c3-ytdlp-name-fallback-design.md`

---

## File Structure

- `src-tauri/crates/download-core/src/ytdlp.rs` — **modifier** : `RawInfo` += `id`/`uploader` ; nouvelle fn `pick_name` ; `parse_formats` renvoie `pick_name(...)` ; tests.

(Aucun autre fichier : le consommateur `commands.rs::probe_ytdlp_inner` utilise déjà la 1ʳᵉ valeur du tuple comme nom suggéré — sémantique inchangée.)

---

## Task 1: `pick_name` + extraction id/uploader (TDD)

**Files:**
- Modify/Test: `src-tauri/crates/download-core/src/ytdlp.rs`

- [ ] **Step 1: Écrire les tests qui échouent**

Dans le `#[cfg(test)] mod tests` de `ytdlp.rs`, ajouter :

```rust
    #[test]
    fn pick_name_prefers_title() {
        assert_eq!(pick_name("Ma vidéo", Some("Chaîne"), Some("id1")), "Ma vidéo");
        // titre entouré d'espaces : trimé
        assert_eq!(pick_name("  Ma vidéo  ", None, None), "Ma vidéo");
    }

    #[test]
    fn pick_name_falls_back_to_uploader_and_id() {
        assert_eq!(pick_name("", Some("Ma Page"), Some("123")), "Ma Page - 123");
        // titre blanc => traité comme vide
        assert_eq!(pick_name("   ", Some("Ma Page"), Some("123")), "Ma Page - 123");
    }

    #[test]
    fn pick_name_uploader_only_has_no_orphan_dash() {
        assert_eq!(pick_name("", Some("Ma Page"), None), "Ma Page");
        // uploader/ id blancs => considérés absents
        assert_eq!(pick_name("", Some("  "), Some("123")), "123");
    }

    #[test]
    fn pick_name_id_only_then_empty() {
        assert_eq!(pick_name("", None, Some("123")), "123");
        assert_eq!(pick_name("", None, None), "");
        assert_eq!(pick_name("", Some(""), None), "");
    }

    #[test]
    fn parse_formats_uses_uploader_id_when_no_title() {
        let json = r#"{
            "id": "987",
            "uploader": "Ma Page FB",
            "formats": [
                {"format_id":"hd","height":720,"width":1280,"ext":"mp4","vcodec":"avc1","acodec":"mp4a","filesize":1000,"tbr":900.0}
            ]
        }"#;
        let (name, formats) = parse_formats(json).unwrap();
        assert_eq!(name, "Ma Page FB - 987");
        assert_eq!(formats.len(), 1);
    }
```

- [ ] **Step 2: Lancer pour vérifier l'échec**

Run: `cd src-tauri/crates/download-core && cargo test pick_name`
Expected: FAIL — `cannot find function pick_name`. (Et `parse_formats_uses_uploader_id_when_no_title` échoue car `parse_formats` renvoie encore le titre brut « ».)

- [ ] **Step 3: Ajouter `id`/`uploader` à `RawInfo`**

Dans `ytdlp.rs`, l'actuel :
```rust
#[derive(Deserialize)]
struct RawInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    formats: Vec<RawFormat>,
}
```
devient :
```rust
#[derive(Deserialize)]
struct RawInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    formats: Vec<RawFormat>,
}
```

- [ ] **Step 4: Implémenter `pick_name`**

Ajouter (après `parse_formats`, ou juste avant — au niveau module) :
```rust
/// Choisit un nom d'affichage à partir des champs yt-dlp.
/// Ordre : `title` (non vide, trimé) → `"uploader - id"` → `"uploader"` → `id` → `""`.
/// `uploader`/`id` ne comptent que s'ils sont non vides après `trim`.
pub fn pick_name(title: &str, uploader: Option<&str>, id: Option<&str>) -> String {
    let t = title.trim();
    if !t.is_empty() {
        return t.to_string();
    }
    let up = uploader.map(str::trim).filter(|s| !s.is_empty());
    let id = id.map(str::trim).filter(|s| !s.is_empty());
    match (up, id) {
        (Some(up), Some(id)) => format!("{up} - {id}"),
        (Some(up), None) => up.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => String::new(),
    }
}
```

- [ ] **Step 5: Câbler `parse_formats` pour renvoyer le nom choisi**

Dans `parse_formats`, remplacer :
```rust
    let title = info.title.unwrap_or_default();
```
par :
```rust
    let title = info.title.unwrap_or_default();
    let name = pick_name(&title, info.uploader.as_deref(), info.id.as_deref());
```
et remplacer le `Ok((title, formats))` final par :
```rust
    Ok((name, formats))
```
(Le 1ᵉʳ élément du tuple est désormais le « nom d'affichage » et non le titre brut ; la signature `Result<(String, Vec<YtdlpFormat>), CoreError>` est inchangée, donc `commands.rs::probe_ytdlp_inner` (qui fait `sanitize_title(Some(&name))`) n'a pas à changer.)

- [ ] **Step 6: Lancer pour vérifier le succès**

Run: `cd src-tauri/crates/download-core && cargo test pick_name` puis `cargo test parse_formats`
Expected: PASS (les 5 nouveaux tests + les tests `parse_formats` existants — ceux-ci ont un `title` non vide, donc `name == title`, inchangé).

- [ ] **Step 7: Non-régression complète + 0 warning**

Run: `cd src-tauri/crates/download-core && cargo test` puis `cd src-tauri && cargo test`
Expected: tout vert (download-core + tauri lib), **0 warning rustc**.

- [ ] **Step 8: Commit**

```bash
git checkout phase-4c3-ytdlp-name-fallback
git add src-tauri/crates/download-core/src/ytdlp.rs
git commit -m "feat(core): ytdlp::pick_name (repli uploader - id quand le titre manque)"
```

---

## Task 2: Vérification finale

**Files:** aucun (vérification globale).

- [ ] **Step 1: Suites complètes + build**

Run: `cd src-tauri/crates/download-core && cargo test` ; `cd src-tauri && cargo test` ; `cd c:/Users/steph/Documents/H/idm-clone && npm test && npm run build`
Expected: tout vert ; **0 warning rustc** ; Vitest inchangé (front non modifié) ; build OK.

- [ ] **Step 2: Smoke manuel (note pour le réviseur)**

Avec le binaire yt-dlp en place : capturer une vidéo Facebook **sans titre** → la boîte de variantes doit pré-remplir « Ma Page - <id> » (éditable) au lieu de « video » ; une vidéo YouTube (avec titre) → nom inchangé.

---

## Notes d'implémentation
- **DRY/YAGNI :** une seule fonction pure `pick_name` ; pas de dédup, pas de détection de titres génériques.
- **Vérif existant :** les tests `parse_formats_*` actuels utilisent un JSON avec `title` non vide → `name == title`, ils restent verts. Vérifier qu'aucun n'asserte l'absence d'`id`/`uploader`.
- **Garde git :** `git checkout phase-4c3-ytdlp-name-fallback` avant chaque commit de subagent.
