//! Keeping a team's task stores in step with a Git remote.
//!
//! Ported from Intentio Knowledge, which does the same job for a notes vault.
//! The two are deliberately separate copies rather than a shared crate: they
//! live in different repositories, and a hundred lines duplicated is cheaper
//! than publishing and versioning a crate for two callers. If a third app needs
//! it, that calculation changes.
//!
//! What differs from Knowledge: this syncs the folder *above* the store, because
//! the repository holds everybody's store side by side and yours is one folder
//! within it. Pulling brings your colleagues' logs; the append-only task log
//! merges them by revision, which is what makes concurrent edits safe.
//!
//! Shelling out to the installed `git` rather than linking a library is a
//! deliberate choice: it inherits the user's SSH agent, credential helper and
//! config, so if `git push` works in their terminal it works here. A linked
//! library would have to reimplement all of that and would fail on exactly the
//! setups that are hardest to debug.
//!
//! The rules this follows, in order of importance:
//!
//! 1. Never resolve a conflict. A rebase that stops is aborted so the working
//!    tree is left exactly as it was, and sync pauses until a person fixes it.
//! 2. Never force anything. No `--force`, no history rewriting.
//! 3. Never touch a repository that is mid-operation. If the user is part-way
//!    through their own rebase or merge, this stays out of the way.
//! 4. Never create a repository. Turning a folder into one is the user's
//!    decision, not a side effect of ticking a box.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// What the app can tell about the team folder's relationship with Git.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub is_repo: bool,
    pub has_remote: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Uncommitted changes in the working tree.
    pub dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    /// Set when syncing cannot safely proceed, with the reason to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

/// What one sync attempt did.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    /// Whether anything was committed, pulled or pushed.
    pub changed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|err| format!("git could not be run: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// True while the repository is part-way through a rebase or a merge.
fn operation_in_progress(root: &Path) -> bool {
    let git_dir = root.join(".git");
    git_dir.join("rebase-merge").exists()
        || git_dir.join("rebase-apply").exists()
        || git_dir.join("MERGE_HEAD").exists()
        || git_dir.join("CHERRY_PICK_HEAD").exists()
}

pub fn status(root: &Path) -> SyncStatus {
    let mut status = SyncStatus::default();
    if git(root, &["rev-parse", "--is-inside-work-tree"]).is_err() {
        return status;
    }
    status.is_repo = true;
    status.has_remote = git(root, &["remote"]).map(|out| !out.is_empty()).unwrap_or(false);
    status.branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).ok();
    status.dirty = git(root, &["status", "--porcelain"]).map(|out| !out.is_empty()).unwrap_or(false);

    // `--left-right --count` against the upstream gives "behind<TAB>ahead".
    if let Ok(counts) = git(root, &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]) {
        let mut parts = counts.split_whitespace();
        status.behind = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        status.ahead = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    }

    if operation_in_progress(root) {
        status.blocked = Some("A rebase or merge is already in progress here.".into());
    } else if status.is_repo && !status.has_remote {
        status.blocked = Some("This folder has no Git remote to sync with.".into());
    }
    status
}

/// Commit local work, bring the remote's in, and push — or stop and say why.
pub fn sync(root: &Path) -> SyncOutcome {
    sync_with(root, true)
}

/// Sync without committing: pull the other side's work and push anything
/// already committed here, but leave the working tree alone.
///
/// This is what runs on the interval. Receiving is cheap and wants to be
/// frequent; committing is what fills the history and wants to wait until the
/// writing has stopped.
pub fn receive(root: &Path) -> SyncOutcome {
    sync_with(root, false)
}

fn sync_with(root: &Path, commit_local: bool) -> SyncOutcome {
    let before = status(root);
    if !before.is_repo {
        return blocked("This folder is not a Git repository.");
    }
    if !before.has_remote {
        return blocked("This folder has no Git remote to sync with.");
    }
    if operation_in_progress(root) {
        // The user is in the middle of something of their own.
        return blocked("A rebase or merge is in progress. Sync paused until it is finished.");
    }
    if let Err(err) = git(root, &["config", "user.email"]) {
        let _ = err;
        return blocked("Git has no user.email set, so it cannot commit. Set one and try again.");
    }

    let mut did_something = false;

    if before.dirty && commit_local {
        if let Err(err) = git(root, &["add", "-A"]) {
            return failed(&format!("Could not stage changes: {err}"));
        }
        let (subject, body) = commit_message(root);
        if let Err(err) = git(root, &["commit", "-m", &subject, "-m", &body]) {
            return failed(&format!("Could not commit: {err}"));
        }
        did_something = true;
    }

    match git(root, &["pull", "--rebase", "--autostash"]) {
        Ok(out) => {
            if !out.contains("Already up to date") {
                did_something = true;
            }
        }
        Err(err) => {
            // Leave the tree exactly as it was rather than half-rebased.
            if operation_in_progress(root) {
                let _ = git(root, &["rebase", "--abort"]);
                return blocked(
                    "The same note was changed in both places. Sync is paused — resolve it in a terminal, then sync again.",
                );
            }
            return failed(&format!("Could not pull: {err}"));
        }
    }

    if status(root).ahead > 0 {
        if let Err(err) = git(root, &["push"]) {
            return failed(&format!("Could not push: {err}"));
        }
        did_something = true;
    }

    SyncOutcome {
        changed: did_something,
        message: if did_something { "Synced".into() } else { "Already up to date".into() },
        blocked: None,
    }
}

/// The subject and body for one sync commit.
///
/// Git records its own timestamp, but `git log --oneline` — the view people
/// actually read — does not show it, so a run of automatic commits becomes a
/// wall of identical subjects. The time and the count go in the subject; the
/// files go in the body, where `git show` will find them.
fn commit_message(root: &Path) -> (String, String) {
    let staged = git(root, &["diff", "--cached", "--name-only"]).unwrap_or_default();
    let files: Vec<&str> = staged.lines().filter(|line| !line.trim().is_empty()).collect();
    let when = chrono::Local::now().format("%Y-%m-%d %H:%M");

    let subject = match files.len() {
        0 => format!("Task sync {when}"),
        1 => format!("Task sync {when} — {}", short_name(files[0])),
        n => format!("Task sync {when} — {n} files"),
    };
    (subject, files.join("\n"))
}

/// A note's name without its folders or extension, for the subject line.
fn short_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string()
}

fn blocked(reason: &str) -> SyncOutcome {
    SyncOutcome { changed: false, message: reason.to_string(), blocked: Some(reason.to_string()) }
}

fn failed(reason: &str) -> SyncOutcome {
    SyncOutcome { changed: false, message: reason.to_string(), blocked: Some(reason.to_string()) }
}

pub fn team_path(root: &str) -> PathBuf {
    PathBuf::from(root)
}

/// Emitted after every automatic sync so the UI can show where things stand.
pub const SYNC_EVENT: &str = "tasks-sync";

/// How still the folder must be before an automatic commit is made. A pause this
/// long usually means a thought was finished, which is the right unit for a
/// commit — and it collapses a whole writing session into one.
const QUIET_SECONDS: u64 = 120;

/// The longest the folder may stay uncommitted while being edited continuously.
/// Without this, someone writing all afternoon would sync nothing to anyone.
const MAX_UNCOMMITTED_SECONDS: u64 = 1_800;

/// Sync preferences. Kept beside the store rather than inside it: the store is
/// the thing being synced, so a preference living in it would be pushed to
/// everyone and argued over.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,
}

fn default_interval() -> u64 {
    180
}

impl Default for SyncSettings {
    fn default() -> Self {
        SyncSettings { enabled: false, interval_seconds: default_interval() }
    }
}

fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".intentio").join("tasks-sync.json"))
}

pub fn settings() -> SyncSettings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(next: &SyncSettings) -> std::io::Result<()> {
    let Some(path) = settings_path() else { return Ok(()) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(next).unwrap_or_default();
    std::fs::write(path, format!("{json}\n"))
}

/// Sync the team folder on its own schedule, for as long as the app is open.
///
/// Same shape as Knowledge: receive often, commit only once the writing has
/// stopped, so a working session becomes one commit rather than twenty.
pub fn spawn<R: tauri::Runtime>(app: tauri::AppHandle<R>, team_root: PathBuf) {
    std::thread::spawn(move || {
        let mut last_receive = std::time::Instant::now() - std::time::Duration::from_secs(3_600);
        let mut fingerprint = String::new();
        let mut unchanged_since: Option<std::time::Instant> = None;
        let mut dirty_since: Option<std::time::Instant> = None;

        loop {
            std::thread::sleep(std::time::Duration::from_secs(15));

            let config = settings();
            if !config.enabled {
                continue;
            }

            let current = git(&team_root, &["status", "--porcelain"]).unwrap_or_default();
            if current != fingerprint {
                fingerprint = current.clone();
                unchanged_since = Some(std::time::Instant::now());
            }
            if current.is_empty() {
                dirty_since = None;
            } else if dirty_since.is_none() {
                dirty_since = Some(std::time::Instant::now());
            }

            let settled = unchanged_since
                .map(|at| at.elapsed().as_secs() >= QUIET_SECONDS)
                .unwrap_or(false);
            let overdue = dirty_since
                .map(|at| at.elapsed().as_secs() >= MAX_UNCOMMITTED_SECONDS)
                .unwrap_or(false);
            let should_commit = !current.is_empty() && (settled || overdue);
            let due = last_receive.elapsed().as_secs() >= config.interval_seconds;
            if !should_commit && !due {
                continue;
            }

            let outcome = sync_with(&team_root, should_commit);
            last_receive = std::time::Instant::now();
            if should_commit {
                dirty_since = None;
            }
            let _ = tauri::Emitter::emit(&app, SYNC_EVENT, &outcome);
        }
    });
}
