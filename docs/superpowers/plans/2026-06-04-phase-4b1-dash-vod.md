# Phase 4b-1 — Téléchargement vidéo DASH (VOD) — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Détecter les manifests DASH `.mpd` dans le navigateur, choisir une qualité dans FlashGet, télécharger les segments avec le moteur Rust existant (4 modes d'adressage), muxer l'audio séparé et déchiffrer le CENC ClearKey via ffmpeg, pour produire un `.mp4`.

**Architecture :** Nouveau module `download-core::dash` (parser MPD via `roxmltree`, parser `sidx`, constructeur de segments, ClearKey, sélection, moteur `download_dash`) **pur et Tauri-agnostique**, parallèle au module `hls` de 4a. Les helpers communs de téléchargement de segments sont factorisés de `hls/engine.rs` vers un module partagé `download-core::media_io`. Le trait `Remuxer` gagne une clé de déchiffrement optionnelle (`-decryption_key` ffmpeg). Le flux d'intégration (détection webRequest, `pending_media`, dialogue qualité, `add_media_download`) est généralisé pour servir HLS **et** DASH.

**Tech Stack :** Rust (tokio, reqwest, roxmltree, base64, aes/cbc de 4a), Tauri 2 (`tauri-plugin-shell` ffmpeg sidecar), React/TypeScript, extension Chrome MV3, tests `cargo test` + Vitest.

**Référence spec :** [docs/superpowers/specs/2026-06-04-phase-4b1-dash-vod-design.md](../specs/2026-06-04-phase-4b1-dash-vod-design.md)
**Branche :** `phase-4b1-dash-vod` (empilée sur `phase-4a-hls-vod` ; le code 4a — modules `hls/`, `Remuxer`, `pending_media`, dialogue, `DownloadKind::Hls` — est présent).

**Conventions de test :**
- Cœur Rust : `cargo test --manifest-path src-tauri/Cargo.toml -p download-core <filtre>`.
- Workspace : `cargo test --workspace --manifest-path src-tauri/Cargo.toml`.
- Build : `cargo build --manifest-path src-tauri/Cargo.toml`.
- JS/TS : `npx vitest run <chemin>` ; build front : `npm run build`.
- Un commit par étape verte, message conventionnel.

---

## File Structure

**Nouveaux fichiers (cœur `download-core`) :**
- `src/dash/mod.rs` — déclare les sous-modules.
- `src/dash/mpd.rs` — parser MPD (roxmltree) → types `Mpd`/`Period`/`AdaptationSet`/`Representation`/`Addressing`/`ContentProtection`.
- `src/dash/sidx.rs` — parse la boîte `sidx` → byte-ranges des sous-segments.
- `src/dash/segments.rs` — construit la liste de segments (`SegmentPlan`) depuis l'adressage.
- `src/dash/clearkey.rs` — requête de licence ClearKey + parse JWK.
- `src/dash/select.rs` — sélection vidéo/audio, taille estimée, durée ISO-8601, liste de variantes.
- `src/dash/engine.rs` — `download_dash`.
- `src/media_io.rs` — helpers partagés extraits de `hls/engine.rs` (work_dir, fetch_to_vec, write_segment, concat_track, emit_segments, finish_cancelled).

**Fichiers 4a modifiés :**
- `src/lib.rs` (download-core) — `pub mod dash; pub mod media_io;`.
- `src/http.rs` — `ByteFetcher::post` (méthode par défaut + override `ReqwestFetcher`).
- `src/hls/engine.rs` — consomme `media_io` (helpers déplacés).
- `src/hls/remux.rs` — `RemuxInputs.decryption_key`.
- `src/model.rs` — `DownloadKind::Dash`, `Download.media_selection`.
- `src/storage.rs` — migration + lecture/écriture `media_selection`.
- `src-tauri/src/remux.rs` — `SidecarRemuxer` passe `-decryption_key`.
- `src-tauri/src/state.rs` — `PendingMedia` → `MediaProbe` tagué.
- `src-tauri/src/commands.rs` — `probe_dash`, `add_media_download` (remplace `add_hls_download`), branche `launch` Dash.
- `src-tauri/src/bridge/server.rs` — route `kind=="dash"`.
- `src-tauri/src/lib.rs` — handler `add_media_download`.
- `extension/media-rules.js` — `isDashUrl`/`isDashContentType`.
- `extension/background.js` — `sendMedia` choisit le `kind`.
- `src/api.ts`, `src/App.tsx`, `src/components/DownloadRow.tsx` — `addMediaDownload`, badge DASH.

---

## Task 1 : Dépendances + squelette du module `dash`

**Files:**
- Modify: `src-tauri/crates/download-core/Cargo.toml`
- Modify: `src-tauri/crates/download-core/src/lib.rs`
- Create: `src-tauri/crates/download-core/src/dash/mod.rs`

- [ ] **Step 1 : Ajouter les dépendances**

Dans `src-tauri/crates/download-core/Cargo.toml`, sous `[dependencies]` (après les lignes `aes`/`cbc` de 4a) :

```toml
roxmltree = "0.20"
base64 = "0.22"
```

- [ ] **Step 2 : Déclarer les modules**

Dans `src-tauri/crates/download-core/src/lib.rs`, ajouter après `pub mod hls;` :

```rust
pub mod dash;
pub mod media_io;
```

(`media_io` sera créé en Task 7 ; pour que ça compile maintenant, créer un fichier vide à cette étape OU déplacer la déclaration `pub mod media_io;` en Task 7. **Décision : ajouter `pub mod dash;` ici et `pub mod media_io;` en Task 7.** Donc à cette étape n'ajouter QUE `pub mod dash;`.)

Ligne à ajouter maintenant :

```rust
pub mod dash;
```

- [ ] **Step 3 : Créer le squelette `dash/mod.rs`**

Créer `src-tauri/crates/download-core/src/dash/mod.rs` :

```rust
//! Téléchargement de flux DASH VOD : parsing MPD, index sidx, construction de
//! segments, ClearKey, sélection de variante, moteur (segments → remux ffmpeg).
pub mod mpd;
```

- [ ] **Step 4 : Vérifier la compilation (module vide mais valide)**

Le `dash/mod.rs` référence `pub mod mpd;` qui n'existe pas encore — créer un `mpd.rs` minimal vide d'abord OU créer `mpd.rs` directement en Task 2. **Pour compiler à cette étape**, créer un `src/dash/mpd.rs` vide (un commentaire). Puis :

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compile (roxmltree/base64 téléchargés).

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/Cargo.toml src-tauri/crates/download-core/src/lib.rs src-tauri/crates/download-core/src/dash/
git commit -m "chore(core): dash module skeleton + roxmltree/base64 deps"
```

---

## Task 2 : Parser MPD (`dash/mpd.rs`)

**Files:**
- Modify: `src-tauri/crates/download-core/src/dash/mpd.rs`

> Le parser réutilise `crate::hls::playlist::resolve_uri` (déjà `pub(crate)` en 4a) pour résoudre les BaseURL/URLs relatives.

- [ ] **Step 1 : Écrire les types + tests**

Remplacer le contenu de `src/dash/mpd.rs` par les types et un premier jeu de tests (l'implémentation suit) :

```rust
use crate::hls::playlist::resolve_uri;
use crate::CoreError;
use roxmltree::Document;

#[derive(Debug, Clone, PartialEq)]
pub enum ContentType { Video, Audio, Other }

#[derive(Debug, Clone, PartialEq)]
pub enum ProtScheme { Cenc, ClearKey, Other }

#[derive(Debug, Clone, PartialEq)]
pub struct ContentProtection {
    pub default_kid: Option<String>,
    pub clearkey_laurl: Option<String>,
    pub scheme: ProtScheme,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineS { pub t: Option<u64>, pub d: u64, pub r: i64 }

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentTemplate {
    pub init: Option<String>,
    pub media: String,
    pub timescale: u64,
    pub duration: Option<u64>,
    pub start_number: u64,
    pub timeline: Option<Vec<TimelineS>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentListEntry { pub media: String, pub range: Option<(u64, u64)> }

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentList {
    pub init: Option<String>,
    pub init_range: Option<(u64, u64)>,
    pub segments: Vec<SegmentListEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentBase {
    /// URL du fichier unique de la Representation (BaseURL résolue).
    pub url: String,
    pub init_range: Option<(u64, u64)>,
    pub index_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Addressing { Template(SegmentTemplate), List(SegmentList), Base(SegmentBase) }

#[derive(Debug, Clone, PartialEq)]
pub struct Representation {
    pub id: String,
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mime: Option<String>,
    /// BaseURL effective de la Representation (pour résoudre les templates/listes relatifs).
    pub base: String,
    pub addressing: Addressing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdaptationSet {
    pub content_type: ContentType,
    pub lang: Option<String>,
    pub representations: Vec<Representation>,
    pub content_protection: Option<ContentProtection>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Period { pub adaptation_sets: Vec<AdaptationSet> }

#[derive(Debug, Clone, PartialEq)]
pub struct Mpd {
    pub dynamic: bool,
    /// Durée totale (s) depuis MPD@mediaPresentationDuration (0 si absente).
    pub duration_secs: f64,
    pub periods: Vec<Period>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE_NUMBER: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT10S">
  <Period>
    <AdaptationSet contentType="video" mimeType="video/mp4">
      <SegmentTemplate initialization="init-$RepresentationID$.mp4" media="seg-$RepresentationID$-$Number$.m4s" timescale="1000" duration="2000" startNumber="1"/>
      <Representation id="v0" bandwidth="800000" width="640" height="360"/>
      <Representation id="v1" bandwidth="2400000" width="1280" height="720"/>
    </AdaptationSet>
    <AdaptationSet contentType="audio" lang="en" mimeType="audio/mp4">
      <SegmentTemplate initialization="init-$RepresentationID$.mp4" media="seg-$RepresentationID$-$Number$.m4s" timescale="1000" duration="2000" startNumber="1"/>
      <Representation id="a0" bandwidth="128000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    #[test]
    fn parses_static_template_mpd() {
        let m = parse_mpd(TEMPLATE_NUMBER, "https://h/p/manifest.mpd").unwrap();
        assert!(!m.dynamic);
        assert_eq!(m.duration_secs, 10.0);
        assert_eq!(m.periods.len(), 1);
        let video = &m.periods[0].adaptation_sets[0];
        assert_eq!(video.content_type, ContentType::Video);
        assert_eq!(video.representations.len(), 2);
        assert_eq!(video.representations[1].bandwidth, 2_400_000);
        assert_eq!(video.representations[1].width, Some(1280));
        let audio = &m.periods[0].adaptation_sets[1];
        assert_eq!(audio.content_type, ContentType::Audio);
        assert_eq!(audio.lang.as_deref(), Some("en"));
        match &video.representations[0].addressing {
            Addressing::Template(t) => {
                assert_eq!(t.media, "seg-$RepresentationID$-$Number$.m4s");
                assert_eq!(t.duration, Some(2000));
                assert_eq!(t.timescale, 1000);
            }
            other => panic!("attendu Template, obtenu {other:?}"),
        }
    }

    #[test]
    fn detects_dynamic_and_clearkey() {
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" xmlns:cenc="urn:mpeg:cenc:2013" type="dynamic">
  <Period><AdaptationSet contentType="video">
    <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" cenc:default_KID="abc"/>
    <ContentProtection schemeIdUri="urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e">
      <clearkey:Laurl xmlns:clearkey="http://dashif.org/guidelines/clearKey">https://lic/ck</clearkey:Laurl>
    </ContentProtection>
    <Representation id="v0" bandwidth="1"><BaseURL>v.mp4</BaseURL><SegmentBase indexRange="0-99"/></Representation>
  </AdaptationSet></Period>
</MPD>"#;
        let m = parse_mpd(xml, "https://h/p/m.mpd").unwrap();
        assert!(m.dynamic);
        let cp = m.periods[0].adaptation_sets[0].content_protection.as_ref().unwrap();
        assert_eq!(cp.default_kid.as_deref(), Some("abc"));
        assert_eq!(cp.clearkey_laurl.as_deref(), Some("https://lic/ck"));
        match &m.periods[0].adaptation_sets[0].representations[0].addressing {
            Addressing::Base(b) => {
                assert_eq!(b.url, "https://h/p/v.mp4");
                assert_eq!(b.index_range, Some((0, 99)));
            }
            other => panic!("attendu Base, obtenu {other:?}"),
        }
    }
}
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::mpd`
Expected: FAIL — `parse_mpd` introuvable.

- [ ] **Step 3 : Implémenter `parse_mpd` + helpers**

Ajouter dans `mpd.rs` (avant `mod tests`) :

```rust
/// Parse un manifest MPD. Les URLs sont résolues contre `base_url`.
pub fn parse_mpd(xml: &str, base_url: &str) -> Result<Mpd, CoreError> {
    let doc = Document::parse(xml).map_err(|e| CoreError::Media(format!("MPD invalide: {e}")))?;
    let root = doc.root_element();
    let dynamic = attr(&root, "type") == Some("dynamic".to_string());
    let duration_secs = attr(&root, "mediaPresentationDuration")
        .and_then(|s| parse_iso8601_duration(&s))
        .unwrap_or(0.0);

    // BaseURL au niveau MPD.
    let mpd_base = child_base_url(&root, base_url);

    let mut periods = Vec::new();
    for pnode in root.children().filter(|n| n.has_tag_name_local("Period")) {
        let period_base = child_base_url(&pnode, &mpd_base);
        let mut adaptation_sets = Vec::new();
        for asnode in pnode.children().filter(|n| n.has_tag_name_local("AdaptationSet")) {
            let as_base = child_base_url(&asnode, &period_base);
            let content_type = parse_content_type(&asnode);
            let lang = attr(&asnode, "lang");
            let content_protection = parse_content_protection(&asnode);
            // SegmentTemplate hérité au niveau AdaptationSet.
            let as_template = asnode
                .children()
                .find(|n| n.has_tag_name_local("SegmentTemplate"))
                .map(|n| parse_segment_template(&n, &as_base));

            let mut representations = Vec::new();
            for rnode in asnode.children().filter(|n| n.has_tag_name_local("Representation")) {
                let rep_base = child_base_url_with_default(&rnode, &as_base);
                let id = attr(&rnode, "id").unwrap_or_default();
                let bandwidth = attr(&rnode, "bandwidth").and_then(|s| s.parse().ok()).unwrap_or(0);
                let width = attr(&rnode, "width").and_then(|s| s.parse().ok());
                let height = attr(&rnode, "height").and_then(|s| s.parse().ok());
                let mime = attr(&rnode, "mimeType");
                let addressing = parse_addressing(&rnode, &rep_base, &as_base, as_template.as_ref())?;
                representations.push(Representation { id, bandwidth, width, height, mime, base: rep_base, addressing });
            }
            adaptation_sets.push(AdaptationSet { content_type, lang, representations, content_protection });
        }
        periods.push(Period { adaptation_sets });
    }
    Ok(Mpd { dynamic, duration_secs, periods })
}

/// Adressage d'une Representation : SegmentBase/SegmentList/SegmentTemplate au niveau
/// Representation, sinon le SegmentTemplate hérité de l'AdaptationSet.
fn parse_addressing(
    rnode: &roxmltree::Node,
    rep_base: &str,
    _as_base: &str,
    as_template: Option<&SegmentTemplate>,
) -> Result<Addressing, CoreError> {
    if let Some(b) = rnode.children().find(|n| n.has_tag_name_local("SegmentBase")) {
        return Ok(Addressing::Base(SegmentBase {
            url: rep_base.to_string(),
            init_range: b
                .children()
                .find(|n| n.has_tag_name_local("Initialization"))
                .and_then(|i| attr(&i, "range").as_deref().and_then(parse_range)),
            index_range: attr(&b, "indexRange").as_deref().and_then(parse_range),
        }));
    }
    if let Some(l) = rnode.children().find(|n| n.has_tag_name_local("SegmentList")) {
        let init = l
            .children()
            .find(|n| n.has_tag_name_local("Initialization"))
            .and_then(|i| attr(&i, "sourceURL"));
        let segments = l
            .children()
            .filter(|n| n.has_tag_name_local("SegmentURL"))
            .map(|s| SegmentListEntry {
                media: attr(&s, "media").unwrap_or_default(),
                range: attr(&s, "mediaRange").as_deref().and_then(parse_range),
            })
            .collect();
        return Ok(Addressing::List(SegmentList { init, init_range: None, segments }));
    }
    if let Some(t) = rnode.children().find(|n| n.has_tag_name_local("SegmentTemplate")) {
        return Ok(Addressing::Template(parse_segment_template(&t, rep_base)));
    }
    if let Some(t) = as_template {
        return Ok(Addressing::Template(t.clone()));
    }
    Err(CoreError::Media("Representation sans adressage de segments".into()))
}

fn parse_segment_template(n: &roxmltree::Node, _base: &str) -> SegmentTemplate {
    let timeline = n.children().find(|c| c.has_tag_name_local("SegmentTimeline")).map(|tl| {
        tl.children()
            .filter(|s| s.has_tag_name_local("S"))
            .map(|s| TimelineS {
                t: attr(&s, "t").and_then(|v| v.parse().ok()),
                d: attr(&s, "d").and_then(|v| v.parse().ok()).unwrap_or(0),
                r: attr(&s, "r").and_then(|v| v.parse().ok()).unwrap_or(0),
            })
            .collect()
    });
    SegmentTemplate {
        init: attr(n, "initialization"),
        media: attr(n, "media").unwrap_or_default(),
        timescale: attr(n, "timescale").and_then(|s| s.parse().ok()).unwrap_or(1),
        duration: attr(n, "duration").and_then(|s| s.parse().ok()),
        start_number: attr(n, "startNumber").and_then(|s| s.parse().ok()).unwrap_or(1),
        timeline,
    }
}

fn parse_content_type(asnode: &roxmltree::Node) -> ContentType {
    if let Some(ct) = attr(asnode, "contentType") {
        return match ct.as_str() {
            "video" => ContentType::Video,
            "audio" => ContentType::Audio,
            _ => ContentType::Other,
        };
    }
    match attr(asnode, "mimeType") {
        Some(m) if m.starts_with("video/") => ContentType::Video,
        Some(m) if m.starts_with("audio/") => ContentType::Audio,
        _ => ContentType::Other,
    }
}

fn parse_content_protection(asnode: &roxmltree::Node) -> Option<ContentProtection> {
    let cps: Vec<roxmltree::Node> =
        asnode.children().filter(|n| n.has_tag_name_local("ContentProtection")).collect();
    if cps.is_empty() {
        return None;
    }
    let mut default_kid = None;
    let mut clearkey_laurl = None;
    let mut scheme = ProtScheme::Other;
    for cp in &cps {
        // cenc:default_KID (attribut namespacé : on matche par nom local).
        if let Some(kid) = cp.attributes().find(|a| a.name() == "default_KID").map(|a| a.value().to_string()) {
            default_kid = Some(kid);
        }
        let s = attr(cp, "schemeIdUri").unwrap_or_default();
        if s == "urn:mpeg:dash:mp4protection:2011" {
            scheme = ProtScheme::Cenc;
        }
        if s.eq_ignore_ascii_case("urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e") {
            scheme = ProtScheme::ClearKey;
            if let Some(la) = cp.children().find(|n| n.has_tag_name_local("Laurl")) {
                clearkey_laurl = la.text().map(|t| t.trim().to_string());
            }
        }
    }
    Some(ContentProtection { default_kid, clearkey_laurl, scheme })
}

/// BaseURL enfant direct résolu contre `parent_base` ; si absent, renvoie `parent_base`.
fn child_base_url(node: &roxmltree::Node, parent_base: &str) -> String {
    match node.children().find(|n| n.has_tag_name_local("BaseURL")).and_then(|b| b.text()) {
        Some(b) => resolve_uri(parent_base, b.trim()),
        None => parent_base.to_string(),
    }
}
fn child_base_url_with_default(node: &roxmltree::Node, parent_base: &str) -> String {
    child_base_url(node, parent_base)
}

fn attr(n: &roxmltree::Node, name: &str) -> Option<String> {
    n.attribute(name).map(|s| s.to_string())
}

/// "start-end" (inclusif) → (u64, u64).
fn parse_range(s: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once('-')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Parse une durée ISO-8601 limitée (`PT#H#M#S`, secondes fractionnaires) en secondes.
fn parse_iso8601_duration(s: &str) -> Option<f64> {
    let s = s.strip_prefix('P')?;
    let t = s.split_once('T').map(|(_, t)| t).unwrap_or(s);
    let mut secs = 0.0;
    let mut num = String::new();
    for c in t.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            match c {
                'H' => secs += v * 3600.0,
                'M' => secs += v * 60.0,
                'S' => secs += v,
                _ => {}
            }
        }
    }
    Some(secs)
}

/// Extension d'aide : test du nom local d'une balise (ignore le namespace).
trait LocalName {
    fn has_tag_name_local(&self, name: &str) -> bool;
}
impl LocalName for roxmltree::Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name() == name
    }
}
```

- [ ] **Step 4 : Vérifier la réussite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::mpd`
Expected: 2 tests PASS.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/dash/mpd.rs
git commit -m "feat(core): parse DASH MPD (periods, adaptation sets, addressing, ClearKey)"
```

---

## Task 3 : Parser `sidx` (`dash/sidx.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/dash/sidx.rs`
- Modify: `src-tauri/crates/download-core/src/dash/mod.rs` (`pub mod sidx;`)

- [ ] **Step 1 : Déclarer le module**

Dans `dash/mod.rs`, ajouter :

```rust
pub mod sidx;
```

- [ ] **Step 2 : Écrire le test (sidx construit à la main)**

Créer `src/dash/sidx.rs` :

```rust
use crate::CoreError;

/// Parse une boîte `sidx` (Segment Index, ISO BMFF) et renvoie les byte-ranges
/// absolus (start, end inclusif) de chaque sous-segment. `anchor` = position
/// absolue de l'octet juste APRÈS la boîte sidx (point d'ancrage de first_offset).
pub fn parse_sidx(b: &[u8], anchor: u64) -> Result<Vec<(u64, u64)>, CoreError> {
    // Cherche la boîte 'sidx' (size:4, type:4).
    if b.len() < 12 {
        return Err(CoreError::Media("sidx tronqué".into()));
    }
    // On suppose que `b` commence à la boîte sidx (size@0, type@4).
    if &b[4..8] != b"sidx" {
        return Err(CoreError::Media("boîte sidx absente".into()));
    }
    let version = b[8];
    let mut p = 12usize; // après version(1)+flags(3)
    p += 4; // reference_ID
    p += 4; // timescale
    if version == 0 {
        p += 4; // earliest_presentation_time
        let first_offset = u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as u64;
        p += 4;
        read_refs(b, p, anchor + first_offset)
    } else {
        p += 8; // earliest_presentation_time (64)
        let first_offset = u64::from_be_bytes(b[p..p + 8].try_into().unwrap());
        p += 8;
        read_refs(b, p, anchor + first_offset)
    }
}

fn read_refs(b: &[u8], mut p: usize, mut start: u64) -> Result<Vec<(u64, u64)>, CoreError> {
    p += 2; // reserved
    if p + 2 > b.len() {
        return Err(CoreError::Media("sidx tronqué (count)".into()));
    }
    let count = u16::from_be_bytes(b[p..p + 2].try_into().unwrap()) as usize;
    p += 2;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if p + 12 > b.len() {
            return Err(CoreError::Media("sidx tronqué (refs)".into()));
        }
        let w = u32::from_be_bytes(b[p..p + 4].try_into().unwrap());
        let referenced_size = (w & 0x7FFF_FFFF) as u64; // bit de poids fort = reference_type
        p += 12; // referenced_size(4) + subsegment_duration(4) + SAP(4)
        out.push((start, start + referenced_size - 1));
        start += referenced_size;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_references() {
        // sidx version 0, 2 références (tailles 100 et 200), first_offset=0.
        let mut b = Vec::new();
        b.extend_from_slice(&56u32.to_be_bytes()); // size
        b.extend_from_slice(b"sidx"); // type
        b.push(0); // version
        b.extend_from_slice(&[0, 0, 0]); // flags
        b.extend_from_slice(&1u32.to_be_bytes()); // reference_ID
        b.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        b.extend_from_slice(&0u32.to_be_bytes()); // earliest_presentation_time
        b.extend_from_slice(&0u32.to_be_bytes()); // first_offset
        b.extend_from_slice(&0u16.to_be_bytes()); // reserved
        b.extend_from_slice(&2u16.to_be_bytes()); // reference_count
        // ref 1
        b.extend_from_slice(&100u32.to_be_bytes());
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&0x9000_0000u32.to_be_bytes());
        // ref 2
        b.extend_from_slice(&200u32.to_be_bytes());
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&0x9000_0000u32.to_be_bytes());
        assert_eq!(b.len(), 56);

        let ranges = parse_sidx(&b, 56).unwrap();
        assert_eq!(ranges, vec![(56, 155), (156, 355)]);
    }

    #[test]
    fn rejects_non_sidx() {
        let b = vec![0u8; 12];
        assert!(parse_sidx(&b, 0).is_err());
    }
}
```

- [ ] **Step 3 : Vérifier l'échec puis la réussite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::sidx`
Expected: 2 tests PASS.

- [ ] **Step 4 : Commit**

```bash
git add src-tauri/crates/download-core/src/dash/
git commit -m "feat(core): parse DASH sidx box into subsegment byte ranges"
```

---

## Task 4 : Construction des segments (`dash/segments.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/dash/segments.rs`
- Modify: `src-tauri/crates/download-core/src/dash/mod.rs` (`pub mod segments;`)

- [ ] **Step 1 : Déclarer le module**

Dans `dash/mod.rs` : `pub mod segments;`

- [ ] **Step 2 : Écrire les types + tests**

Créer `src/dash/segments.rs` :

```rust
use crate::dash::mpd::{Addressing, Representation, SegmentBase, SegmentList, SegmentTemplate};
use crate::hls::playlist::resolve_uri;

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRef {
    pub url: String,
    pub byte_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SegmentPlan {
    pub init: Option<SegmentRef>,
    pub media: Vec<SegmentRef>,
}

/// Construit le plan de segments d'une Representation. Pour SegmentBase, `media`
/// est laissé vide (rempli à l'exécution après lecture du sidx via `base_media`).
pub fn build_segments(repr: &Representation, base: &str, total_duration_secs: f64) -> SegmentPlan {
    match &repr.addressing {
        Addressing::Template(t) => build_template(repr, t, base, total_duration_secs),
        Addressing::List(l) => build_list(l, base),
        Addressing::Base(b) => SegmentPlan {
            init: Some(SegmentRef { url: b.url.clone(), byte_range: b.init_range }),
            media: Vec::new(),
        },
    }
}

/// Médias d'un SegmentBase à partir des ranges issus du sidx.
pub fn base_media(b: &SegmentBase, sidx_ranges: &[(u64, u64)]) -> Vec<SegmentRef> {
    sidx_ranges
        .iter()
        .map(|r| SegmentRef { url: b.url.clone(), byte_range: Some(*r) })
        .collect()
}

fn build_template(repr: &Representation, t: &SegmentTemplate, base: &str, total: f64) -> SegmentPlan {
    let init = t.init.as_ref().map(|tpl| SegmentRef {
        url: resolve_uri(base, &expand(tpl, &repr.id, repr.bandwidth, 0, 0)),
        byte_range: None,
    });
    let mut media = Vec::new();
    if let Some(timeline) = &t.timeline {
        let mut number = t.start_number;
        let mut time = 0u64;
        for s in timeline {
            if let Some(tt) = s.t {
                time = tt;
            }
            let repeat = if s.r < 0 { 0 } else { s.r as u64 }; // r=-1 (illimité) non géré en VOD
            for _ in 0..=repeat {
                media.push(SegmentRef {
                    url: resolve_uri(base, &expand(&t.media, &repr.id, repr.bandwidth, number, time)),
                    byte_range: None,
                });
                number += 1;
                time += s.d;
            }
        }
    } else if let Some(d) = t.duration {
        let count = if d == 0 { 0 } else {
            ((total * t.timescale as f64) / d as f64).ceil() as u64
        };
        for i in 0..count {
            let number = t.start_number + i;
            let time = i * d;
            media.push(SegmentRef {
                url: resolve_uri(base, &expand(&t.media, &repr.id, repr.bandwidth, number, time)),
                byte_range: None,
            });
        }
    }
    SegmentPlan { init, media }
}

fn build_list(l: &SegmentList, base: &str) -> SegmentPlan {
    let init = l.init.as_ref().map(|u| SegmentRef { url: resolve_uri(base, u), byte_range: l.init_range });
    let media = l
        .segments
        .iter()
        .map(|s| SegmentRef { url: resolve_uri(base, &s.media), byte_range: s.range })
        .collect();
    SegmentPlan { init, media }
}

/// Substitution des identifiants de template DASH.
fn expand(template: &str, rep_id: &str, bandwidth: u64, number: u64, time: u64) -> String {
    let mut out = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Trouver le '$' fermant.
        let end = match template[i + 1..].find('$') {
            Some(e) => i + 1 + e,
            None => { out.push('$'); break; }
        };
        let tok = &template[i + 1..end];
        if tok.is_empty() {
            out.push('$'); // $$ => $
        } else {
            let (name, fmt) = match tok.split_once('%') {
                Some((n, f)) => (n, Some(f)),
                None => (tok, None),
            };
            let val: u64 = match name {
                "Number" => number,
                "Time" => time,
                "Bandwidth" => bandwidth,
                "RepresentationID" => { out.push_str(rep_id); i = end + 1; continue; }
                _ => { out.push_str(&template[i..=end]); i = end + 1; continue; }
            };
            match fmt {
                Some(f) => out.push_str(&format_padded(val, f)),
                None => out.push_str(&val.to_string()),
            }
        }
        i = end + 1;
    }
    out
}

/// Format minimal `0Nd` (zéro-padding). Ex. "05d" => largeur 5.
fn format_padded(val: u64, fmt: &str) -> String {
    let digits: String = fmt.chars().filter(|c| c.is_ascii_digit()).collect();
    let width: usize = digits.trim_start_matches('0').parse().unwrap_or_else(|_| digits.parse().unwrap_or(0));
    format!("{val:0width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::mpd::{Addressing, Representation, SegmentTemplate, TimelineS};

    fn repr(addr: Addressing) -> Representation {
        Representation { id: "v0".into(), bandwidth: 800000, width: None, height: None, mime: None, base: String::new(), addressing: addr }
    }

    #[test]
    fn expands_number_template_by_duration() {
        let t = SegmentTemplate {
            init: Some("init-$RepresentationID$.mp4".into()),
            media: "seg-$Number%05d$.m4s".into(),
            timescale: 1000,
            duration: Some(2000),
            start_number: 1,
            timeline: None,
        };
        let plan = build_segments(&repr(Addressing::Template(t)), "https://h/p/", 6.0);
        assert_eq!(plan.init.unwrap().url, "https://h/p/init-v0.mp4");
        // 6 s, segments de 2 s => 3 segments.
        assert_eq!(plan.media.len(), 3);
        assert_eq!(plan.media[0].url, "https://h/p/seg-00001.m4s");
        assert_eq!(plan.media[2].url, "https://h/p/seg-00003.m4s");
    }

    #[test]
    fn expands_timeline() {
        let t = SegmentTemplate {
            init: None,
            media: "$Time$.m4s".into(),
            timescale: 1000,
            duration: None,
            start_number: 1,
            timeline: Some(vec![TimelineS { t: Some(0), d: 1000, r: 2 }]),
        };
        let plan = build_segments(&repr(Addressing::Template(t)), "https://h/p/", 0.0);
        // r=2 => 3 segments aux temps 0,1000,2000.
        assert_eq!(plan.media.iter().map(|s| s.url.clone()).collect::<Vec<_>>(),
            vec!["https://h/p/0.m4s", "https://h/p/1000.m4s", "https://h/p/2000.m4s"]);
    }

    #[test]
    fn base_media_from_sidx() {
        let b = crate::dash::mpd::SegmentBase { url: "https://h/p/v.mp4".into(), init_range: Some((0, 99)), index_range: Some((100, 199)) };
        let refs = base_media(&b, &[(200, 299), (300, 399)]);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], SegmentRef { url: "https://h/p/v.mp4".into(), byte_range: Some((200, 299)) });
    }
}
```

- [ ] **Step 3 : Vérifier l'échec puis la réussite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::segments`
Expected: 3 tests PASS.

- [ ] **Step 4 : Commit**

```bash
git add src-tauri/crates/download-core/src/dash/
git commit -m "feat(core): build DASH segment lists (template/timeline/list/base)"
```

---

## Task 5 : Sélection & estimation (`dash/select.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/dash/select.rs`
- Modify: `src-tauri/crates/download-core/src/dash/mod.rs` (`pub mod select;`)

- [ ] **Step 1 : Déclarer le module**

Dans `dash/mod.rs` : `pub mod select;`

- [ ] **Step 2 : Écrire les fonctions + tests**

Créer `src/dash/select.rs` :

```rust
use crate::dash::mpd::{ContentType, Mpd};

/// Coordonnées d'une Representation dans le MPD.
#[derive(Debug, Clone, PartialEq)]
pub struct ReprCoord { pub period: usize, pub adaptation: usize, pub representation: usize }

/// Variante affichable côté UI (même forme que `MediaVariant` côté Tauri).
#[derive(Debug, Clone, PartialEq)]
pub struct DashVariant {
    pub index: u32,
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

/// Représentations vidéo de la 1ère Period, dans l'ordre, avec leurs coordonnées.
pub fn video_variants(mpd: &Mpd) -> Vec<(ReprCoord, DashVariant)> {
    let mut out = Vec::new();
    if let Some(period) = mpd.periods.first() {
        for (ai, aset) in period.adaptation_sets.iter().enumerate() {
            if aset.content_type != ContentType::Video {
                continue;
            }
            for (ri, r) in aset.representations.iter().enumerate() {
                let coord = ReprCoord { period: 0, adaptation: ai, representation: ri };
                let est = if r.bandwidth > 0 && mpd.duration_secs > 0.0 {
                    Some(estimated_size(r.bandwidth, mpd.duration_secs))
                } else {
                    None
                };
                out.push((coord, DashVariant {
                    index: out.len() as u32,
                    resolution: match (r.width, r.height) {
                        (Some(w), Some(h)) => Some(format!("{w}x{h}")),
                        _ => None,
                    },
                    bandwidth: r.bandwidth,
                    est_size: est,
                }));
            }
        }
    }
    out
}

/// Index de la meilleure variante vidéo (plus haut bitrate). None si aucune.
pub fn best_video_index(mpd: &Mpd) -> Option<u32> {
    video_variants(mpd).into_iter().max_by_key(|(_, v)| v.bandwidth).map(|(_, v)| v.index)
}

/// Coordonnées de l'AdaptationSet audio (langue demandée sinon première) + sa
/// meilleure Representation. None si pas d'audio.
pub fn pick_audio(mpd: &Mpd, lang: Option<&str>) -> Option<ReprCoord> {
    let period = mpd.periods.first()?;
    let audio: Vec<(usize, &_)> = period
        .adaptation_sets
        .iter()
        .enumerate()
        .filter(|(_, a)| a.content_type == ContentType::Audio && !a.representations.is_empty())
        .collect();
    let chosen = lang
        .and_then(|l| audio.iter().find(|(_, a)| a.lang.as_deref() == Some(l)))
        .or_else(|| audio.first())?;
    let (ai, aset) = (chosen.0, chosen.1);
    // meilleure Representation audio (plus haut bitrate)
    let ri = aset
        .representations
        .iter()
        .enumerate()
        .max_by_key(|(_, r)| r.bandwidth)
        .map(|(i, _)| i)?;
    Some(ReprCoord { period: 0, adaptation: ai, representation: ri })
}

pub fn estimated_size(bandwidth: u64, total_duration_secs: f64) -> u64 {
    ((bandwidth as f64) * total_duration_secs / 8.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::mpd::*;

    fn rep(id: &str, bw: u64, w: Option<u32>, h: Option<u32>) -> Representation {
        Representation {
            id: id.into(), bandwidth: bw, width: w, height: h, mime: None, base: String::new(),
            addressing: Addressing::Template(SegmentTemplate {
                init: None, media: "$Number$".into(), timescale: 1, duration: Some(1), start_number: 1, timeline: None,
            }),
        }
    }
    fn aset(ct: ContentType, lang: Option<&str>, reps: Vec<Representation>) -> AdaptationSet {
        AdaptationSet { content_type: ct, lang: lang.map(String::from), representations: reps, content_protection: None }
    }

    fn mpd() -> Mpd {
        Mpd {
            dynamic: false, duration_secs: 10.0,
            periods: vec![Period { adaptation_sets: vec![
                aset(ContentType::Video, None, vec![rep("v0", 800000, Some(640), Some(360)), rep("v1", 2400000, Some(1280), Some(720))]),
                aset(ContentType::Audio, Some("en"), vec![rep("a0", 128000, None, None)]),
                aset(ContentType::Audio, Some("fr"), vec![rep("a1", 128000, None, None)]),
            ] }],
        }
    }

    #[test]
    fn video_variants_and_best() {
        let m = mpd();
        let vs = video_variants(&m);
        assert_eq!(vs.len(), 2);
        assert_eq!(vs[1].1.resolution.as_deref(), Some("1280x720"));
        assert_eq!(vs[1].1.est_size, Some(estimated_size(2400000, 10.0)));
        assert_eq!(best_video_index(&m), Some(1));
    }

    #[test]
    fn pick_audio_by_lang_then_first() {
        let m = mpd();
        assert_eq!(pick_audio(&m, Some("fr")).unwrap().adaptation, 2);
        assert_eq!(pick_audio(&m, None).unwrap().adaptation, 1);
        assert_eq!(pick_audio(&m, Some("de")).unwrap().adaptation, 1); // langue absente => premier
    }
}
```

- [ ] **Step 3 : Vérifier la réussite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::select`
Expected: 2 tests PASS.

- [ ] **Step 4 : Commit**

```bash
git add src-tauri/crates/download-core/src/dash/
git commit -m "feat(core): DASH variant selection (video/audio) + size estimation"
```

---

## Task 6 : `ByteFetcher::post` + ClearKey (`dash/clearkey.rs`)

**Files:**
- Modify: `src-tauri/crates/download-core/src/http.rs` (méthode `post`)
- Create: `src-tauri/crates/download-core/src/dash/clearkey.rs`
- Modify: `src-tauri/crates/download-core/src/dash/mod.rs` (`pub mod clearkey;`)

- [ ] **Step 1 : Ajouter `post` au trait `ByteFetcher` (impl par défaut + override reqwest)**

Dans `src-tauri/crates/download-core/src/http.rs`, dans le `trait ByteFetcher` (après `fetch_range`), ajouter une méthode avec impl par défaut :

```rust
    /// POST (corps JSON p. ex. licence ClearKey). Impl par défaut = non supporté.
    async fn post(
        &self,
        _url: &str,
        _body: &[u8],
        _content_type: &str,
        _headers: &RequestHeaders,
    ) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Media("POST non supporté par ce fetcher".into()))
    }
```

Et l'override dans `impl ByteFetcher for ReqwestFetcher` (après `fetch_range`) :

```rust
    async fn post(
        &self,
        url: &str,
        body: &[u8],
        content_type: &str,
        headers: &RequestHeaders,
    ) -> Result<Vec<u8>, CoreError> {
        let req = self.client.post(url).header(reqwest::header::CONTENT_TYPE, content_type).body(body.to_vec());
        let resp = apply_headers(req, headers)
            .send()
            .await
            .map_err(|e| CoreError::Network(describe_reqwest_error(&e)))?;
        let bytes = resp.bytes().await.map_err(|e| CoreError::Network(describe_reqwest_error(&e)))?;
        Ok(bytes.to_vec())
    }
```

(`apply_headers` et `describe_reqwest_error` sont des helpers déjà présents dans `http.rs` ; `self.client` est le `reqwest::Client`.)

- [ ] **Step 2 : Déclarer le module clearkey**

Dans `dash/mod.rs` : `pub mod clearkey;`

- [ ] **Step 3 : Écrire les fonctions pures + tests**

Créer `src/dash/clearkey.rs` :

```rust
use crate::CoreError;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use base64::Engine;

/// Construit une requête de licence W3C ClearKey à partir d'un KID hexadécimal.
/// Corps JSON : { "kids": ["<base64url(kid)>"], "type": "temporary" }.
pub fn build_license_request(kid_hex: &str) -> Result<String, CoreError> {
    let kid = hex_to_bytes(kid_hex).ok_or_else(|| CoreError::Media("KID hex invalide".into()))?;
    let kid_b64 = URL_SAFE_NO_PAD.encode(kid);
    Ok(format!("{{\"kids\":[\"{kid_b64}\"],\"type\":\"temporary\"}}"))
}

/// Parse une réponse JWK ClearKey et renvoie la clé en hexadécimal.
/// { "keys": [ { "kty":"oct", "k":"<base64url>", "kid":"..." } ] }.
pub fn parse_jwk_response(json: &str) -> Result<String, CoreError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| CoreError::Media(format!("JWK invalide: {e}")))?;
    let k = v["keys"][0]["k"]
        .as_str()
        .ok_or_else(|| CoreError::Media("JWK sans clé `k`".into()))?;
    // base64url sans padding, mais tolérer le standard.
    let bytes = URL_SAFE_NO_PAD
        .decode(k)
        .or_else(|_| STANDARD_NO_PAD.decode(k))
        .map_err(|e| CoreError::Media(format!("clé base64 invalide: {e}")))?;
    Ok(bytes_to_hex(&bytes))
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.replace('-', "");
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn bytes_to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    #[test]
    fn license_request_encodes_kid() {
        // KID = 0102...10 (16 octets)
        let kid_hex = "000102030405060708090a0b0c0d0e0f";
        let req = build_license_request(kid_hex).unwrap();
        let expected_b64 = URL_SAFE_NO_PAD.encode((0u8..16).collect::<Vec<u8>>());
        assert!(req.contains(&expected_b64));
        assert!(req.contains("\"type\":\"temporary\""));
    }

    #[test]
    fn jwk_response_to_hex() {
        let key: Vec<u8> = (0u8..16).collect();
        let k_b64 = URL_SAFE_NO_PAD.encode(&key);
        let json = format!(r#"{{"keys":[{{"kty":"oct","k":"{k_b64}","kid":"x"}}]}}"#);
        assert_eq!(parse_jwk_response(&json).unwrap(), "000102030405060708090a0b0c0d0e0f");
    }
}
```

- [ ] **Step 4 : Vérifier la réussite + non-régression du cœur**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::clearkey`
Expected: 2 tests PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core`
Expected: tous PASS (le `post` par défaut n'affecte pas les impls HLS existantes).

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/http.rs src-tauri/crates/download-core/src/dash/
git commit -m "feat(core): ByteFetcher::post + ClearKey license request/JWK parsing"
```

---

## Task 7 : Helpers partagés `media_io.rs` (refactor de `hls/engine.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/media_io.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs` (`pub mod media_io;`)
- Modify: `src-tauri/crates/download-core/src/hls/engine.rs` (consommer `media_io`)

> But : extraire les helpers communs (work_dir, fetch_to_vec, write_segment, concat_track, emit_segments, finish_cancelled) pour que **hls** et **dash** les partagent. Comportement identique — les tests HLS doivent rester verts.

- [ ] **Step 1 : Créer `media_io.rs` avec les helpers (copiés de hls/engine.rs)**

Créer `src-tauri/crates/download-core/src/media_io.rs` :

```rust
//! Helpers de téléchargement/assemblage de segments partagés par HLS et DASH.
use crate::engine::DownloadHandle;
use crate::http::ByteFetcher;
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::CoreError;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

/// Répertoire de travail temporaire associé à un fichier de destination.
pub fn work_dir(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".fghls");
    PathBuf::from(s)
}

/// Lit entièrement un flux d'octets (rate-limité, respecte pause/cancel, cumule la progression).
/// `None` ⇒ annulé.
pub async fn fetch_to_vec<F: ByteFetcher>(
    fetcher: &F,
    url: &str,
    range: Option<(u64, u64)>,
    headers: &RequestHeaders,
    limiter: &RateLimiter,
    handle: &DownloadHandle,
) -> Result<Option<Vec<u8>>, CoreError> {
    let mut stream = fetcher.fetch_range(url, range, headers).await?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        if handle.is_cancelled() {
            return Ok(None);
        }
        let chunk = chunk?;
        limiter.acquire(chunk.len() as u64).await;
        buf.extend_from_slice(&chunk);
        handle.add_downloaded(chunk.len() as u64);
    }
    Ok(Some(buf))
}

/// Écrit un segment via `.part` → rename atomique `<prefix>NNNNN.seg`.
pub fn write_segment(work: &Path, prefix: &str, idx: usize, bytes: &[u8]) -> Result<(), CoreError> {
    let part = work.join(format!("{prefix}{idx:05}.part"));
    let seg = work.join(format!("{prefix}{idx:05}.seg"));
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, &seg)?;
    Ok(())
}

/// Chemin final d'un segment (pour tester sa présence à la reprise).
pub fn seg_path(work: &Path, prefix: &str, idx: usize) -> PathBuf {
    work.join(format!("{prefix}{idx:05}.seg"))
}

/// Concatène `<prefix>_init.seg` (si présent) + tous les `<prefix>NNNNN.seg` triés.
pub fn concat_track(work: &Path, prefix: &str, has_init: bool) -> Result<PathBuf, CoreError> {
    use std::io::Write;
    let out_path = work.join(format!("{prefix}.cat"));
    let mut out = std::fs::File::create(&out_path)?;
    if has_init {
        let init = work.join(format!("{prefix}_init.seg"));
        if init.exists() {
            out.write_all(&std::fs::read(init)?)?;
        }
    }
    // Tri lexical == ordre numérique car les indices sont zéro-paddés sur 5 chiffres
    // (limite : 99999 segments, ≈ plusieurs jours de VOD — hors périmètre).
    let mut segs: Vec<PathBuf> = std::fs::read_dir(work)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(prefix) && n.ends_with(".seg") && !n.contains("_init"))
                .unwrap_or(false)
        })
        .collect();
    segs.sort();
    for s in segs {
        out.write_all(&std::fs::read(s)?)?;
    }
    out.flush()?;
    Ok(out_path)
}

/// Émet une progression par segments.
pub fn emit_segments(
    tx: &tokio::sync::mpsc::UnboundedSender<Progress>,
    id: &str,
    handle: &DownloadHandle,
    done: u32,
    total: u32,
    status: DownloadStatus,
) {
    let _ = tx.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: None,
        speed_bps: 0,
        status,
        segments_done: Some(done),
        segments_total: Some(total),
    });
}

/// Supprime le dossier de travail (annulation).
pub fn finish_cancelled(work: &Path) {
    let _ = std::fs::remove_dir_all(work);
}
```

- [ ] **Step 2 : Déclarer le module**

Dans `src/lib.rs` (download-core), ajouter (à côté de `pub mod dash;`) :

```rust
pub mod media_io;
```

- [ ] **Step 3 : Adapter `hls/engine.rs` pour consommer `media_io`**

Dans `src-tauri/crates/download-core/src/hls/engine.rs` :
1. SUPPRIMER les fonctions locales désormais dans `media_io` : `work_dir`, `fetch_to_vec`, `emit_segments`, `finish_cancelled`, et le corps de `concat_track` (et la logique inline `.part`→`.seg`).
2. Ajouter `use crate::media_io::{self, work_dir, fetch_to_vec, emit_segments, finish_cancelled, concat_track, write_segment, seg_path};` (ou préfixer `media_io::`).
3. Remplacer les appels internes : l'écriture `let part = ...; std::fs::write(...); std::fs::rename(...)` devient `media_io::write_segment(work, prefix, idx, &bytes)?;` ; le test de présence `seg_path.exists()` reste mais via `media_io::seg_path(work, prefix, idx)`. `download_track`/`download_hls` utilisent les helpers importés.
4. NE PAS changer la logique (mêmes signatures publiques `download_hls`/`DownloadHandle`).

- [ ] **Step 4 : Vérifier que les tests HLS restent verts**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core hls`
Expected: tous les tests `hls::*` PASS (parser, crypto, select, remux, engine) — comportement inchangé.

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core`
Expected: tout PASS.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/media_io.rs src-tauri/crates/download-core/src/lib.rs src-tauri/crates/download-core/src/hls/engine.rs
git commit -m "refactor(core): extract shared media_io helpers from hls engine"
```

---

## Task 8 : `Remuxer.decryption_key` + `SidecarRemuxer`

**Files:**
- Modify: `src-tauri/crates/download-core/src/hls/remux.rs`
- Modify: `src-tauri/src/remux.rs`

- [ ] **Step 1 : Ajouter `decryption_key` à `RemuxInputs` + adapter le FakeRemuxer**

Dans `src-tauri/crates/download-core/src/hls/remux.rs` :
1. Étendre la struct :

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RemuxInputs {
    pub video: PathBuf,
    pub audio: Option<PathBuf>,
    pub decryption_key: Option<String>,
}
```

2. Le `FakeRemuxer` (mod `fake`) ignore la clé mais enregistre toujours les inputs ; son corps `remux` reste inchangé (il enregistre `inputs` qui contient maintenant `decryption_key`).

- [ ] **Step 2 : Corriger les constructions existantes de `RemuxInputs`**

Le moteur HLS (`hls/engine.rs`) construit `RemuxInputs { video, audio }` — ajouter `decryption_key: None,` à cette construction. Le test `hls::remux` qui construit `RemuxInputs { video: v.clone(), audio: None }` doit devenir `RemuxInputs { video: v.clone(), audio: None, decryption_key: None }`.

- [ ] **Step 3 : `SidecarRemuxer` passe `-decryption_key`**

Dans `src-tauri/src/remux.rs`, dans `impl Remuxer for SidecarRemuxer`, construire les args en insérant `-decryption_key <hex>` AVANT chaque `-i` quand la clé est présente :

```rust
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into()];
        if let Some(key) = &inputs.decryption_key {
            args.push("-decryption_key".into());
            args.push(key.clone());
        }
        args.push("-i".into());
        args.push(inputs.video.to_string_lossy().to_string());
        if let Some(audio) = &inputs.audio {
            if let Some(key) = &inputs.decryption_key {
                args.push("-decryption_key".into());
                args.push(key.clone());
            }
            args.push("-i".into());
            args.push(audio.to_string_lossy().to_string());
        }
        args.extend(["-c".into(), "copy".into(), "-movflags".into(), "+faststart".into()]);
        args.push(output.to_string_lossy().to_string());

        let sidecar = self
            .app
            .shell()
            .sidecar("ffmpeg")
            .map_err(|e| CoreError::Media(format!("ffmpeg introuvable: {e}")))?;
        let output_run = sidecar
            .args(args)
            .output()
            .await
            .map_err(|e| CoreError::Media(format!("échec exécution ffmpeg: {e}")))?;
        if !output_run.status.success() {
            let stderr = String::from_utf8_lossy(&output_run.stderr);
            let tail: String = stderr.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            return Err(CoreError::Media(format!("ffmpeg a échoué: {tail}")));
        }
        Ok(())
    }
```

- [ ] **Step 4 : Vérifier**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core hls::remux`
Expected: PASS.

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compile (le `SidecarRemuxer` et le moteur HLS construisent bien `RemuxInputs` avec `decryption_key`).

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/hls/remux.rs src-tauri/src/remux.rs
git commit -m "feat: Remuxer decryption_key (ffmpeg -decryption_key for CENC ClearKey)"
```

---

## Task 9 : Moteur DASH `download_dash` (`dash/engine.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/dash/engine.rs`
- Modify: `src-tauri/crates/download-core/src/dash/mod.rs` (`pub mod engine;`)

> Réutilise `media_io` (Task 7), `DownloadHandle`/`Outcome` (engine.rs), `Remuxer`, et les modules `dash::*`. `selection` = (index variante vidéo, langue audio optionnelle), résolu à l'exécution depuis le MPD re-parsé.

- [ ] **Step 1 : Déclarer le module**

Dans `dash/mod.rs` : `pub mod engine;`

- [ ] **Step 2 : Écrire le moteur + tests (contre fakes)**

Créer `src/dash/engine.rs` :

```rust
use crate::dash::clearkey::{build_license_request, parse_jwk_response};
use crate::dash::mpd::{parse_mpd, Addressing, ContentType, Mpd, ProtScheme};
use crate::dash::segments::{base_media, build_segments, SegmentRef};
use crate::dash::select::{best_video_index, pick_audio, video_variants};
use crate::dash::sidx::parse_sidx;
use crate::engine::{DownloadHandle, Outcome};
use crate::hls::remux::{RemuxInputs, Remuxer};
use crate::http::ByteFetcher;
use crate::media_io::{concat_track, emit_segments, fetch_to_vec, finish_cancelled, seg_path, work_dir, write_segment};
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::CoreError;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Sélection persistée d'un download DASH.
#[derive(Debug, Clone, PartialEq)]
pub struct DashSelection {
    pub video_index: u32,
    pub audio_lang: Option<String>,
}

/// Télécharge un flux DASH VOD vers `dest` (.mp4) : segments via le moteur Rust,
/// remux (et déchiffrement CENC ClearKey) via ffmpeg.
#[allow(clippy::too_many_arguments)]
pub async fn download_dash<F: ByteFetcher, R: Remuxer>(
    fetcher: &F,
    remux: &R,
    id: &str,
    mpd_url: &str,
    selection: &DashSelection,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    // 1. Récupère + parse le MPD.
    let mpd_bytes = match fetch_to_vec(fetcher, mpd_url, None, headers, &limiter, &handle).await? {
        Some(b) => b,
        None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
    };
    let mpd = parse_mpd(&String::from_utf8_lossy(&mpd_bytes), mpd_url)?;
    if mpd.dynamic {
        return Err(CoreError::Media("flux live DASH non supporté (à venir)".into()));
    }

    // 2. Résout les Representations vidéo + audio.
    let vlist = video_variants(&mpd);
    let (vcoord, _) = vlist
        .iter()
        .find(|(_, v)| v.index == selection.video_index)
        .or_else(|| vlist.first())
        .ok_or_else(|| CoreError::Media("aucune variante vidéo".into()))?;
    let video_as = &mpd.periods[vcoord.period].adaptation_sets[vcoord.adaptation];
    let video_repr = &video_as.representations[vcoord.representation];
    let audio_coord = pick_audio(&mpd, selection.audio_lang.as_deref());

    // 3. Clé ClearKey si chiffré (vidéo ou audio).
    let key = resolve_key(fetcher, &mpd, vcoord.adaptation, headers, &limiter, &handle).await?;

    // 4. Plans de segments.
    let vplan = build_segments(video_repr, base_url_of(video_repr), mpd.duration_secs);
    let aplan = match &audio_coord {
        Some(c) => {
            let ar = &mpd.periods[c.period].adaptation_sets[c.adaptation].representations[c.representation];
            Some((ar, build_segments(ar, base_url_of(ar), mpd.duration_secs)))
        }
        None => None,
    };

    let total_segments = (vplan.media.len()
        + aplan.as_ref().map(|(_, p)| p.media.len()).unwrap_or(0)) as u32;
    let mut done: u32 = 0;

    // 5. Télécharge la piste vidéo (résout SegmentBase via sidx si besoin).
    match download_track(fetcher, video_repr, vplan, "v", &work, headers, &limiter, &handle, &progress, id, &mut done, total_segments).await? {
        Outcome::Completed => {}
        Outcome::Paused => { emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused); return Ok(Outcome::Paused); }
        Outcome::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
    }

    // 6. Piste audio.
    let has_audio = aplan.is_some();
    if let Some((ar, ap)) = aplan {
        match download_track(fetcher, ar, ap, "a", &work, headers, &limiter, &handle, &progress, id, &mut done, total_segments).await? {
            Outcome::Completed => {}
            Outcome::Paused => { emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused); return Ok(Outcome::Paused); }
            Outcome::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        }
    }

    // 7. Concat + remux (avec clé éventuelle).
    let video_cat = concat_track(&work, "v", true)?;
    let audio_cat = if has_audio { Some(concat_track(&work, "a", true)?) } else { None };
    remux.remux(RemuxInputs { video: video_cat, audio: audio_cat, decryption_key: key }, dest).await?;

    // 8. Nettoyage + progression finale.
    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
        speed_bps: 0, status: DownloadStatus::Completed,
        segments_done: Some(total_segments), segments_total: Some(total_segments),
    });
    Ok(Outcome::Completed)
}

/// BaseURL effective d'une Representation (résolue par le parser), utilisée pour
/// résoudre les templates/listes relatifs.
fn base_url_of(repr: &crate::dash::mpd::Representation) -> &str { &repr.base }

#[allow(clippy::too_many_arguments)]
async fn download_track<F: ByteFetcher>(
    fetcher: &F,
    repr: &crate::dash::mpd::Representation,
    mut plan: crate::dash::segments::SegmentPlan,
    prefix: &str,
    work: &Path,
    headers: &RequestHeaders,
    limiter: &Arc<RateLimiter>,
    handle: &DownloadHandle,
    progress: &mpsc::UnboundedSender<Progress>,
    id: &str,
    done: &mut u32,
    total: u32,
) -> Result<Outcome, CoreError> {
    // Init.
    if let Some(init) = &plan.init {
        let bytes = match fetch_to_vec(fetcher, &init.url, init.byte_range, headers, limiter, handle).await? {
            Some(b) => b, None => return Ok(Outcome::Cancelled),
        };
        std::fs::write(work.join(format!("{prefix}_init.seg")), bytes)?;
    }
    // SegmentBase : lire le sidx pour obtenir les médias.
    if let Addressing::Base(b) = &repr.addressing {
        if let Some((s, e)) = b.index_range {
            let idx = match fetch_to_vec(fetcher, &b.url, Some((s, e)), headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Outcome::Cancelled),
            };
            let ranges = parse_sidx(&idx, e + 1)?;
            plan.media = base_media(b, &ranges);
        }
    }

    for (idx, seg) in plan.media.iter().enumerate() {
        if seg_path(work, prefix, idx).exists() {
            *done += 1;
            continue;
        }
        if handle.is_cancelled() { return Ok(Outcome::Cancelled); }
        if handle.is_paused() { return Ok(Outcome::Paused); }
        let bytes = match fetch_to_vec(fetcher, &seg.url, seg.byte_range, headers, limiter, handle).await? {
            Some(b) => b, None => return Ok(Outcome::Cancelled),
        };
        write_segment(work, prefix, idx, &bytes)?;
        *done += 1;
        emit_segments(progress, id, handle, *done, total, DownloadStatus::Downloading);
    }
    Ok(Outcome::Completed)
}

/// Récupère la clé ClearKey (hex) si l'AdaptationSet vidéo est chiffré, sinon None.
async fn resolve_key<F: ByteFetcher>(
    fetcher: &F,
    mpd: &Mpd,
    video_adaptation: usize,
    headers: &RequestHeaders,
    _limiter: &Arc<RateLimiter>,
    _handle: &DownloadHandle,
) -> Result<Option<String>, CoreError> {
    let cp = match &mpd.periods[0].adaptation_sets[video_adaptation].content_protection {
        Some(cp) => cp,
        None => return Ok(None),
    };
    let kid = match &cp.default_kid {
        Some(k) => k.clone(),
        None => return Err(CoreError::Media("contenu chiffré sans default_KID".into())),
    };
    match (&cp.scheme, &cp.clearkey_laurl) {
        (ProtScheme::ClearKey, Some(laurl)) | (ProtScheme::Cenc, Some(laurl)) => {
            let body = build_license_request(&kid)?;
            let resp = fetcher.post(laurl, body.as_bytes(), "application/json", headers).await?;
            Ok(Some(parse_jwk_response(&String::from_utf8_lossy(&resp))?))
        }
        _ => Err(CoreError::Media("DRM non supporté (ClearKey requis)".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::model::ProbeResult;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use std::collections::HashMap as Map;
    use tempfile::tempdir;

    struct MapFetcher { routes: Map<String, Vec<u8>>, post_resp: Option<Vec<u8>> }
    #[async_trait]
    impl ByteFetcher for MapFetcher {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
        }
        async fn fetch_range(&self, url: &str, range: Option<(u64, u64)>, _h: &RequestHeaders)
            -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = self.routes.get(url).cloned().unwrap_or_default();
            let slice = match range {
                Some((s, e)) => data[s as usize..(e as usize + 1).min(data.len())].to_vec(),
                None => data,
            };
            Ok(stream::iter(vec![Ok(Bytes::from(slice))]).boxed())
        }
        async fn post(&self, _url: &str, _b: &[u8], _ct: &str, _h: &RequestHeaders) -> Result<Vec<u8>, CoreError> {
            Ok(self.post_resp.clone().unwrap_or_default())
        }
    }
    fn unlimited() -> Arc<RateLimiter> { Arc::new(RateLimiter::unlimited()) }

    const MPD: &str = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT4S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate initialization="https://h/p/vinit.mp4" media="https://h/p/v$Number$.m4s" timescale="1000" duration="2000" startNumber="1"/>
    <Representation id="v0" bandwidth="800000" width="640" height="360"/>
  </AdaptationSet></Period>
</MPD>"#;

    #[tokio::test]
    async fn downloads_clear_dash_and_remuxes() {
        let mut routes = Map::new();
        routes.insert("https://h/p/m.mpd".into(), MPD.as_bytes().to_vec());
        routes.insert("https://h/p/vinit.mp4".into(), b"INIT".to_vec());
        routes.insert("https://h/p/v1.m4s".into(), b"AAAA".to_vec());
        routes.insert("https://h/p/v2.m4s".into(), b"BBBB".to_vec());
        let fetcher = MapFetcher { routes, post_resp: None };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let sel = DashSelection { video_index: 0, audio_lang: None };

        let outcome = download_dash(&fetcher, &remux, "id1", "https://h/p/m.mpd", &sel, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default()).await.unwrap();

        assert_eq!(outcome, Outcome::Completed);
        // FakeRemuxer concatène video.cat = INIT + AAAA + BBBB.
        assert_eq!(std::fs::read(&dest).unwrap(), b"INITAAAABBBB");
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists());
    }

    #[tokio::test]
    async fn rejects_dynamic_mpd() {
        let mut routes = Map::new();
        routes.insert("https://h/p/m.mpd".into(),
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic"><Period/></MPD>"#.as_bytes().to_vec());
        let fetcher = MapFetcher { routes, post_resp: None };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let sel = DashSelection { video_index: 0, audio_lang: None };
        let err = download_dash(&fetcher, &remux, "id2", "https://h/p/m.mpd", &sel, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default()).await.unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }
}
```

> Note d'implémentation : `base_url_of(repr)` renvoie `&repr.base` (la BaseURL effective résolue par le parser en Task 2). `build_segments` résout les templates/listes **relatifs** contre cette base ; pour SegmentBase, `SegmentBase.url` est déjà absolu. Les `media`/`init` de SegmentTemplate/SegmentList restent stockés **bruts** (relatifs, avec `$Number$` etc.) par le parser et ne sont résolus+expansés qu'ici via `base`.

- [ ] **Step 3 : Ajouter les accesseurs si manquants**

Le moteur utilise `handle.is_cancelled()/is_paused()/add_downloaded()/downloaded()` — déjà ajoutés en 4a (Task 7 de 4a). Vérifier leur présence dans `engine.rs` ; sinon les ajouter (cf. 4a).

- [ ] **Step 4 : Vérifier**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core dash::engine`
Expected: 2 tests PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core`
Expected: tout PASS.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/dash/
git commit -m "feat(core): DASH download engine (segments, sidx, ClearKey, remux, live guard)"
```

---

## Task 10 : Modèle & storage — `DownloadKind::Dash` + `media_selection`

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs`
- Modify: `src-tauri/crates/download-core/src/storage.rs`
- Modify: `src/types.ts`

- [ ] **Step 1 : Test storage (roundtrip Dash + media_selection)**

Dans `storage.rs` `mod tests`, ajouter :

```rust
    #[test]
    fn round_trips_dash_and_media_selection() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("d1");
        d.kind = DownloadKind::Dash;
        d.media_selection = Some(r#"{"video":0,"audio":"en"}"#.into());
        s.upsert(&d).unwrap();
        let got = s.get_by_id("d1").unwrap().unwrap();
        assert_eq!(got.kind, DownloadKind::Dash);
        assert_eq!(got.media_selection.as_deref(), Some(r#"{"video":0,"audio":"en"}"#));
        let plain = sample("d2");
        s.upsert(&plain).unwrap();
        assert_eq!(s.get_by_id("d2").unwrap().unwrap().media_selection, None);
    }
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core storage`
Expected: FAIL de compilation (`Dash`, `media_selection` absents).

- [ ] **Step 3 : `model.rs` — `Dash` + champ**

Dans l'enum `DownloadKind`, ajouter `Dash` :

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DownloadKind {
    #[default]
    Regular,
    Hls,
    Dash,
}
```

Dans `struct Download`, après `audio_url` :

```rust
    #[serde(default)]
    pub media_selection: Option<String>,
```

- [ ] **Step 4 : `storage.rs` — migration + colonnes**

1. Dans `from_conn`, après la migration `audio_url` de 4a :

```rust
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN media_selection TEXT", []);
```

2. `kind_to_str`/`kind_from_str` : ajouter `Dash`.

```rust
// dans kind_to_str
        DownloadKind::Dash => "dash",
// dans kind_from_str
        "dash" => DownloadKind::Dash,
```

3. `upsert` : passer à 17 colonnes en ajoutant `media_selection` (?17). Étendre la liste de colonnes, les `VALUES`, le `DO UPDATE SET`, et `rusqlite::params![ … , d.media_selection ]`.

4. `list` et `get_by_id` : ajouter `media_selection` au SELECT (index 16) et au mapping `media_selection: row.get(16)?,`.

5. Helper de test `sample(...)` : ajouter `media_selection: None,`.

- [ ] **Step 5 : Vérifier la réussite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p download-core`
Expected: tout PASS.

- [ ] **Step 6 : Types frontend**

Dans `src/types.ts` :
- `DownloadKind` : `"regular" | "hls" | "dash"`.
- `interface Download` : ajouter `media_selection?: string | null;`.

- [ ] **Step 7 : Build front + commit**

Run: `npm run build`
Expected: tsc OK.

```bash
git add src-tauri/crates/download-core/src/model.rs src-tauri/crates/download-core/src/storage.rs src/types.ts
git commit -m "feat(core): DownloadKind::Dash + media_selection persistence"
```

---

## Task 11 : Backend — `MediaProbe`, `probe_dash`, `add_media_download`, branche `launch`

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

> On généralise `pending_media` (HLS+DASH) et on remplace `add_hls_download` par `add_media_download`.

- [ ] **Step 1 : `state.rs` — `MediaProbe`**

Remplacer la struct `PendingMedia` par une version taguée :

```rust
use download_core::dash::mpd::Mpd;

pub enum MediaProbe {
    Hls(download_core::hls::playlist::MasterPlaylist),
    Dash(Mpd),
}

pub struct PendingMedia {
    pub probe: MediaProbe,
    pub headers: RequestHeaders,
    pub file_name: String,
    /// URL source (master HLS ou MPD DASH), nécessaire pour reconstruire un download DASH.
    pub source_url: String,
    pub created: Instant,
}
```

(Garder `MasterPlaylist`/`RequestHeaders`/`Instant` importés.) `probe_hls_inner` et `probe_dash_inner` renseignent `source_url: url.to_string()` dans le `PendingMedia` qu'ils insèrent.

- [ ] **Step 2 : `commands.rs` — renommer la struct sérialisée + adapter `probe_hls`**

1. Renommer `HlsVariant` → `MediaVariant` (struct serde) ; garder les champs `index/resolution/bandwidth/est_size`. Mettre à jour `MediaVariantsEvent.variants: Vec<MediaVariant>`.
2. `probe_hls_inner` (4a) range désormais `MediaProbe::Hls(master)` :

```rust
        // (conserver le bloc retain TTL de 4a juste avant l'insert)
        pending.insert(
            probe_id.clone(),
            PendingMedia { probe: MediaProbe::Hls(master), headers, file_name: file_name.clone(), source_url: url.to_string(), created: std::time::Instant::now() },
        );
```

(L'éviction TTL de 4a est conservée. Les `variants` sont construits comme en 4a, en type `MediaVariant`.)

- [ ] **Step 3 : `commands.rs` — `probe_dash`**

Ajouter (à côté de `probe_hls`) un `probe_dash` qui réutilise le wrapper d'erreur `media://error`. Le plus simple : un `probe_dash_inner` + le même wrapper que `probe_hls`. Implémentation de `probe_dash_inner` :

```rust
use download_core::dash::mpd::parse_mpd;
use download_core::dash::select::video_variants;

async fn probe_dash_inner(app: &AppHandle, url: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<(), String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let text = tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, url, &headers))
        .await
        .map_err(|_| "délai dépassé lors de la récupération du MPD".to_string())?
        .map_err(|e| e.to_string())?;
    let mpd = parse_mpd(&text, url).map_err(|e| e.to_string())?;
    if mpd.dynamic {
        return Err("flux live DASH non supporté (à venir)".into());
    }
    let vlist = video_variants(&mpd);
    if vlist.is_empty() {
        return Err("aucune variante vidéo DASH trouvée".into());
    }
    let variants: Vec<MediaVariant> = vlist.iter().map(|(_, v)| MediaVariant {
        index: v.index, resolution: v.resolution.clone(), bandwidth: v.bandwidth, est_size: v.est_size,
    }).collect();
    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = suggested_name.unwrap_or_else(|| "video".to_string());
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let ttl = std::time::Duration::from_secs(30 * 60);
        pending.retain(|_, pm| pm.created.elapsed() < ttl);
        pending.insert(probe_id.clone(), PendingMedia { probe: MediaProbe::Dash(mpd), headers, file_name: file_name.clone(), source_url: url.to_string(), created: std::time::Instant::now() });
    }
    let _ = app.emit("media://variants", MediaVariantsEvent { probe_id, file_name, variants });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus();
    }
    Ok(())
}

pub(crate) async fn probe_dash(app: &AppHandle, url: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<(), String> {
    let result = probe_dash_inner(app, url, headers, suggested_name).await;
    if let Err(e) = &result {
        let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
        if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    }
    result
}
```

- [ ] **Step 4 : `commands.rs` — `add_media_download` (remplace `add_hls_download`)**

Remplacer la commande `add_hls_download` par `add_media_download`, qui dispatche sur le `MediaProbe` :

```rust
#[tauri::command]
pub async fn add_media_download(app: AppHandle, probe_id: String, variant_index: u32) -> Result<Download, String> {
    // Extraire de quoi construire le Download (lock relâché avant tout await long).
    enum Built { Hls { url: String, audio_url: Option<String> }, Dash { mpd_url: String, selection: String } }
    let (built, file_name, headers) = {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let pm = pending.get(&probe_id).ok_or_else(|| "probe expiré".to_string())?;
        let file_name = download_core::naming::with_extension(
            &download_core::naming::choose_file_name(Some(&pm.file_name), None), "mp4");
        let headers = pm.headers.clone();
        let built = match &pm.probe {
            MediaProbe::Hls(master) => {
                let variant = master.variants.get(variant_index as usize).ok_or_else(|| "variante invalide".to_string())?;
                let audio_url = download_core::hls::select::pick_audio(master, variant).and_then(|a| a.uri.clone());
                Built::Hls { url: variant.uri.clone(), audio_url }
            }
            MediaProbe::Dash(mpd) => {
                // valider l'index
                let exists = download_core::dash::select::video_variants(mpd).iter().any(|(_, v)| v.index == variant_index);
                if !exists { return Err("variante invalide".into()); }
                let selection = serde_json::json!({ "video": variant_index, "audio": serde_json::Value::Null }).to_string();
                Built::Dash { mpd_url: pm.source_url.clone(), selection }
            }
        };
        let r = (built, file_name, headers);
        pending.remove(&probe_id);
        r
    };

    // Dossier de destination (vidéo).
    let dest_dir = {
        let state = app.state::<AppState>();
        match state.storage.category_dir(Category::Video).map_err(|e| e.to_string())? {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;
    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) { return Err("nom de fichier invalide".into()); }

    let (kind, url, audio_url, media_selection) = match built {
        Built::Hls { url, audio_url } => (DownloadKind::Hls, url, audio_url, None),
        Built::Dash { mpd_url, selection } => (DownloadKind::Dash, mpd_url, None, Some(selection)),
    };
    let download = Download {
        id: uuid::Uuid::new_v4().to_string(), url, file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None, downloaded_bytes: 0, status: DownloadStatus::Queued,
        supports_ranges: false, error: None, priority: Priority::Normal, category: Category::Video,
        referrer: headers.referrer, cookie: headers.cookie, user_agent: headers.user_agent,
        kind, audio_url, media_selection,
    };
    { app.state::<AppState>().storage.upsert(&download).map_err(|e| e.to_string())?; }
    reevaluate(&app);
    Ok(download)
}
```

- [ ] **Step 5 : `launch` — branche Dash**

Dans `commands.rs` `launch`, le `match kind` (ajouté en 4a avec `Regular`/`Hls`) gagne un bras `Dash` :

```rust
            DownloadKind::Dash => {
                let remux = crate::remux::SidecarRemuxer::new(app_for_dl.clone());
                let selection = match download.media_selection.as_deref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
                    Some(v) => download_core::dash::engine::DashSelection {
                        video_index: v.get("video").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        audio_lang: v.get("audio").and_then(|x| x.as_str()).map(String::from),
                    },
                    None => download_core::dash::engine::DashSelection { video_index: 0, audio_lang: None },
                };
                download_core::dash::engine::download_dash(&fetcher, &remux, &id_for_dl, &url, &selection, &dest, handle, limiter, tx, &headers).await
            }
```

(`kind`/`url`/`audio_url` sont déjà extraits en locals avant le spawn en 4a ; ajouter `let media_selection = download.media_selection.clone();` et utiliser `media_selection` au lieu de `download.media_selection` dans le bras.)

- [ ] **Step 6 : `lib.rs` — handler**

Dans `tauri::generate_handler![ … ]`, remplacer `commands::add_hls_download,` par `commands::add_media_download,`.

- [ ] **Step 7 : Vérifier la compilation + tests**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compile.

Run: `cargo test --workspace --manifest-path src-tauri/Cargo.toml`
Expected: tout PASS.

- [ ] **Step 8 : Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): generalize pending_media + add_media_download + probe_dash + DASH launch"
```

---

## Task 12 : Bridge — route `kind=="dash"`

**Files:**
- Modify: `src-tauri/src/bridge/server.rs`

- [ ] **Step 1 : Router DASH**

Dans `server.rs`, branche `ServerReply::CaptureRequested { … kind }` (ajoutée en 4a), ajouter le cas `dash` à côté de `hls` :

```rust
                if kind.as_deref() == Some("hls") {
                    match commands::probe_hls(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "hls" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else if kind.as_deref() == Some("dash") {
                    match commands::probe_dash(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "dash" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else {
                    // … bras add_url existant inchangé …
                }
```

- [ ] **Step 2 : Vérifier**

Run: `cargo test --manifest-path src-tauri/Cargo.toml bridge`
Expected: tests bridge PASS.

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compile.

- [ ] **Step 3 : Commit**

```bash
git add src-tauri/src/bridge/server.rs
git commit -m "feat(bridge): route kind=dash captures to probe_dash"
```

---

## Task 13 : Extension — détection DASH

**Files:**
- Modify: `extension/media-rules.js`
- Modify: `extension/media-rules.test.js`
- Modify: `extension/background.js`

- [ ] **Step 1 : Tests `isDashUrl`/`isDashContentType`**

Dans `extension/media-rules.test.js`, ajouter :

```js
import { isDashUrl, isDashContentType } from "./media-rules.js";

describe("isDashUrl", () => {
  it("matches .mpd ignoring query", () => {
    expect(isDashUrl("https://h/p/manifest.mpd")).toBe(true);
    expect(isDashUrl("https://h/p/manifest.mpd?t=1")).toBe(true);
    expect(isDashUrl("https://h/p/master.m3u8")).toBe(false);
  });
});

describe("isDashContentType", () => {
  it("matches application/dash+xml ignoring charset/case", () => {
    expect(isDashContentType("application/dash+xml")).toBe(true);
    expect(isDashContentType("application/DASH+XML; charset=utf-8")).toBe(true);
    expect(isDashContentType("video/mp4")).toBe(false);
  });
});
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `npx vitest run extension/media-rules.test.js`
Expected: FAIL (fonctions absentes).

- [ ] **Step 3 : Implémenter dans `media-rules.js`**

Ajouter à `extension/media-rules.js` :

```js
// Vrai si le chemin de l'URL finit par .mpd (query/fragment ignorés).
export function isDashUrl(url) {
  const path = String(url ?? "").split(/[?#]/)[0];
  return /\.mpd$/i.test(path);
}

// Vrai si le Content-Type est DASH (application/dash+xml).
export function isDashContentType(ct) {
  const base = String(ct ?? "").split(";")[0].trim().toLowerCase();
  return base === "application/dash+xml";
}
```

- [ ] **Step 4 : `background.js` — enregistrer les .mpd + choisir le kind**

1. Importer : `import { isHlsUrl, isHlsContentType, isDashUrl, isDashContentType, dedupeStreams } from "./media-rules.js";`
2. Dans le listener `chrome.webRequest.onBeforeRequest`, enregistrer aussi les DASH :

```js
    if (isHlsUrl(details.url) || isDashUrl(details.url)) recordStream(details.tabId, details.url);
```

3. Dans `onHeadersReceived`, ajouter la détection DASH :

```js
    if (ct && (isHlsContentType(ct.value) || isDashContentType(ct.value))) recordStream(details.tabId, details.url);
```

4. `sendMedia(url, referrer)` choisit le `kind` selon l'URL :

```js
async function sendMedia(url, referrer) {
  const kind = isDashUrl(url) ? "dash" : "hls";
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind })));
      return true;
    } catch (_e) { return false; }
  }
  pending.push({ url, referrer, kind, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```

(`flushPending` propage déjà `kind: it.kind` — inchangé.)

- [ ] **Step 5 : Vérifier**

Run: `npx vitest run extension/media-rules.test.js`
Expected: PASS (HLS + DASH).

Run: `npm run test`
Expected: toutes les suites JS PASS.

- [ ] **Step 6 : Commit**

```bash
git add extension/media-rules.js extension/media-rules.test.js extension/background.js
git commit -m "feat(extension): detect DASH .mpd via webRequest + kind routing"
```

---

## Task 14 : Frontend — `addMediaDownload` + badge DASH

**Files:**
- Modify: `src/api.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/DownloadRow.tsx`
- Modify: `src/components/HlsVariantDialog.tsx` (rename de type, optionnel)

- [ ] **Step 1 : `api.ts` — renommer la commande**

Renommer `addHlsDownload` → `addMediaDownload` (la commande Tauri est désormais `add_media_download`) :

```ts
export function addMediaDownload(probeId: string, variantIndex: number): Promise<Download> {
  return invoke<Download>("add_media_download", { probeId, variantIndex });
}
```

(Optionnel : renommer l'interface `HlsVariant` → `MediaVariant` ; si fait, mettre à jour `MediaVariantsEvent.variants`, `HlsVariantDialog.tsx`. Sinon garder `HlsVariant` — le nom n'impacte pas le runtime. **Décision : garder `HlsVariant` pour minimiser le churn**, seul le nom de la fonction change.)

- [ ] **Step 2 : `App.tsx` — appeler `addMediaDownload`**

Dans `handleHlsConfirm` (4a), remplacer `api.addHlsDownload(...)` par `api.addMediaDownload(...)`. (Le reste — `mediaEvent`, dialogue, `onMediaVariants`, `onMediaError` — est inchangé et sert HLS comme DASH.)

- [ ] **Step 3 : `DownloadRow.tsx` — badge DASH**

À côté du badge HLS (4a), ajouter le badge DASH. Remplacer le bloc du nom de fichier :

```tsx
        <div className="font-semibold truncate">
          {d.kind === "hls" && (
            <span className="mr-1 px-1 py-0.5 text-[10px] rounded bg-indigo-100 text-indigo-700 align-middle">HLS</span>
          )}
          {d.kind === "dash" && (
            <span className="mr-1 px-1 py-0.5 text-[10px] rounded bg-violet-100 text-violet-700 align-middle">DASH</span>
          )}
          {d.file_name}
        </div>
```

La logique de progression segments (`isHls && seg`) doit aussi s'appliquer au DASH. Remplacer `const isHls = d.kind === "hls";` par :

```tsx
  const isSegmented = d.kind === "hls" || d.kind === "dash";
```

et utiliser `isSegmented` partout où `isHls` était utilisé pour la progression (`pct`/`stateLabel`).

- [ ] **Step 4 : Build + tests**

Run: `npm run build`
Expected: tsc + vite OK.

Run: `npm run test`
Expected: PASS.

- [ ] **Step 5 : Commit**

```bash
git add src/api.ts src/App.tsx src/components/DownloadRow.tsx
git commit -m "feat(frontend): addMediaDownload + DASH badge + segment progress for DASH"
```

---

## Task 15 : Vérification finale + smoke-tests

**Files:** (aucun — vérification)

- [ ] **Step 1 : Suites automatiques**

Run: `cargo test --workspace --manifest-path src-tauri/Cargo.toml`
Expected: tout PASS (HLS de 4a + DASH).

Run: `npm run test`
Expected: tout PASS.

Run: `cargo build --manifest-path src-tauri/Cargo.toml` et `npm run build`
Expected: 0 erreur, idéalement 0 warning.

- [ ] **Step 2 : Smoke-test bout en bout (manuel, nécessite le vrai ffmpeg de 4a)**

Avec un `ffmpeg.exe` réel déposé (cf. 4a), `npm run tauri dev` + extension chargée :
1. **DASH VOD clair** (DASH-IF test vectors, p. ex. SegmentTemplate $Number$, SegmentTimeline, et un SegmentBase) : popup → « Télécharger la vidéo » → dialogue qualités → choisir → badge DASH + « X/N segments » → `.mp4` lisible **avec son**.
2. **DASH ClearKey** (vecteur DASH-IF ClearKey) → `.mp4` déchiffré lisible.
3. **MPD dynamic** (live) → download en **Erreur** « live DASH non supporté » + bannière.
4. **Pause/reprise** d'un DASH en cours → reprend sans re-télécharger les segments faits.
5. **Non-régression** : un HLS VOD (4a) et un `.zip` classique se comportent comme avant.

- [ ] **Step 3 : Finalisation**

Suivre la skill `finishing-a-development-branch` pour merger `phase-4b1-dash-vod` (après merge éventuel de 4a) avec `merge --no-ff`.

---

## Notes d'implémentation transverses

- **Réutilisation maximale de 4a** : `media_io` (helpers segments), `Remuxer`/`SidecarRemuxer` (étendus `decryption_key`), `pending_media`/dialogue/`media://variants`/`media://error`, `DownloadHandle`/`Outcome`/`RateLimiter`, le pattern reprise (`.seg`). Le moteur DASH est structurellement le jumeau du moteur HLS.
- **ClearKey via ffmpeg** : on ne déchiffre jamais en Rust ; la clé hex est passée à ffmpeg (`-decryption_key`). Échec de clé ⇒ ffmpeg sort en erreur ⇒ statut Error.
- **Limites assumées (spec §7)** : DASH VOD seul (dynamic rejeté), 1ère Period seulement, pas de xlink, pas de DRM hors ClearKey, ffmpeg Windows. Multi-Period et live → 4b-2.
- **Annulation DASH** : `cancel_download` supprime déjà `dest+".fghls"` (ajouté en 4a) — vaut pour DASH (même work-dir).
- **Résolution d'URL DASH** : le parser stocke `Representation.base` (BaseURL effective, chaîne MPD→Period→AdaptationSet→Representation résolue) et garde les `media`/`init` de template/liste **bruts** ; `build_segments` les résout+expanse contre `repr.base`. Limite : un SegmentTemplate hérité de l'AdaptationSet est résolu contre la base du niveau où il est déclaré ; un cas rare (rep avec BaseURL propre **et** template hérité) peut être légèrement imprécis — à valider au smoke-test.
