use crate::model::Category;

/// Détermine la catégorie d'un fichier d'après son extension (insensible à la casse).
/// Sans extension ou extension inconnue ⇒ `Other`. Ne regarde que la dernière extension
/// (`archive.tar.gz` ⇒ `gz` ⇒ Archive).
pub fn categorize(file_name: &str) -> Category {
    let ext = match file_name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => return Category::Other,
    };
    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "mpg" | "mpeg" | "ts" => {
            Category::Video
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "odt" | "ods" | "csv"
        | "epub" | "rtf" => Category::Document,
        "exe" | "msi" | "dmg" | "pkg" | "deb" | "rpm" | "apk" | "appimage" => Category::Program,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" => Category::Archive,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "opus" => Category::Music,
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_category() {
        assert_eq!(categorize("film.mp4"), Category::Video);
        assert_eq!(categorize("rapport.pdf"), Category::Document);
        assert_eq!(categorize("setup.exe"), Category::Program);
        assert_eq!(categorize("data.zip"), Category::Archive);
        assert_eq!(categorize("song.mp3"), Category::Music);
    }

    #[test]
    fn unknown_and_missing_extension_is_other() {
        assert_eq!(categorize("file.xyz"), Category::Other);
        assert_eq!(categorize("noext"), Category::Other);
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(categorize("MOVIE.MP4"), Category::Video);
        assert_eq!(categorize("Archive.ZIP"), Category::Archive);
    }

    #[test]
    fn uses_last_extension_for_multi_dot() {
        assert_eq!(categorize("backup.tar.gz"), Category::Archive);
    }
}
