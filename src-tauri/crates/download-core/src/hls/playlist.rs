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
