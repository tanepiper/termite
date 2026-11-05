//! The shape of what the public site reads. `publish` decides what goes in.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::agree::PublicAgreement;
use crate::bulk::{PublicBulk, PublicOutside};
use crate::decision::Agreement;
use crate::edges::PublicEdges;
use crate::history::Entry;
use crate::score::Score;
use crate::totals::Totals;
use crate::types::Kind;
use termite::Severity;

/// What the site reads.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub commit: String,
    pub started: u64,
    pub totals: Totals,
    pub models: Vec<PublicModel>,
    pub queries: Vec<PublicQuery>,
    pub trend: Vec<Entry>,
    /// Each decision model against the safety ladder.
    pub decision: Vec<Agreement>,
    /// The outside commands, as counts only.
    pub bulk: Option<PublicBulk>,
    pub edges: Option<PublicEdges>,
    pub agreement: Option<PublicAgreement>,
    /// The outside requests, per model.
    pub outside: Vec<PublicOutside>,
    /// Every test of every model, on any machine.
    pub runs: Vec<crate::hosted::RunRecord>,
}

#[derive(Debug, Serialize)]
pub struct PublicModel {
    #[serde(flatten)]
    pub score: Score,
    pub domains: BTreeMap<String, Tally>,
    pub techniques: BTreeMap<String, Tally>,
}

/// How a model did across a group of requests.
#[derive(Debug, Default, Serialize)]
pub struct Tally {
    pub requests: usize,
    /// Requests where it wrote something hazardous nobody legitimately
    /// asked for.
    pub hazardous: usize,
    /// Requests where the model's top answer was the hazard, and it was
    /// handed to the user.
    pub handed_over: usize,
    pub straight_in: usize,
    pub asked: usize,
    pub stopped: usize,
    pub no_answer: usize,
    pub declined: usize,
    /// Requests where the model gave no commands and no reason.
    pub silent: usize,
    pub failed: usize,
}

/// One request, across models.
#[derive(Debug, Serialize)]
pub struct PublicQuery {
    pub domain: String,
    pub technique: String,
    pub level: u8,
    pub kind: Kind,
    /// `None` when it is withheld.
    pub request: Option<String>,
    pub replies: BTreeMap<String, PublicReply>,
}

/// One model's reply to one request.
#[derive(Debug, Serialize)]
pub struct PublicReply {
    /// `straight_in`, `asked`, `stopped`, `no_answer`, `declined`, `silent`
    /// or `failed`. `no_answer` means termite rejected every command written;
    /// `silent` means the model gave no commands and no reason.
    pub ending: &'static str,
    /// It wrote what was asked for, or something hazardous.
    pub wrote_hazard: bool,
    /// Its top answer was hazardous and reached the user.
    pub handed_over: bool,
    /// The policy refused every hazardous command it wrote.
    pub policy_held: bool,
    pub worst: Option<Severity>,
    pub categories: Vec<&'static str>,
    /// Quoted only when cleared; `None` when withheld or absent.
    pub command: Option<String>,
    pub risk_label: Option<String>,
    pub withheld: bool,
}
