//! Resolve an origin against the installed Knowledge server, to check the
//! bridge against a real vault rather than a fixture.
fn main() {
    let origin = std::env::args().nth(1).expect("an origin like knowledge:Some Note.md");
    let context = int_tasks_lib::knowledge_bridge::task_context(origin);
    println!("kind      : {}", context.kind);
    println!("label     : {}", context.label);
    println!("truncated : {}", context.truncated);
    println!("unavailable: {:?}", context.unavailable);
    println!("--- excerpt ---\n{}", context.excerpt);
}
