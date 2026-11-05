//! termite: turns a plain request such as `? restart docker` into a checked
//! shell command.
//!
//! This crate owns the logic shared by the `tmite` command line tool and the
//! `otty-assist` integration: reading the query prefix, checking commands
//! that a model wrote, merging commands that are really the same, the
//! read-only policy for `?x`, and the clients for the local model and the
//! decision endpoint.

#[cfg(test)]
mod adversarial_tests;
mod answer;
mod candidate;
mod classify;
mod command;
mod config;
mod context;
mod decide;
mod draft;
mod error;
mod generate;
mod hazard;
mod heartbeat;
mod home;
mod merge;
mod place;
mod policy;
mod progress;
mod provenance;
mod query;
mod rating;
mod script;
mod sse;
#[cfg(test)]
mod test_support;

pub use answer::{Answer, Held, answer, gate};
pub use candidate::{Candidate, Risk};
pub use classify::{Classification, Level, classify, classify_candidate};
pub use command::{
    CommandRejected, MAX_COMMAND_CHARS, check_command, to_printable,
};
pub use config::{Config, DECISION_TOKEN_VAR, decision_token};
pub use context::{Caller, Context, Session};
pub use decide::{
    DecisionClient, MAX_HARMFUL, MIN_NO_SECRETS, MIN_ONLY_READS,
    REFUSE_HARMFUL, Ranked, SafetyVerdict, SystemOne, Verdict,
};
pub use draft::{
    Draft, DraftPlace, Drafted, MAX_SCRIPT_BYTES, Retry, ScriptReply,
    ScriptSource, critical, flags, script_problems, summary_of, write_draft,
};
pub use error::{AssistError, Service};
pub use generate::{CommandSource, LlamaServer, MAX_CANDIDATES, TEMPERATURE};
pub use hazard::{Hazard, Severity, hazards, program_of};
pub use home::{
    CONFIG_FILE_NAME, Home, HomeError, NameTaken, PRIVATE_DIR_MODE,
    PRIVATE_FILE_MODE, Problem, ScriptName, TERMITE_HOME_VAR, create_private,
    write_private,
};
pub use merge::merge_candidates;
pub use place::{Isolation, isolation, isolation_under, unattended};
pub use policy::{PolicyRefusal, may_read_secrets, read_only, read_only_in};
pub use progress::{NoProgress, Progress, Stage};
pub use provenance::{
    Entry, Event, FileStatus, Log, PROVENANCE_FILE, ProvenanceError, Record,
    sha256_hex,
};
pub use query::{Mode, Query, parse_query, script_request};
pub use rating::{Rating, rate, warnings, warnings_in};
pub use script::{Finding, MAX_COMMAND_BYTES, Splitter, Statement, assess};
