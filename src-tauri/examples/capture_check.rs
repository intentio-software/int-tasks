//! Capture a line the way the app's add_task command does, against real stores.
fn main() {
    let mine = std::path::PathBuf::from(std::env::args().nth(1).expect("my store"));
    let line = std::env::args().nth(2).expect("a line");
    let store = int_tasks_core::Store::open(&mine).unwrap();

    let task = if let Some(owner) = int_tasks_core::capture::parse(&line).owner {
        let members = int_tasks_core::team::members(store.root());
        match members.iter().find(|m| !m.is_me && m.name.eq_ignore_ascii_case(&owner)) {
            Some(target) => {
                println!("routing to {} …", target.name);
                int_tasks_core::team::assign(target, &line, "max").unwrap()
            }
            None => {
                println!("no colleague called {owner}; keeping it here");
                store.capture(&line, None).unwrap()
            }
        }
    } else {
        store.capture(&line, None).unwrap()
    };
    println!("  title    : {}", task.title);
    println!("  assignee : {:?}", task.assignee);
    println!("  from     : {:?}", task.assigned_by);
}
