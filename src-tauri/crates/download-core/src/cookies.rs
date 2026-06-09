//! Cookies pour yt-dlp : sérialisation Netscape + choix de la source (fichier / navigateur).

/// Un cookie prêt à sérialiser au format Netscape.
#[derive(Debug, Clone, PartialEq)]
pub struct NetscapeCookie {
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<i64>, // epoch (s) ; None = cookie de session
    pub name: String,
    pub value: String,
}

/// Sérialise des cookies au format Netscape (lu par yt-dlp/curl).
/// httpOnly → domaine préfixé `#HttpOnly_` (convention reconnue par yt-dlp).
pub fn to_netscape(cookies: &[NetscapeCookie]) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in cookies {
        let include_sub = if c.domain.starts_with('.') { "TRUE" } else { "FALSE" };
        let secure = if c.secure { "TRUE" } else { "FALSE" };
        let expiry = c.expires.unwrap_or(0);
        let domain = if c.http_only { format!("#HttpOnly_{}", c.domain) } else { c.domain.clone() };
        out.push_str(&format!(
            "{domain}\t{include_sub}\t{}\t{secure}\t{expiry}\t{}\t{}\n",
            c.path, c.name, c.value
        ));
    }
    out
}

/// Vrai si l'hôte de `url` appartient à YouTube/Google (on n'attache les cookies que là).
pub fn cookies_apply_to(url: &str) -> bool {
    let host = url
        .split("://").nth(1).unwrap_or(url)
        .split('/').next().unwrap_or("")
        .rsplit('@').next().unwrap_or("")
        .split(':').next().unwrap_or("")
        .to_ascii_lowercase();
    const DOMAINS: [&str; 5] = ["youtube.com", "youtu.be", "googlevideo.com", "google.com", "ytimg.com"];
    DOMAINS.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Source de cookies choisie pour un run yt-dlp.
#[derive(Debug, Clone, PartialEq)]
pub enum CookieSource {
    File,
    Browser(String),
    None,
}

/// Décide la source : fichier cookies.txt si présent, sinon repli navigateur, sinon rien.
/// N'attache rien hors domaines YouTube/Google.
pub fn choose_cookie_source(url: &str, has_file: bool, browser_fallback: Option<&str>) -> CookieSource {
    if !cookies_apply_to(url) {
        return CookieSource::None;
    }
    if has_file {
        return CookieSource::File;
    }
    match browser_fallback {
        Some(b) if !b.is_empty() => CookieSource::Browser(b.to_string()),
        _ => CookieSource::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ck(name: &str, val: &str, domain: &str, http_only: bool, expires: Option<i64>) -> NetscapeCookie {
        NetscapeCookie { domain: domain.into(), path: "/".into(), secure: true, http_only, expires, name: name.into(), value: val.into() }
    }

    #[test]
    fn netscape_header_and_line() {
        let s = to_netscape(&[ck("SID", "abc", ".youtube.com", false, Some(1700000000))]);
        assert!(s.starts_with("# Netscape HTTP Cookie File\n"));
        assert!(s.contains(".youtube.com\tTRUE\t/\tTRUE\t1700000000\tSID\tabc\n"));
    }

    #[test]
    fn netscape_httponly_prefix() {
        let s = to_netscape(&[ck("__Secure-3PSID", "x", ".youtube.com", true, Some(1))]);
        assert!(s.contains("#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t1\t__Secure-3PSID\tx\n"));
    }

    #[test]
    fn netscape_session_cookie_expiry_zero_and_host_only() {
        let s = to_netscape(&[ck("VISITOR", "y", "www.youtube.com", false, None)]);
        assert!(s.contains("www.youtube.com\tFALSE\t/\tTRUE\t0\tVISITOR\ty\n"));
    }

    #[test]
    fn apply_to_youtube_google_only() {
        assert!(cookies_apply_to("https://www.youtube.com/watch?v=x"));
        assert!(cookies_apply_to("https://youtu.be/abc"));
        assert!(cookies_apply_to("https://r1---sn-x.googlevideo.com/videoplayback"));
        assert!(cookies_apply_to("https://accounts.google.com/x"));
        assert!(!cookies_apply_to("https://example.com/f.zip"));
        assert!(!cookies_apply_to("https://notyoutube.com.evil.com/x"));
    }

    #[test]
    fn choose_source_branches() {
        assert_eq!(choose_cookie_source("https://youtube.com/x", true, None), CookieSource::File);
        assert_eq!(choose_cookie_source("https://youtube.com/x", false, Some("chrome")), CookieSource::Browser("chrome".into()));
        assert_eq!(choose_cookie_source("https://youtube.com/x", false, Some("")), CookieSource::None);
        assert_eq!(choose_cookie_source("https://youtube.com/x", false, None), CookieSource::None);
        assert_eq!(choose_cookie_source("https://example.com/x", true, Some("chrome")), CookieSource::None);
    }
}
