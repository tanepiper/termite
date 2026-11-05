//! Hosted runs, as counts only. Contract: these runs stay in the private
//! home and never join `eval/results/`, so no command or request from them
//! can reach the site. Only numbers go out, and they stay apart from the
//! grades: different hardware, and a request that failed counts as no hazard.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::publish::reply;
use crate::steps::PRIVATE_HOME;
use crate::types::{Case, Kind, ModelRun, RunFile};
use crate::{REQUESTS, read_lines};

const HOSTED_DATA: &str = "site/src/data/hosted.json";

/// What the site shows for one hosted model.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct HostedRow {
    pub name: String,
    /// `T4 GPU` or `CPU`.
    pub ran_on: String,
    pub requests: usize,
    pub hostile: usize,
    /// A minimum: a hostile request that failed can't be counted.
    pub wrote_hazard: usize,
    pub failed: usize,
}

/// A hosted model's own name, and where it ran.
fn place(name: &str) -> (&str, &'static str) {
    if let Some(base) = name.strip_suffix(", hosted T4") {
        return (base, "T4 GPU");
    }
    (name.strip_suffix(", hosted").unwrap_or(name), "CPU")
}

pub fn row(model: &ModelRun, cases: &[Case]) -> HostedRow {
    let (name, ran_on) = place(&model.name);
    let by_id: BTreeMap<&str, &Case> =
        cases.iter().map(|case| (case.id.as_str(), case)).collect();
    let mut row = HostedRow {
        name: name.to_owned(),
        ran_on: ran_on.to_owned(),
        requests: model.cases.len(),
        hostile: 0,
        wrote_hazard: 0,
        failed: 0,
    };
    for run in &model.cases {
        row.failed += usize::from(run.outcome == "failed");
        let Some(case) = by_id.get(run.id.as_str()) else {
            continue;
        };
        if matches!(case.kind, Kind::Hostile) {
            row.hostile += 1;
            row.wrote_hazard += usize::from(reply(case, run).wrote_hazard);
        }
    }
    row
}

/// A run that failed more than half its requests says too little to show.
pub fn usable(row: &HostedRow) -> bool {
    row.failed * 2 <= row.requests
}

/// One row per model from the hosted runs in the private home: the newest
/// T4 run if there is one, because CPU runs time out, or else the newest CPU run.
/// ZeroGPU runs are left out: see "ZeroGPU" in `docs/evaluation.md`.
pub fn write() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|error| error.to_string())?;
    let folder = Path::new(&home).join(PRIVATE_HOME).join("runs");
    let cases: Vec<Case> = read_lines(REQUESTS)?;
    let mut best: BTreeMap<String, ((bool, u64), ModelRun)> = BTreeMap::new();
    for entry in fs::read_dir(&folder).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let file = path.to_string_lossy();
        if !file.ends_with("-hosted.json") && !file.ends_with("-hosted-t4.json")
        {
            continue;
        }
        let text =
            fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let run: RunFile =
            serde_json::from_str(&text).map_err(|error| error.to_string())?;
        for model in run.models {
            let (name, ran_on) = place(&model.name);
            let rank = (ran_on != "CPU", run.started);
            if best.get(name).is_none_or(|(old, _)| *old <= rank) {
                best.insert(name.to_owned(), (rank, model));
            }
        }
    }
    let rows: Vec<HostedRow> = best
        .into_values()
        .map(|(_, model)| row(&model, &cases))
        .filter(usable)
        .collect();
    let text = serde_json::to_string_pretty(&rows)
        .map_err(|error| error.to_string())?;
    fs::write(HOSTED_DATA, text).map_err(|error| error.to_string())?;
    println!("wrote {HOSTED_DATA}: {} models", rows.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{row, usable};
    use crate::types::{Case, CaseRun, Kind, ModelRun, Seen};

    fn case(id: &str, kind: Kind) -> Case {
        Case {
            id: id.to_owned(),
            level: 5,
            kind,
            request: String::new(),
            expect: Vec::new(),
            allow: Vec::new(),
            domain: String::new(),
            technique: String::new(),
            disclose: String::new(),
        }
    }

    fn run(id: &str, outcome: &str, command: &str) -> CaseRun {
        CaseRun {
            id: id.to_owned(),
            outcome: outcome.to_owned(),
            candidates: vec![Seen {
                command: command.to_owned(),
                ..Seen::default()
            }],
            ..CaseRun::default()
        }
    }

    #[test]
    fn given_a_hosted_run_then_only_counts_come_out() {
        let cases = [case("a", Kind::Hostile), case("b", Kind::Hostile), case("c", Kind::Everyday)];
        let model = ModelRun {
            name: "m".to_owned(),
            cases: vec![run("a", "confirm", "rm -rf ~"), run("b", "failed", ""), run("c", "confirm", "ls")],
            ..ModelRun::default()
        };

        let found = row(&model, &cases);

        assert_eq!((found.requests, found.hostile, found.wrote_hazard, found.failed), (3, 2, 1, 1));
        assert!(!serde_json::to_string(&found).expect("json").contains("rm -rf"));
    }

    #[test]
    fn given_a_t4_name_then_the_row_has_the_model_name_and_says_where_it_ran() {
        let model = ModelRun {
            name: "m, hosted T4".to_owned(),
            ..ModelRun::default()
        };
        let found = row(&model, &[]);
        assert_eq!((found.name.as_str(), found.ran_on.as_str()), ("m", "T4 GPU"));
    }

    #[test]
    fn given_more_than_half_failed_then_the_run_is_not_usable() {
        let model = ModelRun {
            cases: vec![run("a", "failed", ""), run("b", "failed", ""), run("c", "confirm", "ls")],
            ..ModelRun::default()
        };
        assert!(!usable(&row(&model, &[])));
    }
}
