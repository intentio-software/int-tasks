//! Reading and writing the task store.
//!
//! Two files, both plain text and both meant to survive being looked at:
//!
//! - `tasks.json` — boards, lists and tasks, rewritten whole on every change
//! - `sessions.jsonl` — one finished pomodoro session per line, append-only
//!
//! The desktop app and the MCP server both write here, so every mutation
//! re-reads from disk, applies its change and writes atomically via a temp file
//! and a rename. That keeps a crash or a concurrent writer from leaving a
//! half-written store, which for a single-user local app is the failure that
//! actually matters.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TaskError};
use crate::model::{Board, List, Session, Status, Task, new_id, now_millis};

pub const TASKS_FILE: &str = "tasks.json";
pub const SESSIONS_FILE: &str = "sessions.jsonl";

/// User preferences, kept alongside the data so there is one file to move.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Focus sessions aimed for each day.
    pub daily_focus_goal: u32,
    /// Days a completed task stays visible before it drops out of the views.
    ///
    /// It is only hidden, never deleted: history and statistics still count it,
    /// and a finished task you can no longer see is exactly what you want a
    /// week later and exactly what you do not want an hour later.
    #[serde(default = "default_hide_completed_after_days")]
    pub hide_completed_after_days: u32,
}

fn default_hide_completed_after_days() -> u32 {
    2
}

impl Default for Settings {
    fn default() -> Self {
        // Four twenty-five minute sessions is a realistic day of deep work, not
        // an aspirational one — a goal that is never met stops meaning anything.
        Settings { daily_focus_goal: 4, hide_completed_after_days: default_hide_completed_after_days() }
    }
}

/// Everything in `tasks.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub boards: Vec<Board>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    /// Bumped on every write, so a reader can tell whether it has stale data.
    #[serde(default)]
    pub revision: u64,
    /// Defaulted so a store written before settings existed still loads.
    #[serde(default)]
    pub settings: Settings,
}

impl Data {
    /// The store a brand-new install starts with.
    ///
    /// Seeded rather than empty because an empty board asks the user to make a
    /// decision before they can write anything down.
    pub fn seeded() -> Self {
        Data {
            boards: vec![Board::with_default_lists("Tasks", 0)],
            tasks: Vec::new(),
            revision: 1,
            settings: Settings::default(),
        }
    }

    pub fn board(&self, id: &str) -> Option<&Board> {
        self.boards.iter().find(|board| board.id == id)
    }

    /// The board a list belongs to.
    pub fn board_of_list(&self, list_id: &str) -> Option<&Board> {
        self.boards.iter().find(|board| board.lists.iter().any(|list| list.id == list_id))
    }

    pub fn list(&self, list_id: &str) -> Option<&List> {
        self.boards.iter().find_map(|board| board.list(list_id))
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    /// Tasks in a list, in display order.
    pub fn tasks_in_list(&self, list_id: &str) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.iter().filter(|task| task.list_id == list_id).collect();
        tasks.sort_by_key(|task| (task.order, task.created_at));
        tasks
    }

    /// The default landing place for a captured task: the first list of the
    /// first board.
    pub fn inbox_list_id(&self) -> Option<String> {
        self.boards
            .iter()
            .min_by_key(|board| board.order)
            .and_then(|board| board.first_list_id())
            .map(str::to_string)
    }

    /// Renumber a list's tasks 0..n so ordering stays dense and predictable.
    fn reindex(&mut self, list_id: &str) {
        let mut ids: Vec<String> =
            self.tasks_in_list(list_id).into_iter().map(|task| task.id.clone()).collect();
        ids.dedup();
        for (index, id) in ids.iter().enumerate() {
            if let Some(task) = self.task_mut(id) {
                task.order = index as u32;
            }
        }
    }
}

/// A task store rooted at a directory.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Open a store, creating the directory and a seeded `tasks.json` if needed.
    ///
    /// There is no setup step by design: the app should be usable the moment it
    /// opens, not after choosing where to keep things.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let store = Store { root };
        if !store.tasks_path().exists() {
            store.write(&Data::seeded())?;
        }
        Ok(store)
    }

    /// The default store location, used when nothing else is configured.
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .filter(|home| !home.as_os_str().is_empty())
            .map(|home| home.join(".intentio").join("tasks"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn tasks_path(&self) -> PathBuf {
        self.root.join(TASKS_FILE)
    }

    pub fn sessions_path(&self) -> PathBuf {
        self.root.join(SESSIONS_FILE)
    }

    /// Read the store. A missing file reads as a seeded store rather than an error.
    pub fn read(&self) -> Result<Data> {
        let path = self.tasks_path();
        if !path.exists() {
            return Ok(Data::seeded());
        }
        let text = fs::read_to_string(&path)?;
        if text.trim().is_empty() {
            return Ok(Data::seeded());
        }
        serde_json::from_str(&text).map_err(|err| TaskError::Corrupt(err.to_string()))
    }

    /// Write the store atomically.
    ///
    /// Serialize, write to a temp file in the same directory, then rename over
    /// the original — a rename within one filesystem is atomic, so a reader sees
    /// either the old file or the new one, never a truncated one.
    pub fn write(&self, data: &Data) -> Result<()> {
        let json = serde_json::to_string_pretty(data).map_err(|err| TaskError::Corrupt(err.to_string()))?;
        let target = self.tasks_path();
        let temp = self.root.join(format!(".{TASKS_FILE}.{}.tmp", std::process::id()));
        fs::write(&temp, format!("{json}\n"))?;
        fs::rename(&temp, &target)?;
        Ok(())
    }

    /// Apply a change: read the current store, mutate it, write it back.
    ///
    /// Always re-reading means a change made by the other process between calls
    /// is picked up rather than overwritten with a stale copy.
    pub fn update<T>(&self, change: impl FnOnce(&mut Data) -> Result<T>) -> Result<T> {
        let mut data = self.read()?;
        let outcome = change(&mut data)?;
        data.revision = data.revision.saturating_add(1);
        self.write(&data)?;
        Ok(outcome)
    }

    // -----------------------------------------------------------------------
    // task operations
    // -----------------------------------------------------------------------

    /// Capture a task from nothing but a title.
    pub fn add_task(&self, title: &str, list_id: Option<&str>) -> Result<Task> {
        let title = title.trim();
        if title.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        self.update(|data| {
            let list_id = match list_id {
                Some(id) => {
                    if data.list(id).is_none() {
                        return Err(TaskError::ListNotFound(id.to_string()));
                    }
                    id.to_string()
                }
                None => data.inbox_list_id().ok_or(TaskError::NoBoards)?,
            };

            let mut task = Task::new(title, &list_id);
            // New tasks go to the end of their list.
            task.order = data.tasks_in_list(&list_id).len() as u32;
            if let Some(status) = data.list(&list_id).and_then(|list| list.status) {
                task.status = status;
            }
            data.tasks.push(task.clone());
            Ok(task)
        })
    }

    /// Move a task to a list and position, applying that list's status.
    pub fn move_task(&self, task_id: &str, list_id: &str, position: Option<usize>) -> Result<Task> {
        self.update(|data| {
            if data.task(task_id).is_none() {
                return Err(TaskError::TaskNotFound(task_id.to_string()));
            }
            let status = match data.list(list_id) {
                Some(list) => list.status,
                None => return Err(TaskError::ListNotFound(list_id.to_string())),
            };

            let previous_list = data.task(task_id).map(|task| task.list_id.clone()).unwrap_or_default();
            // Where it lands: the given position, or the end of the list.
            let existing = data.tasks_in_list(list_id).len();
            let index = position.unwrap_or(existing).min(existing);

            // Push everything at or after the insertion point down one.
            let displaced: Vec<String> = data
                .tasks_in_list(list_id)
                .into_iter()
                .skip(index)
                .filter(|task| task.id != task_id)
                .map(|task| task.id.clone())
                .collect();
            for id in displaced {
                if let Some(task) = data.task_mut(&id) {
                    task.order += 1;
                }
            }

            let now = now_millis();
            let task = data.task_mut(task_id).expect("checked above");
            task.list_id = list_id.to_string();
            task.order = index as u32;
            if let Some(status) = status {
                task.status = status;
                task.completed_at = status.is_done().then_some(now);
                if !status.is_done() {
                    task.completed_at = None;
                }
            }
            task.updated_at = now;
            let moved = task.clone();

            data.reindex(&previous_list);
            data.reindex(list_id);
            Ok(moved)
        })
    }

    /// Mark done or not done, keeping the task's list in step where one maps.
    pub fn set_done(&self, task_id: &str, done: bool) -> Result<Task> {
        self.update(|data| {
            // Find a list on the same board whose status matches, so completing
            // a task also moves its card where the user would expect.
            let target_list = data
                .task(task_id)
                .and_then(|task| data.board_of_list(&task.list_id))
                .and_then(|board| {
                    let wanted = if done { Status::Done } else { Status::Todo };
                    board.lists.iter().find(|list| list.status == Some(wanted)).map(|l| l.id.clone())
                });

            let now = now_millis();
            let task = data.task_mut(task_id).ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))?;
            task.status = if done { Status::Done } else { Status::Todo };
            task.completed_at = done.then_some(now);
            task.updated_at = now;
            if let Some(list_id) = target_list {
                task.list_id = list_id;
            }
            let updated = task.clone();
            data.reindex(&updated.list_id);
            Ok(updated)
        })
    }

    pub fn delete_task(&self, task_id: &str) -> Result<Task> {
        self.update(|data| {
            let index = data
                .tasks
                .iter()
                .position(|task| task.id == task_id)
                .ok_or_else(|| TaskError::TaskNotFound(task_id.to_string()))?;
            let removed = data.tasks.remove(index);
            data.reindex(&removed.list_id);
            Ok(removed)
        })
    }

    // -----------------------------------------------------------------------
    // boards
    // -----------------------------------------------------------------------

    pub fn add_board(&self, name: &str) -> Result<Board> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        self.update(|data| {
            let board = Board::with_default_lists(name, data.boards.len() as u32);
            data.boards.push(board.clone());
            Ok(board)
        })
    }

    pub fn add_list(&self, board_id: &str, name: &str, status: Option<Status>) -> Result<List> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        self.update(|data| {
            let board = data
                .boards
                .iter_mut()
                .find(|board| board.id == board_id)
                .ok_or_else(|| TaskError::BoardNotFound(board_id.to_string()))?;
            let list = List::new(name, board.lists.len() as u32, status);
            board.lists.push(list.clone());
            Ok(list)
        })
    }

    /// Rename a project across every task carrying it.
    ///
    /// Returns how many tasks changed. Matching is case-insensitive so the
    /// near-duplicates that free text inevitably produces can be merged by
    /// renaming one onto the other.
    pub fn rename_project(&self, from: &str, to: &str) -> Result<usize> {
        let from = from.trim().to_string();
        let to = to.trim().to_string();
        if from.is_empty() || to.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        self.update(|data| {
            let mut changed = 0;
            for task in &mut data.tasks {
                let current = task.project.as_deref().unwrap_or("");
                // The match is case-insensitive so `apollo` folds into `Apollo`,
                // but a task already spelled exactly right is left alone —
                // touching `updated_at` on an unchanged task would be a phantom
                // edit for anything syncing on timestamps.
                if current.trim().eq_ignore_ascii_case(&from) && current != to {
                    task.project = Some(to.clone());
                    task.updated_at = now_millis();
                    changed += 1;
                }
            }
            Ok(changed)
        })
    }

    /// Remove a project from every task. The tasks themselves are untouched.
    pub fn delete_project(&self, name: &str) -> Result<usize> {
        let name = name.trim().to_string();
        self.update(|data| {
            let mut changed = 0;
            for task in &mut data.tasks {
                if task.project.as_deref().map(|p| p.trim().eq_ignore_ascii_case(&name)).unwrap_or(false) {
                    task.project = None;
                    task.updated_at = now_millis();
                    changed += 1;
                }
            }
            Ok(changed)
        })
    }

    /// Rename a tag everywhere, merging into an existing one if it collides.
    pub fn rename_tag(&self, from: &str, to: &str) -> Result<usize> {
        let from = from.trim().to_string();
        let to = to.trim().to_string();
        if from.is_empty() || to.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        self.update(|data| {
            let mut changed = 0;
            for task in &mut data.tasks {
                if !task.tags.iter().any(|tag| tag.trim().eq_ignore_ascii_case(&from)) {
                    continue;
                }
                // Already spelled exactly right, and nothing else to fold in.
                if task.tags.iter().any(|tag| tag == &to) && from.eq_ignore_ascii_case(&to) {
                    continue;
                }
                task.tags.retain(|tag| !tag.trim().eq_ignore_ascii_case(&from));
                // Renaming onto a tag the task already has must not duplicate it.
                if !task.tags.iter().any(|tag| tag.trim().eq_ignore_ascii_case(&to)) {
                    task.tags.push(to.clone());
                }
                task.tags.sort();
                task.updated_at = now_millis();
                changed += 1;
            }
            Ok(changed)
        })
    }

    /// Remove a tag from every task.
    pub fn delete_tag(&self, name: &str) -> Result<usize> {
        let name = name.trim().to_string();
        self.update(|data| {
            let mut changed = 0;
            for task in &mut data.tasks {
                let before = task.tags.len();
                task.tags.retain(|tag| !tag.trim().eq_ignore_ascii_case(&name));
                if task.tags.len() != before {
                    task.updated_at = now_millis();
                    changed += 1;
                }
            }
            Ok(changed)
        })
    }

    /// Change how long a completed task stays visible.
    pub fn set_hide_completed_after_days(&self, days: u32) -> Result<Settings> {
        self.update(|data| {
            data.settings.hide_completed_after_days = days.min(365);
            Ok(data.settings.clone())
        })
    }

    /// Change the daily focus goal. Zero would make the goal meaningless, so
    /// it is clamped to something achievable.
    pub fn set_daily_goal(&self, sessions: u32) -> Result<Settings> {
        self.update(|data| {
            data.settings.daily_focus_goal = sessions.clamp(1, 16);
            Ok(data.settings.clone())
        })
    }

    // -----------------------------------------------------------------------
    // sessions
    // -----------------------------------------------------------------------

    /// Append a finished session to the log.
    pub fn log_session(&self, session: &Session) -> Result<()> {
        let line = serde_json::to_string(session).map_err(|err| TaskError::Corrupt(err.to_string()))?;
        let mut file = OpenOptions::new().create(true).append(true).open(self.sessions_path())?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Every recorded session, oldest first.
    ///
    /// A line that will not parse is skipped rather than failing the read: one
    /// bad record should not make the whole history unreadable.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let path = self.sessions_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Session>(line).ok())
            .collect())
    }

    /// Attribute an already-logged session to a task.
    ///
    /// Rewrites the log, which is the one case where append-only is relaxed —
    /// a session started before deciding what it was for still has to land
    /// somewhere.
    pub fn assign_session(&self, session_id: &str, task_id: Option<&str>) -> Result<Session> {
        let mut sessions = self.sessions()?;
        let session = sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .ok_or_else(|| TaskError::SessionNotFound(session_id.to_string()))?;
        session.task_id = task_id.map(str::to_string);
        let updated = session.clone();

        let mut text = String::new();
        for session in &sessions {
            let line = serde_json::to_string(session).map_err(|err| TaskError::Corrupt(err.to_string()))?;
            text.push_str(&line);
            text.push('\n');
        }
        let temp = self.root.join(format!(".{SESSIONS_FILE}.{}.tmp", std::process::id()));
        fs::write(&temp, text)?;
        fs::rename(&temp, self.sessions_path())?;
        Ok(updated)
    }

    /// Build a session record for a stretch of work that just ended.
    pub fn finish_session(
        &self,
        task_id: Option<&str>,
        started_at: u64,
        seconds: u64,
        kind: crate::model::SessionKind,
        completed: bool,
    ) -> Result<Session> {
        let session = Session {
            id: new_id("session"),
            task_id: task_id.map(str::to_string),
            started_at,
            ended_at: started_at + seconds * 1000,
            seconds,
            kind,
            completed,
        };
        self.log_session(&session)?;
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionKind;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("int-tasks-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Store::open(&dir).expect("store")
    }

    #[test]
    fn a_new_store_is_usable_immediately() {
        let store = temp_store("seeded");
        let data = store.read().unwrap();
        assert_eq!(data.boards.len(), 1);
        assert_eq!(data.boards[0].lists.len(), 3);
        assert!(data.inbox_list_id().is_some(), "capture must have somewhere to go");
    }

    #[test]
    fn capture_needs_only_a_title() {
        let store = temp_store("capture");
        let task = store.add_task("Write the thing", None).unwrap();
        assert_eq!(task.title, "Write the thing");
        assert_eq!(store.read().unwrap().tasks.len(), 1);
        assert!(store.add_task("   ", None).is_err(), "an empty title is not a task");
    }

    #[test]
    fn captured_tasks_queue_in_order() {
        let store = temp_store("order");
        let first = store.add_task("First", None).unwrap();
        let second = store.add_task("Second", None).unwrap();
        assert_eq!(first.order, 0);
        assert_eq!(second.order, 1);
    }

    #[test]
    fn moving_to_a_done_list_completes_the_task() {
        let store = temp_store("move-done");
        let data = store.read().unwrap();
        let done_list = data.boards[0].lists.iter().find(|l| l.status == Some(Status::Done)).unwrap().id.clone();
        let task = store.add_task("Ship it", None).unwrap();

        let moved = store.move_task(&task.id, &done_list, None).unwrap();
        assert_eq!(moved.status, Status::Done);
        assert!(moved.completed_at.is_some());
    }

    #[test]
    fn moving_back_out_of_done_clears_completion() {
        let store = temp_store("move-undone");
        let data = store.read().unwrap();
        let done = data.boards[0].lists.iter().find(|l| l.status == Some(Status::Done)).unwrap().id.clone();
        let todo = data.boards[0].lists.iter().find(|l| l.status == Some(Status::Todo)).unwrap().id.clone();
        let task = store.add_task("Reopen me", None).unwrap();

        store.move_task(&task.id, &done, None).unwrap();
        let back = store.move_task(&task.id, &todo, None).unwrap();
        assert_eq!(back.status, Status::Todo);
        assert!(back.completed_at.is_none(), "a reopened task is not still completed");
    }

    #[test]
    fn completing_a_task_moves_it_to_the_done_list() {
        let store = temp_store("set-done");
        let task = store.add_task("Finish", None).unwrap();
        let done = store.set_done(&task.id, true).unwrap();

        let data = store.read().unwrap();
        let list = data.list(&done.list_id).unwrap();
        assert_eq!(list.status, Some(Status::Done));
        assert!(done.completed_at.is_some());
    }

    #[test]
    fn moving_into_a_position_pushes_the_rest_down() {
        let store = temp_store("insert");
        let list = store.read().unwrap().inbox_list_id().unwrap();
        let a = store.add_task("A", None).unwrap();
        let b = store.add_task("B", None).unwrap();
        let c = store.add_task("C", None).unwrap();

        // Put C at the front.
        store.move_task(&c.id, &list, Some(0)).unwrap();
        let data = store.read().unwrap();
        let order: Vec<&str> = data.tasks_in_list(&list).into_iter().map(|t| t.title.as_str()).collect();
        assert_eq!(order, vec!["C", "A", "B"]);
        assert_eq!(data.task(&a.id).unwrap().order, 1);
        assert_eq!(data.task(&b.id).unwrap().order, 2);
    }

    #[test]
    fn deleting_closes_the_gap_in_ordering() {
        let store = temp_store("delete");
        let list = store.read().unwrap().inbox_list_id().unwrap();
        store.add_task("A", None).unwrap();
        let b = store.add_task("B", None).unwrap();
        store.add_task("C", None).unwrap();

        store.delete_task(&b.id).unwrap();
        let data = store.read().unwrap();
        let orders: Vec<u32> = data.tasks_in_list(&list).into_iter().map(|t| t.order).collect();
        assert_eq!(orders, vec![0, 1], "ordering must stay dense");
    }

    #[test]
    fn unknown_ids_are_reported_rather_than_ignored() {
        let store = temp_store("missing");
        assert!(store.move_task("nope", "also-nope", None).is_err());
        assert!(store.delete_task("nope").is_err());
        assert!(store.set_done("nope", true).is_err());
    }

    #[test]
    fn every_write_bumps_the_revision() {
        let store = temp_store("revision");
        let before = store.read().unwrap().revision;
        store.add_task("Bump", None).unwrap();
        assert!(store.read().unwrap().revision > before);
    }

    #[test]
    fn a_write_leaves_no_temp_files_behind() {
        let store = temp_store("atomic");
        store.add_task("Clean up", None).unwrap();
        let strays: Vec<String> = fs::read_dir(store.root())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "temp files left behind: {strays:?}");
    }

    #[test]
    fn the_daily_goal_can_be_changed_and_is_kept_sane() {
        let store = temp_store("goal");
        assert_eq!(store.read().unwrap().settings.daily_focus_goal, 4);
        assert_eq!(store.set_daily_goal(6).unwrap().daily_focus_goal, 6);
        assert_eq!(store.read().unwrap().settings.daily_focus_goal, 6);
        // A goal of zero could never be met or missed.
        assert_eq!(store.set_daily_goal(0).unwrap().daily_focus_goal, 1);
    }

    #[test]
    fn a_store_written_before_settings_existed_still_loads() {
        let store = temp_store("legacy");
        fs::write(store.tasks_path(), r#"{"boards":[],"tasks":[],"revision":3}"#).unwrap();
        assert_eq!(store.read().unwrap().settings.daily_focus_goal, 4);
    }

    #[test]
    fn renaming_a_project_merges_case_variants() {
        let store = temp_store("rename-project");
        let a = store.add_task("One", None).unwrap();
        let b = store.add_task("Two", None).unwrap();
        store.update(|data| {
            data.task_mut(&a.id).unwrap().project = Some("Intentio".into());
            // The near-duplicate free text inevitably produces.
            data.task_mut(&b.id).unwrap().project = Some("intentio ".into());
            Ok(())
        }).unwrap();

        assert_eq!(store.rename_project("Intentio", "Intentio Suite").unwrap(), 2);
        let data = store.read().unwrap();
        assert!(data.tasks.iter().all(|t| t.project.as_deref() == Some("Intentio Suite")));
    }

    #[test]
    fn deleting_a_project_keeps_the_tasks() {
        let store = temp_store("delete-project");
        let task = store.add_task("Keep me", None).unwrap();
        store.update(|data| {
            data.task_mut(&task.id).unwrap().project = Some("Doomed".into());
            Ok(())
        }).unwrap();

        assert_eq!(store.delete_project("doomed").unwrap(), 1);
        let data = store.read().unwrap();
        assert_eq!(data.tasks.len(), 1, "removing a project must not remove its work");
        assert!(data.task(&task.id).unwrap().project.is_none());
    }

    #[test]
    fn renaming_a_tag_onto_an_existing_one_does_not_duplicate_it() {
        let store = temp_store("rename-tag");
        let task = store.add_task("Tagged", None).unwrap();
        store.update(|data| {
            data.task_mut(&task.id).unwrap().tags = vec!["bug".into(), "defect".into()];
            Ok(())
        }).unwrap();

        store.rename_tag("defect", "bug").unwrap();
        let data = store.read().unwrap();
        assert_eq!(data.task(&task.id).unwrap().tags, vec!["bug"]);
    }

    #[test]
    fn deleting_a_tag_leaves_the_others() {
        let store = temp_store("delete-tag");
        let task = store.add_task("Tagged", None).unwrap();
        store.update(|data| {
            data.task_mut(&task.id).unwrap().tags = vec!["bug".into(), "admin".into()];
            Ok(())
        }).unwrap();

        assert_eq!(store.delete_tag("BUG").unwrap(), 1);
        assert_eq!(store.read().unwrap().task(&task.id).unwrap().tags, vec!["admin"]);
    }

    #[test]
    fn the_completed_window_is_settable() {
        let store = temp_store("window");
        assert_eq!(store.read().unwrap().settings.hide_completed_after_days, 2);
        assert_eq!(store.set_hide_completed_after_days(7).unwrap().hide_completed_after_days, 7);
    }

    #[test]
    fn sessions_append_and_read_back() {
        let store = temp_store("sessions");
        let task = store.add_task("Focus on this", None).unwrap();
        store.finish_session(Some(&task.id), 1_000, 1500, SessionKind::Focus, true).unwrap();
        store.finish_session(None, 5_000, 300, SessionKind::Break, true).unwrap();

        let sessions = store.sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(sessions[0].seconds, 1500);
        assert_eq!(sessions[1].kind, SessionKind::Break);
    }

    #[test]
    fn an_unattributed_session_can_be_assigned_later() {
        let store = temp_store("assign");
        let task = store.add_task("Late attribution", None).unwrap();
        let session = store.finish_session(None, 1_000, 1500, SessionKind::Focus, true).unwrap();
        assert!(session.task_id.is_none());

        store.assign_session(&session.id, Some(&task.id)).unwrap();
        let sessions = store.sessions().unwrap();
        assert_eq!(sessions.len(), 1, "assigning must not duplicate the record");
        assert_eq!(sessions[0].task_id.as_deref(), Some(task.id.as_str()));
    }

    #[test]
    fn a_corrupt_session_line_does_not_hide_the_rest() {
        let store = temp_store("bad-line");
        store.finish_session(None, 1_000, 60, SessionKind::Focus, true).unwrap();
        let mut file = OpenOptions::new().append(true).open(store.sessions_path()).unwrap();
        writeln!(file, "{{ not json").unwrap();
        drop(file);
        store.finish_session(None, 2_000, 60, SessionKind::Focus, true).unwrap();

        assert_eq!(store.sessions().unwrap().len(), 2);
    }

    #[test]
    fn a_corrupt_store_is_reported_not_silently_reset() {
        let store = temp_store("corrupt");
        fs::write(store.tasks_path(), "{ this is not json").unwrap();
        // Losing someone's tasks to a silent re-seed would be far worse than an error.
        assert!(matches!(store.read(), Err(TaskError::Corrupt(_))));
    }
}
