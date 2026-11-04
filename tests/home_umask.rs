//! termite's folders and files get their exact modes whatever the umask.
//! The umask belongs to the whole process, so this is its own test binary
//! with a single test.

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use termite::{Home, write_private};

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("exists").permissions().mode() & 0o7777
}

#[test]
fn modes_do_not_depend_on_the_umask() {
    let base = std::env::temp_dir()
        .join(format!("termite-umask-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir(&base).expect("base");
    fs::set_permissions(&base, Permissions::from_mode(0o700)).expect("mode");

    for umask in [0o000, 0o022, 0o077, 0o777] {
        // SAFETY: umask only sets the process file mode mask.
        let previous = unsafe { libc::umask(umask) };
        let home =
            Home::at(base.join(format!("home-{umask:o}"))).expect("absolute");
        let result = home.ensure().and_then(|()| {
            let draft = write_private(&home.drafts(), "d", b"x", 0o600)?;
            let script = write_private(&home.bin(), "s", b"x", 0o700)?;
            Ok((draft, script))
        });
        // SAFETY: as above.
        unsafe { libc::umask(previous) };

        let (draft, script) = result.expect("written");
        assert_eq!(mode(home.root()), 0o700, "umask {umask:o}");
        assert_eq!(mode(&home.bin()), 0o700, "umask {umask:o}");
        assert_eq!(mode(&home.lib()), 0o700, "umask {umask:o}");
        assert_eq!(mode(&draft), 0o600, "umask {umask:o}");
        assert_eq!(mode(&script), 0o700, "umask {umask:o}");
    }

    fs::remove_dir_all(&base).expect("cleaned up");
}
