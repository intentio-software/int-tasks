//! The pomodoro timer, and the menu bar countdown it drives.
//!
//! The timer lives in Rust rather than the webview for one reason: it has to
//! keep running and keep counting down when the window is closed. On macOS the
//! tray title is the only always-visible surface the app has, so that is where
//! the remaining time goes.
//!
//! A finished session is written to the store immediately. Time that was worked
//! but never recorded because the app was quit is the one failure this feature
//! cannot afford.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use int_tasks_core::{Session, SessionKind, Store, now_millis};

/// Emitted whenever the timer changes, so the UI can follow without polling.
pub const TIMER_EVENT: &str = "timer";
/// Emitted with a focus session that was recorded against no task, so the app
/// can ask what it counted towards while the answer is still fresh.
pub const UNASSIGNED_EVENT: &str = "session-unassigned";

pub const DEFAULT_FOCUS_MINUTES: u64 = 25;
pub const DEFAULT_BREAK_MINUTES: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub running: bool,
    /// Running but not counting down. Kept distinct from stopped: a paused
    /// session keeps its task and the time already worked.
    pub paused: bool,
    /// Task the current session counts towards, if one was chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Shown in the tray and the UI so the user can see what they are on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    pub kind: SessionKind,
    pub started_at: u64,
    /// Length of the session in seconds.
    pub planned_seconds: u64,
    pub remaining_seconds: u64,
}

impl Default for TimerState {
    fn default() -> Self {
        TimerState {
            running: false,
            paused: false,
            task_id: None,
            task_title: None,
            kind: SessionKind::Focus,
            started_at: 0,
            planned_seconds: DEFAULT_FOCUS_MINUTES * 60,
            remaining_seconds: DEFAULT_FOCUS_MINUTES * 60,
        }
    }
}

/// Shared timer, safe to touch from commands and from the ticking thread.
pub struct Timer {
    state: Mutex<TimerState>,
    /// Set while a ticking thread is alive, so a restart cannot leave two running.
    ticking: AtomicBool,
    /// The shape of the tray menu currently installed, so it is only rebuilt
    /// when it would actually differ — not once a second on every tick.
    menu_mode: AtomicU8,
}

impl Default for Timer {
    fn default() -> Self {
        Timer {
            state: Mutex::new(TimerState::default()),
            ticking: AtomicBool::new(false),
            menu_mode: AtomicU8::new(MODE_UNSET),
        }
    }
}

impl Timer {
    pub fn snapshot(&self) -> TimerState {
        self.state.lock().map(|state| state.clone()).unwrap_or_default()
    }
}

const MODE_UNSET: u8 = 0;
const MODE_IDLE: u8 = 1;
const MODE_RUNNING: u8 = 2;
const MODE_PAUSED: u8 = 3;

fn mode_of(state: &TimerState) -> u8 {
    match (state.running, state.paused) {
        (false, _) => MODE_IDLE,
        (true, true) => MODE_PAUSED,
        (true, false) => MODE_RUNNING,
    }
}

/// Format remaining time for the menu bar: `24:31`.
fn clock(seconds: u64) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// What the tray shows. Idle shows nothing, so the menu bar stays quiet when
/// the timer is not in use.
fn tray_title(state: &TimerState) -> String {
    if !state.running {
        return String::new();
    }
    let time = match state.kind {
        SessionKind::Focus => clock(state.remaining_seconds),
        SessionKind::Break => format!("☕ {}", clock(state.remaining_seconds)),
    };
    if state.paused { format!("⏸ {time}") } else { time }
}

/// The menu behind the tray icon.
///
/// Only what applies right now is offered: there is no Resume while running and
/// no Pause while idle, which is cheaper to read than a list of greyed-out
/// items.
fn tray_menu<R: Runtime>(app: &AppHandle<R>, state: &TimerState) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "tray-show", "Open Intentio Tasks", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    if !state.running {
        let focus = MenuItem::with_id(app, "tray-focus", "Start Focus Session", true, None::<&str>)?;
        let brk = MenuItem::with_id(app, "tray-break", "Start Break", true, None::<&str>)?;
        return Menu::with_items(app, &[&focus, &brk, &separator, &show]);
    }

    let label = state
        .task_title
        .as_deref()
        .map(|title| format!("Working on {title}"))
        .unwrap_or_else(|| match state.kind {
            SessionKind::Focus => "Focus session".to_string(),
            SessionKind::Break => "Break".to_string(),
        });
    let current = MenuItem::with_id(app, "tray-current", label, false, None::<&str>)?;
    let toggle = if state.paused {
        MenuItem::with_id(app, "tray-resume", "Resume", true, None::<&str>)?
    } else {
        MenuItem::with_id(app, "tray-pause", "Pause", true, None::<&str>)?
    };
    let stop = MenuItem::with_id(app, "tray-stop", "Stop and Record", true, None::<&str>)?;
    Menu::with_items(app, &[&current, &separator, &toggle, &stop, &PredefinedMenuItem::separator(app)?, &show])
}

fn push_tray<R: Runtime>(
    app: &AppHandle<R>,
    timer: &Timer,
    tray: &Option<TrayIcon<R>>,
    state: &TimerState,
) {
    let Some(tray) = tray else { return };
    let _ = tray.set_title(Some(tray_title(state)));

    let mode = mode_of(state);
    if timer.menu_mode.swap(mode, Ordering::SeqCst) != mode {
        if let Ok(menu) = tray_menu(app, state) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Install the idle title and menu, so the tray is useful before the timer has
/// ever been started.
pub fn init_tray<R: Runtime>(app: &AppHandle<R>, timer: &Timer, tray: &Option<TrayIcon<R>>) {
    push_tray(app, timer, tray, &timer.snapshot());
}

/// Bring the window to the front — used when the app needs an answer.
pub fn focus_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Ask the app what a session counted towards, if it counted towards nothing.
///
/// Breaks are never asked about, and neither is a session already tied to a
/// task; the question is only worth interrupting for when the answer is
/// genuinely missing.
fn ask_to_assign<R: Runtime>(app: &AppHandle<R>, session: Option<Session>) {
    let Some(session) = session else { return };
    if session.kind != SessionKind::Focus || session.task_id.is_some() {
        return;
    }
    focus_window(app);
    let _ = app.emit(UNASSIGNED_EVENT, &session);
}

/// Start a session, replacing any that is already running.
///
/// Returns the state so the caller can render immediately rather than waiting
/// for the first tick.
pub fn start<R: Runtime>(
    app: &AppHandle<R>,
    timer: Arc<Timer>,
    tray: Option<TrayIcon<R>>,
    store: Store,
    task_id: Option<String>,
    task_title: Option<String>,
    minutes: Option<u64>,
    kind: SessionKind,
) -> TimerState {
    // Stopping first records whatever was already in progress.
    if timer.snapshot().running {
        stop(app, timer.clone(), tray.clone(), store.clone(), true);
    }

    let planned = minutes.unwrap_or(match kind {
        SessionKind::Focus => DEFAULT_FOCUS_MINUTES,
        SessionKind::Break => DEFAULT_BREAK_MINUTES,
    }) * 60;

    let started = {
        let mut state = timer.state.lock().expect("timer lock");
        *state = TimerState {
            running: true,
            paused: false,
            task_id,
            task_title,
            kind,
            started_at: now_millis(),
            planned_seconds: planned,
            remaining_seconds: planned,
        };
        state.clone()
    };

    push_tray(app, &timer, &tray, &started);
    let _ = app.emit(TIMER_EVENT, &started);

    // One ticking thread at a time.
    if !timer.ticking.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        let timer_thread = timer.clone();
        let tray_thread = tray.clone();
        let store_thread = store.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));

                let (state, finished) = {
                    let mut state = match timer_thread.state.lock() {
                        Ok(state) => state,
                        Err(_) => break,
                    };
                    if !state.running {
                        break;
                    }
                    if state.paused {
                        // Still the current session, just not counting down.
                        (state.clone(), false)
                    } else {
                        state.remaining_seconds = state.remaining_seconds.saturating_sub(1);
                        (state.clone(), state.remaining_seconds == 0)
                    }
                };

                push_tray(&app, &timer_thread, &tray_thread, &state);
                let _ = app.emit(TIMER_EVENT, &state);

                if finished {
                    // Ran its full length: record it and fall back to idle.
                    complete(&app, timer_thread.clone(), tray_thread.clone(), store_thread.clone());
                    break;
                }
            }
            timer_thread.ticking.store(false, Ordering::SeqCst);
        });
    }

    started
}

/// Stop the current session, recording the time actually worked.
///
/// `record` is false only when discarding, which the UI does not currently
/// offer — stopping early still counts as work done.
pub fn stop<R: Runtime>(
    app: &AppHandle<R>,
    timer: Arc<Timer>,
    tray: Option<TrayIcon<R>>,
    store: Store,
    record: bool,
) -> TimerState {
    let previous = {
        let mut state = timer.state.lock().expect("timer lock");
        let previous = state.clone();
        *state = TimerState { planned_seconds: previous.planned_seconds, ..TimerState::default() };
        previous
    };

    let mut recorded = None;
    if previous.running && record {
        let worked = previous.planned_seconds.saturating_sub(previous.remaining_seconds);
        // Sub-minute stretches are almost always a mis-click, and logging them
        // would clutter the time report without telling anyone anything.
        if worked >= 60 {
            recorded = store
                .finish_session(
                    previous.task_id.as_deref(),
                    previous.started_at,
                    worked,
                    previous.kind,
                    false,
                )
                .ok();
        }
    }

    let idle = timer.snapshot();
    push_tray(app, &timer, &tray, &idle);
    let _ = app.emit(TIMER_EVENT, &idle);
    ask_to_assign(app, recorded);
    idle
}

/// Hold the countdown where it is, keeping the session and its task.
pub fn pause<R: Runtime>(app: &AppHandle<R>, timer: Arc<Timer>, tray: Option<TrayIcon<R>>) -> TimerState {
    set_paused(app, timer, tray, true)
}

/// Carry on from where a pause left off.
pub fn resume<R: Runtime>(app: &AppHandle<R>, timer: Arc<Timer>, tray: Option<TrayIcon<R>>) -> TimerState {
    set_paused(app, timer, tray, false)
}

fn set_paused<R: Runtime>(
    app: &AppHandle<R>,
    timer: Arc<Timer>,
    tray: Option<TrayIcon<R>>,
    paused: bool,
) -> TimerState {
    let state = {
        let mut state = timer.state.lock().expect("timer lock");
        if state.running {
            state.paused = paused;
        }
        state.clone()
    };
    push_tray(app, &timer, &tray, &state);
    let _ = app.emit(TIMER_EVENT, &state);
    state
}

/// A session that ran to its planned length.
fn complete<R: Runtime>(app: &AppHandle<R>, timer: Arc<Timer>, tray: Option<TrayIcon<R>>, store: Store) {
    let finished = {
        let mut state = timer.state.lock().expect("timer lock");
        let finished = state.clone();
        *state = TimerState { planned_seconds: finished.planned_seconds, ..TimerState::default() };
        finished
    };

    let recorded = store
        .finish_session(
            finished.task_id.as_deref(),
            finished.started_at,
            finished.planned_seconds,
            finished.kind,
            true,
        )
        .ok();

    let idle = timer.snapshot();
    push_tray(app, &timer, &tray, &idle);
    // A distinct event so the UI can celebrate rather than just re-render.
    let _ = app.emit("timer-finished", &finished);
    let _ = app.emit(TIMER_EVENT, &idle);
    ask_to_assign(app, recorded);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_is_always_two_digits() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(61), "01:01");
        assert_eq!(clock(25 * 60), "25:00");
        assert_eq!(clock(59), "00:59");
    }

    #[test]
    fn an_idle_timer_shows_nothing_in_the_menu_bar() {
        let state = TimerState::default();
        assert_eq!(tray_title(&state), "", "the menu bar stays quiet when not in use");
    }

    #[test]
    fn a_running_focus_session_shows_the_countdown() {
        let state = TimerState { running: true, remaining_seconds: 1471, ..Default::default() };
        assert_eq!(tray_title(&state), "24:31");
    }

    #[test]
    fn a_paused_session_says_so_in_the_menu_bar() {
        let state = TimerState {
            running: true,
            paused: true,
            remaining_seconds: 754,
            ..Default::default()
        };
        assert_eq!(tray_title(&state), "⏸ 12:34");
    }

    #[test]
    fn pausing_does_not_look_like_stopping() {
        // Stopped shows nothing at all; paused still holds its place.
        let paused = TimerState { running: true, paused: true, ..Default::default() };
        let stopped = TimerState::default();
        assert_ne!(tray_title(&paused), tray_title(&stopped));
        assert_eq!(tray_title(&stopped), "");
    }

    #[test]
    fn the_menu_is_rebuilt_only_when_its_shape_changes() {
        let running = TimerState { running: true, remaining_seconds: 900, ..Default::default() };
        let ticked = TimerState { remaining_seconds: 899, ..running.clone() };
        assert_eq!(mode_of(&running), mode_of(&ticked), "a tick must not rebuild the menu");

        let paused = TimerState { paused: true, ..running.clone() };
        assert_ne!(mode_of(&running), mode_of(&paused));
        assert_ne!(mode_of(&running), mode_of(&TimerState::default()));
    }

    #[test]
    fn a_break_is_visibly_different_from_focus() {
        let state = TimerState {
            running: true,
            kind: SessionKind::Break,
            remaining_seconds: 300,
            ..Default::default()
        };
        assert_eq!(tray_title(&state), "☕ 05:00");
    }
}
