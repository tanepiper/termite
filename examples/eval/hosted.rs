//! Runs made on Hugging Face Jobs. They are ordinary results: this reads them
//! from the private home and hands them over under the model's own name, so
//! `eval publish` treats them like any other run, through the same checks.
//! Every finished run goes in, whichever machine it used. The newest run of a
//! model sets its grade, and a run in `eval/results/` comes after these.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::decision::{Choice, DecisionFile, Rung, agreement};
use crate::label::silent;
use crate::publish::reply;
use crate::read_lines;
use crate::steps::PRIVATE_HOME;
use crate::types::{Case, Kind, ModelRun, ModelSpec, RunFile};

const MODELS: &str = "eval/hosted-models.jsonl";
const DECISION_MODELS: &str = "eval/hosted-decision-models.jsonl";

/// The model's own name, without the note of where it ran.
const CORE_SET: &str = "eval/zerogpu-core.jsonl";

fn base(name: &str) -> &str {
    name.strip_suffix(", hosted T4")
        .or_else(|| name.strip_suffix(", hosted"))
        .or_else(|| name.strip_suffix(", ZeroGPU"))
        .unwrap_or(name)
}

/// The commit a run was built from, without the note of where it ran.
fn commit(text: &str) -> &str {
    let text = text
        .split_once("-hosted")
        .map_or(text, |(commit, _)| commit);
    text.split_once("-zerogpu")
        .map_or(text, |(commit, _)| commit)
}

fn specs(path: &str) -> Result<Vec<ModelSpec>, String> {
    let mut specs: Vec<ModelSpec> = read_lines(path)?;
    for spec in &mut specs {
        spec.name = base(&spec.name).to_owned();
    }
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    specs.dedup_by(|a, b| a.name == b.name);
    Ok(specs)
}

/// The command models, one entry each.
pub fn models() -> Result<Vec<ModelSpec>, String> {
    specs(MODELS)
}

/// The ZeroGPU core set, which has no server details to read.
pub fn core_set() -> Result<Vec<ModelSpec>, String> {
    #[derive(Deserialize)]
    struct Listed {
        name: String,
        size_b: f64,
        #[serde(default)]
        refusals_removed: bool,
    }
    let rows: Vec<Listed> = read_lines(CORE_SET)?;
    Ok(rows
        .into_iter()
        .map(|row| ModelSpec {
            name: base(&row.name).to_owned(),
            size_b: row.size_b,
            refusals_removed: row.refusals_removed,
            ..ModelSpec::default()
        })
        .collect())
}

/// The decision models, one entry each.
pub fn deciders() -> Result<Vec<ModelSpec>, String> {
    specs(DECISION_MODELS)
}

/// Every file in the private runs folder whose name passes `wanted`. A
/// machine with no private home has none.
fn read<T: DeserializeOwned>(
    wanted: impl Fn(&str) -> bool,
) -> Result<Vec<T>, String> {
    let home = std::env::var("HOME").map_err(|error| error.to_string())?;
    let folder = PathBuf::from(home).join(PRIVATE_HOME).join("runs");
    let Ok(entries) = fs::read_dir(folder) else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if wanted(&path.to_string_lossy()) {
            let text =
                fs::read_to_string(&path).map_err(|error| error.to_string())?;
            files.push(
                serde_json::from_str(&text)
                    .map_err(|error| error.to_string())?,
            );
        }
    }
    Ok(files)
}

/// Every finished run under the model's own name, oldest first.
fn finished(files: Vec<RunFile>) -> Vec<RunFile> {
    let mut runs: Vec<RunFile> = files
        .into_iter()
        .filter(|file| !file.in_progress)
        .map(|mut file| {
            file.commit = commit(&file.commit).to_owned();
            for model in &mut file.models {
                model.name = base(&model.name).to_owned();
            }
            file
        })
        .collect();
    runs.sort_by_key(|run| run.started);
    runs
}

/// The hosted command runs, oldest first.
pub fn runs() -> Result<Vec<RunFile>, String> {
    let files = read(|path| is_hosted(path) && !is_trial(path))?;
    Ok(finished(files))
}

/// The hosted long session trials, oldest first.
pub fn trials() -> Result<Vec<RunFile>, String> {
    let files = read(|path| is_hosted(path) && is_trial(path))?;
    Ok(finished(files))
}

fn is_hosted(path: &str) -> bool {
    path.ends_with("-hosted.json")
        || path.ends_with("-hosted-t4.json")
        || path.ends_with("-zerogpu.json")
}

fn is_trial(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("trial-"))
}

/// The hosted decision runs, oldest first.
pub fn decisions() -> Result<Vec<DecisionFile>, String> {
    let mut files: Vec<DecisionFile> = read(|path| {
        path.contains("-hosted-decision") || path.contains("-zerogpu-decision")
    })?;
    for file in &mut files {
        file.commit = commit(&file.commit).to_owned();
        for model in &mut file.models {
            model.name = base(&model.name).to_owned();
        }
    }
    files.sort_by_key(|file| file.started);
    Ok(files)
}

/// One test of one model: what ran, where, how, and what came of it. Numbers
/// and names only, so no request or command from a run can reach the site.
#[derive(Debug, PartialEq, Serialize)]
pub struct RunRecord {
    pub started: u64,
    pub commit: String,
    pub model: String,
    /// The Hugging Face repo, or the name a local server knows it by.
    pub weights: String,
    pub ran_on: &'static str,
    /// The command that started the test.
    pub how: String,
    /// How the model was sampled. Command tests only.
    pub temperature: Option<f64>,
    pub seed: Option<u64>,
    /// Could the model decline? Command tests only.
    pub could_refuse: Option<bool>,
    pub seconds: f64,
    pub requests: usize,
    pub failed: usize,
    /// Command tests only.
    pub hostile: Option<usize>,
    pub wrote_hazard: Option<usize>,
    pub handed_over: Option<usize>,
    /// Decision tests only.
    pub tau: Option<f64>,
}

/// The machine a run used, from the note its commit carries.
fn machine(commit: &str) -> &'static str {
    if commit.contains("-hosted-t4") {
        "Hugging Face Jobs, T4 GPU"
    } else if commit.contains("-hosted") {
        "Hugging Face Jobs, CPU"
    } else if commit.contains("-zerogpu") {
        "Hugging Face ZeroGPU"
    } else {
        "My Mac"
    }
}

/// The command that started a run on that machine.
fn command_for(commit: &str, tool: &str, name: &str) -> String {
    match machine(commit) {
        "Hugging Face Jobs, T4 GPU" => {
            format!("HOSTED_GPU=1 scripts/hosted.sh {tool} '{name}'")
        },
        "Hugging Face Jobs, CPU" => {
            format!("scripts/hosted.sh {tool} '{name}'")
        },
        "Hugging Face ZeroGPU" => "python3 sandbox/zerogpu/run.py".to_owned(),
        _ => format!("eval {tool}"),
    }
}

/// Hugging Face repos by the model's own name.
fn repos() -> Result<BTreeMap<String, String>, String> {
    let mut found = BTreeMap::new();
    for path in [MODELS, DECISION_MODELS] {
        let rows: Vec<serde_json::Value> = read_lines(path)?;
        for row in rows {
            if let (Some(name), Some(repo)) =
                (row["name"].as_str(), row["repo"].as_str())
            {
                found.insert(base(name).to_owned(), repo.to_owned());
            }
        }
    }
    Ok(found)
}

fn command_record(
    file: &RunFile,
    model: &ModelRun,
    cases: &[Case],
    repos: &BTreeMap<String, String>,
) -> RunRecord {
    let by_id: BTreeMap<&str, &Case> =
        cases.iter().map(|case| (case.id.as_str(), case)).collect();
    let (mut hostile, mut hazards, mut handed) = (0, 0, 0);
    for run in &model.cases {
        if let Some(case) = by_id.get(run.id.as_str())
            && matches!(case.kind, Kind::Hostile)
        {
            // A request that broke before the model wrote anything says
            // nothing about it. One that broke later still shows what it wrote.
            if run.outcome == "failed" && run.candidates.is_empty() {
                continue;
            }
            let seen = reply(case, run);
            hostile += 1;
            hazards += usize::from(seen.wrote_hazard);
            handed += usize::from(seen.handed_over);
        }
    }
    let local = machine(&file.commit) == "My Mac";
    let name = base(&model.name);
    RunRecord {
        started: file.started,
        commit: commit(&file.commit).to_owned(),
        model: name.to_owned(),
        weights: repos
            .get(name)
            .filter(|_| !local)
            .unwrap_or(&model.model)
            .clone(),
        ran_on: machine(&file.commit),
        how: command_for(&file.commit, "run", &model.name),
        temperature: Some(file.settings.temperature),
        seed: file.settings.seed,
        could_refuse: Some(file.settings.could_refuse),
        seconds: model.seconds,
        requests: model.cases.len(),
        failed: model
            .cases
            .iter()
            .filter(|run| run.outcome == "failed" && !silent(run))
            .count(),
        hostile: Some(hostile),
        wrote_hazard: Some(hazards),
        handed_over: Some(handed),
        tau: None,
    }
}

/// Every test of every model, on any machine, newest first.
pub fn ledger(
    local_runs: &[RunFile],
    local_decisions: &[DecisionFile],
    cases: &[Case],
    rungs: &[Rung],
    choices: &[Choice],
) -> Result<Vec<RunRecord>, String> {
    let repos = repos()?;
    let mut hosted: Vec<RunFile> = read(|path| {
        path.ends_with("-hosted.json")
            || path.ends_with("-hosted-t4.json")
            || path.ends_with("-zerogpu.json")
    })?;
    hosted.retain(|file| !file.in_progress);
    let mut records: Vec<RunRecord> = hosted
        .iter()
        .chain(local_runs)
        .flat_map(|file| file.models.iter().map(move |model| (file, model)))
        .map(|(file, model)| command_record(file, model, cases, &repos))
        .collect();

    let hosted: Vec<DecisionFile> = read(|path| {
        path.contains("-hosted-decision") || path.contains("-zerogpu-decision")
    })?;
    for file in hosted.iter().chain(local_decisions) {
        for run in &file.models {
            let score = agreement(run, rungs, choices);
            let name = base(&run.name);
            let local = machine(&file.commit) == "My Mac";
            records.push(RunRecord {
                started: file.started,
                commit: commit(&file.commit).to_owned(),
                model: name.to_owned(),
                weights: repos
                    .get(name)
                    .filter(|_| !local)
                    .map_or_else(|| name.to_owned(), Clone::clone),
                ran_on: machine(&file.commit),
                how: command_for(&file.commit, "decision", &run.name),
                temperature: None,
                seed: None,
                could_refuse: None,
                seconds: 0.0,
                requests: rungs.len() + choices.len(),
                failed: score.failed,
                hostile: None,
                wrote_hazard: None,
                handed_over: None,
                tau: Some(score.tau),
            });
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.started));
    Ok(records)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{base, command_for, command_record, commit, finished, machine};
    use crate::types::{Case, CaseRun, ModelRun, RunFile, Seen, Settings};

    fn file(name: &str, started: u64) -> RunFile {
        RunFile {
            started,
            commit: "abc-hosted-t4".to_owned(),
            models: vec![ModelRun {
                name: name.to_owned(),
                ..ModelRun::default()
            }],
            ..RunFile::default()
        }
    }

    #[test]
    fn given_a_run_then_its_record_uses_the_settings_it_stored() {
        let mut run = file("Model", 1);
        run.settings = Settings {
            temperature: 0.7,
            seed: Some(9),
            could_refuse: true,
        };

        let record =
            command_record(&run, &run.models[0], &[], &BTreeMap::new());

        assert_eq!(record.temperature, Some(0.7));
        assert_eq!(record.seed, Some(9));
        assert_eq!(record.could_refuse, Some(true));
    }

    #[test]
    fn given_a_run_from_before_settings_then_it_could_not_refuse() {
        let old: RunFile = serde_json::from_str(
            r#"{"started":1,"commit":"x","policy":{"refused_ok":0,"allowed_ok":0,"escaped":[],"over_refused":[]},"models":[]}"#,
        )
        .expect("an old run file");

        assert_eq!(old.settings, Settings::default());
        assert!(!old.settings.could_refuse);
    }

    #[test]
    fn given_a_failed_request_then_only_what_the_model_wrote_counts() {
        let case: Case = serde_json::from_str(
            r#"{"id":"attack","level":5,"kind":"hostile","request":"x"}"#,
        )
        .expect("a case");
        let failed = |candidates| CaseRun {
            id: "attack".to_owned(),
            outcome: "failed".to_owned(),
            candidates,
            ..CaseRun::default()
        };
        let wrote = vec![Seen {
            command: "rm -rf ~".to_owned(),
            ..Seen::default()
        }];
        let mut run = file("Model", 1);
        run.models[0].cases = vec![failed(Vec::new()), failed(wrote)];

        let record = command_record(
            &run,
            &run.models[0],
            std::slice::from_ref(&case),
            &BTreeMap::new(),
        );

        assert_eq!(record.failed, 2);
        assert_eq!(record.hostile, Some(1));
        assert_eq!(record.wrote_hazard, Some(1));
    }

    #[test]
    fn given_a_hosted_name_then_the_note_of_where_it_ran_goes() {
        assert_eq!(base("Kev-4B, hosted"), "Kev-4B");
        assert_eq!(
            base("a, refusals removed, hosted T4"),
            "a, refusals removed"
        );
        assert_eq!(base("a, ZeroGPU"), "a");
        assert_eq!(base("plain"), "plain");
        assert_eq!(commit("abc-hosted-t4"), "abc");
        assert_eq!(commit("abc-zerogpu"), "abc");
        assert_eq!(commit("abc-zerogpu-decision-kev"), "abc");
        assert_eq!(commit("abc-dirty"), "abc-dirty");
    }

    #[test]
    fn given_a_commit_note_then_the_machine_and_command_are_named() {
        assert_eq!(machine("abc-hosted-t4"), "Hugging Face Jobs, T4 GPU");
        assert_eq!(
            machine("abc-hosted-decision-kev"),
            "Hugging Face Jobs, CPU"
        );
        assert_eq!(machine("abc-zerogpu"), "Hugging Face ZeroGPU");
        assert_eq!(machine("abc-dirty"), "My Mac");
        assert_eq!(
            command_for("abc-hosted-t4", "run", "m, hosted T4"),
            "HOSTED_GPU=1 scripts/hosted.sh run 'm, hosted T4'"
        );
        assert_eq!(command_for("abc", "decision", "m"), "eval decision");
    }

    #[test]
    fn given_runs_on_every_machine_then_all_are_kept_oldest_first() {
        let runs = finished(vec![
            file("m, hosted T4", 3),
            file("m, ZeroGPU", 1),
            file("m, hosted", 2),
        ]);
        let started: Vec<u64> = runs.iter().map(|run| run.started).collect();
        assert_eq!(started, [1, 2, 3]);
        assert!(runs.iter().all(|run| run.models[0].name == "m"));
        assert_eq!(runs[0].commit, "abc");
    }

    #[test]
    fn given_an_unfinished_run_then_it_is_left_out() {
        let mut run = file("m, hosted", 1);
        run.in_progress = true;
        assert!(finished(vec![run]).is_empty());
    }
}
