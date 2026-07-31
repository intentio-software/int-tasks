//! The impact/effort matrix, and picking something to do when you are stuck.
//!
//! Impact and effort are optional. A task without them is not plotted, because
//! guessing a position would put work in a quadrant it was never assessed for —
//! and the whole value of the matrix is that its positions mean something.

use serde::{Deserialize, Serialize};

use crate::dates::days_between;
use crate::model::Task;
use crate::store::Data;

/// Scores above this are "high". Ten-point scales are read as two halves, so
/// the split belongs in the middle rather than somewhere cleverer.
pub const HIGH: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quadrant {
    /// High impact, low effort. Do these first.
    QuickWin,
    /// High impact, high effort. Worth planning properly.
    BigBet,
    /// Low impact, low effort. Good when energy is low.
    FillIn,
    /// Low impact, high effort. Usually the answer is not to.
    Thankless,
}

impl Quadrant {
    pub fn of(impact: u8, effort: u8) -> Self {
        match (impact > HIGH, effort > HIGH) {
            (true, false) => Quadrant::QuickWin,
            (true, true) => Quadrant::BigBet,
            (false, false) => Quadrant::FillIn,
            (false, true) => Quadrant::Thankless,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quadrant::QuickWin => "Quick wins",
            Quadrant::BigBet => "Big bets",
            Quadrant::FillIn => "Fill-ins",
            Quadrant::Thankless => "Thankless",
        }
    }
}

/// How pressing a task is, 0–10.
///
/// Derived from the due date rather than stored, because a stored urgency is
/// wrong the moment time passes: something marked 8 for next Friday is still 8
/// the following Monday. A date knows what day it is; a number does not.
///
/// An explicit `priority` raises the floor, for work that matters regardless of
/// any deadline.
pub fn urgency(task: &Task, today: &str) -> u8 {
    let from_due = match task.due.as_deref() {
        None => 0,
        Some(due) if due < today => 10,
        Some(due) if due == today => 9,
        Some(due) => match days_between(today, due) {
            Some(1) => 8,
            Some(2..=3) => 7,
            Some(4..=7) => 5,
            Some(8..=14) => 3,
            Some(15..=30) => 2,
            _ => 1,
        },
    };

    // Priority 1 is highest; treat it as a floor so a flagged task cannot sink.
    let from_priority = match task.priority {
        Some(1) => 9,
        Some(2) => 7,
        Some(3) => 5,
        Some(_) => 3,
        None => 0,
    };

    from_due.max(from_priority)
}

/// A task placed on the matrix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plotted {
    #[serde(flatten)]
    pub task: Task,
    pub impact: u8,
    pub effort: u8,
    /// The third dimension, derived from the due date and priority.
    pub urgency: u8,
    pub quadrant: Quadrant,
    /// Impact per unit of effort. Higher is better value.
    pub ratio: f32,
    /// Value adjusted for how pressing it is. What "do this next" sorts by.
    pub score: f32,
}

/// Every open, scored task, most worth doing first.
pub fn plot(data: &Data, today: &str) -> Vec<Plotted> {
    let mut plotted: Vec<Plotted> = data
        .tasks
        .iter()
        .filter(|task| !task.status.is_done())
        .filter_map(|task| {
            let impact = task.impact?;
            let effort = task.effort?.max(1);
            let urgency = urgency(task, today);
            let ratio = impact as f32 / effort as f32;
            Some(Plotted {
                quadrant: Quadrant::of(impact, effort),
                ratio,
                // Urgency scales value rather than replacing it, so a looming
                // deadline promotes work without burying something valuable.
                score: ratio * (1.0 + urgency as f32 / 10.0),
                impact,
                effort,
                urgency,
                task: task.clone(),
            })
        })
        .collect();

    plotted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.effort.cmp(&b.effort))
            .then_with(|| a.task.created_at.cmp(&b.task.created_at))
    });
    plotted
}

/// Something worth doing right now, given how much energy there is for it.
///
/// `low_energy` looks for the cheapest thing that still pays — the point of the
/// feature is that when you are sluggish, being handed a big bet is worse than
/// being handed nothing.
pub fn suggest(data: &Data, today: &str, low_energy: bool) -> Option<Plotted> {
    let plotted = plot(data, today);
    if low_energy {
        // Cheapest first, and among equally cheap ones the most valuable.
        return plotted
            .iter()
            .filter(|entry| entry.effort <= HIGH)
            .min_by(|a, b| {
                a.effort.cmp(&b.effort).then_with(|| {
                    b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                })
            })
            .cloned()
            // Nothing cheap is available: fall back rather than refusing to help.
            .or_else(|| plotted.first().cloned());
    }
    plotted.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Board, Status, Task};

    const TODAY: &str = "2026-07-31";

    fn scored(title: &str, impact: Option<u8>, effort: Option<u8>, status: Status) -> Task {
        let mut task = Task::new(title, "list_1");
        task.impact = impact;
        task.effort = effort;
        task.status = status;
        task
    }

    fn data_with(tasks: Vec<Task>) -> Data {
        Data { boards: vec![Board::with_default_lists("Tasks", 0)], tasks, revision: 1, settings: Default::default() }
    }

    #[test]
    fn quadrants_split_at_the_middle_of_the_scale() {
        assert_eq!(Quadrant::of(10, 1), Quadrant::QuickWin);
        assert_eq!(Quadrant::of(10, 10), Quadrant::BigBet);
        assert_eq!(Quadrant::of(1, 1), Quadrant::FillIn);
        assert_eq!(Quadrant::of(1, 10), Quadrant::Thankless);
        // Exactly 5 is the low half of a ten-point scale.
        assert_eq!(Quadrant::of(5, 5), Quadrant::FillIn);
        assert_eq!(Quadrant::of(6, 5), Quadrant::QuickWin);
    }

    #[test]
    fn unscored_tasks_are_not_plotted() {
        let data = data_with(vec![
            scored("Scored", Some(8), Some(2), Status::Todo),
            scored("No effort", Some(8), None, Status::Todo),
            scored("Nothing", None, None, Status::Todo),
        ]);
        let plotted = plot(&data, TODAY);
        assert_eq!(plotted.len(), 1, "a task must not be placed where it was never assessed");
        assert_eq!(plotted[0].task.title, "Scored");
    }

    #[test]
    fn completed_work_leaves_the_matrix() {
        let data = data_with(vec![scored("Done", Some(9), Some(1), Status::Done)]);
        assert!(plot(&data, TODAY).is_empty());
    }

    #[test]
    fn the_best_value_sorts_first() {
        let data = data_with(vec![
            scored("Expensive", Some(10), Some(10), Status::Todo),
            scored("Bargain", Some(8), Some(2), Status::Todo),
            scored("Middling", Some(6), Some(3), Status::Todo),
        ]);
        let plotted = plot(&data, TODAY);
        let order: Vec<&str> = plotted.iter().map(|p| p.task.title.as_str()).collect();
        assert_eq!(order, vec!["Bargain", "Middling", "Expensive"]);
    }

    #[test]
    fn a_low_energy_suggestion_is_cheap_not_merely_valuable() {
        let data = data_with(vec![
            scored("Huge but brilliant", Some(10), Some(9), Status::Todo),
            scored("Small and useful", Some(6), Some(2), Status::Todo),
            scored("Tiny and dull", Some(2), Some(1), Status::Todo),
        ]);
        // Cheapest wins when energy is low, even though its ratio is not top.
        assert_eq!(suggest(&data, TODAY, true).unwrap().task.title, "Tiny and dull");
        // With energy, best value wins.
        assert_eq!(suggest(&data, TODAY, false).unwrap().task.title, "Small and useful");
    }

    #[test]
    fn a_low_energy_suggestion_falls_back_rather_than_refusing() {
        let data = data_with(vec![scored("Only a big one", Some(9), Some(10), Status::Todo)]);
        // Handing back nothing when the user asked for help is the worst answer.
        assert_eq!(suggest(&data, TODAY, true).unwrap().task.title, "Only a big one");
    }

    #[test]
    fn nothing_to_suggest_when_nothing_is_scored() {
        assert!(suggest(&data_with(vec![]), TODAY, true).is_none());
    }
}
