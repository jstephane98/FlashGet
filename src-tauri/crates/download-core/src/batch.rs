//! Parsing d'un collage multi-lignes (une URL par ligne) en liste d'URLs à télécharger en lot.

use std::collections::HashSet;

/// Résultat du parsing d'un texte collé.
#[derive(Debug, Default, PartialEq)]
pub struct BatchParse {
    /// URLs gardées, ordre d'apparition conservé, dédupliquées.
    pub urls: Vec<String>,
    /// Lignes non vides qui ne sont pas des URLs `http(s)://`.
    pub skipped_invalid: usize,
    /// Occurrences d'URLs `http(s)` répétées (au-delà de la première).
    pub skipped_duplicate: usize,
}

/// Vrai si `s` commence par `http://` ou `https://` (schéma insensible à la casse).
fn is_http_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Découpe `text` en lignes, `trim` chacune, ignore les lignes vides (non comptées),
/// garde uniquement les URLs `http(s)://` (sinon `skipped_invalid`), et déduplique
/// dans le lot en conservant l'ordre (répétitions → `skipped_duplicate`).
pub fn parse_batch(text: &str) -> BatchParse {
    let mut out = BatchParse::default();
    let mut seen = HashSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue; // ligne vide : ignorée, non comptée
        }
        if !is_http_url(line) {
            out.skipped_invalid += 1;
            continue;
        }
        if seen.insert(line) {
            out.urls.push(line.to_string());
        } else {
            out.skipped_duplicate += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_empty_result() {
        assert_eq!(parse_batch(""), BatchParse::default());
    }

    #[test]
    fn blank_lines_are_ignored_not_counted() {
        let r = parse_batch("\n  \n\t\n");
        assert!(r.urls.is_empty());
        assert_eq!(r.skipped_invalid, 0);
        assert_eq!(r.skipped_duplicate, 0);
    }

    #[test]
    fn keeps_http_and_https_in_order() {
        let r = parse_batch("https://a.com/1.zip\nhttp://b.com/2.iso");
        assert_eq!(r.urls, vec!["https://a.com/1.zip", "http://b.com/2.iso"]);
        assert_eq!(r.skipped_invalid, 0);
        assert_eq!(r.skipped_duplicate, 0);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let r = parse_batch("   https://a.com/1.zip  \n\t http://b.com/2.iso\t");
        assert_eq!(r.urls, vec!["https://a.com/1.zip", "http://b.com/2.iso"]);
    }

    #[test]
    fn handles_crlf_line_endings() {
        let r = parse_batch("https://a.com/1.zip\r\nhttps://b.com/2.zip\r\n");
        assert_eq!(r.urls, vec!["https://a.com/1.zip", "https://b.com/2.zip"]);
    }

    #[test]
    fn non_http_lines_counted_invalid() {
        let r = parse_batch("ftp://x.com/f\nwww.y.com/z\njuste du texte\nhttps://ok.com/a.zip");
        assert_eq!(r.urls, vec!["https://ok.com/a.zip"]);
        assert_eq!(r.skipped_invalid, 3);
        assert_eq!(r.skipped_duplicate, 0);
    }

    #[test]
    fn duplicates_are_deduped_and_counted() {
        let r = parse_batch("https://a.com/1.zip\nhttps://a.com/1.zip\nhttps://a.com/1.zip");
        assert_eq!(r.urls, vec!["https://a.com/1.zip"]);
        assert_eq!(r.skipped_duplicate, 2);
        assert_eq!(r.skipped_invalid, 0);
    }

    #[test]
    fn uppercase_scheme_is_kept() {
        let r = parse_batch("HTTPS://a.com/1.zip\nHttp://b.com/2.zip");
        assert_eq!(r.urls, vec!["HTTPS://a.com/1.zip", "Http://b.com/2.zip"]);
        assert_eq!(r.skipped_invalid, 0);
    }

    #[test]
    fn mixed_realistic_input() {
        let text = "\
https://site.com/a.zip
   https://site.com/b.iso
https://site.com/a.zip
pas une url
ftp://nope.com/x

http://site.com/c.bin
";
        let r = parse_batch(text);
        assert_eq!(
            r.urls,
            vec![
                "https://site.com/a.zip",
                "https://site.com/b.iso",
                "http://site.com/c.bin",
            ]
        );
        assert_eq!(r.skipped_duplicate, 1); // le 2e a.zip
        assert_eq!(r.skipped_invalid, 2);   // "pas une url" + ftp
    }
}
