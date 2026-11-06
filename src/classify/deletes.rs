//! The delete cluster: every way of saying "delete" counts the same. Contract:
//! the more exactly a command names what it deletes, the lower its level. A
//! name asks first, and anything else must be typed by a person.

use super::Level;

/// Tools with subcommands, where a word below means a delete. A list, so that
/// `mkdir delete` and `grep rm notes.txt` stay what they are.
const TOOLS: &[&str] = &[
    "apt",
    "apt-get",
    "aws",
    "az",
    "brew",
    "cargo",
    "defaults",
    "dnf",
    "docker",
    "gcloud",
    "gem",
    "gh",
    "helm",
    "kubectl",
    "npm",
    "pacman",
    "pip",
    "pip3",
    "pipx",
    "pnpm",
    "podman",
    "pulumi",
    "redis-cli",
    "snap",
    "terraform",
    "tmutil",
    "tofu",
    "yarn",
    "yum",
];

/// Programs that delete the thing named next, by their own name.
const NAMED_PROGRAMS: &[&str] = &["deluser", "dropdb", "dropuser"];

/// Options that make an ordinary program delete its input.
const DELETING_OPTIONS: &[(&str, &str)] = &[
    ("rsync", "--remove-source-files"),
    ("tar", "--remove-files"),
    ("zip", "--move"),
    ("zip", "-m"),
];

/// Delete the thing named next.
const NAMED_WORDS: &[&str] = &[
    "del",
    "delete",
    "drop",
    "erase",
    "purge",
    "remove",
    "rm",
    "rmi",
    "uninstall",
];

/// Delete whatever fits a test, with no name given.
const SWEEP_WORDS: &[&str] =
    &["autoremove", "destroy", "flushall", "flushdb", "prune"];

/// A cache comes back by itself, so emptying one isn't a delete.
const CACHE: &[&str] = &["cache"];

/// `kubectl delete` options that pick by something other than a name.
const SELECTORS: &[&str] =
    &["--all", "--all-namespaces", "-A", "-l", "--selector"];

/// `find` tests that take a name.
const NAME_TESTS: &[&str] = &[
    "-name",
    "-iname",
    "-path",
    "-ipath",
    "-wholename",
    "-iwholename",
];

/// `find` words that make a name mean something wider.
const LOOSENERS: &[&str] = &["!", "-not", "-o", "-or", "-regex", "-iregex"];

pub(super) const LOOSE: &str = "deletes what fits a pattern, not a name, so it can match more than you meant";
const NAMED: &str = "deletes data";

/// A delete through a tool's subcommand or an option. A subcommand comes in
/// the first two words, as in `docker rm` and `docker volume rm`.
pub(super) fn by_tool(
    name: &str,
    arguments: &[String],
) -> Option<(Level, &'static str)> {
    let option = |word: &str| arguments.iter().any(|one| one == word);
    if name == "rsync"
        && arguments.iter().any(|word| word.starts_with("--delete"))
    {
        return Some((Level::SystemDangerous, LOOSE));
    }
    let deleting = DELETING_OPTIONS
        .iter()
        .any(|(program, word)| *program == name && option(word));
    if deleting || NAMED_PROGRAMS.contains(&name) {
        return Some((Level::ContainedDestructive, NAMED));
    }
    if !TOOLS.contains(&name) {
        return None;
    }
    let verbs: Vec<&str> = arguments
        .iter()
        .filter(|word| !word.starts_with('-'))
        .take(2)
        .map(String::as_str)
        .collect();
    let has = |list: &[&str]| verbs.iter().any(|verb| list.contains(verb));
    let selects = SELECTORS.iter().any(|word| option(word));
    let volumes = has(&["down"]) && (option("-v") || option("--volumes"));
    if has(CACHE) {
        None
    } else if has(SWEEP_WORDS) || (has(NAMED_WORDS) && selects) || volumes {
        Some((Level::SystemDangerous, LOOSE))
    } else if has(NAMED_WORDS) {
        Some((Level::ContainedDestructive, NAMED))
    } else {
        None
    }
}

/// Does `find` name exactly what it picks? Only a plain name does: a pattern,
/// `-not`, `-o`, a file type or an age can each match more than was meant.
pub(super) fn find_names_exactly(arguments: &[String]) -> bool {
    let plain = |word: &String| word.trim_matches(['\'', '"', '\\']).to_owned();
    let loosened = arguments
        .iter()
        .any(|word| LOOSENERS.contains(&plain(word).as_str()));
    let names: Vec<&String> = arguments
        .windows(2)
        .filter(|pair| NAME_TESTS.contains(&pair[0].as_str()))
        .map(|pair| &pair[1])
        .collect();
    !loosened
        && !names.is_empty()
        && !names.iter().any(|name| name.contains(['*', '?', '[']))
}
