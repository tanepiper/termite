use std::ffi::OsStr;
use std::fs::{self, Permissions};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::{
    Home, HomeError, NameTaken, Problem, ScriptName, create_private,
    current_uid, write_private,
};

/// A private folder under the system temp folder, removed when dropped.
pub(crate) struct TempFolder(PathBuf);

impl TempFolder {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        // Test-only: the folder is made fresh with mode 0700 below, and the
        // tests check modes and ownership themselves.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let path = std::env::temp_dir().join(format!(
            "termite-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).expect("temp folder");
        fs::set_permissions(&path, Permissions::from_mode(0o700))
            .expect("temp folder mode");
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path)
        .expect("exists")
        .permissions()
        .mode()
        & 0o7777
}

fn unsafe_problem(result: Result<(), HomeError>) -> (PathBuf, Problem) {
    match result {
        Err(HomeError::Unsafe { path, problem }) => (path, problem),
        other => panic!("expected an unsafe folder, got {other:?}"),
    }
}

#[test]
fn given_env_when_home_is_resolved_then_termite_home_wins_and_must_be_absolute()
{
    let home = Home::resolve(Some(OsStr::new("/t")), Some(OsStr::new("/u")))
        .expect("absolute");
    assert_eq!(home.root(), Path::new("/t"));
    assert_eq!(home.config_file(), Path::new("/t/config.json"));
    assert_eq!(home.lib(), Path::new("/t/lib/termite"));

    let default = Home::resolve(Some(OsStr::new("")), Some(OsStr::new("/u")))
        .expect("falls back to HOME");
    assert_eq!(default.root(), Path::new("/u/.config/termite"));

    assert!(matches!(
        Home::resolve(Some(OsStr::new("rel")), None),
        Err(HomeError::NotAbsolute { .. })
    ));
    assert!(matches!(Home::resolve(None, None), Err(HomeError::NoHome)));
}

#[test]
fn given_a_new_home_when_ensured_then_every_folder_is_private() {
    let temp = TempFolder::new();
    let home = Home::at(temp.path().join("termite")).expect("absolute");

    home.ensure().expect("created");

    for folder in home.folders() {
        assert_eq!(mode(&folder), 0o700, "{}", folder.display());
    }
    home.check().expect("checks pass");
}

#[test]
fn given_an_existing_folder_when_ensured_then_it_is_not_changed() {
    let temp = TempFolder::new();
    let home = Home::at(temp.path().join("termite")).expect("absolute");
    fs::create_dir(home.root()).expect("root");
    fs::set_permissions(home.root(), Permissions::from_mode(0o755))
        .expect("mode");

    home.ensure().expect("0755 is not writable by others");

    assert_eq!(mode(home.root()), 0o755);
}

#[test]
fn given_unsafe_folders_when_checked_then_the_folder_and_problem_are_named() {
    let temp = TempFolder::new();
    let home = Home::at(temp.path().join("termite")).expect("absolute");
    home.ensure().expect("created");

    fs::set_permissions(home.bin(), Permissions::from_mode(0o770))
        .expect("mode");
    assert_eq!(
        unsafe_problem(home.check()),
        (home.bin(), Problem::WritableByOthers)
    );
    fs::set_permissions(home.bin(), Permissions::from_mode(0o702))
        .expect("mode");
    assert_eq!(
        unsafe_problem(home.check()),
        (home.bin(), Problem::WritableByOthers)
    );
    fs::set_permissions(home.bin(), Permissions::from_mode(0o700))
        .expect("mode");

    fs::remove_dir(home.drafts()).expect("removed");
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).expect("created");
    symlink(&elsewhere, home.drafts()).expect("symlink");
    assert_eq!(
        unsafe_problem(home.check()),
        (home.drafts(), Problem::Symlink)
    );
    fs::remove_file(home.drafts()).expect("removed");
    fs::write(home.drafts(), "").expect("a file");
    assert_eq!(
        unsafe_problem(home.check()),
        (home.drafts(), Problem::NotAFolder)
    );
    fs::remove_file(home.drafts()).expect("removed");
    assert_eq!(
        unsafe_problem(home.check()),
        (home.drafts(), Problem::Missing)
    );
}

#[test]
fn given_a_folder_owned_by_someone_else_when_checked_then_it_is_refused() {
    let temp = TempFolder::new();
    let home = Home::at(temp.path().join("termite")).expect("absolute");
    home.ensure().expect("created");

    let someone_else = current_uid().wrapping_add(1);

    assert_eq!(
        unsafe_problem(home.check_as(someone_else)),
        (home.root().to_owned(), Problem::NotYours)
    );
}

#[test]
fn given_a_file_when_written_privately_then_it_has_the_mode_and_contents() {
    let temp = TempFolder::new();

    let path =
        write_private(temp.path(), "draft", b"one", 0o600).expect("written");
    assert_eq!(fs::read(&path).expect("read"), b"one");
    assert_eq!(mode(&path), 0o600);

    let path =
        write_private(temp.path(), "draft", b"two", 0o700).expect("replaced");
    assert_eq!(fs::read(&path).expect("read"), b"two");
    assert_eq!(mode(&path), 0o700);

    let names: Vec<_> = fs::read_dir(temp.path())
        .expect("list")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["draft"], "no temporary files left");
}

#[test]
fn given_a_failing_write_when_written_then_no_temporary_file_is_left() {
    let temp = TempFolder::new();
    fs::create_dir(temp.path().join("taken")).expect("a folder in the way");

    let result = write_private(temp.path(), "taken", b"x", 0o600);

    assert!(matches!(result, Err(HomeError::Io { .. })), "{result:?}");
    let names: Vec<_> = fs::read_dir(temp.path())
        .expect("list")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, ["taken"]);
}

#[test]
fn given_a_symlink_at_the_target_when_written_then_it_is_refused() {
    let temp = TempFolder::new();
    let victim = temp.path().join("victim");
    fs::write(&victim, "keep").expect("victim");
    symlink(&victim, temp.path().join("script")).expect("symlink");

    let result = write_private(temp.path(), "script", b"x", 0o700);

    assert!(
        matches!(
            result,
            Err(HomeError::Unsafe {
                problem: Problem::Symlink,
                ..
            })
        ),
        "{result:?}"
    );
    assert_eq!(fs::read(&victim).expect("read"), b"keep");
}

#[test]
fn given_a_new_log_when_created_privately_then_folder_and_file_are_private() {
    use std::io::Write;

    let temp = TempFolder::new();
    let folder = temp.path().join(".cache/termite");
    let mut log = create_private(&folder, "server.log").expect("created");
    log.write_all(b"first run").expect("written");
    drop(log);
    assert_eq!(mode(&folder), 0o700);
    assert_eq!(mode(&folder.join("server.log")), 0o600);

    let log = create_private(&folder, "server.log").expect("created again");
    drop(log);
    assert_eq!(fs::read(folder.join("server.log")).expect("read"), b"");
}

#[test]
fn given_a_symlink_or_open_folder_when_a_log_is_created_then_it_is_refused() {
    let temp = TempFolder::new();
    let victim = temp.path().join("victim");
    fs::write(&victim, "keep").expect("victim");
    symlink(&victim, temp.path().join("server.log")).expect("symlink");

    assert!(create_private(temp.path(), "server.log").is_err());
    assert_eq!(fs::read_to_string(&victim).expect("read"), "keep");

    let open = temp.path().join("open");
    fs::create_dir(&open).expect("folder");
    fs::set_permissions(&open, Permissions::from_mode(0o777)).expect("mode");
    let (_, problem) =
        unsafe_problem(create_private(&open, "server.log").map(drop));
    assert_eq!(problem, Problem::WritableByOthers);
    assert!(create_private(temp.path(), "../escape.log").is_err());
}

#[test]
fn given_names_with_folder_parts_when_written_then_they_are_refused() {
    let temp = TempFolder::new();
    for name in ["", ".", "..", "a/b", "../escape", "nul\0"] {
        assert!(
            matches!(
                write_private(temp.path(), name, b"x", 0o600),
                Err(HomeError::BadFileName { .. })
            ),
            "{name:?}"
        );
    }
}

#[test]
fn given_names_when_parsed_then_only_plain_names_are_accepted() {
    let good = ["ab", "notes-backup", "disk2-report", &"a".repeat(40)];
    for name in good {
        assert!(ScriptName::parse(name).is_ok(), "{name:?} is fine");
    }

    let bad = [
        "",
        "a",
        "Notes",
        "2fast",
        "-dash",
        "dash-",
        "two--dashes",
        "a_b",
        "a.b",
        "a b",
        "a/b",
        "ünï",
        &"a".repeat(41),
    ];
    for name in bad {
        assert!(
            matches!(ScriptName::parse(name), Err(NameTaken::Malformed { .. })),
            "{name:?} is turned down"
        );
    }
}

#[test]
fn given_names_that_shadow_something_when_checked_then_they_are_turned_down() {
    let temp = TempFolder::new();
    let system = temp.path().join("system");
    let bin = temp.path().join("bin");
    fs::create_dir(&system).expect("system");
    fs::create_dir(&bin).expect("bin");
    for (folder, name, mode) in [
        (&system, "tool", 0o755),
        (&system, "notes", 0o644),
        (&bin, "mine", 0o700),
    ] {
        let path = folder.join(name);
        fs::write(&path, "").expect("program");
        fs::set_permissions(&path, Permissions::from_mode(mode)).expect("mode");
    }
    let path = std::env::join_paths([&system, &bin]).expect("PATH");
    let check = |name: &str| {
        ScriptName::parse(name)
            .expect("well formed")
            .check_free_in(Some(&path), &bin)
    };

    assert!(matches!(check("cd"), Err(NameTaken::ShellWord { .. })));
    assert!(matches!(check("if"), Err(NameTaken::ShellWord { .. })));
    assert!(matches!(check("whence"), Err(NameTaken::ShellWord { .. })));
    assert_eq!(
        check("tool"),
        Err(NameTaken::Program {
            name: "tool".to_owned(),
            program: system.join("tool"),
        })
    );
    assert_eq!(check("notes"), Ok(()), "not executable, so not a program");
    assert_eq!(check("mine"), Ok(()), "already one of your scripts");
    assert_eq!(check("fresh-name"), Ok(()));
}
