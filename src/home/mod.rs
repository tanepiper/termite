//! termite's own folder, `$TERMITE_HOME` (default `~/.config/termite`):
//! where it is, checking that nobody else can change it, and writing files
//! into it that only the owner can read.
//!
//! ```text
//! $TERMITE_HOME         0700
//! ├── config.json
//! ├── bin/              0700  installed scripts, 0700
//! ├── lib/              0700
//! │   └── termite/      0700  the bash library, 0600
//! └── drafts/           0700  scripts waiting for review, 0600
//! ```

mod names;
mod write;

use std::env;
use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub use self::names::{NameTaken, ScriptName};
pub use self::write::write_private;

/// The environment variable that moves termite's folder.
pub const TERMITE_HOME_VAR: &str = "TERMITE_HOME";

/// Mode for termite's folders and installed scripts.
pub const PRIVATE_DIR_MODE: u32 = 0o700;

/// Mode for drafts and library files.
pub const PRIVATE_FILE_MODE: u32 = 0o600;

/// What is wrong with a folder or file termite relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Missing,
    NotAFolder,
    Symlink,
    NotYours,
    WritableByOthers,
}

impl Problem {
    fn describe(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::NotAFolder => "not a folder",
            Self::Symlink => "a symlink",
            Self::NotYours => "not owned by you",
            Self::WritableByOthers => "writable by other users",
        }
    }
}

/// Why termite's folder cannot be used or written.
#[derive(Debug, thiserror::Error)]
pub enum HomeError {
    #[error("HOME is not set, and neither is TERMITE_HOME")]
    NoHome,

    #[error("TERMITE_HOME must be an absolute path, not {}", path.display())]
    NotAbsolute { path: PathBuf },

    #[error("{} is {}", path.display(), problem.describe())]
    Unsafe { path: PathBuf, problem: Problem },

    #[error("cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },

    #[error("{name:?} cannot be used as a file name here")]
    BadFileName { name: String },
}

impl HomeError {
    pub(crate) fn io(
        action: &'static str,
        path: &Path,
    ) -> impl FnOnce(io::Error) -> Self {
        let path = path.to_owned();
        move |source| Self::Io {
            action,
            path,
            source,
        }
    }
}

/// termite's folder and the folders inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// `$TERMITE_HOME` when set and not empty, else `$HOME/.config/termite`.
    pub fn from_env() -> Result<Self, HomeError> {
        Self::resolve(
            env::var_os(TERMITE_HOME_VAR).as_deref(),
            env::var_os("HOME").as_deref(),
        )
    }

    fn resolve(
        termite_home: Option<&std::ffi::OsStr>,
        home: Option<&std::ffi::OsStr>,
    ) -> Result<Self, HomeError> {
        let root = match termite_home.filter(|value| !value.is_empty()) {
            Some(value) => PathBuf::from(value),
            None => home
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config/termite"))
                .ok_or(HomeError::NoHome)?,
        };
        if !root.is_absolute() {
            return Err(HomeError::NotAbsolute { path: root });
        }

        Ok(Self { root })
    }

    /// A home at `root`, which must be absolute.
    pub fn at(root: impl Into<PathBuf>) -> Result<Self, HomeError> {
        let root = root.into();
        if !root.is_absolute() {
            return Err(HomeError::NotAbsolute { path: root });
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }

    /// Installed scripts; on `PATH`.
    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// The bash library's folder, `lib/termite`.
    pub fn lib(&self) -> PathBuf {
        self.root.join("lib/termite")
    }

    /// Scripts waiting for review.
    pub fn drafts(&self) -> PathBuf {
        self.root.join("drafts")
    }

    /// Every folder termite uses, parents first.
    fn folders(&self) -> [PathBuf; 5] {
        [
            self.root.clone(),
            self.bin(),
            self.root.join("lib"),
            self.lib(),
            self.drafts(),
        ]
    }

    /// Check that every folder exists, is a real folder, belongs to the
    /// current user and is not writable by anyone else. Changes nothing.
    pub fn check(&self) -> Result<(), HomeError> {
        self.check_as(current_uid())
    }

    fn check_as(&self, uid: u32) -> Result<(), HomeError> {
        self.folders()
            .iter()
            .try_for_each(|folder| check_folder(folder, uid))
    }

    /// Create the folders that are missing, with mode 0700, then
    /// [`check`](Self::check) them all. Existing folders are not changed.
    pub fn ensure(&self) -> Result<(), HomeError> {
        for folder in self.folders() {
            match fs::symlink_metadata(&folder) {
                Ok(_) => {},
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    create_private_folder(&folder)?;
                },
                Err(error) => {
                    return Err(HomeError::io("read", &folder)(error));
                },
            }
        }
        self.check()
    }
}

/// The effective user id of this process.
pub(crate) fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// Check one folder: real (not a symlink), owned by `uid`, not writable by
/// group or others.
pub(crate) fn check_folder(path: &Path, uid: u32) -> Result<(), HomeError> {
    let unsafe_because = |problem| HomeError::Unsafe {
        path: path.to_owned(),
        problem,
    };
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(unsafe_because(Problem::Missing));
        },
        Err(error) => return Err(HomeError::io("read", path)(error)),
    };

    if metadata.file_type().is_symlink() {
        return Err(unsafe_because(Problem::Symlink));
    }
    if !metadata.is_dir() {
        return Err(unsafe_because(Problem::NotAFolder));
    }
    if metadata.uid() != uid {
        return Err(unsafe_because(Problem::NotYours));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(unsafe_because(Problem::WritableByOthers));
    }
    Ok(())
}

/// Create one folder (its parent must exist or be creatable) and set 0700
/// explicitly, since the umask may have removed bits.
fn create_private_folder(folder: &Path) -> Result<(), HomeError> {
    DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(folder)
        .map_err(HomeError::io("create", folder))?;
    fs::set_permissions(folder, Permissions::from_mode(PRIVATE_DIR_MODE))
        .map_err(HomeError::io("set the mode of", folder))
}

#[cfg(test)]
pub(crate) mod tests;
