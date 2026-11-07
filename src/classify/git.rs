//! What each git subcommand can lose.

use super::{Level, deletes};

/// The git subcommand, past `-C folder` and `-c name=value`.
pub(super) fn level_of(arguments: &[String]) -> Option<(Level, &'static str)> {
    let mut rest = arguments;
    while let [first, _, others @ ..] = rest {
        if first != "-C" && first != "-c" {
            break;
        }
        rest = others;
    }
    let (verb, options) = rest.split_first()?;
    let has = |word: &str| options.iter().any(|option| option == word);
    let loses = (Level::ContainedDestructive, "loses uncommitted work");
    let sweeps = (Level::SystemDangerous, deletes::LOOSE);
    let deletes = (Level::ContainedDestructive, "deletes data");
    match verb.as_str() {
        "push" if force_push(options) || has("--delete") || has("-d") => {
            Some((Level::SystemDangerous, "overwrites a shared branch"))
        },
        "reset" if has("--hard") => Some(loses),
        "clean" if only_lists(options) => None,
        "clean" => Some(sweeps),
        "restore" => Some(loses),
        "rm" => Some(deletes),
        "remote" | "worktree" if has("remove") || has("rm") => Some(deletes),
        "checkout" if has("--") || has(".") || has("-f") || has("--force") => {
            Some(loses)
        },
        "stash" if has("clear") => Some(sweeps),
        "stash" if has("drop") => Some(loses),
        "branch" if has("-D") || has("-d") || has("--delete") => Some(loses),
        "tag" | "update-ref" if has("-d") || has("--delete") => Some(deletes),
        "reflog" if has("expire") => Some(sweeps),
        "gc" if options.iter().any(|one| one.starts_with("--prune")) => {
            Some(sweeps)
        },
        _ => None,
    }
}

/// A dry run with only options known to be safe. `-e` takes a value, a later
/// `--no-dry-run` cancels the dry run, and a word after `--` is a path.
fn only_lists(options: &[String]) -> bool {
    let mut dry = false;
    for option in options {
        match option.as_str() {
            "--" => break,
            "--dry-run" => dry = true,
            "--quiet" => {},
            long if long.starts_with("--") => return false,
            short if short.starts_with('-') => {
                if !short[1..].chars().all(|one| "ndxXq".contains(one)) {
                    return false;
                }
                dry |= short.contains('n');
            },
            _ => {},
        }
    }
    dry
}

fn force_push(options: &[String]) -> bool {
    options.iter().any(|option| {
        option.starts_with("--force")
            || option.starts_with('+')
            || (option.starts_with('-')
                && !option.starts_with("--")
                && option.contains('f'))
    })
}
