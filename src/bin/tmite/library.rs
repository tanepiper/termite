//! The bash library that scripts load, built into `tmite` and installed from
//! it. Contract: files are owner-only, and a newer library is never replaced
//! by an older one.

use std::fs;

use termite::{Event, Home, PRIVATE_FILE_MODE, write_private};

use crate::provenance;

const FILES: [(&str, &str); 8] = [
    ("VERSION", include_str!("../../../bash/lib/termite/VERSION")),
    (
        "README.md",
        include_str!("../../../bash/lib/termite/README.md"),
    ),
    ("core.sh", include_str!("../../../bash/lib/termite/core.sh")),
    ("log.sh", include_str!("../../../bash/lib/termite/log.sh")),
    ("args.sh", include_str!("../../../bash/lib/termite/args.sh")),
    (
        "require.sh",
        include_str!("../../../bash/lib/termite/require.sh"),
    ),
    ("tmp.sh", include_str!("../../../bash/lib/termite/tmp.sh")),
    (
        "confirm.sh",
        include_str!("../../../bash/lib/termite/confirm.sh"),
    ),
];

fn version(text: &str) -> u32 {
    text.trim().parse().unwrap_or(0)
}

/// Install the library, or update an older one. `home` must be checked first.
pub(crate) fn install(home: &Home) -> Result<(), String> {
    let folder = home.lib();
    let installed = fs::read_to_string(folder.join("VERSION"))
        .map(|text| version(&text))
        .unwrap_or(0);
    if installed > version(FILES[0].1) {
        return Ok(());
    }
    for (name, text) in FILES {
        let current = fs::read(folder.join(name)).unwrap_or_default();
        if current == text.as_bytes() {
            continue;
        }
        write_private(&folder, name, text.as_bytes(), PRIVATE_FILE_MODE)
            .map_err(|error| error.to_string())?;
        provenance::record(
            home,
            Event::LibraryUpdated,
            &format!("lib/termite/{name}"),
            text.as_bytes(),
            Vec::new(),
            "tmite",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn temp_home(tag: &str) -> Home {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let root = std::env::temp_dir()
            .join(format!("tmite-lib-{tag}-{}", std::process::id()));
        // Left over from an earlier failed run.
        let _ = fs::remove_dir_all(&root);
        let home = Home::at(root).expect("home");
        home.ensure().expect("folders");
        home
    }

    #[test]
    fn given_no_library_then_every_file_is_installed_owner_only() {
        let home = temp_home("new");

        install(&home).expect("installs");

        for (name, text) in FILES {
            let path = home.lib().join(name);
            let mode = fs::metadata(&path).expect("file").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
            assert_eq!(fs::read_to_string(&path).expect("text"), text);
        }
        fs::remove_dir_all(home.root()).expect("clean up");
    }

    #[test]
    fn given_a_newer_library_then_it_is_left_alone() {
        let home = temp_home("newer");
        write_private(&home.lib(), "VERSION", b"999\n", 0o600).expect("write");

        install(&home).expect("installs");

        let text =
            fs::read_to_string(home.lib().join("VERSION")).expect("text");
        assert_eq!(text, "999\n");
        fs::remove_dir_all(home.root()).expect("clean up");
    }
}
