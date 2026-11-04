//! termite's Semgrep rules (semgrep/) pass their own tests and find
//! nothing in the code. Skipped, with a note, when Semgrep is not
//! installed; scripts/sonar.sh and CI always run it.

use std::path::Path;
use std::process::Command;

#[test]
fn termite_rules_pass_and_find_nothing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if Command::new("semgrep").arg("--version").output().is_err() {
        eprintln!("skipped: semgrep is not installed (brew install semgrep)");
        return;
    }

    let output = Command::new(root.join("scripts/semgrep.sh"))
        .arg("--local")
        .current_dir(root)
        .output()
        .expect("scripts/semgrep.sh runs");
    assert!(
        output.status.success(),
        "scripts/semgrep.sh --local failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
