//! What the card shows, free of any window or database concerns.

use crate::counts::Reading;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppId {
    English,
    Forscher,
}

impl AppId {
    pub const ALL: [AppId; 2] = [AppId::English, AppId::Forscher];

    pub fn label(self) -> &'static str {
        match self {
            AppId::English => "English",
            AppId::Forscher => "Forscher",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Count(i64),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub app: AppId,
    pub label: &'static str,
    pub count: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct View {
    pub rows: Vec<Row>,
    pub visible: bool,
}

#[derive(Debug, Default)]
pub struct Model {
    statuses: [Option<Status>; 2],
    notes: [Option<String>; 2],
    /// The statuses at the moment × was pressed; hidden until they differ.
    dismissed_at: Option<[Option<Status>; 2]>,
}

impl Model {
    pub fn apply(&mut self, app: AppId, reading: &Reading) {
        let slot = &mut self.statuses[app.index()];
        *slot = match reading {
            Reading::Count(n) => Some(Status::Count(*n)),
            Reading::Failed(_) => Some(Status::Unknown),
            Reading::Busy => slot.or(Some(Status::Unknown)),
        };
        if self.dismissed_at.is_some_and(|at| at != self.statuses) {
            self.dismissed_at = None;
        }
    }

    pub fn dismiss(&mut self) {
        self.dismissed_at = Some(self.statuses);
    }

    pub fn reveal(&mut self) {
        self.dismissed_at = None;
    }

    pub fn set_note(&mut self, app: AppId, note: Option<String>) {
        self.notes[app.index()] = note;
    }

    pub fn view(&self) -> View {
        let rows: Vec<Row> = AppId::ALL
            .into_iter()
            .filter_map(|app| {
                let count = match self.statuses[app.index()]? {
                    Status::Count(0) => return None,
                    Status::Count(n) => n.to_string(),
                    Status::Unknown => "?".to_string(),
                };
                Some(Row {
                    app,
                    label: app.label(),
                    count,
                    note: self.notes[app.index()].clone(),
                })
            })
            .collect();
        let visible = !rows.is_empty() && self.dismissed_at.is_none();
        View { rows, visible }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AppId::{English, Forscher};

    fn model(english: i64, forscher: i64) -> Model {
        let mut m = Model::default();
        m.apply(English, &Reading::Count(english));
        m.apply(Forscher, &Reading::Count(forscher));
        m
    }

    fn counts(m: &Model) -> Vec<(AppId, String)> {
        m.view()
            .rows
            .into_iter()
            .map(|r| (r.app, r.count))
            .collect()
    }

    #[test]
    fn nothing_is_shown_before_the_first_reading() {
        assert_eq!(
            Model::default().view(),
            View {
                rows: vec![],
                visible: false
            }
        );
    }

    #[test]
    fn zero_rows_are_hidden_and_all_zero_hides_the_card() {
        let m = model(0, 4);
        assert_eq!(counts(&m), vec![(Forscher, "4".to_string())]);
        assert!(m.view().visible);
        assert!(!model(0, 0).view().visible);
    }

    #[test]
    fn failure_shows_a_question_mark_rather_than_hiding() {
        let mut m = model(0, 0);
        m.apply(English, &Reading::Failed("no such table".into()));
        assert_eq!(counts(&m), vec![(English, "?".to_string())]);
        assert!(m.view().visible);
    }

    #[test]
    fn busy_keeps_the_previous_value() {
        let mut m = model(5, 0);
        m.apply(English, &Reading::Busy);
        assert_eq!(counts(&m), vec![(English, "5".to_string())]);
    }

    #[test]
    fn busy_before_any_value_is_unknown() {
        let mut m = Model::default();
        m.apply(English, &Reading::Busy);
        assert_eq!(counts(&m), vec![(English, "?".to_string())]);
    }

    #[test]
    fn dismiss_holds_until_a_count_changes() {
        let mut m = model(5, 2);
        m.dismiss();
        m.apply(English, &Reading::Count(5));
        m.apply(Forscher, &Reading::Count(2));
        assert!(!m.view().visible);
        m.apply(Forscher, &Reading::Count(1));
        assert!(m.view().visible);
    }

    #[test]
    fn dismiss_does_not_return_when_a_count_goes_back() {
        let mut m = model(5, 2);
        m.dismiss();
        m.apply(English, &Reading::Count(6));
        m.apply(English, &Reading::Count(5));
        assert!(m.view().visible);
    }

    #[test]
    fn busy_does_not_undo_a_dismiss() {
        let mut m = model(5, 2);
        m.dismiss();
        m.apply(English, &Reading::Busy);
        assert!(!m.view().visible);
    }

    #[test]
    fn reveal_clears_a_dismiss() {
        let mut m = model(5, 2);
        m.dismiss();
        m.reveal();
        assert!(m.view().visible);
    }

    #[test]
    fn note_appears_on_its_row_and_clears() {
        let mut m = model(5, 2);
        m.set_note(English, Some("未検出".into()));
        assert_eq!(m.view().rows[0].note.as_deref(), Some("未検出"));
        assert_eq!(m.view().rows[1].note, None);
        m.set_note(English, None);
        assert_eq!(m.view().rows[0].note, None);
    }
}
