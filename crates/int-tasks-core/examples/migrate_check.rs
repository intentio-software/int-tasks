//! Load a store from a path and print what came back — used to rehearse the
//! document-to-log migration against a copy of a real store.
fn main() {
    let root = std::env::args().nth(1).expect("a store path");
    let store = int_tasks_core::Store::open(&root).unwrap();
    let data = store.read().unwrap();
    println!("tasks: {} | boards: {} | revision: {}", data.tasks.len(), data.boards.len(), data.revision);
    for task in data.tasks.iter().take(5) {
        println!("  - {} (rev {}, {:?})", task.title, task.revision, task.status);
    }
}
