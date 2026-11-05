//! The blue and red team commands. They ask a model, rate what comes back
//! and save it. They run nothing a model writes, and `tests/architecture.rs`
//! keeps process tools out of this folder.
//!
//! ```text
//! eval team roster             which models each team uses
//! eval team check 'COMMAND'    rate a command, no model involved
//! eval team blue 'REQUEST'     ask the best model, needs the sandbox
//! eval team red 'REQUEST'      ask the worst model, needs a container or VM
//! ```

use std::fs;
use std::path::Path;

use serde::Serialize;
use termite::{
    Candidate, CommandSource, Config, Context, LlamaServer, NoProgress,
    PRIVATE_FILE_MODE, Rating, Session, Severity, hazards, rate, read_only,
    warnings, write_private,
};

use crate::gate::{self, Need};
use crate::read_lines;
use crate::roster::{Entry, ROSTER};

/// Where a team's answers go. Git ignores it, and it's never published.
const SAVED: &str = "eval/results/team";
/// A folder that does not exist, so no real files colour the answer.
const CWD: &str = "/tmp/termite-eval/project";
const HOME: &str = "/tmp/termite-eval/home";

#[derive(Debug, Serialize)]
struct Saved<'a> {
    team: &'a str,
    model: &'a str,
    request: &'a str,
    commands: Vec<Rated>,
}

#[derive(Debug, Serialize)]
struct Rated {
    command: String,
    description: String,
    rating: &'static str,
    warnings: Vec<&'static str>,
    only_reads: bool,
}

pub fn run(words: &[String]) -> Result<(), String> {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["roster"] => roster(),
        ["check", command] => {
            check(command);
            Ok(())
        },
        ["blue", request] => ask("blue", Need::Sandbox, request),
        ["red", request] => ask("red", Need::Isolated, request),
        _ => Err("usage: eval team roster|check|blue|red ['TEXT']".to_owned()),
    }
}

fn roster() -> Result<(), String> {
    let entries: Vec<Entry> = read_lines(ROSTER)?;
    for entry in entries {
        println!(
            "{} {}: {} ({}% of hostile requests obeyed, {}% of everyday requests answered)",
            entry.team,
            entry.rank,
            entry.name,
            entry.hostile_obeyed,
            entry.everyday_answered
        );
    }
    Ok(())
}

fn check(command: &str) {
    let found = warnings(command);
    let reads = read_only(command).is_ok();
    let verdict = if !found.is_empty() {
        "✗ Risky"
    } else if reads {
        "✓ Only reads"
    } else {
        "⚠ Changes something"
    };
    println!("{verdict}");
    for warning in found {
        println!("  {warning}");
    }
}

fn ask(team: &str, need: Need, request: &str) -> Result<(), String> {
    gate::require(need)?;
    let entries: Vec<Entry> = read_lines(ROSTER)?;
    let entry = entries
        .iter()
        .find(|entry| entry.team == team)
        .ok_or("the roster has no model for this team: run `eval publish`")?;

    let candidates = generate(entry, request)?;
    let commands: Vec<Rated> = candidates.iter().map(rated).collect();
    println!("Asked {}", entry.name);
    for command in &commands {
        show(team, command);
    }
    let path = save(&Saved {
        team,
        model: &entry.name,
        request,
        commands,
    })?;
    println!("Saved to {path}. Nothing was run.");
    Ok(())
}

fn generate(entry: &Entry, request: &str) -> Result<Vec<Candidate>, String> {
    // Only generation happens here, so the decision model isn't used.
    let text = serde_json::json!({
        "model_url": entry.url,
        "model": entry.model,
        "decision_url": entry.url,
        "decision_model": entry.model,
        "generation_timeout_secs": 120,
        "decision_timeout_secs": 60,
    })
    .to_string();
    let config = Config::from_json(&text).map_err(|e| e.to_string())?;
    let source = LlamaServer::new(&config).map_err(|e| e.to_string())?;
    let context = Context::new("macos", "zsh", CWD, Session::Local);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(source.generate(request, &context, &NoProgress))
        .map_err(|error| error.to_string())
}

fn rated(candidate: &Candidate) -> Rated {
    let rating = rate(candidate, Path::new(CWD), Some(Path::new(HOME)));
    Rated {
        command: candidate.command().to_owned(),
        description: candidate.description().to_owned(),
        rating: match rating {
            Rating::ReadsOnly => "reads",
            Rating::Changes => "changes",
            Rating::Risky => "risky",
        },
        warnings: warnings(candidate.command()),
        only_reads: read_only(candidate.command()).is_ok(),
    }
}

/// A red team command that really does harm stays in the saved file and off
/// the screen, so it doesn't end up in a transcript or a scrollback.
fn withheld(team: &str, command: &str) -> bool {
    team == "red"
        && hazards(command)
            .iter()
            .any(|hazard| hazard.severity >= Severity::High)
}

fn show(team: &str, rated: &Rated) {
    println!("\n{}: {}", rated.rating, rated.description);
    if withheld(team, &rated.command) {
        println!("  (command withheld: it's in the saved file)");
    } else {
        println!("  {}", rated.command);
    }
    for warning in &rated.warnings {
        println!("  {warning}");
    }
}

fn save(saved: &Saved<'_>) -> Result<String, String> {
    fs::create_dir_all(SAVED).map_err(|error| error.to_string())?;
    let name = format!("{}-{}.json", saved.team, crate::now()?);
    let text = serde_json::to_string_pretty(saved)
        .map_err(|error| error.to_string())?;
    write_private(Path::new(SAVED), &name, text.as_bytes(), PRIVATE_FILE_MODE)
        .map_err(|error| format!("{SAVED}/{name}: {error}"))?;
    let path = format!("{SAVED}/{name}");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_harmful_command_then_red_withholds_it_and_blue_does_not() {
        assert!(withheld("red", "rm -rf ~"));
        assert!(!withheld("blue", "rm -rf ~"));
    }

    #[test]
    fn given_a_harmless_command_then_red_shows_it() {
        assert!(!withheld("red", "ls"));
    }

    #[test]
    fn given_the_wrong_words_then_it_says_how_to_use_it() {
        let error = run(&["fly".to_owned()]).expect_err("usage");

        assert!(error.contains("usage"), "{error}");
    }
}
