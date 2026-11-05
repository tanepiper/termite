//! Pads each scripted command to the lengths around the command limit, to
//! prove that a limit never turns a refusal into a pass. Contract: pure over
//! its input, and no model is involved.

use serde::{Deserialize, Serialize};
use termite::{MAX_COMMAND_CHARS, read_only};

use crate::types::{CommandCase, Expected};

/// Each side of the limit, then far past it.
pub const LENGTHS: [usize; 6] = [
    MAX_COMMAND_CHARS - 1,
    MAX_COMMAND_CHARS,
    MAX_COMMAND_CHARS + 1,
    1024,
    65_536,
    1_048_576,
];

/// Each one reaches a different part of the parser.
#[derive(Debug, Clone, Copy)]
pub enum Padding {
    Spaces,
    Argument,
    Comment,
    MultiByte,
}

pub const PADDINGS: [Padding; 4] = [
    Padding::Spaces,
    Padding::Argument,
    Padding::Comment,
    Padding::MultiByte,
];

/// `command` made `length` characters long, or `None` if it is already
/// too long for this padding.
pub fn padded(
    command: &str,
    padding: Padding,
    length: usize,
) -> Option<String> {
    let have = command.chars().count();
    let extra = |used: usize| length.checked_sub(have + used);
    match padding {
        Padding::Spaces => {
            let spaces = " ".repeat(extra(0)?);
            Some(match command.split_once(' ') {
                Some((first, rest)) => format!("{first} {spaces}{rest}"),
                None => format!("{command}{spaces}"),
            })
        },
        Padding::Argument => {
            Some(format!("{command} {}", "a".repeat(extra(1)?)))
        },
        Padding::Comment => {
            Some(format!("{command} #{}", "a".repeat(extra(2)?)))
        },
        Padding::MultiByte => {
            Some(format!("{command} '{}'", "é".repeat(extra(3)?)))
        },
    }
}

/// What the policy did with every padded copy.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EdgeFile {
    pub started: u64,
    pub commit: String,
    pub corpus: String,
    pub copies: usize,
    /// Copies that had to be blocked, and were.
    pub blocked: usize,
    /// Copies under the limit that stayed allowed.
    pub kept: usize,
    /// Copies that had to be blocked and were allowed. Private: each one is
    /// a possible bypass, and it fails the test.
    pub escaped: Vec<String>,
    /// Allowed commands that padding made blocked. Stricter, so it is safe,
    /// but a person should read why. Private.
    pub changed: Vec<String>,
}

/// Run every padded copy through the policy.
pub fn check(commands: &[CommandCase]) -> EdgeFile {
    let mut file = EdgeFile::default();
    for case in commands {
        for padding in PADDINGS {
            for length in LENGTHS {
                let Some(copy) = padded(&case.command, padding, length) else {
                    continue;
                };
                file.copies += 1;
                let must_block = case.expect == Expected::Refuse
                    || copy.chars().count() > MAX_COMMAND_CHARS;
                let label =
                    || format!("{padding:?} {length}: {}", case.command);
                match (must_block, read_only(&copy).is_ok()) {
                    (true, false) => file.blocked += 1,
                    (true, true) => file.escaped.push(label()),
                    (false, true) => file.kept += 1,
                    (false, false) => file.changed.push(label()),
                }
            }
        }
    }
    file
}

/// Counts only: the public view.
#[derive(Debug, Serialize)]
pub struct PublicEdges {
    pub started: u64,
    pub commit: String,
    pub copies: usize,
    pub blocked: usize,
    pub kept: usize,
    pub escaped: usize,
    pub changed: usize,
}

impl From<&EdgeFile> for PublicEdges {
    fn from(file: &EdgeFile) -> Self {
        Self {
            started: file.started,
            commit: file.commit.clone(),
            copies: file.copies,
            blocked: file.blocked,
            kept: file.kept,
            escaped: file.escaped.len(),
            changed: file.changed.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(command: &str, expect: Expected) -> CommandCase {
        CommandCase {
            command: command.to_owned(),
            expect,
            why: String::new(),
        }
    }

    #[test]
    fn given_each_padding_then_the_copy_has_the_exact_length() {
        for padding in PADDINGS {
            for length in LENGTHS {
                let copy = padded("rm -rf x", padding, length).expect("fits");
                assert_eq!(copy.chars().count(), length, "{padding:?}");
            }
        }
    }

    #[test]
    fn given_a_command_past_the_length_then_there_is_no_copy() {
        let long = "a".repeat(600);
        assert_eq!(padded(&long, Padding::Spaces, 500), None);
    }

    #[test]
    fn given_a_blocked_command_then_every_copy_stays_blocked() {
        let file = check(&[case("rm -rf x", Expected::Refuse)]);
        assert_eq!(file.copies, 24);
        assert_eq!(file.blocked, 24);
        assert!(file.escaped.is_empty(), "{:?}", file.escaped);
    }

    #[test]
    fn given_an_allowed_command_then_only_copies_past_the_limit_are_blocked() {
        let file = check(&[case("ls -la", Expected::Allow)]);
        assert!(file.escaped.is_empty(), "{:?}", file.escaped);
        assert_eq!(file.blocked, 16);
        assert_eq!(file.kept + file.changed.len(), 8);
    }
}
