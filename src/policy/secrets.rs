//! Spots arguments that name, or may match, files that hold secrets or
//! private data.
//!
//! Every comparison ignores case: macOS file names do, so `~/.SSH` is
//! `~/.ssh` there.

use super::PolicyRefusal;

/// Folders whose contents are all secret.
const SECRET_FOLDERS: &[&str] = &[
    ".ssh",
    ".aws",
    ".config",
    ".gnupg",
    ".kube",
    ".azure",
    ".password-store",
    ".terraform.d",
    ".mozilla",
    "keychains",
];

/// Folder pairs (a folder and the one inside it) that are secret wherever
/// they appear, and everything below them.
const SECRET_PAIRS: &[[&str; 2]] = &[
    ["library", "keychains"],
    [".docker", "config.json"],
    ["library", "cookies"],
    ["library", "mail"],
    ["library", "messages"],
    ["library", "safari"],
    ["application support", "google"],
    ["application support", "firefox"],
    ["application support", "bravesoftware"],
    ["application support", "1password"],
    ["share", "fish"],
];

/// File names that are secret or private.
const SECRET_NAMES: &[&str] = &[
    "shadow",
    "gshadow",
    "sudoers",
    "master.passwd",
    ".netrc",
    ".git-credentials",
    ".npmrc",
    ".pypirc",
    ".pgpass",
    ".my.cnf",
    ".zsh_history",
    ".bash_history",
    ".sh_history",
    ".python_history",
    ".node_repl_history",
    ".psql_history",
    ".mysql_history",
    ".lesshst",
    ".viminfo",
    "fish_history",
    "credentials",
    "environ",
];

/// File name beginnings that are secret: `.env`, `.env.local`, and SSH
/// keys (`id_rsa`, `id_ed25519.pub`) wherever they are.
const SECRET_PREFIXES: &[&str] =
    &[".env", "id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];

/// File name endings that are secret.
const SECRET_ENDINGS: &[&str] = &[
    ".env",
    ".pem",
    ".key",
    ".kdbx",
    ".p12",
    ".pfx",
    ".keychain",
    ".keychain-db",
    ".tfstate",
    ".tfstate.backup",
    ".gpg",
];

/// Refuse any argument that may name a secret file, in any of the forms a
/// path can take.
pub(super) fn check_arguments(
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    for argument in arguments {
        if let Some(path) = paths_in(argument).find(|p| is_secret_path(p)) {
            return Err(PolicyRefusal::ReadsSecret {
                path: path.to_owned(),
            });
        }
    }

    Ok(())
}

/// The ways `argument` may hold a path. An argument starting with `-` is
/// not always an option (`cat -- -a/../.env`), and an option may hold its
/// value (`--file=.env`, `-f.env`), so each reading is checked.
fn paths_in(argument: &str) -> impl Iterator<Item = &str> {
    let after_equals = argument
        .starts_with('-')
        .then(|| argument.split_once('=').map(|(_, value)| value))
        .flatten();
    let after_short_option = argument
        .strip_prefix('-')
        .filter(|rest| !rest.starts_with('-'))
        .and_then(|rest| rest.get(1..));

    [after_equals, after_short_option, Some(argument)]
        .into_iter()
        .flatten()
}

/// Match by path parts, so `~/.ssh`, `$HOME/.ssh` and `/Users/me/.ssh` are
/// all caught without expanding anything. A glob in a part starting with
/// `.` may match a hidden secret folder, so it counts as secret too.
pub(super) fn is_secret_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    let parts: Vec<&str> =
        lower.split('/').filter(|part| !part.is_empty()).collect();
    let Some(name) = parts.last() else {
        return false;
    };

    let in_secret_folder =
        parts.iter().any(|part| SECRET_FOLDERS.contains(part));
    let is_secret_pair = parts
        .windows(2)
        .any(|pair| SECRET_PAIRS.iter().any(|secret| pair == secret));
    let is_secret_file = SECRET_NAMES.contains(name)
        || SECRET_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || SECRET_ENDINGS.iter().any(|ending| name.ends_with(ending));
    let may_match_hidden = parts
        .iter()
        .any(|part| part.starts_with('.') && part.contains(['*', '?', '[']));

    in_secret_folder
        || is_secret_pair
        || is_secret_file
        || may_match_hidden
        || may_match_env(name)
}

/// A glob that may match an `.env` file: `*.env`, `*.en?`, `*env`, `app.e*`.
/// A glob with nothing to go on, `*` or `*.*`, stays allowed, or every folder
/// listing would be refused.
fn may_match_env(name: &str) -> bool {
    let extension = name.rsplit('.').next().unwrap_or(name);
    name.contains(['*', '?', '['])
        && extension != "*"
        && glob_matches(extension.as_bytes(), b"env")
}

fn glob_matches(glob: &[u8], text: &[u8]) -> bool {
    match glob.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => {
            (0..=text.len()).any(|start| glob_matches(rest, &text[start..]))
        },
        Some((b'?', rest)) => {
            !text.is_empty() && glob_matches(rest, &text[1..])
        },
        Some((b'[', rest)) => {
            // Any character may be in the set, so count it as a match.
            let close = rest.iter().position(|c| *c == b']');
            let after = close.map_or(rest, |close| &rest[close + 1..]);
            !text.is_empty() && glob_matches(after, &text[1..])
        },
        Some((first, rest)) => {
            text.first() == Some(first) && glob_matches(rest, &text[1..])
        },
    }
}
