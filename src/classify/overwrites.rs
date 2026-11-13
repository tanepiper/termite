//! Writing over a file that exists loses what it held, the same as deleting
//! it. A write to a new file loses nothing, so the file is looked up.

use std::path::PathBuf;

use super::Level;
use super::paths::is_system;
use super::programs::targets;
use super::walk::Walk;

const OVERWRITES: &str = "replaces a file that already exists";
const STARTS: &str = "leaves something behind that survives a restart";
const TOOL_SETTING: &str = "changes which program a tool runs";

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
        let mut exists = false;
        for word in words {
            let paths = self.place.resolve(word, base.as_deref());
            exists |= paths.iter().flatten().any(|path| path.is_file());
            self.starts_with_the_shell(&paths);
        }
        if exists {
            self.raise(Level::ContainedDestructive, OVERWRITES);
        }
    }

    /// A write to a file that a shell or git runs by itself: what is
    /// written runs later, when nobody is looking.
    pub(super) fn starts_with_the_shell(&mut self, paths: &[Option<PathBuf>]) {
        let later = paths
            .iter()
            .flatten()
            .any(|path| self.place.runs_later(path));
        if later {
            self.raise(Level::SystemDangerous, STARTS);
        }
    }

    /// What a command that changes something may reach: a system folder,
    /// a file that runs later, or a package tool's own settings.
    pub(super) fn reaches(&mut self, name: &str, arguments: &[String]) {
        let base = self.base.clone();
        let mut system = false;
        for word in targets(arguments) {
            let paths = self.place.resolve(word, base.as_deref());
            system |= paths.iter().flatten().any(|path| is_system(path));
            self.starts_with_the_shell(&paths);
        }
        if system {
            self.raise(Level::SystemDangerous, "reaches a system folder");
        }
        let sets = matches!(name, "npm" | "pnpm" | "yarn")
            && matches!(targets(arguments).as_slice(), ["config", "set", ..]);
        if sets {
            self.raise(Level::SystemDangerous, TOOL_SETTING);
        }
    }
}
