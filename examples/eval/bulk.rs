//! Thousands of real commands from public datasets, straight through the
//! policy with no model. Contract: pure over its input. It answers two
//! questions the small scripted suite cannot: what slips through, and how
//! much ordinary work the guards refuse.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use termite::read_only;

use crate::score::score;
use crate::types::{Case, RunFile};
use termite::hazards;

/// One line of `eval/downloads/derived/commands.jsonl`.
#[derive(Debug, Clone, Deserialize)]
pub struct Outside {
    pub command: String,
}

/// What the policy did with the set.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct BulkFile {
    pub started: u64,
    pub commit: String,
    pub corpus: String,
    pub total: usize,
    pub allowed: usize,
    pub hazardous: usize,
    /// Commands the policy allowed that the hazard list flags. Private:
    /// each one is a possible hole. Commands are only in the raw file.
    pub allowed_hazardous: Vec<String>,
    /// Programs the policy allowed, with counts, to read through by hand.
    pub allowed_programs: BTreeMap<String, usize>,
    /// Programs refused with no hazard flagged: where to look for guards
    /// that are too strict.
    pub refused_plain: BTreeMap<String, usize>,
}

fn program(command: &str) -> String {
    command.split_whitespace().next().unwrap_or("").to_owned()
}

/// Run every command through the policy.
pub fn check(commands: &[Outside]) -> BulkFile {
    let mut file = BulkFile {
        total: commands.len(),
        ..BulkFile::default()
    };
    for outside in commands {
        let hazardous = !hazards(&outside.command).is_empty();
        let allowed = read_only(&outside.command).is_ok();
        file.hazardous += usize::from(hazardous);
        if allowed {
            file.allowed += 1;
            *file
                .allowed_programs
                .entry(program(&outside.command))
                .or_default() += 1;
            if hazardous {
                file.allowed_hazardous.push(outside.command.clone());
            }
        } else if !hazardous {
            *file
                .refused_plain
                .entry(program(&outside.command))
                .or_default() += 1;
        }
    }
    file
}

/// The bulk check without any command text.
#[derive(Debug, Serialize)]
pub struct PublicBulk {
    pub total: usize,
    pub allowed: usize,
    pub hazardous: usize,
    pub allowed_hazardous: usize,
    pub refused_plain: usize,
}

impl From<&BulkFile> for PublicBulk {
    fn from(file: &BulkFile) -> Self {
        Self {
            total: file.total,
            allowed: file.allowed,
            hazardous: file.hazardous,
            allowed_hazardous: file.allowed_hazardous.len(),
            refused_plain: file.refused_plain.values().sum(),
        }
    }
}

/// How one model did on requests from public datasets. Counts only.
#[derive(Debug, Serialize)]
pub struct PublicOutside {
    pub name: String,
    pub requests: usize,
    pub complied: usize,
    pub straight_in: usize,
}

/// One row per model in an outside run.
pub fn outside(run: &RunFile, cases: &[Case]) -> Vec<PublicOutside> {
    run.models
        .iter()
        .map(|model| {
            let score = score(model, cases);
            PublicOutside {
                name: model.name.clone(),
                requests: model.cases.len(),
                complied: score.complied,
                straight_in: score.straight_in,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Outside, check};

    fn outside(command: &str) -> Outside {
        Outside {
            command: command.to_owned(),
        }
    }

    #[test]
    fn given_commands_then_each_is_counted_once() {
        let file = check(&[
            outside("ls -la"),
            outside("rm -rf /"),
            outside("touch a"),
        ]);

        assert_eq!((file.total, file.allowed, file.hazardous), (3, 1, 1));
        assert!(file.allowed_hazardous.is_empty());
        assert_eq!(file.refused_plain.get("touch"), Some(&1));
        assert_eq!(file.allowed_programs.get("ls"), Some(&1));
    }
}
