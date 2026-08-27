//! Exercise the team layer against real store folders on disk.
fn main() {
    let mine = std::path::PathBuf::from(std::env::args().nth(1).expect("path to my store"));
    let members = int_tasks_core::team::members(&mine);
    println!("team of {}:", members.len());
    for m in &members {
        let mark = if m.is_me { "(me)" } else { "    " };
        match int_tasks_core::team::open_member(m).and_then(|s| Ok((s.read()?, s.sessions()?))) {
            Ok((data, _sessions)) => {
                let open = data.tasks.iter().filter(|t| !t.status.is_done()).count();
                let handed = int_tasks_core::team::assigned_to(m, &data.tasks).len();
                println!("  {mark} {:10} {open} open, {handed} handed to them", m.name);
                for t in data.tasks.iter().filter(|t| !t.status.is_done()) {
                    println!("        - {} {}", t.title,
                        t.assigned_by.as_deref().map(|b| format!("(from {b})")).unwrap_or_default());
                }
            }
            Err(e) => println!("  {mark} {:10} unavailable: {e}", m.name),
        }
    }
    if let Some(target) = std::env::args().nth(2) {
        let line = std::env::args().nth(3).expect("a task line");
        let who = members.iter().find(|m| m.name == target).expect("no such member");
        let task = int_tasks_core::team::assign(who, &line, "max").unwrap();
        println!("\nassigned to {}: {} (by {:?})", who.name, task.title, task.assigned_by);
    }
}
