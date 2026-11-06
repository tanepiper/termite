//! Finds a command that a person can't use as written: one that is cut off,
//! holds a placeholder, or names a program this machine doesn't have. It
//! only reads the text. It rates nothing, and no level depends on it, so it
//! can only stop a suggestion. What it can't follow counts as real.

use super::parse::{Parsed, Stage, parse};
use super::programs::unwrap;

/// Words that start a loop, a branch or a function: the word after one is
/// not a program, so the program check leaves the line alone.
const SHELL_SYNTAX: &[&str] = &[
    "!", "[[", "case", "coproc", "do", "done", "elif", "else", "esac", "fi",
    "for", "function", "if", "repeat", "select", "then", "until", "while", "{",
    "}",
];

/// Commands the shell runs itself, so they are not on `PATH`.
const SHELL_BUILTINS: &[&str] = &[
    ".",
    ":",
    "[",
    "alias",
    "autoload",
    "bg",
    "bind",
    "bindkey",
    "break",
    "builtin",
    "caller",
    "cd",
    "chdir",
    "command",
    "compgen",
    "complete",
    "compopt",
    "continue",
    "declare",
    "dirs",
    "disown",
    "echo",
    "emulate",
    "enable",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "functions",
    "getopts",
    "hash",
    "help",
    "history",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "noglob",
    "popd",
    "print",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "rehash",
    "return",
    "set",
    "setopt",
    "shift",
    "shopt",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "true",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "unsetopt",
    "vared",
    "wait",
    "whence",
    "where",
    "which",
    "zle",
    "zmodload",
    "zstyle",
];

/// Why a command can't be used as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotReal {
    /// The line stops part way through, such as after a `|`.
    CutOff,
    /// It still holds a placeholder, such as `<file>` or `path/to/file`.
    Placeholder(String),
    /// Its program isn't on this machine.
    NoSuchProgram(String),
}

/// Why `command` can't be used, if it can't. `installed` says whether a
/// program can be found; `None` leaves that check out, for a session on
/// another machine.
pub fn not_real(
    command: &str,
    installed: Option<&dyn Fn(&str) -> bool>,
) -> Option<NotReal> {
    let parsed = parse(command);
    if parsed.broken || is_cut_off(command) {
        return Some(NotReal::CutOff);
    }
    if let Some(found) = placeholder(command) {
        return Some(NotReal::Placeholder(found));
    }
    missing_program(command, &parsed, installed?).map(NotReal::NoSuchProgram)
}

/// The line ends on something that needs more after it. Quotes are closed
/// by now, so a last character that is an operator is not quoted.
fn is_cut_off(command: &str) -> bool {
    let line = command.trim_end();
    let escaped = line
        .chars()
        .rev()
        .skip(1)
        .take_while(|c| *c == '\\')
        .count()
        % 2
        == 1;
    let open = line.ends_with(['|', '<', '>', '$']) || line.ends_with("&&");
    open && !escaped
}

/// The command with quoted text and escaped characters blanked out.
fn unquoted(command: &str) -> String {
    let mut plain = String::with_capacity(command.len());
    let mut quote = None;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('"'), '\\') | (None, '\\') => {
                chars.next();
                plain.push_str("  ");
            },
            (Some(open), _) if open == c => {
                quote = None;
                plain.push(' ');
            },
            (Some(_), _) => plain.push(' '),
            (None, '\'' | '"') => {
                quote = Some(c);
                plain.push(' ');
            },
            (None, _) => plain.push(c),
        }
    }
    plain
}

fn placeholder(command: &str) -> Option<String> {
    let plain = unquoted(command);
    plain
        .split_whitespace()
        .find(|word| is_placeholder(word))
        .map(str::to_owned)
}

fn is_placeholder(word: &str) -> bool {
    let named = word
        .rsplit_once('<')
        .and_then(|(_, rest)| rest.strip_suffix('>'))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || "_-".contains(c))
        });
    named
        || word == "..."
        || word.contains("{{")
        || word.starts_with("path/to/")
        || word.starts_with("/path/to/")
}

/// The first program, in a line simple enough to follow, that isn't here.
fn missing_program(
    command: &str,
    parsed: &Parsed,
    installed: &dyn Fn(&str) -> bool,
) -> Option<String> {
    // `f() { …; }; f` defines its own program.
    let defines = command.contains("()");
    if defines || parsed.stages.iter().any(starts_with_syntax) {
        return None;
    }
    parsed
        .stages
        .iter()
        .filter_map(program_of)
        .find(|program| !installed(program))
}

fn words_of(stage: &Stage) -> Option<Vec<String>> {
    shell_words::split(&stage.text).ok()
}

fn starts_with_syntax(stage: &Stage) -> bool {
    words_of(stage)
        .and_then(|words| words.into_iter().next())
        .is_some_and(|word| SHELL_SYNTAX.contains(&word.as_str()))
}

/// The program a stage runs, when the text names one plainly. A path is left
/// out, because an earlier stage may create it.
fn program_of(stage: &Stage) -> Option<String> {
    let first = unwrap(words_of(stage)?).words.into_iter().next()?;
    let plain = !first.contains(['/', '$', '*', '?', '[', '~', '=']);
    (plain && !SHELL_BUILTINS.contains(&first.as_str())).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_ls_and_friends(program: &str) -> bool {
        [
            "ls", "grep", "cat", "sort", "git", "head", "wc", "find", "xargs",
        ]
        .contains(&program)
    }

    fn check(command: &str) -> Option<NotReal> {
        not_real(command, Some(&only_ls_and_friends))
    }

    #[test]
    fn given_a_line_that_stops_early_then_it_is_cut_off() {
        for command in [
            "ls |",
            "ls | ",
            "ls &&",
            "ls ||",
            "ls >",
            "cat <",
            "tmp=$",
            "echo 'open",
            "echo \"open",
            "ls $(grep a",
            "cat <file>",
            "cat <source> <target>",
            ":help<Enter>",
        ] {
            assert_eq!(check(command), Some(NotReal::CutOff), "{command}");
        }
    }

    #[test]
    fn given_a_placeholder_then_it_is_named() {
        for (command, word) in [
            ("grep <pattern> notes.txt", "<pattern>"),
            ("cat path/to/file", "path/to/file"),
            ("ls /path/to/folder", "/path/to/folder"),
            ("ls {{path}}", "{{path}}"),
            ("git commit ...", "..."),
        ] {
            let found = Some(NotReal::Placeholder(word.to_owned()));
            assert_eq!(check(command), found, "{command}");
        }
    }

    #[test]
    fn given_a_program_that_is_not_here_then_it_is_named() {
        for (command, program) in [
            ("gti status", "gti"),
            ("ls | gti", "gti"),
            ("ls && gti status", "gti"),
            ("FOO=1 gti status", "gti"),
            ("sudo gti status", "gti"),
            ("find . | xargs gti", "gti"),
        ] {
            let found = Some(NotReal::NoSuchProgram(program.to_owned()));
            assert_eq!(check(command), found, "{command}");
        }
    }

    #[test]
    fn given_real_commands_then_nothing_is_found() {
        for command in [
            "ls -la",
            "ls &",
            "ls ; cd /tmp ; ls",
            "cd /tmp && ls",
            "grep '<div>' page.html",
            "grep \"a|\" notes.txt",
            "grep -c 'b>' notes.txt",
            "grep a\\| notes.txt",
            "sort <in.txt >out.txt",
            "cat notes.txt > out.txt",
            "git log --oneline | head -5",
            "find . -name '*.rs' | xargs wc -l",
            "echo cost: \\$",
            "echo 'price in $'",
            "export FOO=1",
            "FOO=1",
            "source ./env.sh",
            "./run.sh --fast",
            "/opt/tool/bin/tool",
            "~/bin/tool",
            "$EDITOR notes.txt",
            "for f in *.txt; do gti \"$f\"; done",
            "if ls; then gti; fi",
            "f() { ls; }; f",
            "{ ls; }",
        ] {
            assert_eq!(check(command), None, "{command}");
        }
    }

    #[test]
    fn given_no_way_to_look_then_programs_are_not_checked() {
        assert_eq!(not_real("gti status", None), None);
        assert_eq!(not_real("gti |", None), Some(NotReal::CutOff));
    }
}
