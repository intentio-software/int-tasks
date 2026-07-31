//! # int-tasks-core
//!
//! The task engine behind Intentio Tasks: boards, lists, tasks and recorded
//! pomodoro sessions, kept in plain JSON on the user's own disk.
//!
//! Two ideas shape it. Capture must never ask a question — a task needs a title
//! and nothing else. And the store must stay legible: `tasks.json` and
//! `sessions.jsonl` can be read, diffed and repaired by hand, which is what lets
//! an AI agent work on the same data through the MCP server without a protocol
//! between them.
//!
//! ```no_run
//! use int_tasks_core::{Store, query};
//!
//! let store = Store::open(Store::default_root().unwrap())?;
//! store.add_task("Write the release notes", None)?;
//!
//! let data = store.read()?;
//! for entry in query::today(&data, "2026-07-31") {
//!     println!("{} — {:?}", entry.task.title, entry.reason);
//! }
//! # Ok::<(), int_tasks_core::TaskError>(())
//! ```

pub mod capture;
pub mod dates;
pub mod error;
pub mod model;
pub mod matrix;
pub mod query;
pub mod stats;
pub mod store;

pub use capture::{parse as parse_capture, Captured};
pub use error::{Result, TaskError};
pub use model::{Board, List, Session, SessionKind, Status, Task, new_id, now_millis};
pub use matrix::{Plotted, Quadrant};
pub use stats::Stats;
pub use query::{Filter, LabelUse, TaskTime, TimeSummary, TodayEntry, TodayReason};
pub use store::{Data, Settings, Store};
