//! The key that signs the provenance log. It lives in the macOS Keychain or the
//! Linux Secret Service, and in a 0600 file when neither can be used. A custom
//! `TERMITE_HOME` always uses the file, because the keychain holds one key.

use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use termite::{Home, PRIVATE_FILE_MODE, write_private};

use crate::script_tools::run_tool;

const SERVICE: &str = "termite-provenance";
const KEY_FILE: &str = "provenance.key";
const KEY_BYTES: usize = 32;

/// Where the key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyPlace {
    SystemStore,
    File,
}

/// The log's key, made the first time it's needed.
pub(crate) fn key(home: &Home) -> Result<(Vec<u8>, KeyPlace), String> {
    let file = home.root().join(KEY_FILE);
    if file.exists() || !is_default_home(home) {
        return from_file(home).map(|key| (key, KeyPlace::File));
    }
    match from_system_store()? {
        Some(key) => Ok((key, KeyPlace::SystemStore)),
        None => from_file(home).map(|key| (key, KeyPlace::File)),
    }
}

fn is_default_home(home: &Home) -> bool {
    std::env::var_os("HOME")
        .map(|user| PathBuf::from(user).join(".config/termite"))
        .is_some_and(|default| default == home.root())
}

fn new_key() -> Result<String, String> {
    let mut bytes = [0_u8; KEY_BYTES];
    File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(|error| format!("cannot make a key: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn from_file(home: &Home) -> Result<Vec<u8>, String> {
    let path = home.root().join(KEY_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(text.trim().as_bytes().to_vec()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = new_key()?;
            write_private(
                home.root(),
                KEY_FILE,
                key.as_bytes(),
                PRIVATE_FILE_MODE,
            )
            .map_err(|error| error.to_string())?;
            Ok(key.into_bytes())
        },
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// `Ok(None)` means there's no store on this machine. A store that's there
/// but won't give up the key is an error: a second key would break the log.
fn from_system_store() -> Result<Option<Vec<u8>>, String> {
    let store = if cfg!(target_os = "macos") {
        Store::Keychain
    } else {
        Store::SecretService
    };
    let Some(found) = store.find() else {
        return Ok(None);
    };
    if let Some(key) = found {
        return Ok(Some(key));
    }
    store.add(&new_key()?);
    match store.find() {
        Some(Some(key)) => Ok(Some(key)),
        _ => Err(format!("{} wouldn't store the key", store.name())),
    }
}

#[derive(Clone, Copy)]
enum Store {
    Keychain,
    SecretService,
}

impl Store {
    fn name(self) -> &'static str {
        match self {
            Self::Keychain => "the Keychain",
            Self::SecretService => "the Secret Service",
        }
    }

    /// `None` when the tool isn't there, `Some(None)` when it has no key.
    fn find(self) -> Option<Option<Vec<u8>>> {
        let mut command = match self {
            Self::Keychain => {
                let mut command = Command::new("security");
                command.args([
                    "find-generic-password",
                    "-s",
                    SERVICE,
                    "-a",
                    "termite",
                    "-w",
                ]);
                command
            },
            Self::SecretService => {
                let mut command = Command::new("secret-tool");
                command.args(["lookup", "service", SERVICE]);
                command
            },
        };
        let (passed, output) = run_tool(&mut command, "").ok()?;
        let key = output.trim();
        Some((passed && !key.is_empty()).then(|| key.as_bytes().to_vec()))
    }

    /// The key goes in on standard input, because other users can read a
    /// process's arguments. Whether it worked is found by reading it back.
    fn add(self, key: &str) {
        let (mut command, input) = match self {
            Self::Keychain => {
                let mut command = Command::new("security");
                command.arg("-i");
                let line = format!(
                    "add-generic-password -s {SERVICE} -a termite -w {key}\n"
                );
                (command, line)
            },
            Self::SecretService => {
                let mut command = Command::new("secret-tool");
                command.args(["store", "--label=termite", "service", SERVICE]);
                (command, key.to_owned())
            },
        };
        // The read that follows says how this went.
        let _ = run_tool(&mut command, &input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_custom_home_then_the_key_is_a_private_file_and_stays_the_same() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir()
            .join(format!("tmite-key-{}", std::process::id()));
        // Left over from an earlier failed run.
        let _ = fs::remove_dir_all(&root);
        let home = Home::at(&root).expect("home");
        home.ensure().expect("folders");

        let (first, place) = key(&home).expect("key");
        let (second, _) = key(&home).expect("key");

        assert_eq!(place, KeyPlace::File);
        assert_eq!(first, second);
        assert_eq!(first.len(), KEY_BYTES * 2);
        let mode = fs::metadata(root.join(KEY_FILE))
            .expect("file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        fs::remove_dir_all(&root).expect("clean up");
    }
}
