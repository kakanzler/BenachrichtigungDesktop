//! Mirrors `due_count` and `settle_absence` in
//! 601---English `AI-Coach/backend/app/engines/active_recall.py`.

use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeSet;

const DEFAULT_ABSENCE_DAYS: i64 = 3;
const RETRO_VACATION_DAYS: i64 = 60;

/// Same `target_type` -> table mapping as `_target_for`. Orphaned rows and
/// unknown types are not due there, so they are not counted here either.
const COUNT_SQL: &str = "
    SELECT COUNT(*) FROM review_queue q
    WHERE q.next_review <= ?1 AND CASE q.target_type
        WHEN 'grammar'    THEN EXISTS (SELECT 1 FROM grammar t             WHERE t.id = q.target_id)
        WHEN 'idiom'      THEN EXISTS (SELECT 1 FROM idioms t              WHERE t.id = q.target_id)
        WHEN 'phrase'     THEN EXISTS (SELECT 1 FROM phrases t             WHERE t.id = q.target_id)
        WHEN 'sentence'   THEN EXISTS (SELECT 1 FROM sentences t           WHERE t.id = q.target_id)
        WHEN 'vocabulary' THEN EXISTS (SELECT 1 FROM vocabulary t          WHERE t.id = q.target_id)
        WHEN 'article'    THEN EXISTS (SELECT 1 FROM articles t            WHERE t.id = q.target_id)
        WHEN 'toeic'      THEN EXISTS (SELECT 1 FROM toeic_questions t     WHERE t.id = q.target_id)
        WHEN 'listening'  THEN EXISTS (SELECT 1 FROM listening_questions t WHERE t.id = q.target_id)
        WHEN 'assort'     THEN EXISTS (SELECT 1 FROM assorts t             WHERE t.id = q.target_id)
        ELSE 0
    END";

pub fn count_due(conn: &Connection, today: NaiveDate) -> rusqlite::Result<i64> {
    let shift = pending_shift(&load_absence(conn)?, today);
    // next_review + shift <= today, without writing the shift to the database.
    let cutoff = today - Duration::days(shift);
    conn.query_row(
        COUNT_SQL,
        params![cutoff.format("%Y-%m-%d").to_string()],
        |row| row.get(0),
    )
}

/// Raw `settings` values. `None` means the row (or its value) is absent.
#[derive(Debug, Default, Clone)]
pub struct Absence {
    pub pause_from: Option<String>,
    pub pause_until: Option<String>,
    pub absence_days: Option<String>,
    pub last_seen: Option<String>,
    pub shifted_days: Option<String>,
    /// `settle_absence` treats an absent `ar_shifted_days` row as an upgrade from
    /// its old version, unlike a row that merely holds an empty value.
    pub shifted_row_exists: bool,
}

fn load_absence(conn: &Connection) -> rusqlite::Result<Absence> {
    let raw = |key: &str| -> rusqlite::Result<Option<Option<String>>> {
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
    };
    let value = |key: &str| raw(key).map(Option::flatten);
    let shifted = raw("ar_shifted_days")?;
    Ok(Absence {
        pause_from: value("ar_pause_from")?,
        pause_until: value("ar_pause_until")?,
        absence_days: value("ar_absence_days")?,
        last_seen: value("ar_last_seen")?,
        shifted_row_exists: shifted.is_some(),
        shifted_days: shifted.flatten(),
    })
}

fn parse_day(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

fn parse_opt_day(value: &Option<String>) -> Option<NaiveDate> {
    value.as_deref().and_then(parse_day)
}

/// `vacation_days`: the days of the vacation range that have already begun.
fn vacation_days(a: &Absence, today: NaiveDate) -> BTreeSet<NaiveDate> {
    let Some(start) = parse_opt_day(&a.pause_from) else {
        return BTreeSet::new();
    };
    let until = parse_opt_day(&a.pause_until);
    if until.is_some_and(|end| end < start) || start > today {
        return BTreeSet::new();
    }
    let end = until.map_or(today, |end| end.min(today));
    start.iter_days().take_while(|d| *d <= end).collect()
}

/// `_absence_threshold`: blank or invalid uses the default; 0 disables it.
fn absence_threshold(a: &Absence) -> i64 {
    match a.absence_days.as_deref().map(str::trim) {
        None | Some("") => DEFAULT_ABSENCE_DAYS,
        Some(v) => v.parse::<i64>().map_or(DEFAULT_ABSENCE_DAYS, |n| n.max(0)),
    }
}

/// The days `settle_absence` would shift the schedule by if it ran today.
pub fn pending_shift(a: &Absence, today: NaiveDate) -> i64 {
    let Some(last_seen) = parse_opt_day(&a.last_seen) else {
        return 0; // The app's first run ever only records today.
    };
    let vacation = vacation_days(a, today);
    let shifted: BTreeSet<NaiveDate> = if a.shifted_row_exists {
        a.shifted_days
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter_map(parse_day)
            .collect()
    } else {
        vacation
            .iter()
            .copied()
            .filter(|d| *d < last_seen)
            .collect()
    };

    let oldest = today - Duration::days(RETRO_VACATION_DAYS);
    let mut frozen: BTreeSet<NaiveDate> = vacation.into_iter().filter(|d| *d > oldest).collect();
    let gap = (today - last_seen).num_days() - 1; // days with no visit, today excluded
    let threshold = absence_threshold(a);
    if threshold > 0 && gap >= threshold {
        frozen.extend((1..=gap).map(|i| last_seen + Duration::days(i)));
    }
    frozen.difference(&shifted).count() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_day(s).unwrap()
    }

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    /// Settled yesterday: no gap, nothing pending by default.
    fn settled(last_seen: &str) -> Absence {
        Absence {
            last_seen: s(last_seen),
            shifted_days: s(""),
            shifted_row_exists: true,
            ..Absence::default()
        }
    }

    const TODAY: &str = "2026-11-08";

    #[test]
    fn first_run_ever_shifts_nothing() {
        let a = Absence {
            pause_from: s("2026-11-01"),
            ..Absence::default()
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 0);
    }

    #[test]
    fn gap_below_threshold_shifts_nothing() {
        // Last seen the 5th: the 6th and 7th were missed, a gap of 2 < 3.
        assert_eq!(pending_shift(&settled("2026-11-05"), d(TODAY)), 0);
    }

    #[test]
    fn gap_at_threshold_shifts_the_whole_gap() {
        // Last seen the 4th: the 5th, 6th and 7th were missed.
        assert_eq!(pending_shift(&settled("2026-11-04"), d(TODAY)), 3);
    }

    #[test]
    fn threshold_zero_disables_the_gap_rule() {
        let a = Absence {
            absence_days: s("0"),
            ..settled("2026-10-01")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 0);
    }

    #[test]
    fn invalid_threshold_uses_the_default() {
        let a = Absence {
            absence_days: s("soon"),
            ..settled("2026-11-04")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 3);
    }

    #[test]
    fn vacation_including_today_shifts_each_begun_day() {
        let a = Absence {
            pause_from: s("2026-11-07"),
            pause_until: s("2026-11-20"),
            ..settled("2026-11-07")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 2);
    }

    #[test]
    fn open_ended_vacation_runs_through_today() {
        let a = Absence {
            pause_from: s("2026-11-06"),
            ..settled("2026-11-07")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 3);
    }

    #[test]
    fn vacation_that_has_not_begun_or_is_inverted_is_ignored() {
        let future = Absence {
            pause_from: s("2026-12-01"),
            ..settled("2026-11-07")
        };
        assert_eq!(pending_shift(&future, d(TODAY)), 0);
        let inverted = Absence {
            pause_from: s("2026-11-07"),
            pause_until: s("2026-11-01"),
            ..settled("2026-11-07")
        };
        assert_eq!(pending_shift(&inverted, d(TODAY)), 0);
    }

    #[test]
    fn days_already_shifted_are_not_counted_twice() {
        let a = Absence {
            pause_from: s("2026-11-06"),
            pause_until: s("2026-11-08"),
            shifted_days: s("2026-11-06,2026-11-07"),
            ..settled("2026-11-07")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 1);
    }

    #[test]
    fn vacation_and_gap_overlap_counts_each_day_once() {
        let a = Absence {
            pause_from: s("2026-11-05"),
            pause_until: s("2026-11-06"),
            ..settled("2026-11-04")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 3);
    }

    #[test]
    fn missing_shifted_row_treats_days_before_last_seen_as_shifted() {
        // Upgrade path: the 5th and 6th count as done; the 7th (last_seen itself)
        // and the 8th do not.
        let a = Absence {
            pause_from: s("2026-11-05"),
            pause_until: s("2026-11-08"),
            last_seen: s("2026-11-07"),
            shifted_row_exists: false,
            ..Absence::default()
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 2);
    }

    #[test]
    fn vacation_days_older_than_the_retro_window_are_dropped() {
        // today - 60 is 2026-09-09 and the window keeps later days only, so of
        // this range just the 10th remains.
        let a = Absence {
            pause_from: s("2026-09-01"),
            pause_until: s("2026-09-10"),
            absence_days: s("0"),
            ..settled("2026-11-07")
        };
        assert_eq!(pending_shift(&a, d(TODAY)), 1);
    }

    const TARGET_TABLES: [&str; 9] = [
        "grammar",
        "idioms",
        "phrases",
        "sentences",
        "vocabulary",
        "articles",
        "toeic_questions",
        "listening_questions",
        "assorts",
    ];

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE review_queue (target_type TEXT, target_id TEXT, next_review TEXT);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        for table in TARGET_TABLES {
            conn.execute_batch(&format!(
                "CREATE TABLE {table} (id TEXT PRIMARY KEY);
                 INSERT INTO {table} VALUES ('x');"
            ))
            .unwrap();
        }
        conn
    }

    fn enqueue(conn: &Connection, target_type: &str, id: &str, next_review: &str) {
        conn.execute(
            "INSERT INTO review_queue VALUES (?1, ?2, ?3)",
            params![target_type, id, next_review],
        )
        .unwrap();
    }

    fn set(conn: &Connection, key: &str, value: &str) {
        conn.execute("INSERT INTO settings VALUES (?1, ?2)", params![key, value])
            .unwrap();
    }

    #[test]
    fn counts_every_target_type() {
        let conn = db();
        for t in [
            "grammar",
            "idiom",
            "phrase",
            "sentence",
            "vocabulary",
            "article",
            "toeic",
            "listening",
            "assort",
        ] {
            enqueue(&conn, t, "x", TODAY);
        }
        assert_eq!(count_due(&conn, d(TODAY)).unwrap(), 9);
    }

    #[test]
    fn excludes_orphans_unknown_types_and_future_rows() {
        let conn = db();
        enqueue(&conn, "vocabulary", "deleted", TODAY);
        enqueue(&conn, "mystery", "x", TODAY);
        enqueue(&conn, "grammar", "x", "2026-11-09");
        enqueue(&conn, "idiom", "x", "2026-11-01"); // overdue: counted
        assert_eq!(count_due(&conn, d(TODAY)).unwrap(), 1);
    }

    #[test]
    fn pending_shift_moves_rows_out_of_today() {
        let conn = db();
        set(&conn, "ar_last_seen", "2026-11-04");
        set(&conn, "ar_shifted_days", "");
        enqueue(&conn, "grammar", "x", "2026-11-05"); // + 3 days = today: due
        enqueue(&conn, "idiom", "x", "2026-11-06"); // + 3 days = tomorrow: not due
        assert_eq!(count_due(&conn, d(TODAY)).unwrap(), 1);
    }

    #[test]
    fn load_distinguishes_an_absent_row_from_an_empty_one() {
        let conn = db();
        assert!(!load_absence(&conn).unwrap().shifted_row_exists);
        set(&conn, "ar_shifted_days", "");
        assert!(load_absence(&conn).unwrap().shifted_row_exists);
    }
}
