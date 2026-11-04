//! Writing files only the owner can read, without a moment where they are
//! readable by others or half written.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions, Permissions};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process;
use std::time::SystemTime;

use super::HomeError;

/// Write `bytes` to `folder/name` with `mode` (for example 0o600 or 0o700):
///
/// 1. open `.name.tmp-<random>` in the same folder with `O_CREAT|O_EXCL|
///    O_NOFOLLOW` and the final mode, so nothing else can have it open and
///    it is never readable by others;
/// 2. set the mode again on the open file, since the umask may have
///    removed bits;
/// 3. write, `fsync`, rename into place, `fsync` the folder.
///
/// The target is replaced if it is a file. A symlink at the target is
/// refused. On any error the temporary file is removed. Check the folder
/// with [`Home::check`](super::Home::check) first.
pub fn write_private(
    folder: &Path,
    name: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<PathBuf, HomeError> {
    if !is_plain_file_name(name) {
        return Err(HomeError::BadFileName {
            name: name.to_owned(),
        });
    }
    let target = folder.join(name);
    if fs::symlink_metadata(&target)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(HomeError::Unsafe {
            path: target,
            problem: super::Problem::Symlink,
        });
    }

    let temporary = folder.join(format!(".{name}.tmp-{:016x}", random()));
    let result = write_then_rename(&temporary, &target, bytes, mode);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;

    if let Ok(folder_handle) = File::open(folder) {
        let _ = folder_handle.sync_all();
    }
    Ok(target)
}

fn write_then_rename(
    temporary: &Path,
    target: &Path,
    bytes: &[u8],
    mode: u32,
) -> Result<(), HomeError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(temporary)
        .map_err(HomeError::io("create", temporary))?;
    file.set_permissions(Permissions::from_mode(mode))
        .map_err(HomeError::io("set the mode of", temporary))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(HomeError::io("write", temporary))?;
    drop(file);

    fs::rename(temporary, target).map_err(HomeError::io("replace", target))
}

/// A name with no folder parts: not empty, not `.` or `..`, no `/` or NUL.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\0'])
        && Path::new(name).file_name() == Some(OsStr::new(name))
}

/// Enough randomness to avoid clashing with another termite process.
/// `create_new` makes a clash fail safely rather than share a file.
fn random() -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u32(process::id());
    if let Ok(elapsed) =
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
    {
        hasher.write_u128(elapsed.as_nanos());
    }
    hasher.finish()
}
