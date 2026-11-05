//! The commands that read finished results and write something from them.
//! Contract: this is the only place that knows where those files live.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use termite::{PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, write_private};

use crate::bulk::{self, BulkFile, Outside, PublicBulk};
use crate::decision::{self, Agreement, Choice, DecisionFile, Rung, agreement};
use crate::history::{self, Entry, latest_view};
use crate::observe::{self, Suite, Write};
use crate::private::{self, KnownIssue};
use crate::private_page::{index_page, recommendations, report_page, stamp};
use crate::publish;
use crate::types::{Case, ModelSpec, RunFile};
use crate::unload;
use crate::{
    COMMANDS, MODELS, REQUESTS, RESULTS, now, read_lines, read_results,
    sorted_runs,
};

pub(crate) const LADDER: &str = "eval/corpus/ladder.jsonl";
pub(crate) const CHOICES: &str = "eval/corpus/choices.jsonl";
pub(crate) const DECISION_MODELS: &str = "eval/decision-models.jsonl";
const OUTSIDE_COMMANDS: &str = "eval/downloads/derived/commands.jsonl";
const OBSERVATIONS: &str = "eval/observations";
const OUTSIDE_REQUESTS: &str = "eval/downloads/derived/requests.jsonl";
pub(crate) const KNOWN_ISSUES: &str = "eval/known-issues.jsonl";
const PUBLIC_DATA: &str = "site/src/data/results.json";
const HISTORY: &str = "eval/history.jsonl";
/// Below the home folder, so no git command can ever pick it up.
pub const PRIVATE_HOME: &str = "termite-private";

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))
}

/// What the test material was when a run started.
pub fn corpus_fingerprint(requests: &str) -> Result<String, String> {
    Ok(history::fingerprint(&[&read(requests)?, &read(COMMANDS)?]))
}

fn save<T: serde::Serialize>(path: &str, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}

/// Rate the ladder with every decision model.
pub fn write_decision(commit: &str) -> Result<(), String> {
    let rungs: Vec<Rung> = read_lines(LADDER)?;
    let choices: Vec<Choice> = read_lines(CHOICES)?;
    let models: Vec<ModelSpec> = read_lines(DECISION_MODELS)?;
    let home = decision::fixture(&rungs)?;

    let started = now()?;
    let mut file = DecisionFile {
        started,
        commit: commit.to_owned(),
        models: Vec::new(),
    };
    fs::create_dir_all(RESULTS).map_err(|error| error.to_string())?;
    let path = format!("{RESULTS}/decision-{started}-{commit}.json");
    for spec in &models {
        println!("{}", spec.name);
        let run = decision::run_model(spec, &rungs, &choices, &home);
        unload::release(spec);
        file.models.push(run?);
        save(&path, &file)?;
    }
    fs::remove_dir_all(&home).map_err(|error| error.to_string())?;
    println!("saved {path}");
    Ok(())
}

/// Run the outside commands through the policy.
pub fn write_bulk(commit: &str) -> Result<(), String> {
    let text = read(OUTSIDE_COMMANDS)?;
    let commands: Vec<Outside> = read_lines(OUTSIDE_COMMANDS)?;
    let started = now()?;
    let file = BulkFile {
        started,
        commit: commit.to_owned(),
        corpus: history::fingerprint(&[&text]),
        ..bulk::check(&commands)
    };
    println!(
        "bulk: {} commands, {} allowed, {} flagged, {} allowed and flagged",
        file.total,
        file.allowed,
        file.hazardous,
        file.allowed_hazardous.len()
    );
    fs::create_dir_all(RESULTS).map_err(|error| error.to_string())?;
    save(&format!("{RESULTS}/bulk-{started}-{commit}.json"), &file)
}

fn finished(prefix: &str) -> Vec<RunFile> {
    let mut runs: Vec<RunFile> = read_results(prefix);
    runs.retain(|run| !run.in_progress);
    runs
}

/// Every run, oldest first, wherever it ran. Where a model ran doesn't matter,
/// so its newest run wins.
fn all_runs() -> Result<Vec<RunFile>, String> {
    let mut runs = crate::hosted::runs()?;
    runs.extend(sorted_runs());
    runs.sort_by_key(|run| run.started);
    Ok(runs)
}

/// Runs, holding only the models still listed for commands.
fn listed_runs() -> Result<Vec<RunFile>, String> {
    let mut specs: Vec<ModelSpec> = read_lines(MODELS)?;
    specs.extend(crate::hosted::models()?);
    let mut names: BTreeSet<String> =
        specs.into_iter().map(|spec| spec.name).collect();
    names.extend(crate::hosted::core_set()?);
    let mut runs = all_runs()?;
    for run in &mut runs {
        run.models.retain(|model| names.contains(&model.name));
    }
    Ok(runs)
}

fn newest_by<T>(mut files: Vec<T>, started: impl Fn(&T) -> u64) -> Option<T> {
    files.sort_by_key(started);
    files.pop()
}

/// Each model's newest result, wherever it ran.
fn scores(
    decisions: &[DecisionFile],
    rungs: &[Rung],
    choices: &[Choice],
) -> Vec<Agreement> {
    let mut newest_first: Vec<&DecisionFile> = decisions.iter().collect();
    newest_first.sort_by_key(|file| std::cmp::Reverse(file.started));
    let mut found: Vec<Agreement> = Vec::new();
    for run in newest_first.iter().flat_map(|file| &file.models) {
        if !found.iter().any(|score| score.name == run.name) {
            found.push(agreement(run, rungs, choices));
        }
    }
    found
}

fn sorted_decisions() -> Vec<DecisionFile> {
    let mut files: Vec<DecisionFile> = read_results("decision-");
    files.sort_by_key(|file| file.started);
    files
}

/// Add every result on disk to the permanent history, then read it back.
fn update_history(
    runs: &[RunFile],
    cases: &[Case],
    decisions: &[DecisionFile],
    rungs: &[Rung],
    choices: &[Choice],
    bulks: &[BulkFile],
) -> Result<Vec<Entry>, String> {
    let known: Vec<Entry> = if Path::new(HISTORY).exists() {
        read_lines(HISTORY)?
    } else {
        Vec::new()
    };
    let ladder = history::fingerprint(&[&read(LADDER)?, &read(CHOICES)?]);
    let mut new: Vec<Entry> = publish::trend(runs, cases);
    for file in decisions {
        let scored = scores(std::slice::from_ref(file), rungs, choices);
        new.push(history::from_decision(
            file.started,
            &file.commit,
            &ladder,
            &scored,
        ));
    }
    for file in bulks {
        new.push(Entry {
            started: file.started,
            commit: file.commit.clone(),
            kind: "bulk".to_owned(),
            corpus: file.corpus.clone(),
            bulk_commands: file.total,
            bulk_allowed_hazardous: file.allowed_hazardous.len(),
            ..Entry::default()
        });
    }
    let added = history::append(HISTORY, &known, new)?;
    println!("history: {added} new entries");
    read_lines(HISTORY)
}

/// Write one file of observations for each source. Files are committed, so
/// losing the raw runs loses no chart. Only `relabel` rewrites them.
fn record_observations(
    runs: &[RunFile],
    cases: &[Case],
    decisions: &[DecisionFile],
    rungs: &[Rung],
    write: Write,
) -> Result<(), String> {
    let mut specs: Vec<ModelSpec> = read_lines(MODELS)?;
    let mut decision: Vec<ModelSpec> = read_lines(DECISION_MODELS)?;
    decision.extend(crate::hosted::deciders()?);
    let deciders: Vec<String> =
        decision.iter().map(|spec| spec.name.clone()).collect();
    specs.extend(crate::hosted::models()?);
    specs.extend(decision);
    fs::create_dir_all(OBSERVATIONS).map_err(|error| error.to_string())?;

    let mut batches = Vec::new();
    for run in runs {
        let rows =
            observe::from_run(run, Suite::Core, cases, &specs, &deciders);
        batches.push((format!("core-{}", run.started), rows));
    }
    for run in finished("trial-") {
        let rows =
            observe::from_run(&run, Suite::Trial, cases, &specs, &deciders);
        batches.push((format!("trial-{}", run.started), rows));
    }
    if Path::new(OUTSIDE_REQUESTS).exists() {
        let outside: Vec<Case> = read_lines(OUTSIDE_REQUESTS)?;
        for run in finished("outside-") {
            let rows = observe::from_run(
                &run,
                Suite::Outside,
                &outside,
                &specs,
                &deciders,
            );
            batches.push((format!("outside-{}", run.started), rows));
        }
    }
    let ladder = history::fingerprint(&[&read(LADDER)?]);
    for file in decisions {
        let head = (file.started, file.commit.as_str(), ladder.as_str());
        let rows = observe::from_decision(head, &file.models, rungs, &specs);
        batches.push((format!("ladder-{}", file.started), rows));
    }

    observe::save_batches(OBSERVATIONS, batches, write)
}

/// Rewrite the recorded observations from the raw runs, after a change to
/// the label rules. It adds no source: `publish` does that.
pub fn relabel() -> Result<(), String> {
    let decisions: Vec<DecisionFile> = crate::hosted::decisions()?
        .into_iter()
        .chain(sorted_decisions())
        .collect();
    record_observations(
        &all_runs()?,
        &read_lines(REQUESTS)?,
        &decisions,
        &read_lines(LADDER)?,
        Write::Recorded,
    )
}

pub fn write_public() -> Result<(), String> {
    crate::families::refresh()?;
    let runs = listed_runs()?;
    let cases: Vec<Case> = read_lines(REQUESTS)?;
    let rungs: Vec<Rung> = read_lines(LADDER)?;
    let choices: Vec<Choice> = read_lines(CHOICES)?;
    let local = sorted_decisions();
    let hosted = crate::hosted::decisions()?;
    let decisions: Vec<DecisionFile> =
        hosted.iter().chain(&local).cloned().collect();
    let mut bulks: Vec<BulkFile> = read_results("bulk-");
    bulks.sort_by_key(|file| file.started);

    record_observations(&all_runs()?, &cases, &decisions, &rungs, Write::New)?;
    let mut data = publish::build(&runs, &cases)
        .ok_or("no model results yet: run `eval run` first")?;
    data.trend =
        update_history(&runs, &cases, &decisions, &rungs, &choices, &bulks)?;
    data.decision = scores(&decisions, &rungs, &choices);
    data.bulk = bulks.last().map(PublicBulk::from);
    let outside = finished("outside-");
    if let Some(run) = newest_by(outside, |run| run.started) {
        // Outside requests are never quoted, so only counts go out.
        data.outside = bulk::outside(&run, &outside_cases(&run)?);
    }
    data.runs = crate::hosted::ledger(
        &sorted_runs(),
        &local,
        &cases,
        &rungs,
        &choices,
    )?;
    crate::roster::write(&data)?;
    let text = publish::render(&data, &runs, &cases)?;
    fs::write(PUBLIC_DATA, text).map_err(|error| error.to_string())?;
    println!("wrote {PUBLIC_DATA}");
    Ok(())
}

/// Rework the team lists from the latest scores.
pub fn write_roster() -> Result<(), String> {
    let cases: Vec<Case> = read_lines(REQUESTS)?;
    let data = publish::build(&listed_runs()?, &cases)
        .ok_or("no model results yet: run `eval run` first")?;
    crate::roster::write(&data)
}

fn outside_cases(_run: &RunFile) -> Result<Vec<Case>, String> {
    read_lines(OUTSIDE_REQUESTS)
}

fn private_folder(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| error.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIR_MODE))
        .map_err(|error| error.to_string())
}

fn private_home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(PRIVATE_HOME))
        .ok_or_else(|| "no home folder".to_owned())
}

fn write(folder: &Path, name: &str, text: &str) -> Result<(), String> {
    write_private(folder, name, text.as_bytes(), PRIVATE_FILE_MODE)
        .map_err(|error| error.to_string())
        .map(|_| ())
}

/// A new dated report in the private home, and a fresh front page.
pub fn write_private_report() -> Result<(), String> {
    let runs = listed_runs()?;
    let latest =
        latest_view(&runs).ok_or("no model results yet: run `eval run`")?;
    let cases: Vec<Case> = read_lines(REQUESTS)?;
    let known: Vec<KnownIssue> = read_lines(KNOWN_ISSUES)?;
    let rungs: Vec<Rung> = read_lines(LADDER)?;
    let choices: Vec<Choice> = read_lines(CHOICES)?;
    let decisions = scores(&sorted_decisions(), &rungs, &choices);
    let bulks: Vec<BulkFile> = read_results("bulk-");
    let bulk = newest_by(bulks, |file| file.started);

    let data = publish::build(&runs, &cases).ok_or("no public data")?;
    let reached = data
        .queries
        .iter()
        .flat_map(|query| query.replies.values())
        .filter(|reply| reply.wrote_hazard && reply.ending == "asked")
        .count();
    let advice = recommendations(&latest, reached, &decisions, bulk.as_ref());

    let home = private_home()?;
    let reports = home.join("reports");
    private_folder(&home)?;
    private_folder(&reports)?;
    let when = stamp(now()?);
    let name = format!("{when}-{}", latest.commit);
    let folder = reports.join(&name);
    if folder.exists() {
        return Err(format!(
            "{name} already exists; reports are never replaced"
        ));
    }
    private_folder(&folder)?;

    let findings = private::render(&latest, &cases, &known);
    write(&folder, "findings.md", &findings)?;
    write(
        &folder,
        "index.html",
        &report_page(&when, &latest, &advice, &findings),
    )?;
    write(
        &folder,
        "summary.txt",
        &format!("{} things to look at", advice.len()),
    )?;

    let mut rows: Vec<(String, String)> = fs::read_dir(&reports)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let summary = fs::read_to_string(entry.path().join("summary.txt"))
                .unwrap_or_default();
            (name, summary)
        })
        .collect();
    rows.sort();
    rows.reverse();
    write(&home, "index.html", &index_page(&rows))?;
    println!("wrote {} (private)", folder.display());
    Ok(())
}
