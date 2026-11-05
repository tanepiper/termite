//! The read-only policy for `?x`: plain code that decides whether a command
//! only looks at the machine. No model can talk its way past it; the
//! decision model can only make the answer stricter.
//!
//! A refusal is cheap: the command falls back to `?` and the user confirms
//! it. So every rule leans towards refusing when in doubt.

mod git_config;
mod options;
mod programs;
mod secrets;
mod services;
mod shell;

use std::path::Path;

use thiserror::Error;

use crate::command::{CommandRejected, check_command};

/// Why a command may not run without asking. The message completes the
/// sentence "Not run automatically: …".
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PolicyRefusal {
    /// The command failed the basic command checks.
    #[error("{0}")]
    Unusable(CommandRejected),
    /// More than one command: `;`, `&&`, `||`, `&`.
    #[error("it runs more than one command")]
    ChainsCommands,
    /// `>`, `<` and friends, which can write or read any file.
    #[error("it redirects input or output")]
    Redirects,
    /// `$(…)`, backticks, subshells, `eval`, `source`, `exec`, `xargs`.
    #[error("it runs code that is built while it runs")]
    RunsBuiltCode,
    /// `!` history expansion, which the shell replaces with an earlier
    /// command.
    #[error("it uses `!`, which the shell can replace with an earlier command")]
    UsesHistory,
    /// `{a,b}` or `{1..3}`, which the shell expands into other words.
    #[error("it uses `{{…}}`, which the shell expands into other words")]
    ExpandsBraces,
    /// `$'…'` or `$"…"`, which build text from escape codes.
    #[error("it builds text from escape codes")]
    EncodedText,
    /// `NAME=value` in front of the command.
    #[error("it sets environment variables for the command")]
    SetsVariables,
    /// `sudo`, `doas`, `su`, `pkexec`.
    #[error("`{program}` raises privileges")]
    RaisesPrivileges {
        /// The program that was used.
        program: String,
    },
    /// `tee`.
    #[error("`{program}` writes files")]
    WritesFiles {
        /// The program that was used.
        program: String,
    },
    /// The program is not on the read-only list.
    #[error("`{program}` is not on the read-only list")]
    ProgramNotAllowed {
        /// The program that was used.
        program: String,
    },
    /// The subcommand is not on the read-only list.
    #[error("`{program} {subcommand}` is not on the read-only list")]
    SubcommandNotAllowed {
        /// The program that was used.
        program: String,
        /// The subcommand, or nothing when it is missing.
        subcommand: String,
    },
    /// An option or argument that can change things or run other programs.
    #[error("`{program} {option}` can change things or run other programs")]
    OptionNotAllowed {
        /// The program that was used.
        program: String,
        /// The option or argument that was turned down.
        option: String,
    },
    /// An option that keeps the command running until it is stopped.
    #[error("`{program} {option}` keeps running until it is stopped")]
    NeverEnds {
        /// The program that was used.
        program: String,
        /// The option that was turned down.
        option: String,
    },
    /// A search that reads every file below a folder, hidden ones too.
    #[error(
        "`{program} {option}` reads every file below a folder, hidden ones included"
    )]
    SearchesEverything {
        /// The program that was used.
        program: String,
        /// The option that makes it read everything.
        option: String,
    },
    /// A search that starts from `/` or a home folder.
    #[error("it searches `{path}`, which holds private files")]
    SearchesTooWidely {
        /// The folder as written, or the current folder.
        path: String,
    },
    /// A device file or `-` (the keyboard), which may never end.
    #[error("it reads `{path}`, a device or the keyboard, which may never end")]
    ReadsDevice {
        /// The argument as written.
        path: String,
    },
    /// The repository's own config makes git run other programs.
    #[error("this repository's git config {detail}")]
    RepoConfigRunsCode {
        /// What the config does, such as "sets `core.fsmonitor`".
        detail: String,
    },
    /// An argument that names, or may match, a file holding secrets.
    #[error("it may read `{path}`, which can hold secrets")]
    ReadsSecret {
        /// The argument as written.
        path: String,
    },
}

/// Decide whether a command is read-only: it looks at the machine without
/// changing it, needs no extra privileges, and runs no other code.
///
/// ```
/// use termite::{PolicyRefusal, read_only};
///
/// assert!(read_only("ls -la ~/code | wc -l").is_ok());
/// assert_eq!(
///     read_only("rm -rf build"),
///     Err(PolicyRefusal::ProgramNotAllowed { program: "rm".into() })
/// );
/// ```
pub fn read_only(command: &str) -> Result<(), PolicyRefusal> {
    check_command(command).map_err(PolicyRefusal::Unusable)?;

    let stages = shell::pipeline_stages(command)?;

    for words in &stages {
        programs::check_stage(words)?;
        secrets::check_arguments(&words[1..])?;
    }

    Ok(())
}

/// [`read_only`], plus the checks that depend on where the command runs:
/// git in a repository whose own config runs programs, and `rg` started
/// from `/` or the home folder.
///
/// ```
/// use std::path::Path;
/// use termite::{PolicyRefusal, read_only_in};
///
/// let home = Path::new("/Users/me");
/// assert!(read_only_in("rg todo", Path::new("/Users/me/code"), Some(home)).is_ok());
/// assert_eq!(
///     read_only_in("rg todo", home, Some(home)),
///     Err(PolicyRefusal::SearchesTooWidely { path: "/Users/me".into() })
/// );
/// ```
pub fn read_only_in(
    command: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Result<(), PolicyRefusal> {
    read_only(command)?;

    // A bare `ls` names no path, so the folder itself has to be checked.
    let folder = cwd.to_string_lossy();
    if secrets::is_secret_path(&folder) {
        return Err(PolicyRefusal::ReadsSecret {
            path: folder.into_owned(),
        });
    }

    for words in shell::pipeline_stages(command)? {
        match words[0].as_str() {
            "git" => {
                if let Some(detail) = git_config::runs_code(cwd) {
                    return Err(PolicyRefusal::RepoConfigRunsCode { detail });
                }
            },
            "rg" if cwd == Path::new("/") || home == Some(cwd) => {
                return Err(PolicyRefusal::SearchesTooWidely {
                    path: cwd.display().to_string(),
                });
            },
            _ => {},
        }
    }

    Ok(())
}

/// Whether `command`, run in `cwd`, may read a file that holds secrets. The
/// folder counts as well as the text: `cat *` names no secret, but in
/// `~/.ssh` it prints every private key.
///
/// ```
/// use std::path::Path;
/// use termite::may_read_secrets;
///
/// assert!(may_read_secrets("cat *", Path::new("/Users/me/.ssh")));
/// assert!(may_read_secrets("cat .netrc", Path::new("/Users/me")));
/// assert!(!may_read_secrets("cat *", Path::new("/Users/me/code")));
/// ```
pub fn may_read_secrets(command: &str, cwd: &Path) -> bool {
    secrets::is_secret_path(&cwd.to_string_lossy())
        || matches!(read_only(command), Err(PolicyRefusal::ReadsSecret { .. }))
}

/// Whether `path` names, or may match, a file that holds secrets.
pub(crate) fn names_secret(path: &str) -> bool {
    secrets::is_secret_path(path)
}

#[cfg(test)]
mod tests;
