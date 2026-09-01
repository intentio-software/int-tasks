//! The tool surface an AI agent sees for a task store.
//!
//! Written around the same principle as the app: capture takes a title and
//! nothing else. Everything an agent might want to set is optional, so a model
//! that only knows "remind me to call the bank" can still record it correctly.

use serde_json::{json, Value};

use int_tasks_core::{Filter, SessionKind, Status, Store, matrix, model, query, stats};

use crate::mcp::{opt_bool, opt_str, opt_str_list, opt_usize, require_str, ServerInfo, Tool, ToolOutput, ToolProvider};

pub struct TaskTools {
    store: Store,
}

impl TaskTools {
    pub fn new(store: Store) -> Self {
        TaskTools { store }
    }

    /// Today's date in the machine's own timezone.
    ///
    /// The core deliberately takes this as a parameter rather than reading a
    /// clock, so the timezone decision is made once, here.
    fn today(&self) -> String {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    fn object(properties: Value, required: &[&str]) -> Value {
        json!({ "type": "object", "properties": properties, "required": required })
    }
}

/// Parse a status name, rejecting anything else rather than guessing.
fn parse_status(value: &str) -> Result<Status, String> {
    match value.trim().to_lowercase().as_str() {
        "todo" | "to do" | "open" => Ok(Status::Todo),
        "doing" | "in progress" | "in-progress" => Ok(Status::Doing),
        "done" | "complete" | "completed" => Ok(Status::Done),
        other => Err(format!("unknown status `{other}`; use todo, doing or done")),
    }
}

/// A date the store will accept: `YYYY-MM-DD`, or the words today/tomorrow.
fn parse_due(value: &str) -> Result<String, String> {
    let text = value.trim().to_lowercase();
    let today = chrono::Local::now().date_naive();
    let date = match text.as_str() {
        "today" => today,
        "tomorrow" => today.succ_opt().unwrap_or(today),
        "yesterday" => today.pred_opt().unwrap_or(today),
        other => {
            return chrono::NaiveDate::parse_from_str(other, "%Y-%m-%d")
                .map(|date| date.format("%Y-%m-%d").to_string())
                .map_err(|_| format!("`{value}` is not a date; use YYYY-MM-DD, today or tomorrow"))
        }
    };
    Ok(date.format("%Y-%m-%d").to_string())
}

impl ToolProvider for TaskTools {
    fn server_info(&self) -> ServerInfo {
        ServerInfo {
            name: "intentio-tasks".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            instructions: concat!(
                "Read and write the user's Intentio Tasks store — boards, lists, tasks and recorded ",
                "pomodoro sessions, kept as plain JSON on their own machine.\n\n",
                "Guidance:\n",
                "- `add_task` needs only a title. Do not invent a due date, list or priority the user ",
                "did not give; an unscheduled task is normal and correct.\n",
                "- `today` is the list the user actually works from: overdue, due today, flagged, or ",
                "in progress. Prefer it over `list_tasks` when asked what to do now.\n",
                "- Dates are `YYYY-MM-DD`; `today` and `tomorrow` are also accepted.\n",
                "- `complete_task` moves a task into its board's Done list as well as marking it, so ",
                "there is no need to move it separately.\n",
                "- Recorded sessions are the user's time log. Add to it with `log_session` only for ",
                "work that actually happened.",
            )
            .into(),
        }
    }

    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::new(
                "add_task",
                "Capture a task. Only a title is required — omit anything the user did not specify rather than guessing. Lands in the first list of the first board unless a list is given.",
                Self::object(
                    json!({
                        "title": {"type": "string", "description": "What needs doing. A leading `(project)` and `[tag]` are read out of it — `(stm) [chore] clean up the login page` files the task under project `stm`, tagged `chore`. Explicit project/tags arguments win over the title."},
                        "notes": {"type": "string", "description": "Longer detail, if the user gave any."},
                        "list_id": {"type": "string", "description": "List to file it under. Omit for the default inbox list."},
                        "due": {"type": "string", "description": "`YYYY-MM-DD`, or `today` / `tomorrow`."},
                        "today": {"type": "boolean", "description": "Pull onto the Today list regardless of due date."},
                        "project": {"type": "string", "description": "The project this belongs to. One per task."},
                        "tags": {"type": "array", "items": {"type": "string"}, "description": "Type labels: bug, admin, deep-work."},
                        "impact": {"type": "integer", "description": "How much finishing it is worth, 1-10. Omit unless the user said."},
                        "effort": {"type": "integer", "description": "How much it will cost, 1-10. Omit unless the user said."},
                        "priority": {"type": "integer", "description": "1 is highest. Omit if unstated."},
                        "estimate_minutes": {"type": "integer", "description": "Rough size, for planning against pomodoro sessions."},
                        "origin": {"type": "string", "description": "Where the task came from, as `scheme:reference` — `mindmap:<map>#<node>`, `knowledge:<note path>`. Set this when creating a task on behalf of another app so it can be traced back."}
                    }),
                    &["title"],
                ),
            ),
            Tool::new(
                "today",
                "The user's Today list: overdue, due today, flagged for today, or already in progress — ordered so the most pressing is first. Each entry says why it is there. This is the right tool for \"what should I work on\".",
                Self::object(json!({}), &[]),
            ),
            Tool::new(
                "list_tasks",
                "List tasks with optional filters. Completed tasks are excluded unless include_done is set.",
                Self::object(
                    json!({
                        "board_id": {"type": "string"},
                        "list_id": {"type": "string"},
                        "status": {"type": "string", "description": "todo, doing or done."},
                        "tag": {"type": "string"},
                        "project": {"type": "string"},
                        "query": {"type": "string", "description": "Text matched against title and notes."},
                        "include_done": {"type": "boolean", "description": "Default false."},
                        "limit": {"type": "integer", "description": "Default 100."}
                    }),
                    &[],
                ),
            ),
            Tool::new(
                "get_task",
                "Read one task in full, including its recorded time.",
                Self::object(json!({"task_id": {"type": "string"}}), &["task_id"]),
            ),
            Tool::new(
                "update_task",
                "Change fields on a task. Only the fields given are touched; pass null to clear an optional one.",
                Self::object(
                    json!({
                        "task_id": {"type": "string"},
                        "title": {"type": "string"},
                        "notes": {"type": "string"},
                        "due": {"type": "string", "description": "`YYYY-MM-DD`, `today`, `tomorrow`, or null to clear."},
                        "today": {"type": "boolean"},
                        "project": {"type": "string", "description": "The project this belongs to. One per task."},
                        "tags": {"type": "array", "items": {"type": "string"}, "description": "Type labels: bug, admin, deep-work."},
                        "impact": {"type": "integer", "description": "How much finishing it is worth, 1-10. Omit unless the user said."},
                        "effort": {"type": "integer", "description": "How much it will cost, 1-10. Omit unless the user said."},
                        "priority": {"type": "integer"},
                        "estimate_minutes": {"type": "integer"}
                    }),
                    &["task_id"],
                ),
            ),
            Tool::new(
                "complete_task",
                "Mark a task done and move it into its board's Done list.",
                Self::object(json!({"task_id": {"type": "string"}}), &["task_id"]),
            ),
            Tool::new(
                "reopen_task",
                "Undo completion, returning the task to its board's To Do list.",
                Self::object(json!({"task_id": {"type": "string"}}), &["task_id"]),
            ),
            Tool::new(
                "move_task",
                "Move a task to a different list, and optionally to a position within it.",
                Self::object(
                    json!({
                        "task_id": {"type": "string"},
                        "list_id": {"type": "string"},
                        "position": {"type": "integer", "description": "0-based. Omit for the end of the list."}
                    }),
                    &["task_id", "list_id"],
                ),
            ),
            Tool::new(
                "delete_task",
                "Delete a task permanently. Confirm with the user first — completing a task is usually what is wanted instead.",
                Self::object(json!({"task_id": {"type": "string"}}), &["task_id"]),
            ),
            Tool::new(
                "list_boards",
                "List boards with their lists and ids, so tasks can be filed and moved.",
                Self::object(json!({}), &[]),
            ),
            Tool::new(
                "add_board",
                "Create a board, pre-populated with To Do, Doing and Done lists.",
                Self::object(json!({"name": {"type": "string"}}), &["name"]),
            ),
            Tool::new(
                "add_list",
                "Add a list to a board.",
                Self::object(
                    json!({
                        "board_id": {"type": "string"},
                        "name": {"type": "string"},
                        "status": {"type": "string", "description": "todo, doing or done — applied to tasks moved into this list."}
                    }),
                    &["board_id", "name"],
                ),
            ),
            Tool::new(
                "log_session",
                "Record focused time against a task, for work that has already happened.",
                Self::object(
                    json!({
                        "task_id": {"type": "string", "description": "Omit to record unattributed time."},
                        "minutes": {"type": "integer", "description": "Length of the session."},
                        "kind": {"type": "string", "description": "focus (default) or break."}
                    }),
                    &["minutes"],
                ),
            ),
            Tool::new(
                "team_activity",
                "What the whole team finished or is working on for one project, newest first. This is the material for a client update: report what it returns rather than guessing, and say only what you can point at.",
                Self::object(
                    json!({
                        "project": {"type": "string", "description": "Project code as used on tasks, e.g. stm."},
                        "since_days": {"type": "integer", "description": "How far back to count completed work. Default 14."}
                    }),
                    &["project"],
                ),
            ),
            Tool::new(
                "sessions",
                "Recorded focus and break sessions, newest first. Use this to review or correct what time was logged against.",
                Self::object(
                    json!({"limit": {"type": "integer", "description": "Default 50."}}),
                    &[],
                ),
            ),
            Tool::new(
                "assign_session",
                "Attribute a recorded session to a task, or pass no task_id to leave it unattributed. This is the correction for time logged against the wrong thing.",
                Self::object(
                    json!({
                        "session_id": {"type": "string"},
                        "task_id": {"type": "string", "description": "Omit to clear the attribution."}
                    }),
                    &["session_id"],
                ),
            ),
            Tool::new(
                "delete_session",
                "Remove a recorded session — a timer left running, or time logged twice. Confirm with the user first: reassigning is usually the right correction, and deleting changes every report the session fed.",
                Self::object(json!({"session_id": {"type": "string"}}), &["session_id"]),
            ),
            Tool::new(
                "time_summary",
                "Total focus time and a per-task breakdown. Breaks are excluded.",
                Self::object(
                    json!({"since_days": {"type": "integer", "description": "Only sessions from the last N days. Omit for all time."}}),
                    &[],
                ),
            ),
            Tool::new(
                "matrix",
                "Open tasks placed on the impact/effort matrix, most worth doing first. Each carries its quadrant (quick-win, big-bet, fill-in, thankless) and an urgency derived from its due date and priority. Only tasks with both scores appear.",
                Self::object(json!({}), &[]),
            ),
            Tool::new(
                "suggest_task",
                "Pick something worth doing now. Use low_energy when the user says they are tired, stuck or sluggish: it returns the cheapest task that still pays rather than the most valuable one.",
                Self::object(
                    json!({"low_energy": {"type": "boolean", "description": "Default false."}}),
                    &[],
                ),
            ),
            Tool::new(
                "stats",
                "The day's standing: focus streak, sessions against the daily goal, and impact points from work finished. All derived from what actually happened.",
                Self::object(json!({}), &[]),
            ),
            Tool::new(
                "labels",
                "Projects and type tags in use, with how many open and completed tasks carry each. Read this before setting a project or tag so an existing one is reused rather than a near-duplicate created.",
                Self::object(json!({}), &[]),
            ),
            Tool::new(
                "rename_label",
                "Rename a project or tag on every task that carries it. Renaming onto a name already in use merges the two, which is how near-duplicates get tidied up.",
                Self::object(
                    json!({
                        "kind": {"type": "string", "description": "project or tag."},
                        "from": {"type": "string"},
                        "to": {"type": "string"}
                    }),
                    &["kind", "from", "to"],
                ),
            ),
            Tool::new(
                "delete_label",
                "Clear a project or tag from every task that carries it. The tasks themselves are untouched.",
                Self::object(
                    json!({
                        "kind": {"type": "string", "description": "project or tag."},
                        "name": {"type": "string"}
                    }),
                    &["kind", "name"],
                ),
            ),
            Tool::new(
                "store_info",
                "Where the store lives and what is in it.",
                Self::object(json!({}), &[]),
            ),
        ]
    }

    fn call(&mut self, name: &str, args: &Value) -> Result<ToolOutput, String> {
        let store = &self.store;
        let err = |e: int_tasks_core::TaskError| e.to_string();

        match name {
            "add_task" => {
                let title = require_str(args, "title")?;

                // Everything that can be rejected is validated before anything is
                // written. Creating the task first and failing afterwards would
                // leave it behind while reporting an error, and an agent that
                // retries would then duplicate it.
                let due = match opt_str(args, "due") {
                    Some(value) => Some(parse_due(&value)?),
                    None => None,
                };

                let task = store.capture(&title, opt_str(args, "list_id").as_deref(), &self.today()).map_err(err)?;
                let tags = opt_str_list(args, "tags");
                let today = opt_bool(args, "today", false);
                let priority = args.get("priority").and_then(Value::as_u64).map(|p| p as u8);
                let estimate = args.get("estimate_minutes").and_then(Value::as_u64).map(|m| m as u32);
                let notes = opt_str(args, "notes");
                let project = opt_str(args, "project");
                let origin = opt_str(args, "origin");
                let impact = args.get("impact").and_then(Value::as_u64).map(|v| (v as u8).clamp(1, 10));
                let effort = args.get("effort").and_then(Value::as_u64).map(|v| (v as u8).clamp(1, 10));

                if due.is_some() || !tags.is_empty() || today || priority.is_some() || estimate.is_some()
                    || notes.is_some() || project.is_some() || impact.is_some() || effort.is_some()
                    || origin.is_some() {
                    store
                        .update(|data| {
                            let stored = data.task_mut(&task.id).expect("just added");
                            if let Some(due) = due {
                                stored.due = Some(due);
                            }
                            if !tags.is_empty() {
                                stored.tags = tags;
                            }
                            if today {
                                stored.today = true;
                            }
                            if priority.is_some() {
                                stored.priority = priority;
                            }
                            if estimate.is_some() {
                                stored.estimate_minutes = estimate;
                            }
                            if notes.is_some() {
                                stored.notes = notes;
                            }
                            if project.is_some() {
                                stored.project = project;
                            }
                            if impact.is_some() {
                                stored.impact = impact;
                            }
                            if effort.is_some() {
                                stored.effort = effort;
                            }
                            if origin.is_some() {
                                stored.origin = origin;
                            }
                            stored.touch();
                            Ok(())
                        })
                        .map_err(err)?;
                }

                let data = store.read().map_err(err)?;
                Ok(ToolOutput::json(&json!({ "added": data.task(&task.id) })))
            }

            "today" => {
                let data = store.read().map_err(err)?;
                let entries = query::today(&data, &self.today());
                Ok(ToolOutput::json(&json!({
                    "date": self.today(),
                    "count": entries.len(),
                    "tasks": entries,
                })))
            }

            "list_tasks" => {
                let data = store.read().map_err(err)?;
                let status = match opt_str(args, "status") {
                    Some(value) => Some(parse_status(&value)?),
                    None => None,
                };
                let filter = Filter {
                    board_id: opt_str(args, "board_id"),
                    list_id: opt_str(args, "list_id"),
                    status,
                    tag: opt_str(args, "tag"),
                    project: opt_str(args, "project"),
                    query: opt_str(args, "query"),
                    include_done: opt_bool(args, "include_done", false),
                    limit: Some(opt_usize(args, "limit", 100)),
                };
                let tasks = query::find(&data, &filter);
                Ok(ToolOutput::json(&json!({ "count": tasks.len(), "tasks": tasks })))
            }

            "get_task" => {
                let id = require_str(args, "task_id")?;
                let data = store.read().map_err(err)?;
                let task = data.task(&id).ok_or_else(|| format!("no task with id {id}"))?;
                let sessions = store.sessions().map_err(err)?;
                let summary = query::time_summary(&data, &sessions, None, None);
                let recorded = summary.by_task.iter().find(|entry| entry.task_id == id);
                Ok(ToolOutput::json(&json!({
                    "task": task,
                    "list": data.list(&task.list_id),
                    "board": data.board_of_list(&task.list_id).map(|board| json!({"id": board.id, "name": board.name})),
                    "recorded_seconds": recorded.map(|entry| entry.seconds).unwrap_or(0),
                    "sessions": recorded.map(|entry| entry.sessions).unwrap_or(0),
                })))
            }

            "update_task" => {
                let id = require_str(args, "task_id")?;
                // Parsed before the write so a bad date cannot half-apply an edit.
                let due = match args.get("due") {
                    Some(Value::Null) => Some(None),
                    Some(Value::String(text)) => Some(Some(parse_due(text)?)),
                    _ => None,
                };
                let title = opt_str(args, "title");
                let notes = args.get("notes").map(|value| match value {
                    Value::Null => None,
                    other => other.as_str().map(str::to_string),
                });
                let today = args.get("today").and_then(Value::as_bool);
                let tags = args.get("tags").map(|_| opt_str_list(args, "tags"));
                let project = args.get("project").map(|value| match value {
                    Value::Null => None,
                    other => other.as_str().map(str::to_string),
                });
                let impact = args.get("impact").map(|v| v.as_u64().map(|n| (n as u8).clamp(1, 10)));
                let effort = args.get("effort").map(|v| v.as_u64().map(|n| (n as u8).clamp(1, 10)));
                let priority = args.get("priority").map(|value| value.as_u64().map(|p| p as u8));
                let estimate = args.get("estimate_minutes").map(|value| value.as_u64().map(|m| m as u32));

                let updated = store
                    .update(|data| {
                        let task = data
                            .task_mut(&id)
                            .ok_or_else(|| int_tasks_core::TaskError::TaskNotFound(id.clone()))?;
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
                            task.project = project;
                        }
                        if let Some(impact) = impact {
                            task.impact = impact;
                        }
                        if let Some(effort) = effort {
                            task.effort = effort;
                        }
                        if let Some(priority) = priority {
                            task.priority = priority;
                        }
                        if let Some(estimate) = estimate {
                            task.estimate_minutes = estimate;
                        }
                        task.touch();
                        Ok(task.clone())
                    })
                    .map_err(err)?;
                Ok(ToolOutput::json(&json!({ "updated": updated })))
            }

            "complete_task" => {
                let id = require_str(args, "task_id")?;
                let task = store.set_done(&id, true).map_err(err)?;
                Ok(ToolOutput::json(&json!({ "completed": task })))
            }

            "reopen_task" => {
                let id = require_str(args, "task_id")?;
                let task = store.set_done(&id, false).map_err(err)?;
                Ok(ToolOutput::json(&json!({ "reopened": task })))
            }

            "move_task" => {
                let id = require_str(args, "task_id")?;
                let list_id = require_str(args, "list_id")?;
                let position = args.get("position").and_then(Value::as_u64).map(|p| p as usize);
                let task = store.move_task(&id, &list_id, position).map_err(err)?;
                Ok(ToolOutput::json(&json!({ "moved": task })))
            }

            "delete_task" => {
                let id = require_str(args, "task_id")?;
                let task = store.delete_task(&id).map_err(err)?;
                Ok(ToolOutput::json(&json!({ "deleted": { "id": task.id, "title": task.title }, "recoverable": false })))
            }

            "list_boards" => {
                let data = store.read().map_err(err)?;
                Ok(ToolOutput::json(&json!({ "boards": data.boards })))
            }

            "add_board" => {
                let board = store.add_board(&require_str(args, "name")?).map_err(err)?;
                Ok(ToolOutput::json(&json!({ "added": board })))
            }

            "add_list" => {
                let board_id = require_str(args, "board_id")?;
                let name = require_str(args, "name")?;
                let status = match opt_str(args, "status") {
                    Some(value) => Some(parse_status(&value)?),
                    None => None,
                };
                let list = store.add_list(&board_id, &name, status).map_err(err)?;
                Ok(ToolOutput::json(&json!({ "added": list })))
            }

            "log_session" => {
                let minutes = args
                    .get("minutes")
                    .and_then(Value::as_u64)
                    .ok_or("`minutes` is required and must be a number")?;
                if minutes == 0 {
                    return Err("`minutes` must be greater than zero".into());
                }
                let kind = match opt_str(args, "kind").as_deref() {
                    Some("break") => SessionKind::Break,
                    Some("focus") | None => SessionKind::Focus,
                    Some(other) => return Err(format!("unknown kind `{other}`; use focus or break")),
                };
                let seconds = minutes * 60;
                let started_at = model::now_millis().saturating_sub(seconds * 1000);
                let session = store
                    .finish_session(opt_str(args, "task_id").as_deref(), started_at, seconds, kind, true)
                    .map_err(err)?;
                Ok(ToolOutput::json(&json!({ "logged": session })))
            }

            "time_summary" => {
                let data = store.read().map_err(err)?;
                let sessions = store.sessions().map_err(err)?;
                let from = args.get("since_days").and_then(Value::as_u64).map(|days| {
                    model::now_millis().saturating_sub(days * 24 * 60 * 60 * 1000)
                });
                let summary = query::time_summary(&data, &sessions, from, None);
                Ok(ToolOutput::json(&json!({
                    "total_minutes": summary.total_seconds / 60,
                    "focus_sessions": summary.focus_sessions,
                    "unattributed_minutes": summary.unattributed_seconds / 60,
                    "by_task": summary.by_task,
                })))
            }

            "matrix" => {
                let data = store.read().map_err(err)?;
                let plotted = matrix::plot(&data, &self.today());
                Ok(ToolOutput::json(&json!({ "count": plotted.len(), "tasks": plotted })))
            }

            "suggest_task" => {
                let data = store.read().map_err(err)?;
                let low_energy = opt_bool(args, "low_energy", false);
                match matrix::suggest(&data, &self.today(), low_energy) {
                    Some(picked) => Ok(ToolOutput::json(&json!({ "suggestion": picked, "lowEnergy": low_energy }))),
                    None => Ok(ToolOutput::json(&json!({
                        "suggestion": Value::Null,
                        "note": "Nothing is scored yet. Set impact and effort on a few tasks to get a suggestion."
                    }))),
                }
            }

            "stats" => {
                let data = store.read().map_err(err)?;
                let sessions = store.sessions().map_err(err)?;
                let offset = chrono::Local::now().offset().local_minus_utc();
                let stats = stats::stats(&data, &sessions, &self.today(), offset, data.settings.daily_focus_goal);
                let value = serde_json::to_value(&stats).map_err(|err| err.to_string())?;
                Ok(ToolOutput::json(&value))
            }

            "labels" => {
                let data = store.read().map_err(err)?;
                Ok(ToolOutput::json(&json!({
                    "projects": serde_json::to_value(query::projects(&data)).map_err(|e| e.to_string())?,
                    "tags": serde_json::to_value(query::tags(&data)).map_err(|e| e.to_string())?,
                })))
            }

            "rename_label" => {
                let kind = require_str(args, "kind")?;
                let from = require_str(args, "from")?;
                let to = require_str(args, "to")?;
                let touched = match label_kind(&kind)? {
                    LabelKind::Project => store.rename_project(&from, &to).map_err(err)?,
                    LabelKind::Tag => store.rename_tag(&from, &to).map_err(err)?,
                };
                Ok(ToolOutput::json(&json!({
                    "renamed": to.trim(),
                    "tasks_updated": touched,
                })))
            }

            "delete_label" => {
                let kind = require_str(args, "kind")?;
                let name = require_str(args, "name")?;
                let touched = match label_kind(&kind)? {
                    LabelKind::Project => store.delete_project(&name).map_err(err)?,
                    LabelKind::Tag => store.delete_tag(&name).map_err(err)?,
                };
                Ok(ToolOutput::json(&json!({
                    "cleared": name.trim(),
                    "tasks_updated": touched,
                })))
            }

            "team_activity" => {
                let project = require_str(args, "project")?;
                let days = opt_usize(args, "since_days", 14) as i64;
                let since = int_tasks_core::model::now_millis().saturating_sub((days * 86_400_000) as u64);
                let found = int_tasks_core::team::activity(store.root(), &project, since);
                let completed: Vec<&int_tasks_core::team::Activity> =
                    found.iter().filter(|a| a.state == "completed").collect();
                let in_progress: Vec<&int_tasks_core::team::Activity> =
                    found.iter().filter(|a| a.state == "in_progress").collect();
                Ok(ToolOutput::json(&json!({
                    "project": project,
                    "since_days": days,
                    "completed": completed.iter().map(|a| json!({"who": a.member, "title": a.title})).collect::<Vec<_>>(),
                    "in_progress": in_progress.iter().map(|a| json!({"who": a.member, "title": a.title})).collect::<Vec<_>>(),
                })))
            }

            "sessions" => {
                let limit = opt_usize(args, "limit", 50);
                let mut sessions = store.sessions().map_err(err)?;
                sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
                sessions.truncate(limit);
                let data = store.read().map_err(err)?;
                let listed: Vec<Value> = sessions
                    .iter()
                    .map(|session| {
                        json!({
                            "id": session.id,
                            "task_id": session.task_id,
                            "task": session.task_id.as_deref().and_then(|id| data.task(id)).map(|task| task.title.clone()),
                            "minutes": session.seconds / 60,
                            "kind": session.kind,
                            "completed": session.completed,
                            "started_at": session.started_at,
                        })
                    })
                    .collect();
                Ok(ToolOutput::json(&json!({ "sessions": listed })))
            }

            "assign_session" => {
                let session_id = require_str(args, "session_id")?;
                let task_id = opt_str(args, "task_id");
                let updated = store.assign_session(&session_id, task_id.as_deref()).map_err(err)?;
                Ok(ToolOutput::json(&json!({
                    "session": updated.id,
                    "task_id": updated.task_id,
                })))
            }

            "delete_session" => {
                let session_id = require_str(args, "session_id")?;
                let removed = store.delete_session(&session_id).map_err(err)?;
                Ok(ToolOutput::json(&json!({
                    "deleted": removed.id,
                    "minutes": removed.seconds / 60,
                    "recoverable": false,
                })))
            }

            "store_info" => {
                let data = store.read().map_err(err)?;
                let sessions = store.sessions().map_err(err)?;
                let open = data.tasks.iter().filter(|task| !task.status.is_done()).count();
                Ok(ToolOutput::json(&json!({
                    "path": store.root().to_string_lossy(),
                    "boards": data.boards.len(),
                    "tasks": data.tasks.len(),
                    "open_tasks": open,
                    "sessions": sessions.len(),
                    "today": self.today(),
                })))
            }

            other => Err(format!("unknown tool: {other}")),
        }
    }
}

/// Which label an agent means. Spelled out rather than taken as a bare string
/// so a typo fails loudly instead of silently doing nothing.
enum LabelKind {
    Project,
    Tag,
}

fn label_kind(raw: &str) -> Result<LabelKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "project" | "projects" => Ok(LabelKind::Project),
        "tag" | "tags" => Ok(LabelKind::Tag),
        other => Err(format!("kind must be `project` or `tag`, got `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tools(name: &str) -> TaskTools {
        let dir = std::env::temp_dir().join(format!("int-tasks-mcp-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        TaskTools::new(Store::open(&dir).unwrap())
    }

    fn call(tools: &mut TaskTools, name: &str, args: Value) -> Value {
        let out = tools.call(name, &args).expect("tool call succeeded");
        serde_json::from_str(&out.text).expect("json")
    }

    #[test]
    fn every_tool_has_a_description_and_object_schema() {
        let t = tools("schemas");
        for tool in t.tools() {
            assert!(!tool.description.is_empty(), "{} has no description", tool.name);
            assert_eq!(tool.input_schema["type"], "object", "{} is not an object schema", tool.name);
        }
    }

    #[test]
    fn labels_report_what_is_actually_in_use() {
        let mut t = tools("labels");
        call(&mut t, "add_task", json!({"title": "A", "project": "Apollo", "tags": ["bug", "admin"]}));
        call(&mut t, "add_task", json!({"title": "B", "project": "Apollo", "tags": ["bug"]}));
        let labels = call(&mut t, "labels", json!({}));
        assert_eq!(labels["projects"][0]["name"], "Apollo");
        assert_eq!(labels["projects"][0]["open"], 2);
        assert_eq!(labels["tags"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn renaming_a_label_onto_an_existing_one_merges_them() {
        let mut t = tools("merge");
        call(&mut t, "add_task", json!({"title": "A", "project": "apollo"}));
        call(&mut t, "add_task", json!({"title": "B", "project": "Apollo"}));
        let renamed = call(&mut t, "rename_label", json!({"kind": "project", "from": "apollo", "to": "Apollo"}));
        assert_eq!(renamed["tasks_updated"], 1);
        let labels = call(&mut t, "labels", json!({}));
        assert_eq!(labels["projects"].as_array().unwrap().len(), 1, "the two should have merged");
        assert_eq!(labels["projects"][0]["open"], 2);
    }

    #[test]
    fn deleting_a_label_leaves_the_tasks_alone() {
        let mut t = tools("dellabel");
        call(&mut t, "add_task", json!({"title": "A", "tags": ["bug", "admin"]}));
        let cleared = call(&mut t, "delete_label", json!({"kind": "tag", "name": "bug"}));
        assert_eq!(cleared["tasks_updated"], 1);
        let listed = call(&mut t, "list_tasks", json!({}));
        assert_eq!(listed["tasks"].as_array().unwrap().len(), 1, "the task survives");
        assert_eq!(listed["tasks"][0]["tags"], json!(["admin"]));
    }

    #[test]
    fn a_misspelled_kind_is_rejected_rather_than_ignored() {
        let mut t = tools("badkind");
        let err = t.call("delete_label", &json!({"kind": "porject", "name": "x"})).unwrap_err();
        assert!(err.contains("project"), "the error should say what was expected: {err}");
    }

    #[test]
    fn a_title_alone_is_enough_to_capture() {
        let mut t = tools("capture");
        let added = call(&mut t, "add_task", json!({"title": "Call the bank"}));
        assert_eq!(added["added"]["title"], "Call the bank");
        assert!(added["added"]["due"].is_null(), "no due date should be invented");
    }

    #[test]
    fn an_agent_gets_the_same_reading_of_a_typed_line() {
        let mut t = tools("capture-markers");
        let added = call(&mut t, "add_task", json!({"title": "(stm) [chore] clean up the login page"}));
        assert_eq!(added["added"]["title"], "clean up the login page");
        assert_eq!(added["added"]["project"], "stm");
        assert_eq!(added["added"]["tags"], json!(["chore"]));
    }

    #[test]
    fn an_explicit_project_beats_the_one_in_the_title() {
        // An agent told which project to use was told for a reason.
        let mut t = tools("capture-override");
        let added = call(&mut t, "add_task", json!({
            "title": "(stm) ship the thing", "project": "acme"
        }));
        assert_eq!(added["added"]["title"], "ship the thing");
        assert_eq!(added["added"]["project"], "acme");
    }

    #[test]
    fn a_task_created_for_another_app_remembers_where_it_came_from() {
        let mut t = tools("origin");
        let added = call(&mut t, "add_task", json!({
            "title": "Wire up billing", "origin": "mindmap:map_7#node_3"
        }));
        assert_eq!(added["added"]["origin"], "mindmap:map_7#node_3");
    }

    #[test]
    fn optional_detail_is_applied_when_given() {
        let mut t = tools("detail");
        let added = call(&mut t, "add_task", json!({
            "title": "Write spec", "due": "2026-08-01", "tags": ["work"], "priority": 1, "today": true
        }));
        assert_eq!(added["added"]["due"], "2026-08-01");
        assert_eq!(added["added"]["tags"], json!(["work"]));
        assert_eq!(added["added"]["priority"], 1);
        assert_eq!(added["added"]["today"], true);
    }

    #[test]
    fn relative_dates_are_understood() {
        let mut t = tools("dates");
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let added = call(&mut t, "add_task", json!({"title": "Due now", "due": "today"}));
        assert_eq!(added["added"]["due"], today);
    }

    #[test]
    fn a_bad_date_is_refused_rather_than_guessed() {
        let mut t = tools("bad-date");
        let err = t.call("add_task", &json!({"title": "x", "due": "next Tuesday"})).unwrap_err();
        assert!(err.contains("is not a date"), "{err}");
    }

    #[test]
    fn a_rejected_capture_leaves_nothing_behind() {
        let mut t = tools("atomic-capture");
        assert!(t.call("add_task", &json!({"title": "x", "due": "next Tuesday"})).is_err());
        // Reporting an error while having created the task would make a retrying
        // agent produce duplicates.
        assert_eq!(call(&mut t, "list_tasks", json!({}))["count"], 0);
    }

    #[test]
    fn today_lists_what_matters_and_says_why() {
        let mut t = tools("today");
        call(&mut t, "add_task", json!({"title": "Overdue thing", "due": "2020-01-01"}));
        call(&mut t, "add_task", json!({"title": "Flagged thing", "today": true}));
        call(&mut t, "add_task", json!({"title": "Someday thing"}));

        let today = call(&mut t, "today", json!({}));
        assert_eq!(today["count"], 2, "unscheduled work must not appear on Today");
        assert_eq!(today["tasks"][0]["title"], "Overdue thing");
        assert_eq!(today["tasks"][0]["reason"], "overdue");
    }

    #[test]
    fn completing_moves_the_task_into_done() {
        let mut t = tools("complete");
        let added = call(&mut t, "add_task", json!({"title": "Finish it"}));
        let id = added["added"]["id"].as_str().unwrap().to_string();

        let done = call(&mut t, "complete_task", json!({"task_id": id}));
        assert_eq!(done["completed"]["status"], "done");

        let boards = call(&mut t, "list_boards", json!({}));
        let done_list = boards["boards"][0]["lists"].as_array().unwrap().iter()
            .find(|l| l["status"] == "done").unwrap()["id"].as_str().unwrap().to_string();
        assert_eq!(done["completed"]["list_id"], done_list);

        // And it drops off the default listing.
        assert_eq!(call(&mut t, "list_tasks", json!({}))["count"], 0);
    }

    #[test]
    fn update_clears_a_field_when_passed_null() {
        let mut t = tools("clear");
        let added = call(&mut t, "add_task", json!({"title": "Dated", "due": "2026-08-01"}));
        let id = added["added"]["id"].as_str().unwrap().to_string();

        let updated = call(&mut t, "update_task", json!({"task_id": id, "due": null}));
        assert!(updated["updated"]["due"].is_null());
    }

    #[test]
    fn update_leaves_unmentioned_fields_alone() {
        let mut t = tools("partial");
        let added = call(&mut t, "add_task", json!({"title": "Keep me", "due": "2026-08-01", "priority": 2}));
        let id = added["added"]["id"].as_str().unwrap().to_string();

        let updated = call(&mut t, "update_task", json!({"task_id": id, "title": "Renamed"}));
        assert_eq!(updated["updated"]["title"], "Renamed");
        assert_eq!(updated["updated"]["due"], "2026-08-01");
        assert_eq!(updated["updated"]["priority"], 2);
    }

    #[test]
    fn time_is_logged_and_summarised_per_task() {
        let mut t = tools("time");
        let added = call(&mut t, "add_task", json!({"title": "Deep work"}));
        let id = added["added"]["id"].as_str().unwrap().to_string();

        call(&mut t, "log_session", json!({"task_id": id, "minutes": 25}));
        call(&mut t, "log_session", json!({"task_id": id, "minutes": 25}));
        call(&mut t, "log_session", json!({"minutes": 5, "kind": "break"}));

        let summary = call(&mut t, "time_summary", json!({}));
        assert_eq!(summary["total_minutes"], 50, "breaks must not count as focus time");
        assert_eq!(summary["focus_sessions"], 2);
        assert_eq!(summary["by_task"][0]["seconds"], 3000);

        let task = call(&mut t, "get_task", json!({"task_id": id}));
        assert_eq!(task["recorded_seconds"], 3000);
    }

    #[test]
    fn search_matches_title_and_notes() {
        let mut t = tools("search");
        call(&mut t, "add_task", json!({"title": "Unrelated", "notes": "mentions invoices"}));
        call(&mut t, "add_task", json!({"title": "Send invoices"}));
        assert_eq!(call(&mut t, "list_tasks", json!({"query": "INVOICES"}))["count"], 2);
    }

    #[test]
    fn scoring_places_a_task_on_the_matrix() {
        let mut t = tools("matrix");
        call(&mut t, "add_task", json!({"title": "Quick win", "impact": 9, "effort": 2}));
        call(&mut t, "add_task", json!({"title": "Unscored"}));

        let matrix = call(&mut t, "matrix", json!({}));
        assert_eq!(matrix["count"], 1, "only scored tasks are placed");
        assert_eq!(matrix["tasks"][0]["quadrant"], "quick-win");
    }

    #[test]
    fn a_low_energy_suggestion_is_the_cheap_one() {
        let mut t = tools("suggest");
        call(&mut t, "add_task", json!({"title": "Big and brilliant", "impact": 10, "effort": 9}));
        call(&mut t, "add_task", json!({"title": "Small and dull", "impact": 3, "effort": 1}));

        let tired = call(&mut t, "suggest_task", json!({"low_energy": true}));
        assert_eq!(tired["suggestion"]["title"], "Small and dull");
        let fresh = call(&mut t, "suggest_task", json!({}));
        assert_eq!(fresh["suggestion"]["title"], "Small and dull", "best ratio still wins");
    }

    #[test]
    fn suggesting_with_nothing_scored_explains_itself() {
        let mut t = tools("no-scores");
        call(&mut t, "add_task", json!({"title": "Unscored"}));
        let result = call(&mut t, "suggest_task", json!({}));
        assert!(result["suggestion"].is_null());
        assert!(result["note"].as_str().unwrap().contains("Set impact and effort"));
    }

    #[test]
    fn tasks_filter_by_project() {
        let mut t = tools("project");
        call(&mut t, "add_task", json!({"title": "Mine", "project": "Intentio"}));
        call(&mut t, "add_task", json!({"title": "Other", "project": "Elsewhere"}));
        assert_eq!(call(&mut t, "list_tasks", json!({"project": "intentio"}))["count"], 1);
    }

    #[test]
    fn stats_report_the_days_standing() {
        let mut t = tools("stats");
        let added = call(&mut t, "add_task", json!({"title": "Worth doing", "impact": 7}));
        let id = added["added"]["id"].as_str().unwrap().to_string();
        call(&mut t, "complete_task", json!({"task_id": id}));

        let stats = call(&mut t, "stats", json!({}));
        assert_eq!(stats["pointsToday"], 7);
        assert_eq!(stats["completedToday"], 1);
        assert_eq!(stats["dailyGoal"], 4);
    }

    #[test]
    fn unknown_ids_and_tools_are_reported() {
        let mut t = tools("errors");
        assert!(t.call("complete_task", &json!({"task_id": "nope"})).is_err());
        assert!(t.call("nonsense", &json!({})).is_err());
    }
}
