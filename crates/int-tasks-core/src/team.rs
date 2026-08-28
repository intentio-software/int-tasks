//! Reading a whole team's task stores, not just your own.
//!
//! A team is a folder of person folders, each one an ordinary store:
//!
//! ```text
//! intentio-tasks/
//!   max/      tasks.jsonl · sessions.jsonl · meta.json
//!   mathew/   …
//!   vernon/   …
//! ```
//!
//! Members are found by looking at the siblings of your own store, so there is
//! nothing extra to configure: point the app at `intentio-tasks/max` and the
//! rest of the team appears. With the default store at `~/.intentio/tasks` the
//! siblings are unrelated folders holding no task log, so nothing is found and
//! the app stays single-user, which is correct.
//!
//! Focus sessions are deliberately not part of what a team shares. The point of
//! seeing each other's work is encouragement — what moved today — and hours at a
//! desk measure something else. A team folder should carry `sessions.jsonl` in
//! its `.gitignore`; `TEAM_GITIGNORE` below is the file to write.
//!
//! Everything here is read-only. Another person's store is theirs; the one
//! exception is assignment, which appends to their log through the ordinary
//! store API rather than reaching into their files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Task;
use crate::store::Store;

/// Someone whose tasks we can see.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    /// The folder name, which is how the team refers to each other.
    pub name: String,
    pub root: PathBuf,
    /// Whether this is the store the app itself is using.
    pub is_me: bool,
}

/// What a team folder's `.gitignore` should contain.
///
/// Task logs are shared and merge by appending. Session logs stay on the
/// machine that recorded them.
pub const TEAM_GITIGNORE: &str = "\
# Focus sessions stay on the machine that recorded them. Tasks are shared so
# the team can see what moved; hours at a desk are nobody else's business.
sessions.jsonl
";

/// What a team folder's `.gitattributes` should contain.
///
/// Without the union driver Git treats two people appending to a log as a
/// conflict, when appending is exactly what makes them safe to merge.
pub const TEAM_GITATTRIBUTES: &str = "*.jsonl merge=union\n";

/// A store folder is one that holds a task log.
fn is_store(path: &Path) -> bool {
    path.join(crate::store::TASKS_FILE).is_file()
        || path.join(crate::store::LEGACY_TASKS_FILE).is_file()
}

/// Everyone whose store sits beside yours, you included, in name order.
///
/// Returns empty when yours is the only one — a single-user setup should show
/// no team, not a team of one.
pub fn members(my_root: &Path) -> Vec<Member> {
    let Some(parent) = my_root.parent() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };

    let mut found: Vec<Member> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && is_store(path))
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().to_string();
            // Dot folders are machinery, not people.
            if name.starts_with('.') {
                return None;
            }
            let is_me = same_folder(&path, my_root);
            Some(Member { name, root: path, is_me })
        })
        .collect();

    if found.len() < 2 {
        return Vec::new();
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Compare by canonical path where possible, so `./max` and `max` are one place.
fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Open a member's store read-only.
///
/// `Store::open` would seed an empty store in a folder that has none, which is
/// not something to do inside someone else's directory, so membership is
/// established by `is_store` before this is ever called.
pub fn open_member(member: &Member) -> crate::error::Result<Store> {
    Store::open(&member.root)
}

/// Tasks in a member's store that someone else put there.
pub fn assigned_to(member: &Member, tasks: &[Task]) -> Vec<Task> {
    tasks
        .iter()
        .filter(|task| task.assigned_by.is_some() && !task.status.is_done())
        .filter(|task| {
            // An assignee is recorded when known, but a task appended to a
            // person's own store is theirs whether or not it says so.
            task.assignee.as_deref().map(|who| who == member.name).unwrap_or(true)
        })
        .cloned()
        .collect()
}

/// Put a task into a colleague's store.
///
/// The task is appended to their log through the ordinary store API, so it
/// merges like anything else they wrote and carries a revision they can edit
/// over. It lands in their inbox rather than a list of ours — their board is
/// theirs, and guessing at its shape would put work somewhere they never look.
///
/// `from` is recorded so an unexpected task is never mysterious.
pub fn assign(member: &Member, title: &str, from: &str, today: &str) -> crate::error::Result<Task> {
    let store = open_member(member)?;
    let task = store.capture(title, None, today)?;
    store.update(|data| {
        let stored = data
            .task_mut(&task.id)
            .ok_or_else(|| crate::error::TaskError::TaskNotFound(task.id.clone()))?;
        stored.assigned_by = Some(from.to_string());
        // A line that already named someone is honoured; otherwise it is theirs.
        if stored.assignee.is_none() {
            stored.assignee = Some(member.name.clone());
        }
        Ok(stored.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("int-tasks-team-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_team_is_the_folders_beside_your_own() {
        let root = team("basic");
        for who in ["max", "mathew", "vernon"] {
            Store::open(root.join(who)).unwrap();
        }
        let found = members(&root.join("max"));
        assert_eq!(found.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["mathew", "max", "vernon"]);
        assert_eq!(found.iter().filter(|m| m.is_me).count(), 1);
        assert!(found.iter().find(|m| m.name == "max").unwrap().is_me);
    }

    #[test]
    fn a_folder_with_no_task_log_is_not_a_person() {
        let root = team("mixed");
        Store::open(root.join("max")).unwrap();
        Store::open(root.join("mathew")).unwrap();
        std::fs::create_dir_all(root.join("attachments")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();

        let names: Vec<String> = members(&root.join("max")).into_iter().map(|m| m.name).collect();
        assert_eq!(names, vec!["mathew", "max"]);
    }

    #[test]
    fn working_alone_is_not_a_team_of_one() {
        // The default store sits beside unrelated folders; finding "a team"
        // there would put a team view in front of someone who has no team.
        let root = team("solo");
        Store::open(root.join("tasks")).unwrap();
        std::fs::create_dir_all(root.join("knowledge")).unwrap();
        assert!(members(&root.join("tasks")).is_empty());
    }

    #[test]
    fn assigning_puts_the_task_in_their_store_not_ours() {
        let root = team("assign");
        let mine = Store::open(root.join("max")).unwrap();
        let theirs = Store::open(root.join("mathew")).unwrap();

        let member = Member { name: "mathew".into(), root: root.join("mathew"), is_me: false };
        let task = assign(&member, "(stm) review the migration plan", "max", "2026-08-28").unwrap();

        assert_eq!(task.title, "review the migration plan", "the line is read as usual");
        assert_eq!(task.project.as_deref(), Some("stm"));
        assert_eq!(task.assigned_by.as_deref(), Some("max"));
        assert_eq!(task.assignee.as_deref(), Some("mathew"));

        assert!(theirs.read().unwrap().tasks.iter().any(|t| t.id == task.id), "it is in theirs");
        assert!(mine.read().unwrap().tasks.is_empty(), "and not in mine");
    }

    #[test]
    fn an_owner_written_into_the_line_is_honoured() {
        let root = team("assign-named");
        Store::open(root.join("max")).unwrap();
        Store::open(root.join("mathew")).unwrap();
        let member = Member { name: "mathew".into(), root: root.join("mathew"), is_me: false };

        // Handing something to Mathew that is ultimately for Vernon.
        let task = assign(&member, "chase the invoice @vernon", "max", "2026-08-28").unwrap();
        assert_eq!(task.assignee.as_deref(), Some("vernon"), "the line wins");
        assert_eq!(task.assigned_by.as_deref(), Some("max"));
    }

    #[test]
    fn only_work_someone_else_handed_over_counts_as_assigned() {
        let root = team("assigned");
        let mine = Store::open(root.join("mathew")).unwrap();
        let own = mine.add_task("Something I thought of", None).unwrap();
        let handed = mine.add_task("Something Max asked for", None).unwrap();
        mine.update(|data| {
            let task = data.task_mut(&handed.id).unwrap();
            task.assigned_by = Some("max".into());
            task.assignee = Some("mathew".into());
            Ok(())
        })
        .unwrap();

        let member = Member { name: "mathew".into(), root: root.join("mathew"), is_me: false };
        let tasks = mine.read().unwrap().tasks;
        let assigned = assigned_to(&member, &tasks);
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].id, handed.id);
        assert!(!assigned.iter().any(|t| t.id == own.id));
    }
}
