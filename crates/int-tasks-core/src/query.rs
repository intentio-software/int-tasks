//! Views over the store: Today, filters, search and time summaries.
//!
//! Every date-aware query takes today's date as a `YYYY-MM-DD` string rather
//! than reading the clock. The core has no opinion about timezones, and a caller
//! that knows its own local date gets correct answers without this crate
//! carrying a date library — which also makes these tests deterministic.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{Session, Status, Task};
use crate::store::Data;

/// What Today shows, and why each task is on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TodayReason {
    /// Past its due date.
    Overdue,
    /// Due today.
    Due,
    /// Explicitly pulled onto today.
    Flagged,
    /// Already being worked on.
    InProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayEntry {
    #[serde(flatten)]
    pub task: Task,
    pub reason: TodayReason,
}

/// The Today list.
///
/// Deliberately narrow: overdue, due today, flagged, or in progress. A Today
/// list that quietly includes everything else is just the backlog, and stops
/// being worth opening.
pub fn today(data: &Data, today: &str) -> Vec<TodayEntry> {
    let mut entries: Vec<TodayEntry> = data
        .tasks
        .iter()
        .filter(|task| !task.status.is_done())
        .filter_map(|task| {
            let reason = match task.due.as_deref() {
                Some(due) if due < today => Some(TodayReason::Overdue),
                Some(due) if due == today => Some(TodayReason::Due),
                _ if task.today => Some(TodayReason::Flagged),
                _ if task.status == Status::Doing => Some(TodayReason::InProgress),
                _ => None,
            };
            reason.map(|reason| TodayEntry { task: task.clone(), reason })
        })
        .collect();

    // Overdue first, then in-progress, then priority, then oldest.
    entries.sort_by(|a, b| {
        rank(a.reason)
            .cmp(&rank(b.reason))
            .then_with(|| a.task.priority.unwrap_or(u8::MAX).cmp(&b.task.priority.unwrap_or(u8::MAX)))
            .then_with(|| a.task.created_at.cmp(&b.task.created_at))
    });
    entries
}

fn rank(reason: TodayReason) -> u8 {
    match reason {
        TodayReason::Overdue => 0,
        TodayReason::InProgress => 1,
        TodayReason::Due => 2,
        TodayReason::Flagged => 3,
    }
}

/// Filters for listing tasks.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub board_id: Option<String>,
    pub list_id: Option<String>,
    pub status: Option<Status>,
    pub tag: Option<String>,
    pub project: Option<String>,
    /// Text matched against title and notes, case-insensitively.
    pub query: Option<String>,
    /// Include completed tasks. Off by default — done work is rarely what you
    /// are looking for, and it swamps everything else.
    pub include_done: bool,
    pub limit: Option<usize>,
}

pub fn find(data: &Data, filter: &Filter) -> Vec<Task> {
    let needle = filter.query.as_ref().map(|q| q.to_lowercase());
    let board_lists: Option<Vec<String>> = filter.board_id.as_ref().and_then(|id| {
        data.board(id).map(|board| board.lists.iter().map(|list| list.id.clone()).collect())
    });

    let mut found: Vec<Task> = data
        .tasks
        .iter()
        .filter(|task| filter.include_done || !task.status.is_done())
        .filter(|task| filter.status.map(|status| task.status == status).unwrap_or(true))
        .filter(|task| filter.list_id.as_ref().map(|id| &task.list_id == id).unwrap_or(true))
        .filter(|task| {
            board_lists.as_ref().map(|lists| lists.contains(&task.list_id)).unwrap_or(true)
        })
        .filter(|task| {
            filter
                .project
                .as_ref()
                .map(|project| {
                    task.project.as_deref().map(|p| p.eq_ignore_ascii_case(project)).unwrap_or(false)
                })
                .unwrap_or(true)
        })
        .filter(|task| {
            filter
                .tag
                .as_ref()
                .map(|tag| task.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
                .unwrap_or(true)
        })
        .filter(|task| match &needle {
            Some(needle) => {
                task.title.to_lowercase().contains(needle)
                    || task.notes.as_deref().unwrap_or("").to_lowercase().contains(needle)
            }
            None => true,
        })
        .cloned()
        .collect();

    found.sort_by_key(|task| (task.list_id.clone(), task.order, task.created_at));
    if let Some(limit) = filter.limit {
        found.truncate(limit);
    }
    found
}

/// A project or tag, with how much work carries it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelUse {
    pub name: String,
    pub open: usize,
    pub done: usize,
}

/// Projects in use, with counts, sorted by name.
pub fn projects(data: &Data) -> Vec<LabelUse> {
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for task in &data.tasks {
        if let Some(project) = task.project.as_deref().filter(|p| !p.trim().is_empty()) {
            let entry = counts.entry(project.to_string()).or_insert((0, 0));
            if task.status.is_done() {
                entry.1 += 1;
            } else {
                entry.0 += 1;
            }
        }
    }
    into_sorted(counts)
}

/// Tags in use, with counts, sorted by name.
pub fn tags(data: &Data) -> Vec<LabelUse> {
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for task in &data.tasks {
        for tag in task.tags.iter().filter(|tag| !tag.trim().is_empty()) {
            let entry = counts.entry(tag.clone()).or_insert((0, 0));
            if task.status.is_done() {
                entry.1 += 1;
            } else {
                entry.0 += 1;
            }
        }
    }
    into_sorted(counts)
}

fn into_sorted(counts: HashMap<String, (usize, usize)>) -> Vec<LabelUse> {
    let mut labels: Vec<LabelUse> = counts
        .into_iter()
        .map(|(name, (open, done))| LabelUse { name, open, done })
        .collect();
    labels.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    labels
}

/// Whether a completed task has aged out of the views.
///
/// Only ever hidden, never removed: statistics, streaks and points all still
/// count it. Anything not completed is always visible.
pub fn is_stale_completion(
    task: &Task,
    today: &str,
    utc_offset_seconds: i32,
    settings: &crate::store::Settings,
) -> bool {
    if !task.status.is_done() {
        return false;
    }
    let Some(completed_at) = task.completed_at else {
        return false;
    };
    let finished = crate::dates::local_date(completed_at, utc_offset_seconds);
    // Working days strictly after the day it was finished, up to and including
    // today. Counted rather than subtracted so a weekend or a holiday in
    // between buys the task the time it should.
    let mut elapsed = 0u32;
    let mut cursor = today.to_string();
    for _ in 0..3_650 {
        if cursor <= finished {
            break;
        }
        if settings.is_working_day(&cursor) {
            elapsed += 1;
        }
        match crate::dates::days_before(&cursor, 1) {
            Some(previous) => cursor = previous,
            None => break,
        }
    }
    elapsed > settings.hide_completed_after_days
}

/// Time recorded against tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSummary {
    pub total_seconds: u64,
    pub focus_sessions: usize,
    /// Seconds per task id, highest first.
    pub by_task: Vec<TaskTime>,
    /// Focus time not attributed to any task.
    pub unattributed_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTime {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub seconds: u64,
    pub sessions: usize,
}

/// Summarize focus time, optionally within a millisecond range.
///
/// Breaks are excluded: they are time away from the work, and counting them
/// would flatter every report.
pub fn time_summary(data: &Data, sessions: &[Session], from: Option<u64>, to: Option<u64>) -> TimeSummary {
    let focus: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.kind == crate::model::SessionKind::Focus)
        .filter(|session| from.map(|from| session.started_at >= from).unwrap_or(true))
        .filter(|session| to.map(|to| session.started_at <= to).unwrap_or(true))
        .collect();

    let mut per_task: HashMap<&str, (u64, usize)> = HashMap::new();
    let mut unattributed = 0u64;
    for session in &focus {
        match session.task_id.as_deref() {
            Some(id) => {
                let entry = per_task.entry(id).or_insert((0, 0));
                entry.0 += session.seconds;
                entry.1 += 1;
            }
            None => unattributed += session.seconds,
        }
    }

    let mut by_task: Vec<TaskTime> = per_task
        .into_iter()
        .map(|(id, (seconds, count))| TaskTime {
            title: data.task(id).map(|task| task.title.clone()),
            task_id: id.to_string(),
            seconds,
            sessions: count,
        })
        .collect();
    by_task.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.task_id.cmp(&b.task_id)));

    TimeSummary {
        total_seconds: focus.iter().map(|session| session.seconds).sum(),
        focus_sessions: focus.len(),
        by_task,
        unattributed_seconds: unattributed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Board, SessionKind, Task};
    use crate::store::Data;

    fn data_with(tasks: Vec<Task>) -> Data {
        let board = Board::with_default_lists("Tasks", 0);
        Data { boards: vec![board], tasks, revision: 1, settings: Default::default() }
    }

    fn task(title: &str, due: Option<&str>, flagged: bool, status: Status) -> Task {
        let mut task = Task::new(title, "list_1");
        task.due = due.map(str::to_string);
        task.today = flagged;
        task.status = status;
        task
    }

    #[test]
    fn today_shows_overdue_due_flagged_and_in_progress() {
        let data = data_with(vec![
            task("Overdue", Some("2026-07-01"), false, Status::Todo),
            task("Due today", Some("2026-07-31"), false, Status::Todo),
            task("Flagged", None, true, Status::Todo),
            task("In progress", None, false, Status::Doing),
            task("Someday", None, false, Status::Todo),
            task("Future", Some("2026-12-01"), false, Status::Todo),
        ]);
        let titles: Vec<String> =
            today(&data, "2026-07-31").into_iter().map(|e| e.task.title).collect();

        assert!(!titles.contains(&"Someday".to_string()), "unscheduled work is not Today");
        assert!(!titles.contains(&"Future".to_string()), "future work is not Today");
        assert_eq!(titles.len(), 4);
    }

    #[test]
    fn today_puts_overdue_first_and_in_progress_next() {
        let data = data_with(vec![
            task("Flagged", None, true, Status::Todo),
            task("Due today", Some("2026-07-31"), false, Status::Todo),
            task("In progress", None, false, Status::Doing),
            task("Overdue", Some("2026-07-01"), false, Status::Todo),
        ]);
        let order: Vec<String> = today(&data, "2026-07-31").into_iter().map(|e| e.task.title).collect();
        assert_eq!(order, vec!["Overdue", "In progress", "Due today", "Flagged"]);
    }

    #[test]
    fn today_never_shows_completed_work() {
        let mut done = task("Finished", Some("2026-07-01"), true, Status::Done);
        done.completed_at = Some(1);
        let data = data_with(vec![done]);
        assert!(today(&data, "2026-07-31").is_empty());
    }

    #[test]
    fn today_reports_why_each_task_is_there() {
        let data = data_with(vec![task("Overdue", Some("2026-01-01"), false, Status::Todo)]);
        assert_eq!(today(&data, "2026-07-31")[0].reason, TodayReason::Overdue);
    }

    #[test]
    fn projects_and_tags_are_counted_by_state() {
        let mut open = task("Open", None, false, Status::Todo);
        open.project = Some("Intentio".into());
        open.tags = vec!["admin".into()];
        let mut closed = task("Closed", None, false, Status::Done);
        closed.project = Some("Intentio".into());
        closed.tags = vec!["admin".into(), "bug".into()];
        let data = data_with(vec![open, closed]);

        let projects = projects(&data);
        assert_eq!(projects.len(), 1);
        assert_eq!((projects[0].open, projects[0].done), (1, 1));

        let tags = tags(&data);
        assert_eq!(tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["admin", "bug"]);
    }

    /// Midday UTC on a date, so a test offset cannot shift the day.
    fn at(date: &str) -> u64 {
        (crate::dates::civil_days(date).unwrap() as u64) * 86_400_000 + 12 * 3_600_000
    }

    fn weekdays() -> crate::store::Settings {
        crate::store::Settings { hide_completed_after_days: 2, ..Default::default() }
    }

    #[test]
    fn a_completed_task_hides_only_after_the_window() {
        // 2026-08-03 to 2026-08-07 is a Monday to Friday.
        let mut done = task("Finished", None, false, Status::Done);
        done.completed_at = Some(at("2026-08-04"));

        assert!(
            !is_stale_completion(&done, "2026-08-05", 0, &weekdays()),
            "yesterday's work is still visible"
        );
        assert!(
            !is_stale_completion(&done, "2026-08-06", 0, &weekdays()),
            "two working days on, still inside the window"
        );
        assert!(
            is_stale_completion(&done, "2026-08-07", 0, &weekdays()),
            "three working days on, it drops out"
        );
    }

    #[test]
    fn a_weekend_does_not_count_against_the_window() {
        // Finished on Friday. Come Monday only one working day has passed, so
        // it is still there — which is the point of counting working days.
        let mut done = task("Friday finish", None, false, Status::Done);
        done.completed_at = Some(at("2026-07-31"));
        assert!(!is_stale_completion(&done, "2026-08-03", 0, &weekdays()), "Monday still shows it");
        assert!(!is_stale_completion(&done, "2026-08-04", 0, &weekdays()));
        assert!(is_stale_completion(&done, "2026-08-05", 0, &weekdays()), "Wednesday clears it");
    }

    #[test]
    fn a_holiday_buys_the_same_grace_as_a_weekend() {
        let mut settings = weekdays();
        settings.holidays = vec!["2026-08-05".to_string()];
        let mut done = task("Before the holiday", None, false, Status::Done);
        done.completed_at = Some(at("2026-08-03"));
        // Tue, [Wed holiday], Thu -> only two working days by Thursday.
        assert!(!is_stale_completion(&done, "2026-08-06", 0, &settings));
        assert!(is_stale_completion(&done, "2026-08-07", 0, &settings));
    }

    #[test]
    fn open_tasks_never_go_stale() {
        let open = task("Still going", None, false, Status::Todo);
        assert!(!is_stale_completion(&open, "2026-08-03", 0, &weekdays()));
    }

    #[test]
    fn a_completion_with_no_timestamp_is_kept() {
        // Older records may lack the stamp; vanishing them would look like loss.
        let mut done = task("Legacy", None, false, Status::Done);
        done.completed_at = None;
        assert!(!is_stale_completion(&done, "2026-08-03", 0, &weekdays()));
    }

    #[test]
    fn find_hides_done_tasks_unless_asked() {
        let data = data_with(vec![
            task("Open", None, false, Status::Todo),
            task("Closed", None, false, Status::Done),
        ]);
        assert_eq!(find(&data, &Filter::default()).len(), 1);
        let with_done = Filter { include_done: true, ..Default::default() };
        assert_eq!(find(&data, &with_done).len(), 2);
    }

    #[test]
    fn find_matches_title_and_notes_case_insensitively() {
        let mut with_notes = task("Unrelated", None, false, Status::Todo);
        with_notes.notes = Some("mentions Pomodoro inside".into());
        let data = data_with(vec![task("Pomodoro timer", None, false, Status::Todo), with_notes]);

        let filter = Filter { query: Some("pomodoro".into()), ..Default::default() };
        assert_eq!(find(&data, &filter).len(), 2);
    }

    #[test]
    fn find_filters_by_project() {
        let mut mine = task("Mine", None, false, Status::Todo);
        mine.project = Some("Intentio".into());
        let data = data_with(vec![mine, task("Someone else's", None, false, Status::Todo)]);

        let filter = Filter { project: Some("intentio".into()), ..Default::default() };
        assert_eq!(find(&data, &filter).len(), 1, "projects match case-insensitively");
    }

    #[test]
    fn find_filters_by_tag() {
        let mut tagged = task("Tagged", None, false, Status::Todo);
        tagged.tags = vec!["Work".into()];
        let data = data_with(vec![tagged, task("Untagged", None, false, Status::Todo)]);

        let filter = Filter { tag: Some("work".into()), ..Default::default() };
        assert_eq!(find(&data, &filter).len(), 1, "tags match case-insensitively");
    }

    #[test]
    fn time_summary_groups_by_task_and_excludes_breaks() {
        let mut task_a = Task::new("A", "list_1");
        task_a.id = "task_a".into();
        let data = data_with(vec![task_a]);

        let sessions = vec![
            Session { id: "s1".into(), task_id: Some("task_a".into()), started_at: 1000, ended_at: 2000, seconds: 1500, kind: SessionKind::Focus, completed: true },
            Session { id: "s2".into(), task_id: Some("task_a".into()), started_at: 3000, ended_at: 4000, seconds: 900, kind: SessionKind::Focus, completed: false },
            Session { id: "s3".into(), task_id: None, started_at: 5000, ended_at: 6000, seconds: 600, kind: SessionKind::Focus, completed: true },
            Session { id: "s4".into(), task_id: Some("task_a".into()), started_at: 7000, ended_at: 8000, seconds: 300, kind: SessionKind::Break, completed: true },
        ];

        let summary = time_summary(&data, &sessions, None, None);
        assert_eq!(summary.total_seconds, 3000, "breaks must not count as focus time");
        assert_eq!(summary.focus_sessions, 3);
        assert_eq!(summary.unattributed_seconds, 600);
        assert_eq!(summary.by_task[0].task_id, "task_a");
        assert_eq!(summary.by_task[0].seconds, 2400);
        assert_eq!(summary.by_task[0].title.as_deref(), Some("A"));
    }

    #[test]
    fn time_summary_respects_a_date_range() {
        let data = data_with(vec![]);
        let sessions = vec![
            Session { id: "s1".into(), task_id: None, started_at: 1000, ended_at: 2000, seconds: 60, kind: SessionKind::Focus, completed: true },
            Session { id: "s2".into(), task_id: None, started_at: 9000, ended_at: 9500, seconds: 60, kind: SessionKind::Focus, completed: true },
        ];
        let summary = time_summary(&data, &sessions, Some(5000), None);
        assert_eq!(summary.focus_sessions, 1);
    }
}
