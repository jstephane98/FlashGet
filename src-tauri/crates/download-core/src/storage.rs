use crate::model::{Download, DownloadStatus};
use crate::CoreError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

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
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn upsert(&self, d: &Download) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads (id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                url=?2, file_name=?3, dest_path=?4, total_bytes=?5,
                downloaded_bytes=?6, status=?7, supports_ranges=?8, error=?9",
            rusqlite::params![
                d.id, d.url, d.file_name, d.dest_path,
                d.total_bytes.map(|v| v as i64),
                d.downloaded_bytes as i64,
                status_to_str(d.status),
                d.supports_ranges as i64,
                d.error,
            ],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Download>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, file_name, dest_path, total_bytes, downloaded_bytes, status, supports_ranges, error FROM downloads")
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
            "UPDATE downloads SET downloaded_bytes=?1, status=?2 WHERE id=?3",
            rusqlite::params![downloaded_bytes as i64, status_to_str(status), id],
        ).map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
}

fn status_to_str(s: DownloadStatus) -> &'static str {
    match s {
        DownloadStatus::Queued => "queued",
        DownloadStatus::Downloading => "downloading",
        DownloadStatus::Paused => "paused",
        DownloadStatus::Completed => "completed",
        DownloadStatus::Error => "error",
    }
}

fn status_from_str(s: &str) -> DownloadStatus {
    match s {
        "downloading" => DownloadStatus::Downloading,
        "paused" => DownloadStatus::Paused,
        "completed" => DownloadStatus::Completed,
        "error" => DownloadStatus::Error,
        _ => DownloadStatus::Queued,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
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
}
