//! Fetching the context a task came from.
//!
//! A task records where it came from in `origin`, as `scheme:reference`. When
//! that scheme is `knowledge`, the note behind it can be read through
//! `int-knowledge-mcp` and shown next to the task — so "what was this about?"
//! is answered without leaving the task, which is the moment the question
//! actually gets asked.
//!
//! Read-only, deliberately. Tasks has no business editing someone's notes.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// How much of a note is worth showing beside a task. Enough to remember what
/// the work is; not so much that the task disappears underneath it.
const EXCERPT_CHARS: usize = 600;

fn locate_binary(name: &str, env_var: &str) -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var(env_var) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join("bin").join(name));
    }
    candidates.push(PathBuf::from("/usr/local/bin").join(name));
    candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
    candidates.into_iter().find(|path| path.is_file())
}

/// One tool call against an MCP server over stdio.
fn call_tool(binary: &PathBuf, name: &str, arguments: Value) -> Result<Value, String> {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("could not start {}: {err}", binary.display()))?;

    {
        let stdin = child.stdin.as_mut().ok_or("no stdin")?;
        writeln!(stdin, "{}", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
            .map_err(|err| err.to_string())?;
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                   "params":{"name": name, "arguments": arguments}})
        )
        .map_err(|err| err.to_string())?;
    }
    drop(child.stdin.take());

    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut result: Option<Value> = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|err| err.to_string())?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        if message.get("id").and_then(Value::as_u64) == Some(2) {
            result = message.get("result").cloned();
        }
    }
    let _ = child.wait();

    let result = result.ok_or("no reply")?;
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        return Err(text);
    }
    serde_json::from_str::<Value>(&text).map_err(|err| format!("unreadable reply: {err}"))
}

/// The context behind a task, ready to show.
#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContext {
    /// `knowledge` or `mindmap`.
    pub kind: String,
    /// What to call it on screen.
    pub label: String,
    /// The note's text, trimmed. Empty when there is nothing to preview.
    pub excerpt: String,
    /// True when the excerpt stops short of the whole note.
    pub truncated: bool,
    /// Set when the context could not be fetched, to be shown as-is.
    pub unavailable: Option<String>,
}

/// Resolve a task's `origin` into something worth putting on screen.
#[tauri::command]
pub fn task_context(origin: String) -> TaskContext {
    let Some((scheme, reference)) = origin.split_once(':') else {
        return TaskContext { kind: "unknown".into(), label: origin, ..Default::default() };
    };

    match scheme {
        "knowledge" => read_note_context(reference),
        // The map is not read here. A node reference means little out of
        // context, and the map is the place to look at a map.
        "mindmap" => {
            let map = reference.split('#').next().unwrap_or(reference);
            TaskContext {
                kind: "mindmap".into(),
                label: map.replace('-', " "),
                ..Default::default()
            }
        }
        other => TaskContext { kind: other.into(), label: reference.to_string(), ..Default::default() },
    }
}

fn read_note_context(path: &str) -> TaskContext {
    let label = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .to_string();

    let Some(binary) = locate_binary("int-knowledge-mcp", "INT_KNOWLEDGE_MCP") else {
        return TaskContext {
            kind: "knowledge".into(),
            label,
            unavailable: Some("Intentio Knowledge is not installed.".into()),
            ..Default::default()
        };
    };

    match call_tool(&binary, "read_note", json!({"path": path})) {
        Ok(reply) => {
            let note = reply.get("note").unwrap_or(&reply);
            let body = note.get("body").and_then(Value::as_str).unwrap_or_default().trim();
            let title = note
                .get("title")
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
                .map(str::to_string)
                .unwrap_or(label);
            // Cut on a character boundary, and prefer to stop at a line end so
            // the excerpt does not break mid-sentence.
            let truncated = body.chars().count() > EXCERPT_CHARS;
            let mut excerpt: String = body.chars().take(EXCERPT_CHARS).collect();
            if truncated {
                if let Some(cut) = excerpt.rfind('\n') {
                    excerpt.truncate(cut);
                }
            }
            TaskContext {
                kind: "knowledge".into(),
                label: title,
                excerpt: excerpt.trim().to_string(),
                truncated,
                unavailable: None,
            }
        }
        Err(error) => TaskContext {
            kind: "knowledge".into(),
            label,
            unavailable: Some(if error.contains("not found") || error.contains("No such") {
                "That note is no longer in the vault.".into()
            } else {
                error
            }),
            ..Default::default()
        },
    }
}
