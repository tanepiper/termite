use std::fs;
use std::path::PathBuf;

use super::{Change, Place, State, install, remove, state};
use crate::hooks::harness::{HARNESSES, Harness};
use crate::hooks::json::Json;

fn place(name: &str) -> Place {
    // Test-only: nothing here is trusted, and it is removed afterwards.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let temp = std::env::temp_dir();
    let home = temp.join(format!("termite-plan-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).expect("home");
    Place {
        home,
        program: PathBuf::from("/opt/my tools/tmite"),
    }
}

fn harness(id: &str) -> &'static Harness {
    Harness::find(id).expect("a harness")
}

/// Write out a plan, as `apply` does.
fn write(changes: &[Change]) {
    for change in changes {
        match &change.new {
            Some(text) => {
                fs::create_dir_all(change.path.parent().expect("parent"))
                    .expect("folder");
                fs::write(&change.path, text).expect("file");
            },
            None => fs::remove_file(&change.path).expect("gone"),
        }
    }
}

#[test]
fn given_no_settings_then_every_harness_installs_and_removes_cleanly() {
    for harness in HARNESSES {
        let place = place(&format!("cycle-{}", harness.id));
        assert_eq!(state(harness, &place), Ok(State::Absent), "{}", harness.id);

        write(&install(harness, &place).expect("install"));
        assert_eq!(
            state(harness, &place),
            Ok(State::Current),
            "{}",
            harness.id
        );
        assert_eq!(install(harness, &place), Ok(Vec::new()), "{}", harness.id);

        write(&remove(harness, &place).expect("remove"));
        assert_eq!(state(harness, &place), Ok(State::Absent), "{}", harness.id);
        assert_eq!(remove(harness, &place), Ok(Vec::new()), "{}", harness.id);

        fs::remove_dir_all(&place.home).expect("clean up");
    }
}

const CLAUDE: &str = r#"{
  "zebra": true,
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit",
        "hooks": [
          {
            "type": "command",
            "command": "my-lint"
          }
        ]
      }
    ],
    "Stop": []
  },
  "apple": 1
}
"#;

#[test]
fn given_other_hooks_then_they_survive_in_order_and_removal_restores_the_file()
{
    let harness = harness("claude");
    let place = place("kept");
    let path = harness.path_in(&place.home);
    fs::create_dir_all(path.parent().expect("parent")).expect("folder");
    fs::write(&path, CLAUDE).expect("file");

    write(&install(harness, &place).expect("install"));
    let text = fs::read_to_string(&path).expect("file");
    assert!(text.contains("my-lint"), "{text}");
    assert!(text.find("zebra") < text.find("apple"), "{text}");
    assert!(text.contains("\"onFailure\": \"block\""), "{text}");
    assert!(text.contains(r#"'/opt/my tools/tmite' hook claude || exit 2"#));

    write(&remove(harness, &place).expect("remove"));
    assert_eq!(fs::read_to_string(&path).expect("file"), CLAUDE);
    fs::remove_dir_all(&place.home).expect("clean up");
}

#[test]
fn given_a_moved_program_then_the_hook_is_different_and_install_replaces_it() {
    let harness = harness("codex");
    let mut place = place("moved");
    write(&install(harness, &place).expect("install"));

    place.program = PathBuf::from("/usr/local/bin/tmite");

    assert_eq!(state(harness, &place), Ok(State::Different));
    write(&install(harness, &place).expect("install"));
    assert_eq!(state(harness, &place), Ok(State::Current));
    let text = fs::read_to_string(harness.path_in(&place.home)).expect("file");
    assert_eq!(text.matches("hook codex").count(), 1, "{text}");
    fs::remove_dir_all(&place.home).expect("clean up");
}

#[test]
fn given_settings_with_comments_then_they_are_left_alone() {
    let harness = harness("gemini");
    let place = place("comments");
    let path = harness.path_in(&place.home);
    fs::create_dir_all(path.parent().expect("parent")).expect("folder");
    fs::write(&path, "{ // mine\n}\n").expect("file");

    let problem = install(harness, &place).expect_err("refused");

    assert!(problem.contains("by hand"), "{problem}");
    fs::remove_dir_all(&place.home).expect("clean up");
}

#[test]
fn given_a_hermes_config_then_only_the_block_changes() {
    let harness = harness("hermes");
    let place = place("hermes");
    let path = harness.path_in(&place.home);
    let original = "model:\n  default: x\nagent:\n  max_turns: 60\n";
    fs::create_dir_all(path.parent().expect("parent")).expect("folder");
    fs::write(&path, original).expect("file");

    write(&install(harness, &place).expect("install"));
    let text = fs::read_to_string(&path).expect("file");
    assert!(text.starts_with(original), "{text}");
    assert!(text.contains("fail_closed: true"), "{text}");
    assert!(text.contains("hook hermes"), "{text}");
    assert!(!text.contains("|| exit"), "{text}");

    write(&remove(harness, &place).expect("remove"));
    assert_eq!(fs::read_to_string(&path).expect("file"), original);
    fs::remove_dir_all(&place.home).expect("clean up");
}

#[test]
fn given_a_hermes_config_with_its_own_hooks_then_it_is_left_alone() {
    let harness = harness("hermes");
    let place = place("hermes-own");
    let path = harness.path_in(&place.home);
    fs::create_dir_all(path.parent().expect("parent")).expect("folder");
    fs::write(&path, "hooks:\n  on_session_start: []\n").expect("file");

    let problem = install(harness, &place).expect_err("refused");

    assert!(problem.contains("by hand"), "{problem}");
    fs::remove_dir_all(&place.home).expect("clean up");
}

/// Every harness that can be told to stop on a failed hook is told to.
#[test]
fn given_each_harness_then_a_failed_hook_stops_the_command() {
    for (id, words) in [
        ("claude", "\"onFailure\": \"block\""),
        ("cursor", "\"failClosed\": true"),
        ("codex", "|| exit 2"),
        ("gemini", "|| exit 2"),
        ("copilot", "|| exit 2"),
        ("windsurf", "|| exit 2"),
        ("hermes", "fail_closed: true"),
        ("goose", "\"on_failure\": \"block\""),
        ("opencode", "run.status !== 0"),
    ] {
        let harness = harness(id);
        let place = place(&format!("fail-{id}"));
        let changes = install(harness, &place).expect("install");
        let text: String = changes
            .iter()
            .filter_map(|change| change.new.clone())
            .collect();

        assert!(text.contains(words), "{id}: {text}");
        fs::remove_dir_all(&place.home).expect("clean up");
    }
}

#[test]
fn given_a_json_entry_then_it_parses_back() {
    for harness in HARNESSES.iter().filter(|h| h.file.ends_with("json")) {
        let place = place(&format!("parse-{}", harness.id));
        for change in install(harness, &place).expect("install") {
            Json::parse(&change.new.expect("text")).expect("json");
        }
        fs::remove_dir_all(&place.home).expect("clean up");
    }
}
