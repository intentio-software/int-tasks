//! The task model.
//!
//! Shaped around one rule: a task needs a title and nothing else. Every other
//! field is optional, so capture never stops to ask a question. Fields that a
//! future Intentio IMS sync will need — stable ids, timestamps, an external
//! reference — are present from the start, because retrofitting identity onto
//! existing records is the one thing that cannot be done cleanly later.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Where a task sits in the flow. Lists are user-defined; this is the meaning
/// behind them, so "done" survives a board being renamed or reorganised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Todo,
    Doing,
    Done,
}

impl Status {
    pub fn is_done(self) -> bool {
        matches!(self, Status::Done)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    #[serde(default)]
    pub status: Status,

    /// List this task sits in. Every task has one; quick capture uses the
    /// board's first list so it never has to ask.
    pub list_id: String,

    /// Position within the list, ascending. Rewritten on reorder.
    #[serde(default)]
    pub order: u32,

    /// `YYYY-MM-DD`. Compared as a string, which sorts correctly by date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,

    /// Pulled onto the Today list regardless of its due date.
    #[serde(default, skip_serializing_if = "is_false")]
    pub today: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// 1 is highest. Absent means unprioritised, which sorts last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,

    /// Rough size in minutes, for planning a day against pomodoro sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate_minutes: Option<u32>,

    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,

    /// Identifier in Intentio IMS once tasks can be assigned from there.
    /// Unused today; present so synced tasks can be matched rather than duplicated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

impl Task {
    /// A task with only a title, in the given list.
    pub fn new(title: &str, list_id: &str) -> Self {
        let now = now_millis();
        Task {
            id: new_id("task"),
            title: title.trim().to_string(),
            notes: None,
            status: Status::Todo,
            list_id: list_id.to_string(),
            order: 0,
            due: None,
            today: false,
            tags: Vec::new(),
            priority: None,
            estimate_minutes: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
            external_id: None,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_millis();
    }
}

/// A column on a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub id: String,
    pub name: String,
    pub order: u32,
    /// Status applied to tasks dropped into this list. A list called "Done"
    /// should complete what lands in it without the user restating that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

impl List {
    pub fn new(name: &str, order: u32, status: Option<Status>) -> Self {
        List { id: new_id("list"), name: name.trim().to_string(), order, status }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub lists: Vec<List>,
}

impl Board {
    /// A board with the three lists almost every board starts with.
    pub fn with_default_lists(name: &str, order: u32) -> Self {
        Board {
            id: new_id("board"),
            name: name.trim().to_string(),
            order,
            lists: vec![
                List::new("To Do", 0, Some(Status::Todo)),
                List::new("Doing", 1, Some(Status::Doing)),
                List::new("Done", 2, Some(Status::Done)),
            ],
        }
    }

    pub fn first_list_id(&self) -> Option<&str> {
        self.lists.iter().min_by_key(|list| list.order).map(|list| list.id.as_str())
    }

    pub fn list(&self, id: &str) -> Option<&List> {
        self.lists.iter().find(|list| list.id == id)
    }
}

/// A recorded stretch of focused work.
///
/// Sessions are append-only: a finished session is a fact about the past, and
/// editing history would make time reports meaningless.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    /// The task this time counted towards. `None` while unattributed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub started_at: u64,
    pub ended_at: u64,
    pub seconds: u64,
    #[serde(default)]
    pub kind: SessionKind,
    /// Whether the session ran to its planned length or was stopped early.
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    #[default]
    Focus,
    Break,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Milliseconds since the Unix epoch.
pub fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A short, sortable, collision-resistant id.
///
/// Time-prefixed so ids sort roughly by creation, with a counter so two made in
/// the same millisecond still differ.
pub fn new_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    format!("{prefix}_{nanos:x}{seq:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_needs_only_a_title() {
        let task = Task::new("  Buy milk  ", "list_1");
        assert_eq!(task.title, "Buy milk");
        assert_eq!(task.status, Status::Todo);
        assert!(task.due.is_none());
        assert!(!task.today);
        assert!(task.completed_at.is_none());
    }

    #[test]
    fn ids_are_unique_within_the_same_instant() {
        let ids: Vec<String> = (0..500).map(|_| new_id("task")).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn default_board_has_the_usual_three_lists() {
        let board = Board::with_default_lists("Work", 0);
        let names: Vec<&str> = board.lists.iter().map(|list| list.name.as_str()).collect();
        assert_eq!(names, vec!["To Do", "Doing", "Done"]);
        assert_eq!(board.first_list_id(), Some(board.lists[0].id.as_str()));
        assert_eq!(board.lists[2].status, Some(Status::Done));
    }

    #[test]
    fn optional_fields_stay_out_of_the_json() {
        let task = Task::new("Simple", "list_1");
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("notes"), "absent fields must not be serialized: {json}");
        assert!(!json.contains("today"));
        assert!(!json.contains("externalId") && !json.contains("external_id"));
    }

    #[test]
    fn a_task_round_trips_through_json() {
        let mut task = Task::new("Round trip", "list_1");
        task.due = Some("2026-08-01".into());
        task.tags = vec!["work".into()];
        task.today = true;
        let json = serde_json::to_string(&task).unwrap();
        assert_eq!(serde_json::from_str::<Task>(&json).unwrap(), task);
    }

    #[test]
    fn due_dates_sort_correctly_as_strings() {
        let mut dates = ["2026-12-01", "2026-02-09", "2026-02-10"];
        dates.sort();
        assert_eq!(dates, ["2026-02-09", "2026-02-10", "2026-12-01"]);
    }
}
