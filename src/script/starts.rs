//! Finds where each simple command in a line starts. Contract: pure text
//! work that runs nothing; what it can't follow is split, so it gets looked at.

/// Words that declare a variable, and may give it an array.
const DECLARES: [&str; 5] =
    ["local", "declare", "typeset", "readonly", "export"];

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
            (None, '(') if opens_array(&text[from..index]) => {},
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

/// Does `(` here start an array, as in `local items=(`, not a subshell? What
/// comes before it must be an assignment and nothing that could run.
fn opens_array(before: &str) -> bool {
    let mut words = before.split_whitespace();
    let assigned = words
        .next_back()
        .and_then(|word| word.strip_suffix('='))
        .map(|name| name.strip_suffix('+').unwrap_or(name))
        .is_some_and(is_variable_name);
    assigned
        && words.all(|word| {
            word.starts_with('-')
                || DECLARES.contains(&word)
                || RUNS_NEXT.contains(&word)
        })
}

fn is_variable_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
