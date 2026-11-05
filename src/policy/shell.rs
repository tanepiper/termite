//! Finds shell features that are never allowed unguarded, and splits a
//! command into the stages of a pipe.

use super::PolicyRefusal;
use crate::command::CommandRejected;

/// Split a command at unquoted `|` into the words of each stage, refusing
/// every other operator, redirect, substitution, history expansion and
/// escape-code quoting along the way.
pub(super) fn pipeline_stages(
    command: &str,
) -> Result<Vec<Vec<String>>, PolicyRefusal> {
    let chars: Vec<(usize, char)> = command.char_indices().collect();
    let mut stage_texts = Vec::new();
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
                    stage_texts.push(&command[stage_start..position]);
                    stage_start = position + 1;
                },
                _ => {},
            },
        }

        index += 1;
    }
    stage_texts.push(&command[stage_start..]);

    stage_texts.into_iter().map(stage_words).collect()
}

fn stage_words(text: &str) -> Result<Vec<String>, PolicyRefusal> {
    let words = shell_words::split(text)
        .map_err(|_| PolicyRefusal::Unusable(CommandRejected::Unparseable))?;

    if words.is_empty() {
        return Err(PolicyRefusal::Unusable(CommandRejected::Empty));
    }

    Ok(words)
}
