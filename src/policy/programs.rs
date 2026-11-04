//! The programs allowed unguarded, with the options and subcommands each
//! one may use. Anything not listed here is refused.

use super::PolicyRefusal;

/// `find` actions that delete, run programs or write files.
const FIND_ACTIONS: &[&str] = &[
    "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0",
    "-fprintf", "-fls",
];

/// `git` subcommands that only read.
const GIT_READERS: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "rev-parse",
    "blame",
    "ls-files",
];

/// `git branch` options that only list branches.
const GIT_BRANCH_LISTING: &[&str] = &[
    "-a",
    "-r",
    "-v",
    "-vv",
    "--all",
    "--remotes",
    "--verbose",
    "--list",
    "--show-current",
    "--color",
    "--no-color",
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
        | "uname" | "id" | "uptime" | "cat" | "head" | "grep" => Ok(()),
        "tree" => refuse_options(program, arguments, &['o'], &[]),
        "file" => refuse_options(program, arguments, &['C'], &["--compile"]),
        "date" => check_date(program, arguments),
        "tail" => {
            refuse_endless(program, arguments, &['f', 'F'], &["--follow"])
        },
        "less" => check_less(program, arguments),
        "bat" => refuse_options(program, arguments, &[], &["--pager"]),
        "rg" => refuse_options(program, arguments, &[], &["--pre"]),
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
        None => Ok(()),
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

fn check_git(program: &str, arguments: &[String]) -> Result<(), PolicyRefusal> {
    let mut rest = arguments;

    // Global options before the subcommand: only these are safe.
    while let Some((first, tail)) = rest.split_first() {
        match first.as_str() {
            "-C" => rest = tail.get(1..).unwrap_or_default(),
            "--no-pager" | "-P" => rest = tail,
            option if option.starts_with('-') => {
                return Err(option_not_allowed(program, option));
            },
            _ => break,
        }
    }

    let Some((subcommand, rest)) = rest.split_first() else {
        return Err(subcommand_not_allowed(program, ""));
    };

    match subcommand.as_str() {
        name if GIT_READERS.contains(&name) => {
            refuse_options(program, rest, &[], &["--output"])
        },
        "branch" => match rest
            .iter()
            .find(|argument| !GIT_BRANCH_LISTING.contains(&argument.as_str()))
        {
            Some(argument) => Err(option_not_allowed(program, argument)),
            None => Ok(()),
        },
        "remote" => match rest {
            [] => Ok(()),
            [flag] if flag == "-v" || flag == "--verbose" => Ok(()),
            [first, ..] => Err(option_not_allowed(program, first)),
        },
        _ => Err(subcommand_not_allowed(program, subcommand)),
    }
}

fn check_docker(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    let Some((subcommand, rest)) = arguments.split_first() else {
        return Err(subcommand_not_allowed(program, ""));
    };

    match subcommand.as_str() {
        option if option.starts_with('-') => {
            Err(option_not_allowed(program, option))
        },
        "ps" | "images" | "inspect" | "version" | "info" => Ok(()),
        "logs" => refuse_endless(program, rest, &['f'], &["--follow"]),
        _ => Err(subcommand_not_allowed(program, subcommand)),
    }
}

fn check_kubectl(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    let mut rest = arguments;

    // Choosing a namespace or context before the subcommand is fine.
    while let Some((first, tail)) = rest.split_first() {
        match first.as_str() {
            "-n" | "--namespace" | "--context" => {
                rest = tail.get(1..).unwrap_or_default();
            },
            option
                if option.starts_with("--namespace=")
                    || option.starts_with("--context=") =>
            {
                rest = tail;
            },
            option if option.starts_with('-') => {
                return Err(option_not_allowed(program, option));
            },
            _ => break,
        }
    }

    let Some((subcommand, rest)) = rest.split_first() else {
        return Err(subcommand_not_allowed(program, ""));
    };

    match subcommand.as_str() {
        "get" => {
            refuse_endless(
                program,
                rest,
                &['w'],
                &["--watch", "--watch-only"],
            )?;
            match operands(rest).find(|operand| names_secrets(operand)) {
                Some(operand) => Err(option_not_allowed(program, operand)),
                None => Ok(()),
            }
        },
        "describe" | "version" => Ok(()),
        "logs" => refuse_endless(program, rest, &['f'], &["--follow"]),
        "config" if rest.len() == 1 && rest[0] == "current-context" => Ok(()),
        _ => Err(subcommand_not_allowed(program, subcommand)),
    }
}

fn check_package_tool(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    match arguments {
        [] => Err(subcommand_not_allowed(program, "")),
        [first, extra, ..] if first == "--version" => {
            Err(option_not_allowed(program, extra))
        },
        [first, ..]
            if matches!(
                first.as_str(),
                "--version" | "list" | "info" | "outdated"
            ) =>
        {
            Ok(())
        },
        [first, ..] => Err(subcommand_not_allowed(program, first)),
    }
}

/// `secret`, `secrets`, `secret/name` or `secrets.v1`, alone or in a
/// comma-separated list of kinds.
fn names_secrets(operand: &str) -> bool {
    operand.split(',').any(|kind| {
        let kind = kind.split(['/', '.']).next().unwrap_or_default();
        kind == "secret" || kind == "secrets"
    })
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

fn refuse_options(
    program: &str,
    arguments: &[String],
    short: &[char],
    long: &[&str],
) -> Result<(), PolicyRefusal> {
    match find_option(arguments, short, long) {
        Some(found) => Err(option_not_allowed(program, found)),
        None => Ok(()),
    }
}

fn refuse_endless(
    program: &str,
    arguments: &[String],
    short: &[char],
    long: &[&str],
) -> Result<(), PolicyRefusal> {
    match find_option(arguments, short, long) {
        Some(found) => Err(PolicyRefusal::NeverEnds {
            program: program.to_owned(),
            option: found.to_owned(),
        }),
        None => Ok(()),
    }
}

/// Find an option before `--` that is one of `short` (also inside a
/// cluster such as `-nF`) or one of `long` (also as `--long=value`, or
/// shortened the way GNU tools accept, such as `--fol` for `--follow`).
fn find_option<'a>(
    arguments: &'a [String],
    short: &[char],
    long: &[&str],
) -> Option<&'a str> {
    arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .map(String::as_str)
        .find(|argument| {
            if let Some(name) = argument.strip_prefix("--") {
                let name = name.split('=').next().unwrap_or_default();
                !name.is_empty()
                    && long.iter().any(|option| {
                        let option = &option[2..];
                        option == name || option.starts_with(name)
                    })
            } else if let Some(cluster) = argument.strip_prefix('-') {
                cluster.chars().any(|c| short.contains(&c))
            } else {
                false
            }
        })
}

/// Words that are not options: all words after `--`, and words before it
/// that do not start with `-`.
fn operands(arguments: &[String]) -> impl Iterator<Item = &str> {
    let mut after_double_dash = false;

    arguments
        .iter()
        .map(String::as_str)
        .filter(move |argument| {
            if after_double_dash {
                return true;
            }
            if *argument == "--" {
                after_double_dash = true;
                return false;
            }
            !argument.starts_with('-') || *argument == "-"
        })
}

fn not_allowed(program: &str) -> PolicyRefusal {
    PolicyRefusal::ProgramNotAllowed {
        program: program.to_owned(),
    }
}

fn subcommand_not_allowed(program: &str, subcommand: &str) -> PolicyRefusal {
    PolicyRefusal::SubcommandNotAllowed {
        program: program.to_owned(),
        subcommand: subcommand.to_owned(),
    }
}

fn option_not_allowed(program: &str, option: &str) -> PolicyRefusal {
    PolicyRefusal::OptionNotAllowed {
        program: program.to_owned(),
        option: option.to_owned(),
    }
}
