//! Here-documents, and the code inside `$(…)`. A `)` or a quote inside the
//! text of a here-document is text, so it never closes anything.

use super::{Heredoc, Scanner, Unread};

impl Scanner {
    /// `$((…))` is a sum, not a command. Its value isn't known, so a `$`
    /// is left where it was. One that never closes can't be followed.
    pub(super) fn arithmetic(&mut self) {
        let mut depth = 0_usize;
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {},
            }
            if depth == 0 {
                self.stage.text.push('$');
                return;
            }
        }
        self.parsed.broken = true;
    }

    /// Copy code up to the `)` that closes it. A `)` inside quotes or inside
    /// a here-document is text, so it closes nothing. False if it never
    /// closes.
    pub(super) fn copy_until_close(&mut self, code: &mut String) -> bool {
        let mut ends: Vec<String> = Vec::new();
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                ')' => return true,
                '(' => {
                    code.push(c);
                    if !self.copy_until_close(code) {
                        return false;
                    }
                    code.push(')');
                },
                '\\' => {
                    code.push(c);
                    code.extend(self.peek(0));
                    self.at += 1;
                },
                '\'' => {
                    code.push(c);
                    if !self.copy_single_quoted(code) {
                        return false;
                    }
                },
                '"' => {
                    code.push(c);
                    if !self.copy_double_quoted(code) {
                        return false;
                    }
                },
                '<' if self.peek(0) == Some('<') => {
                    code.push_str("<<");
                    self.at += 1;
                    if self.peek(0) == Some('<') {
                        code.push('<');
                        self.at += 1;
                    } else {
                        ends.extend(self.end_word_ahead().map(|(end, _)| end));
                    }
                },
                '\n' => {
                    code.push(c);
                    for end in ends.drain(..) {
                        let body = self.read_body(&end);
                        code.push_str(&body);
                        code.push_str(&end);
                        code.push('\n');
                    }
                },
                _ => code.push(c),
            }
        }
        false
    }

    fn copy_single_quoted(&mut self, code: &mut String) -> bool {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            code.push(c);
            if c == '\'' {
                return true;
            }
        }
        false
    }

    fn copy_double_quoted(&mut self, code: &mut String) -> bool {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            code.push(c);
            match c {
                '"' => return true,
                '\\' => {
                    code.extend(self.peek(0));
                    self.at += 1;
                },
                '$' if self.peek(0) == Some('(') => {
                    self.at += 1;
                    code.push('(');
                    if !self.copy_until_close(code) {
                        return false;
                    }
                    code.push(')');
                },
                _ => {},
            }
        }
        false
    }

    /// The word that ends a here-document, read from just after `<<` without
    /// moving on, and whether the shell expands the text: it does unless the
    /// word has a quote or a backslash.
    fn end_word_ahead(&self) -> Option<(String, bool)> {
        let mut at = self.at;
        if self.chars.get(at) == Some(&'-') {
            at += 1;
        }
        while self.chars.get(at).is_some_and(|c| *c == ' ' || *c == '\t') {
            at += 1;
        }
        let raw: String = self.chars[at.min(self.chars.len())..]
            .iter()
            .take_while(|c| !c.is_whitespace() && !";&|<>()".contains(**c))
            .collect();
        let quoted = raw.contains(['\'', '"', '\\']);
        let end: String = raw
            .chars()
            .filter(|c| !matches!(c, '\'' | '"' | '\\'))
            .collect();
        (!end.is_empty()).then_some((end, !quoted))
    }

    /// The lines after this one are the text of each here-document that
    /// opened on it. Commands the shell runs inside that text are handed
    /// back like any other inner code.
    pub(super) fn read_bodies(&mut self) {
        for unread in std::mem::take(&mut self.unread) {
            let end = self.parsed.bodies[unread.place].end.clone();
            let body = self.read_body(&end);
            if unread.expands {
                self.parsed.inner.extend(substitutions(&body));
            }
            self.parsed.bodies[unread.place].body = body;
        }
    }

    fn read_body(&mut self, end: &str) -> String {
        let from = self.at;
        let body = read_body(&self.chars, &mut self.at, end);
        self.cuts.push((from, from + body.chars().count()));
        body
    }

    /// `<<END`: the text itself starts on the next line. With no end word it
    /// can't be followed.
    pub(super) fn open_heredoc(&mut self) {
        let Some((end, expands)) = self.end_word_ahead() else {
            self.parsed.doubt = true;
            return;
        };
        self.word();
        let place = self.parsed.bodies.len();
        self.parsed.bodies.push(Heredoc {
            body: String::new(),
            end,
        });
        self.stage.heredocs.push(place);
        self.unread.push(Unread { place, expands });
    }
}

/// The lines from `at` up to the line that is only `end`, moving `at` past
/// that line. With no such line the text runs to the end, as in a shell.
fn read_body(chars: &[char], at: &mut usize, end: &str) -> String {
    let mut body = String::new();
    while *at < chars.len() {
        let rest = &chars[*at..];
        let length = rest.iter().position(|c| *c == '\n').unwrap_or(rest.len());
        let line: String = rest[..length].iter().collect();
        *at += (length + 1).min(rest.len());
        if line.trim_start_matches('\t').trim_end_matches('\r') == end {
            break;
        }
        body.push_str(&line);
        body.push('\n');
    }
    body
}

/// The commands inside `$(…)` and backticks in text the shell expands.
fn substitutions(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 1,
            '`' => {
                let rest = &chars[at + 1..];
                let length =
                    rest.iter().position(|c| *c == '`').unwrap_or(rest.len());
                found.push(rest[..length].iter().collect());
                at += length + 1;
            },
            '$' if chars.get(at + 1) == Some(&'(') => {
                let mut depth = 1;
                let mut code = String::new();
                at += 2;
                while at < chars.len() && depth > 0 {
                    match chars[at] {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {},
                    }
                    if depth > 0 {
                        code.push(chars[at]);
                    }
                    at += 1;
                }
                found.push(code);
                continue;
            },
            _ => {},
        }
        at += 1;
    }
    found
}
