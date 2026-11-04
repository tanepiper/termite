//! termite: turns a plain request such as `? restart docker` into a checked
//! shell command.
//!
//! This crate owns the logic shared by the `tmite` command line tool and the
//! `otty-assist` integration: reading the query prefix, checking commands
//! that a model wrote, merging commands that are really the same, the
//! read-only policy for `?x`, and the clients for the local model and the
//! decision endpoint.

mod answer;
mod candidate;
mod command;
mod config;
mod context;
mod decide;
mod error;
mod generate;
mod home;
mod merge;
mod policy;
mod query;
#[cfg(test)]
mod test_support;

pub use answer::{Answer, answer};
pub use candidate::{Candidate, Risk};
pub use command::{
    CommandRejected, MAX_COMMAND_CHARS, check_command, to_printable,
};
pub use config::{Config, DECISION_TOKEN_VAR, decision_token};
pub use context::{Context, Session};
pub use decide::{
    DecisionClient, MAX_HARMFUL, MIN_NO_SECRETS, MIN_ONLY_READS,
    REFUSE_HARMFUL, Ranked, SafetyVerdict, SystemOne, Verdict,
};
pub use error::{AssistError, Service};
pub use generate::{CommandSource, LlamaServer, MAX_CANDIDATES};
pub use home::{
    CONFIG_FILE_NAME, Home, HomeError, NameTaken, PRIVATE_DIR_MODE,
    PRIVATE_FILE_MODE, Problem, ScriptName, TERMITE_HOME_VAR, create_private,
    write_private,
};
pub use merge::merge_candidates;
pub use policy::{PolicyRefusal, read_only};
pub use query::{Mode, Query, parse_query};
