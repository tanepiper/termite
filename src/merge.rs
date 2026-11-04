//! Merges candidates that are really the same command, so `??` never shows
//! the same thing twice and a single sensible command stays single.

use std::collections::BTreeSet;

use crate::candidate::Candidate;

/// Characters that make the shell expand a word. A word holding one keeps
/// its quotes in the merge key, because quoting changes its meaning.
const EXPANDING_CHARS: &[char] = &['$', '`', '*', '?', '[', '~', '{', '!'];

/// Characters that, outside quotes, join or redirect commands.
const OPERATOR_CHARS: &[char] = &['|', '&', ';', '<', '>', '(', ')'];

/// What two candidates must share to be the same command.
#[derive(Debug, PartialEq, Eq)]
enum MergeKey {
    /// Only an identical word list matches (used when a word mixes text and
    /// operators, such as `ls|wc`).
    Exact(Vec<String>),
    /// Programs with their options as sets, split at operators.
    Normalized(Vec<KeyPart>),
}

#[derive(Debug, PartialEq, Eq)]
enum KeyPart {
    Operator(String),
    Command(CommandShape),
}

/// One program call with option order removed: `ls -la` and `ls -a -l`
/// have the same shape.
#[derive(Debug, Default, PartialEq, Eq)]
struct CommandShape {
    program: Option<String>,
    short_options: BTreeSet<char>,
    long_options: BTreeSet<String>,
    operands: Vec<String>,
}

impl CommandShape {
    fn add_word(&mut self, word: Word, after_double_dash: &mut bool) {
        if self.program.is_none() {
            self.program = Some(word.value);
            return;
        }

        if !word.may_be_option || *after_double_dash {
            self.operands.push(word.value);
        } else if word.value == "--" {
            *after_double_dash = true;
            self.operands.push(word.value);
        } else if let Some(long) = word.value.strip_prefix("--") {
            self.long_options.insert(long.to_owned());
        } else if let Some(short) = word
            .value
            .strip_prefix('-')
            .filter(|short| !short.is_empty())
        {
            self.short_options.extend(short.chars());
        } else {
            self.operands.push(word.value);
        }
    }
}

/// A word as written, with its quotes, and whether it holds an operator
/// character outside quotes.
struct RawWord {
    text: String,
    has_operator: bool,
}

/// A word ready for the merge key.
struct Word {
    value: String,
    may_be_option: bool,
}

/// Merge candidates that differ only in spacing, quoting or the order of
/// their options. The first command text is kept, the longest description
/// wins, and the most harmful risk label wins. Order of first appearance is
/// kept.
///
/// Quoting is only ignored when no word holds a character the shell expands
/// (`$`, backticks, globs, `~`, braces); otherwise only exact matches merge.
///
/// ```
/// use termite::{Candidate, Risk, merge_candidates};
///
/// let merged = merge_candidates(vec![
///     Candidate::new("ls -la", "List files", Risk::Read),
///     Candidate::new("ls -al", "List all files, long format", Risk::Read),
/// ]);
/// assert_eq!(merged.len(), 1);
/// assert_eq!(merged[0].command(), "ls -la");
/// ```
pub fn merge_candidates(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut merged: Vec<(Option<MergeKey>, Candidate)> = Vec::new();

    for candidate in candidates {
        let key = merge_key(candidate.command());
        let existing = merged.iter_mut().find(|(existing_key, kept)| {
            match (existing_key, &key) {
                (Some(existing_key), Some(key)) => existing_key == key,
                _ => kept.command() == candidate.command(),
            }
        });

        match existing {
            Some((_, kept)) => kept.absorb_duplicate(candidate),
            None => merged.push((key, candidate)),
        }
    }

    merged.into_iter().map(|(_, candidate)| candidate).collect()
}

/// Build the merge key, or `None` when the command cannot be split into
/// words (it then only merges with an identical command).
fn merge_key(command: &str) -> Option<MergeKey> {
    let raw_words = split_raw_words(command)?;

    if raw_words
        .iter()
        .any(|word| word.has_operator && !is_operator_word(&word.text))
    {
        let words = raw_words.into_iter().map(|word| word.text).collect();
        return Some(MergeKey::Exact(words));
    }

    let mut parts = Vec::new();
    let mut shape = CommandShape::default();
    let mut after_double_dash = false;

    for raw in raw_words {
        if raw.has_operator {
            parts.push(KeyPart::Command(std::mem::take(&mut shape)));
            parts.push(KeyPart::Operator(raw.text));
            after_double_dash = false;
        } else {
            shape.add_word(key_word(raw.text), &mut after_double_dash);
        }
    }
    parts.push(KeyPart::Command(shape));

    Some(MergeKey::Normalized(parts))
}

/// Remove a word's quotes, unless it holds a character the shell expands.
fn key_word(text: String) -> Word {
    if text.contains(EXPANDING_CHARS) {
        return Word {
            value: text,
            may_be_option: false,
        };
    }

    match shell_words::split(&text) {
        Ok(mut values) if values.len() == 1 => Word {
            value: values.remove(0),
            may_be_option: true,
        },
        _ => Word {
            value: text,
            may_be_option: false,
        },
    }
}

fn is_operator_word(text: &str) -> bool {
    text.chars().all(|c| OPERATOR_CHARS.contains(&c))
}

/// Split a command at whitespace outside quotes, keeping each word exactly
/// as written. Returns `None` for an unclosed quote or a trailing `\`.
fn split_raw_words(command: &str) -> Option<Vec<RawWord>> {
    let mut words = Vec::new();
    let mut text = String::new();
    let mut has_operator = false;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for c in command.chars() {
        if escaped {
            text.push(c);
            escaped = false;
            continue;
        }

        match quote {
            Some(open) => {
                text.push(c);
                if c == open {
                    quote = None;
                } else if c == '\\' && open == '"' {
                    escaped = true;
                }
            },
            None if c.is_whitespace() => {
                if !text.is_empty() {
                    words.push(RawWord {
                        text: std::mem::take(&mut text),
                        has_operator,
                    });
                    has_operator = false;
                }
            },
            None => {
                text.push(c);
                match c {
                    '\\' => escaped = true,
                    '\'' | '"' => quote = Some(c),
                    _ if OPERATOR_CHARS.contains(&c) => has_operator = true,
                    _ => {},
                }
            },
        }
    }

    if quote.is_some() || escaped {
        return None;
    }

    if !text.is_empty() {
        words.push(RawWord { text, has_operator });
    }

    Some(words)
}

#[cfg(test)]
mod tests {
    use super::merge_candidates;
    use crate::candidate::{Candidate, Risk};

    fn commands(candidates: &[Candidate]) -> Vec<&str> {
        candidates.iter().map(Candidate::command).collect()
    }

    #[test]
    fn given_equivalent_commands_when_merged_then_one_is_kept() {
        let cases = [
            ("ls -la", "ls -al"),
            ("ls -la", "ls -l -a"),
            ("ls  -la   ~/code", "ls -la ~/code"),
            ("cat 'a b'", "cat \"a b\""),
            ("grep --count -i foo f", "grep -i --count foo f"),
            ("ls -la", "ls -la"),
            ("ls -la | wc -l", "ls -al  |  wc -l"),
        ];

        for (first, second) in cases {
            let merged = merge_candidates(vec![
                Candidate::new(first, "first", Risk::Read),
                Candidate::new(second, "second", Risk::Read),
            ]);
            assert_eq!(commands(&merged), vec![first], "{first} / {second}");
        }
    }

    #[test]
    fn given_different_commands_when_merged_then_both_are_kept() {
        let cases = [
            ("ls -la", "ls -lah"),
            ("cp a b", "cp b a"),
            ("head -n 5 f", "head f -n 5"),
            ("echo \"$HOME\"", "echo '$HOME'"),
            ("ls *.rs", "ls '*.rs'"),
            ("rm -- -f", "rm -f"),
            ("docker restart web", "docker stop web"),
            ("ls -l | wc -c", "ls -c | wc -l"),
            ("ls -la | wc -l", "ls -la ; wc -l"),
            ("ls|wc -l", "ls|wc"),
            ("echo 'a  b'", "echo 'a b'"),
        ];

        for (first, second) in cases {
            let merged = merge_candidates(vec![
                Candidate::new(first, "first", Risk::Read),
                Candidate::new(second, "second", Risk::Read),
            ]);
            assert_eq!(
                commands(&merged),
                vec![first, second],
                "{first} / {second}"
            );
        }
    }

    #[test]
    fn given_merged_commands_then_longest_description_and_highest_risk_win() {
        let merged = merge_candidates(vec![
            Candidate::new("docker ps -a", "List", Risk::Read),
            Candidate::new("docker restart web", "Restart web", Risk::Write),
            Candidate::new("docker ps  -a", "List all containers", Risk::Write),
        ]);

        assert_eq!(
            merged,
            vec![
                Candidate::new(
                    "docker ps -a",
                    "List all containers",
                    Risk::Write
                ),
                Candidate::new(
                    "docker restart web",
                    "Restart web",
                    Risk::Write
                ),
            ]
        );
    }
}
