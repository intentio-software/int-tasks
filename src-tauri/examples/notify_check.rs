//! Exercise colleague_completions' logic directly: what gets announced, once.
use std::collections::HashSet;
fn main() {
    let mine = std::path::PathBuf::from(std::env::args().nth(1).expect("my store"));
    let state = std::path::PathBuf::from(std::env::args().nth(2).expect("announced file"));

    let mut announced: HashSet<String> = std::fs::read_to_string(&state)
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default();
    let first_run = announced.is_empty();
    let mut fresh = Vec::new();

    for m in int_tasks_core::team::members(&mine) {
        if m.is_me { continue }
        let Ok(store) = int_tasks_core::team::open_member(&m) else { continue };
        let Ok(data) = store.read() else { continue };
        for task in data.tasks.iter().filter(|t| t.status.is_done()) {
            if !announced.insert(task.id.clone()) || first_run { continue }
            fresh.push(format!("{} finished: {}", m.name, task.title));
        }
    }
    let ids: Vec<&String> = announced.iter().collect();
    std::fs::write(&state, serde_json::to_string(&ids).unwrap()).unwrap();
    println!("  first_run={first_run} announcing={}", fresh.len());
    for f in &fresh { println!("    {f}"); }
}
