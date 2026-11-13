//! Turns lines into commands, joining what the shell would join. Contract:
//! pure text work with bounded memory; a command over the limit is handed
//! back cut short and marked, so it is rated as risky.

use super::{MAX_COMMAND_BYTES, Statement};

/// What a line is still inside when it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Single,
    Double,
    /// `$(`, or a `(` inside one.
    Substitution,
}

/// Turns lines into commands, joining what the shell would join: a quote or
/// a `$(` left open, a line that ends in `\`, and a here-document with its
/// text. Feed it each line, then call [`Splitter::finish`] at the end.
#[derive(Debug, Default)]
pub struct Splitter {
    line: usize,
    pending: String,
    outline: String,
    start: usize,
    open: Vec<Open>,
    /// The end words of here-documents whose text is still arriving.
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
            self.pending.push_str(text);
            if text.trim_start_matches('\t') == end {
                self.heredocs.remove(0);
                self.outline.push_str(text);
            }
            return self.joined(false);
        }

        if self.pending.is_empty() {
            if is_comment_or_blank(text) {
                return None;
            }
            self.start = self.line;
        }

        let scanned = scan(text, &mut self.open);
        self.heredocs.extend(scanned.heredocs);
        let text = if scanned.continues {
            text.strip_suffix('\\').unwrap_or(text)
        } else {
            text
        };
        self.pending.push_str(text);
        self.outline.push_str(text);
        self.joined(scanned.continues)
    }

    /// The command so far, if this line ended it. Otherwise the next line
    /// belongs to it too.
    fn joined(&mut self, continues: bool) -> Option<Statement> {
        if self.pending.len() > MAX_COMMAND_BYTES {
            return Some(self.flush(true));
        }
        if continues {
            return None;
        }
        if self.open.is_empty() && self.heredocs.is_empty() {
            return Some(self.flush(false));
        }
        self.pending.push('\n');
        if !self.outline.ends_with('\n') {
            self.outline.push('\n');
        }
        None
    }

    /// One piece of a line too long to hold: rate what's there, then start
    /// clean, so the rest of the input is still read properly.
    pub fn push_oversized(&mut self, bytes: &[u8]) -> Statement {
        self.line += 1;
        let piece = String::from_utf8_lossy(bytes);
        self.pending.push_str(&piece);
        self.outline.push_str(&piece);
        self.start = self.line;
        self.flush(true)
    }

    /// Whatever is left at the end of the input.
    pub fn finish(&mut self) -> Option<Statement> {
        (!self.pending.trim().is_empty()).then(|| self.flush(false))
    }

    fn flush(&mut self, oversized: bool) -> Statement {
        self.open.clear();
        self.heredocs.clear();
        Statement {
            line: self.start,
            text: std::mem::take(&mut self.pending),
            outline: std::mem::take(&mut self.outline),
            oversized,
        }
    }
}

fn is_comment_or_blank(line: &str) -> bool {
    let line = line.trim_start();
    line.is_empty() || line.starts_with('#')
}

struct Scanned {
    continues: bool,
    heredocs: Vec<String>,
}

/// Follow the quotes and `$(` along a line, starting inside whatever the
/// line before left open. A `#` starts a comment only at the start of a word
/// and outside quotes.
fn scan(line: &str, open: &mut Vec<Open>) -> Scanned {
    let chars: Vec<char> = line.chars().collect();
    let mut heredocs = Vec::new();
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        let next = chars.get(index + 1).copied();
        match (open.last().copied(), c) {
            (Some(Open::Single), '\'') | (Some(Open::Double), '"') => {
                open.pop();
            },
            (Some(Open::Single), _) => {},
            (_, '\\') => {
                if next.is_none() {
                    return Scanned {
                        continues: true,
                        heredocs,
                    };
                }
                index += 1;
            },
            (_, '$') if next == Some('(') => {
                open.push(Open::Substitution);
                index += 1;
            },
            (Some(Open::Double), _) => {},
            (_, '\'') => open.push(Open::Single),
            (_, '"') => open.push(Open::Double),
            (Some(Open::Substitution), '(') => open.push(Open::Substitution),
            (Some(Open::Substitution), ')') => {
                open.pop();
            },
            (_, '#') if index == 0 || chars[index - 1].is_whitespace() => {
                break;
            },
            (_, '<') if chars[index..].starts_with(&['<', '<', '<']) => {
                index += 2;
            },
            (_, '<') if next == Some('<') => {
                if let Some(end) = heredoc_end(&chars[index + 2..]) {
                    heredocs.push(end);
                    index += 1;
                }
            },
            _ => {},
        }
        index += 1;
    }
    Scanned {
        continues: false,
        heredocs,
    }
}

/// The word that ends a heredoc, from the text after `<<`. A number is far
/// more often the right side of a shift, as in `$((1 << 4))`.
fn heredoc_end(after: &[char]) -> Option<String> {
    let text: String = after.iter().collect();
    let text = text.strip_prefix('-').unwrap_or(&text).trim_start();
    let word: String = text
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let starts_a_word =
        word.chars().next().is_some_and(|c| !c.is_ascii_digit());
    starts_a_word.then_some(word)
}
