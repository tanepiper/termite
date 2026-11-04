//! Spots arguments that name, or may match, files that hold secrets.

use super::PolicyRefusal;

/// Folders whose contents are all secret.
const SECRET_FOLDERS: &[&str] = &[".ssh", ".aws"];

/// Folder and file pairs that are secret wherever they appear.
const SECRET_PAIRS: &[[&str; 2]] =
    &[[".config", "gh"], [".docker", "config.json"]];

/// File name endings that are secret.
const SECRET_ENDINGS: &[&str] = &[".pem", ".key", ".kdbx"];

/// Refuse any argument, or `--option=value` value, that may be a secret
/// file.
pub(super) fn check_arguments(
    arguments: &[String],
) -> Result<(), PolicyRefusal> {
    for argument in arguments {
        let path = match argument.split_once('=') {
            Some((_, value)) if argument.starts_with('-') => value,
            _ if argument.starts_with('-') => continue,
            _ => argument.as_str(),
        };

        if is_secret_path(path) {
            return Err(PolicyRefusal::ReadsSecret {
                path: path.to_owned(),
            });
        }
    }

    Ok(())
}

/// Match by path parts, so `~/.ssh`, `$HOME/.ssh` and `/Users/me/.ssh` are
/// all caught without expanding anything. A glob in a part starting with
/// `.` may match a hidden secret folder, so it counts as secret too.
fn is_secret_path(path: &str) -> bool {
    let parts: Vec<&str> =
        path.split('/').filter(|part| !part.is_empty()).collect();
    let Some(name) = parts.last() else {
        return false;
    };

    let in_secret_folder =
        parts.iter().any(|part| SECRET_FOLDERS.contains(part));
    let is_secret_pair = parts
        .windows(2)
        .any(|pair| SECRET_PAIRS.iter().any(|secret| pair == secret));
    let is_secret_file = *name == ".netrc"
        || name.starts_with(".env")
        || SECRET_ENDINGS.iter().any(|ending| name.ends_with(ending));
    let may_match_hidden = parts
        .iter()
        .any(|part| part.starts_with('.') && part.contains(['*', '?', '[']));

    in_secret_folder || is_secret_pair || is_secret_file || may_match_hidden
}
