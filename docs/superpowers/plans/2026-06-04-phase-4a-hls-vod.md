# Phase 4a — Téléchargement vidéo HLS (VOD) — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Détecter les flux HLS VOD dans le navigateur, choisir une qualité dans FlashGet, télécharger les segments avec le moteur Rust existant (cookies/referer, rate-limiter, retries, pause/reprise), déchiffrer l'AES-128, muxer l'audio séparé, puis assembler en `.mp4` via ffmpeg embarqué (remux `-c copy`).

**Architecture :** Un nouveau module `download-core::hls` (parser de playlist, crypto AES-128, sélection de variante, moteur `download_hls`) **pur et Tauri-agnostique**, qui télécharge/déchiffre/concatène les segments via le trait `ByteFetcher` existant puis délègue le remux à un trait `Remuxer`. Côté `src-tauri`, un `SidecarRemuxer` appelle le binaire ffmpeg embarqué (sidecar Tauri). La détection se fait dans l'extension via `chrome.webRequest` (observation) ; l'utilisateur déclenche le download depuis le popup ; le backend parse le master, propose les variantes (registre `pending_media`, event `media://variants`) et l'UI affiche un dialogue de choix.

**Tech Stack :** Rust (tokio, reqwest, aes/cbc, rusqlite), Tauri 2 (`tauri-plugin-shell` pour le sidecar ffmpeg), React/TypeScript, extension Chrome MV3, tests `cargo test` + Vitest.

**Référence spec :** [docs/superpowers/specs/2026-06-04-phase-4a-hls-vod-design.md](../specs/2026-06-04-phase-4a-hls-vod-design.md)

**Conventions de test :**
- Cœur Rust : `cd src-tauri && cargo test -p download-core <filtre>`.
- Crate Tauri : `cd src-tauri && cargo test <filtre>`.
- JS/TS : `npx vitest run <chemin>` (ou `npm run test`).
- Commits : un par étape verte, message conventionnel (`feat`/`test`/`chore`).

---

## Task 1 : Parser HLS — utilitaires (attributs + résolution d'URI) + erreur `Media`

**Files:**
- Create: `src-tauri/crates/download-core/src/hls/mod.rs`
- Create: `src-tauri/crates/download-core/src/hls/playlist.rs`
- Modify: `src-tauri/crates/download-core/src/lib.rs` (déclarer `pub mod hls;` + variante d'erreur `Media`)

- [ ] **Step 1 : Déclarer le module `hls` et l'erreur `Media`**

Dans `src-tauri/crates/download-core/src/lib.rs`, ajouter la ligne de module (après `pub mod scheduler;`) :

```rust
pub mod hls;
```

Et ajouter une variante à l'enum `CoreError` (juste avant `#[error("serialization error: {0}")]`) :

```rust
    #[error("media error: {0}")]
    Media(String),
```

- [ ] **Step 2 : Créer le squelette du module hls**

Créer `src-tauri/crates/download-core/src/hls/mod.rs` :

```rust
//! Téléchargement de flux HLS VOD : parsing de playlist, déchiffrement AES-128,
//! sélection de variante, moteur de download (segments → remux ffmpeg).
pub mod playlist;
```

- [ ] **Step 3 : Écrire les tests des utilitaires (attributs + URI)**

Créer `src-tauri/crates/download-core/src/hls/playlist.rs` avec, pour commencer, uniquement les helpers et leurs tests :

```rust
use std::collections::HashMap;

/// Parse la liste d'attributs d'une ligne `#EXT-X-...:key=val,key="val,with,commas",...`.
/// Gère les valeurs entre guillemets (qui peuvent contenir des virgules). Les guillemets
/// sont retirés. Renvoie une map clé→valeur.
pub(crate) fn parse_attributes(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key = s[key_start..i].trim().to_string();
        i += 1; // saute '='
        let val;
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1; // saute guillemet ouvrant
            let vs = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            val = s[vs..i].to_string();
            if i < bytes.len() {
                i += 1; // saute guillemet fermant
            }
            while i < bytes.len() && bytes[i] != b',' {
                i += 1; // jusqu'à la virgule
            }
        } else {
            let vs = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            val = s[vs..i].trim().to_string();
        }
        if i < bytes.len() && bytes[i] == b',' {
            i += 1; // saute la virgule séparatrice
        }
        if !key.is_empty() {
            map.insert(key, val);
        }
    }
    map
}

/// Résout une URI (éventuellement relative) contre l'URL de base de la playlist.
/// Gestion simplifiée (pas de normalisation `..`) : absolue, protocole-relative `//`,
/// racine `/`, sinon relative au répertoire de `base`.
pub(crate) fn resolve_uri(base: &str, uri: &str) -> String {
    let uri = uri.trim();
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return uri.to_string();
    }
    let scheme_end = base.find("://").map(|p| p + 3).unwrap_or(0);
    if let Some(rest) = uri.strip_prefix("//") {
        let scheme = &base[..scheme_end.saturating_sub(3).max(0)];
        return format!("{scheme}://{rest}");
    }
    // origine = scheme://host (jusqu'au premier '/' après le scheme)
    let after_scheme = &base[scheme_end..];
    let host_len = after_scheme.find('/').unwrap_or(after_scheme.len());
    let origin = &base[..scheme_end + host_len];
    if uri.starts_with('/') {
        return format!("{origin}{uri}");
    }
    // relative : répertoire de base (jusqu'au dernier '/')
    let dir_end = base.rfind('/').map(|p| p + 1).unwrap_or(base.len());
    format!("{}{}", &base[..dir_end], uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_basic() {
        let m = parse_attributes("BANDWIDTH=1280000,RESOLUTION=640x360,AUDIO=\"aud1\"");
        assert_eq!(m.get("BANDWIDTH").unwrap(), "1280000");
        assert_eq!(m.get("RESOLUTION").unwrap(), "640x360");
        assert_eq!(m.get("AUDIO").unwrap(), "aud1");
    }

    #[test]
    fn attributes_quoted_value_with_commas() {
        let m = parse_attributes("CODECS=\"avc1.4d401e,mp4a.40.2\",BANDWIDTH=500000");
        assert_eq!(m.get("CODECS").unwrap(), "avc1.4d401e,mp4a.40.2");
        assert_eq!(m.get("BANDWIDTH").unwrap(), "500000");
    }

    #[test]
    fn resolve_absolute_is_unchanged() {
        assert_eq!(resolve_uri("https://h/p/x.m3u8", "https://c/seg.ts"), "https://c/seg.ts");
    }

    #[test]
    fn resolve_relative_to_dir() {
        assert_eq!(resolve_uri("https://h/p/x.m3u8", "seg0.ts"), "https://h/p/seg0.ts");
    }

    #[test]
    fn resolve_root_relative() {
        assert_eq!(resolve_uri("https://h/p/x.m3u8", "/v/seg0.ts"), "https://h/v/seg0.ts");
    }
}
```

- [ ] **Step 4 : Vérifier l'échec puis la compilation/réussite**

Run: `cd src-tauri && cargo test -p download-core hls::playlist`
Expected: d'abord erreur de compilation si l'erreur `Media` n'est pas ajoutée ; une fois Steps 1-3 en place → 5 tests PASS.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/lib.rs src-tauri/crates/download-core/src/hls/
git commit -m "feat(core): hls playlist attribute + uri helpers, Media error"
```

---

## Task 2 : Parser HLS — master playlist

**Files:**
- Modify: `src-tauri/crates/download-core/src/hls/playlist.rs`

- [ ] **Step 1 : Écrire les tests du parseur master**

Ajouter en haut de `playlist.rs` (après les `use`) les structures, puis les tests. D'abord les types :

```rust
/// Une variante de qualité d'un master playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: u64,
    pub resolution: Option<(u32, u32)>,
    pub audio_group: Option<String>,
}

/// Une piste audio alternative (`#EXT-X-MEDIA:TYPE=AUDIO`).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRendition {
    pub group_id: String,
    pub name: String,
    pub uri: Option<String>,
    pub default: bool,
    pub autoselect: bool,
}

/// Master playlist : variantes + pistes audio alternatives.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MasterPlaylist {
    pub variants: Vec<Variant>,
    pub audio: Vec<AudioRendition>,
}
```

Ajouter dans le `mod tests` :

```rust
    const MASTER: &str = "#EXTM3U\n\
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",DEFAULT=YES,AUTOSELECT=YES,URI=\"audio/en.m3u8\"\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,AUDIO=\"aud\"\n\
v360/index.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=1280x720,AUDIO=\"aud\"\n\
v720/index.m3u8\n";

    #[test]
    fn is_master_detects_stream_inf() {
        assert!(is_master(MASTER));
        assert!(!is_master("#EXTM3U\n#EXTINF:6.0,\nseg0.ts\n"));
    }

    #[test]
    fn parses_master_variants_and_audio() {
        let m = parse_master(MASTER, "https://h/p/master.m3u8");
        assert_eq!(m.variants.len(), 2);
        assert_eq!(m.variants[0].bandwidth, 800000);
        assert_eq!(m.variants[0].resolution, Some((640, 360)));
        assert_eq!(m.variants[0].uri, "https://h/p/v360/index.m3u8");
        assert_eq!(m.variants[0].audio_group.as_deref(), Some("aud"));
        assert_eq!(m.variants[1].resolution, Some((1280, 720)));

        assert_eq!(m.audio.len(), 1);
        assert_eq!(m.audio[0].group_id, "aud");
        assert!(m.audio[0].default);
        assert_eq!(m.audio[0].uri.as_deref(), Some("https://h/p/audio/en.m3u8"));
    }
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cd src-tauri && cargo test -p download-core hls::playlist`
Expected: FAIL — `is_master`/`parse_master` introuvables.

- [ ] **Step 3 : Implémenter `is_master` et `parse_master`**

Ajouter dans `playlist.rs` (avant le `mod tests`) :

```rust
/// Vrai si le texte est un master playlist (contient au moins un `EXT-X-STREAM-INF`).
pub fn is_master(text: &str) -> bool {
    text.lines().any(|l| l.trim_start().starts_with("#EXT-X-STREAM-INF"))
}

/// Parse un master playlist. Les URIs sont résolues contre `base_url`.
pub fn parse_master(text: &str, base_url: &str) -> MasterPlaylist {
    let mut out = MasterPlaylist::default();
    let mut pending: Option<(u64, Option<(u32, u32)>, Option<String>)> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("#EXT-X-MEDIA:") {
            let attrs = parse_attributes(&line["#EXT-X-MEDIA:".len()..]);
            if attrs.get("TYPE").map(|t| t == "AUDIO").unwrap_or(false) {
                out.audio.push(AudioRendition {
                    group_id: attrs.get("GROUP-ID").cloned().unwrap_or_default(),
                    name: attrs.get("NAME").cloned().unwrap_or_default(),
                    uri: attrs.get("URI").map(|u| resolve_uri(base_url, u)),
                    default: attrs.get("DEFAULT").map(|v| v == "YES").unwrap_or(false),
                    autoselect: attrs.get("AUTOSELECT").map(|v| v == "YES").unwrap_or(false),
                });
            }
        } else if line.starts_with("#EXT-X-STREAM-INF:") {
            let attrs = parse_attributes(&line["#EXT-X-STREAM-INF:".len()..]);
            let bandwidth = attrs.get("BANDWIDTH").and_then(|v| v.parse().ok()).unwrap_or(0);
            let resolution = attrs.get("RESOLUTION").and_then(|r| {
                let (w, h) = r.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            });
            let audio_group = attrs.get("AUDIO").cloned();
            pending = Some((bandwidth, resolution, audio_group));
        } else if !line.is_empty() && !line.starts_with('#') {
            if let Some((bandwidth, resolution, audio_group)) = pending.take() {
                out.variants.push(Variant {
                    uri: resolve_uri(base_url, line),
                    bandwidth,
                    resolution,
                    audio_group,
                });
            }
        }
    }
    out
}
```

- [ ] **Step 4 : Vérifier la réussite**

Run: `cd src-tauri && cargo test -p download-core hls::playlist`
Expected: tous les tests PASS (helpers + master).

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/hls/playlist.rs
git commit -m "feat(core): parse HLS master playlist (variants + audio renditions)"
```

---

## Task 3 : Parser HLS — media playlist (segments, clé, map, byterange, live)

**Files:**
- Modify: `src-tauri/crates/download-core/src/hls/playlist.rs`

- [ ] **Step 1 : Écrire les tests du parseur media**

Ajouter les types (avant `mod tests`) :

```rust
/// Méthode de chiffrement d'un segment.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyMethod {
    Aes128,
    SampleAes,
    Other,
}

/// Clé de chiffrement courante appliquée à un segment.
#[derive(Debug, Clone, PartialEq)]
pub struct EncKey {
    pub method: KeyMethod,
    pub uri: Option<String>,
    pub iv: Option<[u8; 16]>,
}

/// Un segment média.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub uri: String,
    pub duration: f64,
    /// Plage d'octets absolue (start, end inclusif), résolue depuis EXT-X-BYTERANGE.
    pub byte_range: Option<(u64, u64)>,
    /// Clé applicable (None ⇒ segment en clair).
    pub key: Option<EncKey>,
}

/// Media playlist : segments + métadonnées.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaPlaylist {
    pub segments: Vec<Segment>,
    /// Segment d'initialisation fMP4 (#EXT-X-MAP URI), déjà résolu.
    pub map: Option<String>,
    /// `true` si la playlist est un live (pas de #EXT-X-ENDLIST).
    pub is_live: bool,
    pub target_duration: f64,
    pub media_sequence: u64,
}

impl MediaPlaylist {
    /// Durée totale (somme des EXTINF), en secondes.
    pub fn total_duration(&self) -> f64 {
        self.segments.iter().map(|s| s.duration).sum()
    }
}
```

Tests dans `mod tests` :

```rust
    const MEDIA: &str = "#EXTM3U\n\
#EXT-X-VERSION:3\n\
#EXT-X-TARGETDURATION:7\n\
#EXT-X-MEDIA-SEQUENCE:0\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n\
#EXTINF:6.0,\n\
seg0.ts\n\
#EXTINF:6.0,\n\
seg1.ts\n\
#EXT-X-ENDLIST\n";

    #[test]
    fn parses_media_segments_and_key() {
        let p = parse_media(MEDIA, "https://h/p/index.m3u8");
        assert!(!p.is_live);
        assert_eq!(p.target_duration, 7.0);
        assert_eq!(p.media_sequence, 0);
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.segments[0].uri, "https://h/p/seg0.ts");
        assert_eq!(p.segments[0].duration, 6.0);
        let key = p.segments[0].key.as_ref().unwrap();
        assert_eq!(key.method, KeyMethod::Aes128);
        assert_eq!(key.uri.as_deref(), Some("https://h/p/key.bin"));
        assert_eq!(p.total_duration(), 12.0);
    }

    #[test]
    fn live_when_no_endlist() {
        let live = "#EXTM3U\n#EXTINF:6.0,\nseg0.ts\n";
        assert!(parse_media(live, "https://h/p/i.m3u8").is_live);
    }

    #[test]
    fn parses_map_and_byterange() {
        let txt = "#EXTM3U\n\
#EXT-X-MAP:URI=\"init.mp4\"\n\
#EXT-X-BYTERANGE:1000@2000\n\
#EXTINF:4.0,\n\
seg.m4s\n\
#EXT-X-ENDLIST\n";
        let p = parse_media(txt, "https://h/p/i.m3u8");
        assert_eq!(p.map.as_deref(), Some("https://h/p/init.mp4"));
        assert_eq!(p.segments[0].byte_range, Some((2000, 2999)));
    }

    #[test]
    fn key_none_resets_encryption() {
        let txt = "#EXTM3U\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n\
#EXTINF:1.0,\na.ts\n\
#EXT-X-KEY:METHOD=NONE\n\
#EXTINF:1.0,\nb.ts\n\
#EXT-X-ENDLIST\n";
        let p = parse_media(txt, "https://h/p/i.m3u8");
        assert!(p.segments[0].key.is_some());
        assert!(p.segments[1].key.is_none());
    }
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cd src-tauri && cargo test -p download-core hls::playlist`
Expected: FAIL — `parse_media` introuvable.

- [ ] **Step 3 : Implémenter `parse_media`**

Ajouter (avant `mod tests`) :

```rust
/// Parse un media playlist. URIs résolues contre `base_url`. La clé courante
/// (#EXT-X-KEY) s'applique aux segments suivants jusqu'à un nouveau #EXT-X-KEY.
pub fn parse_media(text: &str, base_url: &str) -> MediaPlaylist {
    let mut out = MediaPlaylist { is_live: true, ..Default::default() };
    let mut cur_key: Option<EncKey> = None;
    let mut next_duration = 0.0f64;
    let mut next_range: Option<(u64, u64)> = None;
    let mut running_offset: u64 = 0;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("#EXT-X-TARGETDURATION:") {
            out.target_duration = line["#EXT-X-TARGETDURATION:".len()..].trim().parse().unwrap_or(0.0);
        } else if line.starts_with("#EXT-X-MEDIA-SEQUENCE:") {
            out.media_sequence = line["#EXT-X-MEDIA-SEQUENCE:".len()..].trim().parse().unwrap_or(0);
        } else if line.starts_with("#EXT-X-ENDLIST") {
            out.is_live = false;
        } else if line.starts_with("#EXT-X-MAP:") {
            let attrs = parse_attributes(&line["#EXT-X-MAP:".len()..]);
            out.map = attrs.get("URI").map(|u| resolve_uri(base_url, u));
        } else if line.starts_with("#EXT-X-KEY:") {
            let attrs = parse_attributes(&line["#EXT-X-KEY:".len()..]);
            let method = match attrs.get("METHOD").map(|s| s.as_str()) {
                Some("NONE") => { cur_key = None; continue; }
                Some("AES-128") => KeyMethod::Aes128,
                Some("SAMPLE-AES") => KeyMethod::SampleAes,
                _ => KeyMethod::Other,
            };
            let iv = attrs.get("IV").and_then(|s| parse_hex_iv(s));
            cur_key = Some(EncKey {
                method,
                uri: attrs.get("URI").map(|u| resolve_uri(base_url, u)),
                iv,
            });
        } else if line.starts_with("#EXTINF:") {
            let v = &line["#EXTINF:".len()..];
            let num = v.split(',').next().unwrap_or("0");
            next_duration = num.trim().parse().unwrap_or(0.0);
        } else if line.starts_with("#EXT-X-BYTERANGE:") {
            let v = line["#EXT-X-BYTERANGE:".len()..].trim();
            let (len, offset) = match v.split_once('@') {
                Some((l, o)) => (l.parse::<u64>().unwrap_or(0), o.parse::<u64>().unwrap_or(running_offset)),
                None => (v.parse::<u64>().unwrap_or(0), running_offset),
            };
            next_range = Some((offset, offset + len.saturating_sub(1)));
            running_offset = offset + len;
        } else if !line.is_empty() && !line.starts_with('#') {
            out.segments.push(Segment {
                uri: resolve_uri(base_url, line),
                duration: next_duration,
                byte_range: next_range.take(),
                key: cur_key.clone(),
            });
            next_duration = 0.0;
        }
    }
    out
}

/// Parse un IV hexadécimal `0x...` (32 chiffres hex) en 16 octets.
fn parse_hex_iv(s: &str) -> Option<[u8; 16]> {
    let h = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    if h.len() != 32 {
        return None;
    }
    let mut iv = [0u8; 16];
    for i in 0..16 {
        iv[i] = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(iv)
}
```

- [ ] **Step 4 : Vérifier la réussite**

Run: `cd src-tauri && cargo test -p download-core hls::playlist`
Expected: tous les tests PASS.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/src/hls/playlist.rs
git commit -m "feat(core): parse HLS media playlist (segments, key, map, byterange, live)"
```

---

## Task 4 : Déchiffrement AES-128-CBC (`hls/crypto.rs`)

**Files:**
- Modify: `src-tauri/crates/download-core/Cargo.toml` (deps `aes`, `cbc`)
- Create: `src-tauri/crates/download-core/src/hls/crypto.rs`
- Modify: `src-tauri/crates/download-core/src/hls/mod.rs` (déclarer `pub mod crypto;`)

- [ ] **Step 1 : Ajouter les dépendances aes/cbc**

Dans `src-tauri/crates/download-core/Cargo.toml`, sous `[dependencies]` (après `rusqlite`) :

```toml
aes = "0.8"
cbc = { version = "0.1", features = ["alloc"] }
```

- [ ] **Step 2 : Déclarer le module crypto**

Dans `src-tauri/crates/download-core/src/hls/mod.rs`, ajouter :

```rust
pub mod crypto;
```

- [ ] **Step 3 : Écrire les tests crypto**

Créer `src-tauri/crates/download-core/src/hls/crypto.rs` :

```rust
use crate::CoreError;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Déchiffre des données AES-128-CBC (padding PKCS7).
pub fn decrypt_aes128_cbc(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Result<Vec<u8>, CoreError> {
    Aes128CbcDec::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| CoreError::Media(format!("échec déchiffrement AES-128: {e}")))
}

/// IV d'un segment : IV explicite s'il existe, sinon le numéro de séquence média
/// codé en big-endian sur 128 bits.
pub fn iv_for_segment(explicit: Option<[u8; 16]>, media_sequence: u64) -> [u8; 16] {
    if let Some(iv) = explicit {
        return iv;
    }
    let mut iv = [0u8; 16];
    iv[8..16].copy_from_slice(&media_sequence.to_be_bytes());
    iv
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    #[test]
    fn decrypt_roundtrips_encrypt() {
        let key = [0x42u8; 16];
        let iv = [0x01u8; 16];
        let plaintext = b"hello hls segment payload!!".to_vec();
        let ct = Aes128CbcEnc::new((&key).into(), (&iv).into())
            .encrypt_padded_vec_mut::<Pkcs7>(&plaintext);
        let pt = decrypt_aes128_cbc(&ct, &key, &iv).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn iv_explicit_is_returned() {
        let iv = [0xABu8; 16];
        assert_eq!(iv_for_segment(Some(iv), 5), iv);
    }

    #[test]
    fn iv_from_sequence_is_big_endian() {
        let iv = iv_for_segment(None, 1);
        let mut expected = [0u8; 16];
        expected[15] = 1;
        assert_eq!(iv, expected);
    }
}
```

- [ ] **Step 4 : Vérifier la réussite**

Run: `cd src-tauri && cargo test -p download-core hls::crypto`
Expected: 3 tests PASS (peut télécharger/compiler aes+cbc au 1er run).

- [ ] **Step 5 : Commit**

```bash
git add src-tauri/crates/download-core/Cargo.toml src-tauri/crates/download-core/src/hls/
git commit -m "feat(core): AES-128-CBC segment decryption + IV derivation"
```

---

## Task 5 : Sélection de variante & estimation (`hls/select.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/hls/select.rs`
- Modify: `src-tauri/crates/download-core/src/hls/mod.rs` (déclarer `pub mod select;`)

- [ ] **Step 1 : Déclarer le module**

Dans `hls/mod.rs`, ajouter :

```rust
pub mod select;
```

- [ ] **Step 2 : Écrire les tests de sélection**

Créer `src-tauri/crates/download-core/src/hls/select.rs` :

```rust
use crate::hls::playlist::{AudioRendition, MasterPlaylist, Variant};

/// Index de la meilleure variante (plus haut BANDWIDTH). 0 si vide.
pub fn best_variant(m: &MasterPlaylist) -> usize {
    m.variants
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v.bandwidth)
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Taille estimée en octets : bitrate (bits/s) × durée (s) ÷ 8.
pub fn estimated_size(bandwidth: u64, total_duration_secs: f64) -> u64 {
    ((bandwidth as f64) * total_duration_secs / 8.0) as u64
}

/// Piste audio à utiliser pour une variante : la rendition DEFAULT (sinon AUTOSELECT,
/// sinon la première) du groupe audio de la variante, à condition qu'elle ait une URI.
pub fn pick_audio<'a>(m: &'a MasterPlaylist, variant: &Variant) -> Option<&'a AudioRendition> {
    let group = variant.audio_group.as_ref()?;
    let in_group: Vec<&AudioRendition> =
        m.audio.iter().filter(|a| &a.group_id == group && a.uri.is_some()).collect();
    in_group
        .iter()
        .find(|a| a.default)
        .or_else(|| in_group.iter().find(|a| a.autoselect))
        .or_else(|| in_group.first())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::playlist::{AudioRendition, MasterPlaylist, Variant};

    fn variant(bw: u64, group: Option<&str>) -> Variant {
        Variant { uri: "u".into(), bandwidth: bw, resolution: None, audio_group: group.map(String::from) }
    }

    #[test]
    fn best_variant_picks_highest_bandwidth() {
        let m = MasterPlaylist {
            variants: vec![variant(800_000, None), variant(2_400_000, None), variant(1_200_000, None)],
            audio: vec![],
        };
        assert_eq!(best_variant(&m), 1);
    }

    #[test]
    fn estimated_size_uses_bitrate_times_duration() {
        // 8_000_000 bits/s × 10 s ÷ 8 = 10_000_000 octets
        assert_eq!(estimated_size(8_000_000, 10.0), 10_000_000);
    }

    #[test]
    fn pick_audio_prefers_default() {
        let m = MasterPlaylist {
            variants: vec![],
            audio: vec![
                AudioRendition { group_id: "aud".into(), name: "FR".into(), uri: Some("fr.m3u8".into()), default: false, autoselect: true },
                AudioRendition { group_id: "aud".into(), name: "EN".into(), uri: Some("en.m3u8".into()), default: true, autoselect: true },
            ],
        };
        let v = variant(1, Some("aud"));
        assert_eq!(pick_audio(&m, &v).unwrap().name, "EN");
    }

    #[test]
    fn pick_audio_none_when_no_group() {
        let m = MasterPlaylist::default();
        assert!(pick_audio(&m, &variant(1, None)).is_none());
    }
}
```

- [ ] **Step 3 : Vérifier la réussite**

Run: `cd src-tauri && cargo test -p download-core hls::select`
Expected: 4 tests PASS.

- [ ] **Step 4 : Commit**

```bash
git add src-tauri/crates/download-core/src/hls/
git commit -m "feat(core): HLS variant selection + size estimation"
```

---

## Task 6 : Trait `Remuxer` + `FakeRemuxer` (`hls/remux.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/hls/remux.rs`
- Modify: `src-tauri/crates/download-core/src/hls/mod.rs` (déclarer `pub mod remux;`)

- [ ] **Step 1 : Déclarer le module**

Dans `hls/mod.rs`, ajouter :

```rust
pub mod remux;
```

- [ ] **Step 2 : Écrire le trait + le fake + un test**

Créer `src-tauri/crates/download-core/src/hls/remux.rs` :

```rust
use crate::CoreError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Fichiers d'entrée du remux : déjà concaténés par le moteur (un par piste).
#[derive(Debug, Clone, PartialEq)]
pub struct RemuxInputs {
    pub video: PathBuf,
    pub audio: Option<PathBuf>,
}

/// Assemble les pistes en un conteneur de sortie (sans ré-encodage).
#[async_trait]
pub trait Remuxer: Send + Sync {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError>;
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    /// Remuxer de test : écrit un fichier de sortie factice et mémorise les inputs.
    #[derive(Default)]
    pub struct FakeRemuxer {
        pub calls: Mutex<Vec<RemuxInputs>>,
    }

    #[async_trait]
    impl Remuxer for FakeRemuxer {
        async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
            // Concatène les octets des entrées pour simuler un conteneur.
            let mut buf = std::fs::read(&inputs.video)?;
            if let Some(a) = &inputs.audio {
                buf.extend_from_slice(&std::fs::read(a)?);
            }
            std::fs::write(output, buf)?;
            self.calls.lock().unwrap().push(inputs);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeRemuxer;
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fake_remuxer_writes_output_and_records_call() {
        let dir = tempdir().unwrap();
        let v = dir.path().join("video.cat");
        std::fs::write(&v, b"VIDEO").unwrap();
        let out = dir.path().join("out.mp4");
        let r = FakeRemuxer::default();
        r.remux(RemuxInputs { video: v.clone(), audio: None }, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"VIDEO");
        assert_eq!(r.calls.lock().unwrap().len(), 1);
    }
}
```

- [ ] **Step 3 : Vérifier la réussite**

Run: `cd src-tauri && cargo test -p download-core hls::remux`
Expected: 1 test PASS.

- [ ] **Step 4 : Commit**

```bash
git add src-tauri/crates/download-core/src/hls/
git commit -m "feat(core): Remuxer trait + FakeRemuxer for tests"
```

---

## Task 7 : Moteur HLS `download_hls` (`hls/engine.rs`)

**Files:**
- Create: `src-tauri/crates/download-core/src/hls/engine.rs`
- Modify: `src-tauri/crates/download-core/src/hls/mod.rs` (déclarer `pub mod engine;`)
- Modify: `src-tauri/crates/download-core/src/model.rs` (champs segments dans `Progress`)

> Ce moteur réutilise `DownloadHandle`/`Outcome` de `engine.rs`, `ByteFetcher`, `RateLimiter`, et le trait `Remuxer`. Il télécharge chaque segment **en mémoire** (nécessaire pour le déchiffrement CBC), déchiffre si besoin, écrit `vNNNNN.part`→rename `vNNNNN.seg`, concatène en `video.cat` (+ init fMP4), idem audio en `audio.cat`, puis appelle le remux vers `dest`. Reprise : saute les `*.seg` déjà présents.

- [ ] **Step 1 : Ajouter les champs segments à `Progress`**

Dans `src-tauri/crates/download-core/src/model.rs`, modifier la struct `Progress` :

```rust
/// Payload de progression poussé vers l'UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: u64,
    pub status: DownloadStatus,
    #[serde(default)]
    pub segments_done: Option<u32>,
    #[serde(default)]
    pub segments_total: Option<u32>,
}
```

Puis corriger les constructions existantes de `Progress` qui n'ont pas ces champs :
- `src-tauri/crates/download-core/src/engine.rs`, fn `emit` : ajouter `segments_done: None, segments_total: None,` dans le `Progress { ... }`.
- `src-tauri/src/commands.rs`, fn `launch` (bloc d'erreur, `Progress { ... }`) : ajouter `segments_done: None, segments_total: None,`.

- [ ] **Step 2 : Déclarer le module engine**

Dans `hls/mod.rs`, ajouter :

```rust
pub mod engine;
```

- [ ] **Step 3 : Écrire le test du moteur (clair, multi-segments)**

Créer `src-tauri/crates/download-core/src/hls/engine.rs` avec d'abord l'en-tête et un test contre des fakes. Le test utilise un `FakeFetcher` qui sert le texte de playlist puis les segments selon l'URL.

```rust
use crate::hls::crypto::{decrypt_aes128_cbc, iv_for_segment};
use crate::hls::playlist::{parse_media, KeyMethod};
use crate::hls::remux::{RemuxInputs, Remuxer};
use crate::http::ByteFetcher;
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::engine::{DownloadHandle, Outcome};
use crate::CoreError;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Répertoire de travail temporaire associé à un fichier de destination.
fn work_dir(dest: &Path) -> std::path::PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".fghls");
    std::path::PathBuf::from(s)
}

/// Lit entièrement un flux d'octets (rate-limité, respecte pause/cancel, cumule la progression).
async fn fetch_to_vec<F: ByteFetcher>(
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
        if handle_cancelled(handle) {
            return Ok(None);
        }
        let chunk = chunk?;
        limiter.acquire(chunk.len() as u64).await;
        buf.extend_from_slice(&chunk);
        handle.add_downloaded(chunk.len() as u64);
    }
    Ok(Some(buf))
}

fn handle_cancelled(handle: &DownloadHandle) -> bool {
    handle.is_cancelled()
}

fn emit_segments(
    tx: &mpsc::UnboundedSender<Progress>,
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

/// Télécharge un flux HLS VOD vers `dest` (.mp4) via le moteur Rust + remux ffmpeg.
#[allow(clippy::too_many_arguments)]
pub async fn download_hls<F: ByteFetcher, R: Remuxer>(
    fetcher: &F,
    remux: &R,
    id: &str,
    media_url: &str,
    audio_url: Option<&str>,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    // 1. Récupère + parse la media playlist vidéo.
    let video_pl_bytes = match fetch_to_vec(fetcher, media_url, None, headers, &limiter, &handle).await? {
        Some(b) => b,
        None => return finish_cancelled(&work),
    };
    let video_pl = parse_media(&String::from_utf8_lossy(&video_pl_bytes), media_url);
    if video_pl.is_live {
        return Err(CoreError::Media("flux live non supporté (à venir)".into()));
    }
    // Refuse tout chiffrement non géré.
    for seg in &video_pl.segments {
        if let Some(k) = &seg.key {
            if k.method != KeyMethod::Aes128 {
                return Err(CoreError::Media("chiffrement non supporté (DRM/SAMPLE-AES)".into()));
            }
        }
    }

    // 2. (Optionnel) media playlist audio.
    let audio_pl = if let Some(au) = audio_url {
        let bytes = match fetch_to_vec(fetcher, au, None, headers, &limiter, &handle).await? {
            Some(b) => b,
            None => return finish_cancelled(&work),
        };
        Some(parse_media(&String::from_utf8_lossy(&bytes), au))
    } else {
        None
    };

    let total_segments = (video_pl.segments.len()
        + audio_pl.as_ref().map(|p| p.segments.len()).unwrap_or(0)) as u32;
    let mut done: u32 = 0;
    let mut key_cache: HashMap<String, [u8; 16]> = HashMap::new();

    // 3. Télécharge la piste vidéo (+ init fMP4).
    if let Some(map_uri) = &video_pl.map {
        let init = match fetch_to_vec(fetcher, map_uri, None, headers, &limiter, &handle).await? {
            Some(b) => b,
            None => return finish_cancelled(&work),
        };
        std::fs::write(work.join("v_init.seg"), init)?;
    }
    match download_track(
        fetcher, &video_pl, "v", &work, headers, &limiter, &handle, &progress, id,
        &mut done, total_segments, &mut key_cache,
    )
    .await?
    {
        Outcome::Completed => {}
        Outcome::Paused => {
            emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused);
            return Ok(Outcome::Paused); // work dir conservé pour la reprise
        }
        Outcome::Cancelled => return finish_cancelled(&work),
    }

    // 4. Télécharge la piste audio si présente.
    if let Some(ap) = &audio_pl {
        if let Some(map_uri) = &ap.map {
            let init = match fetch_to_vec(fetcher, map_uri, None, headers, &limiter, &handle).await? {
                Some(b) => b,
                None => return finish_cancelled(&work),
            };
            std::fs::write(work.join("a_init.seg"), init)?;
        }
        match download_track(
            fetcher, ap, "a", &work, headers, &limiter, &handle, &progress, id,
            &mut done, total_segments, &mut key_cache,
        )
        .await?
        {
            Outcome::Completed => {}
            Outcome::Paused => {
                emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused);
                return Ok(Outcome::Paused);
            }
            Outcome::Cancelled => return finish_cancelled(&work),
        }
    }

    // 5. Concatène chaque piste en un fichier unique, puis remux.
    let video_cat = concat_track(&work, "v", video_pl.map.is_some())?;
    let audio_cat = if let Some(ap) = &audio_pl {
        Some(concat_track(&work, "a", ap.map.is_some())?)
    } else {
        None
    };
    remux
        .remux(RemuxInputs { video: video_cat, audio: audio_cat }, dest)
        .await?;

    // 6. Nettoyage + progression finale.
    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: None,
        speed_bps: 0,
        status: DownloadStatus::Completed,
        segments_done: Some(total_segments),
        segments_total: Some(total_segments),
    });
    Ok(Outcome::Completed)
}

/// Télécharge tous les segments d'une piste vers `work/<prefix>NNNNN.seg`.
#[allow(clippy::too_many_arguments)]
async fn download_track<F: ByteFetcher>(
    fetcher: &F,
    pl: &crate::hls::playlist::MediaPlaylist,
    prefix: &str,
    work: &Path,
    headers: &RequestHeaders,
    limiter: &Arc<RateLimiter>,
    handle: &DownloadHandle,
    progress: &mpsc::UnboundedSender<Progress>,
    id: &str,
    done: &mut u32,
    total: u32,
    key_cache: &mut HashMap<String, [u8; 16]>,
) -> Result<Outcome, CoreError> {
    for (idx, seg) in pl.segments.iter().enumerate() {
        let seg_path = work.join(format!("{prefix}{idx:05}.seg"));
        if seg_path.exists() {
            *done += 1; // reprise : segment déjà fait
            continue;
        }
        if handle.is_cancelled() {
            return Ok(Outcome::Cancelled);
        }
        if handle.is_paused() {
            return Ok(Outcome::Paused);
        }
        let raw = match fetch_to_vec(fetcher, &seg.uri, seg.byte_range, headers, limiter, handle).await? {
            Some(b) => b,
            None => return Ok(Outcome::Cancelled),
        };
        let bytes = if let Some(k) = &seg.key {
            let uri = k.uri.as_ref().ok_or_else(|| CoreError::Media("clé AES-128 sans URI".into()))?;
            let key = match key_cache.get(uri) {
                Some(k) => *k,
                None => {
                    let kb = match fetch_to_vec(fetcher, uri, None, headers, limiter, handle).await? {
                        Some(b) => b,
                        None => return Ok(Outcome::Cancelled),
                    };
                    if kb.len() != 16 {
                        return Err(CoreError::Media("clé AES-128 de taille invalide".into()));
                    }
                    let mut arr = [0u8; 16];
                    arr.copy_from_slice(&kb);
                    key_cache.insert(uri.clone(), arr);
                    arr
                }
            };
            let iv = iv_for_segment(k.iv, pl.media_sequence + idx as u64);
            decrypt_aes128_cbc(&raw, &key, &iv)?
        } else {
            raw
        };
        let part = work.join(format!("{prefix}{idx:05}.part"));
        std::fs::write(&part, &bytes)?;
        std::fs::rename(&part, &seg_path)?; // rename atomique
        *done += 1;
        emit_segments(progress, id, handle, *done, total, DownloadStatus::Downloading);
    }
    Ok(Outcome::Completed)
}

/// Concatène `<prefix>_init.seg` (si fMP4) + tous les `<prefix>NNNNN.seg` triés en un fichier.
fn concat_track(work: &Path, prefix: &str, has_init: bool) -> Result<std::path::PathBuf, CoreError> {
    use std::io::Write;
    let out_path = work.join(format!("{prefix}.cat"));
    let mut out = std::fs::File::create(&out_path)?;
    if has_init {
        let init = work.join(format!("{prefix}_init.seg"));
        if init.exists() {
            out.write_all(&std::fs::read(init)?)?;
        }
    }
    let mut segs: Vec<std::path::PathBuf> = std::fs::read_dir(work)?
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

fn finish_cancelled(work: &Path) -> Result<Outcome, CoreError> {
    let _ = std::fs::remove_dir_all(work);
    Ok(Outcome::Cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::http::ByteFetcher;
    use crate::model::ProbeResult;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use std::collections::HashMap as Map;
    use tempfile::tempdir;

    /// Fetcher qui mappe URL → octets (playlists, segments, clés).
    struct MapFetcher {
        routes: Map<String, Vec<u8>>,
    }
    #[async_trait]
    impl ByteFetcher for MapFetcher {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
        }
        async fn fetch_range(
            &self,
            url: &str,
            range: Option<(u64, u64)>,
            _h: &RequestHeaders,
        ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = self.routes.get(url).cloned().unwrap_or_default();
            let slice = match range {
                Some((s, e)) => data[s as usize..(e as usize + 1).min(data.len())].to_vec(),
                None => data,
            };
            Ok(stream::iter(vec![Ok(Bytes::from(slice))]).boxed())
        }
    }

    fn unlimited() -> Arc<RateLimiter> {
        Arc::new(RateLimiter::unlimited())
    }

    #[tokio::test]
    async fn downloads_clear_vod_and_remuxes() {
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXTINF:1.0,\nseg0.ts\n#EXTINF:1.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
        let mut routes = Map::new();
        routes.insert("https://h/p/index.m3u8".to_string(), media.as_bytes().to_vec());
        routes.insert("https://h/p/seg0.ts".to_string(), b"AAAA".to_vec());
        routes.insert("https://h/p/seg1.ts".to_string(), b"BBBB".to_vec());
        let fetcher = MapFetcher { routes };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();

        let outcome = download_hls(
            &fetcher, &remux, "id1", "https://h/p/index.m3u8", None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
        )
        .await
        .unwrap();

        assert_eq!(outcome, Outcome::Completed);
        // FakeRemuxer concatène les inputs : video.cat = "AAAABBBB".
        assert_eq!(std::fs::read(&dest).unwrap(), b"AAAABBBB");
        // Le work dir temporaire doit être nettoyé après remux.
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists(), "le work dir doit être nettoyé");
    }

    #[tokio::test]
    async fn rejects_live_stream() {
        let media = "#EXTM3U\n#EXTINF:1.0,\nseg0.ts\n"; // pas d'ENDLIST
        let mut routes = Map::new();
        routes.insert("https://h/p/index.m3u8".to_string(), media.as_bytes().to_vec());
        let fetcher = MapFetcher { routes };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();

        let err = download_hls(
            &fetcher, &remux, "id2", "https://h/p/index.m3u8", None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }
}
```

- [ ] **Step 4 : Ajouter les accesseurs manquants à `DownloadHandle`**

Le moteur HLS utilise `handle.is_cancelled()`, `handle.is_paused()`, `handle.add_downloaded(n)`. Les champs de `DownloadHandle` sont privés ; ajouter ces méthodes dans `src-tauri/crates/download-core/src/engine.rs`, dans `impl DownloadHandle` (après `downloaded()`) :

```rust
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::SeqCst)
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn add_downloaded(&self, n: u64) {
        self.downloaded.fetch_add(n, Ordering::SeqCst);
    }
```

- [ ] **Step 5 : Vérifier l'échec puis la réussite**

Run: `cd src-tauri && cargo test -p download-core hls::engine`
Expected: après Steps 1-4, 2 tests PASS. Lancer aussi la suite complète du cœur : `cargo test -p download-core` (tous PASS, y compris `engine` et `storage` mis à jour pour les champs `Progress`).

- [ ] **Step 6 : Commit**

```bash
git add src-tauri/crates/download-core/src/
git commit -m "feat(core): HLS download engine (segments, decrypt, concat, remux, live guard)"
```

---

## Task 8 : Modèle & storage — `DownloadKind` + `audio_url` + nom `.mp4`

**Files:**
- Modify: `src-tauri/crates/download-core/src/model.rs`
- Modify: `src-tauri/crates/download-core/src/storage.rs`
- Modify: `src-tauri/crates/download-core/src/naming.rs`

- [ ] **Step 1 : Écrire les tests (kind/audio_url roundtrip + with_extension)**

Dans `storage.rs`, le helper de test `sample(...)` devra fournir les nouveaux champs ; on l'ajuste à l'étape d'implémentation. Ajouter d'abord ces tests dans `storage.rs` `mod tests` :

```rust
    #[test]
    fn round_trips_kind_and_audio_url() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("k1");
        d.kind = DownloadKind::Hls;
        d.audio_url = Some("https://h/a.m3u8".into());
        s.upsert(&d).unwrap();
        let got = s.get_by_id("k1").unwrap().unwrap();
        assert_eq!(got.kind, DownloadKind::Hls);
        assert_eq!(got.audio_url.as_deref(), Some("https://h/a.m3u8"));
        // Défaut : Regular + None.
        let plain = sample("k2");
        s.upsert(&plain).unwrap();
        let got2 = s.get_by_id("k2").unwrap().unwrap();
        assert_eq!(got2.kind, DownloadKind::Regular);
        assert_eq!(got2.audio_url, None);
    }
```

Dans `naming.rs`, ajouter dans `mod tests` :

```rust
    #[test]
    fn with_extension_forces_mp4() {
        assert_eq!(with_extension("video", "mp4"), "video.mp4");
        assert_eq!(with_extension("clip.ts", "mp4"), "clip.mp4");
        assert_eq!(with_extension("a.b.m3u8", "mp4"), "a.b.mp4");
    }
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cd src-tauri && cargo test -p download-core`
Expected: FAIL de compilation — `DownloadKind`, champs `kind`/`audio_url`, `with_extension` absents.

- [ ] **Step 3 : Ajouter `DownloadKind` + champs au modèle**

Dans `model.rs`, ajouter l'enum (après `Category`) :

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DownloadKind {
    #[default]
    Regular,
    Hls,
}
```

Et dans `struct Download`, ajouter après `user_agent` :

```rust
    #[serde(default)]
    pub kind: DownloadKind,
    #[serde(default)]
    pub audio_url: Option<String>,
```

Importer `DownloadKind` là où nécessaire (déjà ré-exporté par le module `model`).

- [ ] **Step 4 : Migration + lecture/écriture storage**

Dans `storage.rs` :

1. Importer `DownloadKind` : modifier le `use crate::model::{...}` pour inclure `DownloadKind`.
2. Dans `from_conn`, après les migrations en-têtes, ajouter :

```rust
        // Migration additive Phase 4a : type de download + URL audio HLS.
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN kind TEXT NOT NULL DEFAULT 'regular'",
            [],
        );
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN audio_url TEXT", []);
```

3. `upsert` : étendre l'INSERT. Remplacer la requête et les params pour inclure `kind`, `audio_url` (colonnes 15-16) :

```rust
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent, kind, audio_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9, priority=?10, category=?11,
                referrer=?12, cookie=?13, user_agent=?14, kind=?15, audio_url=?16",
            rusqlite::params![
                d.id, d.url, d.file_name, d.dest_path,
                d.total_bytes.map(|v| v as i64),
                d.downloaded_bytes as i64,
                status_to_str(d.status),
                d.supports_ranges as i64,
                d.error,
                priority_to_str(d.priority),
                category_to_str(d.category),
                d.referrer,
                d.cookie,
                d.user_agent,
                kind_to_str(d.kind),
                d.audio_url,
            ],
```

4. `list` et `get_by_id` : ajouter `kind, audio_url` à la fin du `SELECT` (colonnes 14-15, indices 14 et 15) et au mapping `Download { ... }` :

```rust
                    kind: kind_from_str(&row.get::<_, String>(14)?),
                    audio_url: row.get(15)?,
```

(Mettre à jour la chaîne SELECT des deux fonctions : `... user_agent, kind, audio_url FROM downloads ...`.)

5. Ajouter les helpers de conversion (près de `category_from_str`) :

```rust
fn kind_to_str(k: DownloadKind) -> &'static str {
    match k {
        DownloadKind::Regular => "regular",
        DownloadKind::Hls => "hls",
    }
}

fn kind_from_str(s: &str) -> DownloadKind {
    match s {
        "hls" => DownloadKind::Hls,
        _ => DownloadKind::Regular,
    }
}
```

6. Mettre à jour le helper de test `sample(...)` pour inclure `kind: DownloadKind::Regular, audio_url: None,`.

- [ ] **Step 5 : Implémenter `with_extension`**

Dans `naming.rs`, ajouter :

```rust
/// Force l'extension d'un nom de fichier (remplace l'existante, ou l'ajoute).
pub fn with_extension(name: &str, ext: &str) -> String {
    let base = match name.rfind('.') {
        Some(p) if p > 0 => &name[..p],
        _ => name,
    };
    format!("{base}.{ext}")
}
```

- [ ] **Step 6 : Vérifier la réussite**

Run: `cd src-tauri && cargo test -p download-core`
Expected: tous PASS (storage + naming + reste).

- [ ] **Step 7 : Mettre à jour les types frontend (mirror)**

Dans `src/types.ts`, étendre :

```ts
export type DownloadKind = "regular" | "hls";
```

Ajouter à `interface Download` :

```ts
  kind?: DownloadKind;
  audio_url?: string | null;
```

Ajouter à `interface Progress` :

```ts
  segments_done?: number | null;
  segments_total?: number | null;
```

- [ ] **Step 8 : Commit**

```bash
git add src-tauri/crates/download-core/src/ src/types.ts
git commit -m "feat(core): DownloadKind + audio_url persistence, with_extension, Progress segment fields"
```

---

## Task 9 : ffmpeg sidecar + `SidecarRemuxer` (src-tauri)

**Files:**
- Modify: `src-tauri/Cargo.toml` (dep `tauri-plugin-shell`)
- Modify: `src-tauri/tauri.conf.json` (`bundle.externalBin`)
- Modify: `src-tauri/capabilities/default.json` (permission shell sidecar)
- Modify: `src-tauri/src/lib.rs` (init plugin shell)
- Create: `src-tauri/src/remux.rs` (`SidecarRemuxer`)
- Create: `src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe` (binaire, manuel)

> ⚠️ Le binaire ffmpeg ne peut pas être créé par le code. Étape manuelle documentée ci-dessous.

- [ ] **Step 1 : Placer le binaire ffmpeg (manuel)**

Télécharger un build statique **LGPL** Windows de ffmpeg (ex. gyan.dev « essentials » ou BtbN), extraire `ffmpeg.exe`, et le copier en :
`src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe`

Vérifier la cible exacte attendue par Tauri :

Run: `cd src-tauri && rustc -Vv | findstr host`
Expected: `host: x86_64-pc-windows-msvc` (sinon renommer le binaire avec le triplet affiché).

- [ ] **Step 2 : Déclarer le sidecar dans tauri.conf.json**

Dans `src-tauri/tauri.conf.json`, dans l'objet `"bundle"`, ajouter la clé `externalBin` :

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "externalBin": ["bin/ffmpeg"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
```

- [ ] **Step 3 : Ajouter le plugin shell (Cargo + init + capability)**

Dans `src-tauri/Cargo.toml`, sous `[dependencies]`, ajouter :

```toml
tauri-plugin-shell = "2"
```

Dans `src-tauri/src/lib.rs`, ajouter le plugin dans le builder (après `tauri_plugin_dialog::init()`) :

```rust
        .plugin(tauri_plugin_shell::init())
```

Dans `src-tauri/capabilities/default.json`, ajouter au tableau `permissions` la permission d'exécuter le sidecar :

```json
    {
      "identifier": "shell:allow-execute",
      "allow": [{ "name": "bin/ffmpeg", "sidecar": true, "args": true }]
    }
```

> `"args": true` autorise n'importe quels arguments passés au sidecar (nos options ffmpeg). Le schéma exact de cette permission peut varier selon la version de `tauri-plugin-shell` ; vérifier au `cargo build`/runtime et ajuster (`shell:default` + scope) si l'exécution est refusée.

- [ ] **Step 4 : Implémenter `SidecarRemuxer`**

Créer `src-tauri/src/remux.rs` :

```rust
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::CoreError;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

/// Remuxer de production : appelle le binaire ffmpeg embarqué (sidecar Tauri).
/// Ne fait que le remux conteneur (`-c copy`) — la concaténation est faite côté cœur.
pub struct SidecarRemuxer {
    app: AppHandle,
}

impl SidecarRemuxer {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait::async_trait]
impl Remuxer for SidecarRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into()];
        args.push("-i".into());
        args.push(inputs.video.to_string_lossy().to_string());
        if let Some(audio) = &inputs.audio {
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
}
```

Déclarer le module dans `src-tauri/src/lib.rs` (en haut, avec les autres `mod`) :

```rust
mod remux;
```

- [ ] **Step 5 : Vérifier la compilation**

Run: `cd src-tauri && cargo build`
Expected: compile sans erreur (async-trait est déjà transitif ; sinon ajouter `async-trait = "0.1"` aux deps de `src-tauri`).

- [ ] **Step 6 : Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/src/lib.rs src-tauri/src/remux.rs
git commit -m "feat(tauri): embed ffmpeg sidecar + SidecarRemuxer (container remux)"
```

> Note : le binaire `bin/ffmpeg-*.exe` est volumineux (~80 Mo selon le build). Vérifier la politique git du repo (ajouter à `.gitignore` + distribuer hors VCS, ou committer si accepté). Décider à l'exécution.

---

## Task 10 : Backend — `probe_hls`, registre `pending_media`, `add_hls_download`, branche `launch`

**Files:**
- Modify: `src-tauri/src/state.rs` (champ `pending_media`)
- Modify: `src-tauri/src/commands.rs` (probe_hls, add_hls_download, types, branche launch)
- Modify: `src-tauri/src/lib.rs` (enregistrer les 2 commandes)

- [ ] **Step 1 : Ajouter le registre `pending_media` à l'état**

Dans `src-tauri/src/state.rs` :

1. Ajouter les imports :

```rust
use download_core::model::RequestHeaders;
use download_core::hls::playlist::MasterPlaylist;
```

2. Définir la structure de pending media (en bas du fichier) :

```rust
/// Probe HLS en attente d'un choix de variante par l'utilisateur.
pub struct PendingMedia {
    pub master: MasterPlaylist,
    pub headers: RequestHeaders,
    pub file_name: String,
    /// Durée totale (s) d'une variante de référence (pour estimer la taille).
    pub total_duration: f64,
}
```

3. Ajouter le champ à `AppState` :

```rust
    pub pending_media: Mutex<HashMap<String, PendingMedia>>,
```

4. L'initialiser dans `AppState::new` :

```rust
            pending_media: Mutex::new(HashMap::new()),
```

- [ ] **Step 2 : Définir les types de variante exposés + commande `probe_hls`**

Dans `src-tauri/src/commands.rs`, ajouter les imports :

```rust
use download_core::hls::playlist::{is_master, parse_master, parse_media, MasterPlaylist, Variant};
use download_core::hls::select::{best_variant, estimated_size, pick_audio};
use crate::state::PendingMedia;
```

(Avec `Variant` importé, remplacer dans `probe_hls` la construction `download_core::hls::playlist::Variant { ... }` par `Variant { ... }`.)

Ajouter les structs sérialisées et la commande :

```rust
#[derive(serde::Serialize, Clone)]
pub struct HlsVariant {
    pub index: u32,
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

#[derive(serde::Serialize, Clone)]
pub struct MediaVariantsEvent {
    pub probe_id: String,
    pub file_name: String,
    pub variants: Vec<HlsVariant>,
}

/// Sonde un master/media HLS, mémorise le probe et émet `media://variants`.
pub(crate) async fn probe_hls(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    suggested_name: Option<String>,
) -> Result<(), String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let text = fetch_text(&fetcher, url, &headers).await.map_err(|e| e.to_string())?;

    let (master, total_duration) = if is_master(&text) {
        let m = parse_master(&text, url);
        // Durée de référence : la media playlist de la meilleure variante.
        let dur = if let Some(v) = m.variants.get(best_variant(&m)) {
            match fetch_text(&fetcher, &v.uri, &headers).await {
                Ok(t) => parse_media(&t, &v.uri).total_duration(),
                Err(_) => 0.0,
            }
        } else {
            0.0
        };
        (m, dur)
    } else {
        // Media playlist directe ⇒ variante unique implicite.
        let media = parse_media(&text, url);
        let dur = media.total_duration();
        let m = MasterPlaylist {
            variants: vec![download_core::hls::playlist::Variant {
                uri: url.to_string(),
                bandwidth: 0,
                resolution: None,
                audio_group: None,
            }],
            audio: vec![],
        };
        (m, dur)
    };

    if master.variants.is_empty() {
        return Err("aucune variante HLS trouvée".into());
    }

    let variants: Vec<HlsVariant> = master
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| HlsVariant {
            index: i as u32,
            resolution: v.resolution.map(|(w, h)| format!("{w}x{h}")),
            bandwidth: v.bandwidth,
            est_size: if v.bandwidth > 0 && total_duration > 0.0 {
                Some(estimated_size(v.bandwidth, total_duration))
            } else {
                None
            },
        })
        .collect();

    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = suggested_name.unwrap_or_else(|| "video".to_string());
    {
        let state = app.state::<AppState>();
        state.pending_media.lock().unwrap().insert(
            probe_id.clone(),
            PendingMedia { master, headers, file_name: file_name.clone(), total_duration },
        );
    }
    let _ = app.emit("media://variants", MediaVariantsEvent { probe_id, file_name, variants });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    Ok(())
}

/// Lit entièrement une ressource texte (playlist) via le fetcher.
async fn fetch_text<F: ByteFetcher>(fetcher: &F, url: &str, headers: &RequestHeaders) -> Result<String, download_core::CoreError> {
    use futures_util::StreamExt;
    let mut stream = fetcher.fetch_range(url, None, headers).await?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk?);
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}
```

- [ ] **Step 3 : Commande `add_hls_download`**

Toujours dans `commands.rs`, ajouter :

```rust
/// Crée un download HLS à partir d'un probe et de la variante choisie.
#[tauri::command]
pub async fn add_hls_download(app: AppHandle, probe_id: String, variant_index: u32) -> Result<Download, String> {
    let (media_url, audio_url, file_name, headers) = {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let pm = pending.get(&probe_id).ok_or_else(|| "probe expiré".to_string())?;
        let variant = pm
            .master
            .variants
            .get(variant_index as usize)
            .ok_or_else(|| "variante invalide".to_string())?;
        let audio_url = pick_audio(&pm.master, variant).and_then(|a| a.uri.clone());
        let file_name = download_core::naming::with_extension(&pm.file_name, "mp4");
        let tuple = (variant.uri.clone(), audio_url, file_name, pm.headers.clone());
        pending.remove(&probe_id);
        tuple
    };

    // Dossier : dossier vidéo configuré, sinon Téléchargements.
    let dest_dir = {
        let state = app.state::<AppState>();
        match state.storage.category_dir(Category::Video).map_err(|e| e.to_string())? {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;
    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("nom de fichier invalide".into());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: media_url,
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category: Category::Video,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
        kind: download_core::model::DownloadKind::Hls,
        audio_url,
    };
    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(download)
}
```

- [ ] **Step 4 : Brancher `launch` sur le moteur HLS**

Dans `commands.rs`, fn `launch`, remplacer le bloc « Tâche moteur » qui appelle `engine::download(...)` par un `match` sur le `kind`. Modifier la closure spawnée :

```rust
    tauri::async_runtime::spawn(async move {
        let result = match download.kind {
            download_core::model::DownloadKind::Regular => {
                engine::download(&fetcher, &id_for_dl, &url, &dest, handle, limiter, tx, &headers).await
            }
            download_core::model::DownloadKind::Hls => {
                let remux = crate::remux::SidecarRemuxer::new(app_for_dl.clone());
                download_core::hls::engine::download_hls(
                    &fetcher,
                    &remux,
                    &id_for_dl,
                    &url,
                    download.audio_url.as_deref(),
                    &dest,
                    handle,
                    limiter,
                    tx,
                    &headers,
                )
                .await
            }
        };
        match result {
            Ok(_) => {}
            Err(e) => {
                eprintln!("download {id_for_dl} failed: {e}");
                let bytes = handle_for_err.downloaded();
                let st = app_for_dl.state::<AppState>();
                if let Err(se) = st.storage.mark_error(&id_for_dl, bytes, &e.to_string()) {
                    eprintln!("failed to persist error for {id_for_dl}: {se}");
                }
                let _ = app_for_dl.emit("download://progress", &Progress {
                    id: id_for_dl.clone(),
                    downloaded_bytes: bytes,
                    total_bytes: None,
                    speed_bps: 0,
                    status: DownloadStatus::Error,
                    segments_done: None,
                    segments_total: None,
                });
            }
        }
        {
            let st = app_for_dl.state::<AppState>();
            st.handles.lock().unwrap().remove(&id_for_dl);
        }
        reevaluate(&app_for_dl);
    });
```

> Note : `launch` capture déjà `download` par valeur ; vérifier qu'il n'est pas `move`-consommé avant le match (il est cloné en `dest`/`url`/`headers` en amont — garder l'accès à `download.kind`/`download.audio_url`). Au besoin, déplacer la lecture de `download.audio_url`/`download.kind` dans des variables locales avant le `spawn`.

- [ ] **Step 5 : Nettoyer le work dir HLS à l'annulation**

Dans `commands.rs`, fn `cancel_download`, dans le bloc `if !was_active { if let Ok(Some(d)) = ... }`, après `download_core::resume::clear(&p);`, ajouter la suppression du dossier de segments HLS (best-effort, sans échouer s'il est absent) :

```rust
            let _ = std::fs::remove_dir_all(format!("{}.fghls", d.dest_path));
```

(Pour un download actif, `download_hls` nettoie déjà le work dir à la réception du signal d'annulation ; cette ligne couvre le cas non-actif, comme pour le fichier partiel.)

- [ ] **Step 6 : Enregistrer `add_hls_download`**

Dans `src-tauri/src/lib.rs`, ajouter à `tauri::generate_handler![ ... ]` :

```rust
            commands::add_hls_download,
```

- [ ] **Step 7 : Vérifier la compilation**

Run: `cd src-tauri && cargo build`
Expected: compile. Lancer la suite : `cargo test` (cœur + crate) → PASS.

- [ ] **Step 8 : Commit**

```bash
git add src-tauri/src/
git commit -m "feat(tauri): probe_hls + pending_media + add_hls_download + HLS launch branch"
```

---

## Task 11 : Bridge — champ `kind` et routage HLS

**Files:**
- Modify: `src-tauri/src/bridge/protocol.rs`
- Modify: `src-tauri/src/bridge/server.rs`

- [ ] **Step 1 : Écrire un test protocole pour `kind`**

Dans `protocol.rs` `mod tests`, ajouter :

```rust
    #[test]
    fn capture_carries_kind_hls() {
        let msg = parse_client_message(
            r#"{"type":"capture","url":"http://x/m.m3u8","kind":"hls"}"#,
        );
        let (_authed, reply) = handle_message(true, "secret", msg);
        match reply {
            ServerReply::CaptureRequested { kind, .. } => assert_eq!(kind.as_deref(), Some("hls")),
            other => panic!("attendu CaptureRequested, obtenu {other:?}"),
        }
    }
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `cd src-tauri && cargo test bridge::protocol::tests::capture_carries_kind_hls`
Expected: FAIL — pas de champ `kind`.

- [ ] **Step 3 : Ajouter `kind` au protocole**

Dans `protocol.rs` :

1. `ClientMessage::Capture` : ajouter `kind: Option<String>,`.
2. `ServerReply::CaptureRequested` : ajouter `kind: Option<String>,`.
3. `RawMessage::Capture` : ajouter `#[serde(default)] kind: Option<String>,`.
4. `parse_client_message` : propager `kind` dans `ClientMessage::Capture { ..., kind }`.
5. `handle_message`, branche `Capture` : déstructurer `kind` et le passer dans `CaptureRequested { ..., kind }`.

Les tests existants qui construisent/filtrent `CaptureRequested { url, .. }` restent valides grâce au `..`.

- [ ] **Step 4 : Router HLS dans server.rs**

Dans `server.rs`, branche `ServerReply::CaptureRequested { ... }`, déstructurer aussi `kind` et router :

```rust
            ServerReply::CaptureRequested { url, filename, referrer, cookie, user_agent, kind } => {
                let headers = RequestHeaders { referrer, cookie, user_agent };
                if kind.as_deref() == Some("hls") {
                    match commands::probe_hls(&app, &url, headers, filename).await {
                        Ok(()) => serde_json::json!({ "type": "ack", "kind": "hls" }).to_string(),
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                } else {
                    match commands::add_url(&app, &url, "", headers, filename).await {
                        Ok(d) => {
                            let _ = app.emit("download://added", &d);
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.unminimize();
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                            serde_json::json!({ "type": "ack", "id": d.id }).to_string()
                        }
                        Err(e) => serde_json::json!({ "type": "error", "error": e }).to_string(),
                    }
                }
            }
```

(`probe_hls` rend déjà la fenêtre au premier plan.)

- [ ] **Step 5 : Vérifier la réussite**

Run: `cd src-tauri && cargo test`
Expected: tous PASS (protocole + reste).

- [ ] **Step 6 : Commit**

```bash
git add src-tauri/src/bridge/
git commit -m "feat(bridge): route kind=hls captures to probe_hls"
```

---

## Task 12 : Extension — `media-rules.js` (pur, TDD) + `kind` dans buildCapture

**Files:**
- Create: `extension/media-rules.js`
- Create: `extension/media-rules.test.js`
- Modify: `extension/buildCapture.js`
- Modify: `extension/buildCapture.test.js`

- [ ] **Step 1 : Écrire les tests de media-rules**

Créer `extension/media-rules.test.js` :

```js
import { describe, it, expect } from "vitest";
import { isHlsUrl, isHlsContentType, dedupeStreams } from "./media-rules.js";

describe("isHlsUrl", () => {
  it("matches .m3u8 path ignoring query", () => {
    expect(isHlsUrl("https://h/p/master.m3u8")).toBe(true);
    expect(isHlsUrl("https://h/p/master.m3u8?token=abc")).toBe(true);
  });
  it("rejects non-m3u8", () => {
    expect(isHlsUrl("https://h/p/video.mp4")).toBe(false);
    expect(isHlsUrl("https://h/p/seg.ts")).toBe(false);
  });
});

describe("isHlsContentType", () => {
  it("matches HLS mime types ignoring charset/case", () => {
    expect(isHlsContentType("application/vnd.apple.mpegurl")).toBe(true);
    expect(isHlsContentType("application/x-mpegURL; charset=utf-8")).toBe(true);
    expect(isHlsContentType("audio/mpegurl")).toBe(true);
  });
  it("rejects others", () => {
    expect(isHlsContentType("video/mp4")).toBe(false);
    expect(isHlsContentType("")).toBe(false);
  });
});

describe("dedupeStreams", () => {
  it("removes duplicate urls preserving order", () => {
    const out = dedupeStreams([
      { url: "a" },
      { url: "b" },
      { url: "a" },
    ]);
    expect(out.map((s) => s.url)).toEqual(["a", "b"]);
  });
});
```

- [ ] **Step 2 : Vérifier l'échec**

Run: `npx vitest run extension/media-rules.test.js`
Expected: FAIL — module introuvable.

- [ ] **Step 3 : Implémenter media-rules.js**

Créer `extension/media-rules.js` :

```js
// Logique pure et testable de détection HLS (aucun chrome.*).

// Vrai si le chemin de l'URL finit par .m3u8 (query/fragment ignorés).
export function isHlsUrl(url) {
  const path = String(url ?? "").split(/[?#]/)[0];
  return /\.m3u8$/i.test(path);
}

// Vrai si le Content-Type est un type HLS (paramètres ;charset ignorés, insensible à la casse).
export function isHlsContentType(ct) {
  const base = String(ct ?? "").split(";")[0].trim().toLowerCase();
  return [
    "application/vnd.apple.mpegurl",
    "application/x-mpegurl",
    "audio/mpegurl",
    "audio/x-mpegurl",
  ].includes(base);
}

// Dédoublonne une liste de flux par URL, en conservant l'ordre d'apparition.
export function dedupeStreams(list) {
  const seen = new Set();
  const out = [];
  for (const s of list || []) {
    if (seen.has(s.url)) continue;
    seen.add(s.url);
    out.push(s);
  }
  return out;
}
```

- [ ] **Step 4 : Vérifier la réussite**

Run: `npx vitest run extension/media-rules.test.js`
Expected: tous PASS.

- [ ] **Step 5 : Étendre buildCapture pour `kind`**

Dans `extension/buildCapture.js`, ajouter `"kind"` à la liste des clés autorisées :

```js
  for (const key of ["referrer", "cookie", "user_agent", "filename", "kind"]) {
```

Dans `extension/buildCapture.test.js`, ajouter un test :

```js
  it("includes kind when provided", () => {
    const msg = buildCapture("http://x/m.m3u8", { kind: "hls" });
    expect(msg.kind).toBe("hls");
  });
```

- [ ] **Step 6 : Vérifier la réussite**

Run: `npx vitest run extension/buildCapture.test.js extension/media-rules.test.js`
Expected: tous PASS.

- [ ] **Step 7 : Commit**

```bash
git add extension/media-rules.js extension/media-rules.test.js extension/buildCapture.js extension/buildCapture.test.js
git commit -m "feat(extension): HLS detection rules + kind in capture message"
```

---

## Task 13 : Extension — détection webRequest + popup « Vidéos détectées »

**Files:**
- Modify: `extension/manifest.json` (permission `webRequest`)
- Modify: `extension/background.js` (listeners webRequest + messages popup)
- Modify: `extension/popup.html` (section vidéos)
- Modify: `extension/popup.js` (liste + envoi)

> Ces changements touchent l'API `chrome.*` (non testable en Vitest) ; validation au smoke-test (Task 15).

- [ ] **Step 1 : Permission webRequest**

Dans `extension/manifest.json`, ajouter `"webRequest"` au tableau `permissions` :

```json
  "permissions": ["contextMenus", "storage", "activeTab", "alarms", "cookies", "downloads", "webRequest"],
```

- [ ] **Step 2 : Détection HLS par onglet dans background.js**

En haut de `extension/background.js`, ajouter l'import :

```js
import { isHlsUrl, isHlsContentType, dedupeStreams } from "./media-rules.js";
```

Ajouter un registre par onglet et les listeners (après la déclaration des variables d'état, ex. après `let pending = [];`) :

```js
// Flux HLS détectés par onglet : Map<tabId, [{ url, ts }]>
const detectedByTab = new Map();
const MEDIA_TTL_MS = 10 * 60 * 1000;

function recordStream(tabId, url) {
  if (tabId < 0) return;
  const list = detectedByTab.get(tabId) || [];
  const next = dedupeStreams([...list, { url, ts: Date.now() }]);
  detectedByTab.set(tabId, next.slice(-20)); // borne anti-accumulation
}

chrome.webRequest.onBeforeRequest.addListener(
  (details) => {
    if (isHlsUrl(details.url)) recordStream(details.tabId, details.url);
  },
  { urls: ["http://*/*", "https://*/*"] },
);

chrome.webRequest.onHeadersReceived.addListener(
  (details) => {
    const ct = (details.responseHeaders || []).find(
      (h) => h.name.toLowerCase() === "content-type",
    );
    if (ct && isHlsContentType(ct.value)) recordStream(details.tabId, details.url);
  },
  { urls: ["http://*/*", "https://*/*"] },
  ["responseHeaders"],
);

// Purge à la navigation principale et à la fermeture d'onglet.
chrome.webRequest.onBeforeNavigate?.addListener?.(() => {});
chrome.tabs.onRemoved.addListener((tabId) => detectedByTab.delete(tabId));
chrome.webNavigation?.onBeforeNavigate?.addListener?.((d) => {
  if (d.frameId === 0) detectedByTab.delete(d.tabId);
});
```

> `chrome.webNavigation` nécessite la permission `webNavigation` ; si on veut l'éviter, purger plutôt sur `chrome.tabs.onUpdated` avec `changeInfo.url`. Pour rester minimal sans permission supplémentaire, remplacer le bloc `webNavigation` par :

```js
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.url) detectedByTab.delete(tabId);
});
```

(Retirer la ligne `chrome.webNavigation?...` et la ligne `chrome.webRequest.onBeforeNavigate?...`.)

- [ ] **Step 3 : Messages popup pour lister/envoyer un flux**

Dans `extension/background.js`, dans le listener `chrome.runtime.onMessage`, ajouter deux commandes (avant le `return false;` final) :

```js
  if (req.cmd === "listMedia") {
    const list = detectedByTab.get(req.tabId) || [];
    sendResponse({ streams: list });
    return false;
  }
  if (req.cmd === "sendMedia" && req.url) {
    (async () => {
      let referrer;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        referrer = tab && tab.url;
      } catch (_e) {
        // pas de referer
      }
      const ok = await sendMedia(req.url, referrer);
      sendResponse({ ok });
    })();
    return true;
  }
```

Ajouter la fonction `sendMedia` (à côté de `sendCapture`) :

```js
// Envoie une capture HLS (kind:"hls"). Comme sendCapture mais marque le type média.
async function sendMedia(url, referrer) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind: "hls" })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, kind: "hls", ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```

Mettre à jour `flushPending` pour propager `kind` :

```js
      const headers = await gatherHeaders(it.url, it.referrer);
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename, kind: it.kind })));
```

- [ ] **Step 4 : Section vidéos dans le popup**

Dans `extension/popup.html`, ajouter dans `#connectedView` (après le bouton `sendTab`) :

```html
      <div id="mediaSection" class="hidden">
        <hr />
        <div class="muted">Vidéos sur cette page</div>
        <div id="mediaList"></div>
      </div>
```

Dans `extension/popup.js`, après `refresh();`, ajouter le chargement des flux :

```js
async function refreshMedia() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab) return;
  chrome.runtime.sendMessage({ cmd: "listMedia", tabId: tab.id }, (res) => {
    const streams = (res && res.streams) || [];
    const section = document.getElementById("mediaSection");
    const list = document.getElementById("mediaList");
    list.innerHTML = "";
    section.classList.toggle("hidden", streams.length === 0);
    streams.forEach((s, i) => {
      const btn = document.createElement("button");
      btn.textContent = `Télécharger la vidéo ${streams.length > 1 ? i + 1 : ""}`.trim();
      btn.addEventListener("click", () => {
        chrome.runtime.sendMessage({ cmd: "sendMedia", url: s.url, tabId: tab.id }, (r) => {
          flash(r && r.ok ? "Envoyé ✓" : "Échec — non connecté");
        });
      });
      list.appendChild(btn);
    });
  });
}

refreshMedia();
```

- [ ] **Step 5 : Vérifier la non-régression des suites JS**

Run: `npm run test`
Expected: toutes les suites Vitest PASS (les fichiers `chrome.*` ne sont pas testés unitairement).

- [ ] **Step 6 : Commit**

```bash
git add extension/manifest.json extension/background.js extension/popup.html extension/popup.js
git commit -m "feat(extension): detect HLS via webRequest + popup video download list"
```

---

## Task 14 : Frontend — dialogue de qualité + progression segments + badge HLS

**Files:**
- Modify: `src/api.ts` (event `media://variants`, commande `addHlsDownload`)
- Create: `src/components/HlsVariantDialog.tsx`
- Modify: `src/App.tsx` (écoute event + dialogue + état segments)
- Modify: `src/components/DownloadTable.tsx` (passer segments)
- Modify: `src/components/DownloadRow.tsx` (badge + progression segments)
- Modify: `src/format.ts` (format taille) — si non présent

- [ ] **Step 1 : API — event + commande**

Dans `src/api.ts`, ajouter les types et fonctions :

```ts
export interface HlsVariant {
  index: number;
  resolution: string | null;
  bandwidth: number;
  est_size: number | null;
}

export interface MediaVariantsEvent {
  probe_id: string;
  file_name: string;
  variants: HlsVariant[];
}

export function onMediaVariants(cb: (e: MediaVariantsEvent) => void): Promise<UnlistenFn> {
  return listen<MediaVariantsEvent>("media://variants", (e) => cb(e.payload));
}

export function addHlsDownload(probeId: string, variantIndex: number): Promise<Download> {
  return invoke<Download>("add_hls_download", { probeId, variantIndex });
}
```

- [ ] **Step 2 : Composant HlsVariantDialog**

Créer `src/components/HlsVariantDialog.tsx` :

```tsx
import { useEffect, useState } from "react";
import type { HlsVariant, MediaVariantsEvent } from "../api";

function fmtSize(bytes: number | null): string {
  if (!bytes) return "—";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) return `≈ ${(mb / 1024).toFixed(1)} Go`;
  return `≈ ${Math.round(mb)} Mo`;
}

function label(v: HlsVariant): string {
  const res = v.resolution ? v.resolution.split("x")[1] + "p" : "audio/auto";
  const mbps = v.bandwidth ? `${(v.bandwidth / 1_000_000).toFixed(1)} Mbps` : "";
  return [res, mbps, fmtSize(v.est_size)].filter(Boolean).join(" · ");
}

export function HlsVariantDialog({
  event,
  onClose,
  onConfirm,
}: {
  event: MediaVariantsEvent | null;
  onClose: () => void;
  onConfirm: (probeId: string, variantIndex: number) => void;
}) {
  const [selected, setSelected] = useState(0);
  useEffect(() => {
    // Pré-sélectionne la meilleure variante (plus haut bandwidth).
    if (event && event.variants.length) {
      const best = event.variants.reduce((a, b) => (b.bandwidth > a.bandwidth ? b : a));
      setSelected(best.index);
    }
  }, [event]);
  if (!event) return null;
  return (
    <div className="fixed inset-0 bg-black/30 flex items-center justify-center" onClick={onClose}>
      <div className="bg-white rounded-lg p-5 w-[480px]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-semibold mb-1">Télécharger la vidéo</h3>
        <p className="text-sm text-gray-500 mb-3 truncate">{event.file_name}</p>
        <div className="space-y-1 max-h-64 overflow-auto">
          {event.variants.map((v) => (
            <label key={v.index} className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-gray-50 cursor-pointer">
              <input
                type="radio"
                name="variant"
                checked={selected === v.index}
                onChange={() => setSelected(v.index)}
              />
              <span className="text-sm">{label(v)}</span>
            </label>
          ))}
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-gray-100" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md bg-indigo-600 text-white font-semibold"
            onClick={() => onConfirm(event.probe_id, selected)}
          >
            Télécharger
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3 : Câbler le dialogue + l'état segments dans App.tsx**

Dans `src/App.tsx` :

1. Importer :

```tsx
import { HlsVariantDialog } from "./components/HlsVariantDialog";
import type { MediaVariantsEvent } from "./api";
```

2. Ajouter l'état :

```tsx
  const [mediaEvent, setMediaEvent] = useState<MediaVariantsEvent | null>(null);
  const [segs, setSegs] = useState<Record<string, { done: number; total: number }>>({});
```

3. Dans le `useEffect`, étendre `onProgress` pour capter les segments et ajouter l'écoute `onMediaVariants` :

```tsx
    const un = api.onProgress((p) => {
      setSpeeds((s) => ({ ...s, [p.id]: p.speed_bps }));
      if (p.segments_total != null && p.segments_done != null) {
        setSegs((m) => ({ ...m, [p.id]: { done: p.segments_done!, total: p.segments_total! } }));
      }
      setDownloads((list) =>
        list.map((d) =>
          d.id === p.id ? { ...d, downloaded_bytes: p.downloaded_bytes, status: p.status, total_bytes: p.total_bytes } : d,
        ),
      );
      if (p.status === "completed" || p.status === "paused" || p.status === "error") refresh();
    });
    const unMedia = api.onMediaVariants((e) => setMediaEvent(e));
```

Et dans le cleanup `return`, ajouter `unMedia.then((f) => f());`.

4. Ajouter le handler de confirmation :

```tsx
  async function handleHlsConfirm(probeId: string, variantIndex: number) {
    setMediaEvent(null);
    try {
      await api.addHlsDownload(probeId, variantIndex);
      await refresh();
    } catch (e) {
      setError(`Impossible de démarrer la vidéo : ${String(e)}`);
    }
  }
```

5. Rendre le dialogue (à côté de `<SettingsDialog ... />`) :

```tsx
      <HlsVariantDialog event={mediaEvent} onClose={() => setMediaEvent(null)} onConfirm={handleHlsConfirm} />
```

6. Passer `segs` à la table :

```tsx
        <DownloadTable
          downloads={visible}
          allDownloads={downloads}
          speeds={speeds}
          segments={segs}
          ...
```

- [ ] **Step 4 : Propager `segments` dans DownloadTable → DownloadRow**

Dans `src/components/DownloadTable.tsx`, ajouter la prop `segments` au type des props et la passer à chaque `DownloadRow` :

```tsx
  segments: Record<string, { done: number; total: number }>;
```

et au rendu de la ligne :

```tsx
          seg={segments[d.id]}
```

Dans `src/components/DownloadRow.tsx` :

1. Ajouter au type des props :

```tsx
  seg?: { done: number; total: number };
```

2. Recevoir `seg` dans la déstructuration des props.

3. Adapter `stateLabel` et la barre pour le cas HLS. Remplacer le calcul `pct`/`stateLabel` par :

```tsx
  const isHls = d.kind === "hls";
  const segPct = seg && seg.total > 0 ? Math.round((seg.done / seg.total) * 100) : 0;
  const pct = isHls && seg ? segPct : percent(d.downloaded_bytes, d.total_bytes);
  const stateLabel =
    d.status === "queued" && queuePosition !== null
      ? `En file (${queuePosition})`
      : isHls && seg && d.status === "downloading"
        ? `En cours · ${seg.done}/${seg.total} segments`
        : `${STATUS_LABEL[d.status]} · ${pct}%`;
```

4. Ajouter un badge « HLS » à côté du nom de fichier :

```tsx
        <div className="font-semibold truncate">
          {d.kind === "hls" && (
            <span className="mr-1 px-1 py-0.5 text-[10px] rounded bg-indigo-100 text-indigo-700 align-middle">HLS</span>
          )}
          {d.file_name}
        </div>
```

- [ ] **Step 5 : Vérifier le build TypeScript**

Run: `npm run build`
Expected: `tsc` + build OK (pas d'erreur de type). Lancer aussi `npm run test`.

- [ ] **Step 6 : Commit**

```bash
git add src/
git commit -m "feat(frontend): HLS quality dialog + segment progress + HLS badge"
```

---

## Task 15 : Vérification finale + smoke-tests

**Files:** (aucun — vérification)

- [ ] **Step 1 : Suites automatiques**

Run: `cd src-tauri && cargo test`
Expected: toutes les suites Rust PASS.

Run: `npm run test`
Expected: toutes les suites Vitest PASS.

- [ ] **Step 2 : Smoke-test bout en bout (manuel)**

Lancer l'app (`npm run tauri dev`), charger l'extension non empaquetée (`chrome://extensions` → « charger l'extension non empaquetée » → dossier `extension/`), configurer le token dans les options (déjà fait en phase 3). Puis :

1. **VOD clair** : ouvrir une page avec un flux HLS VOD non chiffré, ouvrir le popup → un bouton « Télécharger la vidéo » apparaît → cliquer → le dialogue de qualité s'affiche → choisir → le download apparaît (badge HLS, « X/N segments ») → à 100 % un `.mp4` lisible est produit dans le dossier Vidéos/Téléchargements.
2. **AES-128** : répéter sur un flux chiffré AES-128 clé en clair → le `.mp4` est lisible (déchiffrement OK).
3. **Audio séparé** : flux avec piste audio EXT-X-MEDIA séparée → la vidéo finale **a du son**.
4. **Live** : flux live (sans ENDLIST) → le download passe en **Erreur** avec message « flux live non supporté ».
5. **Pause/reprise** : mettre en pause un HLS en cours puis reprendre → il **ne re-télécharge pas** les segments déjà faits et termine.

- [ ] **Step 3 : Self-review & merge**

Vérifier l'absence de régression sur les téléchargements classiques (ajout manuel d'un `.zip`), puis suivre la skill `finishing-a-development-branch` pour merger `phase-4a-hls-vod` dans `main` (`merge --no-ff`).

---

## Notes d'implémentation transverses

- **Concaténation côté cœur** : `download_hls` produit un fichier unique par piste (`v.cat`, `a.cat`, init fMP4 préfixé) ; `Remuxer` ne fait que le remux conteneur. Le `FakeRemuxer` concatène les inputs, ce qui rend les tests déterministes.
- **Reprise HLS** : repose sur la présence des fichiers `*.seg` dans le work dir `dest+".fghls"`. Pas de sidecar JSON dédié (différent du moteur classique) — le répertoire de segments **est** l'état de reprise. L'annulation supprime ce répertoire (via `cancel_download` qui supprime `dest` ; ajouter la suppression du work dir si nécessaire — voir ci-dessous).
- **Annulation HLS** : `cancel_download` (commands.rs) supprime `dest_path` et le sidecar `.fgdownload` ; la suppression du work dir `dest_path + ".fghls"` est ajoutée en Task 10 Step 5 (cas non-actif) et faite par `download_hls` lui-même pour un download actif.
- **Limites assumées (spec §7)** : pas de live, pas de DASH, pas de DRM/SAMPLE-AES, ffmpeg Windows seul, résolution d'URI sans normalisation `..`, byterange à offset implicite supporté via offset courant. Ces limites sont documentées dans la spec §7 et §10.
