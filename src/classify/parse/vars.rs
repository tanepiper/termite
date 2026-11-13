//! Variables a command line sets and then uses, as in `S=/tmp/x; cat $S/a`.
//! Putting the value back where it is used lets every path check read the
//! real path. Only a plain value counts, set by a stage that always runs.
//! Anything else stays unknown, which is doubt.

use super::Scanner;

/// Characters a plain value may hold: enough for a path, and nothing the
/// shell would change or split.
const PLAIN: &str = "/._-:~+,@%=";

impl Scanner {
    /// Remember `NAME=value` if this stage is only that, and forget any
    /// variable the stage may set another way, such as `for NAME in` or
    /// `read NAME`.
    pub(super) fn note_values(&mut self, stage: &str, kept: bool) {
        let stage = stage.trim();
        let stage = stage.strip_prefix("export ").unwrap_or(stage).trim();
        self.values.retain(|(name, _)| {
            !stage
                .split(|c: char| !is_name_part(c))
                .any(|word| word == name)
        });
        let Some((name, value)) = stage.split_once('=') else {
            return;
        };
        let value = unquoted(value);
        let plain = !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || PLAIN.contains(c));
        if kept && is_name(name) && plain {
            self.values.push((name.to_owned(), value.to_owned()));
        }
    }

    /// The value of the `NAME` or `{NAME}` just after a `$`, if this line
    /// set it, and how many characters the name takes.
    pub(super) fn value_ahead(&self) -> Option<(String, usize)> {
        let rest = &self.chars[self.at.min(self.chars.len())..];
        let braced = rest.first() == Some(&'{');
        let name: String = rest
            .iter()
            .skip(usize::from(braced))
            .take_while(|c| is_name_part(**c))
            .collect();
        let length = name.len() + 2 * usize::from(braced);
        if braced && rest.get(length - 1) != Some(&'}') {
            return None;
        }
        let (_, value) =
            self.values.iter().find(|(known, _)| *known == name)?;
        Some((value.clone(), length))
    }
}

fn is_name_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(is_name_part)
}

/// The value without one pair of quotes around it.
fn unquoted(value: &str) -> &str {
    ['"', '\'']
        .iter()
        .find_map(|quote| value.strip_prefix(*quote)?.strip_suffix(*quote))
        .unwrap_or(value)
}
