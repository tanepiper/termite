//! What a read may show. A search below a folder reads every file there, so
//! this looks for a secret file below it. It reads names only, opens no
//! file, follows no link, and gives up after a fixed number of names.

use std::fs;
use std::path::Path;

use super::paths::is_secret;
use super::programs::targets;
use super::walk::Walk;

/// The most names looked at for one command. Past this the folder counts
/// as holding a secret.
const MAX_NAMES: usize = 10_000;

impl Walk<'_> {
    /// May a search below the folders this stage names read a secret file?
    /// True unless it names folders, every one is inside the project and not
    /// the project itself, and none of the names below them is a secret's.
    /// A word that names nothing on disk is the pattern, not a folder.
    pub(super) fn search_may_read_secrets(&self, arguments: &[String]) -> bool {
        let Some(base) = self.base.as_deref() else {
            return true;
        };
        let mut roots = Vec::new();
        for word in targets(arguments) {
            for path in self.place.resolve(folder_of(word), Some(base)) {
                match path {
                    Some(path) if path.exists() => roots.push(path),
                    Some(_) => {},
                    None => return true,
                }
            }
        }
        let mut left = MAX_NAMES;
        roots.is_empty()
            || roots.iter().any(|root| {
                !self.place.contained(root) || holds_secret(root, &mut left)
            })
    }
}

/// A word with a pattern in it, such as `src/*.rs`, can only match below
/// the folder written before the pattern, so that folder is searched.
fn folder_of(word: &str) -> &str {
    let Some(at) = word.find(['*', '?', '[']) else {
        return word;
    };
    word[..at].rfind('/').map_or(".", |slash| &word[..=slash])
}

/// Is a secret's name at or below `path`? Git's own folder holds objects,
/// not files a search would show, so it isn't walked.
fn holds_secret(path: &Path, left: &mut usize) -> bool {
    if *left == 0 || is_secret("", Some(path)) {
        return true;
    }
    *left -= 1;
    let is_folder = fs::symlink_metadata(path).is_ok_and(|file| file.is_dir());
    if !is_folder || path.file_name().is_some_and(|name| name == ".git") {
        return false;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return true;
    };
    entries.into_iter().any(|entry| match entry {
        Ok(entry) => holds_secret(&entry.path(), left),
        Err(_) => true,
    })
}

/// The words of a stage that were written inside quotes, with their quotes
/// taken off. The shell matches no file names in these, so `'.[0]'` is text
/// and not a pattern that could match a hidden file.
pub(super) fn quoted_words(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut quoted = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some('"') | None, '\\') => word.extend(chars.next()),
            (Some(_), _) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                quoted = true;
            },
            (None, _) if c.is_whitespace() => {
                if std::mem::take(&mut quoted) {
                    found.push(std::mem::take(&mut word));
                }
                word.clear();
            },
            (None, _) => word.push(c),
        }
    }
    if quoted {
        found.push(word);
    }
    found
}

/// A quoted word with the characters that would make it a pattern taken
/// out, so only its plain name is compared with the names of secrets.
pub(super) fn as_plain_name(word: &str) -> String {
    word.replace(['*', '?', '['], "_")
}
