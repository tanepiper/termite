use termite::write_private;

use super::*;

struct Temp(Home);

impl Temp {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir()
            .join(format!("tmite-prov-{tag}-{}", std::process::id()));
        // Left over from an earlier failed run.
        let _ = fs::remove_dir_all(&root);
        let home = Home::at(root).expect("home");
        home.ensure().expect("folders");
        Self(home)
    }

    fn config(&self, text: &str) {
        write_private(self.0.root(), CONFIG, text.as_bytes(), 0o600)
            .expect("config");
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // A test that failed may leave it behind; nothing depends on it.
        let _ = fs::remove_dir_all(self.0.root());
    }
}

#[test]
fn given_a_changed_config_then_it_is_a_problem_until_accepted() {
    let temp = Temp::new("config");
    temp.config("one");
    record(&temp.0, Event::Installed, CONFIG, b"one", vec![], "test")
        .expect("logs");
    assert_eq!(config_problem(&temp.0), None);

    temp.config("two");
    assert!(config_problem(&temp.0).is_some());

    let text = accept(&temp.0, CONFIG, &Prompt::answering("n\n"), &|_| {})
        .expect("asks");
    assert!(text.contains("Nothing changed"));
    assert!(config_problem(&temp.0).is_some());

    accept(&temp.0, CONFIG, &Prompt::answering("y\n"), &|_| {})
        .expect("accepts");
    assert_eq!(config_problem(&temp.0), None);
}

#[test]
fn given_an_untracked_config_then_termite_still_works() {
    let temp = Temp::new("untracked");
    temp.config("one");

    assert_eq!(config_problem(&temp.0), None);
}

#[test]
fn given_no_terminal_then_accept_changes_nothing() {
    let temp = Temp::new("noterm");
    temp.config("one");

    let result = accept(&temp.0, CONFIG, &Prompt::without_terminal(), &|_| {});

    assert!(result.is_err());
    assert!(entries(&temp.0).expect("entries").is_empty());
}

#[test]
fn given_a_path_outside_the_folder_then_accept_refuses() {
    let temp = Temp::new("paths");
    for path in [
        "../x",
        "/etc/passwd",
        "bin/../../x",
        "provenance.jsonl",
        "drafts/x",
    ] {
        let result = accept(&temp.0, path, &Prompt::answering("y\n"), &|_| {});

        assert!(result.is_err(), "{path}");
    }
}

#[test]
fn given_files_then_a_list_note_says_what_is_wrong() {
    let temp = Temp::new("notes");
    record(&temp.0, Event::Installed, "bin/one", b"one", vec![], "test")
        .expect("logs");

    assert_eq!(file_note(&temp.0, "bin/one", b"one"), "");
    assert!(file_note(&temp.0, "bin/one", b"two").contains("changed"));
    assert!(file_note(&temp.0, "bin/other", b"x").contains("no record"));
}

#[test]
fn given_a_verified_log_then_the_report_states_its_limits() {
    let temp = Temp::new("verify");
    record(&temp.0, Event::Installed, "bin/one", b"one", vec![], "test")
        .expect("logs");

    let text = verify(&temp.0).expect("verifies");

    assert!(
        text.contains("1 entries") && text.contains("removed"),
        "{text}"
    );
    assert!(text.contains("forge"), "{text}");
}

#[test]
fn given_an_alert_then_it_is_listed_without_the_request() {
    let temp = Temp::new("alerts");
    let (key, _) = key(&temp.0).expect("key");
    Log::at(temp.0.root())
        .append(
            &key,
            Record::alert(
                "my private words",
                vec!["wrecks".to_owned()],
                "withheld",
                "test",
            ),
        )
        .expect("logs");

    let text = recent_alerts(&temp.0).expect("alerts");

    assert!(
        text.contains("withheld") && text.contains("wrecks"),
        "{text}"
    );
    assert!(!text.contains("private"), "{text}");
}
