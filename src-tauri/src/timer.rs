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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Runtime};

use int_tasks_core::{SessionKind, Store, now_millis};

/// Emitted whenever the timer changes, so the UI can follow without polling.
pub const TIMER_EVENT: &str = "timer";

pub const DEFAULT_FOCUS_MINUTES: u64 = 25;
pub const DEFAULT_BREAK_MINUTES: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub running: bool,
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
}

impl Default for Timer {
    fn default() -> Self {
        Timer { state: Mutex::new(TimerState::default()), ticking: AtomicBool::new(false) }
    }
}

impl Timer {
    pub fn snapshot(&self) -> TimerState {
        self.state.lock().map(|state| state.clone()).unwrap_or_default()
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
    match state.kind {
        SessionKind::Focus => clock(state.remaining_seconds),
        SessionKind::Break => format!("☕ {}", clock(state.remaining_seconds)),
    }
}

fn push_tray<R: Runtime>(tray: &Option<TrayIcon<R>>, state: &TimerState) {
    if let Some(tray) = tray {
        let _ = tray.set_title(Some(tray_title(state)));
    }
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
            task_id,
            task_title,
            kind,
            started_at: now_millis(),
            planned_seconds: planned,
            remaining_seconds: planned,
        };
        state.clone()
    };

    push_tray(&tray, &started);
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
                    state.remaining_seconds = state.remaining_seconds.saturating_sub(1);
                    (state.clone(), state.remaining_seconds == 0)
                };

                push_tray(&tray_thread, &state);
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

    if previous.running && record {
        let worked = previous.planned_seconds.saturating_sub(previous.remaining_seconds);
        // Sub-minute stretches are almost always a mis-click, and logging them
        // would clutter the time report without telling anyone anything.
        if worked >= 60 {
            let _ = store.finish_session(
                previous.task_id.as_deref(),
                previous.started_at,
                worked,
                previous.kind,
                false,
            );
        }
    }

    let idle = timer.snapshot();
    push_tray(&tray, &idle);
    let _ = app.emit(TIMER_EVENT, &idle);
    idle
}

/// A session that ran to its planned length.
fn complete<R: Runtime>(app: &AppHandle<R>, timer: Arc<Timer>, tray: Option<TrayIcon<R>>, store: Store) {
    let finished = {
        let mut state = timer.state.lock().expect("timer lock");
        let finished = state.clone();
        *state = TimerState { planned_seconds: finished.planned_seconds, ..TimerState::default() };
        finished
    };

    let _ = store.finish_session(
        finished.task_id.as_deref(),
        finished.started_at,
        finished.planned_seconds,
        finished.kind,
        true,
    );

    let idle = timer.snapshot();
    push_tray(&tray, &idle);
    // A distinct event so the UI can celebrate rather than just re-render.
    let _ = app.emit("timer-finished", &finished);
    let _ = app.emit(TIMER_EVENT, &idle);
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
