//! Reading structure out of a typed title.
//!
//! Capture has to stay a single line of typing — stopping to fill in a project
//! field is exactly the friction this app exists to avoid. So the prefix people
//! already write by hand is treated as input:
//!
//! ```text
//! (stm) [chore] clean up the front end login page
//! ```
//!
//! becomes the title `clean up the front end login page`, filed under project
//! `stm` and tagged `chore`. Both markers are optional and may appear in either
//! order.
//!
//! Parsing only ever happens on capture. Editing a title later leaves it exactly
//! as typed, because a field that silently rewrites itself while you are working
//! in it is worse than one that never helps at all.

/// What a typed line turned out to contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub title: String,
    pub project: Option<String>,
    pub tags: Vec<String>,
}

/// Split a typed line into a title, a project and type tags.
///
/// Leading `(project)` and `[tag]` markers are consumed in any order until
/// something that is not a marker appears; everything from there on is the
/// title, untouched.
pub fn parse(input: &str) -> Captured {
    let mut rest = input.trim();
    let mut project: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    // Anything a marker cannot claim is put back, so nothing is ever silently
    // dropped — a second `(project)` is unusual input, not a licence to lose it.
    let mut kept: Vec<&str> = Vec::new();

    while let Some((marker, open, remainder)) = take_marker(rest) {
        match open {
            '(' if project.is_none() => project = Some(marker.to_string()),
            '[' => tags.push(marker.to_string()),
            // A second project marker is not a project.
            _ => kept.push(&rest[..rest.len() - remainder.len()]),
        }
        rest = remainder;
    }

    let mut title = String::new();
    for piece in kept {
        title.push_str(piece.trim());
        title.push(' ');
    }
    title.push_str(rest);
    let title = title.trim().to_string();

    // A line that is nothing but markers has no title to show in a list, so it
    // is left exactly as typed rather than becoming an unidentifiable task.
    if title.is_empty() {
        return Captured { title: input.trim().to_string(), project: None, tags: Vec::new() };
    }

    tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Captured { title, project, tags }
}

/// Take one leading `(word)` or `[word]` marker.
///
/// Returns the word, which bracket opened it, and what follows. A marker must
/// hold a single word: `(see note)` is prose someone opened a line with, not a
/// project code.
fn take_marker(input: &str) -> Option<(&str, char, &str)> {
    let open = input.chars().next()?;
    let close = match open {
        '(' => ')',
        '[' => ']',
        _ => return None,
    };

    let end = input.find(close)?;
    let body = &input[open.len_utf8()..end];
    if body.is_empty() || body.chars().any(char::is_whitespace) {
        return None;
    }

    Some((body, open, input[end + close.len_utf8()..].trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> (String, Option<String>, Vec<String>) {
        let captured = parse(input);
        (captured.title, captured.project, captured.tags)
    }

    #[test]
    fn the_convention_is_read_in_full() {
        let (title, project, tags) = parsed("(stm) [chore] clean up the front end login page");
        assert_eq!(title, "clean up the front end login page");
        assert_eq!(project.as_deref(), Some("stm"));
        assert_eq!(tags, vec!["chore"]);
    }

    #[test]
    fn either_order_works() {
        let (title, project, tags) = parsed("[bug] (acme) the export hangs");
        assert_eq!(title, "the export hangs");
        assert_eq!(project.as_deref(), Some("acme"));
        assert_eq!(tags, vec!["bug"]);
    }

    #[test]
    fn each_marker_is_optional() {
        assert_eq!(parsed("(stm) ship it"), ("ship it".into(), Some("stm".into()), vec![]));
        assert_eq!(parsed("[chore] ship it"), ("ship it".into(), None, vec!["chore".to_string()]));
        assert_eq!(parsed("ship it"), ("ship it".into(), None, vec![]));
    }

    #[test]
    fn several_tags_are_all_kept() {
        let (title, _, tags) = parsed("(stm) [chore] [urgent] rotate the keys");
        assert_eq!(title, "rotate the keys");
        assert_eq!(tags, vec!["chore", "urgent"]);
    }

    #[test]
    fn spacing_is_not_required() {
        let (title, project, tags) = parsed("(stm)[chore]clean up");
        assert_eq!(title, "clean up");
        assert_eq!(project.as_deref(), Some("stm"));
        assert_eq!(tags, vec!["chore"]);
    }

    #[test]
    fn markers_are_only_read_at_the_start() {
        // Mid-sentence brackets are prose and must survive untouched.
        let (title, project, tags) = parsed("explain the (stm) rollout [later]");
        assert_eq!(title, "explain the (stm) rollout [later]");
        assert_eq!(project, None);
        assert!(tags.is_empty());
    }

    #[test]
    fn a_parenthesised_phrase_is_not_a_project() {
        let (title, project, _) = parsed("(see note) follow up with the client");
        assert_eq!(title, "(see note) follow up with the client");
        assert_eq!(project, None);
    }

    #[test]
    fn an_unclosed_bracket_is_left_alone() {
        let (title, project, _) = parsed("(stm clean up the login page");
        assert_eq!(title, "(stm clean up the login page");
        assert_eq!(project, None);
    }

    #[test]
    fn a_second_project_stays_in_the_title() {
        // Better a strange title than a quietly discarded word.
        let (title, project, _) = parsed("(stm) (acme) shared migration");
        assert_eq!(project.as_deref(), Some("stm"));
        assert_eq!(title, "(acme) shared migration");
    }

    #[test]
    fn markers_alone_are_kept_as_the_title() {
        // There would otherwise be nothing to identify the task by.
        let (title, project, tags) = parsed("(stm) [chore]");
        assert_eq!(title, "(stm) [chore]");
        assert_eq!(project, None);
        assert!(tags.is_empty());
    }

    #[test]
    fn a_repeated_tag_is_only_recorded_once() {
        let (_, _, tags) = parsed("[chore] [Chore] tidy up");
        assert_eq!(tags, vec!["chore"]);
    }

    #[test]
    fn case_and_length_are_the_users_business() {
        // Nothing here enforces three letters: the convention is the user's, and
        // a task typed `(intentio)` should not lose its project.
        let (_, project, _) = parsed("(Intentio) write the release notes");
        assert_eq!(project.as_deref(), Some("Intentio"));
    }
}
