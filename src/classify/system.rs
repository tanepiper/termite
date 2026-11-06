//! Forms of system tools that only look. Anything not named here keeps the
//! tool's level, so an option this file doesn't know stays refused.

const SYSTEMCTL_OPTIONS: &[&str] = &[
    "--no-pager",
    "--user",
    "--all",
    "-a",
    "--failed",
    "--full",
    "-l",
];
const SYSTEMCTL_VERBS: &[&str] = &[
    "status",
    "is-active",
    "is-enabled",
    "is-failed",
    "list-units",
    "list-unit-files",
    "list-timers",
];
const IP_OPTIONS: &[&str] = &[
    "-4", "-6", "-s", "-o", "-br", "-brief", "-c", "-color", "-d", "-details",
    "-j", "-json", "-p",
];
const IP_OBJECTS: &[&str] = &[
    "a", "addr", "address", "l", "link", "r", "route", "n", "neigh",
];

/// The program only shows something, in a form known to change nothing.
pub(super) fn only_looks(name: &str, arguments: &[String]) -> bool {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match name {
        "diskutil" => matches!(words.first(), Some(&"list" | &"info")),
        "launchctl" => words.first() == Some(&"list") && words.len() <= 2,
        "sysctl" => sysctl_reads(&words),
        "ip" => ip_reads(&words),
        "systemctl" => systemctl_verb(&words)
            .is_some_and(|verb| SYSTEMCTL_VERBS.contains(&verb)),
        _ => false,
    }
}

/// `systemctl status` prints recent log lines, and logs can hold anything.
pub(super) fn shows_logs(name: &str, arguments: &[String]) -> bool {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    name == "systemctl" && systemctl_verb(&words) == Some("status")
}

/// The first plain word, if every option on the line is a known one.
fn systemctl_verb<'a>(words: &[&'a str]) -> Option<&'a str> {
    let known = |word: &&&str| {
        !word.starts_with('-') || SYSTEMCTL_OPTIONS.contains(*word)
    };
    if !words.iter().all(|word| known(&word)) {
        return None;
    }
    words.iter().find(|word| !word.starts_with('-')).copied()
}

/// Reads only with `-n`, `-a` and plain names: `-w`, `-p` and `name=value`
/// all write.
fn sysctl_reads(words: &[&str]) -> bool {
    !words.is_empty()
        && words.iter().all(|word| {
            matches!(*word, "-n" | "-a")
                || (!word.starts_with('-')
                    && word.chars().all(|one| {
                        one.is_ascii_alphanumeric() || "._-".contains(one)
                    }))
        })
}

/// `ip [options] object [show|list ...]`.
fn ip_reads(words: &[&str]) -> bool {
    let mut rest = words;
    while let [first, others @ ..] = rest {
        if !IP_OPTIONS.contains(first) {
            break;
        }
        rest = others;
    }
    match rest {
        [object] => IP_OBJECTS.contains(object),
        [object, action, ..] => {
            IP_OBJECTS.contains(object) && matches!(*action, "show" | "list")
        },
        [] => false,
    }
}
