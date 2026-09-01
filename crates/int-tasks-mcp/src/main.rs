//! `int-tasks-mcp` — an MCP server over the Intentio Tasks store.
//!
//! Runs as a plain stdio process against the same JSON files the desktop app
//! uses, so it works whether or not the app is open:
//!
//! ```text
//! claude mcp add tasks -- int-tasks-mcp
//! ```

mod mcp;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;

use int_tasks_core::Store;
use tools::TaskTools;

const USAGE: &str = "\
int-tasks-mcp — MCP server for Intentio Tasks

USAGE:
    int-tasks-mcp [STORE_DIR]

ARGS:
    <STORE_DIR>    Folder holding tasks.jsonl and sessions.jsonl.
                   Defaults to wherever the desktop app keeps them: the folder
                   named in ~/.intentio/tasks-root if the store has been moved
                   into a shared Git folder, otherwise ~/.intentio/tasks — so
                   usually you pass nothing at all.

OPTIONS:
    -h, --help     Print this help
    -V, --version  Print version

ENVIRONMENT:
    INT_TASKS_DIR  Store folder, used when no path is given

Register it with an agent:

    claude mcp add tasks -- int-tasks-mcp
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let root = match parse_args(&args) {
        Ok(Some(root)) => root,
        // --help / --version already printed.
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let store = match Store::open(&root) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("error: cannot open task store at {}: {err}", root.display());
            return ExitCode::FAILURE;
        }
    };

    // Diagnostics go to stderr; stdout carries protocol traffic only.
    eprintln!("[tasks] store: {}", store.root().display());

    match mcp::serve(TaskTools::new(store)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// `Ok(None)` means help or version was printed and the process should exit.
fn parse_args(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut root: Option<PathBuf> = None;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("int-tasks-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            other if other.starts_with('-') => return Err(format!("unknown option: {other}")),
            other => {
                if root.is_some() {
                    return Err("only one store folder can be given".into());
                }
                root = Some(expand(other));
                index += 1;
                continue;
            }
        }
    }

    // `configured_root`, not `default_root`: it honours the `tasks-root` marker
    // the desktop app writes when the store is moved into a shared Git folder.
    // Falling back to the default here silently split the agent's tasks from
    // the app's - written to ~/.intentio/tasks, never synced, never seen.
    let root = root
        .or_else(|| std::env::var_os("INT_TASKS_DIR").map(|value| expand(&value.to_string_lossy())))
        .or_else(Store::configured_root)
        .ok_or("cannot determine a store folder; pass one explicitly")?;
    Ok(Some(root))
}

/// Expand a leading `~`, which clients routinely pass through unexpanded.
fn expand(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" || trimmed.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return if trimmed == "~" { home } else { home.join(&trimmed[2..]) };
        }
    }
    PathBuf::from(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_to_the_shared_store_when_given_nothing() {
        // The common case: the agent and the app must land on the same folder
        // without the user configuring anything - including when the app has
        // moved the store via the tasks-root marker.
        let root = parse_args(&[]).unwrap().unwrap();
        assert_eq!(root, Store::configured_root().unwrap(), "agent and app disagree on the store");
    }

    #[test]
    fn accepts_an_explicit_folder() {
        assert_eq!(parse_args(&to_args(&["/tmp/store"])).unwrap().unwrap(), PathBuf::from("/tmp/store"));
    }

    #[test]
    fn rejects_unknown_options_and_extra_paths() {
        assert!(parse_args(&to_args(&["--nope"])).is_err());
        assert!(parse_args(&to_args(&["/a", "/b"])).is_err());
    }
}
