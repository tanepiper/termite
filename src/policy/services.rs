//! The rules for tools with subcommands: `git`, `docker`, `kubectl` and the
//! package tools. Contract: an allowlist; anything not named is refused.

use super::PolicyRefusal;
use super::options::{
    operands, option_not_allowed, refuse_endless, refuse_options,
    subcommand_not_allowed,
};

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

pub(super) fn check_git(
    program: &str,
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    let mut rest = arguments;

    // Global options before the subcommand: only these are safe. `-C`
    // would move to a repository whose config the gate has not checked.
    while let Some((first, tail)) = rest.split_first() {
        match first.as_str() {
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
        // `--ext-diff` and `--textconv` run drivers named in config.
        name if GIT_READERS.contains(&name) => refuse_options(
            program,
            rest,
            &[],
            &["--output", "--ext-diff", "--textconv"],
        ),
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

pub(super) fn check_docker(
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

pub(super) fn check_kubectl(
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
            // `--raw` reads any API path, secrets included.
            refuse_options(program, rest, &[], &["--raw"])?;
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

pub(super) fn check_package_tool(
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
