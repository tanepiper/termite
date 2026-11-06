//! What each program does, by name and arguments. Only programs that can do
//! harm are listed: anything else is an ordinary change, level 2.

use super::Level;
use super::deletes;

/// Programs that run the next word as a command.
const WRAPPERS: &[&str] = &[
    "builtin",
    "caffeinate",
    "command",
    "exec",
    "ionice",
    "nice",
    "nohup",
    "stdbuf",
    "time",
    "timeout",
    "xargs",
];

/// Programs that run the rest of the line as root.
const ROOT: &[&str] = &["sudo", "doas", "su", "pkexec", "run0"];

/// Shells, which run whatever text they are fed.
pub(super) const SHELLS: &[&str] =
    &["sh", "bash", "zsh", "dash", "ksh", "fish", "ash"];

/// Languages that also run what they are fed.
pub(super) const LANGUAGES: &[&str] = &[
    "python", "python2", "python3", "perl", "ruby", "php", "node", "lua",
];

/// Programs that delete what they are given.
pub(super) const DELETERS: &[&str] = &[
    "rm",
    "rmdir",
    "unlink",
    "shred",
    "srm",
    "truncate",
    "trash",
    "trash-put",
    "rimraf",
];

/// Programs that send data off the machine.
pub(super) const SENDERS: &[&str] = &[
    "curl", "wget", "nc", "ncat", "netcat", "socat", "scp", "sftp", "rsync",
    "ftp", "telnet",
];

/// Programs that fetch something to run.
pub(super) const FETCHERS: &[&str] = &["curl", "wget", "fetch", "aria2c"];

/// Programs that search below a folder.
pub(super) const SEARCHERS: &[&str] =
    &["grep", "egrep", "fgrep", "rg", "ag", "ack", "find"];

/// Programs that print the environment, which holds tokens.
const SHOW_ENVIRONMENT: &[&str] = &["env", "printenv", "set", "declare"];

/// Programs that act on the whole machine.
const SYSTEM: &[&str] = &[
    "chpasswd",
    "crontab",
    "csrutil",
    "dd",
    "diskutil",
    "fdisk",
    "firewall-cmd",
    "gdisk",
    "groupadd",
    "halt",
    "ifconfig",
    "insmod",
    "ip",
    "ip6tables",
    "iptables",
    "launchctl",
    "modprobe",
    "mount",
    "networksetup",
    "nft",
    "nvram",
    "parted",
    "passwd",
    "pfctl",
    "poweroff",
    "reboot",
    "rmmod",
    "route",
    "scutil",
    "service",
    "setenforce",
    "shutdown",
    "spctl",
    "sysctl",
    "systemctl",
    "tccutil",
    "ufw",
    "umount",
    "useradd",
    "userdel",
    "usermod",
    "visudo",
    "wipefs",
];

/// A program and subcommand words that lose data.
const LOSES_DATA: &[(&str, &[&[&str]])] = &[
    (
        "docker",
        &[
            &["rm"],
            &["rmi"],
            &["prune"],
            &["system", "prune"],
            &["container", "rm"],
            &["container", "prune"],
            &["image", "rm"],
            &["image", "prune"],
            &["network", "rm"],
            &["volume", "rm"],
            &["volume", "prune"],
            &["compose", "down"],
            &["compose", "rm"],
        ],
    ),
    ("kubectl", &[&["delete"], &["drain"]]),
    ("helm", &[&["uninstall"], &["delete"]]),
];

/// The program's own name: no folder, and no case, as macOS ignores it.
pub(super) fn name_of(word: &str) -> String {
    word.rsplit('/').next().unwrap_or(word).to_lowercase()
}

/// A stage's words after the parts that only start something else.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Unwrapped {
    pub(super) words: Vec<String>,
    /// It runs as root.
    pub(super) root: bool,
    /// `xargs`: the arguments come from somewhere else.
    pub(super) unknown_arguments: bool,
    /// It sets a variable that changes which code runs.
    pub(super) doubt: bool,
}

/// Take off `FOO=1`, `env`, `sudo`, `xargs` and friends, with their options.
pub(super) fn unwrap(words: Vec<String>) -> Unwrapped {
    let mut found = Unwrapped::default();
    let mut rest = words.as_slice();
    loop {
        let after = skip_assignments(rest);
        let set = &rest[..rest.len() - after.len()];
        found.doubt |= set.iter().any(|word| changes_code(word));
        rest = after;
        let Some(first) = rest.first() else { break };
        let name = name_of(first);
        let is_env = name == "env" && rest.len() > 1;
        if ROOT.contains(&name.as_str()) {
            found.root = true;
        } else if name == "xargs" {
            found.unknown_arguments = true;
        } else if !is_env && !WRAPPERS.contains(&name.as_str()) {
            break;
        }
        rest = skip_options(&rest[1..]);
    }
    found.words = rest.to_vec();
    found
}

fn changes_code(assignment: &str) -> bool {
    ["LD_", "DYLD_", "PATH=", "PYTHONPATH=", "NODE_OPTIONS="]
        .iter()
        .any(|start| assignment.starts_with(start))
}

fn skip_assignments(words: &[String]) -> &[String] {
    let is_assignment = |word: &String| {
        word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        })
    };
    let count = words.iter().take_while(|word| is_assignment(word)).count();
    &words[count..]
}

/// Options, and the plain numbers that follow `timeout` and `nice -n`.
fn skip_options(words: &[String]) -> &[String] {
    let is_option = |word: &String| {
        word.starts_with('-')
            || word.chars().next().is_some_and(|c| c.is_ascii_digit())
    };
    let count = words.iter().take_while(|word| is_option(word)).count();
    &words[count..]
}

/// The arguments that aren't options. Anything after `--` is an argument.
pub(super) fn targets(arguments: &[String]) -> Vec<&str> {
    let mut found = Vec::new();
    let mut options_done = false;
    for word in arguments {
        if !options_done && word == "--" {
            options_done = true;
        } else if options_done || !word.starts_with('-') {
            found.push(word.as_str());
        }
    }
    found
}

fn has_option(arguments: &[String], short: char, long: &str) -> bool {
    arguments.iter().any(|word| {
        word == long
            || (word.starts_with('-')
                && !word.starts_with("--")
                && word.contains(short))
    })
}

/// `-r`, `-R` or `--recursive`.
pub(super) fn is_recursive(arguments: &[String]) -> bool {
    has_option(arguments, 'r', "--recursive")
        || has_option(arguments, 'R', "--recursive")
}

/// The level this program sets by itself, with why.
pub(super) fn level_of(
    name: &str,
    arguments: &[String],
) -> Option<(Level, &'static str)> {
    if SYSTEM.contains(&name) || name.starts_with("mkfs") {
        return Some((Level::SystemDangerous, "changes the whole system"));
    }
    if name == "git" {
        return git(arguments);
    }
    if privileged_container(name, arguments) {
        return Some((Level::SystemDangerous, "gives a container the host"));
    }
    if let Some(found) = deletes::by_tool(name, arguments) {
        return Some(found);
    }
    LOSES_DATA
        .iter()
        .find(|(program, _)| *program == name)
        .filter(|(_, verbs)| verbs.iter().any(|v| starts_with(arguments, v)))
        .map(|_| (Level::ContainedDestructive, "deletes data"))
}

fn starts_with(arguments: &[String], verb: &[&str]) -> bool {
    let words = arguments.iter().filter(|word| !word.starts_with('-'));
    verb.iter().zip(words).filter(|(a, b)| *a == *b).count() == verb.len()
}

fn privileged_container(name: &str, arguments: &[String]) -> bool {
    matches!(name, "docker" | "podman")
        && arguments.iter().any(|word| {
            word == "--privileged"
                || word.starts_with("/:/")
                || word.starts_with("--pid=host")
                || word.contains("docker.sock")
        })
}

/// The git subcommand, past `-C folder` and `-c name=value`.
fn git(arguments: &[String]) -> Option<(Level, &'static str)> {
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
        "clean" if has("-n") || has("--dry-run") => Some(loses),
        "clean" => Some(sweeps),
        "restore" => Some(loses),
        "rm" => Some(deletes),
        "remote" | "worktree" if has("remove") || has("rm") => Some(deletes),
        "checkout" if has("--") || has("-f") || has("--force") => Some(loses),
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

fn force_push(options: &[String]) -> bool {
    options.iter().any(|option| {
        option.starts_with("--force")
            || option.starts_with('+')
            || (option.starts_with('-')
                && !option.starts_with("--")
                && option.contains('f'))
    })
}

/// Reads that show secrets through a tool, not through a file.
pub(super) fn shows_secrets(name: &str, arguments: &[String]) -> bool {
    let has = |word: &str| arguments.iter().any(|one| one == word);
    let mentions = |text: &str| {
        arguments
            .iter()
            .any(|one| one.to_lowercase().contains(text))
    };
    match name {
        "export" => has("-p"),
        "kubectl" => mentions("secret"),
        "gh" => has("auth") && has("token"),
        "aws" => has("configure") && has("get"),
        "gpg" => mentions("--export-secret"),
        "security" => arguments.iter().any(|one| one.starts_with("find-")),
        "pass" | "op" => true,
        _ => SHOW_ENVIRONMENT.contains(&name),
    }
}

/// Hostile whatever it is asked for: why, if so.
pub(super) fn hostile(
    name: &str,
    arguments: &[String],
) -> Option<&'static str> {
    let has = |word: &str| arguments.iter().any(|one| one == word);
    let any_disable = arguments.iter().any(|one| one.contains("disable"));
    let lower = arguments.join(" ").to_lowercase();
    match name {
        "csrutil" if any_disable => {
            Some("switches off System Integrity Protection")
        },
        "spctl" if any_disable => Some("switches off Gatekeeper"),
        "security" if has("dump-keychain") => Some("dumps the keychain"),
        "nc" | "ncat" | "netcat" if has("-e") || has("-c") => {
            Some("hands your shell to someone else")
        },
        "socat" if lower.contains("exec:") => {
            Some("hands your shell to someone else")
        },
        _ if LANGUAGES.contains(&name) && lower.contains("socket") => {
            Some("hands your shell to someone else")
        },
        _ => None,
    }
}

/// Code passed as an argument, which this file can't read.
pub(super) fn runs_inline_code(name: &str, arguments: &[String]) -> bool {
    let has = |word: &str| arguments.iter().any(|one| one == word);
    let is_language = LANGUAGES.iter().any(|one| name.starts_with(one));
    match name {
        "awk" | "gawk" | "mawk" => {
            arguments.iter().any(|one| one.contains("system("))
        },
        "git" => has("-c"),
        _ => is_language && (has("-c") || has("-e")),
    }
}

/// `base64 -d`, `xxd -r` and `openssl … -d`.
pub(super) fn decodes(name: &str, arguments: &[String]) -> bool {
    let has = |word: &str| arguments.iter().any(|one| one == word);
    match name {
        "base64" | "base32" => has("-d") || has("-D") || has("--decode"),
        "xxd" => has("-r"),
        "openssl" => has("-d"),
        _ => false,
    }
}
