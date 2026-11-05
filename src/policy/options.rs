//! Reading command-line options. Contract: pure functions over the words of
//! one stage; each answer can only lead to a refusal.

use super::PolicyRefusal;

pub(super) fn refuse_options(
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

pub(super) fn refuse_endless(
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
pub(super) fn find_option<'a>(
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
pub(super) fn operands(arguments: &[String]) -> impl Iterator<Item = &str> {
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

pub(super) fn not_allowed(program: &str) -> PolicyRefusal {
    PolicyRefusal::ProgramNotAllowed {
        program: program.to_owned(),
    }
}

pub(super) fn subcommand_not_allowed(
    program: &str,
    subcommand: &str,
) -> PolicyRefusal {
    PolicyRefusal::SubcommandNotAllowed {
        program: program.to_owned(),
        subcommand: subcommand.to_owned(),
    }
}

pub(super) fn option_not_allowed(program: &str, option: &str) -> PolicyRefusal {
    PolicyRefusal::OptionNotAllowed {
        program: program.to_owned(),
        option: option.to_owned(),
    }
}
