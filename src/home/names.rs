//! Names for installed scripts. A name must be plain, and must not take
//! the place of anything you can already run.

use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Bash and zsh reserved words and builtins. A script with one of these
/// names could never be run by name, or would hide the builtin.
const SHELL_WORDS: &[&str] = &[
    // reserved words
    "case",
    "coproc",
    "do",
    "done",
    "elif",
    "else",
    "end",
    "esac",
    "fi",
    "for",
    "foreach",
    "function",
    "if",
    "in",
    "nocorrect",
    "repeat",
    "select",
    "then",
    "time",
    "until",
    "while",
    // bash builtins
    "alias",
    "bg",
    "bind",
    "break",
    "builtin",
    "caller",
    "cd",
    "command",
    "compgen",
    "complete",
    "compopt",
    "continue",
    "declare",
    "dirs",
    "disown",
    "echo",
    "enable",
    "eval",
    "exec",
    "exit",
    "export",
    "false",
    "fc",
    "fg",
    "getopts",
    "hash",
    "help",
    "history",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "popd",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "return",
    "set",
    "shift",
    "shopt",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "true",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "wait",
    // zsh builtins
    "autoload",
    "bindkey",
    "bye",
    "chdir",
    "compadd",
    "compdef",
    "emulate",
    "float",
    "functions",
    "getln",
    "integer",
    "limit",
    "log",
    "noglob",
    "print",
    "pushln",
    "r",
    "rehash",
    "sched",
    "setopt",
    "unfunction",
    "unhash",
    "unlimit",
    "unsetopt",
    "vared",
    "whence",
    "where",
    "which",
    "zcompile",
    "zformat",
    "zle",
    "zmodload",
    "zparseopts",
    "zstyle",
];

const MIN_LENGTH: usize = 2;
const MAX_LENGTH: usize = 40;

/// A valid script name: 2–40 characters of `a-z`, `0-9` and `-`, starting
/// with a letter, with no leading, trailing or doubled `-`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScriptName(String);

/// Why a name cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameTaken {
    #[error(
        "{name:?} is not a usable script name: use {MIN_LENGTH} to \
         {MAX_LENGTH} lowercase letters, digits and single dashes, \
         starting with a letter"
    )]
    Malformed { name: String },

    #[error("{name} is a shell keyword or builtin")]
    ShellWord { name: String },

    #[error("{name} would hide {}", program.display())]
    Program { name: String, program: PathBuf },
}

impl ScriptName {
    /// Check the shape of `name`.
    pub fn parse(name: &str) -> Result<Self, NameTaken> {
        let length_ok = (MIN_LENGTH..=MAX_LENGTH).contains(&name.len());
        let starts_with_letter =
            name.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        let characters_ok = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let dashes_ok = !name.ends_with('-') && !name.contains("--");

        if length_ok && starts_with_letter && characters_ok && dashes_ok {
            Ok(Self(name.to_owned()))
        } else {
            Err(NameTaken::Malformed {
                name: name.to_owned(),
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check that nothing you can already run has this name: no shell
    /// keyword or builtin, and no program in `PATH` outside `bin` (a script
    /// already in `bin` may be replaced, after asking). Aliases and shell
    /// functions are only visible to the shell, which checks them itself.
    pub fn check_free(&self, bin: &Path) -> Result<(), NameTaken> {
        self.check_free_in(env::var_os("PATH").as_deref(), bin)
    }

    pub(crate) fn check_free_in(
        &self,
        path: Option<&OsStr>,
        bin: &Path,
    ) -> Result<(), NameTaken> {
        if SHELL_WORDS.contains(&self.as_str()) {
            return Err(NameTaken::ShellWord {
                name: self.0.clone(),
            });
        }

        let bin = fs::canonicalize(bin).unwrap_or_else(|_| bin.to_owned());
        for folder in path.map(env::split_paths).into_iter().flatten() {
            let same_as_bin = fs::canonicalize(&folder)
                .unwrap_or_else(|_| folder.clone())
                == bin;
            if same_as_bin || folder.as_os_str().is_empty() {
                continue;
            }
            let program = folder.join(&self.0);
            if is_runnable(&program) {
                return Err(NameTaken::Program {
                    name: self.0.clone(),
                    program,
                });
            }
        }
        Ok(())
    }
}

impl fmt::Display for ScriptName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A file (or a symlink to one) with any execute bit set.
fn is_runnable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    })
}
