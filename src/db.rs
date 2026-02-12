// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Database operations for the rebuild queue.
//!
//! Uses SQLite with WAL mode for concurrent access from pacman hooks.
//! The database stores:
//! - `queue`: Packages currently marked for rebuild
//! - `trigger_events`: History of trigger events for debugging

use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

/// Default database path.
pub const DEFAULT_DB_PATH: &str = "/var/lib/anneal/anneal.db";

/// Get the database path, checking ANNEAL_DB_PATH environment variable.
pub fn get_db_path() -> std::path::PathBuf {
    std::env::var("ANNEAL_DB_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(DEFAULT_DB_PATH))
}

/// Database connection wrapper.
pub struct Database {
    conn: Connection,
    /// Retention period for trigger events in days (0 = keep forever).
    retention_days: u32,
}

/// A package in the rebuild queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    /// Package name.
    pub package: String,
    /// When the package was first marked (ISO8601).
    pub first_marked_at: String,
}

/// A trigger event in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerEvent {
    /// Event ID.
    pub id: i64,
    /// Package that was marked.
    pub package: String,
    /// Trigger package that caused the mark (None for external marks).
    pub trigger_package: Option<String>,
    /// Version of the trigger package at time of mark.
    pub trigger_version: Option<String>,
    /// When the package was marked (ISO8601).
    pub marked_at: String,
}

/// Database errors.
#[derive(Debug)]
pub enum DbError {
    /// SQLite error.
    Sqlite(rusqlite::Error),
    /// I/O error (e.g., creating directory).
    Io(std::io::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "database error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            Self::Io(e) => Some(e),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl Database {
    /// Open the database at the default path.
    ///
    /// Creates the database and parent directories if they don't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or initialized.
    pub fn open(retention_days: u32) -> Result<Self, DbError> {
        Self::open_at(&get_db_path(), retention_days)
    }

    /// Open the database at a specific path.
    ///
    /// Creates the database and parent directories if they don't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created, database cannot
    /// be opened, or schema initialization fails.
    pub fn open_at(path: &Path, retention_days: u32) -> Result<Self, DbError> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let mut db = Self {
            conn,
            retention_days,
        };
        db.init()?;
        Ok(db)
    }

    /// Open the database in read-only mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the database doesn't exist or cannot be opened.
    pub fn open_readonly(path: &Path) -> Result<Self, DbError> {
        // We use immutable=1 to prevent SQLite from trying to create side files
        // (-shm, -wal) even if the database was left in WAL mode.
        let path_str = path.to_string_lossy();
        let uri = format!("file:{}?immutable=1", path_str);
        let conn = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )?;
        Ok(Self {
            conn,
            retention_days: 0, // Not used for read-only
        })
    }

    /// Initialize the database schema.
    fn init(&mut self) -> Result<(), DbError> {
        // Use DELETE mode to ensure read-only users can access the DB.
        // WAL mode requires write access to the directory to create -shm files,
        // which prevents non-root users from running `anneal list`.
        self.conn.pragma_update(None, "journal_mode", "DELETE")?;

        self.conn.execute_batch(
            r"
            -- Packages currently marked for rebuild
            CREATE TABLE IF NOT EXISTS queue (
                package TEXT PRIMARY KEY,
                first_marked_at TEXT NOT NULL
            );

            -- Trigger event history
            CREATE TABLE IF NOT EXISTS trigger_events (
                id INTEGER PRIMARY KEY,
                package TEXT NOT NULL,
                trigger_package TEXT,
                trigger_version TEXT,
                marked_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_trigger_events_package
                ON trigger_events(package);
            CREATE INDEX IF NOT EXISTS idx_trigger_events_trigger
                ON trigger_events(trigger_package);
            CREATE INDEX IF NOT EXISTS idx_trigger_events_marked_at
                ON trigger_events(marked_at);
            ",
        )?;

        Ok(())
    }

    /// Mark a package for rebuild.
    ///
    /// If the package is already in the queue, this is a no-op for the queue
    /// but still records a trigger event.
    ///
    /// Returns `true` if the package was newly added to the queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn mark(
        &mut self,
        package: &str,
        trigger_package: Option<&str>,
        trigger_version: Option<&str>,
    ) -> Result<bool, DbError> {
        let now = now_iso8601();
        let tx = self.conn.transaction()?;

        // Try to insert into queue (ignore if already exists)
        let newly_added = tx.execute(
            "INSERT OR IGNORE INTO queue (package, first_marked_at) VALUES (?1, ?2)",
            params![package, now],
        )? > 0;

        // Always record the trigger event
        tx.execute(
            "INSERT INTO trigger_events (package, trigger_package, trigger_version, marked_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![package, trigger_package, trigger_version, now],
        )?;

        tx.commit()?;

        // Opportunistic cleanup after transaction
        self.prune_old_events()?;

        Ok(newly_added)
    }

    /// Remove a package from the rebuild queue.
    ///
    /// Returns `true` if the package was in the queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn unmark(&mut self, package: &str) -> Result<bool, DbError> {
        let removed = self
            .conn
            .execute("DELETE FROM queue WHERE package = ?1", params![package])?
            > 0;
        Ok(removed)
    }

    /// Check if a package is in the rebuild queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn is_marked(&self, package: &str) -> Result<bool, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM queue WHERE package = ?1",
            params![package],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// List all packages in the rebuild queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn list(&self) -> Result<Vec<QueueEntry>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT package, first_marked_at FROM queue ORDER BY first_marked_at")?;

        let entries = stmt
            .query_map([], |row| {
                Ok(QueueEntry {
                    package: row.get(0)?,
                    first_marked_at: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    /// Query which of the given packages are in the queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn query(&self, packages: &[&str]) -> Result<Vec<String>, DbError> {
        if packages.is_empty() {
            return Ok(Vec::new());
        }

        // Build a query with placeholders for each package
        let placeholders: Vec<_> = packages.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT package FROM queue WHERE package IN ({}) ORDER BY package",
            placeholders.join(", ")
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            packages.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let found = stmt
            .query_map(params.as_slice(), |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(found)
    }

    /// Clear the entire rebuild queue.
    ///
    /// Does not clear trigger event history.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn clear(&mut self) -> Result<usize, DbError> {
        let count = self.conn.execute("DELETE FROM queue", [])?;
        Ok(count)
    }

    /// Clear trigger events for a specific trigger package.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub fn clear_trigger_events(&mut self, trigger_package: &str) -> Result<usize, DbError> {
        let count = self.conn.execute(
            "DELETE FROM trigger_events WHERE trigger_package = ?1",
            params![trigger_package],
        )?;
        Ok(count)
    }

    /// Get trigger events for a package.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_events(&self, package: &str) -> Result<Vec<TriggerEvent>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, package, trigger_package, trigger_version, marked_at
             FROM trigger_events WHERE package = ?1 ORDER BY marked_at DESC",
        )?;

        let events = stmt
            .query_map(params![package], |row| {
                Ok(TriggerEvent {
                    id: row.get(0)?,
                    package: row.get(1)?,
                    trigger_package: row.get(2)?,
                    trigger_version: row.get(3)?,
                    marked_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    /// Get the most recent trigger event for a package.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_latest_event(&self, package: &str) -> Result<Option<TriggerEvent>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, package, trigger_package, trigger_version, marked_at
             FROM trigger_events WHERE package = ?1 ORDER BY marked_at DESC LIMIT 1",
        )?;

        let event = stmt
            .query_row(params![package], |row| {
                Ok(TriggerEvent {
                    id: row.get(0)?,
                    package: row.get(1)?,
                    trigger_package: row.get(2)?,
                    trigger_version: row.get(3)?,
                    marked_at: row.get(4)?,
                })
            })
            .optional()?;

        Ok(event)
    }

    /// Prune trigger events older than retention period.
    fn prune_old_events(&mut self) -> Result<usize, DbError> {
        if self.retention_days == 0 {
            return Ok(0);
        }

        let cutoff = cutoff_date(self.retention_days);
        let count = self.conn.execute(
            "DELETE FROM trigger_events WHERE marked_at < ?1",
            params![cutoff],
        )?;
        Ok(count)
    }
}

/// Get current time as ISO8601 string with millisecond precision.
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Convert to date components (simplified - doesn't handle leap seconds)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Calculate date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

/// Calculate cutoff date for retention period.
fn cutoff_date(retention_days: u32) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let cutoff_secs = now
        .as_secs()
        .saturating_sub(u64::from(retention_days) * 86400);

    let days = cutoff_secs / 86400;
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days: u64) -> (i32, u32, u32) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y as i32, m, d)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn temp_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.db");
        let db = Database::open_at(&path, 90).expect("open db");
        (dir, db)
    }

    #[test]
    fn mark_and_list() {
        let (_dir, mut db) = temp_db();

        assert!(db.mark("pkg1", None, None).expect("mark"));
        assert!(
            db.mark("pkg2", Some("qt6-base"), Some("6.7.0"))
                .expect("mark")
        );

        let queue = db.list().expect("list");
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].package, "pkg1");
        assert_eq!(queue[1].package, "pkg2");
    }

    #[test]
    fn mark_idempotent() {
        let (_dir, mut db) = temp_db();

        assert!(db.mark("pkg1", None, None).expect("first mark"));
        assert!(!db.mark("pkg1", None, None).expect("second mark"));

        let queue = db.list().expect("list");
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn mark_creates_event_even_when_already_marked() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", Some("trigger1"), None).expect("first mark");
        db.mark("pkg1", Some("trigger2"), None)
            .expect("second mark");

        let events = db.get_events("pkg1").expect("events");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn unmark() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", None, None).expect("mark");
        assert!(db.is_marked("pkg1").expect("is_marked"));

        assert!(db.unmark("pkg1").expect("unmark"));
        assert!(!db.is_marked("pkg1").expect("is_marked"));

        // Unmark non-existent returns false
        assert!(!db.unmark("pkg1").expect("unmark again"));
    }

    #[test]
    fn is_marked() {
        let (_dir, mut db) = temp_db();

        assert!(!db.is_marked("pkg1").expect("is_marked"));
        db.mark("pkg1", None, None).expect("mark");
        assert!(db.is_marked("pkg1").expect("is_marked"));
    }

    #[test]
    fn query() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", None, None).expect("mark");
        db.mark("pkg3", None, None).expect("mark");

        let found = db.query(&["pkg1", "pkg2", "pkg3", "pkg4"]).expect("query");
        assert_eq!(found, vec!["pkg1", "pkg3"]);
    }

    #[test]
    fn query_empty() {
        let (_dir, db) = temp_db();
        let found = db.query(&[]).expect("query");
        assert!(found.is_empty());
    }

    #[test]
    fn clear() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", None, None).expect("mark");
        db.mark("pkg2", None, None).expect("mark");

        let count = db.clear().expect("clear");
        assert_eq!(count, 2);

        let queue = db.list().expect("list");
        assert!(queue.is_empty());
    }

    #[test]
    fn trigger_events() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", Some("qt6-base"), Some("6.7.0"))
            .expect("mark");

        let events = db.get_events("pkg1").expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].package, "pkg1");
        assert_eq!(events[0].trigger_package, Some("qt6-base".to_string()));
        assert_eq!(events[0].trigger_version, Some("6.7.0".to_string()));
    }

    #[test]
    fn external_mark_has_null_trigger() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", None, None).expect("mark");

        let events = db.get_events("pkg1").expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trigger_package, None);
        assert_eq!(events[0].trigger_version, None);
    }

    #[test]
    fn get_latest_event() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", Some("trigger1"), None).expect("first mark");
        std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure different timestamps
        db.mark("pkg1", Some("trigger2"), None)
            .expect("second mark");

        let latest = db
            .get_latest_event("pkg1")
            .expect("latest")
            .expect("should exist");
        assert_eq!(latest.trigger_package, Some("trigger2".to_string()));
    }

    #[test]
    fn get_latest_event_empty() {
        let (_dir, db) = temp_db();
        let latest = db.get_latest_event("pkg1").expect("latest");
        assert!(latest.is_none());
    }

    #[test]
    fn clear_trigger_events() {
        let (_dir, mut db) = temp_db();

        db.mark("pkg1", Some("qt6-base"), None).expect("mark");
        db.mark("pkg2", Some("gtk4"), None).expect("mark");

        let count = db.clear_trigger_events("qt6-base").expect("clear");
        assert_eq!(count, 1);

        let events1 = db.get_events("pkg1").expect("events");
        assert!(events1.is_empty());

        let events2 = db.get_events("pkg2").expect("events");
        assert_eq!(events2.len(), 1);
    }

    #[test]
    fn readonly_mode() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.db");

        // Create and populate database
        {
            let mut db = Database::open_at(&path, 90).expect("open db");
            db.mark("pkg1", None, None).expect("mark");
        }

        // Open read-only
        let db = Database::open_readonly(&path).expect("open readonly");
        assert!(db.is_marked("pkg1").expect("is_marked"));

        let queue = db.list().expect("list");
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn iso8601_format() {
        let ts = now_iso8601();
        // Basic format check: YYYY-MM-DDTHH:MM:SS.mmmZ
        assert_eq!(ts.len(), 24);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        assert_eq!(&ts[19..20], ".");
        assert_eq!(&ts[23..24], "Z");
    }

    #[test]
    fn days_to_date_epoch() {
        // 1970-01-01
        assert_eq!(days_to_date(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_date_known_dates() {
        // 2024-01-01 is 19723 days from epoch
        assert_eq!(days_to_date(19723), (2024, 1, 1));
        // 2000-01-01 is 10957 days from epoch
        assert_eq!(days_to_date(10957), (2000, 1, 1));
    }

    #[test]
    fn readonly_mode_strict() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.db");

        // Create and populate database
        {
            let mut db = Database::open_at(&path, 90).expect("open db");
            db.mark("pkg1", None, None).expect("mark");
        }

        // Restrict permissions to read-only for file and directory
        let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&path, perms).expect("set file permissions");

        let mut dir_perms = std::fs::metadata(dir.path())
            .expect("dir metadata")
            .permissions();
        dir_perms.set_mode(0o555);
        std::fs::set_permissions(dir.path(), dir_perms).expect("set dir permissions");

        // Open read-only
        let db = Database::open_readonly(&path).expect("open readonly");
        let queue = db.list().expect("list");
        assert_eq!(queue.len(), 1);
    }
}
