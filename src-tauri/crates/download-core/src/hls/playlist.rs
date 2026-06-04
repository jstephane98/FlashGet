use std::collections::HashMap;

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

/// Parse la liste d'attributs d'une ligne `#EXT-X-...:key=val,key="val,with,commas",...`.
/// Gère les valeurs entre guillemets (qui peuvent contenir des virgules). Les guillemets
/// sont retirés. Renvoie une map clé→valeur.
pub(crate) fn parse_attributes(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && bytes[i] != b',' {
            i += 1;
        }
        // Token sans '=' (malformé ou vide) : on saute jusqu'à la prochaine virgule et on continue.
        if i >= bytes.len() || bytes[i] == b',' {
            if i < bytes.len() {
                i += 1; // saute la virgule
            }
            continue;
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
    // Position de "://" dans la base ; `scheme` = partie avant (ex. "https"), vide si absente.
    let colon = base.find("://");
    let scheme = colon.map(|p| &base[..p]).unwrap_or("");
    let scheme_end = colon.map(|p| p + 3).unwrap_or(0);
    if let Some(rest) = uri.strip_prefix("//") {
        // Protocole-relatif : hérite du scheme de la base, sinon https par défaut.
        let scheme = if scheme.is_empty() { "https" } else { scheme };
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

    #[test]
    fn resolve_protocol_relative() {
        assert_eq!(
            resolve_uri("https://h/p/x.m3u8", "//cdn.example.com/seg.ts"),
            "https://cdn.example.com/seg.ts"
        );
    }

    #[test]
    fn attributes_empty_and_malformed_tokens() {
        assert!(parse_attributes("").is_empty());
        // Un token malformé (sans '=') ne doit pas faire perdre les paires valides suivantes.
        let m = parse_attributes("GARBAGE,BANDWIDTH=42");
        assert_eq!(m.get("BANDWIDTH").unwrap(), "42");
    }

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
}
