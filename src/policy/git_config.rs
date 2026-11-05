//! Reads a repository's own git config for settings that make read-only
//! git commands run other programs: `git status` runs `core.fsmonitor`,
//! `git log` runs a pager, `git diff` runs external diff, textconv and
//! filter drivers. A repository that arrives with its `.git` folder (a
//! tarball, a shared drive) can set these. Includes are refused too,
//! because they pull in config that is not read here.
//!
//! The user's own global config is theirs to trust and is not read.

use std::fs;
use std::path::{Path, PathBuf};

/// Config files larger than this are not read, and count as unsafe.
const MAX_CONFIG_BYTES: u64 = 1 << 20;

/// What makes the repository at or above `cwd` run code, as a phrase that
/// follows "this repository's git config", or `None` when its config sets
/// nothing that runs code (or there is no repository).
pub(crate) fn runs_code(cwd: &Path) -> Option<String> {
    let git_dir = find_git_dir(cwd)?;
    let git_dir = match git_dir {
        Ok(git_dir) => git_dir,
        Err(detail) => return Some(detail),
    };

    let common_dir = match fs::read_to_string(git_dir.join("commondir")) {
        Ok(text) => git_dir.join(text.trim()),
        Err(_) => git_dir.clone(),
    };

    [common_dir.join("config"), git_dir.join("config.worktree")]
        .iter()
        .find_map(|path| check_file(path))
}

/// The `.git` folder for `cwd`: the nearest `.git` folder, or the folder a
/// `.git` file points to (worktrees, submodules). `None` outside a
/// repository; `Some(Err(..))` when a `.git` file cannot be followed.
fn find_git_dir(cwd: &Path) -> Option<Result<PathBuf, String>> {
    for folder in cwd.ancestors() {
        let dot_git = folder.join(".git");
        let Ok(metadata) = fs::symlink_metadata(&dot_git) else {
            continue;
        };
        if metadata.is_dir() {
            return Some(Ok(dot_git));
        }
        let pointer = fs::read_to_string(&dot_git).ok().and_then(|text| {
            text.lines()
                .next()
                .and_then(|line| line.strip_prefix("gitdir:"))
                .map(|path| folder.join(path.trim()))
        });
        return Some(pointer.ok_or_else(|| {
            format!("cannot be found from `{}`", dot_git.display())
        }));
    }
    None
}

/// Check one config file; a missing file is fine.
fn check_file(path: &Path) -> Option<String> {
    let Ok(metadata) = fs::metadata(path) else {
        return None;
    };
    if metadata.len() > MAX_CONFIG_BYTES {
        return Some(format!("`{}` is too large to check", path.display()));
    }
    match fs::read_to_string(path) {
        Ok(text) => check_text(&text),
        Err(_) => Some(format!("`{}` cannot be read", path.display())),
    }
}

/// Find the first setting that runs code, in git's config file syntax.
fn check_text(text: &str) -> Option<String> {
    let mut section = Section::default();

    for line in text.lines() {
        let mut line = line.trim();
        if is_blank(line) {
            continue;
        }
        if line.ends_with('\\') {
            return Some("continues a line, which is not checked".to_owned());
        }
        if let Some(rest) = line.strip_prefix('[') {
            let Some((header, after)) = rest.split_once(']') else {
                return Some("has a section it cannot read".to_owned());
            };
            section = Section::parse(header);
            // A setting may follow the header on the same line.
            line = after.trim();
            if is_blank(line) {
                continue;
            }
        }

        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        let key = key.trim().to_lowercase();
        if section.runs_code(&key, &value.trim().to_lowercase()) {
            return Some(format!(
                "sets `{}`, which runs a program",
                section.setting(&key)
            ));
        }
    }
    None
}

fn is_blank(line: &str) -> bool {
    line.is_empty() || line.starts_with(['#', ';'])
}

/// A `[name]` or `[name "subsection"]` header.
#[derive(Default)]
struct Section {
    name: String,
    has_subsection: bool,
}

impl Section {
    fn parse(header: &str) -> Self {
        let (name, rest) =
            header.split_once([' ', '"', '.']).unwrap_or((header, ""));
        Self {
            name: name.trim().to_lowercase(),
            has_subsection: !rest.trim().is_empty(),
        }
    }

    fn runs_code(&self, key: &str, value: &str) -> bool {
        let switched_on = !matches!(value, "" | "false" | "no" | "off" | "0");
        match (self.name.as_str(), self.has_subsection, key) {
            ("core", false, "fsmonitor" | "pager") | ("pager", _, _) => {
                switched_on
            },
            ("diff", false, "external")
            | ("diff", true, "textconv" | "command")
            | ("filter", true, "clean" | "smudge" | "process")
            | ("include" | "includeif", _, "path") => true,
            _ => false,
        }
    }

    fn setting(&self, key: &str) -> String {
        let name = &self.name;
        if self.has_subsection {
            format!("{name}.….{key}")
        } else {
            format!("{name}.{key}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    use super::{MAX_CONFIG_BYTES, check_text, runs_code};
    use crate::home::create_private;
    use crate::home::tests::TempFolder;
    use crate::test_support::Random;

    #[test]
    fn given_config_that_runs_code_when_checked_then_the_setting_is_named() {
        let cases = [
            ("[core]\n\tfsmonitor = ./evil", "core.fsmonitor"),
            ("[core]\n\tpager = sh -c x", "core.pager"),
            ("[CORE]\n\tPager = less", "core.pager"),
            ("[core] pager = less", "core.pager"),
            ("[pager]\n\tlog = ./evil", "pager.log"),
            ("[diff]\n\texternal = ./evil", "diff.external"),
            ("[diff \"x\"]\n\ttextconv = ./evil", "diff.….textconv"),
            ("[diff.x]\n\tcommand = ./evil", "diff.….command"),
            ("[filter \"lfs\"]\n\tclean = ./evil", "filter.….clean"),
            ("[filter \"x\"]\n\tprocess = ./evil", "filter.….process"),
            ("[include]\n\tpath = other", "include.path"),
            (
                "[includeIf \"gitdir:~/\"]\n\tpath = other",
                "includeif.….path",
            ),
        ];
        for (text, setting) in cases {
            let found = check_text(text).unwrap_or_default();
            assert!(found.contains(setting), "{text:?} gave {found:?}");
        }
    }

    /// Whatever a hostile repository writes, reading it never panics, and
    /// a real setting is never lost among the noise.
    #[test]
    fn given_random_config_then_the_setting_is_still_found() {
        let parts = [
            "[", "]", "\"", "=", " ", "\t", "\n", "#", ";", "core", "pager",
            "x", "\\", ".", "é", "\u{0}",
        ];
        let mut random = Random::new(11);
        for _ in 0..20_000 {
            let noise = random.text(&parts, 10);
            let _ = check_text(&noise);
            let text = format!(
                "[user]\n\tname = x\n[core]\n\tpager = ./evil\n# {noise}"
            );
            let tail = noise.contains('\n');
            assert!(tail || check_text(&text).is_some(), "{text:?}");
        }
    }

    /// A file with `contents`, written the way any test fixture is.
    fn put(folder: &Path, name: &str, contents: &[u8]) {
        fs::create_dir_all(folder).expect("folder");
        let mut file = create_private(folder, name).expect("file");
        file.write_all(contents).expect("written");
    }

    const EVIL: &[u8] = b"[core]\n\tfsmonitor = ./evil\n";

    #[test]
    fn given_no_repository_then_there_is_nothing_to_find() {
        let folder = TempFolder::new();
        assert_eq!(runs_code(&folder.path().join("nowhere")), None);
    }

    #[test]
    fn given_a_subfolder_then_the_repository_above_is_read() {
        let folder = TempFolder::new();
        put(&folder.path().join(".git"), "config", EVIL);

        let found = runs_code(&folder.path().join("src/deep"));

        assert!(found.is_some_and(|text| text.contains("fsmonitor")));
    }

    /// Worktrees and submodules keep their config elsewhere.
    #[test]
    fn given_a_worktree_then_the_config_it_points_to_is_read() {
        let folder = TempFolder::new();
        let main = folder.path().join("main.git");
        let worktree = main.join("worktrees/w");
        let project = folder.path().join("project");
        put(&main, "config", EVIL);
        put(&worktree, "commondir", b"../..\n");
        let pointer = format!("gitdir: {}\n", worktree.display());
        put(&project, ".git", pointer.as_bytes());

        assert!(runs_code(&project).is_some());
    }

    #[test]
    fn given_a_worktree_config_then_it_is_read_too() {
        let folder = TempFolder::new();
        put(&folder.path().join(".git"), "config.worktree", EVIL);
        assert!(runs_code(folder.path()).is_some());
    }

    /// What cannot be checked counts as unsafe.
    #[test]
    fn given_config_that_cannot_be_checked_then_it_counts_as_unsafe() {
        let unreadable_pointer = TempFolder::new();
        put(unreadable_pointer.path(), ".git", b"not a pointer\n");
        assert!(runs_code(unreadable_pointer.path()).is_some());

        let binary = TempFolder::new();
        put(&binary.path().join(".git"), "config", b"\xff\xfe");
        assert!(runs_code(binary.path()).is_some());

        let huge = TempFolder::new();
        let padding = vec![b'#'; MAX_CONFIG_BYTES as usize + 1];
        put(&huge.path().join(".git"), "config", &padding);
        let found = runs_code(huge.path());
        assert!(found.is_some_and(|text| text.contains("too large")));
    }

    #[test]
    fn given_ordinary_config_when_checked_then_nothing_is_found() {
        let text = "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\
                    \tfsmonitor = false\n\tpager =\n\
                    [remote \"origin\"]\n\turl = https://example.com/x.git\n\
                    # pager = ./evil\n[branch \"main\"]\n\tremote = origin\n";
        assert_eq!(check_text(text), None);
    }

    #[test]
    fn given_config_that_cannot_be_followed_when_checked_then_it_is_unsafe() {
        assert!(check_text("[core\n\tpager = x").is_some());
        assert!(check_text("[core]\n\tpager = a \\\n b").is_some());
    }
}
