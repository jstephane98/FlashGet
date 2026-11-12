//! Langue des messages affichés à l'utilisateur (erreurs, notifications, menu).
//! L'interface transmet sa langue au démarrage et à chaque changement ; le français reste
//! la langue par défaut.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn set_english(english: bool) {
    ENGLISH.store(english, Ordering::Relaxed);
}

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// Choix pur entre les deux versions, sans lire l'état global.
pub fn pick<'a>(english: bool, fr: &'a str, en: &'a str) -> &'a str {
    if english { en } else { fr }
}

/// Texte simple : version française ou anglaise selon la langue courante.
pub fn tr(fr: &'static str, en: &'static str) -> &'static str {
    pick(is_english(), fr, en)
}

/// Comme `format!`, avec une version française et une version anglaise du modèle.
#[macro_export]
macro_rules! trf {
    ($fr:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_english() { format!($en $(, $arg)*) } else { format!($fr $(, $arg)*) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Les tests ne changent jamais la langue globale : ils tournent en parallèle avec
    // d'autres qui vérifient des messages en français.
    #[test]
    fn choisit_la_version() {
        assert_eq!(pick(false, "Fichier", "File"), "Fichier");
        assert_eq!(pick(true, "Fichier", "File"), "File");
    }

    #[test]
    fn francais_par_defaut() {
        assert!(!is_english());
        assert_eq!(tr("Fichier", "File"), "Fichier");
        let n = 3;
        assert_eq!(crate::trf!("{n} fichiers", "{n} files"), "3 fichiers");
        assert_eq!(crate::trf!("{} fichiers", "{} files", n), "3 fichiers");
    }
}
