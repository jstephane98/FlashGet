//! Choix du nom de fichier final pour un téléchargement.

/// Nom de fichier de repli quand le titre est absent ou inutilisable.
const DEFAULT_TITLE: &str = "video";

/// Noms de périphériques réservés Windows (comparaison insensible à la casse).
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Transforme un titre de page libre en radical de nom de fichier sûr (sans extension).
/// `None`, chaîne vide ou inutilisable → `"video"`.
///
/// Contrairement à [`choose_file_name`], cette fonction ne traite jamais `/` ni `\`
/// comme des séparateurs de chemin : un titre comme `AC/DC` est conservé entier.
pub fn sanitize_title(raw: Option<&str>) -> String {
    match raw {
        Some(s) => sanitize_capped(s, MAX_TITLE_CHARS),
        None => DEFAULT_TITLE.to_string(),
    }
}

/// Longueur maximale (en caractères) d'un titre ou d'un nom de fichier nettoyé.
const MAX_TITLE_CHARS: usize = 120;

/// Extensions média qu'un changement de format remplace au lieu de les compléter.
const MEDIA_EXTENSIONS: &[&str] = &["mp4", "mp3", "m4a", "webm", "opus", "mkv"];

/// Comme [`sanitize_title`] pour un nom de fichier complet : seul le radical est nettoyé
/// et tronqué, l'extension (dernier `.xxx`, alphanumérique, 16 caractères au plus) est
/// conservée, de sorte que le total tienne dans la même limite.
pub fn sanitize_file_name(name: &str) -> String {
    let name = name.trim();
    if let Some((stem, ext)) = name.rsplit_once('.') {
        let ext_ok = !ext.is_empty() && ext.len() <= 16 && ext.chars().all(|c| c.is_ascii_alphanumeric());
        if ext_ok && !stem.trim().is_empty() {
            let stem = sanitize_capped(stem, MAX_TITLE_CHARS - ext.len() - 1);
            return format!("{stem}.{ext}");
        }
    }
    sanitize_title(Some(name))
}

fn sanitize_capped(raw: &str, cap: usize) -> String {
    // 1. Nettoyage caractère par caractère.
    let cleaned: String = raw
        .chars()
        .filter_map(|c| match c {
            '/' | '\\' => Some(' '),                                  // texte, pas séparateur
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => None,         // interdits Windows
            c if (c as u32) < 0x20 || c == '\u{7f}' => None,        // caractères de contrôle (C0 + DEL)
            c => Some(c),
        })
        .collect();
    // 2. Réduire les espaces multiples + trim.
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    // 3. Plafonner la longueur (frontières de caractères).
    let capped: String = collapsed.chars().take(cap).collect();
    // 4. Couper points et espaces de fin (interdits/problématiques sous Windows).
    let trimmed = capped.trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        return DEFAULT_TITLE.to_string();
    }
    // 5. Neutraliser les noms réservés.
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(&trimmed)) {
        return format!("_{trimmed}");
    }
    trimmed
}

/// Choisit le nom de fichier final : nom suggéré (basename, non vide) sinon nom du probe
/// (non vide) sinon `download.bin`.
pub fn choose_file_name(suggested: Option<&str>, probe: Option<&str>) -> String {
    if let Some(name) = suggested.map(basename).filter(|s| !s.is_empty()) {
        return name;
    }
    if let Some(name) = probe.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        return name;
    }
    "download.bin".to_string()
}

/// Ajoute `.ext` au nom, sauf s'il se termine déjà par `.ext` (insensible à la casse).
/// Une autre extension média connue (`mp4`, `mp3`, `m4a`, `webm`, `opus`, `mkv`) est
/// remplacée : `x.mp4` en MP3 → `x.mp3`.
///
/// Ne tronque PAS un nom contenant d'autres points :
/// un titre comme `Partie 2.5` conserve son point (→ `Partie 2.5.mp4`).
pub fn ensure_extension(name: &str, ext: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(&format!(".{}", ext.to_ascii_lowercase())) {
        return name.to_string();
    }
    let stem = MEDIA_EXTENSIONS
        .iter()
        .find(|m| lower.ends_with(&format!(".{m}")) && name.len() > m.len() + 1)
        .map(|m| &name[..name.len() - m.len() - 1])
        .unwrap_or(name);
    format!("{stem}.{ext}")
}

/// Dernier segment de chemin (séparateurs `/` ou `\`), espaces retirés.
fn basename(s: &str) -> String {
    s.rsplit(['/', '\\']).next().unwrap_or(s).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_suggested_when_present() {
        assert_eq!(choose_file_name(Some("video.mp4"), Some("download.bin")), "video.mp4");
    }

    #[test]
    fn takes_basename_of_suggested() {
        assert_eq!(choose_file_name(Some("sub/dir/file.zip"), None), "file.zip");
        assert_eq!(choose_file_name(Some("a\\b\\file.iso"), None), "file.iso");
    }

    #[test]
    fn falls_back_to_probe_when_suggested_empty_or_blank() {
        assert_eq!(choose_file_name(Some(""), Some("fromprobe.iso")), "fromprobe.iso");
        assert_eq!(choose_file_name(Some("   "), Some("x.bin")), "x.bin");
        assert_eq!(choose_file_name(None, Some("p.zip")), "p.zip");
        // Nom suggéré réduit à des séparateurs ⇒ basename vide ⇒ on retombe sur le probe.
        assert_eq!(choose_file_name(Some("/"), Some("probe.zip")), "probe.zip");
    }

    #[test]
    fn falls_back_to_default_when_nothing_usable() {
        assert_eq!(choose_file_name(None, None), "download.bin");
        assert_eq!(choose_file_name(Some(""), Some("")), "download.bin");
    }

    #[test]
    fn sanitize_title_empty_or_none_is_video() {
        assert_eq!(sanitize_title(None), "video");
        assert_eq!(sanitize_title(Some("")), "video");
        assert_eq!(sanitize_title(Some("   ")), "video");
    }

    #[test]
    fn sanitize_title_removes_illegal_chars() {
        assert_eq!(sanitize_title(Some("a:b?c*")), "abc");
        assert_eq!(sanitize_title(Some("a<b>c|d\"e")), "abcde");
    }

    #[test]
    fn sanitize_title_slash_is_not_a_path_separator() {
        // Anti-régression : '/' et '\' deviennent des espaces, jamais des séparateurs.
        assert_eq!(sanitize_title(Some("AC/DC - Live")), "AC DC - Live");
        assert_eq!(sanitize_title(Some("a\\b")), "a b");
    }

    #[test]
    fn sanitize_title_reserved_names_are_prefixed() {
        assert_eq!(sanitize_title(Some("CON")), "_CON");
        assert_eq!(sanitize_title(Some("nul")), "_nul");
        assert_eq!(sanitize_title(Some("COM1")), "_COM1");
    }

    #[test]
    fn sanitize_title_trims_trailing_dots_and_spaces() {
        assert_eq!(sanitize_title(Some("titre...  ")), "titre");
        assert_eq!(sanitize_title(Some("  un titre  ")), "un titre");
    }

    #[test]
    fn sanitize_title_caps_length() {
        let long = "a".repeat(300);
        let out = sanitize_title(Some(&long));
        assert_eq!(out.chars().count(), 120);
    }

    #[test]
    fn sanitize_title_keeps_unicode() {
        assert_eq!(sanitize_title(Some("Café 日本")), "Café 日本");
    }

    #[test]
    fn sanitize_title_cap_trailing_dot_is_trimmed() {
        // 119 'a' + '.' = 120 chars entrant dans take(120) ; après trim, pas de point final.
        let s = format!("{}.", "a".repeat(119));
        let out = sanitize_title(Some(&s));
        assert_eq!(out.chars().count(), 119);
        assert!(!out.ends_with('.'));
    }

    #[test]
    fn ensure_extension_appends_when_missing() {
        assert_eq!(ensure_extension("video", "mp4"), "video.mp4");
        assert_eq!(ensure_extension("Partie 2.5", "mp4"), "Partie 2.5.mp4");
        assert_eq!(ensure_extension("a.b.m3u8", "mp4"), "a.b.m3u8.mp4");
    }

    #[test]
    fn ensure_extension_keeps_existing_case_insensitive() {
        assert_eq!(ensure_extension("clip.mp4", "mp4"), "clip.mp4");
        assert_eq!(ensure_extension("clip.MP4", "mp4"), "clip.MP4");
    }

    #[test]
    fn ensure_extension_replaces_other_media_extension() {
        assert_eq!(ensure_extension("x.mp4", "mp3"), "x.mp3");
        assert_eq!(ensure_extension("Mon clip.MP4", "m4a"), "Mon clip.m4a");
        assert_eq!(ensure_extension("a.webm", "mp4"), "a.mp4");
        assert_eq!(ensure_extension("a.mkv", "mp4"), "a.mp4");
        // Extension non média : complétée, pas remplacée.
        assert_eq!(ensure_extension("notes.txt", "mp3"), "notes.txt.mp3");
    }

    #[test]
    fn sanitize_file_name_keeps_extension_when_truncating() {
        let long = format!("{}.iso", "a".repeat(196));
        assert_eq!(long.chars().count(), 200);
        let out = sanitize_file_name(&long);
        assert!(out.ends_with(".iso"), "{out}");
        assert_eq!(out.chars().count(), 120);
        assert_eq!(out, format!("{}.iso", "a".repeat(116)));
    }

    #[test]
    fn sanitize_file_name_short_names() {
        assert_eq!(sanitize_file_name("a.iso"), "a.iso");
        assert_eq!(sanitize_file_name("a:b?.zip"), "ab.zip");
        assert_eq!(sanitize_file_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_file_name("sans extension"), "sans extension");
        assert_eq!(sanitize_file_name(".bashrc"), ".bashrc");
        assert_eq!(sanitize_file_name(""), "video");
    }

    #[test]
    fn sanitize_title_all_illegal_is_video() {
        assert_eq!(sanitize_title(Some(":?*")), "video");
        assert_eq!(sanitize_title(Some("///")), "video");
    }
}
