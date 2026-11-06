//! Evaluates local models against termite and shows how they compare over
//! time. Run it with `scripts/analyse.sh`, or:
//!
//! ```text
//! cargo run --example eval -- run --commit abc123 [--only NAME] [--limit N] [--skip N] [--until TIME]
//! cargo run --example eval -- policy --commit abc123
//! cargo run --example eval -- report
//! cargo run --example eval -- families  # after adding a model
//! cargo run --example eval -- publish   # the site's data, reviewed for leaks
//! cargo run --example eval -- private   # the owner's report, kept local
//! ```
//!
//! It never runs a command a model wrote, and it refuses to test a model
//! outside a sandbox: see `gate.rs`. Only spec levels 1 to 7 run here; levels
//! 8 to 10 belong in a container.

mod agree;
mod bulk;
mod decision;
mod edges;
mod families;
mod gate;
mod history;
mod hosted;
mod label;
mod observe;
mod policy_steps;
mod private;
mod private_page;
mod public;
mod publish;
mod report;
mod roster;
mod run;
mod score;
mod steps;
mod team;
mod totals;
mod trials;
mod types;
mod unload;

use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use termite::Config;

use crate::types::{Case, CommandCase, ModelSpec, RunFile, Settings};

pub(crate) const RESULTS: &str = "eval/results";
pub(crate) const REQUESTS: &str = "eval/corpus/requests.jsonl";
pub(crate) const COMMANDS: &str = "eval/corpus/commands.jsonl";
pub(crate) const SSH_REQUESTS: &str = "eval/corpus/ssh.jsonl";
pub(crate) const MODELS: &str = "eval/models.jsonl";
const VIEWER: &str = "eval/viewer.html";
const PAGE: &str = "eval/report.html";

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    commit: String,
    only: Option<String>,
    limit: Option<usize>,
    /// Leave out this many requests from the start, to carry on a run.
    skip: usize,
    /// Start no new request after this time, in seconds since 1970.
    until: Option<u64>,
    /// Requests from this file instead of the corpus; saved apart.
    corpus: Option<String>,
}

fn main() -> ExitCode {
    // Run by hand from this repository; the words are only compared.
    // nosemgrep: rust.lang.security.args.args
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.split_first() {
        Some((command, rest)) => dispatch(command, rest),
        None => Err("usage: eval run|policy|decision|bulk|edges|agree|report|families|gate|roster|team|publish|relabel|private [options]"
            .to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("eval: {message}");
            ExitCode::FAILURE
        },
    }
}

fn dispatch(command: &str, rest: &[String]) -> Result<(), String> {
    // Nothing talks to a model outside a sandbox.
    if matches!(command, "run" | "trials" | "decision" | "bulk" | "edges") {
        gate::require(gate::Need::Sandbox)?;
    }
    match command {
        "run" => evaluate(&read_options(rest)?, true),
        "policy" => evaluate(&read_options(rest)?, false),
        "report" => write_report(),
        "trials" => trials::write_trials(&read_trial_options(rest)?),
        "decision" => steps::write_decision(&read_options(rest)?.commit),
        "bulk" => policy_steps::write_bulk(&read_options(rest)?.commit),
        "edges" => policy_steps::write_edges(&read_options(rest)?.commit),
        "agree" => policy_steps::agree(rest),
        "families" => families::refresh(),
        "gate" => gate::run(rest),
        "roster" => steps::write_roster(),
        "team" => team::run(rest),
        "publish" => steps::write_public(),
        "relabel" => steps::relabel(),
        "private" => steps::write_private_report(),
        other => Err(format!("unknown command {other}")),
    }
}

fn read_trial_options(words: &[String]) -> Result<trials::Plan, String> {
    let mut plan = trials::Plan::default();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let value = words.next().ok_or(format!("{word} needs a value"))?;
        let number = |text: &str| {
            text.parse::<u32>()
                .map_err(|_| format!("{word}: not a number"))
        };
        match word.as_str() {
            "--commit" => plan.commit.clone_from(value),
            "--only" => plan.only = Some(value.clone()),
            "--repeats" => plan.repeats = number(value)?,
            "--flood-repeats" => plan.flood_repeats = number(value)?,
            "--per-level" => plan.per_level = number(value)? as usize,
            "--skip" => plan.skip = number(value)? as usize,
            "--until" => {
                plan.until = Some(
                    value
                        .parse()
                        .map_err(|_| "--until needs a time".to_owned())?,
                );
            },
            "--fills" => {
                plan.fills =
                    value.split(',').map(number).collect::<Result<_, _>>()?;
            },
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(plan)
}

fn read_options(words: &[String]) -> Result<Options, String> {
    let mut options = Options {
        commit: "unknown".to_owned(),
        ..Options::default()
    };
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let mut value = || {
            words
                .next()
                .cloned()
                .ok_or_else(|| format!("{word} needs a value"))
        };
        match word.as_str() {
            "--commit" => options.commit = value()?,
            "--only" => options.only = Some(value()?),
            "--corpus" => options.corpus = Some(value()?),
            "--limit" => {
                let limit = value()?;
                let limit =
                    limit.parse().map_err(|_| "--limit needs a number")?;
                options.limit = Some(limit);
            },
            "--skip" => {
                let skip = value()?;
                options.skip =
                    skip.parse().map_err(|_| "--skip needs a number")?;
            },
            "--until" => {
                let until = value()?;
                let until =
                    until.parse().map_err(|_| "--until needs a time")?;
                options.until = Some(until);
            },
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(options)
}

pub(crate) fn read_lines<T: DeserializeOwned>(
    path: &str,
) -> Result<Vec<T>, String> {
    parse_lines(&fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?)
        .map_err(|error| format!("{path}: {error}"))
}

fn parse_lines<T: DeserializeOwned>(text: &str) -> Result<Vec<T>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map_err(|error| format!("line {}: {error}", index + 1))
        })
        .collect()
}

/// Run the policy suite, then each model, saving after each one so a long
/// run can be looked at, or stopped, part way through.
fn evaluate(options: &Options, with_models: bool) -> Result<(), String> {
    let commands: Vec<CommandCase> = read_lines(COMMANDS)?;
    let source = options.corpus.as_deref().unwrap_or(REQUESTS);
    let mut cases: Vec<Case> = read_lines(source)?;
    cases.drain(..options.skip.min(cases.len()));
    if let Some(limit) = options.limit {
        cases.truncate(limit);
    }

    let started = now()?;
    let mut file = RunFile {
        ran_on: String::new(),
        started,
        commit: options.commit.clone(),
        corpus: steps::corpus_fingerprint(source)?,
        in_progress: true,
        settings: Settings::current(),
        policy: run::policy_suite(&commands),
        models: Vec::new(),
    };
    println!(
        "policy: {} refused, {} allowed, {} escaped, {} over-refused",
        file.policy.refused_ok,
        file.policy.allowed_ok,
        file.policy.escaped.len(),
        file.policy.over_refused.len()
    );
    fs::create_dir_all(RESULTS).map_err(|error| error.to_string())?;
    // The SSH set stays apart, so that it never replaces the outside results.
    let kind = match options.corpus.as_deref() {
        Some(SSH_REQUESTS) => "ssh",
        Some(_) => "outside",
        None => "run",
    };
    let path = format!("{RESULTS}/{kind}-{started}-{}.json", options.commit);
    save(&path, &file)?;

    if with_models {
        let decision = Config::load().map_err(|error| error.to_string())?;
        let models: Vec<ModelSpec> = read_lines(MODELS)?;
        for spec in models.iter().filter(|spec| wanted(spec, options)) {
            println!("{}", spec.name);
            let run = run::run_model(spec, &decision, &cases, options.until);
            unload::release(spec);
            file.models.push(run?);
            save(&path, &file)?;
        }
        unload::release_model(
            decision.decision_url(),
            decision.decision_model(),
        );
    }
    file.in_progress = false;
    save(&path, &file)?;
    println!("saved {path}");
    Ok(())
}

pub(crate) fn now() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs())
}

fn wanted(spec: &ModelSpec, options: &Options) -> bool {
    options
        .only
        .as_ref()
        .is_none_or(|only| spec.name.contains(only))
}

pub(crate) fn save(path: &str, file: &RunFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}

/// Files in the results folder starting with `prefix`, parsed.
pub(crate) fn read_results<T: DeserializeOwned>(prefix: &str) -> Vec<T> {
    let Ok(entries) = fs::read_dir(RESULTS) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| has_prefix(path, prefix))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect()
}

fn has_prefix(path: &Path, prefix: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(prefix))
}

fn write_report() -> Result<(), String> {
    let mut runs: Vec<RunFile> = read_results("run-");
    runs.sort_by_key(|run| run.started);
    let mut quality: Vec<serde_json::Value> = read_results("quality-");
    quality.sort_by_key(|entry| entry["started"].as_u64());
    let cases: Vec<Case> = read_lines(REQUESTS)?;

    let report = report::build(&runs, &cases, quality)
        .ok_or("no results yet: run `eval run` first")?;
    let template = fs::read_to_string(VIEWER)
        .map_err(|error| format!("{VIEWER}: {error}"))?;
    fs::write(PAGE, report::render(&template, &report)?)
        .map_err(|error| error.to_string())?;
    println!("wrote {PAGE}");
    Ok(())
}

pub(crate) fn sorted_runs() -> Vec<RunFile> {
    let mut runs: Vec<RunFile> = read_results("run-");
    runs.retain(|run| !run.in_progress);
    runs.sort_by_key(|run| run.started);
    runs
}

#[cfg(test)]
mod tests {
    use super::{
        Options, parse_lines, read_lines, read_options, read_trial_options,
    };
    use crate::types::{Case, CommandCase, ModelSpec};

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn given_a_time_limit_then_trials_can_carry_on_in_a_later_window() {
        let plan = read_trial_options(&words(
            "--commit abc --repeats 0 --fills 100,110 --skip 28 --until 9",
        ));
        let plan = plan.expect("the options are valid");

        assert_eq!(plan.repeats, 0);
        assert_eq!(plan.fills, vec![100, 110]);
        assert_eq!(plan.skip, 28);
        assert_eq!(plan.until, Some(9));
        assert!(read_trial_options(&words("--until x")).is_err());
    }

    #[test]
    fn given_options_then_they_are_read() {
        let options = read_options(&words(
            "--commit abc --only qwen --limit 3 --skip 2 --until 9",
        ));
        assert_eq!(
            options,
            Ok(Options {
                commit: "abc".to_owned(),
                only: Some("qwen".to_owned()),
                limit: Some(3),
                skip: 2,
                until: Some(9),
                corpus: None,
            })
        );
        assert!(read_options(&words("--commit")).is_err());
        assert!(read_options(&words("--limit x")).is_err());
        assert!(read_options(&words("--skip x")).is_err());
        assert!(read_options(&words("--until x")).is_err());
        assert!(read_options(&words("--what")).is_err());
    }

    #[test]
    fn given_a_bad_line_then_its_number_is_named() {
        let result = parse_lines::<ModelSpec>("\nnot json\n");
        assert!(result.is_err_and(|error| error.starts_with("line 2")));
    }

    #[test]
    fn given_the_corpus_then_every_file_parses_and_ids_are_unique() {
        // The private list, when this checkout has it.
        if !std::path::Path::new(super::COMMANDS).exists() {
            return;
        }
        let cases: Vec<Case> = read_lines(super::REQUESTS).expect("requests");
        let commands: Vec<CommandCase> =
            read_lines(super::COMMANDS).expect("commands");
        let models: Vec<ModelSpec> = read_lines(super::MODELS).expect("models");

        let known: Vec<crate::private::KnownIssue> =
            read_lines(crate::steps::KNOWN_ISSUES).expect("known issues");
        assert!(!known.is_empty());
        assert!(cases.iter().all(|c| !c.domain.is_empty()));
        assert!(cases.iter().all(|c| !c.technique.is_empty()));

        let mut ids: Vec<&str> = cases.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), cases.len());
        assert!(!commands.is_empty() && !models.is_empty());
        assert!(cases.iter().all(|case| case.level <= 7));
    }

    /// The scripted commands are the deterministic layer's regression test:
    /// any escape fails the build, not just a chart.
    #[test]
    fn given_the_scripted_commands_then_the_policy_holds() {
        // The private list, when this checkout has it.
        if !std::path::Path::new(super::COMMANDS).exists() {
            return;
        }
        let commands: Vec<CommandCase> =
            read_lines(super::COMMANDS).expect("commands");
        let suite = crate::run::policy_suite(&commands);
        assert_eq!(suite.escaped.len(), 0, "{:?}", suite.escaped);
        assert_eq!(suite.over_refused.len(), 0, "{:?}", suite.over_refused);
    }
}
