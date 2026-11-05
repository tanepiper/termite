//! The programs allowed unguarded, with the options and subcommands each
//! one may use. Anything not listed here is refused.

use super::PolicyRefusal;
use super::options::{
    find_option, not_allowed, operands, option_not_allowed, refuse_endless,
    refuse_options,
};
use super::services::{
    check_docker, check_git, check_kubectl, check_package_tool,
};

/// `find` actions that delete, run programs or write files.
const FIND_ACTIONS: &[&str] = &[
    "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0",
    "-fprintf", "-fls",
];

/// Check one stage of a pipe: its words, program first.
pub(super) fn check_stage(words: &[String]) -> Result<(), PolicyRefusal> {
    let Some((program, arguments)) = words.split_first() else {
        return Ok(());
    };

    if is_assignment(program) {
        return Err(PolicyRefusal::SetsVariables);
    }

    match program.as_str() {
        "sudo" | "doas" | "su" | "pkexec" => {
            Err(PolicyRefusal::RaisesPrivileges {
                program: program.clone(),
            })
        },
        "eval" | "source" | "." | "exec" | "xargs" => {
            Err(PolicyRefusal::RunsBuiltCode)
        },
        "tee" => Err(PolicyRefusal::WritesFiles {
            program: program.clone(),
        }),
        "ls" | "pwd" | "stat" | "du" | "df" | "wc" | "which" | "whoami"
        | "uname" | "id" | "uptime" => Ok(()),
        "cat" | "head" => refuse_devices(arguments),
        "grep" => check_grep(program, arguments),
        "tree" => refuse_options(program, arguments, &['o'], &[]),
        "file" => refuse_options(program, arguments, &['C'], &["--compile"]),
        "date" => check_date(program, arguments),
        "tail" => {
            refuse_endless(program, arguments, &['f', 'F'], &["--follow"])?;
            refuse_devices(arguments)
        },
        "less" => check_less(program, arguments),
        "bat" => {
            refuse_options(program, arguments, &[], &["--pager"])?;
            refuse_devices(arguments)
        },
        "rg" => check_rg(program, arguments),
        "fd" => refuse_options(
            program,
            arguments,
            &['x', 'X'],
            &["--exec", "--exec-batch"],
        ),
        "find" => check_find(program, arguments),
        "git" => check_git(program, arguments),
        "docker" => check_docker(program, arguments),
        "kubectl" => check_kubectl(program, arguments),
        "brew" | "npm" | "cargo" => check_package_tool(program, arguments),
        _ => Err(not_allowed(program)),
    }
}

fn check_date(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    refuse_options(program, arguments, &['s'], &["--set"])?;

    // An operand that is not a `+format` sets the clock.
    match operands(arguments).find(|operand| !operand.starts_with('+')) {
        Some(operand) => Err(option_not_allowed(program, operand)),
        None => Ok(()),
    }
}

/// `grep` reads only what it is given. Recursion reads every file below a
/// folder, `.env` and other hidden files included.
fn check_grep(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    let recursive = find_option(
        arguments,
        &['r', 'R'],
        &["--recursive", "--dereference-recursive"],
    )
    .or_else(|| directories_recurse(arguments));
    if let Some(option) = recursive {
        return Err(PolicyRefusal::SearchesEverything {
            program: program.to_owned(),
            option: option.to_owned(),
        });
    }
    refuse_devices(arguments)
}

/// `-d recurse`, `-drecurse` or `--directories=recurse`.
fn directories_recurse(arguments: &[String]) -> Option<&str> {
    let mut previous = "";
    for argument in arguments.iter().take_while(|a| a.as_str() != "--") {
        let recurse = argument.ends_with("recurse")
            && (argument.starts_with('-')
                || previous == "-d"
                || previous.starts_with("--d"));
        if recurse {
            return Some(argument);
        }
        previous = argument;
    }
    None
}

/// `rg` skips hidden and ignored files by default; these options undo
/// that, follow symlinks into other folders, or run a preprocessor.
fn check_rg(program: &str, arguments: &[String]) -> Result<(), PolicyRefusal> {
    refuse_options(program, arguments, &[], &["--pre"])?;
    if let Some(option) = find_option(
        arguments,
        &['.', 'u', 'L'],
        &[
            "--hidden",
            "--unrestricted",
            "--follow",
            "--no-ignore",
            "--no-ignore-vcs",
            "--no-ignore-dot",
            "--no-ignore-global",
            "--no-ignore-parent",
            "--no-ignore-exclude",
            "--no-ignore-files",
        ],
    ) {
        return Err(PolicyRefusal::SearchesEverything {
            program: program.to_owned(),
            option: option.to_owned(),
        });
    }
    if let Some(path) = operands(arguments).find(|path| is_broad_folder(path)) {
        return Err(PolicyRefusal::SearchesTooWidely {
            path: path.to_owned(),
        });
    }
    refuse_devices(arguments)
}

/// `/`, a home folder, or the folder that holds home folders, as written.
/// The current folder is checked against the real home folder by the gate.
pub(super) fn is_broad_folder(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    let parts: Vec<&str> =
        trimmed.split('/').filter(|part| !part.is_empty()).collect();

    trimmed.is_empty()
        || trimmed.starts_with('~') && !trimmed.contains('/')
        || matches!(trimmed, "$HOME" | "${HOME}")
        || matches!(
            parts.as_slice(),
            ["Users" | "home"] | ["Users" | "home", _] | ["root"]
        ) && trimmed.starts_with('/')
}

/// Device files and `-` (standard input): reading them may never end.
fn refuse_devices(arguments: &[String]) -> Result<(), PolicyRefusal> {
    match operands(arguments)
        .find(|path| path.starts_with("/dev/") || *path == "-")
    {
        Some(path) => Err(PolicyRefusal::ReadsDevice {
            path: path.to_owned(),
        }),
        None => Ok(()),
    }
}

fn check_less(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    refuse_options(
        program,
        arguments,
        &['o', 'O'],
        &["--log-file", "--LOG-FILE"],
    )?;

    // `+command` runs a less command at start, which can run a shell.
    match operands(arguments).find(|operand| operand.starts_with('+')) {
        Some(operand) => Err(option_not_allowed(program, operand)),
        None => refuse_devices(arguments),
    }
}

fn check_find(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    match arguments
        .iter()
        .find(|argument| FIND_ACTIONS.contains(&argument.as_str()))
    {
        Some(action) => Err(option_not_allowed(program, action)),
        None => Ok(()),
    }
}

/// `NAME=value` as the first word sets a variable for the command.
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
