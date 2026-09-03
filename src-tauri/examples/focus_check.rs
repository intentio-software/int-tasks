//! Check that a colleague's focus comes from their published summary, and that
//! their session log is not read even though it is sitting right there.
fn main() {
    let mine = std::path::PathBuf::from(std::env::args().nth(1).expect("my store"));
    for m in int_tasks_core::team::members(&mine) {
        let published = int_tasks_core::team::read_focus(&m.root);
        let log_on_disk = m.root.join("sessions.jsonl").is_file();
        println!(
            "  {:8} is_me={:5} log_on_disk={:5} published={}",
            m.name, m.is_me, log_on_disk,
            published.map(|f| format!("{} sessions, {} min, {} day streak",
                f.sessions_today, f.focus_minutes_today, f.streak_days))
                .unwrap_or_else(|| "none".into())
        );
    }
}
