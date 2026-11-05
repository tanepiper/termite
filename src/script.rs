//! Reads a script or a stream and rates each command in it, as the lines
//! arrive. It only reads text: nothing here runs anything, and no model is
//! involved. Memory stays bounded, however long the input. A clean result
//! means no known hazard was found. It doesn't prove the script is safe,
//! because a script can build what it does while it runs.

use std::path::Path;

use crate::command::is_hidden;
use crate::hazard::{Severity, hazards};
use crate::policy::read_only_in;
use crate::rating::{Rating, warnings};

/// The most text one command may hold. Anything longer is risky, because it
/// can't be checked whole.
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;

const CANNOT_CHECK: &str = "builds what it runs, so it can't be checked";
const HIDDEN_TEXT: &str = "hides characters you can't see";
const TOO_LONG: &str = "too long to check";

/// Lines that only give a script its shape.
const SHAPE_ONLY: [&str; 11] = [
    "fi", "done", "esac", "else", "then", "do", "{", "}", ";;", "(", ")",
];

/// What came in, ready to be rated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The line it starts on, from 1.
    pub line: usize,
    pub text: String,
    /// A line of a heredoc: text, which may still be code in disguise.
    pub heredoc: bool,
    /// Cut short because it was over [`MAX_COMMAND_BYTES`].
    pub oversized: bool,
}

/// What termite makes of one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub command: String,
    pub rating: Rating,
    pub severity: Option<Severity>,
    pub warnings: Vec<&'static str>,
}

/// Turns lines into commands, joining what the shell would join. Feed it each
/// line, then call [`Splitter::finish`] at the end.
#[derive(Debug, Default)]
pub struct Splitter {
    line: usize,
    pending: String,
    start: usize,
    quote: Option<char>,
    heredocs: Vec<String>,
}

impl Splitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one line, without its line ending. Bytes that aren't valid text
    /// are kept as a replacement character, which counts as hidden.
    pub fn push(&mut self, bytes: &[u8]) -> Option<Statement> {
        self.line += 1;
        let text = String::from_utf8_lossy(bytes);
        let text = text.strip_suffix('\r').unwrap_or(&text);

        if let Some(end) = self.heredocs.first() {
            if text.trim_start_matches('\t') == end {
                self.heredocs.remove(0);
                return None;
            }
            return (!text.trim().is_empty()).then(|| Statement {
                line: self.line,
                text: text.to_owned(),
                heredoc: true,
                oversized: false,
            });
        }

        if self.pending.is_empty() {
            if is_comment_or_blank(text) {
                return None;
            }
            self.start = self.line;
        }

        let scanned = scan(text, self.quote);
        self.quote = scanned.quote;
        self.heredocs.extend(scanned.heredocs);
        let text = if scanned.continues {
            text.strip_suffix('\\').unwrap_or(text)
        } else {
            text
        };
        self.pending.push_str(text);

        if self.pending.len() > MAX_COMMAND_BYTES {
            return Some(self.flush(true));
        }
        if scanned.continues {
            return None;
        }
        if self.quote.is_some() {
            self.pending.push(' ');
            return None;
        }
        Some(self.flush(false))
    }

    /// One piece of a line too long to hold: rate what's there, then start
    /// clean, so the rest of the input is still read properly.
    pub fn push_oversized(&mut self, bytes: &[u8]) -> Statement {
        self.line += 1;
        self.pending.push_str(&String::from_utf8_lossy(bytes));
        self.start = self.line;
        self.flush(true)
    }

    /// Whatever is left at the end of the input.
    pub fn finish(&mut self) -> Option<Statement> {
        (!self.pending.trim().is_empty()).then(|| self.flush(false))
    }

    fn flush(&mut self, oversized: bool) -> Statement {
        self.quote = None;
        if oversized {
            self.heredocs.clear();
        }
        Statement {
            line: self.start,
            text: std::mem::take(&mut self.pending),
            heredoc: false,
            oversized,
        }
    }
}

fn is_comment_or_blank(line: &str) -> bool {
    let line = line.trim_start();
    line.is_empty() || line.starts_with('#')
}

struct Scanned {
    quote: Option<char>,
    continues: bool,
    heredocs: Vec<String>,
}

/// Follow the quotes along a line. A `#` starts a comment only at the start
/// of a word and outside quotes.
fn scan(line: &str, mut quote: Option<char>) -> Scanned {
    let chars: Vec<char> = line.chars().collect();
    let mut heredocs = Vec::new();
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('"') | None, '\\') => {
                if index + 1 == chars.len() {
                    return Scanned {
                        quote,
                        continues: true,
                        heredocs,
                    };
                }
                index += 1;
            },
            (None, '\'' | '"') => quote = Some(c),
            (None, '#') if index == 0 || chars[index - 1].is_whitespace() => {
                break;
            },
            (None, '<') if chars[index..].starts_with(&['<', '<']) => {
                if chars.get(index + 2) == Some(&'<') {
                    index += 2;
                } else if let Some(end) = heredoc_end(&chars[index + 2..]) {
                    heredocs.push(end);
                    index += 1;
                }
            },
            _ => {},
        }
        index += 1;
    }
    Scanned {
        quote,
        continues: false,
        heredocs,
    }
}

/// The word that ends a heredoc, from the text after `<<`.
fn heredoc_end(after: &[char]) -> Option<String> {
    let text: String = after.iter().collect();
    let text = text.strip_prefix('-').unwrap_or(&text).trim_start();
    let word: String = text
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!word.is_empty()).then_some(word)
}

/// Rate one command as it would run in `cwd`. `None` means there's nothing
/// to rate: a line that only gives the script its shape.
pub fn assess(
    statement: &Statement,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<Finding> {
    let text = statement.text.trim();
    if SHAPE_ONLY.contains(&text) {
        return None;
    }

    // Quotes and backslashes don't change what the shell runs: `r\m` is `rm`.
    let plain: String = text
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .collect();
    let mut found = hazards(text);
    found.extend(hazards(&plain));
    let severity = found.iter().map(|hazard| hazard.severity).max();

    let mut said = warnings(text);
    said.extend(warnings(&plain));
    if statement.oversized {
        said.push(TOO_LONG);
    }
    if text.chars().any(hidden) {
        said.push(HIDDEN_TEXT);
    }
    if builds_what_it_runs(text) {
        said.push(CANNOT_CHECK);
    }
    let mut seen = Vec::new();
    said.retain(|warning| {
        let new = !seen.contains(warning);
        seen.push(*warning);
        new
    });

    let rating = if !said.is_empty() {
        Rating::Risky
    } else if !statement.heredoc && read_only_in(text, cwd, home).is_err() {
        Rating::Changes
    } else {
        Rating::ReadsOnly
    };
    Some(Finding {
        line: statement.line,
        command: statement.text.clone(),
        rating,
        severity: severity.filter(|level| *level >= Severity::Medium),
        warnings: said,
    })
}

/// Tabs are normal in scripts. The replacement character marks bytes that
/// weren't text.
fn hidden(c: char) -> bool {
    c != '\t' && (is_hidden(c) || c == char::REPLACEMENT_CHARACTER)
}

/// Words that run the word after them.
const RUNS_NEXT: [&str; 14] = [
    "if", "elif", "while", "until", "then", "do", "else", "!", "time", "exec",
    "command", "builtin", "sudo", "nohup",
];

/// One simple command inside a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandStart<'a>(&'a str);

impl<'a> CommandStart<'a> {
    pub(crate) fn text(self) -> &'a str {
        self.0
    }

    pub(crate) fn first_word(self) -> &'a str {
        self.0.split_whitespace().next().unwrap_or("")
    }
}

/// The simple commands in a line, split where the shell would split them:
/// not inside quotes, `[[ ]]` or `(( ))`.
pub(crate) fn command_starts(text: &str) -> Vec<CommandStart<'_>> {
    let mut starts = Vec::new();
    let mut quote = None;
    let mut test_depth = 0_usize;
    let mut from = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let next = chars.peek().map(|(_, c)| *c);
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some('"'), '\\') | (None, '\\') => {
                chars.next();
            },
            (Some(_), _) => {},
            (None, '\'' | '"') => quote = Some(c),
            (None, '[') if next == Some('[') => test_depth += 1,
            (None, ']') if next == Some(']') => {
                test_depth = test_depth.saturating_sub(1);
            },
            (None, '(') if next == Some('(') => test_depth += 1,
            (None, ')') if next == Some(')') => {
                test_depth = test_depth.saturating_sub(1);
            },
            (None, ';' | '|' | '&' | '{' | '(') if test_depth == 0 => {
                push_start(&mut starts, &text[from..index]);
                from = index + c.len_utf8();
            },
            _ => {},
        }
    }
    push_start(&mut starts, &text[from..]);
    starts
}

fn push_start<'a>(starts: &mut Vec<CommandStart<'a>>, part: &'a str) {
    let mut part = part.trim_start();
    loop {
        let word = part.split_whitespace().next().unwrap_or("");
        if !RUNS_NEXT.contains(&word) {
            break;
        }
        part = part[word.len()..].trim_start();
    }
    if !part.is_empty() {
        starts.push(CommandStart(part));
    }
}

/// Is the program itself worked out while the script runs? `x=$(date)` is
/// fine, but `$x -rf ~` or `eval "$y"` can be anything.
fn builds_what_it_runs(text: &str) -> bool {
    text.contains("$'")
        || command_starts(text).iter().any(|start| {
            let program = start.first_word().trim_start_matches(['"', '\'']);
            program.starts_with('$')
                || program.starts_with('`')
                || program == "eval"
                || runs_inline_code(start.text())
        })
}

/// Code handed to another language on the command line. termite only reads
/// shell, so it can't tell what this does.
fn runs_inline_code(command: &str) -> bool {
    let mut words = command.split_whitespace();
    let program = words.next().unwrap_or("");
    let program = program.rsplit('/').next().unwrap_or(program);
    let interpreter = ["python", "perl", "ruby", "node", "php", "lua"]
        .iter()
        .any(|name| program.starts_with(name));
    interpreter && words.any(|word| matches!(word, "-c" | "-e" | "-r"))
}

/// The commands in a whole text.
pub(crate) fn statements(text: &str) -> Vec<Statement> {
    let mut splitter = Splitter::new();
    let mut found: Vec<Statement> = text
        .lines()
        .filter_map(|line| splitter.push(line.as_bytes()))
        .collect();
    found.extend(splitter.finish());
    found
}

/// Rate every command in a whole text.
pub(crate) fn findings(
    text: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Vec<Finding> {
    statements(text)
        .iter()
        .filter_map(|statement| assess(statement, cwd, home))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(text: &str) -> Vec<Finding> {
        statements(text)
            .iter()
            .filter_map(|s| assess(s, Path::new("/tmp/project"), None))
            .collect()
    }

    fn worst(text: &str) -> Rating {
        let ranked = |rating: &Rating| match rating {
            Rating::ReadsOnly => 0,
            Rating::Changes => 1,
            Rating::Risky => 2,
        };
        findings(text)
            .iter()
            .map(|finding| finding.rating)
            .max_by_key(ranked)
            .unwrap_or(Rating::ReadsOnly)
    }

    #[test]
    fn given_a_plain_script_then_each_command_gets_its_line() {
        let found = statements("#!/bin/sh\n# note\n\nls\n\necho hi\n");

        let lines: Vec<_> = found.iter().map(|s| (s.line, &*s.text)).collect();
        assert_eq!(lines, [(4, "ls"), (6, "echo hi")]);
    }

    #[test]
    fn given_commands_of_each_kind_then_they_are_rated() {
        assert_eq!(worst("ls"), Rating::ReadsOnly);
        assert_eq!(worst("mkdir build"), Rating::Changes);
        assert_eq!(worst("ls\nrm -rf ~\nls"), Rating::Risky);
    }

    #[test]
    fn given_a_commented_out_hazard_then_it_is_not_a_command() {
        assert_eq!(worst("# rm -rf ~\nls"), Rating::ReadsOnly);
    }

    #[test]
    fn given_a_hazard_split_over_lines_then_it_is_still_found() {
        assert_eq!(worst("rm -rf \\\n  ~"), Rating::Risky);
        assert_eq!(worst("bash -c 'echo hi\nrm -rf ~'"), Rating::Risky);
    }

    #[test]
    fn given_a_joined_command_then_it_keeps_its_first_line() {
        let found = statements("ls\nrm -rf \\\n  build\nls");

        assert_eq!(found[1].line, 2);
        assert_eq!(found[2].line, 4);
    }

    #[test]
    fn given_a_quote_in_a_comment_then_the_next_line_is_its_own_command() {
        let found = statements("ls # don't\nrm -rf ~");

        assert_eq!(found.len(), 2);
    }

    #[test]
    fn given_disguised_commands_then_they_are_still_found() {
        for script in [
            "r\\m -rf ~",
            "\"r\"m -rf ~",
            "'rm' -rf ~",
            "rm -rf \"$HOME\"",
        ] {
            assert_eq!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_code_built_while_it_runs_then_it_is_risky() {
        for script in [
            "x=rm\n$x -rf ~",
            "\"$tool\" --go",
            "eval \"$y\"",
            "if true; then $cmd; fi",
            "`get-command`",
            "perl -e 'unlink glob \"*\"'",
            "/usr/bin/python3 -c 'print(1)'",
            "$(get-command) --go",
            "echo $'\\x72\\x6d'",
        ] {
            assert_eq!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_an_ordinary_variable_then_it_is_not_risky() {
        for script in [
            "x=$(date)\necho \"$x\"",
            "[[ -n $a && $b == c ]] && ls",
            "(( $a || $b ))",
            "printf '%s' 'a | $b'",
            "main \"$@\"",
        ] {
            assert_ne!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_a_wrapper_then_the_program_after_it_is_what_counts() {
        for script in ["sudo \"$x\" go", "exec $x", "ls && $x", "ls; { $x; }"] {
            assert_eq!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_code_in_a_heredoc_then_it_is_found() {
        assert_eq!(worst("bash <<EOF\nrm -rf ~\nEOF\nls"), Rating::Risky);
        assert_eq!(worst("bash <<-'END'\n\trm -rf ~\n\tEND"), Rating::Risky);
    }

    #[test]
    fn given_harmless_heredoc_text_then_it_stays_harmless() {
        let found = findings("cat <<EOF\nhello there\nEOF\nls");

        assert_eq!(found.len(), 3);
        assert_eq!(found[1].rating, Rating::ReadsOnly);
        assert_eq!(found[2].command, "ls");
    }

    #[test]
    fn given_a_here_string_then_no_heredoc_starts() {
        let found = statements("grep x <<< hello\nls");

        assert!(found.iter().all(|s| !s.heredoc));
    }

    #[test]
    fn given_hidden_characters_then_it_is_risky() {
        assert_eq!(worst("ls\u{1b}[2K"), Rating::Risky);
        assert_eq!(worst("ls\u{202e}"), Rating::Risky);
        assert_eq!(worst("\tls"), Rating::ReadsOnly);
    }

    #[test]
    fn given_bytes_that_are_not_text_then_it_is_risky() {
        let mut splitter = Splitter::new();
        let statement = splitter.push(&[b'l', b's', 0xff]).expect("command");

        let finding =
            assess(&statement, Path::new("/tmp"), None).expect("finding");

        assert_eq!(finding.rating, Rating::Risky);
    }

    #[test]
    fn given_an_unclosed_quote_then_nothing_after_it_escapes_the_check() {
        assert_eq!(worst("echo 'hi\nrm -rf ~\nls"), Rating::Risky);
    }

    #[test]
    fn given_an_endless_quote_then_memory_stays_bounded_and_reading_goes_on() {
        let mut splitter = Splitter::new();
        splitter.push(b"echo '");
        let long = "x".repeat(1024);
        let mut oversized = 0;
        for _ in 0..200 {
            if let Some(statement) = splitter.push(long.as_bytes()) {
                assert!(statement.text.len() <= MAX_COMMAND_BYTES + 2048);
                oversized += usize::from(statement.oversized);
            }
        }
        assert!(oversized > 0);

        let next = splitter.push(b"rm -rf ~").expect("still reading");

        assert_eq!(next.text, "rm -rf ~");
    }

    #[test]
    fn given_an_oversized_line_then_it_is_risky() {
        let mut splitter = Splitter::new();
        let statement = splitter.push_oversized(b"ls");

        let finding =
            assess(&statement, Path::new("/tmp"), None).expect("finding");

        assert_eq!(finding.rating, Rating::Risky);
        assert_eq!(finding.warnings, [TOO_LONG]);
    }

    #[test]
    fn given_lines_that_only_shape_the_script_then_they_are_not_counted() {
        let found = findings("if ls; then\n  ls\nfi");

        assert_eq!(found.len(), 2);
    }

    #[test]
    fn given_windows_line_endings_then_they_do_not_count_as_hidden() {
        assert_eq!(worst("ls\r\nls\r"), Rating::ReadsOnly);
    }

    #[test]
    fn given_a_severe_command_then_the_severity_is_reported() {
        let found = findings("rm -rf ~");

        assert_eq!(found[0].severity, Some(Severity::Critical));
        assert!(found[0].warnings.contains(&"wrecks the machine"));
    }
}
