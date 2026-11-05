//! Drives each real shell widget in a pseudo-terminal with a fake `tmite`, to
//! prove that the answer is explained first, that only a yes puts it at the
//! prompt, and that nothing is ever run. A shell that isn't installed is
//! skipped; zsh also does the driving.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// The command every fake answer offers: it leaves a mark if it ever runs.
const COMMAND: &str = "touch ran";

/// Starts an interactive zsh in zpty, types a request, presses the keys one
/// at a time, then presses ^T, which writes the prompt's text to a file.
const DRIVER: &str = r#"
zmodload zsh/zpty
zpty inner "cd $ZDOTDIR; HOME=$ZDOTDIR XDG_CONFIG_HOME=$ZDOTDIR TMITE_BIN=$FAKE $START"
local seen= chunk key answered=0 marks
# fish asks the terminal what it is before each prompt and each `read`, and
# waits to be told, so answer every question as a real terminal would.
drain() {
  for _ in {1..$1}; do
    sleep 0.05
    while zpty -r -t inner chunk 2>/dev/null; do seen+=$chunk; done
    marks=${seen//$'\e[0c'/$'\x01'}
    marks=${marks//[^$'\x01']/}
    while (( answered < ${#marks} )); do
      zpty -w -n inner $'\e[?62;c'
      (( answered++ ))
    done
  done
}
# Wait for the first prompt: a busy machine can take a while to get there.
for _ in {1..200}; do
  [[ $seen == *'% '* ]] && break
  drain 1
done
zpty -w -n inner "$REQUEST"$'\r'
# A long wait: these tests run side by side, and a busy machine answers late.
drain 120
for key in ${(s::)KEYS}; do
  zpty -w -n inner "$key"
  drain 30
done
zpty -w -n inner $'\x14'
drain 12
print -r -- "$seen"
zpty -d inner
"#;

/// A shell, how to start it, and what to read at startup. Each binds ^T to
/// write the prompt's text to `prompt.txt`.
struct Shell {
    name: &'static str,
    start: &'static str,
    file: &'static str,
    widget: &'static str,
    setup: &'static str,
}

const SHELLS: [Shell; 3] = [
    Shell {
        name: "zsh",
        start: "zsh -i",
        file: ".zshrc",
        widget: "shell/tmite.zsh",
        setup: "PS1='% '\ndump() { print -r -- \"$BUFFER\" > prompt.txt }\nzle -N dump\nbindkey '^T' dump\n",
    },
    Shell {
        name: "bash",
        start: "bash --noprofile --rcfile .bashrc -i",
        file: ".bashrc",
        widget: "shell/tmite.bash",
        setup: "PS1='% '\nbind -x '\"\\C-t\": printf \"%s\\n\" \"$READLINE_LINE\" > prompt.txt'\n",
    },
    Shell {
        name: "fish",
        start: "fish --no-config -C 'source tmite-rc.fish'",
        file: "tmite-rc.fish",
        widget: "shell/tmite.fish",
        setup: "function fish_prompt; printf '%% '; end\nbind \\ct 'commandline > prompt.txt'\n",
    },
];

/// Run `check` once for each shell that's here.
fn in_each_shell(check: impl Fn(&Shell)) {
    if Command::new("zsh").arg("--version").output().is_err() {
        return;
    }
    for shell in &SHELLS {
        if Command::new(shell.name).arg("--version").output().is_ok() {
            check(shell);
        }
    }
}

struct Seen {
    screen: String,
    prompt: String,
    ran: bool,
    asked: usize,
}

/// Run the widget with a fake `tmite` that prints `answer`.
fn drive(
    shell: &Shell,
    name: &str,
    request: &str,
    keys: &str,
    answer: &str,
) -> Seen {
    let folder = std::env::temp_dir().join(format!(
        "termite-widget-{}-{}-{name}",
        std::process::id(),
        shell.name
    ));
    fs::create_dir_all(&folder).expect("temp folder");
    let fake = folder.join("tmite");
    let script = format!(
        "#!/bin/sh\necho asked >> '{}'\nprintf 'progress\\tasking the model… 0s\\n'\nsleep 0.1\ncat <<'END'\n{answer}\nEND\n",
        folder.join("asked").display()
    );
    fs::write(&fake, script).expect("fake tmite");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700))
        .expect("executable");

    let widget = Path::new(shell.widget).canonicalize().expect("widget");
    let rc = format!(
        "source {}
{}",
        widget.display(),
        shell.setup
    );
    fs::write(folder.join(shell.file), rc).expect("startup file");

    let output = Command::new("zsh")
        .args(["-f", "-c", DRIVER])
        .env("FAKE", &fake)
        .env("START", shell.start)
        .env("ZDOTDIR", &folder)
        .env("REQUEST", request)
        .env("KEYS", keys)
        .output()
        .expect("zsh runs");

    let read =
        |file: &str| fs::read_to_string(folder.join(file)).unwrap_or_default();
    let seen = Seen {
        screen: String::from_utf8_lossy(&output.stdout).into_owned(),
        prompt: read("prompt.txt").trim_end().to_owned(),
        ran: folder.join("ran").exists(),
        asked: read("asked").lines().count(),
    };
    // The inner shell may still be writing its history as it exits.
    fs::remove_dir_all(&folder).ok();
    seen
}

fn one(kind: &str, rating: &str) -> String {
    format!(
        "{kind}\n{rating}\n{COMMAND}\n✓ reads only  Make a file\n  {COMMAND}"
    )
}

fn list() -> String {
    format!(
        "choose\n2\nreads\tls\nchanges\t{COMMAND}\n1. first option\n2. second option"
    )
}

#[test]
fn given_a_yes_then_the_explained_command_reaches_the_prompt_and_does_not_run()
{
    in_each_shell(|shell| {
        let seen = drive(shell, "yes", "? go", "y", &one("confirm", "reads"));

        assert!(seen.screen.contains("Make a file"), "{}", seen.screen);
        assert!(seen.screen.contains("[Y/n]"), "{}", seen.screen);
        assert_eq!(seen.prompt, COMMAND, "{}", shell.name);
        assert!(!seen.ran, "{}", shell.name);
    });
}

#[test]
fn given_enter_then_the_default_is_yes_unless_it_is_risky() {
    in_each_shell(|shell| {
        let plain =
            drive(shell, "enter", "? go", "\r", &one("confirm", "reads"));
        assert_eq!(plain.prompt, COMMAND, "{}", shell.name);
        assert!(!plain.ran, "{}", shell.name);

        let risky =
            drive(shell, "risky", "? go", "\r", &one("confirm", "risky"));
        assert!(risky.screen.contains("[y/N]"), "{}", risky.screen);
        assert_eq!(risky.prompt, "", "{}", shell.name);
        assert!(!risky.ran, "{}", shell.name);
    });
}

#[test]
fn given_a_no_then_the_prompt_is_left_empty() {
    in_each_shell(|shell| {
        let seen = drive(shell, "no", "? go", "n", &one("confirm", "reads"));

        assert_eq!(seen.prompt, "", "{}", shell.name);
        assert!(!seen.ran, "{}", shell.name);
    });
}

#[test]
fn given_straight_in_then_it_reaches_the_prompt_unasked_and_does_not_run() {
    in_each_shell(|shell| {
        let answer = one("straight_in", "reads");
        let seen = drive(shell, "straight", "?x go", "", &answer);

        assert!(!seen.screen.contains("[Y/n]"), "{}", seen.screen);
        assert_eq!(seen.prompt, COMMAND, "{}", shell.name);
        assert!(!seen.ran, "{}", shell.name);
    });
}

#[test]
fn given_a_refusal_then_nothing_reaches_the_prompt() {
    in_each_shell(|shell| {
        let answer = "refused\n\n\n✗ refused  No.\n  touch ran";
        let seen = drive(shell, "refused", "?x go", "", answer);

        assert!(seen.screen.contains("refused"), "{}", seen.screen);
        assert_eq!(seen.prompt, "", "{}", shell.name);
        assert!(!seen.ran, "{}", shell.name);
    });
}

#[test]
fn given_a_list_then_a_number_picks_a_command_for_the_prompt() {
    in_each_shell(|shell| {
        let seen = drive(shell, "pick", "?? go", "2", &list());

        assert!(seen.screen.contains("second option"), "{}", seen.screen);
        assert_eq!(seen.prompt, COMMAND, "{}", shell.name);
        assert!(!seen.ran, "{}", shell.name);
    });
}

#[test]
fn given_a_list_then_a_asks_again_and_another_key_cancels() {
    in_each_shell(|shell| {
        let again = drive(shell, "again", "?? go", "a2", &list());
        assert_eq!(again.asked, 2, "{}", shell.name);
        assert_eq!(again.prompt, COMMAND, "{}", shell.name);

        let cancel = drive(shell, "cancel", "?? go", "q", &list());
        assert_eq!(cancel.asked, 1, "{}", shell.name);
        assert_eq!(cancel.prompt, "", "{}", shell.name);
    });
}

#[test]
fn given_a_risky_pick_from_a_list_then_it_needs_a_second_yes() {
    in_each_shell(|shell| {
        let answer = format!("choose\n1\nrisky\t{COMMAND}\n1. first option");
        let no = drive(shell, "riskyno", "?? go", "1n", &answer);
        assert_eq!(no.prompt, "", "{}", shell.name);

        let yes = drive(shell, "riskyyes", "?? go", "1y", &answer);
        assert_eq!(yes.prompt, COMMAND, "{}", shell.name);
        assert!(!yes.ran, "{}", shell.name);
    });
}

/// An ordinary line still runs, and that's the shell doing it.
#[test]
fn given_an_older_tmite_then_nothing_reaches_the_prompt_and_it_says_why() {
    in_each_shell(|shell| {
        for kind in ["confirm", "straight_in"] {
            let answer =
                format!("{kind}\n{COMMAND}\nMake a file (read, score 0.68)");
            let seen = drive(shell, kind, "? go", "", &answer);

            assert!(seen.screen.contains("does not match"), "{}", seen.screen);
            assert!(!seen.screen.contains("[Y/n]"), "{}", seen.screen);
            assert_eq!(seen.prompt, "", "{}", shell.name);
            assert!(!seen.ran, "{}", shell.name);
        }
    });
}

#[test]
fn given_an_ordinary_line_then_the_shell_runs_it_as_normal() {
    in_each_shell(|shell| {
        let seen = drive(shell, "plain", "touch ran", "", "unused");

        assert!(seen.ran, "{}: {}", shell.name, seen.screen);
        assert_eq!(seen.asked, 0, "{}", shell.name);
    });
}
