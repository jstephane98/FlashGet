use crate::model::{Download, DownloadStatus, Priority};

/// Étant donné l'ensemble des téléchargements et la limite de simultanés,
/// renvoie les ids des téléchargements `Queued` à démarrer, triés par
/// (priorité décroissante, rang de file, ordre du slice).
/// `forced` (démarrage manuel) passe en premier s'il est `Queued`.
/// La suspension de file n'est pas un filtre ici : « Suspendre la file » remet les `queued`
/// en attente (`held`) et ce qui est `queued` ensuite (« Télécharger », reprise) démarre.
/// Idempotent : ne renvoie jamais un id déjà `Downloading` / `Recording` / `Paused` / terminé.
pub fn pick_to_start(
    downloads: &[Download],
    max_concurrent: u32,
    forced: Option<&str>,
) -> Vec<String> {
    let active = downloads
        .iter()
        .filter(|d| matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Recording))
        .count() as u32;
    let slots = max_concurrent.saturating_sub(active);
    if slots == 0 {
        return Vec::new();
    }
    let forced_id = forced.filter(|f| {
        downloads.iter().any(|d| d.id == *f && d.status == DownloadStatus::Queued)
    });
    let mut queued: Vec<&Download> = downloads
        .iter()
        .filter(|d| d.status == DownloadStatus::Queued && Some(d.id.as_str()) != forced_id)
        .collect();
    // Tri STABLE : la position d'origine départage les égalités complètes.
    queued.sort_by_key(|d| (priority_rank(d.priority), d.queue_rank.unwrap_or(i64::MAX)));
    forced_id
        .map(|f| f.to_string())
        .into_iter()
        .chain(queued.into_iter().map(|d| d.id.clone()))
        .take(slots as usize)
        .collect()
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
            thumbnail_url: None,
            duration_s: None,
            created_at: None,
            started_at: None,
            completed_at: None,
            queue_rank: None,
        }
    }

    #[test]
    fn starts_up_to_the_limit() {
        let items = vec![
            dl("a", DownloadStatus::Queued, Priority::Normal),
            dl("b", DownloadStatus::Queued, Priority::Normal),
            dl("c", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 2, None), vec!["a", "b"]);
    }

    #[test]
    fn counts_active_against_the_limit() {
        let items = vec![
            dl("x", DownloadStatus::Downloading, Priority::Normal),
            dl("a", DownloadStatus::Queued, Priority::Normal),
            dl("b", DownloadStatus::Queued, Priority::Normal),
        ];
        // 1 actif, limite 2 => 1 slot libre => démarre seulement "a".
        assert_eq!(pick_to_start(&items, 2, None), vec!["a"]);
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
        assert_eq!(pick_to_start(&items, 3, None), vec!["b", "d", "c"]);
    }

    #[test]
    fn ignores_paused_completed_error() {
        let items = vec![
            dl("p", DownloadStatus::Paused, Priority::High),
            dl("done", DownloadStatus::Completed, Priority::High),
            dl("err", DownloadStatus::Error, Priority::High),
            dl("a", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 5, None), vec!["a"]);
    }

    #[test]
    fn recording_counts_as_active_slot() {
        // 1 enregistrement actif + 1 en file, max=1 => rien à démarrer.
        let downloads = vec![
            dl("rec", DownloadStatus::Recording, Priority::Normal),
            dl("q", DownloadStatus::Queued, Priority::Normal),
        ];
        assert!(pick_to_start(&downloads, 1, None).is_empty());
    }

    #[test]
    fn never_starts_held() {
        let items = vec![
            dl("h", DownloadStatus::Held, Priority::High),
            dl("q", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 3, None), vec!["q"]);
    }

    fn ranked(id: &str, priority: Priority, rank: i64) -> Download {
        let mut d = dl(id, DownloadStatus::Queued, priority);
        d.queue_rank = Some(rank);
        d
    }

    #[test]
    fn rank_breaks_ties_within_priority() {
        let items = vec![
            ranked("n5", Priority::Normal, 5),
            ranked("n2", Priority::Normal, 2),
            ranked("h9", Priority::High, 9),
        ];
        assert_eq!(pick_to_start(&items, 3, None), vec!["h9", "n2", "n5"]);
    }

    #[test]
    fn forced_is_picked_but_never_a_held() {
        let items = vec![
            dl("x", DownloadStatus::Queued, Priority::Normal),
            dl("y", DownloadStatus::Queued, Priority::Normal),
            dl("h", DownloadStatus::Held, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 1, Some("y")), vec!["y"]);
        assert_eq!(pick_to_start(&items, 3, Some("h")), vec!["x", "y"]);
    }

    #[test]
    fn suspended_queue_still_starts_new_queued_items() {
        // Après « Suspendre la file » : les anciens en attente sont `held`, les actifs `paused`.
        // Un nouvel élément « Télécharger » (`queued`) démarre ; rien d'autre.
        let items = vec![
            dl("h1", DownloadStatus::Held, Priority::High),
            dl("p1", DownloadStatus::Paused, Priority::High),
            dl("new", DownloadStatus::Queued, Priority::Low),
        ];
        assert_eq!(pick_to_start(&items, 3, None), vec!["new"]);
    }

    #[test]
    fn resume_all_after_suspend_starts_paused_items() {
        // « Tout reprendre » repasse les `paused` en `queued` : ils sont élus, pas les `held`.
        let items = vec![
            dl("h1", DownloadStatus::Held, Priority::High),
            dl("p1", DownloadStatus::Queued, Priority::Normal),
            dl("p2", DownloadStatus::Queued, Priority::Normal),
        ];
        assert_eq!(pick_to_start(&items, 3, None), vec!["p1", "p2"]);
    }

    #[test]
    fn forced_goes_first() {
        let items = vec![
            dl("a", DownloadStatus::Queued, Priority::High),
            dl("f", DownloadStatus::Queued, Priority::Low),
        ];
        assert_eq!(pick_to_start(&items, 1, Some("f")), vec!["f"]);
    }

    #[test]
    fn no_slots_returns_empty() {
        let items = vec![
            dl("x", DownloadStatus::Downloading, Priority::Normal),
            dl("y", DownloadStatus::Downloading, Priority::Normal),
            dl("a", DownloadStatus::Queued, Priority::High),
        ];
        assert_eq!(pick_to_start(&items, 2, None), Vec::<String>::new());
    }
}
