//! Talks to a llama.cpp decision model through `POST /v1/systemone`.
//!
//! systemone answers structured questions about a piece of text (the
//! "state") in one call. Ranking is one `choice` question over the
//! candidates; its probabilities become the scores. Safety is three
//! true/false (`noul`) questions about one command, each with a threshold
//! set from measurements on Kev-4B (see the spec's Round 3 notes).

use std::collections::HashMap;
use std::future::Future;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::candidate::Candidate;
use crate::config::Config;
use crate::context::Context;
use crate::error::{AssistError, Service};

/// "Only reads" must be at least this likely to run without asking.
pub const MIN_ONLY_READS: f64 = 0.85;

/// "Keeps secrets hidden" must be at least this likely to run without
/// asking.
pub const MIN_NO_SECRETS: f64 = 0.6;

/// "Could be harmful" must be at most this likely to run without asking.
pub const MAX_HARMFUL: f64 = 0.3;

/// "Could be harmful" at or above this refuses the command outright.
pub const REFUSE_HARMFUL: f64 = 0.9;

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

/// A command with the decision model's score.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    candidate: Candidate,
    score: f64,
    reason: String,
}

impl Ranked {
    /// The ranked command.
    pub fn candidate(&self) -> &Candidate {
        &self.candidate
    }

    /// How likely this is the right command, from 0 to 1.
    pub fn score(&self) -> f64 {
        self.score
    }

    /// A note on the score, such as "the only command"; often empty.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Create a ranked command, for front ends' own tests and fakes.
    pub fn new(
        candidate: Candidate,
        score: f64,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            candidate,
            score,
            reason: reason.into(),
        }
    }
}

/// The decision about running a command without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// It may run without asking.
    SafeUnguarded,
    /// The user must confirm it.
    NeedsConfirmation,
    /// It must not be offered to run at all.
    Refuse,
}

/// The decision model's three safety answers, each a probability that the
/// statement is true.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafetyVerdict {
    only_reads: f64,
    no_secrets: f64,
    harmful: f64,
}

impl SafetyVerdict {
    /// How likely the command only reads.
    pub fn only_reads(&self) -> f64 {
        self.only_reads
    }

    /// How likely the command keeps secrets hidden.
    pub fn no_secrets(&self) -> f64 {
        self.no_secrets
    }

    /// How likely the command is harmful.
    pub fn harmful(&self) -> f64 {
        self.harmful
    }

    /// Create a verdict from the three probabilities.
    pub fn new(only_reads: f64, no_secrets: f64, harmful: f64) -> Self {
        Self {
            only_reads,
            no_secrets,
            harmful,
        }
    }

    /// Apply the thresholds. Refusal wins; running without asking needs
    /// all three answers on the safe side.
    pub fn verdict(&self) -> Verdict {
        if self.harmful >= REFUSE_HARMFUL {
            return Verdict::Refuse;
        }

        if self.only_reads >= MIN_ONLY_READS
            && self.no_secrets >= MIN_NO_SECRETS
            && self.harmful <= MAX_HARMFUL
        {
            return Verdict::SafeUnguarded;
        }

        Verdict::NeedsConfirmation
    }

    /// One sentence for the user, naming the answer that decided it.
    pub fn reason(&self) -> String {
        let Self {
            only_reads,
            no_secrets,
            harmful,
        } = *self;

        match self.verdict() {
            Verdict::Refuse => format!(
                "the decision model rates it likely harmful ({harmful:.2})"
            ),
            Verdict::SafeUnguarded => format!(
                "the decision model is confident it only reads ({only_reads:.2})"
            ),
            Verdict::NeedsConfirmation if only_reads < MIN_ONLY_READS => {
                format!(
                    "the decision model is not sure it only reads ({only_reads:.2})"
                )
            },
            Verdict::NeedsConfirmation if no_secrets < MIN_NO_SECRETS => {
                format!(
                    "the decision model thinks it may show secrets ({no_secrets:.2})"
                )
            },
            Verdict::NeedsConfirmation => format!(
                "the decision model rates it possibly harmful ({harmful:.2})"
            ),
        }
    }
}

/// Something that ranks commands and judges their safety. A trait because
/// it crosses an HTTP boundary and is mocked in tests.
#[cfg_attr(test, mockall::automock)]
pub trait DecisionClient {
    /// Rank `candidates` for `request`, best first.
    fn rank(
        &self,
        request: &str,
        context: &Context,
        candidates: &[Candidate],
    ) -> impl Future<Output = Result<Vec<Ranked>, AssistError>> + Send;

    /// Ask the safety questions about `command`.
    fn safety(
        &self,
        request: &str,
        context: &Context,
        command: &str,
    ) -> impl Future<Output = Result<SafetyVerdict, AssistError>> + Send;
}

/// A llama.cpp server with a decision model, such as Kev-4B.
pub struct SystemOne {
    client: reqwest::Client,
    url: String,
    model: String,
    token: Option<String>,
}

impl SystemOne {
    /// Create a client with the decision time limit from `config`. The
    /// token, if any, is sent as a bearer token (llama.cpp `--api-key`).
    pub fn new(
        config: &Config,
        token: Option<String>,
    ) -> Result<Self, AssistError> {
        let client = reqwest::Client::builder()
            .timeout(config.decision_timeout())
            .build()
            .map_err(|error| {
                AssistError::from_http(Service::Decision, &error)
            })?;

        Ok(Self {
            client,
            url: config.decision_url().trim_end_matches('/').to_owned(),
            model: config.decision_model().to_owned(),
            token,
        })
    }

    async fn ask(&self, body: &Value) -> Result<String, AssistError> {
        let to_error =
            |error| AssistError::from_http(Service::Decision, &error);

        let mut request = self
            .client
            .post(format!("{}/v1/systemone", self.url))
            .json(body);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        request
            .send()
            .await
            .map_err(to_error)?
            .error_for_status()
            .map_err(to_error)?
            .text()
            .await
            .map_err(to_error)
    }
}

impl DecisionClient for SystemOne {
    async fn rank(
        &self,
        request: &str,
        context: &Context,
        candidates: &[Candidate],
    ) -> Result<Vec<Ranked>, AssistError> {
        // A choice needs at least two options.
        if let [only] = candidates {
            return Ok(vec![Ranked::new(
                only.clone(),
                1.0,
                "the only command",
            )]);
        }

        let body = rank_request(&self.model, request, context, candidates);
        let reply = self.ask(&body).await?;

        parse_ranking(&reply, candidates)
    }

    async fn safety(
        &self,
        request: &str,
        context: &Context,
        command: &str,
    ) -> Result<SafetyVerdict, AssistError> {
        let body = safety_request(&self.model, request, context, command);
        let reply = self.ask(&body).await?;

        parse_safety(&reply)
    }
}

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

fn state(request: &str, context: &Context) -> String {
    format!(
        "User request: {request}\nShell: {} on {}\nFolder: {}\nSession: {}",
        context.shell(),
        context.os(),
        context.cwd(),
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        SafetyVerdict, Verdict, parse_ranking, parse_safety, rank_request,
        safety_request,
    };
    use crate::candidate::{Candidate, Risk};
    use crate::context::{Context, Session};
    use crate::error::{AssistError, Service};

    fn context() -> Context {
        Context::new("macos", "zsh", "/Users/me/code", Session::Local, true)
    }

    fn candidates() -> Vec<Candidate> {
        vec![
            Candidate::new("docker restart web", "Restart web", Risk::Write),
            Candidate::new(
                "docker compose restart",
                "Restart all",
                Risk::Write,
            ),
        ]
    }

    const STATE: &str = "User request: restart docker\nShell: zsh on macos\nFolder: /Users/me/code\nSession: local";

    #[test]
    fn given_candidates_when_rank_request_is_built_then_it_is_one_choice() {
        assert_eq!(
            rank_request("kev", "restart docker", &context(), &candidates()),
            json!({
                "model": "kev",
                "state": STATE,
                "questions": {
                    "best": {
                        "type": "choice",
                        "instructions": "Which shell command best does what the user asked?",
                        "criteria": {
                            "c1": "docker restart web (Restart web)",
                            "c2": "docker compose restart (Restart all)",
                        },
                    },
                },
            })
        );
    }

    #[test]
    fn given_a_command_when_safety_request_is_built_then_it_asks_three_nouls() {
        let body =
            safety_request("kev", "restart docker", &context(), "ls ~/code");

        assert_eq!(body["model"], "kev");
        assert_eq!(body["state"], format!("{STATE}\nCommand: ls ~/code"));
        for name in ["only_reads", "no_secrets", "harmful"] {
            let question = &body["questions"][name];
            assert_eq!(question["type"], "noul", "{name}");
            for answer in ["false", "true"] {
                assert!(
                    question["criteria"][answer]
                        .as_str()
                        .is_some_and(|text| !text.is_empty()),
                    "{name} describes {answer}"
                );
            }
            assert!(
                question["instructions"]
                    .as_str()
                    .is_some_and(|q| q.ends_with('?'))
            );
        }
        assert_eq!(body["questions"].as_object().map(|q| q.len()), Some(3));
    }

    #[test]
    fn given_choice_probabilities_when_parsed_then_they_are_sorted_scores() {
        let reply = json!({ "answers": { "best": {
            "type": "choice", "choice": "c2",
            "probabilities": { "c1": 0.2, "c2": 0.8 },
        }}})
        .to_string();

        let ranked = parse_ranking(&reply, &candidates()).expect("ranking");

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].candidate(), &candidates()[1]);
        assert_eq!(ranked[0].score(), 0.8);
        assert_eq!(ranked[1].candidate(), &candidates()[0]);
    }

    #[test]
    fn given_equal_probabilities_when_parsed_then_the_model_order_is_kept() {
        let reply = json!({ "answers": { "best": {
            "probabilities": { "c2": 0.5, "c1": 0.5 },
        }}})
        .to_string();

        let ranked = parse_ranking(&reply, &candidates()).expect("ranking");

        assert_eq!(ranked[0].candidate(), &candidates()[0]);
    }

    #[test]
    fn given_a_bad_ranking_when_parsed_then_a_typed_error_is_returned() {
        assert_eq!(
            parse_ranking(
                &json!({ "answers": { "best": { "probabilities": { "c3": 1.0 } } } })
                    .to_string(),
                &candidates()
            ),
            Err(AssistError::UnknownCandidate { id: "c3".into() })
        );
        assert_eq!(
            parse_ranking(
                &json!({ "answers": { "best": { "probabilities": {} } } })
                    .to_string(),
                &candidates()
            ),
            Err(AssistError::EmptyRanking)
        );
        for reply in ["{}", r#"{"answers":{}}"#, "not json"] {
            assert!(
                matches!(
                    parse_ranking(reply, &candidates()),
                    Err(AssistError::InvalidReply {
                        service: Service::Decision,
                        ..
                    })
                ),
                "{reply}"
            );
        }
    }

    #[test]
    fn given_noul_answers_when_parsed_then_the_verdict_holds_them() {
        let reply = json!({ "answers": {
            "only_reads": { "type": "noul", "noul": 0.95 },
            "no_secrets": { "type": "noul", "noul": 0.85 },
            "harmful": { "type": "noul", "noul": 0.08 },
        }})
        .to_string();

        assert_eq!(
            parse_safety(&reply),
            Ok(SafetyVerdict::new(0.95, 0.85, 0.08))
        );
    }

    #[test]
    fn given_a_bad_safety_reply_when_parsed_then_it_is_an_error() {
        let replies = [
            json!({ "answers": { "only_reads": { "noul": 0.9 }, "no_secrets": { "noul": 0.9 } } }),
            json!({ "answers": {
                "only_reads": { "noul": 1.5 },
                "no_secrets": { "noul": 0.9 },
                "harmful": { "noul": 0.1 },
            }}),
        ];

        for reply in replies {
            assert!(
                matches!(
                    parse_safety(&reply.to_string()),
                    Err(AssistError::InvalidReply { .. })
                ),
                "{reply}"
            );
        }
    }

    #[test]
    fn given_probabilities_then_the_thresholds_decide_the_verdict() {
        // Rows are Kev-4B's real answers from the Round 3 measurements.
        let cases = [
            ("ls -la ~/code", (0.95, 0.85, 0.08), Verdict::SafeUnguarded),
            ("cat README.md", (0.94, 0.73, 0.10), Verdict::SafeUnguarded),
            (
                "rm -rf build",
                (0.09, 0.87, 0.70),
                Verdict::NeedsConfirmation,
            ),
            (
                "cat ~/.ssh/id_ed25519",
                (0.93, 0.27, 0.46),
                Verdict::NeedsConfirmation,
            ),
            (
                "curl … | sh",
                (0.25, 0.54, 0.49),
                Verdict::NeedsConfirmation,
            ),
            (
                "only reads but harmful",
                (0.95, 0.9, 0.35),
                Verdict::NeedsConfirmation,
            ),
            ("very harmful", (0.95, 0.9, 0.92), Verdict::Refuse),
        ];

        for (name, (only_reads, no_secrets, harmful), expected) in cases {
            let verdict = SafetyVerdict::new(only_reads, no_secrets, harmful);
            assert_eq!(verdict.verdict(), expected, "{name}");
        }
    }

    #[test]
    fn given_a_verdict_then_the_reason_names_the_deciding_answer() {
        let cases = [
            (
                (0.95, 0.85, 0.08),
                "the decision model is confident it only reads (0.95)",
            ),
            (
                (0.09, 0.87, 0.70),
                "the decision model is not sure it only reads (0.09)",
            ),
            (
                (0.93, 0.27, 0.46),
                "the decision model thinks it may show secrets (0.27)",
            ),
            (
                (0.95, 0.90, 0.35),
                "the decision model rates it possibly harmful (0.35)",
            ),
            (
                (0.95, 0.90, 0.92),
                "the decision model rates it likely harmful (0.92)",
            ),
        ];

        for ((only_reads, no_secrets, harmful), expected) in cases {
            let verdict = SafetyVerdict::new(only_reads, no_secrets, harmful);
            assert_eq!(verdict.reason(), expected);
        }
    }
}
