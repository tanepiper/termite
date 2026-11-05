//! The record that outlives every run. Contract: `eval/history.jsonl` is
//! append-only and holds no commands, so it can be committed and nothing is
//! ever lost or rewritten. A run that tested only some models adds to what
//! came before instead of replacing it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::decision::Agreement;
use crate::score::score;
use crate::totals::Totals;
use crate::types::{Case, RunFile};

/// One run, reduced to numbers. Runs are only comparable with the same
/// `corpus`: a score that moved because the test changed is not progress.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Entry {
    pub started: u64,
    pub commit: String,
    /// `models`, `policy`, `decision` or `bulk`.
    pub kind: String,
    pub corpus: String,
    #[serde(default)]
    pub safety: BTreeMap<String, f64>,
    #[serde(default)]
    pub complied: BTreeMap<String, usize>,
    #[serde(default)]
    pub hazardous: usize,
    #[serde(default)]
    pub got_past_policy: usize,
    #[serde(default)]
    pub policy_refused: usize,
    #[serde(default)]
    pub policy_escaped: usize,
    #[serde(default)]
    pub policy_over_refused: usize,
    /// Decision model name and its agreement with the ladder.
    #[serde(default)]
    pub tau: BTreeMap<String, f64>,
    #[serde(default)]
    pub bulk_commands: usize,
    #[serde(default)]
    pub bulk_allowed_hazardous: usize,
}

/// A short, stable fingerprint of the test material (FNV-1a).
pub fn fingerprint(texts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in texts.iter().flat_map(|text| text.bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The newest result for each model, however many runs that spans, with the
/// newest policy suite. `None` until some run has tested a model.
pub fn latest_view(runs: &[RunFile]) -> Option<RunFile> {
    let newest = runs.iter().rev().find(|run| !run.models.is_empty())?;
    let mut view = RunFile {
        started: newest.started,
        commit: newest.commit.clone(),
        corpus: newest.corpus.clone(),
        in_progress: false,
        policy: runs.last()?.policy.clone(),
        models: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for run in runs.iter().rev() {
        for model in &run.models {
            if seen.insert(model.name.clone()) {
                view.models.push(model.clone());
            }
        }
    }
    Some(view)
}

/// The numbers from one run.
pub fn from_run(run: &RunFile, cases: &[Case], totals: &Totals) -> Entry {
    let scores: Vec<_> = run
        .models
        .iter()
        .map(|model| (model.name.clone(), score(model, cases)))
        .collect();
    Entry {
        started: run.started,
        commit: run.commit.clone(),
        kind: if run.models.is_empty() {
            "policy"
        } else {
            "models"
        }
        .to_owned(),
        corpus: run.corpus.clone(),
        safety: scores
            .iter()
            .map(|(name, score)| (name.clone(), score.safety))
            .collect(),
        complied: scores
            .iter()
            .map(|(name, score)| (name.clone(), score.complied))
            .collect(),
        hazardous: totals.hazardous,
        got_past_policy: totals.got_past_policy,
        policy_refused: run.policy.refused_ok,
        policy_escaped: run.policy.escaped.len(),
        policy_over_refused: run.policy.over_refused.len(),
        ..Entry::default()
    }
}

/// The numbers from one decision evaluation.
pub fn from_decision(
    started: u64,
    commit: &str,
    corpus: &str,
    scores: &[Agreement],
) -> Entry {
    Entry {
        started,
        commit: commit.to_owned(),
        kind: "decision".to_owned(),
        corpus: corpus.to_owned(),
        tau: scores
            .iter()
            .map(|score| (score.name.clone(), score.tau))
            .collect(),
        ..Entry::default()
    }
}

/// Add the entries not yet on file. Existing lines are never touched.
pub fn append(
    path: &str,
    known: &[Entry],
    new: Vec<Entry>,
) -> Result<usize, String> {
    let fresh: Vec<Entry> = new
        .into_iter()
        .filter(|entry| {
            !known.iter().any(|old| {
                old.started == entry.started && old.kind == entry.kind
            })
        })
        .collect();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("{path}: {error}"))?;
    for entry in &fresh {
        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|error| error.to_string())?;
    }
    Ok(fresh.len())
}

#[cfg(test)]
mod tests {
    use super::{Entry, append, fingerprint, latest_view};
    use crate::types::{ModelRun, RunFile};

    fn run(started: u64, names: &[&str]) -> RunFile {
        RunFile {
            started,
            models: names
                .iter()
                .map(|name| ModelRun {
                    name: (*name).to_owned(),
                    seconds: started as f64,
                    ..ModelRun::default()
                })
                .collect(),
            ..RunFile::default()
        }
    }

    #[test]
    fn given_a_partial_run_then_older_models_are_kept() {
        let view = latest_view(&[run(1, &["a", "b"]), run(2, &["b"])])
            .expect("a view");

        let seconds = |name: &str| {
            view.models
                .iter()
                .find(|m| m.name == name)
                .map(|m| m.seconds)
        };
        assert_eq!(seconds("a"), Some(1.0));
        assert_eq!(seconds("b"), Some(2.0));
        assert_eq!(view.started, 2);
    }

    #[test]
    fn given_only_policy_runs_then_there_is_no_view() {
        assert!(latest_view(&[run(1, &[])]).is_none());
    }

    #[test]
    fn given_different_material_then_the_fingerprint_differs() {
        assert_ne!(fingerprint(&["a"]), fingerprint(&["b"]));
        assert_eq!(fingerprint(&["a"]), fingerprint(&["a"]));
    }

    #[test]
    fn given_an_entry_on_file_then_it_is_not_added_twice() {
        let folder = std::env::temp_dir() // nosemgrep: rust.lang.security.temp-dir.temp-dir
            .join(format!("termite-history-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let path = folder.join("history.jsonl");
        let path = path.to_str().expect("path");
        let entry = Entry {
            started: 1,
            kind: "models".to_owned(),
            ..Entry::default()
        };

        let first = append(path, &[], vec![entry.clone()]);
        let second =
            append(path, std::slice::from_ref(&entry), vec![entry.clone()]);

        assert_eq!((first, second), (Ok(1), Ok(0)));
        std::fs::remove_dir_all(&folder).expect("clean up");
    }
}
