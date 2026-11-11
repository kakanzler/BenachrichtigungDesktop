//! Read-only due counts from each app's own database.
//!
//! The apps' HTTP APIs are deliberately not used: English's `settle()` runs on
//! those GETs and can charge expiry penalties and update `ar_last_seen`.

pub mod english;
pub mod forscher;

use chrono::NaiveDate;
use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::path::Path;
use std::time::Duration;

pub const BUSY_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    Count(i64),
    /// The app is mid-commit. Not an error: the previous value still stands.
    Busy,
    Failed(String),
}

/// Opens without creating or migrating anything. `immutable=1` must never be
/// added: Forscher keeps most of its data in the WAL, which that would ignore.
fn open_read_only(path: &Path, busy_timeout: Duration) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(busy_timeout)?;
    Ok(conn)
}

pub fn read(
    path: &Path,
    today: NaiveDate,
    busy_timeout: Duration,
    count: impl Fn(&Connection, NaiveDate) -> rusqlite::Result<i64>,
) -> Reading {
    let result = open_read_only(path, busy_timeout).and_then(|conn| {
        // One transaction, so English's settings and queue come from the same
        // snapshot even if the app commits a schedule shift in between.
        let tx = conn.unchecked_transaction()?;
        count(&tx, today)
    });
    match result {
        Ok(n) => Reading::Count(n),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if matches!(e.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) =>
        {
            Reading::Busy
        }
        Err(e) => Reading::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()
    }

    /// Prints the counts from the real databases, for comparing with the apps:
    /// `cargo test real_data_counts -- --ignored --nocapture`
    #[test]
    #[ignore = "reads the installed apps' real data"]
    fn real_data_counts() {
        let roaming = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap());
        let today = chrono::Local::now().date_naive();
        let english = read(
            &roaming.join(r"com.aicoach.english\coach.db"),
            today,
            BUSY_TIMEOUT,
            english::count_due,
        );
        let forscher = read(
            &roaming.join(r"com.forscher.app\forscher.db"),
            today,
            BUSY_TIMEOUT,
            forscher::count_due,
        );
        println!("english={english:?} forscher={forscher:?}");
    }

    #[test]
    fn missing_file_fails_and_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.db");
        let reading = read(&path, day(), BUSY_TIMEOUT, forscher::count_due);
        assert!(matches!(reading, Reading::Failed(_)));
        assert!(!path.exists());
    }

    #[test]
    fn missing_table_fails_rather_than_counting_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE other (id TEXT);")
            .unwrap();
        let reading = read(&path, day(), BUSY_TIMEOUT, forscher::count_due);
        assert!(matches!(reading, Reading::Failed(_)));
    }

    #[test]
    fn exclusive_lock_reads_as_busy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.db");
        let writer = Connection::open(&path).unwrap();
        writer
            .execute_batch(
                "CREATE TABLE items (archived INTEGER, next_review TEXT);
                 BEGIN EXCLUSIVE;",
            )
            .unwrap();
        let reading = read(&path, day(), Duration::from_millis(10), forscher::count_due);
        assert_eq!(reading, Reading::Busy);
    }

    #[test]
    fn reads_uncheckpointed_wal_data_while_a_writer_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wal.db");
        let writer = Connection::open(&path).unwrap();
        writer
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA wal_autocheckpoint = 0;
                 CREATE TABLE items (archived INTEGER, next_review TEXT);
                 INSERT INTO items VALUES (0, '2026-11-08');",
            )
            .unwrap();
        let reading = read(&path, day(), BUSY_TIMEOUT, forscher::count_due);
        assert_eq!(reading, Reading::Count(1));
    }
}
