//! Reads the prefix (`?`, `??`, `?x`, `?b`) from what the user typed.

/// What happens after the commands are ranked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `?`: explain the best command; the user says yes before it goes to
    /// the prompt.
    Confirm,
    /// `??`: explain every distinct command, best first; the user picks one
    /// for the prompt.
    Choose,
    /// `?x`: put the best command straight at the prompt without asking,
    /// only if every check passes; otherwise behave like [`Mode::Confirm`].
    StraightIn,
}

/// A request with its mode, the prefix already removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    mode: Mode,
    request: String,
}

impl Query {
    /// The mode chosen by the prefix.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The request text without the prefix, trimmed.
    pub fn request(&self) -> &str {
        &self.request
    }
}

/// The request after `?b`, which asks for a script instead of a command. Like
/// `?x`, it counts only when a space follows, so `?bash` and `?bd` are `?`
/// queries.
pub fn script_request(input: &str) -> Option<&str> {
    let request = input.trim().strip_prefix("?b")?;
    if !request.starts_with(char::is_whitespace) {
        return None;
    }
    Some(request.trim()).filter(|request| !request.is_empty())
}

/// Read a query from user input. Returns `None` when the input does not
/// start with `?`, or when nothing is left after the prefix.
///
/// `?x` counts as the prefix only when a space or the end follows it, so
/// `?xargs usage` is a `?` query for "xargs usage".
///
/// ```
/// use termite::{Mode, parse_query};
///
/// let query = parse_query("?? restart docker").expect("a query");
/// assert_eq!(query.mode(), Mode::Choose);
/// assert_eq!(query.request(), "restart docker");
/// assert!(parse_query("ls -la").is_none());
/// ```
pub fn parse_query(input: &str) -> Option<Query> {
    let input = input.trim();

    let (mode, rest) = if let Some(rest) = input.strip_prefix("??") {
        (Mode::Choose, rest)
    } else if let Some(rest) = input
        .strip_prefix("?x")
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        (Mode::StraightIn, rest)
    } else {
        (Mode::Confirm, input.strip_prefix('?')?)
    };

    let request = rest.trim();
    if request.is_empty() {
        return None;
    }

    Some(Query {
        mode,
        request: request.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Mode, parse_query, script_request};

    #[test]
    fn given_b_prefixes_then_only_a_space_after_it_asks_for_a_script() {
        for (input, expected) in [
            ("?b back up notes", Some("back up notes")),
            ("  ?b\tback up", Some("back up")),
            ("?b", None),
            ("?b   ", None),
            ("?bash usage", None),
            ("?bd thing", None),
            ("? b thing", None),
            ("b thing", None),
        ] {
            assert_eq!(script_request(input), expected, "{input}");
        }
    }

    #[test]
    fn given_prefixed_input_when_parsed_then_mode_and_request_are_returned() {
        let cases = [
            ("? restart docker", Mode::Confirm, "restart docker"),
            ("?restart docker", Mode::Confirm, "restart docker"),
            ("?? restart docker", Mode::Choose, "restart docker"),
            ("??restart docker", Mode::Choose, "restart docker"),
            (
                "?x list my code folder",
                Mode::StraightIn,
                "list my code folder",
            ),
            ("?x\tlist files", Mode::StraightIn, "list files"),
            ("?xargs usage", Mode::Confirm, "xargs usage"),
            ("?X list files", Mode::Confirm, "X list files"),
            ("  ? trim me  ", Mode::Confirm, "trim me"),
        ];

        for (input, mode, request) in cases {
            let query = parse_query(input)
                .unwrap_or_else(|| panic!("{input:?} should parse"));
            assert_eq!(query.mode(), mode, "mode for {input:?}");
            assert_eq!(query.request(), request, "request for {input:?}");
        }
    }

    #[test]
    fn given_input_without_a_request_when_parsed_then_none_is_returned() {
        let cases = ["", "   ", "?", "? ", "??", "?? ", "?x", "?x  ", "ls -la"];

        for input in cases {
            assert!(parse_query(input).is_none(), "{input:?} is not a query");
        }
    }
}
