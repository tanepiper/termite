//! Reads the mode prefix (`?`, `??`, `?x`) from what the user typed.

/// What happens after the commands are ranked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `?`: show the best command; the user confirms it.
    Confirm,
    /// `??`: show every distinct command, best first; the user picks one.
    Choose,
    /// `?x`: run the best command without asking, only if the safety gate
    /// passes; otherwise behave like [`Mode::Confirm`].
    RunIfSafe,
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
        (Mode::RunIfSafe, rest)
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
    use super::{Mode, parse_query};

    #[test]
    fn given_prefixed_input_when_parsed_then_mode_and_request_are_returned() {
        let cases = [
            ("? restart docker", Mode::Confirm, "restart docker"),
            ("?restart docker", Mode::Confirm, "restart docker"),
            ("?? restart docker", Mode::Choose, "restart docker"),
            ("??restart docker", Mode::Choose, "restart docker"),
            (
                "?x list my code folder",
                Mode::RunIfSafe,
                "list my code folder",
            ),
            ("?x\tlist files", Mode::RunIfSafe, "list files"),
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
