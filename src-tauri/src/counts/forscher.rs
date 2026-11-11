//! Mirrors `due_today` in 602---Forscher `src-tauri/src/cmd/dashboard.rs`:
//! overdue included, `daily_cap` not applied.

use chrono::NaiveDate;
use rusqlite::{params, Connection};

pub fn count_due(conn: &Connection, today: NaiveDate) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM items WHERE archived = 0 AND next_review <= ?1",
        params![today.format("%Y-%m-%d").to_string()],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_today_and_overdue_but_not_archived_or_future() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE items (id TEXT, archived INTEGER, next_review TEXT);
             INSERT INTO items VALUES
               ('today',    0, '2026-11-08'),
               ('overdue',  0, '2026-10-01'),
               ('tomorrow', 0, '2026-11-09'),
               ('archived', 1, '2026-11-01');",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        assert_eq!(count_due(&conn, today).unwrap(), 2);
    }
}
