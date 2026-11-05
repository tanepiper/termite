//! What every script must have, and what no script may do. These checks only
//! read the text.

use super::{MAX_SCRIPT_BYTES, PREAMBLE};
use crate::command::is_hidden;
use crate::script::{command_starts, statements};

const SHELL_STARTUP_FILES: [&str; 8] = [
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".profile",
    ".zshrc",
    ".zprofile",
    ".zshenv",
    "config.fish",
];

/// Checks that only read the text. Each problem is one line.
pub fn script_problems(script: &str) -> Vec<String> {
    if script.len() > MAX_SCRIPT_BYTES {
        return vec!["the script is over 64 KB".to_owned()];
    }
    let Some(body) = script.strip_prefix(PREAMBLE) else {
        return vec![
            "the script doesn't start with termite's first lines".to_owned(),
        ];
    };
    let mut problems = Vec::new();
    let last = body.lines().rev().find(|line| !is_comment_or_blank(line));
    if last.is_none_or(|line| line.trim() != "main \"$@\"") {
        problems.push("the last line must be main \"$@\"".to_owned());
    }
    if !body.contains("usage()") {
        problems.push("there's no usage() function".to_owned());
    }
    if !body.contains("args_parse \"$@\"") {
        problems.push("main must call args_parse \"$@\", which handles --help and --verbose".to_owned());
    }
    if body
        .chars()
        .any(|c| !matches!(c, '\n' | '\t') && is_hidden(c))
    {
        problems
            .push("the script holds control or hidden characters".to_owned());
    }
    problems.extend(banned(body));
    problems
}

fn is_comment_or_blank(line: &str) -> bool {
    let line = line.trim_start();
    line.is_empty() || line.starts_with('#')
}

/// What no script may do, wherever it does it.
fn banned(body: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for statement in statements(body).iter().filter(|s| !s.heredoc) {
        let text = &statement.text;
        let say = |what: &str| format!("line {}: {what}", statement.line);
        let starts = command_starts(text);
        if starts.iter().any(|word| word.first_word() == "eval") {
            problems.push(say("eval isn't allowed"));
        }
        if starts
            .iter()
            .any(|word| matches!(word.first_word(), "source" | "."))
        {
            problems.push(say("only termite's library may be sourced"));
        }
        if has_backtick(text) {
            problems.push(say("backticks aren't allowed; use $(...)"));
        }
        let writes = text.contains('>') || text.contains("tee ");
        if writes && SHELL_STARTUP_FILES.iter().any(|file| text.contains(file))
        {
            problems.push(say("scripts must not edit shell startup files"));
        }
        if starts.iter().any(|start| opens_to_others(start.text())) {
            problems.push(say(
                "scripts must not make files writable by other users",
            ));
        }
    }
    problems
}

fn has_backtick(text: &str) -> bool {
    let mut single = false;
    text.chars().any(|c| {
        if c == '\'' {
            single = !single;
        }
        c == '`' && !single
    })
}

/// `chmod` giving group or others the right to write.
fn opens_to_others(command: &str) -> bool {
    let mut words = command.split_whitespace();
    if words.next() != Some("chmod") {
        return false;
    }
    words
        .filter(|word| !word.starts_with('-'))
        .take(1)
        .any(|mode| {
            let mode = mode.trim_matches(['"', '\'']);
            if mode.chars().all(|c| c.is_ascii_digit()) {
                let digits = mode.as_bytes();
                return digits.iter().rev().take(2).any(|digit| digit & 2 != 0);
            }
            mode.split(',').any(|part| {
                let (who, what) = part
                    .split_once('+')
                    .or(part.split_once('='))
                    .unwrap_or(("", ""));
                what.contains('w')
                    && (who.is_empty() || who.contains(['g', 'o', 'a']))
            })
        })
}
