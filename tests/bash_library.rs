//! Runs the bash library's own checks (shellcheck, then bats) as part of
//! `cargo test`. Skipped, with a message, when either tool is missing.

use std::path::Path;
use std::process::Command;

fn installed(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn bash_library_passes_shellcheck_and_bats() {
    let missing: Vec<&str> = ["bats", "shellcheck"]
        .into_iter()
        .filter(|program| !installed(program))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "skipped: install {} to test the bash library",
            missing.join(" and ")
        );
        return;
    }

    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("bash/tests/run.sh");
    let output = Command::new("bash")
        .arg(&script)
        .output()
        .expect("bash runs");

    assert!(
        output.status.success(),
        "bash library checks failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
