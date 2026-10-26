// this file manages the history in an SQLite database,,, i couldn't really think of a way to store
// the actual download history, so i looked into how other DMs do the same, and they also use SQLite
use crate::domain::download::Download;
use rusqlite::{Connection, params};
use std::{collections::HashSet, fs, path::Path, time::Duration};

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub last_seen_at: String,
    pub deleted_files: usize,
    pub failed_file_deletions: usize,
}

pub struct HistoryStore {
    connection: Connection,
}

impl HistoryStore {
    // this creates the history directory, checks db path, opens SQLite, secures permission (yeah
    // security guys, i am starting it) and enables WAL mode (took me some time to figure out why i
    // should be using this mode instead of just append or write)
    pub fn open(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .ok_or_else(|| String::from("History database has no parent directory"))?;

        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Could not create history directory {}: {error}",
                parent.display()
            )
        })?;

        secure_data_dir(parent)?;

        // hmmm these feel quite repetitive,,, the things under this,,, i'll look into making these
        // into maybe one function or something lile that,,, its getting boring wirting the same
        // exact things like is_dir, or etc.
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "History database must not be a symbolic link: {}",
                    path.display()
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(format!(
                    "History database is not a regular file: {}",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect history database {}: {error}",
                    path.display()
                ));
            }
        }

        let connection = Connection::open(path).map_err(|error| {
            format!(
                "Could not open history database {}: {error}",
                path.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
                format!(
                    "Could not secure history database {}: {error}",
                    path.display()
                )
            })?;
        }

        let store = Self::from_connection(connection)?;

        store
            .connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("Could not configure history database: {error}"))?;

        Ok(store)
    }

    // configues database timeout, and init table and index
    fn from_connection(connection: Connection) -> Result<Self, String> {
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|error| format!("Could not configure history database timeout: {error}"))?;

        connection.execute_batch("
            CREATE TABLE IF NOT EXISTS download_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                gid TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
                downloaded_bytes INTEGER NOT NULL CHECK (downloaded_bytes >= 0),
                speed_bytes_per_second INTEGER NOT NULL CHECK (speed_bytes_per_second >= 0),
                status TEXT NOT NULL,
                directory TEXT,
                source_url TEXT,
                error_message TEXT,
                deleted_files INTEGER NOT NULL DEFAULT 0,
                failed_file_deletions INTEGER NOT NULL DEFAULT 0,
                first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_download_history_last_seen ON download_history(last_seen_at DESC, id DESC);
        ").map_err(|error| format!("Could not initialize history schema: {error}"))?;

        Ok(Self { connection })
    }

    // inserts / adds new history records
    pub fn record_downloads(&mut self, downloads: &[Download]) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| format!("Could not start history transaction: {error}"))?;

        for download in downloads {
            let size = sql_integer("download size", download.size)?;
            let downloaded = sql_integer("downloaded bytes", download.downloaded)?;
            let speed = sql_integer("download speed", download.speed)?;

            transaction
                .execute(
                    "
                    INSERT INTO download_history (
                        gid,
                        name,
                        size_bytes,
                        downloaded_bytes,
                        speed_bytes_per_second,
                        status,
                        directory,
                        source_url,
                        error_message,
                        last_seen_at
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
                    ON CONFLICT(gid) DO UPDATE SET
                        name = excluded.name,
                        size_bytes = excluded.size_bytes,
                        downloaded_bytes = excluded.downloaded_bytes,
                        speed_bytes_per_second = excluded.speed_bytes_per_second,
                        status = excluded.status,
                        directory = excluded.directory,
                        source_url = excluded.source_url,
                        error_message = excluded.error_message,
                        last_seen_at = CURRENT_TIMESTAMP
                    ",
                    params![
                        download.gid.as_str(),
                        download.name.as_str(),
                        size,
                        downloaded,
                        speed,
                        download.status.label(),
                        download.directory.as_deref(),
                        download.source_url.as_deref(),
                        download.error_message.as_deref()
                    ],
                )
                .map_err(|error| {
                    format!(
                        "Could not save downlaod {} to history: {error}",
                        download.gid
                    )
                })?;
        }

        transaction
            .commit()
            .map_err(|error| format!("Could not commit history transaction: {error}"))
    }

    pub fn reconcile_snapshot(&mut self, downloads: &[Download]) -> Result<usize, String> {
        self.record_downloads(downloads)?;

        let live_gids: HashSet<&str> = downloads
            .iter()
            .map(|download| download.gid.as_str())
            .collect();

        let candidates = {
            let mut statement = self.connection.prepare("SELECT gid FROM download_history WHERE status IN ('Downloading', 'Queued', 'Paused')").map_err(|error| {format!("Could not prepare for history reconcilation: {error}")})?;

            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| {
                    format!("Could not query history reconcilation candidates: {error}")
                })?;

            rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
                format!("Could not read history reconcilation candidates: {error}")
            })?
        };

        let transaction = self.connection.transaction().map_err(|error| {
            format!("Could not start history reconcilation transaction: {error}")
        })?;

        let mut unavailable_count = 0;

        for gid in candidates {
            if live_gids.contains(gid.as_str()) {
                continue;
            }

            unavailable_count += transaction
                .execute(
                    "
                    UPDATE download_history
                    SET status = 'Unavailable',
                        speed_bytes_per_second = 0,
                        error_message = 'Not present in the current aria2 snapshot'
                    WHERE gid = ?1
                        AND status IN ('Downloading', 'Queued', 'Paused')
                    ",
                    params![gid],
                )
                .map_err(|error| {
                    format!("Could not reconcile history for a missing download: {error}")
                })?;
        }

        transaction
            .commit()
            .map_err(|error| format!("Could not commit history reconcilation: {error}"))?;

        Ok(unavailable_count)
    }

    // just marks a history record as removed and records file-deletion results
    pub fn mark_removed(
        &mut self,
        gid: &str,
        deleted_files: usize,
        failed_file_deletions: usize,
    ) -> Result<(), String> {
        let deleted_files = i64::try_from(deleted_files)
            .map_err(|_| String::from("Deleted-file count is too large"))?;
        let failed_file_deletions = i64::try_from(failed_file_deletions)
            .map_err(|_| String::from("Failed-file count is too large"))?;

        let changed = self
            .connection
            .execute(
                "
            UPDATE download_history
            SET status = 'Removed',
                speed_bytes_per_second = 0,
                deleted_files = ?1,
                failed_file_deletions = ?2,
                last_seen_at = CURRENT_TIMESTAMP
            WHERE gid = ?3
            ",
                params![deleted_files, failed_file_deletions, gid],
            )
            .map_err(|error| format!("Could not update removed download history: {error}"))?;

        if changed == 0 {
            return Err(format!("No history record exists for download {gid}"));
        }

        Ok(())
    }

    // lists the last <this many> results as given in the limit argument
    pub fn list_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>, String> {
        let limit = limit.clamp(1, 100) as i64;

        let mut statement = self
            .connection
            .prepare(
                "
                SELECT
                    id,
                    name,
                    status,
                    last_seen_at,
                    deleted_files,
                    failed_file_deletions
                FROM download_history
                ORDER BY last_seen_at DESC, id DESC
                LIMIT ?1
                ",
            )
            .map_err(|error| format!("Could not prepare history query: {error}"))?;

        let rows = statement
            .query_map(params![limit], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    last_seen_at: row.get(3)?,
                    deleted_files: row.get::<_, i64>(4)?.max(0) as usize,
                    failed_file_deletions: row.get::<_, i64>(5)?.max(0) as usize,
                })
            })
            .map_err(|error| format!("Could not query download history: {error}"))?;

        let mut entries = Vec::new();

        for row in rows {
            entries.push(row.map_err(|error| format!("Could not read history entry: {error}"))?);
        }

        Ok(entries)
    }
}

fn sql_integer(field_name: &str, value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("{field_name} exceeds SQLite's integer range"))
}

// only for linux right now,,, it checks the directory ownership and applies restrictive directory
// permissions,,, ofc because limits are requires
fn secure_data_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let metadata = fs::symlink_metadata(path).map_err(|error| {
            format!(
                "Could not inspect history directory {}: {error}",
                path.display()
            )
        })?;

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "History path must be a real directory: {}",
                path.display()
            ));
        }

        let current_uid = unsafe { libc::geteuid() };

        if metadata.uid() != current_uid {
            return Err(format!(
                "History directory is not owned by the current user: {}",
                path.display()
            ));
        }

        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!(
                "Could not secure history directory {}: {error}",
                path.display()
            )
        })?;
    }

    #[cfg(not(unix))]
    let _ = path;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::HistoryStore;
    use crate::domain::{download::Download, status::Status};
    use rusqlite::Connection;

    fn store() -> HistoryStore {
        HistoryStore::from_connection(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn download(name: &str, status: Status, downloaded: u64) -> Download {
        Download::new(
            String::from("0123456789abcdef"),
            name.to_owned(),
            1000,
            downloaded,
            128,
            status,
            Some(String::from("/home/test/Downloads")),
            Some(String::from("https://example.com/file.zip")),
            None,
        )
    }

    #[test]
    fn updating_download_preserves_stable_history_id() {
        let mut history = store();

        history
            .record_downloads(&[download("file.zip", Status::Active, 100)])
            .unwrap();

        let first = history.list_recent(10).unwrap();
        assert_eq!(first.len(), 1);

        history
            .record_downloads(&[download("renamed.zip", Status::Complete, 1000)])
            .unwrap();

        let updated = history.list_recent(10).unwrap();

        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, first[0].id);
        assert_eq!(updated[0].name, "renamed.zip");
        assert_eq!(updated[0].status, "Completed");
    }

    #[test]
    fn removal_keeps_a_history_record() {
        let mut history = store();

        history
            .record_downloads(&[download("file.zip", Status::Complete, 1000)])
            .unwrap();

        history.mark_removed("0123456789abcdef", 2, 1).unwrap();

        let entries = history.list_recent(10).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "Removed");
        assert_eq!(entries[0].deleted_files, 2);
        assert_eq!(entries[0].failed_file_deletions, 1);
    }

    #[test]
    fn missing_live_download_becomes_unavailable() {
        let mut history = store();

        history
            .record_downloads(&[download("file.zip", Status::Active, 100)])
            .unwrap();

        let unavailable = history.reconcile_snapshot(&[]).unwrap();
        let entries = history.list_recent(10).unwrap();

        assert_eq!(unavailable, 1);
        assert_eq!(entries[0].status, "Unavailable");
    }

    #[test]
    fn completed_download_is_not_marked_unavailable() {
        let mut history = store();

        history
            .record_downloads(&[download("file.zip", Status::Complete, 1000)])
            .unwrap();

        history.reconcile_snapshot(&[]).unwrap();

        let entries = history.list_recent(10).unwrap();
        assert_eq!(entries[0].status, "Completed");
    }

    #[test]
    fn reappearing_download_recovers_its_live_status() {
        let mut history = store();

        history
            .record_downloads(&[download("file.zip", Status::Active, 100)])
            .unwrap();

        history.reconcile_snapshot(&[]).unwrap();

        history
            .reconcile_snapshot(&[download("file.zip", Status::Paused, 500)])
            .unwrap();

        let entries = history.list_recent(10).unwrap();
        assert_eq!(entries[0].status, "Paused");
    }
}
