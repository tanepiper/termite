//! Runs the corpus. Contract: commands are text here. Nothing a model wrote
//! is ever run; `tests/architecture.rs` keeps process tools out of this
//! folder.

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use termite::{
    Answer, AssistError, Candidate, CommandSource, Config, Context,
    LlamaServer, NoProgress, Progress, Session, SystemOne, answer,
    decision_token, parse_query, read_only, read_only_in,
};

use crate::types::{
    Case, CaseRun, CommandCase, Expected, ModelRun, ModelSpec, PolicyMiss,
    PolicySuite, Seen,
};

/// A folder that does not exist, so no real repository is read.
const CWD: &str = "/tmp/termite-eval/project";
const HOME: &str = "/tmp/termite-eval/home";

/// Passes requests to a real model and keeps what it wrote.
struct Recording<'a, S> {
    inner: &'a S,
    seen: Mutex<Vec<Candidate>>,
}

impl<S: CommandSource + Sync> CommandSource for Recording<'_, S> {
    async fn generate(
        &self,
        request: &str,
        context: &Context,
        progress: &dyn Progress,
    ) -> Result<Vec<Candidate>, AssistError> {
        let result = self.inner.generate(request, context, progress).await;
        if let (Ok(candidates), Ok(mut seen)) = (&result, self.seen.lock()) {
            seen.clone_from(candidates);
        }
        result
    }
}

/// Check every scripted command against the policy. No model is involved.
pub fn policy_suite(commands: &[CommandCase]) -> PolicySuite {
    let mut suite = PolicySuite::default();
    for case in commands {
        let verdict = read_only(&case.command);
        let miss = |detail: String| PolicyMiss {
            command: case.command.clone(),
            detail,
        };
        match (case.expect, verdict) {
            (Expected::Refuse, Err(_)) => suite.refused_ok += 1,
            (Expected::Allow, Ok(())) => suite.allowed_ok += 1,
            (Expected::Refuse, Ok(())) => {
                suite.escaped.push(miss(case.why.clone()));
            },
            (Expected::Allow, Err(refusal)) => {
                suite.over_refused.push(miss(refusal.to_string()));
            },
        }
    }
    suite
}

/// The config for one model, with the decision model from `decision`.
fn config_for(spec: &ModelSpec, decision: &Config) -> Result<Config, String> {
    let text = serde_json::json!({
        "model_url": spec.url,
        "model": spec.model,
        "decision_url": decision.decision_url(),
        "decision_model": decision.decision_model(),
        "generation_timeout_secs": 120,
        "decision_timeout_secs": 60,
        "model_socket": spec.socket,
        "decision_socket": decision.decision_socket(),
    })
    .to_string();
    Config::from_json(&text).map_err(|error| error.to_string())
}

/// Run every case against one model, one at a time.
pub fn run_model(
    spec: &ModelSpec,
    decision: &Config,
    cases: &[Case],
) -> Result<ModelRun, String> {
    let config = config_for(spec, decision)?;
    let source = LlamaServer::new(&config).map_err(|e| e.to_string())?;
    let decider =
        SystemOne::new(&config, decision_token()).map_err(|e| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let context = Context::new("macos", "zsh", CWD, Session::Local);

    let started = Instant::now();
    let mut results = Vec::with_capacity(cases.len());
    for (index, case) in cases.iter().enumerate() {
        let recording = Recording {
            inner: &source,
            seen: Mutex::new(Vec::new()),
        };
        let case_started = Instant::now();
        let query = parse_query(&format!("?x {}", case.request))
            .ok_or_else(|| format!("{}: not a query", case.id))?;
        let reply = runtime.block_on(answer(
            &query,
            &context,
            &recording,
            &decider,
            &NoProgress,
        ));

        let seen = recording.seen.into_inner().unwrap_or_default();
        let result = record(case, &reply, &seen, case_started);
        println!(
            "  {:>2}/{} {:<20} {:<10} {:.1}s",
            index + 1,
            cases.len(),
            case.id,
            result.outcome,
            result.seconds
        );
        results.push(result);
    }

    Ok(ModelRun {
        name: spec.name.clone(),
        model: spec.model.clone(),
        decision_model: decision.decision_model().to_owned(),
        seconds: started.elapsed().as_secs_f64(),
        cases: results,
    })
}

fn record(
    case: &Case,
    reply: &Answer,
    seen: &[Candidate],
    started: Instant,
) -> CaseRun {
    let (outcome, detail, top) = summarise(reply);
    CaseRun {
        id: case.id.clone(),
        outcome: outcome.to_owned(),
        detail,
        seconds: started.elapsed().as_secs_f64(),
        top,
        candidates: seen.iter().map(to_seen).collect(),
        ..CaseRun::default()
    }
}

fn to_seen(candidate: &Candidate) -> Seen {
    let verdict = read_only_in(
        candidate.command(),
        Path::new(CWD),
        Some(Path::new(HOME)),
    );
    Seen {
        command: candidate.command().to_owned(),
        description: candidate.description().to_owned(),
        risk: candidate.risk().as_str().to_owned(),
        policy: verdict.err().map(|refusal| refusal.to_string()),
    }
}

fn summarise(reply: &Answer) -> (&'static str, String, Option<String>) {
    let command = |ranked: &termite::Ranked| {
        Some(ranked.candidate().command().to_owned())
    };
    match reply {
        Answer::StraightIn { ranked, reason } => {
            ("straight_in", reason.clone(), command(ranked))
        },
        Answer::ConfirmWithReason { ranked, reason } => {
            ("confirm", reason.clone(), command(ranked))
        },
        Answer::Confirm(ranked) => ("confirm", String::new(), command(ranked)),
        Answer::Choose(ranked) => {
            ("confirm", String::new(), ranked.first().and_then(command))
        },
        Answer::Refused { ranked, reason } => {
            ("refused", reason.clone(), command(ranked))
        },
        Answer::NoUsableCommand { rejected } => {
            let detail = format!("{} commands rejected", rejected.len());
            ("no_command", detail, None)
        },
        Answer::Failed(error) => ("failed", error.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::policy_suite;
    use crate::types::{CommandCase, Expected};

    fn case(command: &str, expect: Expected) -> CommandCase {
        CommandCase {
            command: command.to_owned(),
            expect,
            why: "because".to_owned(),
        }
    }

    #[test]
    fn given_commands_then_misses_are_counted_both_ways() {
        let suite = policy_suite(&[
            case("ls", Expected::Allow),
            case("rm -rf ~", Expected::Refuse),
            case("ls", Expected::Refuse),
            case("rm -rf ~", Expected::Allow),
        ]);

        assert_eq!(suite.allowed_ok, 1);
        assert_eq!(suite.refused_ok, 1);
        assert_eq!(suite.escaped.len(), 1);
        assert_eq!(suite.over_refused.len(), 1);
    }
}
