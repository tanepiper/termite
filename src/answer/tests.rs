use super::{Answer, answer};
use crate::candidate::{Candidate, Risk};
use crate::command::CommandRejected;
use crate::context::{Context, Session};
use crate::decide::{MockDecisionClient, Ranked, SafetyVerdict};
use crate::error::{AssistError, Service};
use crate::generate::MockCommandSource;
use crate::progress::{NoProgress, Progress, Stage};
use crate::query::parse_query;
use crate::test_support::ready;

fn local_idle() -> Context {
    Context::new("macos", "zsh", "/Users/me/code", Session::Local)
}

fn read(command: &str) -> Candidate {
    Candidate::new(command, format!("Run {command}"), Risk::Read)
}

/// A source that returns `candidates` once.
fn source(candidates: Vec<Candidate>) -> MockCommandSource {
    let mut source = MockCommandSource::new();
    source.expect_generate().times(1).returning(move |_, _, _| {
        let candidates = candidates.clone();
        Box::pin(async move { Ok(candidates) })
    });
    source
}

/// A decider that ranks in the order it is given, scores 0.9, 0.8, …, and
/// is never asked about safety unless the test adds that.
fn ranking_in_order() -> MockDecisionClient {
    let mut decider = MockDecisionClient::new();
    decider
        .expect_rank()
        .times(1)
        .returning(|_, _, candidates| {
            let ranked = rank_in_order(candidates);
            Box::pin(async move { Ok(ranked) })
        });
    decider.expect_safety().never();
    decider
}

fn rank_in_order(candidates: &[Candidate]) -> Vec<Ranked> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            Ranked::new(candidate.clone(), 0.9 - index as f64 / 10.0, "fits")
        })
        .collect()
}

/// A decider that ranks in order and answers the safety questions with
/// `reply`.
fn with_safety(
    reply: Result<SafetyVerdict, AssistError>,
) -> MockDecisionClient {
    let mut decider = MockDecisionClient::new();
    decider
        .expect_rank()
        .times(1)
        .returning(|_, _, candidates| {
            let ranked = rank_in_order(candidates);
            Box::pin(async move { Ok(ranked) })
        });
    decider.expect_safety().times(1).returning(move |_, _, _| {
        let reply = reply.clone();
        Box::pin(async move { reply })
    });
    decider
}

/// Probabilities for: only reads, keeps secrets hidden, harmful.
fn verdict(only_reads: f64, no_secrets: f64, harmful: f64) -> SafetyVerdict {
    SafetyVerdict::new(only_reads, no_secrets, harmful)
}

fn run(
    input: &str,
    context: &Context,
    source: &MockCommandSource,
    decider: &MockDecisionClient,
) -> Answer {
    let query = parse_query(input).expect("a query");
    ready(answer(&query, context, source, decider, &NoProgress))
}

fn reason(answer: &Answer) -> &str {
    match answer {
        Answer::ConfirmWithReason { reason, .. }
        | Answer::StraightIn { reason, .. }
        | Answer::Refused { reason, .. } => reason,
        other => panic!("no reason in {other:?}"),
    }
}

#[test]
fn given_confirm_mode_when_answered_then_the_best_command_is_returned() {
    let answer = run(
        "? list my code",
        &local_idle(),
        &source(vec![read("ls ~/code"), read("tree ~/code")]),
        &ranking_in_order(),
    );

    let Answer::Confirm(best) = answer else {
        panic!("expected Confirm, got {answer:?}");
    };
    assert_eq!(best.candidate().command(), "ls ~/code");
}

#[test]
fn given_choose_mode_when_answered_then_every_command_is_returned_best_first() {
    let answer = run(
        "?? list my code",
        &local_idle(),
        &source(vec![read("ls ~/code"), read("tree ~/code")]),
        &ranking_in_order(),
    );

    let Answer::Choose(ranked) = answer else {
        panic!("expected Choose, got {answer:?}");
    };
    let commands: Vec<&str> =
        ranked.iter().map(|r| r.candidate().command()).collect();
    assert_eq!(commands, ["ls ~/code", "tree ~/code"]);
}

#[test]
fn given_choose_mode_with_duplicates_then_one_merged_command_is_ranked() {
    let mut decider = MockDecisionClient::new();
    decider
        .expect_rank()
        .withf(|_, _, candidates| candidates.len() == 1)
        .times(1)
        .returning(|_, _, candidates| {
            let ranked = rank_in_order(candidates);
            Box::pin(async move { Ok(ranked) })
        });

    let answer = run(
        "?? list files",
        &local_idle(),
        &source(vec![read("ls -la"), read("ls -al")]),
        &decider,
    );

    let Answer::Choose(ranked) = answer else {
        panic!("expected Choose, got {answer:?}");
    };
    assert_eq!(ranked.len(), 1);
}

#[test]
fn given_commands_failing_checks_then_they_are_not_ranked() {
    let mut decider = MockDecisionClient::new();
    decider
        .expect_rank()
        .withf(|_, _, candidates| {
            candidates.len() == 1 && candidates[0].command() == "ls"
        })
        .times(1)
        .returning(|_, _, candidates| {
            let ranked = rank_in_order(candidates);
            Box::pin(async move { Ok(ranked) })
        });

    let answer = run(
        "? list files",
        &local_idle(),
        &source(vec![read("ls\nrm -rf ~"), read("ls")]),
        &decider,
    );

    assert!(matches!(answer, Answer::Confirm(_)), "{answer:?}");
}

#[test]
fn given_only_commands_failing_checks_then_no_usable_command_is_returned() {
    let mut decider = MockDecisionClient::new();
    decider.expect_rank().never();

    let answer = run(
        "? list files",
        &local_idle(),
        &source(vec![read("ls\nrm -rf ~"), read("")]),
        &decider,
    );

    assert_eq!(
        answer,
        Answer::NoUsableCommand {
            rejected: vec![
                (read("ls\nrm -rf ~"), CommandRejected::MultipleLines),
                (read(""), CommandRejected::Empty),
            ],
        }
    );
}

#[test]
fn given_a_generation_or_ranking_error_then_failed_is_returned() {
    let mut failing_source = MockCommandSource::new();
    failing_source.expect_generate().returning(|_, _, _| {
        Box::pin(async {
            Err(AssistError::Timeout {
                service: Service::Model,
            })
        })
    });
    let mut idle_decider = MockDecisionClient::new();
    idle_decider.expect_rank().never();

    let mut failing_decider = MockDecisionClient::new();
    failing_decider.expect_rank().returning(|_, _, _| {
        Box::pin(async {
            Err(AssistError::Timeout {
                service: Service::Decision,
            })
        })
    });
    failing_decider.expect_safety().never();

    for input in ["? x", "?? x", "?x x"] {
        assert_eq!(
            run(input, &local_idle(), &failing_source, &idle_decider),
            Answer::Failed(AssistError::Timeout {
                service: Service::Model,
            }),
            "{input}"
        );
        assert_eq!(
            run(
                input,
                &local_idle(),
                &source(vec![read("ls")]),
                &failing_decider
            ),
            Answer::Failed(AssistError::Timeout {
                service: Service::Decision,
            }),
            "{input}"
        );
    }
}

#[test]
fn given_every_gate_check_passes_then_the_command_goes_straight_in() {
    let answer = run(
        "?x list my code",
        &local_idle(),
        &source(vec![read("ls ~/code")]),
        &with_safety(Ok(verdict(0.95, 0.85, 0.08))),
    );

    let Answer::StraightIn { ranked, reason } = answer else {
        panic!("expected StraightIn, got {answer:?}");
    };
    assert_eq!(ranked.candidate().command(), "ls ~/code");
    assert_eq!(
        reason,
        "the decision model is confident it only reads (0.95)"
    );
}

#[test]
fn given_a_local_gate_check_fails_then_it_falls_back_without_asking_safety() {
    let cases = [
        (
            local_idle(),
            Candidate::new("ls", "List", Risk::Write),
            "the model labelled it `write`",
        ),
        (
            local_idle(),
            read("rm -rf build"),
            "`rm` is not on the read-only list",
        ),
    ];

    for (context, candidate, expected) in cases {
        let answer = run(
            "?x do it",
            &context,
            &source(vec![candidate]),
            &ranking_in_order(),
        );

        assert!(
            matches!(answer, Answer::ConfirmWithReason { .. }),
            "{answer:?}"
        );
        assert_eq!(reason(&answer), expected);
    }
}

#[test]
fn given_the_safety_check_does_not_approve_then_it_falls_back_to_confirm() {
    let cases = [
        (
            Ok(verdict(0.09, 0.87, 0.70)),
            "the decision model is not sure it only reads (0.09)",
        ),
        (
            Ok(verdict(0.93, 0.27, 0.46)),
            "the decision model thinks it may show secrets (0.27)",
        ),
        (
            Ok(verdict(0.95, 0.90, 0.35)),
            "the decision model rates it possibly harmful (0.35)",
        ),
        (
            Err(AssistError::Timeout {
                service: Service::Decision,
            }),
            "the safety check failed: the decision model did not answer in time",
        ),
        (
            Err(AssistError::InvalidReply {
                service: Service::Decision,
                detail: "no valid answer for `harmful`".into(),
            }),
            "the safety check failed: the decision model sent a reply that is not valid: no valid answer for `harmful`",
        ),
    ];

    for (reply, expected) in cases {
        let answer = run(
            "?x list my code",
            &local_idle(),
            &source(vec![read("ls ~/code")]),
            &with_safety(reply),
        );

        assert!(
            matches!(answer, Answer::ConfirmWithReason { .. }),
            "{answer:?}"
        );
        assert_eq!(reason(&answer), expected);
    }
}

#[test]
fn given_the_safety_check_refuses_then_refused_is_returned() {
    let answer = run(
        "?x list my code",
        &local_idle(),
        &source(vec![read("ls ~/code")]),
        &with_safety(Ok(verdict(0.95, 0.90, 0.92))),
    );

    assert!(matches!(answer, Answer::Refused { .. }), "{answer:?}");
    assert_eq!(
        reason(&answer),
        "the decision model rates it likely harmful (0.92)"
    );
}

#[test]
fn given_real_clients_then_the_answer_future_can_be_sent_between_threads() {
    fn assert_send<T: Send>(_: &T) {}

    let config = crate::config::Config::from_json(
        r#"{"model_url":"http://127.0.0.1:1","model":"m","decision_url":"http://127.0.0.1:1","decision_model":"d"}"#,
    )
    .expect("config");
    let source = crate::generate::LlamaServer::new(&config).expect("client");
    let decider = crate::decide::SystemOne::new(&config, None).expect("client");
    let query = parse_query("? x").expect("query");
    let context = local_idle();

    let future = answer(&query, &context, &source, &decider, &NoProgress);
    assert_send(&future);
}

/// Collects stages for tests.
#[derive(Default)]
struct Stages(std::sync::Mutex<Vec<Stage>>);

impl Progress for Stages {
    fn stage(&self, stage: Stage) {
        self.0.lock().expect("not poisoned").push(stage);
    }
}

#[test]
fn given_a_query_then_each_stage_is_reported_in_order() {
    let cases = [
        ("? x", vec![Stage::Asking, Stage::Ranking]),
        ("?x x", vec![Stage::Asking, Stage::Ranking, Stage::Checking]),
    ];

    for (input, expected) in cases {
        let mut source = MockCommandSource::new();
        source.expect_generate().returning(|_, _, _| {
            Box::pin(async {
                Ok(vec![Candidate::new("ls", "List", Risk::Read)])
            })
        });
        let mut decider = MockDecisionClient::new();
        decider.expect_rank().returning(|_, _, candidates| {
            let ranked = Ranked::new(candidates[0].clone(), 1.0, "best");
            Box::pin(async move { Ok(vec![ranked]) })
        });
        decider.expect_safety().returning(|_, _, _| {
            Box::pin(async { Err(AssistError::EmptyRanking) })
        });
        let query = parse_query(input).expect("query");
        let stages = Stages::default();

        ready(answer(&query, &local_idle(), &source, &decider, &stages));

        assert_eq!(*stages.0.lock().expect("not poisoned"), expected);
    }
}
