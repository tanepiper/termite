//! Reads a script or a stream and rates each command in it, as the lines
//! arrive. It only reads text: nothing here runs anything, and no model is
//! involved. Memory stays bounded, however long the input. A clean result
//! means no known hazard was found. It doesn't prove the script is safe,
//! because a script can build what it does while it runs.

mod split;
mod starts;

use std::path::Path;

use crate::classify::hazard_texts;
use crate::command::is_hidden;
use crate::hazard::{Severity, hazards};
use crate::policy::read_only_in;
use crate::rating::{Rating, warnings};

pub use split::Splitter;
pub(crate) use starts::command_starts;

/// The most text one command may hold. Anything longer is risky, because it
/// can't be checked whole.
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;

pub const CANNOT_CHECK: &str = "builds what it runs, so it can't be checked";
/// Code handed to another language on the command line. termite reads
/// shell, so it can only search this code for words that reach out.
pub const OTHER_LANGUAGE: &str =
    "runs code in another language, which can't be checked";
pub const HIDDEN_TEXT: &str = "hides characters you can't see";
pub const TOO_LONG: &str = "too long to check";

/// Lines that only give a script its shape.
const SHAPE_ONLY: [&str; 11] = [
    "fi", "done", "esac", "else", "then", "do", "{", "}", ";;", "(", ")",
];

/// What came in, ready to be rated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The line it starts on, from 1.
    pub line: usize,
    /// The command, with the text of any here-document it opens.
    pub text: String,
    /// The command with the text of its here-documents taken out.
    pub outline: String,
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

    // The text of a here-document is searched only where something runs it,
    // and then line by line. Quotes and backslashes don't change what the
    // shell runs: `r\m` is `rm`.
    let texts = hazard_texts(text, cwd, home);
    let mut found = Vec::new();
    let mut said = Vec::new();
    for text in &texts {
        let plain: String = text
            .chars()
            .filter(|c| !matches!(c, '\'' | '"' | '\\'))
            .collect();
        found.extend(hazards(text));
        found.extend(hazards(&plain));
        said.extend(warnings(text));
        said.extend(warnings(&plain));
    }
    let severity = found.iter().map(|hazard| hazard.severity).max();
    if statement.oversized {
        said.push(TOO_LONG);
    }
    if text.chars().any(hidden) {
        said.push(HIDDEN_TEXT);
    }
    let command = &statement.outline;
    if builds_what_it_runs(command) {
        said.push(CANNOT_CHECK);
    } else if runs_other_language(command) {
        said.push(OTHER_LANGUAGE);
    }
    let mut seen = Vec::new();
    said.retain(|warning| {
        let new = !seen.contains(warning);
        seen.push(*warning);
        new
    });

    let rating = if !said.is_empty() {
        Rating::Risky
    } else if read_only_in(text, cwd, home).is_err() {
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

/// Tabs are normal in scripts, and a command that runs over several lines
/// holds line breaks. The replacement character marks bytes that weren't
/// text.
fn hidden(c: char) -> bool {
    !matches!(c, '\t' | '\n')
        && (is_hidden(c) || c == char::REPLACEMENT_CHARACTER)
}

/// Does `$'…'` spell characters by number, as `$'\x72\x6d'` spells `rm`? Tabs
/// and newlines hide nothing, so `$'\t'` is fine, and `'a$'` isn't one at all.
/// A statement's quotes are closed, so they can be followed here.
fn spells_by_number(text: &str) -> bool {
    let mut quote = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some('"') | None, '\\') => {
                chars.next();
            },
            (Some(_), _) => {},
            (None, '\'' | '"') => quote = Some(c),
            (None, '$') if chars.peek() == Some(&'\'') => {
                chars.next();
                if dollar_quote_hides(&mut chars) {
                    return true;
                }
            },
            _ => {},
        }
    }
    false
}

/// Read to the end of a `$'…'`. One never closed counts as hiding.
fn dollar_quote_hides(chars: &mut impl Iterator<Item = char>) -> bool {
    while let Some(c) = chars.next() {
        match c {
            '\'' => return false,
            '\\' if !chars.next().is_some_and(is_plain_escape) => return true,
            _ => {},
        }
    }
    true
}

fn is_plain_escape(c: char) -> bool {
    "abeEfnrtv\\'\"?".contains(c)
}

/// Is the program itself worked out while the script runs? `x=$(date)` is
/// fine, but `$x -rf ~` or `eval "$y"` can be anything.
fn builds_what_it_runs(text: &str) -> bool {
    spells_by_number(text)
        || command_starts(text).iter().any(|start| {
            let program = start.first_word().trim_start_matches(['"', '\'']);
            program.starts_with('$')
                || program.starts_with('`')
                || program == "eval"
        })
}

fn runs_other_language(text: &str) -> bool {
    command_starts(text)
        .iter()
        .any(|start| runs_inline_code(start.text()))
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
            "echo $'\\162\\155'",
            "echo $'\\u0072m'",
            "echo $'\\cA'",
            "echo $'\\t\\x72'",
            "echo $'never closed",
            "$'rm' -rf ~",
            "echo 'a' $'\\x72'",
            "echo $'\\'' $'\\x72'",
            "echo 'one\ntwo' $'\\x72\\x6d'",
        ] {
            assert_eq!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_an_array_then_it_is_not_taken_for_a_subshell() {
        for script in [
            "items+=(\"$item\")",
            "items=(\"$a\" \"$b\")",
            "local items=(\"$a\")",
            "declare -a items=(\"$a\")",
            "if true; then items=(\"$a\"); fi",
            "for x in a; do items+=(\"$x\"); done",
        ] {
            assert_ne!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_code_near_an_array_then_it_still_counts() {
        for script in [
            "items=($(\"$tool\" go))",
            "items=(\"$a\"); \"$tool\" go",
            "items=(a); \"${items[@]}\"",
            "echo items=(\"$tool\" go)",
            "1x=(\"$tool\" go)",
            "=(\"$tool\" go)",
            "eval items=(\"$a\")",
        ] {
            assert_eq!(worst(script), Rating::Risky, "{script}");
        }
    }

    #[test]
    fn given_plain_escapes_in_dollar_quotes_then_they_are_not_risky() {
        for script in [
            "grep 'a$' notes.txt",
            "echo \"$'\\x72'\"",
            "IFS=$'\\n\\t'",
            "printf '%s' $'\\e[1m'",
            "sep=$'\\\\x'",
        ] {
            assert_ne!(worst(script), Rating::Risky, "{script}");
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
    fn given_a_heredoc_then_its_text_stays_with_its_command() {
        let found = statements("cat <<EOF\nhello there\nEOF\nls");

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].text, "cat <<EOF\nhello there\nEOF");
        assert_eq!(found[0].outline, "cat <<EOF\nEOF");
        assert_eq!((found[1].line, &*found[1].text), (4, "ls"));
    }

    #[test]
    fn given_heredoc_text_that_nothing_runs_then_it_is_not_a_command() {
        let note =
            "cat > notes.md <<'EOF'\nNever run rm -rf / on a server.\nEOF";
        assert_eq!(worst(note), Rating::Changes);

        let message = "git commit -m \"$(cat <<'EOF'\nDon't run rm -rf ~ (ever)\n\n1) first \"point\"\nEOF\n)\"";
        assert_eq!(statements(message).len(), 1);
        assert_eq!(worst(message), Rating::Changes);
    }

    #[test]
    fn given_heredoc_text_in_another_language_then_each_line_is_searched() {
        let code = "python3 - <<'PY'\nimport os\nos.system('rm -rf ~')\nPY";
        assert_eq!(worst(code), Rating::Risky);

        let fine = "python3 - <<'PY'\nprint(sum(range(4)))\nPY";
        assert_eq!(worst(fine), Rating::Changes);
    }

    #[test]
    fn given_a_here_string_then_no_heredoc_starts() {
        let found = statements("grep x <<< hello\nls");

        assert_eq!(found.len(), 2);
    }

    #[test]
    fn given_a_shift_then_no_heredoc_starts() {
        let found = statements("echo $((1 << 4))\nls");

        assert_eq!(found.len(), 2);
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
        assert!(found[0].warnings.contains(&"destroys the system"));
    }
}
