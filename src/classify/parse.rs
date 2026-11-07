//! Cuts a command line into stages, without running or expanding anything.
//! It never fails: what it can't follow counts as doubt, which raises the
//! level. Code inside `$(…)`, backticks and `(…)` is handed back to be
//! classified on its own.

/// One program and its arguments, with its redirects taken out.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Stage {
    /// The words as written, quotes included.
    pub(super) text: String,
    /// Files written with `>` or `>>`.
    pub(super) writes: Vec<String>,
    /// The files in `writes` that `>` empties first, unlike `>>`.
    pub(super) replaces: Vec<String>,
    /// Files read with `<`.
    pub(super) reads: Vec<String>,
    /// Another stage pipes into this one.
    pub(super) piped_in: bool,
    /// Holds `$(…)`, backticks or `<(…)`.
    pub(super) has_inner: bool,
}

/// A whole command line.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Parsed {
    pub(super) stages: Vec<Stage>,
    /// Code inside substitutions and subshells.
    pub(super) inner: Vec<String>,
    /// A glob, brace expansion, variable or substitution.
    pub(super) doubt: bool,
    /// A quote, backtick or bracket that never closes.
    pub(super) broken: bool,
}

struct Scanner {
    chars: Vec<char>,
    at: usize,
    stage: Stage,
    parsed: Parsed,
}

/// Split `command` into stages.
pub(super) fn parse(command: &str) -> Parsed {
    let mut scanner = Scanner {
        chars: command.chars().collect(),
        at: 0,
        stage: Stage::default(),
        parsed: Parsed::default(),
    };
    scanner.run();
    scanner.parsed
}

impl Scanner {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn run(&mut self) {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                '\\' => self.escaped(c),
                '\'' => self.single_quoted(),
                '"' => self.double_quoted(),
                '$' => self.dollar(c),
                '`' => self.backticks(),
                '(' => self.subshell(),
                ')' => self.parsed.doubt = true,
                '{' => self.open_brace(),
                '}' => self.close_brace(),
                '*' | '?' | '[' => {
                    self.parsed.doubt = true;
                    self.stage.text.push(c);
                },
                ';' | '\n' => self.end_stage(false),
                '&' => self.ampersand(),
                '|' => self.pipe(),
                '>' => self.write_redirect(),
                '<' => self.read_redirect(),
                _ => self.stage.text.push(c),
            }
        }
        self.end_stage(false);
    }

    fn end_stage(&mut self, piped: bool) {
        let stage = std::mem::take(&mut self.stage);
        if !stage.text.trim().is_empty() || !stage.writes.is_empty() {
            self.parsed.stages.push(stage);
        }
        self.stage.piped_in = piped;
    }

    fn escaped(&mut self, c: char) {
        self.stage.text.push(c);
        if let Some(next) = self.peek(0) {
            self.stage.text.push(next);
            self.at += 1;
        }
    }

    fn single_quoted(&mut self) {
        self.stage.text.push('\'');
        while let Some(c) = self.peek(0) {
            self.at += 1;
            self.stage.text.push(c);
            if c == '\'' {
                return;
            }
        }
        self.parsed.broken = true;
    }

    fn double_quoted(&mut self) {
        self.stage.text.push('"');
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                '"' => {
                    self.stage.text.push(c);
                    return;
                },
                '\\' => self.escaped(c),
                '$' => self.dollar(c),
                '`' => self.backticks(),
                _ => self.stage.text.push(c),
            }
        }
        self.parsed.broken = true;
    }

    fn dollar(&mut self, c: char) {
        self.parsed.doubt = true;
        if self.peek(0) == Some('(') {
            self.at += 1;
            self.inner_until_close();
        } else {
            self.stage.text.push(c);
        }
    }

    /// Leave a `$` where the code was: nobody can say what goes there.
    fn mark_inner(&mut self, code: String) {
        self.stage.has_inner = true;
        self.stage.text.push('$');
        self.parsed.inner.push(code);
    }

    fn backticks(&mut self) {
        self.parsed.doubt = true;
        let mut code = String::new();
        while let Some(c) = self.peek(0) {
            self.at += 1;
            if c == '`' {
                self.mark_inner(code);
                return;
            }
            code.push(c);
        }
        self.parsed.broken = true;
        self.mark_inner(code);
    }

    fn subshell(&mut self) {
        self.parsed.doubt = true;
        self.inner_until_close();
    }

    /// Take everything up to the matching `)`, which is already opened.
    fn inner_until_close(&mut self) {
        let mut depth = 1;
        let mut code = String::new();
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {},
            }
            if depth == 0 {
                self.mark_inner(code);
                return;
            }
            code.push(c);
        }
        self.parsed.broken = true;
        self.mark_inner(code);
    }

    /// `{}` belongs to `find`, `{ ls; }` is a group, and the rest expands.
    fn open_brace(&mut self) {
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

    fn close_brace(&mut self) {
        if self.stage.text.trim().is_empty() {
            self.parsed.doubt = true;
        } else {
            self.stage.text.push('}');
        }
    }

    fn ampersand(&mut self) {
        match self.peek(0) {
            Some('>') => {
                self.at += 1;
                self.write_redirect();
            },
            Some('&') => {
                self.at += 1;
                self.end_stage(false);
            },
            _ => self.end_stage(false),
        }
    }

    fn pipe(&mut self) {
        match self.peek(0) {
            Some('|') => {
                self.at += 1;
                self.end_stage(false);
            },
            Some('&') => {
                self.at += 1;
                self.end_stage(true);
            },
            _ => self.end_stage(true),
        }
    }

    fn write_redirect(&mut self) {
        self.drop_file_number();
        let appends = self.peek(0) == Some('>');
        while matches!(self.peek(0), Some('>' | '|')) {
            self.at += 1;
        }
        if self.peek(0) == Some('&') {
            // `2>&1` joins two outputs and names no file.
            self.at += 1;
            while self.peek(0).is_some_and(|c| c.is_ascii_digit() || c == '-') {
                self.at += 1;
            }
            return;
        }
        let target = self.word();
        if target != "/dev/null" {
            if !appends {
                self.stage.replaces.push(target.clone());
            }
            self.stage.writes.push(target);
        }
    }

    fn read_redirect(&mut self) {
        match self.peek(0) {
            Some('(') => {
                self.at += 1;
                self.subshell();
            },
            Some('<') => {
                // A here-document: what it holds can't be followed.
                self.parsed.doubt = true;
                while self.peek(0) == Some('<') {
                    self.at += 1;
                }
                self.word();
            },
            _ => {
                self.drop_file_number();
                let source = self.word();
                self.stage.reads.push(source);
            },
        }
    }

    /// The `2` in `2>file` belongs to the redirect, not to the command.
    fn drop_file_number(&mut self) {
        let text = &self.stage.text;
        let number = text.trim_end_matches(|c: char| c.is_ascii_digit());
        let starts_word = number.is_empty() || number.ends_with(' ');
        if starts_word && number.len() < text.len() {
            let keep = number.len();
            self.stage.text.truncate(keep);
        }
    }

    /// The next word, with its quotes taken off.
    fn word(&mut self) -> String {
        while self.peek(0).is_some_and(char::is_whitespace) {
            self.at += 1;
        }
        let mut word = String::new();
        let mut quote = None;
        while let Some(c) = self.peek(0) {
            match quote {
                Some(open) if c == open => quote = None,
                Some(_) => word.push(c),
                None if c == '\'' || c == '"' => quote = Some(c),
                None if c.is_whitespace() || ";&|<>()".contains(c) => break,
                None => {
                    if "$*?[{`".contains(c) {
                        self.parsed.doubt = true;
                    }
                    word.push(c);
                },
            }
            self.at += 1;
        }
        self.parsed.broken |= quote.is_some();
        word
    }
}
