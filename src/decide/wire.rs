//! The `/v1/systemone` wire format. Contract: pure translation between our
//! types and JSON, no I/O. A reply that is not understood is an error,
//! never a guess.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{Ranked, SafetyVerdict};
use crate::candidate::Candidate;
use crate::command::to_printable;
use crate::context::Context;
use crate::error::{AssistError, Service};

const RANK_QUESTION: &str =
    "Which shell command best does what the user asked?";

/// Each safety question with its descriptions of `false` and `true`.
type Question = (&'static str, &'static str, &'static str);

const ONLY_READS_QUESTION: Question = (
    "Does this command only read or display information, without \
     creating, changing or deleting anything?",
    "it creates, changes or deletes something",
    "it only reads or displays",
);

const NO_SECRETS_QUESTION: Question = (
    "Does this command avoid displaying passwords, private keys, tokens \
     or other secrets?",
    "it may display secrets",
    "it shows no secrets",
);

const HARMFUL_QUESTION: Question = (
    "Could this command delete data, damage the system, or expose secrets?",
    "it is harmless",
    "it could cause harm",
);

/// The ranking request: one `choice` question whose options are the
/// candidates, keyed `c1`, `c2`, … in order.
pub(crate) fn rank_request(
    model: &str,
    request: &str,
    context: &Context,
    candidates: &[Candidate],
) -> Value {
    let options: Map<String, Value> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let option = format!(
                "{} ({})",
                candidate.command(),
                candidate.description()
            );
            (candidate_id(index), Value::String(option))
        })
        .collect();

    json!({
        "model": model,
        "state": state(request, context),
        "questions": {
            "best": {
                "type": "choice",
                "instructions": RANK_QUESTION,
                "criteria": options,
            },
        },
    })
}

/// The safety request: three true/false questions about one command.
pub(crate) fn safety_request(
    model: &str,
    request: &str,
    context: &Context,
    command: &str,
) -> Value {
    // Ollama requires a description for each answer; llama.cpp accepts
    // them too, and Kev-4B separates safe from unsafe more sharply with
    // them.
    let question = |(instructions, no, yes): (&str, &str, &str)| {
        json!({
            "type": "noul",
            "instructions": instructions,
            "criteria": { "false": no, "true": yes },
        })
    };
    let state = format!("{}\nCommand: {command}", state(request, context));

    json!({
        "model": model,
        "state": state,
        "questions": {
            "only_reads": question(ONLY_READS_QUESTION),
            "no_secrets": question(NO_SECRETS_QUESTION),
            "harmful": question(HARMFUL_QUESTION),
        },
    })
}

/// Turn the choice probabilities into scores, best first. A probability
/// for an option that was not sent is an error.
pub(crate) fn parse_ranking(
    reply: &str,
    candidates: &[Candidate],
) -> Result<Vec<Ranked>, AssistError> {
    let reply: SystemOneReply = serde_json::from_str(reply)
        .map_err(|error| AssistError::invalid(Service::Decision, error))?;
    let probabilities = reply
        .answers
        .get("best")
        .and_then(|answer| answer.probabilities.as_ref())
        .ok_or_else(|| {
            AssistError::invalid(
                Service::Decision,
                "no probabilities for `best`",
            )
        })?;

    if probabilities.is_empty() {
        return Err(AssistError::EmptyRanking);
    }

    let mut ranked = Vec::with_capacity(probabilities.len());
    for (id, &score) in probabilities {
        let index = (0..candidates.len())
            .find(|&index| candidate_id(index) == *id)
            .ok_or_else(|| AssistError::UnknownCandidate { id: id.clone() })?;
        ranked.push(Ranked::new(candidates[index].clone(), score, ""));
    }

    // Ties keep the model's order, so the sort is by score, then by id.
    ranked.sort_by(|a, b| {
        b.score.total_cmp(&a.score).then_with(|| {
            let position =
                |r: &Ranked| candidates.iter().position(|c| c == r.candidate());
            position(a).cmp(&position(b))
        })
    });
    Ok(ranked)
}

/// Read the three `noul` answers. A missing answer or a value outside 0 to
/// 1 is an error, never a guess.
pub(crate) fn parse_safety(reply: &str) -> Result<SafetyVerdict, AssistError> {
    let reply: SystemOneReply = serde_json::from_str(reply)
        .map_err(|error| AssistError::invalid(Service::Decision, error))?;
    let answer = |name: &str| {
        reply
            .answers
            .get(name)
            .and_then(|answer| answer.noul)
            .filter(|value| (0.0..=1.0).contains(value))
            .ok_or_else(|| {
                AssistError::invalid(
                    Service::Decision,
                    format!("no valid answer for `{name}`"),
                )
            })
    };

    Ok(SafetyVerdict::new(
        answer("only_reads")?,
        answer("no_secrets")?,
        answer("harmful")?,
    ))
}

/// The state the decision model judges. Each value is made printable, so
/// a request or a folder name holding a line break cannot add a line of
/// its own (such as a second `Command:`).
fn state(request: &str, context: &Context) -> String {
    format!(
        "User request: {}\nShell: {} on {}\nFolder: {}\nSession: {}",
        to_printable(request),
        to_printable(context.shell()),
        to_printable(context.os()),
        to_printable(context.cwd()),
        context.session().as_str()
    )
}

fn candidate_id(index: usize) -> String {
    format!("c{}", index + 1)
}

#[derive(Deserialize)]
struct SystemOneReply {
    answers: HashMap<String, SystemOneAnswer>,
}

#[derive(Deserialize)]
struct SystemOneAnswer {
    #[serde(default)]
    probabilities: Option<HashMap<String, f64>>,
    #[serde(default)]
    noul: Option<f64>,
}
