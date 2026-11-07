//! Writing over a file that exists loses what it held, the same as deleting
//! it. A write to a new file loses nothing, so the file is looked up.

use super::Level;
use super::programs::targets;
use super::walk::Walk;

const OVERWRITES: &str = "replaces a file that already exists";

/// The files `name` would replace, as written. A program told to keep what
/// is there, or to add to it, replaces nothing.
pub(super) fn destinations<'a>(
    name: &str,
    arguments: &'a [String],
) -> Vec<&'a str> {
    let short = |letter: char| {
        arguments.iter().any(|word| {
            word.starts_with('-')
                && !word.starts_with("--")
                && word.contains(letter)
        })
    };
    let long = |text: &str| arguments.iter().any(|word| word.starts_with(text));
    let named = targets(arguments);
    match name {
        "cp" | "mv" | "install"
            if named.len() > 1
                && !short('n')
                && !short('b')
                && !long("--no-clobber")
                && !long("--backup") =>
        {
            named.last().copied().into_iter().collect()
        },
        "ln" if named.len() > 1 && (short('f') || long("--force")) => {
            named.last().copied().into_iter().collect()
        },
        "tee" if !short('a') && !long("--append") => named,
        _ => Vec::new(),
    }
}

impl Walk<'_> {
    /// Raise the level if any of `words` names a file that exists.
    pub(super) fn overwrites<'a>(
        &mut self,
        words: impl IntoIterator<Item = &'a str>,
    ) {
        let base = self.base.clone();
        let exists = words.into_iter().any(|word| {
            self.place
                .resolve(word, base.as_deref())
                .iter()
                .flatten()
                .any(|path| path.is_file())
        });
        if exists {
            self.raise(Level::ContainedDestructive, OVERWRITES);
        }
    }
}
