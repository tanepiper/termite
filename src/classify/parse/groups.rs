//! Brackets that group commands and don't belong to one: `( … )`,
//! `{ …; }`, `name() { … }` and the patterns of `case`. The commands inside
//! are read as commands where they stand.

use super::Scanner;

impl Scanner {
    /// `name()` starts a function, `( … )` at the start of a stage is a
    /// group, and anywhere else the text inside is an argument nobody can
    /// read before it runs.
    pub(super) fn open_paren(&mut self) {
        self.parsed.doubt = true;
        let before = self.stage.text.trim();
        if self.peek(0) == Some(')') && is_name(before) {
            // The body that follows is read as commands, so the function
            // is rated where it is written, whatever it is called.
            self.at += 1;
            self.stage.text.clear();
        } else if before.is_empty() {
            let mut code = String::new();
            if !self.copy_until_close(&mut code) {
                self.parsed.broken = true;
            }
            self.parsed.inner.push(code);
        } else {
            self.inner_until_close();
        }
    }

    /// A `)` with no `(` ends a pattern of `case`. The pattern is matched
    /// against text and never run, so it is left out.
    pub(super) fn close_paren(&mut self) {
        self.parsed.doubt = true;
        let text = self.stage.text.trim_start();
        if self.in_case || text.starts_with("case ") {
            self.in_case = true;
            self.stage.text.clear();
        }
    }

    /// `{}` belongs to `find`, `{ ls; }` is a group, and the rest expands.
    pub(super) fn open_brace(&mut self) {
        match self.peek(0) {
            Some('}') => {
                self.stage.text.push_str("{}");
                self.at += 1;
            },
            None => self.parsed.doubt = true,
            Some(c) if c.is_whitespace() => self.parsed.doubt = true,
            _ => {
                self.parsed.doubt = true;
                self.stage.text.push('{');
            },
        }
    }

    pub(super) fn close_brace(&mut self) {
        if self.stage.text.trim().is_empty() {
            self.parsed.doubt = true;
        } else {
            self.stage.text.push('}');
        }
    }
}

fn is_name(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
