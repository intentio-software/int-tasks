//! Reading structure out of a typed title.
//!
//! Capture has to stay a single line of typing — stopping to fill in a project
//! field is exactly the friction this app exists to avoid. So the prefix people
//! already write by hand is treated as input:
//!
//! ```text
//! (stm) [chore] clean up the login page @vernon due:tomorrow i:8 e:3
//! ```
//!
//! becomes the title `clean up the login page`, filed under project `stm`,
//! tagged `chore`, owned by `vernon`, due tomorrow, impact 8, effort 3. Every
//! marker is optional.
//!
//! Trailing markers are `@owner`, `due:`, `impact:` (or `i:`) and `effort:`
//! (or `e:`). A due date is `YYYY-MM-DD`, `today`, `tomorrow`, or `+N` for N
//! days from now. Impact and effort are 1 to 10, matching the matrix.
//!
//! Project and tag are read from the front, owner and due date from the back,
//! which is both how people write them and what keeps prose safe: `email
//! max@intentio.co.za` ends in a token that does not begin with `@`, so nothing
//! is taken from it.
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
    /// Who the task is for, from a trailing `@owner`.
    pub owner: Option<String>,
    /// A due date as typed: `YYYY-MM-DD`, `today`, `tomorrow` or `+N`.
    /// Resolve it with [`resolve_due`], which needs to know what day it is.
    pub due: Option<String>,
    /// How much finishing this is worth, 1-10.
    pub impact: Option<u8>,
    /// How much it will cost, 1-10.
    pub effort: Option<u8>,
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

    let mut body = String::new();
    for piece in kept {
        body.push_str(piece.trim());
        body.push(' ');
    }
    body.push_str(rest);
    let body = body.trim();

    // Owner and due date are taken off the end, rightmost first.
    let mut owner: Option<String> = None;
    let mut due: Option<String> = None;
    let mut impact: Option<u8> = None;
    let mut effort: Option<u8> = None;
    let mut head = body;
    while let Some((trailing, remainder)) = take_trailing(head) {
        match trailing {
            Trailing::Owner(name) if owner.is_none() => owner = Some(name.to_string()),
            Trailing::Due(date) if due.is_none() => due = Some(date.to_string()),
            Trailing::Impact(value) if impact.is_none() => impact = Some(value),
            Trailing::Effort(value) if effort.is_none() => effort = Some(value),
            // A second one of either is not a correction; leave it in the title
            // rather than quietly replacing what was already read.
            _ => break,
        }
        head = remainder;
    }

    let title = head.trim().to_string();

    // A line that is nothing but markers has no title to show in a list, so it
    // is left exactly as typed rather than becoming an unidentifiable task.
    if title.is_empty() {
        return Captured {
            title: input.trim().to_string(),
            project: None,
            tags: Vec::new(),
            owner: None,
            due: None,
            impact: None,
            effort: None,
        };
    }

    tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Captured { title, project, tags, owner, due, impact, effort }
}

enum Trailing<'a> {
    Owner(&'a str),
    Due(&'a str),
    Impact(u8),
    Effort(u8),
}

/// Take one trailing `@owner` or `due:YYYY-MM-DD`, returning what precedes it.
///
/// Only a whole final token counts, so a bracketed aside or an address in the
/// middle of a sentence is never touched. The date must be a real one: a
/// `due:soon` is prose, and reading it as a date would be a guess.
fn take_trailing(input: &str) -> Option<(Trailing<'_>, &str)> {
    let trimmed = input.trim_end();
    let (head, token) = match trimmed.rfind(char::is_whitespace) {
        Some(at) => (&trimmed[..at], &trimmed[at + 1..]),
        None => ("", trimmed),
    };

    if let Some(name) = token.strip_prefix('@') {
        if !name.is_empty() {
            return Some((Trailing::Owner(name), head));
        }
    }
    if let Some(date) = token.strip_prefix("due:") {
        if is_due_token(date) {
            return Some((Trailing::Due(date), head));
        }
    }
    for prefix in ["impact:", "i:"] {
        if let Some(value) = token.strip_prefix(prefix).and_then(score) {
            return Some((Trailing::Impact(value), head));
        }
    }
    for prefix in ["effort:", "e:"] {
        if let Some(value) = token.strip_prefix(prefix).and_then(score) {
            return Some((Trailing::Effort(value), head));
        }
    }
    None
}

/// A score is 1 to 10. Zero means unscored in the model, and writing `i:0`
/// almost always means a typo rather than a deliberate nothing.
fn score(value: &str) -> Option<u8> {
    match value.parse::<u8>() {
        Ok(number) if (1..=10).contains(&number) => Some(number),
        _ => None,
    }
}

/// Whether a `due:` value is one we understand. Deliberately strict: a date
/// we cannot read is left in the title rather than guessed at.
fn is_due_token(value: &str) -> bool {
    if crate::dates::civil_days(value).is_some() {
        return true;
    }
    if value.eq_ignore_ascii_case("today") || value.eq_ignore_ascii_case("tomorrow") {
        return true;
    }
    value
        .strip_prefix('+')
        .and_then(|days| days.parse::<u16>().ok())
        .is_some_and(|days| days <= 3_650)
}

/// Turn a due token into a date, given what day it is.
///
/// Kept out of `parse` so that reading a line stays a pure function of the
/// line — the clock belongs to the caller, which is also the only place that
/// knows the user's timezone.
pub fn resolve_due(token: &str, today: &str) -> Option<String> {
    if crate::dates::civil_days(token).is_some() {
        return Some(token.to_string());
    }
    if token.eq_ignore_ascii_case("today") {
        return Some(today.to_string());
    }
    let days = if token.eq_ignore_ascii_case("tomorrow") {
        1
    } else {
        token.strip_prefix('+')?.parse::<i64>().ok()?
    };
    crate::dates::civil_date(crate::dates::civil_days(today)? + days).into()
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
    fn the_whole_convention_reads_as_one_line() {
        let captured = parse("(stm) [chore] clean up the login page @vernon due:2026-09-01");
        assert_eq!(captured.title, "clean up the login page");
        assert_eq!(captured.project.as_deref(), Some("stm"));
        assert_eq!(captured.tags, vec!["chore"]);
        assert_eq!(captured.owner.as_deref(), Some("vernon"));
        assert_eq!(captured.due.as_deref(), Some("2026-09-01"));
    }

    #[test]
    fn owner_and_due_may_be_written_in_either_order() {
        let captured = parse("ship the release due:2026-09-01 @max");
        assert_eq!(captured.title, "ship the release");
        assert_eq!(captured.owner.as_deref(), Some("max"));
        assert_eq!(captured.due.as_deref(), Some("2026-09-01"));
    }

    #[test]
    fn either_trailing_marker_is_optional() {
        let owned = parse("review the contract @vernon");
        assert_eq!(owned.title, "review the contract");
        assert_eq!(owned.owner.as_deref(), Some("vernon"));
        assert_eq!(owned.due, None);

        let dated = parse("review the contract due:2026-09-01");
        assert_eq!(dated.title, "review the contract");
        assert_eq!(dated.due.as_deref(), Some("2026-09-01"));
        assert_eq!(dated.owner, None);
    }

    #[test]
    fn scores_are_read_long_or_short() {
        let long = parse("rewrite the importer impact:8 effort:3");
        assert_eq!(long.title, "rewrite the importer");
        assert_eq!(long.impact, Some(8));
        assert_eq!(long.effort, Some(3));

        let short = parse("rewrite the importer i:8 e:3");
        assert_eq!(short.title, "rewrite the importer");
        assert_eq!((short.impact, short.effort), (Some(8), Some(3)));
    }

    #[test]
    fn the_whole_line_reads_together() {
        let c = parse("(stm) [chore] clean up the login page @vernon due:tomorrow i:8 e:3");
        assert_eq!(c.title, "clean up the login page");
        assert_eq!(c.project.as_deref(), Some("stm"));
        assert_eq!(c.tags, vec!["chore"]);
        assert_eq!(c.owner.as_deref(), Some("vernon"));
        assert_eq!(c.due.as_deref(), Some("tomorrow"));
        assert_eq!((c.impact, c.effort), (Some(8), Some(3)));
    }

    #[test]
    fn a_score_outside_the_scale_is_not_a_score() {
        // 0 and 11 are typos, not opinions. They stay in the title.
        for line in ["ship it i:0", "ship it i:11", "ship it e:99", "ship it i:high"] {
            let c = parse(line);
            assert_eq!(c.title, line, "{line} should be left alone");
            assert_eq!((c.impact, c.effort), (None, None));
        }
    }

    #[test]
    fn relative_due_dates_are_accepted_and_resolved_by_the_caller() {
        assert_eq!(parse("ship it due:today").due.as_deref(), Some("today"));
        assert_eq!(parse("ship it due:tomorrow").due.as_deref(), Some("tomorrow"));
        assert_eq!(parse("ship it due:+10").due.as_deref(), Some("+10"));

        assert_eq!(resolve_due("today", "2026-08-28").as_deref(), Some("2026-08-28"));
        assert_eq!(resolve_due("tomorrow", "2026-08-28").as_deref(), Some("2026-08-29"));
        assert_eq!(resolve_due("+4", "2026-08-28").as_deref(), Some("2026-09-01"), "crosses the month");
        assert_eq!(resolve_due("2026-12-25", "2026-08-28").as_deref(), Some("2026-12-25"));
    }

    #[test]
    fn a_due_that_is_not_a_date_stays_in_the_title() {
        for line in ["renew it due:soon", "renew it due:2026-13-45", "renew it due:next-week"] {
            assert_eq!(parse(line).title, line);
            assert_eq!(parse(line).due, None);
        }
    }

    #[test]
    fn an_address_in_the_middle_of_a_line_is_left_alone() {
        // The reason owner is read from the end and not from anywhere.
        let captured = parse("email max@intentio.co.za about the invoice");
        assert_eq!(captured.title, "email max@intentio.co.za about the invoice");
        assert_eq!(captured.owner, None);
    }

    #[test]
    fn a_trailing_address_is_not_mistaken_for_an_owner() {
        let captured = parse("chase the invoice with accounts@acme.com");
        assert_eq!(captured.title, "chase the invoice with accounts@acme.com");
        assert_eq!(captured.owner, None);
    }

    #[test]
    fn a_due_date_has_to_be_a_date() {
        // "soon" is prose, and reading it as a date would be a guess.
        let captured = parse("renew the certificate due:soon");
        assert_eq!(captured.title, "renew the certificate due:soon");
        assert_eq!(captured.due, None);

        let impossible = parse("renew the certificate due:2026-13-45");
        assert_eq!(impossible.title, "renew the certificate due:2026-13-45");
        assert_eq!(impossible.due, None);
    }

    #[test]
    fn a_second_owner_stays_in_the_title() {
        // Same rule as a second project: a strange title beats a lost word.
        let captured = parse("pair on the migration @max @vernon");
        assert_eq!(captured.owner.as_deref(), Some("vernon"));
        assert_eq!(captured.title, "pair on the migration @max");
    }

    #[test]
    fn a_line_of_nothing_but_markers_is_kept_as_typed() {
        let captured = parse("(stm) @vernon due:2026-09-01");
        assert_eq!(captured.title, "(stm) @vernon due:2026-09-01");
        assert_eq!(captured.owner, None);
        assert_eq!(captured.due, None);
    }

    #[test]
    fn an_owner_may_be_written_however_the_team_writes_names() {
        let captured = parse("draft the SOW @Vernon.vd");
        assert_eq!(captured.owner.as_deref(), Some("Vernon.vd"));
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
