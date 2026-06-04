use crate::model::{Download, DownloadStatus, Priority};

/// Étant donné l'ensemble des téléchargements et la limite de simultanés,
/// renvoie les ids des téléchargements `Queued` à démarrer, choisis par
/// priorité décroissante puis par ordre d'arrivée (l'ordre du slice).
/// Idempotent : ne renvoie jamais un id déjà `Downloading` / `Paused` / terminé.
pub fn pick_to_start(downloads: &[Download], max_concurrent: u32) -> Vec<String> {
    let active = downloads
        .iter()
        .filter(|d| d.status == DownloadStatus::Downloading)
        .count() as u32;
    let slots = max_concurrent.saturating_sub(active);
    if slots == 0 {
        return Vec::new();
    }
    let mut queued: Vec<&Download> = downloads
        .iter()
        .filter(|d| d.status == DownloadStatus::Queued)
        .collect();
    // Tri STABLE par rang de priorité : conserve l'ordre d'arrivée à priorité égale.
    queued.sort_by_key(|d| priority_rank(d.priority));
    queued.into_iter().take(slots as usize).map(|d| d.id.clone()).collect()
}

fn priority_rank(p: Priority) -> u8 {
    match p {
        Priority::High => 0,
        Priority::Normal => 1,
        Priority::Low => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, DownloadKind};

    fn dl(id: &str, status: DownloadStatus, priority: Priority) -> Download {
        Download {
            id: id.into(),
            url: "u".into(),
            file_name: "f".into(),
            dest_path: "/f".into(),
            total_bytes: Some(1),
            downloaded_bytes: 0,
            status,
            supports_ranges: true,
            error: None,
            priority,
            category: Category::Other,
            referrer: None,
            cookie: None,
            user_agent: None,
            kind: DownloadKind::Regular,
            audio_url: None,
            media_selection: None,
        }
    }

    #[test]
    fn starts_up_to_the_limit() {
        let items = vec![
            dl("a", DownloadStatus::Queued, Priority::Normal),
            dl("b", DownloadStatus::Queued, Priority::Normal),
            dl("c", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 2), vec!["a", "b"]);
    }

    #[test]
    fn counts_active_against_the_limit() {
        let items = vec![
            dl("x", DownloadStatus::Downloading, Priority::Normal),
            dl("a", DownloadStatus::Queued, Priority::Normal),
            dl("b", DownloadStatus::Queued, Priority::Normal),
        ];
        // 1 actif, limite 2 => 1 slot libre => démarre seulement "a".
        assert_eq!(pick_to_start(&items, 2), vec!["a"]);
    }

    #[test]
    fn higher_priority_first_then_fifo() {
        let items = vec![
            dl("a", DownloadStatus::Queued, Priority::Low),
            dl("b", DownloadStatus::Queued, Priority::High),
            dl("c", DownloadStatus::Queued, Priority::Normal),
            dl("d", DownloadStatus::Queued, Priority::High),
        ];
        // High d'abord (b avant d, FIFO), puis Normal (c), puis Low (a).
        assert_eq!(pick_to_start(&items, 3), vec!["b", "d", "c"]);
    }

    #[test]
    fn ignores_paused_completed_error() {
        let items = vec![
            dl("p", DownloadStatus::Paused, Priority::High),
            dl("done", DownloadStatus::Completed, Priority::High),
            dl("err", DownloadStatus::Error, Priority::High),
            dl("a", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 5), vec!["a"]);
    }

    #[test]
    fn no_slots_returns_empty() {
        let items = vec![
            dl("x", DownloadStatus::Downloading, Priority::Normal),
            dl("y", DownloadStatus::Downloading, Priority::Normal),
            dl("a", DownloadStatus::Queued, Priority::High),
        ];
        assert_eq!(pick_to_start(&items, 2), Vec::<String>::new());
    }
}
