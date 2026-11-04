//! Checks every command a model wrote before it is shown or written to a
//! shell. A failed check turns the command down; nothing is silently
//! removed, because removing characters changes what a command means.

use thiserror::Error;

/// The longest command, in characters, that is accepted.
pub const MAX_COMMAND_CHARS: usize = 500;

/// Why a command was turned down.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandRejected {
    /// The command is empty or only whitespace.
    #[error("the command is empty")]
    Empty,
    /// The command spans more than one line.
    #[error("the command spans more than one line")]
    MultipleLines,
    /// The command holds a control character, an escape sequence, or a
    /// character that changes how text is displayed.
    #[error("the command contains a hidden or control character")]
    HiddenCharacter,
    /// The command is not valid shell words, such as an unclosed quote.
    #[error("the command is not valid shell syntax")]
    Unparseable,
    /// The command is longer than [`MAX_COMMAND_CHARS`].
    #[error(
        "the command is {length} characters long; the limit is {MAX_COMMAND_CHARS}"
    )]
    TooLong {
        /// The command's length in characters.
        length: usize,
    },
}

/// Check that a command is a single printable line of valid shell words.
///
/// ```
/// use termite::{CommandRejected, check_command};
///
/// assert!(check_command("ls -la ~/code").is_ok());
/// assert_eq!(check_command("ls\nrm -rf ~"), Err(CommandRejected::MultipleLines));
/// ```
pub fn check_command(command: &str) -> Result<(), CommandRejected> {
    if command.trim().is_empty() {
        return Err(CommandRejected::Empty);
    }

    if command.chars().any(is_line_break) {
        return Err(CommandRejected::MultipleLines);
    }

    if command.chars().any(is_hidden) {
        return Err(CommandRejected::HiddenCharacter);
    }

    let length = command.chars().count();
    if length > MAX_COMMAND_CHARS {
        return Err(CommandRejected::TooLong { length });
    }

    if shell_words::split(command).is_err() {
        return Err(CommandRejected::Unparseable);
    }

    Ok(())
}

/// Make model-written text safe to print in a terminal or a one-line
/// format: line breaks, control characters (so escape sequences too) and
/// invisible characters become spaces.
///
/// ```
/// use termite::to_printable;
///
/// assert_eq!(to_printable("red\x1b[31m\ttext\nmore"), "red [31m text more");
/// ```
pub fn to_printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if is_line_break(c) || is_hidden(c) {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Control characters (C0 including tab, DEL, C1) and the invisible
/// characters that can make displayed text differ from what runs:
/// zero-width characters and bidirectional overrides.
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

#[cfg(test)]
mod tests {
    use super::{CommandRejected, MAX_COMMAND_CHARS, check_command};

    #[test]
    fn given_a_command_when_checked_then_the_expected_result_is_returned() {
        let at_limit = format!("echo {}", "a".repeat(MAX_COMMAND_CHARS - 5));
        let over_limit = format!("echo {}", "a".repeat(MAX_COMMAND_CHARS - 4));
        let cases: Vec<(&str, Result<(), CommandRejected>)> = vec![
            ("ls -la ~/code", Ok(())),
            ("grep -rn 'a b' src | wc -l", Ok(())),
            ("echo 'héllo wörld'", Ok(())),
            (&at_limit, Ok(())),
            ("", Err(CommandRejected::Empty)),
            ("   ", Err(CommandRejected::Empty)),
            ("ls\nrm -rf ~", Err(CommandRejected::MultipleLines)),
            ("ls\r", Err(CommandRejected::MultipleLines)),
            ("ls\u{2028}rm", Err(CommandRejected::MultipleLines)),
            ("echo \x1b[31mred", Err(CommandRejected::HiddenCharacter)),
            ("echo \x07", Err(CommandRejected::HiddenCharacter)),
            ("echo \x7f", Err(CommandRejected::HiddenCharacter)),
            ("ls\t-la", Err(CommandRejected::HiddenCharacter)),
            ("echo \u{9b}31m", Err(CommandRejected::HiddenCharacter)),
            ("ls \u{202e}txt.hs", Err(CommandRejected::HiddenCharacter)),
            ("ls \u{200b}-la", Err(CommandRejected::HiddenCharacter)),
            ("echo 'unclosed", Err(CommandRejected::Unparseable)),
            (
                &over_limit,
                Err(CommandRejected::TooLong {
                    length: MAX_COMMAND_CHARS + 1,
                }),
            ),
        ];

        for (command, expected) in cases {
            assert_eq!(check_command(command), expected, "{command:?}");
        }
    }
}
