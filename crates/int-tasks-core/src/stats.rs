//! Streaks, the daily goal, and points.
//!
//! All of it is derived from work that already happened — completed tasks and
//! recorded sessions — so there is no separate score to keep in step, and
//! nothing to inflate except by doing the work.
//!
//! The numbers are deliberately modest. A tool you use every day should not
//! shout at you, and a streak that punishes a day off stops being motivating
//! very quickly.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::dates::{days_before, local_date};
use crate::model::{Session, SessionKind};
use crate::store::{Data, Settings};

/// Impact assumed for a task nobody scored, so completing unscored work still
/// counts for something without pretending it was a triumph.
const UNSCORED_IMPACT: u32 = 3;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Consecutive days, ending today or yesterday, with at least one session.
    pub streak_days: u32,
    /// Focus sessions completed today.
    pub sessions_today: u32,
    /// How many the user is aiming for.
    pub daily_goal: u32,
    pub focus_minutes_today: u64,
    /// Impact of everything finished today.
    pub points_today: u32,
    /// Impact of everything ever finished.
    pub points_total: u32,
    pub completed_today: u32,
    /// True once today's goal is met, so the UI can mark it without recomputing.
    pub goal_met: bool,
}

/// One working day's worth of progress, for the trend on Flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayProgress {
    pub date: String,
    pub focus_minutes: u64,
    /// Impact of everything finished that day.
    pub points: u32,
}

/// The last `days` working days, oldest first.
///
/// Working days only, so the bars are evenly spaced and comparable — a trend
/// with a two-day gap in the middle of it invites the wrong reading. Work done
/// on a weekend still counts towards the streak and appears in the session log;
/// it just does not get a column here.
pub fn recent_progress(
    data: &Data,
    sessions: &[Session],
    today: &str,
    utc_offset_seconds: i32,
    days: usize,
) -> Vec<DayProgress> {
    let mut focus_by_day: HashMap<String, u64> = HashMap::new();
    for session in sessions.iter().filter(|s| s.kind == SessionKind::Focus) {
        *focus_by_day.entry(local_date(session.started_at, utc_offset_seconds)).or_insert(0) +=
            session.seconds;
    }

    let mut points_by_day: HashMap<String, u32> = HashMap::new();
    for task in data.tasks.iter().filter(|task| task.status.is_done()) {
        if let Some(at) = task.completed_at {
            *points_by_day.entry(local_date(at, utc_offset_seconds)).or_insert(0) +=
                task.impact.map(u32::from).unwrap_or(UNSCORED_IMPACT);
        }
    }

    let mut out = Vec::with_capacity(days);
    let mut cursor = today.to_string();
    // Bounded: ten working days can never be more than a few weeks back, and a
    // malformed date must not spin.
    for _ in 0..400 {
        if out.len() >= days {
            break;
        }
        if data.settings.is_working_day(&cursor) {
            out.push(DayProgress {
                focus_minutes: focus_by_day.get(&cursor).copied().unwrap_or(0) / 60,
                points: points_by_day.get(&cursor).copied().unwrap_or(0),
                date: cursor.clone(),
            });
        }
        match days_before(&cursor, 1) {
            Some(previous) => cursor = previous,
            None => break,
        }
    }
    out.reverse();
    out
}

/// Work out the day's standing.
pub fn stats(
    data: &Data,
    sessions: &[Session],
    today: &str,
    utc_offset_seconds: i32,
    daily_goal: u32,
) -> Stats {
    // Days on which at least one focus session happened.
    let active_days: HashSet<String> = sessions
        .iter()
        .filter(|session| session.kind == SessionKind::Focus)
        .map(|session| local_date(session.started_at, utc_offset_seconds))
        .collect();

    let today_sessions: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.kind == SessionKind::Focus)
        .filter(|session| local_date(session.started_at, utc_offset_seconds) == today)
        .collect();

    let completed: Vec<&crate::model::Task> = data
        .tasks
        .iter()
        .filter(|task| task.status.is_done())
        .collect();

    let points_of = |task: &crate::model::Task| task.impact.map(u32::from).unwrap_or(UNSCORED_IMPACT);

    let completed_today: Vec<&&crate::model::Task> = completed
        .iter()
        .filter(|task| {
            task.completed_at
                .map(|at| local_date(at, utc_offset_seconds) == today)
                .unwrap_or(false)
        })
        .collect();

    let sessions_today = today_sessions.len() as u32;

    Stats {
        streak_days: streak(&active_days, today, &data.settings),
        sessions_today,
        daily_goal,
        focus_minutes_today: today_sessions.iter().map(|session| session.seconds).sum::<u64>() / 60,
        points_today: completed_today.iter().map(|task| points_of(task)).sum(),
        points_total: completed.iter().map(|task| points_of(task)).sum(),
        completed_today: completed_today.len() as u32,
        goal_met: daily_goal > 0 && sessions_today >= daily_goal,
    }
}

/// Length of the run of working days worked, ending at or before today.
///
/// Three rules, in order. Today never breaks a streak, because it is reported
/// at breakfast before there has been any chance to work. A day that is not a
/// working day never breaks one either — a weekend off is rest, not a lapse —
/// though working through one still counts. Anything else missed ends the run.
fn streak(active_days: &HashSet<String>, today: &str, settings: &Settings) -> u32 {
    let mut count = 0u32;
    let mut cursor = today.to_string();
    // Bounded so a malformed date cannot spin: far longer than any real streak.
    for _ in 0..3_650 {
        if active_days.contains(&cursor) {
            count += 1;
        } else if cursor != today && settings.is_working_day(&cursor) {
            break;
        }
        match days_before(&cursor, 1) {
            Some(previous) => cursor = previous,
            None => break,
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Board, Status, Task};

    const TODAY: &str = "2026-07-31";
    /// Midday UTC on a given date, so offsets in tests do not shift the day.
    fn at(date: &str) -> u64 {
        (crate::dates::civil_days(date).unwrap() as u64) * 86_400_000 + 12 * 3_600_000
    }

    fn focus_on(date: &str) -> Session {
        Session {
            id: crate::model::new_id("session"),
            task_id: None,
            started_at: at(date),
            ended_at: at(date) + 1_500_000,
            seconds: 1500,
            kind: SessionKind::Focus,
            completed: true,
        }
    }

    fn data_with(tasks: Vec<Task>) -> Data {
        Data { boards: vec![Board::with_default_lists("Tasks", 0)], tasks, revision: 1, settings: Default::default() }
    }

    fn done(title: &str, impact: Option<u8>, completed_on: &str) -> Task {
        let mut task = Task::new(title, "list_1");
        task.status = Status::Done;
        task.impact = impact;
        task.completed_at = Some(at(completed_on));
        task
    }

    #[test]
    fn consecutive_days_build_a_streak() {
        let sessions = vec![focus_on("2026-07-31"), focus_on("2026-07-30"), focus_on("2026-07-29")];
        let stats = stats(&data_with(vec![]), &sessions, TODAY, 0, 4);
        assert_eq!(stats.streak_days, 3);
    }

    #[test]
    fn a_gap_ends_the_streak() {
        // Missing the 29th, so only two days count.
        let sessions = vec![focus_on("2026-07-31"), focus_on("2026-07-30"), focus_on("2026-07-28")];
        assert_eq!(stats(&data_with(vec![]), &sessions, TODAY, 0, 4).streak_days, 2);
    }

    #[test]
    fn the_streak_survives_a_morning_with_no_work_yet() {
        // Nothing today, but yesterday counts — a streak should not read as
        // broken before the day has had a chance to start.
        let sessions = vec![focus_on("2026-07-30"), focus_on("2026-07-29")];
        assert_eq!(stats(&data_with(vec![]), &sessions, TODAY, 0, 4).streak_days, 2);
    }

    #[test]
    fn two_days_off_does_end_it() {
        let sessions = vec![focus_on("2026-07-29"), focus_on("2026-07-28")];
        assert_eq!(stats(&data_with(vec![]), &sessions, TODAY, 0, 4).streak_days, 0);
    }

    /// 2026-08-03 is a Monday; 08-01 and 08-02 are the weekend before it.
    const MONDAY: &str = "2026-08-03";

    fn data_with_settings(settings: crate::store::Settings) -> Data {
        Data { boards: vec![Board::with_default_lists("Tasks", 0)], tasks: vec![], revision: 1, settings }
    }

    #[test]
    fn the_trend_covers_working_days_only() {
        let sessions = vec![focus_on(MONDAY), focus_on("2026-08-01"), focus_on("2026-07-31")];
        let progress = recent_progress(&data_with_settings(Default::default()), &sessions, MONDAY, 0, 5);

        assert_eq!(progress.len(), 5);
        let dates: Vec<&str> = progress.iter().map(|day| day.date.as_str()).collect();
        assert_eq!(dates, vec!["2026-07-28", "2026-07-29", "2026-07-30", "2026-07-31", MONDAY]);
        assert!(!dates.contains(&"2026-08-01"), "Saturday gets no column");

        assert_eq!(progress.last().unwrap().focus_minutes, 25, "Monday's session");
        assert_eq!(progress[3].focus_minutes, 25, "Friday's session");
        assert_eq!(progress[0].focus_minutes, 0, "a day with no work is still a column");
    }

    #[test]
    fn the_trend_counts_impact_on_the_day_it_was_finished() {
        let data = Data {
            boards: vec![Board::with_default_lists("Tasks", 0)],
            tasks: vec![done("Big", Some(8), MONDAY), done("Earlier", Some(5), "2026-07-31")],
            revision: 1,
            settings: Default::default(),
        };
        let progress = recent_progress(&data, &[], MONDAY, 0, 3);
        assert_eq!(progress.last().unwrap().points, 8);
        assert_eq!(progress[progress.len() - 2].points, 5);
    }

    #[test]
    fn a_weekend_off_does_not_break_the_streak() {
        // Worked Friday, rested, worked Monday. That is a two day streak, not a
        // broken one — which is the whole point of counting working days.
        let sessions = vec![focus_on(MONDAY), focus_on("2026-07-31")];
        let stats = stats(&data_with_settings(Default::default()), &sessions, MONDAY, 0, 4);
        assert_eq!(stats.streak_days, 2);
    }

    #[test]
    fn working_through_a_weekend_still_counts() {
        let sessions = vec![
            focus_on(MONDAY),
            focus_on("2026-08-02"),
            focus_on("2026-08-01"),
            focus_on("2026-07-31"),
        ];
        let stats = stats(&data_with_settings(Default::default()), &sessions, MONDAY, 0, 4);
        assert_eq!(stats.streak_days, 4, "a day off is not required, only forgiven");
    }

    #[test]
    fn a_holiday_is_treated_like_a_weekend() {
        let mut settings = crate::store::Settings::default();
        settings.holidays = vec!["2026-07-31".to_string()];
        // Nothing on the Friday, because it was a holiday.
        let sessions = vec![focus_on(MONDAY), focus_on("2026-07-30")];
        let stats = stats(&data_with_settings(settings), &sessions, MONDAY, 0, 4);
        assert_eq!(stats.streak_days, 2);
    }

    #[test]
    fn a_missed_working_day_still_ends_it() {
        // The Friday was an ordinary working day and nothing was done on it.
        let sessions = vec![focus_on(MONDAY), focus_on("2026-07-30")];
        let stats = stats(&data_with_settings(Default::default()), &sessions, MONDAY, 0, 4);
        assert_eq!(stats.streak_days, 1);
    }

    #[test]
    fn a_seven_day_working_week_forgives_nothing() {
        let mut settings = crate::store::Settings::default();
        settings.working_days = vec![0, 1, 2, 3, 4, 5, 6];
        let sessions = vec![focus_on(MONDAY), focus_on("2026-07-31")];
        let stats = stats(&data_with_settings(settings), &sessions, MONDAY, 0, 4);
        assert_eq!(stats.streak_days, 1, "the weekend now counts against it");
    }

    #[test]
    fn breaks_do_not_keep_a_streak_alive() {
        let mut rest = focus_on("2026-07-31");
        rest.kind = SessionKind::Break;
        assert_eq!(stats(&data_with(vec![]), &[rest], TODAY, 0, 4).streak_days, 0);
    }

    #[test]
    fn several_sessions_in_one_day_are_still_one_day() {
        let sessions = vec![focus_on("2026-07-31"), focus_on("2026-07-31"), focus_on("2026-07-31")];
        let stats = stats(&data_with(vec![]), &sessions, TODAY, 0, 4);
        assert_eq!(stats.streak_days, 1);
        assert_eq!(stats.sessions_today, 3);
    }

    #[test]
    fn the_goal_is_met_only_when_reached() {
        let sessions = vec![focus_on(TODAY), focus_on(TODAY)];
        assert!(!stats(&data_with(vec![]), &sessions, TODAY, 0, 4).goal_met);
        let four = vec![focus_on(TODAY), focus_on(TODAY), focus_on(TODAY), focus_on(TODAY)];
        assert!(stats(&data_with(vec![]), &four, TODAY, 0, 4).goal_met);
    }

    #[test]
    fn points_come_from_the_impact_of_finished_work() {
        let data = data_with(vec![
            done("Big", Some(9), TODAY),
            done("Small", Some(2), TODAY),
            done("Yesterday", Some(5), "2026-07-30"),
        ]);
        let stats = stats(&data, &[], TODAY, 0, 4);
        assert_eq!(stats.points_today, 11);
        assert_eq!(stats.points_total, 16);
        assert_eq!(stats.completed_today, 2);
    }

    #[test]
    fn unscored_work_still_counts_modestly() {
        let data = data_with(vec![done("Unscored", None, TODAY)]);
        assert_eq!(stats(&data, &[], TODAY, 0, 4).points_today, UNSCORED_IMPACT);
    }

    #[test]
    fn open_tasks_score_nothing() {
        let mut open = Task::new("Not finished", "list_1");
        open.impact = Some(10);
        assert_eq!(stats(&data_with(vec![open]), &[], TODAY, 0, 4).points_total, 0);
    }
}
