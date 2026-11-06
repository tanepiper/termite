//! `tmite provenance` and `tmite alerts`, and the calls that write to the log.
//! Contract: only hashes reach the log, a changed config stops termite until
//! you accept it, and nothing here runs a file.

use std::fs;
use std::path::{Component, Path};
use std::process::ExitCode;

use termite::{
    Answer, Caller, Entry, Event, FileStatus, Home, Log, ProvenanceError,
    Record, flags, sha256_hex, to_printable,
};

use crate::provenance_key::{KeyPlace, key};
use crate::script_tools;
use crate::setup::prompt::Prompt;

const CONFIG: &str = "config.json";
const RECENT_ALERTS: usize = 20;
const USAGE: &str = "Usage: tmite provenance verify
       tmite provenance PATH
       tmite provenance accept PATH
       tmite alerts
PATH is relative to termite's folder: config.json, bin/NAME, lib/termite/FILE
";

/// Stop now if the log can't be written, before anything changes.
pub(crate) fn ready(home: &Home) -> Result<(), String> {
    let (key, _) = key(home)?;
    Log::at(home.root())
        .verify(&key)
        .map(|_| ())
        .map_err(|error| explain(&error))
}

fn explain(error: &ProvenanceError) -> String {
    format!("{error}. Run: tmite provenance verify")
}

/// Log something that happened to a file.
pub(crate) fn record(
    home: &Home,
    event: Event,
    path: &str,
    bytes: &[u8],
    flags: Vec<String>,
    by: &str,
) -> Result<(), String> {
    let (key, _) = key(home)?;
    let record = Record {
        event,
        path: path.to_owned(),
        sha256: sha256_hex(bytes),
        flags,
        by: by.to_owned(),
        verdict: None,
    };
    Log::at(home.root())
        .append(&key, record)
        .map(|_| ())
        .map_err(|error| explain(&error))
}

/// Log that a guard stopped something. A failure here must not change the
/// answer, which has already stopped the command, so it's only reported.
pub(crate) fn alert(
    request: &str,
    flags: Vec<String>,
    verdict: &str,
    by: &str,
) -> Option<String> {
    let result = Home::from_env()
        .map_err(|error| error.to_string())
        .and_then(|home| {
            let (key, _) = key(&home)?;
            Log::at(home.root())
                .append(&key, Record::alert(request, flags, verdict, by))
                .map_err(|error| explain(&error))
        });
    result.err()
}

/// Why the config can't be trusted, if it can't.
pub(crate) fn config_problem(home: &Home) -> Option<String> {
    let bytes = fs::read(home.config_file()).ok()?;
    let status = Log::at(home.root()).status(CONFIG, &bytes);
    (status.ok()? == FileStatus::Changed).then(|| {
        format!(
            "{CONFIG} has changed since termite recorded it. Read it, then run: tmite provenance accept {CONFIG}"
        )
    })
}

/// What to say about a file next to its name in a list.
pub(crate) fn file_note(home: &Home, path: &str, bytes: &[u8]) -> &'static str {
    match Log::at(home.root()).status(path, bytes) {
        Ok(FileStatus::Unchanged) => "",
        Ok(FileStatus::Changed) => "  ✗ changed since it was installed",
        Ok(FileStatus::Unknown) | Err(_) => {
            "  ⚠ termite has no record of this file"
        },
    }
}

/// `tmite provenance`.
pub(crate) fn manage(arguments: &[String]) -> ExitCode {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let home = match Home::from_env() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("tmite: {error}");
            return ExitCode::FAILURE;
        },
    };
    let result = match words.as_slice() {
        ["verify"] => verify(&home),
        ["accept", path] => {
            accept(&home, path, &Prompt::new(), &script_tools::page)
        },
        [path] if !path.starts_with('-') => history(&home, path),
        _ => {
            eprint!("{USAGE}");
            return ExitCode::from(64);
        },
    };
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        },
        Err(message) => {
            eprintln!("tmite: {message}");
            ExitCode::from(2)
        },
    }
}

/// `tmite alerts`.
/// Log an answer that stopped a command. A model's refusal counts only for an
/// agent: a person can see it, and an agent asking for this is worth knowing.
pub(crate) fn record_stop(answer: &Answer, request: &str, caller: Caller) {
    let (verdict, flags) = match answer {
        Answer::Refused { .. } => ("refused", Vec::new()),
        Answer::Held(held) => ("held", vec![held.message()]),
        Answer::Declined { .. } if caller == Caller::Agent => {
            ("declined", vec!["the model declined".to_owned()])
        },
        _ => return,
    };
    let _ = alert(request, flags, verdict, caller.as_str());
}

/// Log that `tmite check` stopped an agent. Hashes the file names only.
pub(crate) fn record_check(files: &[String]) {
    let flags = vec!["risky, or no commands found".to_owned()];
    if let Some(problem) =
        alert(&files.join(" "), flags, "check", Caller::Agent.as_str())
    {
        eprintln!("tmite: this was not logged: {problem}");
    }
}

/// Note that a command went to a person. Nothing here says what they chose.
pub(crate) fn record_ask(request: &str) {
    let flags = vec!["needs approval".to_owned()];
    if let Some(problem) = alert(request, flags, "ask", Caller::Agent.as_str())
    {
        eprintln!("tmite: this was not logged: {problem}");
    }
}

pub(crate) fn alerts() -> ExitCode {
    let found = Home::from_env()
        .map_err(|error| error.to_string())
        .and_then(|home| recent_alerts(&home));
    match found {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        },
        Err(message) => {
            eprintln!("tmite: {message}");
            ExitCode::FAILURE
        },
    }
}

fn entries(home: &Home) -> Result<Vec<Entry>, String> {
    Log::at(home.root())
        .entries()
        .map_err(|error| error.to_string())
}

fn recent_alerts(home: &Home) -> Result<String, String> {
    let all = entries(home)?;
    let alerts: Vec<&Entry> = all
        .iter()
        .filter(|entry| entry.event == Event::Alert)
        .collect();
    if alerts.is_empty() {
        return Ok("No alerts.\n".to_owned());
    }
    let from = alerts.len().saturating_sub(RECENT_ALERTS);
    Ok(alerts[from..]
        .iter()
        .map(|entry| {
            let verdict = entry.verdict.as_deref().unwrap_or("alert");
            format!(
                "{}  {}  {}  {}\n",
                entry.time,
                to_printable(&entry.by),
                to_printable(verdict),
                to_printable(&entry.flags.join(", "))
            )
        })
        .collect())
}

fn verify(home: &Home) -> Result<String, String> {
    let (key, place) = key(home)?;
    let count = Log::at(home.root())
        .verify(&key)
        .map_err(|error| error.to_string())?;
    let mut text = format!(
        "✓ The log verifies: {count} entries. This can't show whether the last entries, or the whole file, were removed.\n"
    );
    if place == KeyPlace::File {
        text.push_str("⚠ The key is in a file next to the log, so anyone who can read your files can forge entries.\n");
    }
    Ok(text)
}

fn history(home: &Home, path: &str) -> Result<String, String> {
    let lines: String = entries(home)?
        .iter()
        .filter(|entry| entry.path == path)
        .map(|entry| {
            format!(
                "{}  {:?}  {}  {}\n",
                entry.time,
                entry.event,
                &entry.sha256[..entry.sha256.len().min(12)],
                to_printable(&entry.by)
            )
        })
        .collect();
    if lines.is_empty() {
        return Err(format!("the log has nothing for {}", to_printable(path)));
    }
    Ok(lines)
}

/// Only paths inside termite's folder, with no way out of it.
fn is_inside(path: &str) -> bool {
    let path = Path::new(path);
    let plain = path
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    plain
        && (path == Path::new(CONFIG)
            || path.starts_with("bin")
            || path.starts_with("lib/termite"))
}

/// Show a changed file, and log it as yours on `y`.
fn accept(
    home: &Home,
    path: &str,
    prompt: &Prompt,
    review: &dyn Fn(&str),
) -> Result<String, String> {
    if !is_inside(path) {
        return Err(USAGE.to_owned());
    }
    if !prompt.is_interactive() {
        return Err(
            "accept needs a terminal, so you can read the file and answer"
                .to_owned(),
        );
    }
    ready(home)?;
    let full = home.root().join(path);
    let metadata = fs::symlink_metadata(&full)
        .map_err(|_| format!("{path} doesn't exist"))?;
    if !metadata.is_file() {
        return Err(format!("{path} isn't a plain file"));
    }
    let bytes = fs::read(&full).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let shown: String = text
        .lines()
        .map(|line| to_printable(&line.replace('\t', "  ")) + "\n")
        .collect();
    review(&shown);
    let cwd = home.root();
    let risky: Vec<String> = flags(&format!("\n{text}"), cwd, None)
        .iter()
        .flat_map(|flag| flag.warnings.iter().map(|w| (*w).to_owned()))
        .collect();
    if !risky.is_empty() {
        println!("⚠ {}", risky.join(", "));
    }
    if !prompt.confirm(&format!("Accept {path} as it is now?"), false) {
        return Ok(
            "Nothing changed. termite won't trust this file.\n".to_owned()
        );
    }
    record(
        home,
        Event::ChangedAccepted,
        path,
        &bytes,
        risky,
        "tmite provenance accept",
    )?;
    Ok(format!("Accepted {path}.\n"))
}

#[cfg(test)]
mod tests;
