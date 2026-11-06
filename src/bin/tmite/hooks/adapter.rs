//! `tmite hook NAME`: what an agent harness runs before a shell command.
//! Reads the harness's JSON, rates the command as `TERMITE_AGENT=1 tmite check`
//! does, and answers in the harness's own shape.
//!
//! Contract: everything that isn't a clean pass is a stop, and a pass is only
//! ever "no objection": the harness's own approval rules still run. A tool
//! that plainly isn't a shell has nothing to rate, so it gets no objection.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use super::harness::{HOOK_TIMEOUT_SECS, Harness};
use crate::check;

/// The largest message from a harness. Commands are far smaller.
const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;

/// When the adapter gives up and stops the command by itself, well before the
/// harness's own timeout.
const DEADLINE: Duration = Duration::from_secs(HOOK_TIMEOUT_SECS * 2 / 3);

/// The exit status that stops a command in every harness.
const STOP: u8 = 2;

/// Words in a tool's name that mean it may run a command.
const SHELL_TOOL_WORDS: &[&str] = &[
    "bash",
    "cmd",
    "command",
    "exec",
    "monitor",
    "powershell",
    "run",
    "shell",
    "terminal",
];

/// Fields that hold a command, whatever the tool is called.
const COMMAND_FIELDS: &[&str] = &[
    "cmd",
    "command",
    "command_line",
    "commandline",
    "script",
    "shell",
];

/// What the check decided.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Pass,
    Stop(String),
}

/// Whether an answer has been given, so only one ever is.
static ANSWERED: AtomicBool = AtomicBool::new(false);

pub(crate) fn run(name: Option<&str>) -> ExitCode {
    let Some(harness) = name.and_then(Harness::find) else {
        // Nothing can be answered in the harness's shape, but the status
        // still stops the command.
        eprintln!("termite stopped this command: unknown harness");
        return ExitCode::from(STOP);
    };

    thread::spawn(move || {
        thread::sleep(DEADLINE);
        let late = Outcome::Stop(
            "termite stopped this command: the check took too long".to_owned(),
        );
        // Exits the process, whatever the check is still waiting for.
        std::process::exit(i32::from(answer(harness, &late)));
    });

    let mut input = Vec::new();
    let read = io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut input);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let outcome = match read {
        Ok(_) => decide(harness, &input, home.as_deref()),
        Err(_) => stop("the hook input could not be read"),
    };
    if matches!(outcome, Outcome::Stop(_)) {
        crate::provenance::record_check(&[format!("hook {}", harness.id)]);
    }
    ExitCode::from(answer(harness, &outcome))
}

fn stop(why: &str) -> Outcome {
    Outcome::Stop(format!("termite stopped this command: {why}"))
}

/// Read what the harness sent and rate the command in it.
pub(super) fn decide(
    harness: &Harness,
    input: &[u8],
    home: Option<&Path>,
) -> Outcome {
    if input.len() as u64 > MAX_INPUT_BYTES {
        return stop("the hook input is too large");
    }
    let Ok(message) = serde_json::from_slice::<Value>(input) else {
        return stop("the hook input is not valid JSON");
    };
    let Some(command) =
        message.pointer(harness.command_at).and_then(Value::as_str)
    else {
        if names_another_tool(&message) {
            return Outcome::Pass;
        }
        return stop("the hook input has no command");
    };
    let cwd = message
        .pointer(harness.cwd_at)
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|cwd| cwd.is_absolute())
        .or_else(|| std::env::current_dir().ok());
    let Some(cwd) = cwd else {
        return stop("the working folder is not known");
    };

    let rating = check::rate_for_agent(command, &cwd, home);
    if rating.pass {
        return Outcome::Pass;
    }
    Outcome::Stop(format!(
        "termite stopped this command. Don't try another way to do the \
         same thing: tell the person what you wanted to run and why.\n{}",
        rating.report.trim_end()
    ))
}

/// Some harnesses ignore the hook's matcher and send every tool, such as
/// `read_file`. Those carry no command, so there is nothing to rate. Anything
/// that could be a shell, or holds a command in another field, doesn't count.
fn names_another_tool(message: &Value) -> bool {
    let Some(tool) = message.pointer("/tool_name").and_then(Value::as_str)
    else {
        return false;
    };
    let tool = tool.to_lowercase();
    let is_shell = SHELL_TOOL_WORDS.iter().any(|word| tool.contains(word));
    let holds_command = message
        .pointer("/tool_input")
        .and_then(Value::as_object)
        .is_none_or(|input| {
            input.keys().any(|key| {
                COMMAND_FIELDS.contains(&key.to_lowercase().as_str())
            })
        });
    !tool.is_empty() && !is_shell && !holds_command
}

/// Give the harness its answer, once. Returns the exit status.
fn answer(harness: &Harness, outcome: &Outcome) -> u8 {
    if ANSWERED.swap(true, Ordering::SeqCst) {
        return STOP;
    }
    let mut out = Vec::new();
    let mut err = Vec::new();
    let status = write_answer(harness, outcome, &mut out, &mut err);
    let wrote = io::stdout().write_all(&out).and(io::stdout().flush());
    let _ = io::stderr().write_all(&err);
    if wrote.is_err() { STOP } else { status }
}

/// The answer itself. A stop goes every way the harness might read it: the
/// status, standard error and its own JSON.
pub(super) fn write_answer(
    harness: &Harness,
    outcome: &Outcome,
    out: &mut Vec<u8>,
    err: &mut Vec<u8>,
) -> u8 {
    match outcome {
        Outcome::Pass => {
            out.extend_from_slice(harness.no_objection.as_bytes());
            0
        },
        Outcome::Stop(reason) => {
            if let Some(denial) = harness.denial(reason) {
                out.extend_from_slice(denial.to_string().as_bytes());
            }
            err.extend_from_slice(reason.as_bytes());
            err.push(b'\n');
            STOP
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Outcome, decide, write_answer};
    use crate::hooks::harness::{HARNESSES, Harness};

    fn harness(id: &str) -> &'static Harness {
        Harness::find(id).expect("a harness")
    }

    #[test]
    fn given_bad_input_then_every_harness_stops() {
        for harness in HARNESSES {
            for input in [
                &b"not json"[..],
                b"",
                b"[]",
                b"{}",
                b"{\"tool_input\":{\"command\":7}}",
                b"{\"command\":[\"ls\"]}",
            ] {
                assert!(
                    matches!(decide(harness, input, None), Outcome::Stop(_)),
                    "{} let through {:?}",
                    harness.id,
                    String::from_utf8_lossy(input)
                );
            }
        }
    }

    #[test]
    fn given_a_tool_that_is_not_a_shell_then_there_is_no_objection() {
        for input in [
            json!({ "tool_name": "read_file", "tool_input": { "path": "a" } }),
            json!({ "tool_name": "Edit", "tool_input": { "file_path": "a" } }),
        ] {
            assert_eq!(
                decide(harness("claude"), input.to_string().as_bytes(), None),
                Outcome::Pass,
                "{input}"
            );
        }
    }

    #[test]
    fn given_anything_that_could_be_a_shell_then_no_command_still_stops() {
        for input in [
            json!({ "tool_name": "Bash", "tool_input": {} }),
            json!({ "tool_name": "run_in_terminal", "tool_input": {} }),
            json!({ "tool_name": "Monitor", "tool_input": {} }),
            json!({ "tool_name": "mcp__x__exec", "tool_input": {} }),
            json!({ "tool_name": "helper", "tool_input": { "cmd": "ls" } }),
            json!({ "tool_name": "helper", "tool_input": { "Script": "ls" } }),
            json!({ "tool_name": "helper" }),
            json!({ "tool_name": "", "tool_input": {} }),
            json!({ "tool_name": 7, "tool_input": {} }),
        ] {
            assert!(
                matches!(
                    decide(
                        harness("claude"),
                        input.to_string().as_bytes(),
                        None
                    ),
                    Outcome::Stop(_)
                ),
                "{input}"
            );
        }
    }

    #[test]
    fn given_too_much_input_then_it_stops() {
        let input = vec![b' '; 4 * 1024 * 1024 + 1];
        assert!(matches!(
            decide(harness("claude"), &input, None),
            Outcome::Stop(_)
        ));
    }

    #[test]
    fn given_an_empty_command_then_it_stops() {
        let input = json!({ "command": "", "cwd": "/tmp" }).to_string();
        assert!(matches!(
            decide(harness("cursor"), input.as_bytes(), None),
            Outcome::Stop(_)
        ));
    }

    #[test]
    fn given_a_stop_then_every_harness_gets_status_two_and_a_reason() {
        for harness in HARNESSES {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let outcome = Outcome::Stop("no".to_owned());

            let status = write_answer(harness, &outcome, &mut out, &mut err);

            assert_eq!(status, 2);
            assert_eq!(err, b"no\n");
            if !out.is_empty() {
                let text = String::from_utf8(out).expect("text");
                let _: Value = serde_json::from_str(&text).expect("json");
                assert!(text.contains("no"));
            }
        }
    }

    /// A pass must never read as an approval. Two harnesses treat silence as
    /// a stop, and get the word that leaves their own rules in charge.
    #[test]
    fn given_a_pass_then_no_harness_is_told_to_approve() {
        for harness in HARNESSES {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let status =
                write_answer(harness, &Outcome::Pass, &mut out, &mut err);

            assert_eq!(status, 0);
            assert!(err.is_empty());
            let text = String::from_utf8(out).expect("text");
            assert!(!text.contains("auto_approve"));
            if !["cursor", "antigravity"].contains(&harness.id) {
                assert!(
                    text.is_empty() || text == "{}",
                    "{} said {text}",
                    harness.id
                );
            }
        }
    }
}
