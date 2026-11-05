//! Finds shell features that are never allowed unguarded, and splits a
//! command into the stages of a pipe.

use super::PolicyRefusal;
use crate::command::CommandRejected;

/// The one redirect allowed: it hides error messages and writes nothing.
const DISCARD_ERRORS: &str = "2>/dev/null";

/// Split a command at unquoted `|` into the words of each stage, refusing
/// every other operator, redirect, substitution, history expansion and
/// escape-code quoting along the way. `2>/dev/null` is left out of the words,
/// and so is a leading `time`, so the checks see the real program.
pub(super) fn pipeline_stages(
    command: &str,
) -> Result<Vec<Vec<String>>, PolicyRefusal> {
    let chars: Vec<(usize, char)> = command.char_indices().collect();
    let mut stage_texts = Vec::new();
    let mut stage_text = String::new();
    let mut stage_start = 0;
    let mut quote: Option<char> = None;
    let mut index = 0;

    while index < chars.len() {
        let (position, c) = chars[index];
        let next = chars.get(index + 1).map(|&(_, next)| next);

        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                }
            },
            Some(_) => match c {
                '"' => quote = None,
                '\\' => index += 1,
                '`' => return Err(PolicyRefusal::RunsBuiltCode),
                '$' if next == Some('(') => {
                    return Err(PolicyRefusal::RunsBuiltCode);
                },
                '!' => return Err(PolicyRefusal::UsesHistory),
                _ => {},
            },
            None => match c {
                '\\' => index += 1,
                '\'' | '"' => quote = Some(c),
                '2' if discards_errors(command, position) => {
                    stage_text.push_str(&command[stage_start..position]);
                    stage_start = position + DISCARD_ERRORS.len();
                    index += DISCARD_ERRORS.len() - 1;
                },
                '$' if matches!(next, Some('\'' | '"')) => {
                    return Err(PolicyRefusal::EncodedText);
                },
                '`' | '(' | ')' => return Err(PolicyRefusal::RunsBuiltCode),
                // An empty `{}` (as in `find -exec … {} +`) is not expanded.
                '{' if next != Some('}') => {
                    return Err(PolicyRefusal::ExpandsBraces);
                },
                '!' => return Err(PolicyRefusal::UsesHistory),
                ';' | '&' => return Err(PolicyRefusal::ChainsCommands),
                '>' | '<' => return Err(PolicyRefusal::Redirects),
                '|' if matches!(next, Some('|' | '&')) => {
                    return Err(PolicyRefusal::ChainsCommands);
                },
                '|' => {
                    stage_text.push_str(&command[stage_start..position]);
                    stage_texts.push(std::mem::take(&mut stage_text));
                    stage_start = position + 1;
                },
                _ => {},
            },
        }

        index += 1;
    }
    stage_text.push_str(&command[stage_start..]);
    stage_texts.push(stage_text);

    stage_texts.iter().map(|text| stage_words(text)).collect()
}

/// Is `2>/dev/null` here, as whole words? Anything joined to it, such as
/// `2>/dev/null>out`, isn't, and is refused as a redirect.
fn discards_errors(command: &str, position: usize) -> bool {
    let starts_word = command[..position]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    let ends_word = command[position..]
        .strip_prefix(DISCARD_ERRORS)
        .is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '|')
        });
    starts_word && ends_word
}

fn stage_words(text: &str) -> Result<Vec<String>, PolicyRefusal> {
    let mut words = shell_words::split(text)
        .map_err(|_| PolicyRefusal::Unusable(CommandRejected::Unparseable))?;

    if words.is_empty() {
        return Err(PolicyRefusal::Unusable(CommandRejected::Empty));
    }

    // `time -o file ls` is left as `-o file ls`, which is no program.
    if words[0] == "time" && words.len() > 1 {
        words.remove(0);
    }

    Ok(words)
}
