//! Tauri commands backing the Intentio Tasks desktop app.
//!
//! Every command goes through `int-tasks-core` — the same crate the MCP server
//! uses — so the app and an agent see one store with one set of rules.

mod timer;

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, State};

use int_tasks_core::{
    Data, Filter, Plotted, Session, SessionKind, Stats, Store, Task, TimeSummary, TodayEntry, matrix,
    query, stats,
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
    Ok(Snapshot {
        today: query::today(&data, &date),
        summary: query::time_summary(&data, &sessions, None, None),
        matrix: matrix::plot(&data, &date),
        stats: stats::stats(&data, &sessions, &date, utc_offset_seconds(), data.settings.daily_focus_goal),
        timer: state.timer.snapshot(),
        data,
        date,
    })
}

#[tauri::command]
fn add_task(state: State<'_, AppState>, title: String, list_id: Option<String>) -> Result<Task, String> {
    state.store.add_task(&title, list_id.as_deref()).map_err(fail)
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

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    use tauri::menu::Submenu;

    let app_menu = Submenu::with_items(
        app,
        "Intentio Tasks",
        true,
        &[
            &MenuItem::with_id(app, "about", "About Intentio Tasks", true, None::<&str>)?,
            &MenuItem::with_id(app, "check-updates", "Check for Updates…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "new-task", "New Task", true, Some("CmdOrCtrl+N"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "new-board", "New Board…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "view-today", "Today", true, Some("CmdOrCtrl+1"))?,
            &MenuItem::with_id(app, "view-board", "Board", true, Some("CmdOrCtrl+2"))?,
            &MenuItem::with_id(app, "view-matrix", "Matrix", true, Some("CmdOrCtrl+3"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "toggle-theme", "Switch Theme", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let timer_menu = Submenu::with_items(
        app,
        "Timer",
        true,
        &[
            &MenuItem::with_id(app, "timer-start", "Start Focus Session", true, Some("CmdOrCtrl+T"))?,
            &MenuItem::with_id(app, "timer-break", "Start Break", true, None::<&str>)?,
            &MenuItem::with_id(app, "timer-stop", "Stop Timer", true, Some("CmdOrCtrl+Shift+T"))?,
        ],
    )?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &MenuItem::with_id(app, "about", "About Intentio Tasks", true, None::<&str>)?,
            &MenuItem::with_id(app, "check-updates", "Check for Updates…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "website", "Intentio Software", true, None::<&str>)?,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu, &view_menu, &timer_menu, &help_menu])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let root = Store::default_root().expect("a home directory");
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
            app.set_menu(build_menu(handle)?)?;
            handle.on_menu_event(|app, event| {
                let _ = app.emit_menu(event.id().as_ref());
            });

            // The tray is where the countdown lives, so the timer is visible
            // with the window closed.
            let tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("bundled icon").clone())
                .icon_as_template(true)
                .tooltip("Intentio Tasks")
                .on_tray_icon_event(|tray, _event| {
                    // Clicking the tray brings the window back.
                    if let Some(window) = tray.app_handle().get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                })
                .build(app)?;

            if let Some(state) = app.try_state::<AppState>() {
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
            add_board,
            add_list,
            find_tasks,
            start_timer,
            stop_timer,
            timer_state,
            sessions,
            assign_session,
            store_path,
            has_native_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Forward a native menu click to the frontend, which owns the behaviour.
trait MenuBridge {
    fn emit_menu(&self, id: &str) -> tauri::Result<()>;
}

impl MenuBridge for AppHandle {
    fn emit_menu(&self, id: &str) -> tauri::Result<()> {
        use tauri::Emitter;
        self.emit("menu-action", id.to_string())
    }
}
