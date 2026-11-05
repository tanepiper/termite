//! What is installed but not running. Contract: only looks at `PATH` and
//! known app folders; starts nothing.

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::discover::{FoundServer, ServerKind};

/// A server program that is installed but not running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) kind: ServerKind,
    pub(crate) path: PathBuf,
    pub(crate) start: String,
}

/// Server programs that are installed, with the command that starts each.
/// Only kinds with no running server are worth showing.
pub(crate) fn installed(running: &[FoundServer]) -> Vec<Installed> {
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let is_running =
        |kind: ServerKind| running.iter().any(|server| server.kind == kind);
    let mut found = Vec::new();

    if !is_running(ServerKind::LlamaCpp) {
        if let Some(path) = find_on_path("llama-server") {
            found.push(Installed {
                kind: ServerKind::LlamaCpp,
                path,
                start: "tmite setup can start it for you".to_owned(),
            });
        }
        let app = PathBuf::from("/Applications/Llama.app");
        if app.exists() {
            found.push(Installed {
                kind: ServerKind::LlamaCpp,
                path: app,
                start: "open -a Llama".to_owned(),
            });
        }
    }

    if !is_running(ServerKind::Ollama) {
        if let Some(path) = find_on_path("ollama") {
            found.push(Installed {
                kind: ServerKind::Ollama,
                path,
                start: "ollama serve".to_owned(),
            });
        }
    }

    if !is_running(ServerKind::LmStudio) {
        let lms = [
            home.join(".cache/lm-studio/bin/lms"),
            home.join(".lmstudio/bin/lms"),
        ]
        .into_iter()
        .find(|path| path.exists())
        .or_else(|| find_on_path("lms"));
        let app = PathBuf::from("/Applications/LM Studio.app");

        match lms {
            Some(path) => found.push(Installed {
                kind: ServerKind::LmStudio,
                start: format!("{} server start", path.display()),
                path,
            }),
            None if app.exists() => found.push(Installed {
                kind: ServerKind::LmStudio,
                path: app,
                start: "open LM Studio and turn on its local server".to_owned(),
            }),
            None => {},
        }
    }

    found
}

/// Find an executable on `PATH`.
pub(crate) fn find_on_path(program: &str) -> Option<PathBuf> {
    find_in(&env::var_os("PATH")?, program)
}

/// Relative entries, and the empty entry that means "here", are skipped: a
/// program found through them comes from whatever folder setup runs in.
fn find_in(path: &OsStr, program: &str) -> Option<PathBuf> {
    env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata().is_ok_and(|meta| {
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::find_in;

    /// A folder with `tool` in it, executable or not.
    fn folder(name: &str, mode: u32) -> PathBuf {
        // Test-only: nothing here is trusted, and it is removed afterwards.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let temp = std::env::temp_dir();
        let folder =
            temp.join(format!("termite-path-{}-{name}", std::process::id()));
        fs::create_dir_all(&folder).expect("folder");
        let tool = folder.join("tool");
        termite::create_private(&folder, "tool").expect("tool");
        fs::set_permissions(&tool, fs::Permissions::from_mode(mode))
            .expect("mode");
        folder
    }

    fn path_of(folders: &[&str]) -> OsString {
        std::env::join_paths(folders).expect("joined")
    }

    #[test]
    fn given_a_path_then_only_executable_files_are_found() {
        let plain = folder("plain", 0o600);
        let runnable = folder("runnable", 0o700);
        let path = path_of(&[
            plain.to_str().expect("path"),
            runnable.to_str().expect("path"),
        ]);

        assert_eq!(find_in(&path, "tool"), Some(runnable.join("tool")));
        assert_eq!(find_in(&path, "missing"), None);

        fs::remove_dir_all(plain).expect("clean up");
        fs::remove_dir_all(runnable).expect("clean up");
    }

    #[test]
    fn given_a_folder_named_like_the_program_then_it_is_not_found() {
        let parent = folder("named", 0o700);
        fs::create_dir(parent.join("thing")).expect("folder");
        let path = path_of(&[parent.to_str().expect("path")]);

        assert_eq!(find_in(&path, "thing"), None);

        fs::remove_dir_all(parent).expect("clean up");
    }

    /// A hostile repository must not supply `llama-server`.
    #[test]
    fn given_relative_path_entries_then_they_are_skipped() {
        for path in [".", "", "bin", "./bin:."] {
            assert_eq!(find_in(OsString::from(path).as_os_str(), "ls"), None);
            assert_eq!(find_in(OsString::from(path).as_os_str(), "tool"), None);
        }
    }
}
