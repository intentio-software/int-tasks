//! A quiet reminder that an hour of a working day has gone unrecorded.
//!
//! The point is not to police anybody. It is that focus time you did not log
//! is focus time that never happened as far as the app is concerned, and the
//! moment to catch that is while you can still remember what you were doing.
//!
//! Three rules keep it a reminder rather than a nag:
//!
//! * **The clock starts at the first thing you record that day**, not at
//!   midnight. Before you have begun, there is nothing to have stopped doing,
//!   and a notification at half past seven is just noise.
//! * **Only on working days.** Saturday is not a lapse.
//! * **It gives up.** Someone who has ignored three reminders is in a workshop
//!   or a hospital, not waiting for a fourth.
//!
//! Activity is read from the store rather than reported by the parts of the
//! app that cause it. Anything that touches the store counts — including work
//! logged by the MCP server, or by Tasks on another machine and synced in —
//! and no new code path can forget to check in.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use int_tasks_core::{Store, now_millis};

/// How often to look. A minute is far finer than the hour being measured, and
/// costs one read of a file that is already in the page cache.
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// Reminders in one day before it stops asking.
const MAX_PER_DAY: u8 = 3;

#[derive(Default)]
pub struct Nudger {
    /// Millisecond timestamp of the last reminder shown.
    last_nudge: AtomicU64,
    /// Reminders shown on `day`.
    shown_today: AtomicU8,
    /// The day `shown_today` counts, as days since the epoch.
    day: AtomicU64,
}

impl Nudger {
    /// Reset the daily count when the date rolls over.
    fn roll(&self, today: u64) {
        if self.day.swap(today, Ordering::Relaxed) != today {
            self.shown_today.store(0, Ordering::Relaxed);
            self.last_nudge.store(0, Ordering::Relaxed);
        }
    }
}

/// The most recent thing the user did, in milliseconds.
///
/// Both halves matter: finishing a task is activity even if no session was
/// timed, and a recorded session is activity even if no task moved.
pub fn last_activity(store: &Store) -> Option<u64> {
    let newest_task = store
        .read()
        .ok()?
        .tasks
        .iter()
        .map(|task| task.updated_at)
        .max();
    let newest_session = store
        .sessions()
        .ok()
        .and_then(|sessions| {
            sessions
                .iter()
                .map(|session| session.started_at.saturating_add(session.seconds * 1000))
                .max()
        });
    newest_task.into_iter().chain(newest_session).max()
}

/// Whether a reminder is due, and why not when it is not.
///
/// Split out from the thread so the rules can be tested without waiting an
/// hour for a timer.
pub fn due(
    now: u64,
    last_activity: Option<u64>,
    idle_minutes: u32,
    is_working_day: bool,
    timer_running: bool,
    shown_today: u8,
    last_nudge: u64,
    start_of_day: u64,
) -> bool {
    if idle_minutes == 0 || !is_working_day || timer_running || shown_today >= MAX_PER_DAY {
        return false;
    }
    let Some(last) = last_activity else { return false };
    // Nothing recorded yet today: the day has not started, so it cannot have
    // stalled. This is what stops it greeting people at breakfast.
    if last < start_of_day {
        return false;
    }
    let idle_ms = (idle_minutes as u64) * 60_000;
    if now.saturating_sub(last) < idle_ms {
        return false;
    }
    // Having already asked, wait the same span again rather than every minute.
    now.saturating_sub(last_nudge) >= idle_ms
}

/// Watch for quiet spells and say something once each.
pub fn spawn<R: Runtime>(app: AppHandle<R>, store: Store, nudger: Arc<Nudger>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(CHECK_EVERY);

        let settings = match store.read() {
            Ok(data) => data.settings,
            Err(_) => continue,
        };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        nudger.roll(days_since_epoch(now_millis()));

        let timer_running = app
            .try_state::<crate::AppState>()
            .map(|state| state.timer.snapshot().running)
            .unwrap_or(false);

        let now = now_millis();
        let quiet = due(
            now,
            last_activity(&store),
            settings.idle_nudge_minutes,
            settings.is_working_day(&today),
            timer_running,
            nudger.shown_today.load(Ordering::Relaxed),
            nudger.last_nudge.load(Ordering::Relaxed),
            start_of_day_millis(now),
        );
        if !quiet {
            continue;
        }

        let minutes = settings.idle_nudge_minutes;
        let body = if minutes % 60 == 0 && minutes >= 60 {
            let hours = minutes / 60;
            format!(
                "Nothing recorded for {hours} hour{}. If you have been working, log it while you \
                 still remember what it was.",
                if hours == 1 { "" } else { "s" }
            )
        } else {
            format!(
                "Nothing recorded for {minutes} minutes. If you have been working, log it while \
                 you still remember what it was."
            )
        };

        let _ = app
            .notification()
            .builder()
            .title("Still going?")
            .body(body)
            .show();

        nudger.last_nudge.store(now, Ordering::Relaxed);
        nudger.shown_today.fetch_add(1, Ordering::Relaxed);
    });
}

fn days_since_epoch(millis: u64) -> u64 {
    millis / 86_400_000
}

/// Local midnight before `now`.
///
/// Local rather than UTC: for somebody in Johannesburg a UTC day rolls over at
/// 2am, which would make "nothing recorded yet today" wrong for two hours every
/// morning.
fn start_of_day_millis(now: u64) -> u64 {
    let offset = (crate::utc_offset_seconds() as i64) * 1000;
    let local = now as i64 + offset;
    let midnight_local = local - local.rem_euclid(86_400_000);
    (midnight_local - offset).max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 3_600_000;
    const MIDNIGHT: u64 = 1_000 * HOUR;

    fn base(last: Option<u64>) -> bool {
        due(MIDNIGHT + 10 * HOUR, last, 60, true, false, 0, 0, MIDNIGHT)
    }

    #[test]
    fn an_hour_of_silence_after_working_earns_a_reminder() {
        assert!(base(Some(MIDNIGHT + 8 * HOUR)));
    }

    #[test]
    fn recent_work_does_not() {
        assert!(!base(Some(MIDNIGHT + 10 * HOUR - 60_000)));
    }

    #[test]
    fn a_day_that_has_not_started_is_left_alone() {
        // Yesterday's work is not this morning's stalled session.
        assert!(!base(Some(MIDNIGHT - 2 * HOUR)));
    }

    #[test]
    fn an_empty_store_is_left_alone() {
        assert!(!base(None));
    }

    #[test]
    fn a_running_timer_needs_no_reminder() {
        let last = Some(MIDNIGHT + 8 * HOUR);
        assert!(!due(MIDNIGHT + 10 * HOUR, last, 60, true, true, 0, 0, MIDNIGHT));
    }

    #[test]
    fn a_day_off_is_not_a_lapse() {
        let last = Some(MIDNIGHT + 8 * HOUR);
        assert!(!due(MIDNIGHT + 10 * HOUR, last, 60, false, false, 0, 0, MIDNIGHT));
    }

    #[test]
    fn zero_turns_it_off() {
        let last = Some(MIDNIGHT + 8 * HOUR);
        assert!(!due(MIDNIGHT + 10 * HOUR, last, 0, true, false, 0, 0, MIDNIGHT));
    }

    #[test]
    fn it_gives_up_after_three() {
        let last = Some(MIDNIGHT + 8 * HOUR);
        assert!(!due(MIDNIGHT + 10 * HOUR, last, 60, true, false, MAX_PER_DAY, 0, MIDNIGHT));
    }

    #[test]
    fn it_does_not_repeat_every_minute() {
        let last = Some(MIDNIGHT + 8 * HOUR);
        let just_asked = MIDNIGHT + 10 * HOUR - 60_000;
        assert!(!due(MIDNIGHT + 10 * HOUR, last, 60, true, false, 1, just_asked, MIDNIGHT));
        // …but asks again once the same span has passed.
        let asked_an_hour_ago = MIDNIGHT + 9 * HOUR;
        assert!(due(MIDNIGHT + 10 * HOUR, last, 60, true, false, 1, asked_an_hour_ago, MIDNIGHT));
    }
}
