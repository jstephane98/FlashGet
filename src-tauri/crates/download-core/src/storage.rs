use crate::model::{Download, DownloadKind, DownloadStatus, Priority, Settings, Category, CategoryDirs};
use crate::CoreError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// Clé du jeton du bridge navigateur dans la table key/value `settings`.
const BRIDGE_TOKEN_KEY: &str = "bridge.token";

/// Secondes Unix courantes.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let conn = Connection::open(path).map_err(|e| CoreError::Storage(e.to_string()))?;
        Self::from_conn(conn)
    }

    pub fn open_in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory().map_err(|e| CoreError::Storage(e.to_string()))?;
        Self::from_conn(conn)
    }

    fn from_conn(conn: Connection) -> Result<Self, CoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                file_name TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                total_bytes INTEGER,
                downloaded_bytes INTEGER NOT NULL,
                status TEXT NOT NULL,
                supports_ranges INTEGER NOT NULL,
                error TEXT
            );",
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        // Migration additive : colonne priority (ignorer l'erreur si déjà présente).
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal'",
            [],
        );
        // Migration additive : colonne category (ignorer l'erreur si déjà présente).
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN category TEXT NOT NULL DEFAULT 'other'",
            [],
        );
        // Migration additive : en-têtes optionnels (3b-1). Nullable, ignorer si déjà présent.
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN referrer TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN cookie TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN user_agent TEXT", []);
        // Migration additive Phase 4a : type de download + URL audio HLS.
        let _ = conn.execute(
            "ALTER TABLE downloads ADD COLUMN kind TEXT NOT NULL DEFAULT 'regular'",
            [],
        );
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN audio_url TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN media_selection TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN thumbnail_url TEXT", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN duration_s INTEGER", []);
        // Horodatage (secondes Unix), nullable pour les anciennes lignes.
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN created_at INTEGER", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN started_at INTEGER", []);
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN completed_at INTEGER", []);
        // Rang dans la file (départage à priorité égale), nullable pour les anciennes lignes.
        let _ = conn.execute("ALTER TABLE downloads ADD COLUMN queue_rank INTEGER", []);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn upsert(&self, d: &Download) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent, kind, audio_url, media_selection, thumbnail_url, duration_s, created_at, started_at, completed_at, queue_rank)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, COALESCE(?20, ?23), ?21, ?22, COALESCE(?24, (SELECT COALESCE(MAX(queue_rank), 0) + 1 FROM downloads)))
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9, priority=?10, category=?11,
                referrer=?12, cookie=?13, user_agent=?14, kind=?15, audio_url=?16, media_selection=?17, thumbnail_url=?18, duration_s=?19,
                created_at=COALESCE(created_at, ?20, ?23),
                started_at=COALESCE(?21, started_at), completed_at=COALESCE(?22, completed_at),
                queue_rank=COALESCE(queue_rank, ?24, (SELECT COALESCE(MAX(queue_rank), 0) + 1 FROM downloads))",
            rusqlite::params![
                d.id, d.url, d.file_name, d.dest_path,
                d.total_bytes.map(|v| v as i64),
                d.downloaded_bytes as i64,
                status_to_str(d.status),
                d.supports_ranges as i64,
                d.error,
                priority_to_str(d.priority),
                category_to_str(d.category),
                d.referrer,
                d.cookie,
                d.user_agent,
                kind_to_str(d.kind),
                d.audio_url,
                d.media_selection,
                d.thumbnail_url,
                d.duration_s.map(|v| v as i64),
                d.created_at,
                d.started_at,
                d.completed_at,
                now_secs(),
                d.queue_rank,
            ],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Download>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent, kind, audio_url, media_selection, thumbnail_url, duration_s, created_at, started_at, completed_at, queue_rank FROM downloads ORDER BY rowid")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Download {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    dest_path: row.get(3)?,
                    total_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    downloaded_bytes: row.get::<_, i64>(5)? as u64,
                    status: status_from_str(&row.get::<_, String>(6)?),
                    supports_ranges: row.get::<_, i64>(7)? != 0,
                    error: row.get(8)?,
                    priority: priority_from_str(&row.get::<_, String>(9)?),
                    category: category_from_str(&row.get::<_, String>(10)?),
                    referrer: row.get(11)?,
                    cookie: row.get(12)?,
                    user_agent: row.get(13)?,
                    kind: kind_from_str(&row.get::<_, String>(14)?),
                    audio_url: row.get(15)?,
                    media_selection: row.get(16)?,
                    thumbnail_url: row.get(17)?,
                    duration_s: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
                    created_at: row.get(19)?,
                    started_at: row.get(20)?,
                    completed_at: row.get(21)?,
                    queue_rank: row.get(22)?,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| CoreError::Storage(e.to_string()))?);
        }
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE id=?1", [id])
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Met à jour la progression et le statut d'un téléchargement existant.
    /// No-op (0 ligne affectée) si l'id n'existe plus (ex. annulé/supprimé).
    pub fn update_state(&self, id: &str, downloaded_bytes: u64, status: DownloadStatus) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_bytes=?1, status=?2,
                completed_at=CASE WHEN ?2='completed' THEN COALESCE(completed_at, ?4) ELSE completed_at END
             WHERE id=?3",
            rusqlite::params![downloaded_bytes as i64, status_to_str(status), id, now_secs()],
        ).map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Met à jour le nom de fichier et le chemin de destination (réconciliation après
    /// un téléchargement dont le conteneur réel diffère de l'extension demandée).
    pub fn update_paths(&self, id: &str, file_name: &str, dest_path: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET file_name=?2, dest_path=?3 WHERE id=?1",
            rusqlite::params![id, file_name, dest_path],
        ).map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Passe un téléchargement en `downloading` SANS écraser un statut terminal
    /// (completed/error). No-op si l'id n'existe pas ou est déjà terminé.
    pub fn mark_active(&self, id: &str, downloaded_bytes: u64) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_bytes=?1, status='downloading', started_at=COALESCE(started_at, ?3) WHERE id=?2 AND status NOT IN ('completed','error')",
            rusqlite::params![downloaded_bytes as i64, id, now_secs()],
        ).map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Marque un téléchargement en erreur (no-op si l'id n'existe plus).
    pub fn mark_error(&self, id: &str, downloaded_bytes: u64, message: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET downloaded_bytes=?1, status='error', error=?2 WHERE id=?3",
            rusqlite::params![downloaded_bytes as i64, message, id],
        ).map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Download>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error, priority, category, referrer, cookie, user_agent, kind, audio_url, media_selection, thumbnail_url, duration_s, created_at, started_at, completed_at, queue_rank FROM downloads WHERE id=?1")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut rows = stmt
            .query_map([id], |row| {
                Ok(Download {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    file_name: row.get(2)?,
                    dest_path: row.get(3)?,
                    total_bytes: row.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    downloaded_bytes: row.get::<_, i64>(5)? as u64,
                    status: status_from_str(&row.get::<_, String>(6)?),
                    supports_ranges: row.get::<_, i64>(7)? != 0,
                    error: row.get(8)?,
                    priority: priority_from_str(&row.get::<_, String>(9)?),
                    category: category_from_str(&row.get::<_, String>(10)?),
                    referrer: row.get(11)?,
                    cookie: row.get(12)?,
                    user_agent: row.get(13)?,
                    kind: kind_from_str(&row.get::<_, String>(14)?),
                    audio_url: row.get(15)?,
                    media_selection: row.get(16)?,
                    thumbnail_url: row.get(17)?,
                    duration_s: row.get::<_, Option<i64>>(18)?.map(|v| v as u64),
                    created_at: row.get(19)?,
                    started_at: row.get(20)?,
                    completed_at: row.get(21)?,
                    queue_rank: row.get(22)?,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| CoreError::Storage(e.to_string()))?)),
            None => Ok(None),
        }
    }

    pub fn set_priority(&self, id: &str, priority: Priority) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET priority=?1 WHERE id=?2",
            rusqlite::params![priority_to_str(priority), id],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn category_dir(&self, category: Category) -> Result<Option<String>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let key = format!("dest.{}", category_to_str(category));
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key.as_str()], |r| r.get::<_, String>(0))
            .ok()
            .filter(|s| !s.is_empty()))
    }

    pub fn get_category_dirs(&self) -> Result<CategoryDirs, CoreError> {
        Ok(CategoryDirs {
            video: self.category_dir(Category::Video)?,
            document: self.category_dir(Category::Document)?,
            program: self.category_dir(Category::Program)?,
            archive: self.category_dir(Category::Archive)?,
            music: self.category_dir(Category::Music)?,
            other: self.category_dir(Category::Other)?,
        })
    }

    pub fn set_category_dir(&self, category: Category, path: Option<String>) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let key = format!("dest.{}", category_to_str(category));
        let val = path.unwrap_or_default();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params![key, val],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Jeton du bridge navigateur (persisté en `settings`, clé `bridge.token`).
    /// Généré fort au premier appel, puis stable.
    pub fn get_or_create_bridge_token(&self) -> Result<String, CoreError> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key=?1",
                [BRIDGE_TOKEN_KEY],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .filter(|s| !s.is_empty());
        if let Some(tok) = existing {
            return Ok(tok);
        }
        let token = new_bridge_token();
        upsert_bridge_token(&conn, &token)?;
        Ok(token)
    }

    /// Régénère et persiste un nouveau jeton ; renvoie la nouvelle valeur.
    pub fn regenerate_bridge_token(&self) -> Result<String, CoreError> {
        let token = new_bridge_token();
        let conn = self.conn.lock().unwrap();
        upsert_bridge_token(&conn, &token)?;
        Ok(token)
    }

    pub fn clear_completed(&self) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE status='completed'", [])
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Repli yt-dlp `--cookies-from-browser` (clé `cookies_from_browser`). Vide = None.
    pub fn get_cookies_from_browser(&self) -> Result<Option<String>, CoreError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key=?1", ["cookies_from_browser"], |r| r.get::<_, String>(0))
            .ok()
            .filter(|s| !s.is_empty()))
    }

    pub fn set_cookies_from_browser(&self, value: Option<String>) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let val = value.unwrap_or_default();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params!["cookies_from_browser", val],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Notif système à la fin d'un téléchargement (clé `notify_on_complete`).
    /// Défaut : activé (true), sauf valeur stockée "0"/"off".
    pub fn get_notify_on_complete(&self) -> Result<bool, CoreError> {
        let conn = self.conn.lock().unwrap();
        let stored: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key=?1", ["notify_on_complete"], |r| r.get::<_, String>(0))
            .ok();
        Ok(!matches!(stored.as_deref(), Some("0") | Some("off")))
    }

    pub fn set_notify_on_complete(&self, value: bool) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let val = if value { "1" } else { "0" };
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params!["notify_on_complete", val],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Préférence d'interface générique (table `settings`). Absente = None.
    pub fn get_pref(&self, key: &str) -> Result<Option<String>, CoreError> {
        let conn = self.conn.lock().unwrap();
        match conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get::<_, String>(0)) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Storage(e.to_string())),
        }
    }

    pub fn set_pref(&self, key: &str, value: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params![key, value],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Renumérote la file : `queue_rank = 1..=n` dans l'ordre reçu, en une transaction.
    pub fn set_queue_order(&self, ids: &[String]) -> Result<(), CoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| CoreError::Storage(e.to_string()))?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE downloads SET queue_rank=?1 WHERE id=?2",
                rusqlite::params![(i + 1) as i64, id],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        }
        tx.commit().map_err(|e| CoreError::Storage(e.to_string()))
    }

    /// File suspendue (clé `queue.paused`) ; absente = faux.
    pub fn queue_paused(&self) -> Result<bool, CoreError> {
        Ok(self.get_pref("queue.paused")?.as_deref() == Some("1"))
    }

    pub fn set_queue_paused(&self, v: bool) -> Result<(), CoreError> {
        self.set_pref("queue.paused", if v { "1" } else { "0" })
    }

    pub fn get_settings(&self) -> Result<Settings, CoreError> {
        let conn = self.conn.lock().unwrap();
        let read = |key: &str| -> Option<String> {
            conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get::<_, String>(0)).ok()
        };
        let mut s = Settings::default();
        if let Some(v) = read("max_concurrent") {
            if let Ok(n) = v.parse::<u32>() { s.max_concurrent = n; }
        }
        if let Some(v) = read("max_speed_bps") {
            if let Ok(n) = v.parse::<u64>() { s.max_speed_bps = n; }
        }
        Ok(s)
    }

    pub fn set_settings(&self, s: &Settings) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let put = |key: &str, val: String| -> Result<(), CoreError> {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
                rusqlite::params![key, val],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;
            Ok(())
        };
        put("max_concurrent", s.max_concurrent.to_string())?;
        put("max_speed_bps", s.max_speed_bps.to_string())?;
        Ok(())
    }
}

fn status_to_str(s: DownloadStatus) -> &'static str {
    match s {
        DownloadStatus::Held => "held",
        DownloadStatus::Queued => "queued",
        DownloadStatus::Downloading => "downloading",
        DownloadStatus::Recording => "recording",
        DownloadStatus::Paused => "paused",
        DownloadStatus::Completed => "completed",
        DownloadStatus::Error => "error",
    }
}

fn status_from_str(s: &str) -> DownloadStatus {
    match s {
        "held" => DownloadStatus::Held,
        "downloading" => DownloadStatus::Downloading,
        "recording" => DownloadStatus::Recording,
        "paused" => DownloadStatus::Paused,
        "completed" => DownloadStatus::Completed,
        "error" => DownloadStatus::Error,
        _ => DownloadStatus::Queued,
    }
}

fn priority_to_str(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

fn priority_from_str(s: &str) -> Priority {
    match s {
        "high" => Priority::High,
        "low" => Priority::Low,
        _ => Priority::Normal,
    }
}

fn category_to_str(c: Category) -> &'static str {
    match c {
        Category::Video => "video",
        Category::Document => "document",
        Category::Program => "program",
        Category::Archive => "archive",
        Category::Music => "music",
        Category::Other => "other",
    }
}

fn category_from_str(s: &str) -> Category {
    match s {
        "video" => Category::Video,
        "document" => Category::Document,
        "program" => Category::Program,
        "archive" => Category::Archive,
        "music" => Category::Music,
        _ => Category::Other,
    }
}

fn kind_to_str(k: DownloadKind) -> &'static str {
    match k {
        DownloadKind::Regular => "regular",
        DownloadKind::Hls => "hls",
        DownloadKind::Dash => "dash",
        DownloadKind::Ytdlp => "ytdlp",
    }
}

fn kind_from_str(s: &str) -> DownloadKind {
    match s {
        "hls" => DownloadKind::Hls,
        "dash" => DownloadKind::Dash,
        "ytdlp" => DownloadKind::Ytdlp,
        _ => DownloadKind::Regular,
    }
}

/// Jeton aléatoire fort : deux UUID v4 concaténés en hexadécimal (64 caractères).
fn new_bridge_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Upsert le jeton du bridge dans la table `settings`.
fn upsert_bridge_token(conn: &Connection, token: &str) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
        rusqlite::params![BRIDGE_TOKEN_KEY, token],
    )
    .map_err(|e| CoreError::Storage(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pref_roundtrip_and_absent() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.get_pref("ui.theme").unwrap(), None);
        s.set_pref("ui.theme", "light").unwrap();
        s.set_pref("ui.theme", "system").unwrap();
        assert_eq!(s.get_pref("ui.theme").unwrap().as_deref(), Some("system"));
    }

    fn sample(id: &str) -> Download {
        Download {
            id: id.into(),
            url: "http://x/f".into(),
            file_name: "f.bin".into(),
            dest_path: "/tmp/f.bin".into(),
            total_bytes: Some(100),
            downloaded_bytes: 40,
            status: DownloadStatus::Paused,
            supports_ranges: true,
            error: None,
            priority: Priority::Normal,
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
    fn new_rows_get_increasing_queue_rank() {
        let s = Storage::open_in_memory().unwrap();
        for id in ["a", "b", "c"] {
            s.upsert(&sample(id)).unwrap();
        }
        let rank = |id: &str| s.get_by_id(id).unwrap().unwrap().queue_rank;
        assert_eq!((rank("a"), rank("b"), rank("c")), (Some(1), Some(2), Some(3)));
        s.upsert(&sample("b")).unwrap();
        assert_eq!(rank("b"), Some(2));
    }

    #[test]
    fn set_queue_order_renumbers() {
        let s = Storage::open_in_memory().unwrap();
        for id in ["a", "b", "c"] {
            s.upsert(&sample(id)).unwrap();
        }
        s.set_queue_order(&["c".to_string(), "a".to_string(), "b".to_string()]).unwrap();
        let rank = |id: &str| s.get_by_id(id).unwrap().unwrap().queue_rank;
        assert_eq!((rank("c"), rank("a"), rank("b")), (Some(1), Some(2), Some(3)));
        // un upsert ultérieur n'écrase pas le rang
        s.upsert(&sample("a")).unwrap();
        assert_eq!(rank("a"), Some(2));
    }

    #[test]
    fn queue_paused_roundtrip() {
        let s = Storage::open_in_memory().unwrap();
        assert!(!s.queue_paused().unwrap());
        s.set_queue_paused(true).unwrap();
        assert!(s.queue_paused().unwrap());
        s.set_queue_paused(false).unwrap();
        assert!(!s.queue_paused().unwrap());
    }

    #[test]
    fn upsert_sets_created_at_once() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        let first = s.get_by_id("a").unwrap().unwrap();
        assert!(first.created_at.is_some());
        s.upsert(&first).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().created_at, first.created_at);
        let mut none = first.clone();
        none.created_at = None;
        s.upsert(&none).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().created_at, first.created_at);
    }

    #[test]
    fn mark_active_sets_started_at_once() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().started_at, None);
        s.mark_active("a", 10).unwrap();
        let t1 = s.get_by_id("a").unwrap().unwrap().started_at;
        assert!(t1.is_some());
        {
            let conn = s.conn.lock().unwrap();
            conn.execute("UPDATE downloads SET started_at=5 WHERE id='a'", []).unwrap();
        }
        s.mark_active("a", 20).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().started_at, Some(5));
    }

    #[test]
    fn completion_sets_completed_at() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        s.update_state("a", 50, DownloadStatus::Paused).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().completed_at, None);
        s.update_state("a", 100, DownloadStatus::Completed).unwrap();
        assert!(s.get_by_id("a").unwrap().unwrap().completed_at.is_some());
    }

    #[test]
    fn legacy_rows_have_no_dates() {
        let s = Storage::open_in_memory().unwrap();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO downloads (id, url, file_name, dest_path, downloaded_bytes, status, supports_ranges) VALUES ('old','u','f','/f',0,'paused',1)",
                [],
            ).unwrap();
        }
        let d = s.get_by_id("old").unwrap().unwrap();
        assert_eq!((d.created_at, d.started_at, d.completed_at), (None, None, None));
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn upsert_then_list() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].downloaded_bytes, 40);
        assert_eq!(all[0].status, DownloadStatus::Paused);
    }

    #[test]
    fn upsert_updates_existing() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        let mut d = sample("a");
        d.downloaded_bytes = 100;
        d.status = DownloadStatus::Completed;
        s.upsert(&d).unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].downloaded_bytes, 100);
        assert_eq!(all[0].status, DownloadStatus::Completed);
    }

    #[test]
    fn delete_removes_row() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        s.delete("a").unwrap();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn update_state_updates_existing() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("b")).unwrap();
        s.update_state("b", 99, DownloadStatus::Completed).unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].downloaded_bytes, 99);
        assert_eq!(all[0].status, DownloadStatus::Completed);
    }

    #[test]
    fn update_state_noop_when_absent() {
        let s = Storage::open_in_memory().unwrap();
        s.update_state("ghost", 5, DownloadStatus::Completed).unwrap();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn mark_error_sets_status_and_message() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("e")).unwrap();
        s.mark_error("e", 42, "boom").unwrap();
        let all = s.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].downloaded_bytes, 42);
        assert_eq!(all[0].status, DownloadStatus::Error);
        assert_eq!(all[0].error, Some("boom".to_string()));
    }

    #[test]
    fn mark_error_noop_when_absent() {
        let s = Storage::open_in_memory().unwrap();
        s.mark_error("ghost", 5, "boom").unwrap();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn round_trips_priority() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("p");
        d.priority = Priority::High;
        s.upsert(&d).unwrap();
        assert_eq!(s.list().unwrap()[0].priority, Priority::High);
    }

    #[test]
    fn set_priority_updates_row() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("a")).unwrap();
        s.set_priority("a", Priority::Low).unwrap();
        assert_eq!(s.get_by_id("a").unwrap().unwrap().priority, Priority::Low);
    }

    #[test]
    fn get_by_id_absent_is_none() {
        let s = Storage::open_in_memory().unwrap();
        assert!(s.get_by_id("nope").unwrap().is_none());
    }

    #[test]
    fn settings_default_then_roundtrip() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.get_settings().unwrap(), Settings::default());
        s.set_settings(&Settings { max_concurrent: 5, max_speed_bps: 1024 }).unwrap();
        assert_eq!(s.get_settings().unwrap(), Settings { max_concurrent: 5, max_speed_bps: 1024 });
    }

    #[test]
    fn list_is_ordered_by_insertion() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("first")).unwrap();
        s.upsert(&sample("second")).unwrap();
        let ids: Vec<String> = s.list().unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["first", "second"]);
    }

    #[test]
    fn mark_active_sets_downloading() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("ma1")).unwrap(); // starts as Paused
        s.mark_active("ma1", 10).unwrap();
        let d = s.get_by_id("ma1").unwrap().unwrap();
        assert_eq!(d.status, DownloadStatus::Downloading);
        assert_eq!(d.downloaded_bytes, 10);
    }

    #[test]
    fn mark_active_noop_when_completed() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("ma2")).unwrap();
        s.update_state("ma2", 50, DownloadStatus::Completed).unwrap();
        s.mark_active("ma2", 99).unwrap();
        let d = s.get_by_id("ma2").unwrap().unwrap();
        // status must remain Completed, not flipped back to Downloading
        assert_eq!(d.status, DownloadStatus::Completed);
        assert_eq!(d.downloaded_bytes, 50);
    }

    #[test]
    fn round_trips_category() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("c");
        d.category = Category::Video;
        s.upsert(&d).unwrap();
        assert_eq!(s.list().unwrap()[0].category, Category::Video);
        assert_eq!(s.get_by_id("c").unwrap().unwrap().category, Category::Video);
    }

    #[test]
    fn category_dir_roundtrip_and_default_none() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.category_dir(Category::Video).unwrap(), None);
        s.set_category_dir(Category::Video, Some("C:/Vids".to_string())).unwrap();
        assert_eq!(s.category_dir(Category::Video).unwrap(), Some("C:/Vids".to_string()));
        // Chaîne vide ⇒ None (revient au défaut).
        s.set_category_dir(Category::Video, None).unwrap();
        assert_eq!(s.category_dir(Category::Video).unwrap(), None);
    }

    #[test]
    fn get_category_dirs_reflects_sets() {
        let s = Storage::open_in_memory().unwrap();
        s.set_category_dir(Category::Music, Some("C:/Music".to_string())).unwrap();
        let dirs = s.get_category_dirs().unwrap();
        assert_eq!(dirs.music, Some("C:/Music".to_string()));
        assert_eq!(dirs.video, None);
    }

    #[test]
    fn clear_completed_removes_only_completed() {
        let s = Storage::open_in_memory().unwrap();
        let mut a = sample("a");
        a.status = DownloadStatus::Completed;
        s.upsert(&a).unwrap();
        let mut b = sample("b");
        b.status = DownloadStatus::Downloading;
        s.upsert(&b).unwrap();
        s.clear_completed().unwrap();
        let ids: Vec<String> = s.list().unwrap().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["b"]);
    }

    #[test]
    fn bridge_token_is_stable_and_non_empty() {
        let s = Storage::open_in_memory().unwrap();
        let a = s.get_or_create_bridge_token().unwrap();
        let b = s.get_or_create_bridge_token().unwrap();
        assert!(!a.is_empty());
        assert_eq!(a.len(), 64, "le jeton doit faire 64 caractères hex");
        assert_eq!(a, b, "le jeton doit être stable entre appels");
    }

    #[test]
    fn regenerate_bridge_token_changes_value() {
        let s = Storage::open_in_memory().unwrap();
        let a = s.get_or_create_bridge_token().unwrap();
        let b = s.regenerate_bridge_token().unwrap();
        assert_ne!(a, b, "régénérer doit changer le jeton");
        assert!(!b.is_empty());
        // Le nouveau jeton est désormais celui qui persiste.
        assert_eq!(b, s.get_or_create_bridge_token().unwrap());
    }

    #[test]
    fn round_trips_dash_and_media_selection() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("d1");
        d.kind = DownloadKind::Dash;
        d.media_selection = Some(r#"{"video":0,"audio":"en"}"#.into());
        s.upsert(&d).unwrap();
        let got = s.get_by_id("d1").unwrap().unwrap();
        assert_eq!(got.kind, DownloadKind::Dash);
        assert_eq!(got.media_selection.as_deref(), Some(r#"{"video":0,"audio":"en"}"#));
        let plain = sample("d2");
        s.upsert(&plain).unwrap();
        assert_eq!(s.get_by_id("d2").unwrap().unwrap().media_selection, None);
    }

    #[test]
    fn status_recording_round_trips() {
        assert_eq!(super::status_from_str(super::status_to_str(DownloadStatus::Recording)), DownloadStatus::Recording);
        assert_eq!(super::status_to_str(DownloadStatus::Recording), "recording");
    }

    #[test]
    fn round_trips_kind_and_audio_url() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("k1");
        d.kind = DownloadKind::Hls;
        d.audio_url = Some("https://h/a.m3u8".into());
        s.upsert(&d).unwrap();
        let got = s.get_by_id("k1").unwrap().unwrap();
        assert_eq!(got.kind, DownloadKind::Hls);
        assert_eq!(got.audio_url.as_deref(), Some("https://h/a.m3u8"));
        // Défaut : Regular + None.
        let plain = sample("k2");
        s.upsert(&plain).unwrap();
        let got2 = s.get_by_id("k2").unwrap().unwrap();
        assert_eq!(got2.kind, DownloadKind::Regular);
        assert_eq!(got2.audio_url, None);
    }

    #[test]
    fn update_paths_updates_name_and_dest() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("p1")).unwrap();
        s.update_paths("p1", "Réel.mkv", "C:/dl/Réel.mkv").unwrap();
        let d = s.get_by_id("p1").unwrap().unwrap();
        assert_eq!(d.file_name, "Réel.mkv");
        assert_eq!(d.dest_path, "C:/dl/Réel.mkv");
    }

    #[test]
    fn update_paths_leaves_status_and_bytes_untouched() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&sample("p")).unwrap();
        s.update_state("p", 1234, DownloadStatus::Completed).unwrap();
        s.update_paths("p", "Réel.mkv", "C:/dl/Réel.mkv").unwrap();
        let d = s.get_by_id("p").unwrap().unwrap();
        assert_eq!(d.file_name, "Réel.mkv");
        assert_eq!(d.dest_path, "C:/dl/Réel.mkv");
        assert_eq!(d.status, DownloadStatus::Completed);
        assert_eq!(d.downloaded_bytes, 1234);
    }

    #[test]
    fn update_paths_noop_when_absent() {
        let s = Storage::open_in_memory().unwrap();
        s.update_paths("ghost", "x", "y").unwrap();
        assert!(s.get_by_id("ghost").unwrap().is_none());
    }

    #[test]
    fn cookies_from_browser_roundtrip_and_default_none() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.get_cookies_from_browser().unwrap(), None);
        s.set_cookies_from_browser(Some("chrome".into())).unwrap();
        assert_eq!(s.get_cookies_from_browser().unwrap(), Some("chrome".into()));
        s.set_cookies_from_browser(None).unwrap();
        assert_eq!(s.get_cookies_from_browser().unwrap(), None);
    }

    #[test]
    fn notify_on_complete_roundtrip_and_default_on() {
        let s = Storage::open_in_memory().unwrap();
        // Défaut : activé.
        assert_eq!(s.get_notify_on_complete().unwrap(), true);
        // Désactivation persistée.
        s.set_notify_on_complete(false).unwrap();
        assert_eq!(s.get_notify_on_complete().unwrap(), false);
        // Réactivation persistée.
        s.set_notify_on_complete(true).unwrap();
        assert_eq!(s.get_notify_on_complete().unwrap(), true);
    }

    #[test]
    fn ytdlp_kind_roundtrips() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("y1");
        d.kind = DownloadKind::Ytdlp;
        s.upsert(&d).unwrap();
        let got = s.get_by_id("y1").unwrap().unwrap();
        assert_eq!(got.kind, DownloadKind::Ytdlp);
        // Défaut : Regular.
        let plain = sample("y2");
        s.upsert(&plain).unwrap();
        assert_eq!(s.get_by_id("y2").unwrap().unwrap().kind, DownloadKind::Regular);
    }

    #[test]
    fn held_and_media_fields_roundtrip() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("h1");
        d.status = DownloadStatus::Held;
        d.thumbnail_url = Some("https://i.ytimg.com/x.jpg".into());
        d.duration_s = Some(768);
        s.upsert(&d).unwrap();
        let back = s.get_by_id("h1").unwrap().unwrap();
        assert_eq!((back.status, back.thumbnail_url, back.duration_s), (DownloadStatus::Held, Some("https://i.ytimg.com/x.jpg".into()), Some(768)));
        let listed = s.list().unwrap();
        assert_eq!(listed[0].status, DownloadStatus::Held);
        assert_eq!(listed[0].duration_s, Some(768));
    }

    #[test]
    fn upsert_then_list_roundtrips_headers() {
        let s = Storage::open_in_memory().unwrap();
        let mut d = sample("h1");
        d.referrer = Some("http://example.com/page".into());
        d.cookie = Some("sid=abc; theme=dark".into());
        d.user_agent = Some("Mozilla/5.0 (Test)".into());
        s.upsert(&d).unwrap();

        let got = s.get_by_id("h1").unwrap().unwrap();
        assert_eq!(got.referrer.as_deref(), Some("http://example.com/page"));
        assert_eq!(got.cookie.as_deref(), Some("sid=abc; theme=dark"));
        assert_eq!(got.user_agent.as_deref(), Some("Mozilla/5.0 (Test)"));

        // Vérifier aussi le chemin `list` (même SELECT/mapping que get_by_id).
        let from_list = s.list().unwrap().into_iter().find(|x| x.id == "h1").unwrap();
        assert_eq!(from_list.cookie.as_deref(), Some("sid=abc; theme=dark"));

        // Absents => NULL => None.
        let plain = sample("h2");
        s.upsert(&plain).unwrap();
        let got2 = s.get_by_id("h2").unwrap().unwrap();
        assert_eq!(got2.referrer, None);
        assert_eq!(got2.cookie, None);
        assert_eq!(got2.user_agent, None);
    }
}
