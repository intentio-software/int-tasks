//! Tauri commands backing the Intentio Tasks desktop app.
//!
//! Every command goes through `int-tasks-core` — the same crate the MCP server
//! uses — so the app and an agent see one store with one set of rules.

pub mod knowledge_bridge;
mod git_sync;
mod menu;
mod timer;

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, State};

use int_tasks_core::{
    Data, Filter, LabelUse, Plotted, Session, SessionKind, Stats, Store, Task, TimeSummary,
    DayProgress, TodayEntry, matrix, query, stats,
};
use timer::{Timer, TimerState};

/// Long-lived app state: the store, the timer, and the tray the timer writes to.
pub struct AppState {
    store: Store,
    timer: Arc<Timer>,
    tray: Mutex<Option<TrayIcon>>,
}

impl AppState {
    fn tray(&self) -> Option<TrayIcon> {
        self.tray.lock().ok().and_then(|tray| tray.clone())
    }
}

/// Everything the UI needs for a render, fetched in one call.
///
/// The alternative is four round trips on every change, which for a list that
/// updates on each keystroke is needless latency.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub data: Data,
    pub today: Vec<TodayEntry>,
    pub timer: TimerState,
    pub summary: TimeSummary,
    /// Open, scored tasks placed on the impact/effort matrix.
    pub matrix: Vec<Plotted>,
    pub stats: Stats,
    pub progress: Vec<DayProgress>,
    pub projects: Vec<LabelUse>,
    pub tags: Vec<LabelUse>,
    pub settings: int_tasks_core::Settings,
    /// Local date the Today list was built for, so the UI can notice midnight.
    pub date: String,
}

/// Seconds this machine is ahead of UTC, so a streak counts the user's days
/// rather than UTC's.
fn utc_offset_seconds() -> i32 {
    chrono::Local::now().offset().local_minus_utc()
}

fn today_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn fail(err: int_tasks_core::TaskError) -> String {
    err.to_string()
}

#[tauri::command]
fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let data = state.store.read().map_err(fail)?;
    let sessions = state.store.sessions().map_err(fail)?;
    let date = today_date();
    // Everything derived is computed from the full store, before completed work
    // ages out of the view — otherwise hiding a task would quietly rewrite the
    // statistics it contributed to.
    let projects = query::projects(&data);
    let tags = query::tags(&data);
    let settings = data.settings.clone();

    let snapshot_today = query::today(&data, &date);
    let snapshot_matrix = matrix::plot(&data, &date);
    let snapshot_stats =
        stats::stats(&data, &sessions, &date, utc_offset_seconds(), data.settings.daily_focus_goal);
    let summary = query::time_summary(&data, &sessions, None, None);
    // Ten working days: enough to see a direction, short enough to read at a glance.
    let progress = stats::recent_progress(&data, &sessions, &date, utc_offset_seconds(), 10);

    let mut data = data;
    data.tasks
        .retain(|task| !query::is_stale_completion(task, &date, utc_offset_seconds(), &settings));

    Ok(Snapshot {
        today: snapshot_today,
        matrix: snapshot_matrix,
        stats: snapshot_stats,
        progress,
        summary,
        projects,
        tags,
        settings,
        timer: state.timer.snapshot(),
        data,
        date,
    })
}

/// Rename a project across every task carrying it.
#[tauri::command]
fn rename_project(state: State<'_, AppState>, from: String, to: String) -> Result<usize, String> {
    state.store.rename_project(&from, &to).map_err(fail)
}

/// Remove a project from its tasks. The tasks themselves stay.
#[tauri::command]
fn delete_project(state: State<'_, AppState>, name: String) -> Result<usize, String> {
    state.store.delete_project(&name).map_err(fail)
}

#[tauri::command]
fn rename_tag(state: State<'_, AppState>, from: String, to: String) -> Result<usize, String> {
    state.store.rename_tag(&from, &to).map_err(fail)
}

#[tauri::command]
fn delete_tag(state: State<'_, AppState>, name: String) -> Result<usize, String> {
    state.store.delete_tag(&name).map_err(fail)
}

/// How long a completed task stays visible.
#[tauri::command]
fn set_working_days(state: State<'_, AppState>, days: Vec<u8>) -> Result<(), String> {
    state.store.set_working_days(days).map(|_| ()).map_err(fail)
}

#[tauri::command]
fn set_holidays(state: State<'_, AppState>, holidays: Vec<String>) -> Result<(), String> {
    state.store.set_holidays(holidays).map(|_| ()).map_err(fail)
}

#[tauri::command]
fn set_hide_completed_after_days(
    state: State<'_, AppState>,
    days: u32,
) -> Result<int_tasks_core::Settings, String> {
    state.store.set_hide_completed_after_days(days).map_err(fail)
}


#[tauri::command]
fn add_task(state: State<'_, AppState>, title: String, list_id: Option<String>) -> Result<Task, String> {
    state.store.capture(&title, list_id.as_deref()).map_err(fail)
}

#[tauri::command]
fn set_done(state: State<'_, AppState>, task_id: String, done: bool) -> Result<Task, String> {
    state.store.set_done(&task_id, done).map_err(fail)
}

#[tauri::command]
fn move_task(
    state: State<'_, AppState>,
    task_id: String,
    list_id: String,
    position: Option<usize>,
) -> Result<Task, String> {
    state.store.move_task(&task_id, &list_id, position).map_err(fail)
}

#[tauri::command]
fn delete_task(state: State<'_, AppState>, task_id: String) -> Result<Task, String> {
    state.store.delete_task(&task_id).map_err(fail)
}

/// Patch a task from the UI. Absent fields are left alone; `null` clears.
#[tauri::command]
fn update_task(
    state: State<'_, AppState>,
    task_id: String,
    title: Option<String>,
    notes: Option<Option<String>>,
    due: Option<Option<String>>,
    today: Option<bool>,
    tags: Option<Vec<String>>,
    project: Option<Option<String>>,
    priority: Option<Option<u8>>,
    estimate_minutes: Option<Option<u32>>,
) -> Result<Task, String> {
    state
        .store
        .update(|data| {
            let task = data
                .task_mut(&task_id)
                .ok_or_else(|| int_tasks_core::TaskError::TaskNotFound(task_id.clone()))?;
            if let Some(title) = title {
                task.title = title;
            }
            if let Some(notes) = notes {
                task.notes = notes;
            }
            if let Some(due) = due {
                task.due = due;
            }
            if let Some(today) = today {
                task.today = today;
            }
            if let Some(tags) = tags {
                task.tags = tags;
            }
            if let Some(project) = project {
                task.project = project.filter(|value| !value.trim().is_empty());
            }
            if let Some(priority) = priority {
                task.priority = priority;
            }
            if let Some(estimate) = estimate_minutes {
                task.estimate_minutes = estimate;
            }
            task.touch();
            Ok(task.clone())
        })
        .map_err(fail)
}

/// Set the matrix scores. Passing null clears one, which takes the task off
/// the matrix rather than parking it at zero.
#[tauri::command]
fn score_task(
    state: State<'_, AppState>,
    task_id: String,
    impact: Option<Option<u8>>,
    effort: Option<Option<u8>>,
) -> Result<Task, String> {
    state
        .store
        .update(|data| {
            let task = data
                .task_mut(&task_id)
                .ok_or_else(|| int_tasks_core::TaskError::TaskNotFound(task_id.clone()))?;
            if let Some(impact) = impact {
                task.impact = impact.map(|value| value.clamp(1, 10));
            }
            if let Some(effort) = effort {
                task.effort = effort.map(|value| value.clamp(1, 10));
            }
            task.touch();
            Ok(task.clone())
        })
        .map_err(fail)
}

/// Something worth doing now. `lowEnergy` asks for the cheapest thing that
/// still pays rather than the most valuable.
#[tauri::command]
fn suggest_task(state: State<'_, AppState>, low_energy: bool) -> Result<Option<Plotted>, String> {
    let data = state.store.read().map_err(fail)?;
    Ok(matrix::suggest(&data, &today_date(), low_energy))
}

/// Change how many focus sessions a day aims for.
#[tauri::command]
fn set_daily_goal(state: State<'_, AppState>, sessions: u32) -> Result<int_tasks_core::Settings, String> {
    state.store.set_daily_goal(sessions).map_err(fail)
}

#[tauri::command]
fn add_board(state: State<'_, AppState>, name: String) -> Result<int_tasks_core::Board, String> {
    state.store.add_board(&name).map_err(fail)
}

#[tauri::command]
fn add_list(
    state: State<'_, AppState>,
    board_id: String,
    name: String,
) -> Result<int_tasks_core::List, String> {
    state.store.add_list(&board_id, &name, None).map_err(fail)
}

#[tauri::command]
fn find_tasks(state: State<'_, AppState>, query_text: Option<String>, include_done: bool) -> Result<Vec<Task>, String> {
    let data = state.store.read().map_err(fail)?;
    Ok(query::find(&data, &Filter { query: query_text, include_done, ..Default::default() }))
}

// ---------------------------------------------------------------------------
// timer
// ---------------------------------------------------------------------------

#[tauri::command]
fn start_timer(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: Option<String>,
    minutes: Option<u64>,
    break_session: Option<bool>,
) -> Result<TimerState, String> {
    // Resolve the title now so the tray and UI can name the task without
    // re-reading the store on every tick.
    let task_title = match &task_id {
        Some(id) => state.store.read().ok().and_then(|data| data.task(id).map(|t| t.title.clone())),
        None => None,
    };
    let kind = if break_session.unwrap_or(false) { SessionKind::Break } else { SessionKind::Focus };
    Ok(timer::start(
        &app,
        state.timer.clone(),
        state.tray(),
        state.store.clone(),
        task_id,
        task_title,
        minutes,
        kind,
    ))
}

#[tauri::command]
fn stop_timer(app: AppHandle, state: State<'_, AppState>) -> Result<TimerState, String> {
    Ok(timer::stop(&app, state.timer.clone(), state.tray(), state.store.clone(), true))
}

#[tauri::command]
fn pause_timer(app: AppHandle, state: State<'_, AppState>) -> TimerState {
    timer::pause(&app, state.timer.clone(), state.tray())
}

#[tauri::command]
fn resume_timer(app: AppHandle, state: State<'_, AppState>) -> TimerState {
    timer::resume(&app, state.timer.clone(), state.tray())
}

#[tauri::command]
fn timer_state(state: State<'_, AppState>) -> TimerState {
    state.timer.snapshot()
}

#[tauri::command]
fn sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    state.store.sessions().map_err(fail)
}

/// Attribute a session that was recorded without a task.
#[tauri::command]
fn assign_session(state: State<'_, AppState>, session_id: String, task_id: Option<String>) -> Result<Session, String> {
    state.store.assign_session(&session_id, task_id.as_deref()).map_err(fail)
}

#[tauri::command]
fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<Session, String> {
    state.store.delete_session(&session_id).map_err(fail)
}

/// One colleague, with enough to answer "how are they doing".
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamMember {
    name: String,
    is_me: bool,
    stats: Stats,
    /// What they are working on today.
    today: Vec<TodayEntry>,
    /// Open work someone else handed them.
    assigned: Vec<Task>,
    /// Set when their store could not be read, rather than showing them as idle.
    unavailable: Option<String>,
}

/// Everyone whose store sits beside this one.
///
/// Reading a colleague's store is reading their files, so this only ever reads:
/// no store is created, seeded or written here.
#[tauri::command]
fn team(state: State<'_, AppState>) -> Vec<TeamMember> {
    let date = today_date();
    let offset = utc_offset_seconds();
    int_tasks_core::team::members(state.store.root())
        .into_iter()
        .map(|member| {
            let mut entry = TeamMember {
                name: member.name.clone(),
                is_me: member.is_me,
                stats: Stats::default(),
                today: Vec::new(),
                assigned: Vec::new(),
                unavailable: None,
            };
            match int_tasks_core::team::open_member(&member)
                .and_then(|store| Ok((store.read()?, store.sessions()?)))
            {
                Ok((data, sessions)) => {
                    entry.stats = stats::stats(&data, &sessions, &date, offset, data.settings.daily_focus_goal);
                    entry.today = query::today(&data, &date);
                    entry.assigned = int_tasks_core::team::assigned_to(&member, &data.tasks);
                }
                Err(err) => entry.unavailable = Some(err.to_string()),
            }
            entry
        })
        .collect()
}

/// Where the team repository is: the folder holding everyone's store.
fn team_root(state: &AppState) -> std::path::PathBuf {
    state.store.root().parent().map(|p| p.to_path_buf()).unwrap_or_else(|| state.store.root().to_path_buf())
}

/// How the team folder stands with its remote.
#[tauri::command]
fn tasks_sync_status(state: State<'_, AppState>) -> serde_json::Value {
    let root = team_root(&state);
    let status = git_sync::status(&root);
    let config = git_sync::settings();
    serde_json::json!({
        "status": status,
        "settings": config,
        "root": root.to_string_lossy(),
    })
}

/// Sync now: bring colleagues' work in and send yours out.
#[tauri::command]
async fn tasks_sync_now(app: AppHandle) -> git_sync::SyncOutcome {
    let root = {
        let state = app.state::<AppState>();
        team_root(&state)
    };
    tauri::async_runtime::spawn_blocking(move || git_sync::sync(&root))
        .await
        .unwrap_or_else(|err| git_sync::SyncOutcome {
            changed: false,
            message: format!("Sync did not run: {err}"),
            blocked: Some("Sync did not run.".into()),
        })
}

/// Turn syncing on or off, and set how often colleagues' work is fetched.
#[tauri::command]
fn set_tasks_sync(enabled: bool, interval_seconds: Option<u64>) -> Result<(), String> {
    let current = git_sync::settings();
    git_sync::save_settings(&git_sync::SyncSettings {
        enabled,
        interval_seconds: interval_seconds.unwrap_or(current.interval_seconds).max(60),
    })
    .map_err(|err| err.to_string())
}

/// Hand a task to a colleague. The line is read exactly as capture reads it.
#[tauri::command]
fn assign_to(state: State<'_, AppState>, member: String, line: String) -> Result<Task, String> {
    let me = state
        .store
        .root()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "someone".into());
    let target = int_tasks_core::team::members(state.store.root())
        .into_iter()
        .find(|candidate| candidate.name == member)
        .ok_or_else(|| format!("no team member called {member}"))?;
    if target.is_me {
        return Err("that is your own store — capture it normally".into());
    }
    int_tasks_core::team::assign(&target, &line, &me).map_err(fail)
}

/// Where this app's store lives, and whether it was chosen or defaulted.
#[tauri::command]
fn store_root(state: State<'_, AppState>) -> serde_json::Value {
    serde_json::json!({
        "root": state.store.root().to_string_lossy(),
        "chosen": Store::root_override().is_some(),
    })
}

/// Point the app at a different store folder. Takes effect on restart.
#[tauri::command]
fn set_store_root(root: Option<String>) -> Result<(), String> {
    let path = root.as_deref().map(std::path::Path::new);
    if let Some(path) = path {
        if !path.is_dir() {
            return Err(format!("{} is not a folder", path.display()));
        }
    }
    Store::set_root_override(path).map_err(fail)
}

#[tauri::command]
fn store_path(state: State<'_, AppState>) -> String {
    state.store.root().to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// chrome
// ---------------------------------------------------------------------------

/// Whether this platform has a native menu bar, so the web layer knows whether
/// to own its keyboard shortcuts.
#[tauri::command]
fn has_native_menu() -> bool {
    cfg!(desktop)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let root = Store::configured_root().expect("a home directory");
    let store = Store::open(&root).expect("task store");

    tauri::Builder::default()
        .manage(AppState {
            store: store.clone(),
            timer: Arc::new(Timer::default()),
            tray: Mutex::new(None),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let handle = app.handle();
            menu::install(handle)?;

            // The tray is where the countdown lives, so the timer is visible
            // with the window closed.
            // A template image carries coverage, not colour: macOS tints it to
            // suit a light or dark menu bar. Handing it the full app icon made
            // every coloured pixel opaque, which is why it showed as a block.
            // scripts/make-tray-icon.py regenerates this glyph.
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
                .expect("tray icon");
            let tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Intentio Tasks")
                // The menu is what a left click opens; the icon itself no longer
                // raises the window, because a menu that appears on the way to
                // the window would be in the way rather than useful.
                .on_menu_event(|app, event| {
                    let Some(state) = app.try_state::<AppState>() else { return };
                    match event.id().as_ref() {
                        "tray-focus" => {
                            // Whatever is at the top of Today is very likely what
                            // the session is for; if Today is empty the session is
                            // unattributed and will ask when it ends.
                            let (task_id, task_title) = state
                                .store
                                .read()
                                .ok()
                                .map(|data| {
                                    query::today(&data, &today_date())
                                        .first()
                                        .map(|entry| {
                                            (Some(entry.task.id.clone()), Some(entry.task.title.clone()))
                                        })
                                        .unwrap_or((None, None))
                                })
                                .unwrap_or((None, None));
                            timer::start(
                                app,
                                state.timer.clone(),
                                state.tray(),
                                state.store.clone(),
                                task_id,
                                task_title,
                                None,
                                SessionKind::Focus,
                            );
                        }
                        "tray-break" => {
                            timer::start(
                                app,
                                state.timer.clone(),
                                state.tray(),
                                state.store.clone(),
                                None,
                                None,
                                None,
                                SessionKind::Break,
                            );
                        }
                        "tray-pause" => {
                            timer::pause(app, state.timer.clone(), state.tray());
                        }
                        "tray-resume" => {
                            timer::resume(app, state.timer.clone(), state.tray());
                        }
                        "tray-stop" => {
                            timer::stop(app, state.timer.clone(), state.tray(), state.store.clone(), true);
                        }
                        "tray-show" => timer::focus_window(app),
                        _ => {}
                    }
                })
                .build(app)?;

            if let Some(state) = app.try_state::<AppState>() {
                // Sync the folder holding every store, not just this one.
                let root = state.store.root().parent().map(|p| p.to_path_buf());
                if let Some(root) = root {
                    git_sync::spawn(handle.clone(), root);
                }
                timer::init_tray(handle, &state.timer, &Some(tray.clone()));
                if let Ok(mut slot) = state.tray.lock() {
                    *slot = Some(tray);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            add_task,
            set_done,
            move_task,
            delete_task,
            update_task,
            score_task,
            suggest_task,
            set_daily_goal,
            set_hide_completed_after_days,
            set_working_days,
            set_holidays,
            rename_project,
            delete_project,
            rename_tag,
            delete_tag,
            add_board,
            add_list,
            find_tasks,
            start_timer,
            stop_timer,
            pause_timer,
            resume_timer,
            timer_state,
            sessions,
            assign_session,
            delete_session,
            store_path,
            team,
            assign_to,
            store_root,
            set_store_root,
            tasks_sync_status,
            tasks_sync_now,
            set_tasks_sync,
            knowledge_bridge::task_context,
            has_native_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
