//! Sites ignorés par l'extension — logique PURE (aucune I/O). Mêmes règles que
//! `extension/site-rules.js` : minuscules, sans port, sans « www. » de tête, sans « *. » de tête,
//! sans point final. La saisie peut en plus être une URL complète (schéma, chemin, requête).

/// Nombre maximal de sites ignorés.
pub const MAX_SITES: usize = 200;

/// Clé de préférence (interne, absente de la liste blanche de l'interface).
pub const PREF_KEY: &str = "ext.ignored_sites";

fn is_ipv4(h: &str) -> bool {
    let parts: Vec<&str> = h.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().map_or(false, |n| n <= 255))
}

fn valid_host(h: &str) -> bool {
    if h.is_empty() || h.len() > 253 {
        return false;
    }
    if h == "localhost" || is_ipv4(h) {
        return true;
    }
    let labels: Vec<&str> = h.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Normalise une saisie (hôte ou URL) en hôte de site ASCII (punycode pour un IDN).
/// Erreur : « Adresse de site invalide : … ».
///
/// Ordre : trim + minuscules ; schéma puis tout depuis le premier `/`, `?` ou `#` ; port ;
/// point final ; `*.` de tête ; `www.` de tête (une seule fois) ; IDN -> punycode ; validation.
pub fn normalize_site(input: &str) -> Result<String, String> {
    let err = || format!("Adresse de site invalide : {}", input.trim());
    let mut s = input.trim().to_lowercase();
    if s.is_empty() || s.chars().any(|c| c.is_whitespace()) {
        return Err(err());
    }
    // Schéma seulement en tête (`^[a-z][a-z0-9+.-]*://`) ; sinon la chaîne reste telle quelle
    // et la coupe au premier `/`, `?` ou `#` s'en charge.
    if let Some(i) = s.find("://") {
        let scheme = s[..i].as_bytes();
        if scheme.first().map_or(false, |b| b.is_ascii_lowercase())
            && scheme.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.'))
        {
            s = s[i + 3..].to_string();
        }
    }
    if let Some(i) = s.find(|c| c == '/' || c == '?' || c == '#') {
        s.truncate(i);
    }
    // Port : un `:chiffres` final.
    if let Some(i) = s.rfind(':') {
        let tail = &s[i + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            s.truncate(i);
        }
    }
    if s.ends_with('.') {
        s.pop();
    }
    if let Some(r) = s.strip_prefix("*.") {
        s = r.to_string();
    }
    if let Some(r) = s.strip_prefix("www.") {
        s = r.to_string();
    }
    // Punycode : seulement pour une saisie non ASCII (évite que `url` réécrive les IPv4 abrégées).
    if !s.is_ascii() {
        if s.contains(|c: char| matches!(c, ':' | '@' | '[' | ']' | '%' | '\\')) {
            return Err(err());
        }
        s = url::Url::parse(&format!("http://{s}/"))
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .ok_or_else(err)?;
    }
    if valid_host(&s) {
        Ok(s)
    } else {
        Err(err())
    }
}

/// Vrai si `host` est un des `sites`, ou (site de type nom de domaine) un sous-domaine de l'un d'eux.
/// Une IPv4 ou `localhost` ne correspondent qu'à elles-mêmes.
// Test seulement : l'extension décide des sites ignorés, pas l'application.
#[cfg(test)]
pub fn host_is_ignored(host: &str, sites: &[String]) -> bool {
    let host = match normalize_site(host) {
        Ok(h) => h,
        Err(_) => return false,
    };
    sites.iter().filter_map(|s| normalize_site(s).ok()).any(|site| {
        host == site || (site != "localhost" && !is_ipv4(&site) && host.ends_with(&format!(".{site}")))
    })
}

/// Normalise une liste complète : refuse tout si une entrée est invalide ; dédoublonne, trie, plafonne.
pub fn clean_list(sites: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(sites.len());
    for s in sites {
        out.push(normalize_site(s)?);
    }
    out.sort();
    out.dedup();
    out.truncate(MAX_SITES);
    Ok(out)
}

/// Ajoute ou retire `host`. Résultat normalisé, sans doublon, trié, au plus `MAX_SITES`.
/// Hôte invalide : la liste est renvoyée nettoyée, inchangée. Liste pleine : l'ajout est ignoré.
pub fn set_ignored(sites: &[String], host: &str, ignored: bool) -> Vec<String> {
    let mut list: Vec<String> = sites.iter().filter_map(|s| normalize_site(s).ok()).collect();
    list.sort();
    list.dedup();
    list.truncate(MAX_SITES);
    if let Ok(h) = normalize_site(host) {
        if ignored {
            if !list.contains(&h) && list.len() < MAX_SITES {
                list.push(h);
                list.sort();
            }
        } else {
            list.retain(|s| *s != h);
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn normalize_site_cases() {
        assert_eq!(normalize_site("https://www.Exemple.com/page?x=1#f").unwrap(), "exemple.com");
        assert_eq!(normalize_site("exemple.com:8080").unwrap(), "exemple.com");
        assert_eq!(normalize_site("*.exemple.com").unwrap(), "exemple.com");
        assert_eq!(normalize_site("Exemple.com.").unwrap(), "exemple.com");
        assert_eq!(normalize_site("  sub.exemple.com  ").unwrap(), "sub.exemple.com");
        assert_eq!(normalize_site("localhost").unwrap(), "localhost");
        assert_eq!(normalize_site("http://localhost:3000/x").unwrap(), "localhost");
        assert_eq!(normalize_site("192.168.1.10").unwrap(), "192.168.1.10");
        assert_eq!(normalize_site("http://10.0.0.1:80/a").unwrap(), "10.0.0.1");
        assert_eq!(normalize_site("exemple.com.:8080").unwrap(), "exemple.com");
        assert_eq!(normalize_site("https://www.Exemple.com/page").unwrap(), "exemple.com");
        assert_eq!(normalize_site("éxemple.com").unwrap(), "xn--xemple-9ua.com");
        assert_eq!(normalize_site("www.www.exemple.com").unwrap(), "www.exemple.com");
        assert_eq!(normalize_site("a.com/x?u=http://b.com").unwrap(), "a.com");
        assert_eq!(normalize_site("https://a.com/r?u=http://x.com").unwrap(), "a.com");
    }

    #[test]
    fn normalize_site_rejects() {
        for bad in ["", "   ", "exemple", "exe mple.com", "a b", "exemple..com", "-a.com", "a_b.com", "www.com", "com", "::1", "[::1]", "http://", "exemple\t.com","ex!.com", "www.", "[::1]"] {
            let r = normalize_site(bad);
            assert!(r.is_err(), "{bad:?} aurait dû être refusé");
            assert!(r.unwrap_err().starts_with("Adresse de site invalide"));
        }
        assert_eq!(normalize_site("a b").unwrap_err(), "Adresse de site invalide : a b");
    }

    #[test]
    fn host_is_ignored_rules() {
        let s = v(&["drive.google.com", "ample.com"]);
        assert!(host_is_ignored("drive.google.com", &s));
        assert!(host_is_ignored("docs.drive.google.com", &s));
        assert!(!host_is_ignored("google.com", &s));
        assert!(!host_is_ignored("exemple.com", &s));
        assert!(host_is_ignored("www.ample.com", &s));
        assert!(host_is_ignored("DRIVE.google.com", &s));
        assert!(!host_is_ignored("", &s));
        let ip = v(&["192.168.1.2", "localhost"]);
        assert!(host_is_ignored("192.168.1.2", &ip));
        assert!(!host_is_ignored("x.192.168.1.2", &ip));
        assert!(host_is_ignored("localhost", &ip));
        assert!(!host_is_ignored("a.localhost", &ip));
        assert!(host_is_ignored("sous.xn--xemple-9ua.com", &v(&["éxemple.com"])));
        assert!(!host_is_ignored("a.com", &v(&[""])));
    }

    #[test]
    fn set_ignored_add_remove_dedup_sort() {
        let l = set_ignored(&[], "https://B.com/x", true);
        assert_eq!(l, v(&["b.com"]));
        let l = set_ignored(&l, "a.com", true);
        assert_eq!(l, v(&["a.com", "b.com"]));
        let l = set_ignored(&l, "www.a.com", true);
        assert_eq!(l, v(&["a.com", "b.com"]));
        let l = set_ignored(&l, "A.com", false);
        assert_eq!(l, v(&["b.com"]));
        // Hôte invalide : inchangé.
        assert_eq!(set_ignored(&l, "pas valide", true), v(&["b.com"]));
        // Retrait d'un absent : inchangé.
        assert_eq!(set_ignored(&l, "z.com", false), v(&["b.com"]));
    }

    #[test]
    fn set_ignored_caps_at_200() {
        let full: Vec<String> = (0..200).map(|i| format!("s{i:03}.com")).collect();
        let l = set_ignored(&full, "nouveau.com", true);
        assert_eq!(l.len(), 200);
        assert!(!l.contains(&"nouveau.com".to_string()));
        let over: Vec<String> = (0..250).map(|i| format!("s{i:03}.com")).collect();
        assert_eq!(set_ignored(&over, "x.com", false).len(), 200);
        assert_eq!(clean_list(&over).unwrap().len(), 200);
    }

    #[test]
    fn clean_list_rejects_whole_list() {
        assert!(clean_list(&v(&["a.com", "mauvais mot"])).unwrap_err().contains("mauvais mot"));
        assert_eq!(clean_list(&v(&["B.com", "a.com", "www.b.com"])).unwrap(), v(&["a.com", "b.com"]));
    }
}
