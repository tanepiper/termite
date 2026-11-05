//! The tests that use no model: every step here sends commands straight to
//! the policy, and nothing is run.

use std::fs;

use crate::agree::{self, AgreeFile};
use crate::bulk::{self, BulkFile, Outside};
use crate::edges::{self, EdgeFile};
use crate::history;
use crate::steps::{read, save};
use crate::types::CommandCase;
use crate::{COMMANDS, RESULTS, now, read_lines, read_results};

const OUTSIDE_COMMANDS: &str = "eval/downloads/derived/commands.jsonl";

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

/// Pad the scripted commands to the lengths around the command limit.
pub fn write_edges(commit: &str) -> Result<(), String> {
    let commands: Vec<CommandCase> = read_lines(COMMANDS)?;
    let started = now()?;
    let file = EdgeFile {
        started,
        commit: commit.to_owned(),
        corpus: history::fingerprint(&[&read(COMMANDS)?]),
        ..edges::check(&commands)
    };
    println!(
        "edges: {} copies, {} blocked, {} kept, {} escaped, {} changed",
        file.copies,
        file.blocked,
        file.kept,
        file.escaped.len(),
        file.changed.len()
    );
    fs::create_dir_all(RESULTS).map_err(|error| error.to_string())?;
    save(&format!("{RESULTS}/edges-{started}-{commit}.json"), &file)
}

/// `agree BUILD COMMIT` records one build. `agree --compare` lists where
/// the newest builds disagree, by position only.
pub fn agree(rest: &[String]) -> Result<(), String> {
    if rest.first().map(String::as_str) == Some("--compare") {
        let files = agree::latest(read_results("agree-"));
        let found = agree::compare(&files);
        for (index, seen) in found.iter().take(40) {
            println!("{index}: {seen:?}");
        }
        println!("agree: {} builds, {} differ", files.len(), found.len());
        return Ok(());
    }
    let [build, commit] = rest else {
        return Err(
            "usage: eval agree BUILD COMMIT | eval agree --compare".to_owned()
        );
    };
    let scripted: Vec<CommandCase> = read_lines(COMMANDS)?;
    let scripted: Vec<String> =
        scripted.into_iter().map(|case| case.command).collect();
    let outside: Vec<Outside> = read_lines(OUTSIDE_COMMANDS)?;
    let outside: Vec<String> =
        outside.into_iter().map(|row| row.command).collect();
    let inputs = agree::inputs(&scripted, &outside);
    let root = agree::folder();
    agree::make_folder(&root)?;
    let started = now()?;
    let file = AgreeFile {
        build: build.clone(),
        started,
        commit: commit.clone(),
        corpus: history::fingerprint(&[
            &read(COMMANDS)?,
            &read(OUTSIDE_COMMANDS)?,
        ]),
        results: agree::results(&inputs, &root),
    };
    println!("agree: {build}, {} commands", file.results.len());
    fs::create_dir_all(RESULTS).map_err(|error| error.to_string())?;
    save(&format!("{RESULTS}/agree-{build}-{started}.json"), &file)
}
