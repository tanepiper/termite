//! The read-only policy for `?x`: plain code that decides whether a command
//! only looks at the machine. No model can talk its way past it; the
//! decision model can only make the answer stricter.
//!
//! A refusal is cheap: the command falls back to `?` and the user confirms
//! it. So every rule leans towards refusing when in doubt.

mod programs;
mod secrets;
mod shell;

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

#[cfg(test)]
mod tests;
