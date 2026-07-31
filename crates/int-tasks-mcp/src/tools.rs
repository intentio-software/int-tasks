//! The tool surface an AI agent sees for a task store.
//!
//! Written around the same principle as the app: capture takes a title and
//! nothing else. Everything an agent might want to set is optional, so a model
//! that only knows "remind me to call the bank" can still record it correctly.

use serde_json::{json, Value};

use int_tasks_core::{Filter, SessionKind, Status, Store, model, query};

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
                        "title": {"type": "string", "description": "What needs doing."},
                        "notes": {"type": "string", "description": "Longer detail, if the user gave any."},
                        "list_id": {"type": "string", "description": "List to file it under. Omit for the default inbox list."},
                        "due": {"type": "string", "description": "`YYYY-MM-DD`, or `today` / `tomorrow`."},
                        "today": {"type": "boolean", "description": "Pull onto the Today list regardless of due date."},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "priority": {"type": "integer", "description": "1 is highest. Omit if unstated."},
                        "estimate_minutes": {"type": "integer", "description": "Rough size, for planning against pomodoro sessions."}
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
                        "tags": {"type": "array", "items": {"type": "string"}},
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
                "time_summary",
                "Total focus time and a per-task breakdown. Breaks are excluded.",
                Self::object(
                    json!({"since_days": {"type": "integer", "description": "Only sessions from the last N days. Omit for all time."}}),
                    &[],
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

                let task = store.add_task(&title, opt_str(args, "list_id").as_deref()).map_err(err)?;
                let tags = opt_str_list(args, "tags");
                let today = opt_bool(args, "today", false);
                let priority = args.get("priority").and_then(Value::as_u64).map(|p| p as u8);
                let estimate = args.get("estimate_minutes").and_then(Value::as_u64).map(|m| m as u32);
                let notes = opt_str(args, "notes");

                if due.is_some() || !tags.is_empty() || today || priority.is_some() || estimate.is_some() || notes.is_some() {
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
    fn a_title_alone_is_enough_to_capture() {
        let mut t = tools("capture");
        let added = call(&mut t, "add_task", json!({"title": "Call the bank"}));
        assert_eq!(added["added"]["title"], "Call the bank");
        assert!(added["added"]["due"].is_null(), "no due date should be invented");
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
    fn unknown_ids_and_tools_are_reported() {
        let mut t = tools("errors");
        assert!(t.call("complete_task", &json!({"task_id": "nope"})).is_err());
        assert!(t.call("nonsense", &json!({})).is_err());
    }
}
