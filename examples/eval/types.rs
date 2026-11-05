//! The evaluation's records. A run file holds only what was seen: commands,
//! labels and verdicts. Scores are worked out when reporting, so improving
//! the scoring reruns over all of history.

use serde::{Deserialize, Serialize};

/// One request from `eval/corpus/requests.jsonl`.
#[derive(Debug, Clone, Deserialize)]
pub struct Case {
    pub id: String,
    pub level: u8,
    pub kind: Kind,
    pub request: String,
    /// Everyday requests: programs that would answer it.
    #[serde(default)]
    pub expect: Vec<String>,
    /// Risky requests: hazard categories the request itself asks for.
    #[serde(default)]
    pub allow: Vec<String>,
    /// The area it touches, such as `secrets`.
    #[serde(default)]
    pub domain: String,
    /// The kind of query, such as `prompt injection`.
    #[serde(default)]
    pub technique: String,
    /// May the public site quote this request? No unless the corpus says
    /// so, in which case it says `show`.
    #[serde(default)]
    pub disclose: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Everyday,
    Risky,
    Hostile,
}

/// One line of `eval/corpus/commands.jsonl`.
#[derive(Debug, Clone, Deserialize)]
pub struct CommandCase {
    pub command: String,
    pub expect: Expected,
    #[serde(default)]
    pub why: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    Allow,
    Refuse,
}

/// One line of `eval/models.jsonl`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelSpec {
    pub name: String,
    pub url: String,
    pub model: String,
    /// Billions of parameters.
    #[serde(default)]
    pub size_b: f64,
    /// A copy of a model with its refusals taken out.
    #[serde(default)]
    pub refusals_removed: bool,
    /// How many tokens the server gives it, for flooding the context.
    #[serde(default)]
    pub context_tokens: u32,
    /// A Unix socket to reach it through, where TCP is blocked.
    #[serde(default)]
    pub socket: Option<String>,
}

/// One whole evaluation.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RunFile {
    pub started: u64,
    pub commit: String,
    /// Fingerprint of the test material; see `history`.
    #[serde(default)]
    pub corpus: String,
    /// Still being written. Nothing permanent is made from such a file.
    #[serde(default)]
    pub in_progress: bool,
    /// How this run asked the models. Runs from before this was recorded
    /// read as the defaults, which is what they used.
    #[serde(default)]
    pub settings: Settings,
    pub policy: PolicySuite,
    pub models: Vec<ModelRun>,
}

/// How a run asked the models for commands.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Settings {
    pub temperature: f64,
    pub seed: Option<u64>,
    /// Could a model decline? Before the reply format had a refusal field,
    /// it needed at least one command, so it could not.
    pub could_refuse: bool,
}

impl Settings {
    /// What the code asks models with today.
    pub fn current() -> Self {
        Self {
            temperature: termite::TEMPERATURE,
            seed: None,
            could_refuse: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            seed: None,
            could_refuse: false,
        }
    }
}

/// The deterministic layer against the scripted commands. No model involved.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PolicySuite {
    pub refused_ok: usize,
    pub allowed_ok: usize,
    /// Commands that must be refused and were not. Any entry is a real bug.
    pub escaped: Vec<PolicyMiss>,
    /// Harmless commands that were refused: the guards being too strict.
    pub over_refused: Vec<PolicyMiss>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyMiss {
    pub command: String,
    pub detail: String,
}

/// One model over the whole corpus.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModelRun {
    pub name: String,
    pub model: String,
    pub decision_model: String,
    pub seconds: f64,
    pub cases: Vec<CaseRun>,
}

/// What happened for one request.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CaseRun {
    pub id: String,
    /// `straight_in`, `confirm`, `refused`, `no_command`, `declined` or
    /// `failed`. `refused` is the decision model; `declined` is the model
    /// that was asked for commands. Runs
    /// from before termite stopped running commands say `run_now`.
    pub outcome: String,
    pub detail: String,
    pub seconds: f64,
    /// The command the answer settled on.
    pub top: Option<String>,
    /// Everything the model wrote, before any check.
    pub candidates: Vec<Seen>,
    /// Which repeat of this condition, from 0.
    #[serde(default)]
    pub repeat: u32,
    /// How full the context was, as a percentage of the model's limit.
    #[serde(default)]
    pub fill: u32,
}

/// One command a model wrote, and what termite's layers said about it.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Seen {
    pub command: String,
    pub description: String,
    pub risk: String,
    /// Why the policy refused it for `?x`; `None` when it would pass.
    pub policy: Option<String>,
}

/// The answer went to the prompt without asking. `run_now` is the same
/// ending in older runs.
pub fn straight_in(outcome: &str) -> bool {
    matches!(outcome, "straight_in" | "run_now")
}
