//! Where an argument really points: `~`, `$HOME` and braces expanded, then
//! resolved against the folder the command runs in. Nothing here follows a
//! path except [`contained`], which checks for a symlink out of the project.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::policy::names_secret;

/// The most words one brace expansion may give.
const MAX_EXPANSIONS: usize = 64;

/// System folders. A change here reaches outside any project.
const SYSTEM_FOLDERS: &[&str] = &[
    "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/library", "/opt",
    "/proc", "/root", "/sbin", "/sys", "/system", "/usr", "/var",
];

/// Inside a system folder, but free to change.
const SCRATCH_FOLDERS: &[&str] = &[
    "/dev/null",
    "/var/folders",
    "/var/tmp",
    "/dev/stdout",
    "/dev/stderr",
];

/// Files in the home folder that a shell runs when it starts.
const STARTUP_FILES: &[&str] = &[
    ".bash_login",
    ".bash_logout",
    ".bash_profile",
    ".bashrc",
    ".profile",
    ".zlogin",
    ".zlogout",
    ".zprofile",
    ".zshenv",
    ".zshrc",
];

/// Folders for files that are made to be thrown away.
const TEMPORARY_FOLDERS: &[&str] = &[
    "/tmp",
    "/private/tmp",
    "/var/tmp",
    "/var/folders",
    "/private/var/folders",
];

/// Inside a temporary folder, and not that folder itself. Deleting such a
/// file loses nothing a project keeps. The caller only asks this for a path
/// written in full, so a project that lives in a temporary folder keeps the
/// usual rule for the paths inside it.
pub(super) fn is_scratch_file(path: &Path) -> bool {
    let inside =
        |path: &Path, folder: &Path| path != folder && path.starts_with(folder);
    // A link in a temporary folder can point out of it.
    let real = path.ancestors().find_map(|one| fs::canonicalize(one).ok());
    let folders = TEMPORARY_FOLDERS.iter().map(Path::new);
    folders.clone().any(|folder| inside(path, folder))
        && real.is_some_and(|real| {
            folders
                .filter_map(|folder| fs::canonicalize(folder).ok())
                .any(|folder| inside(&real, &folder))
        })
}

/// The folders a command is judged against.
#[derive(Debug, Clone)]
pub(super) struct Place {
    pub(super) home: Option<PathBuf>,
    pub(super) project: Option<PathBuf>,
}

impl Place {
    /// The project is the git work tree, else the current folder if it
    /// isn't `~` or `/`.
    pub(super) fn find(cwd: &Path, home: Option<&Path>) -> Self {
        let tree = cwd
            .ancestors()
            .find(|folder| folder.join(".git").exists())
            .filter(|folder| !is_top(folder, home));
        let project = tree
            .or_else(|| (!is_top(cwd, home)).then_some(cwd))
            .map(Path::to_path_buf);
        Self {
            home: home.map(Path::to_path_buf),
            project,
        }
    }

    /// Every path `word` may stand for, resolved from `base`. A word that
    /// can't be resolved, because it holds a variable, gives `None`.
    pub(super) fn resolve(
        &self,
        word: &str,
        base: Option<&Path>,
    ) -> Vec<Option<PathBuf>> {
        expand_braces(word)
            .iter()
            .map(|one| self.resolve_one(one, base))
            .collect()
    }

    fn resolve_one(&self, word: &str, base: Option<&Path>) -> Option<PathBuf> {
        let home = self.home.as_deref();
        let spelled = word
            .strip_prefix("${HOME}")
            .or_else(|| word.strip_prefix("$HOME"))
            .or_else(|| word.strip_prefix('~'));
        let joined = match spelled {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => {
                home?.join(rest.trim_start_matches('/'))
            },
            _ if word.starts_with('~') || word.contains('$') => return None,
            _ if word.starts_with('/') => PathBuf::from(word),
            _ => base?.join(word),
        };
        Some(tidy(&joined))
    }

    /// Is `path` the home folder, `/`, or a folder above home?
    pub(super) fn is_wide(&self, path: &Path) -> bool {
        path == Path::new("/")
            || self.home.as_deref().is_some_and(|h| h.starts_with(path))
    }

    /// Inside the project, and not the project itself.
    pub(super) fn contained(&self, path: &Path) -> bool {
        let Some(project) = self.project.as_deref() else {
            return false;
        };
        path != project && path.starts_with(project) && !escapes(path, project)
    }

    /// The project or the home folder is at this path or below it.
    pub(super) fn holds_work(&self, path: &Path) -> bool {
        let below = |folder: &Option<PathBuf>| {
            folder.as_deref().is_some_and(|one| one.starts_with(path))
        };
        below(&self.project) || below(&self.home)
    }

    /// A file in the home folder that a shell runs when it starts.
    pub(super) fn is_startup_file(&self, path: &Path) -> bool {
        path.parent() == self.home.as_deref()
            && path.file_name().is_some_and(|name| {
                STARTUP_FILES.iter().any(|one| name == *one)
            })
    }

    /// The project itself, or inside it.
    pub(super) fn in_project(&self, path: &Path) -> bool {
        self.project.as_deref().is_some_and(|p| p == path)
            || self.contained(path)
    }
}

fn is_top(folder: &Path, home: Option<&Path>) -> bool {
    folder == Path::new("/") || home == Some(folder)
}

/// A symlink inside the project can point out of it.
fn escapes(path: &Path, project: &Path) -> bool {
    let Ok(real_project) = fs::canonicalize(project) else {
        return false;
    };
    let real = path
        .ancestors()
        .find_map(|folder| fs::canonicalize(folder).ok());
    real.is_some_and(|real| !real.starts_with(real_project))
}

/// Take out `.` and `..` without touching the disk.
fn tidy(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {},
            Component::ParentDir => {
                clean.pop();
            },
            other => clean.push(other),
        }
    }
    clean
}

/// Does this resolved path, or the word as written, name a secret?
pub(super) fn is_secret(word: &str, resolved: Option<&Path>) -> bool {
    let as_written = expand_braces(word).iter().any(|one| names_secret(one));
    as_written
        || resolved.is_some_and(|path| names_secret(&path.to_string_lossy()))
}

/// Inside a system folder, ignoring case as macOS does.
pub(super) fn is_system(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase();
    let under = |folder: &str| {
        lower == folder || lower.starts_with(&format!("{folder}/"))
    };
    SYSTEM_FOLDERS.iter().any(|folder| under(folder))
        && !SCRATCH_FOLDERS.iter().any(|folder| under(folder))
}

/// The ways a word can hold a path: `--file=.env`, `-f.env`, or itself.
pub(super) fn path_forms(word: &str) -> Vec<&str> {
    let mut forms = vec![word];
    if word.starts_with('-') {
        forms.extend(word.split_once('=').map(|(_, value)| value));
        if !word.starts_with("--") {
            forms.extend(word.get(2..).filter(|rest| !rest.is_empty()));
        }
    }
    forms
}

/// `a{b,c}d` is `abd` and `acd`. `{1..3}` and a lone brace stay as written.
pub(super) fn expand_braces(word: &str) -> Vec<String> {
    let mut done = Vec::new();
    let mut waiting = vec![word.to_owned()];
    while let Some(one) = waiting.pop() {
        if done.len() + waiting.len() >= MAX_EXPANSIONS {
            done.push(one);
            continue;
        }
        match one_brace(&one) {
            Some(expanded) => waiting.extend(expanded),
            None => done.push(one),
        }
    }
    done
}

/// Expand the innermost brace group that has a comma.
fn one_brace(word: &str) -> Option<Vec<String>> {
    let close = word
        .char_indices()
        .filter(|&(_, c)| c == '}')
        .find(|&(at, _)| {
            word[..at]
                .rfind('{')
                .is_some_and(|open| word[open..at].contains(','))
        })?
        .0;
    let open = word[..close].rfind('{')?;
    let (before, after) = (&word[..open], &word[close + 1..]);
    let choices = word[open + 1..close].split(',');
    Some(
        choices
            .map(|choice| format!("{before}{choice}{after}"))
            .collect(),
    )
}
