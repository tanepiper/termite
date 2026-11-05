use std::process::Command;

use super::*;

struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let root = std::env::temp_dir()
            .join(format!("tmite-profile-{tag}-{}", std::process::id()));
        // Left over from an earlier failed run.
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("folder");
        Self(root)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // A test that failed may leave it behind; nothing depends on it.
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn given_a_startup_file_then_one_block_is_added_and_the_rest_kept() {
    let temp = Temp::new("once");
    let file = temp.0.join(".zshrc");
    fs::write(&file, "alias a=b\n# keep  me\n").expect("file");

    apply(&file, SH_BLOCK).expect("first");
    apply(&file, SH_BLOCK).expect("second");

    let text = fs::read_to_string(&file).expect("text");
    assert_eq!(text.matches(START).count(), 1);
    assert!(text.starts_with("alias a=b\n# keep  me\n"));
    let backup = fs::read_to_string(backup_of(&file)).expect("backup");
    assert_eq!(backup, "alias a=b\n# keep  me\n");
}

#[test]
fn given_an_old_block_then_it_is_replaced_in_place() {
    let text = format!("before\n{START}\nold line\n{END}\nafter\n");

    let new = with_block(&text, SH_BLOCK);

    assert_eq!(new, format!("before\n{}after\n", wrapped(SH_BLOCK)));
}

#[test]
fn given_a_start_with_no_end_then_nothing_is_cut() {
    let text = format!("before\n{START}\nmy own line\n");

    let new = with_block(&text, SH_BLOCK);

    assert!(new.starts_with(&text));
}

#[test]
fn given_a_linked_file_then_the_target_changes_and_the_link_stays() {
    let temp = Temp::new("link");
    let real = temp.0.join("real-zshrc");
    let link = temp.0.join(".zshrc");
    fs::write(&real, "x\n").expect("file");
    std::os::unix::fs::symlink(&real, &link).expect("link");

    apply(&link, SH_BLOCK).expect("applies");

    assert!(fs::symlink_metadata(&link).expect("link").is_symlink());
    assert!(fs::read_to_string(&real).expect("text").contains(START));
}

#[test]
fn given_a_mode_then_it_is_kept() {
    let temp = Temp::new("mode");
    let file = temp.0.join(".zshrc");
    fs::write(&file, "x\n").expect("file");
    fs::set_permissions(&file, Permissions::from_mode(0o640)).expect("mode");

    apply(&file, SH_BLOCK).expect("applies");

    let mode = fs::metadata(&file).expect("file").permissions().mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn given_bash_then_the_login_file_is_included_only_if_it_needs_it() {
    let temp = Temp::new("bash");

    assert_eq!(blocks(Shell::Bash, &temp.0).len(), 2);

    fs::write(temp.0.join(".bash_profile"), ". ~/.bashrc\n").expect("file");
    assert_eq!(blocks(Shell::Bash, &temp.0).len(), 1);
}

#[test]
fn given_no_yes_then_nothing_changes() {
    let temp = Temp::new("no");
    let home = temp.0.clone();

    offer(&Prompt::answering("n\n"), Some(Shell::Zsh), Some(&home));

    assert!(!home.join(".zshrc").exists());
}

#[test]
fn given_the_block_in_a_shell_then_path_gains_bin_at_the_end_only_once() {
    for shell in ["bash", "zsh"] {
        let script = format!(
            "PATH=/usr/bin:/bin; HOME=/h; unset TERMITE_HOME\n{SH_BLOCK}{SH_BLOCK}printf '%s' \"$PATH\""
        );
        let Ok(output) = Command::new(shell).args(["-c", &script]).output()
        else {
            continue;
        };

        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "/usr/bin:/bin:/h/.config/termite/bin",
            "{shell}"
        );
    }
}
