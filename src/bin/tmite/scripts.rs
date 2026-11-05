//! `tmite script` and `tmite scripts`. Contract: termite never runs a script,
//! a script is only made executable after you have seen it and typed `y`, and
//! every check is repeated at install.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::json;
use termite::{
    Config, Draft, DraftPlace, Drafted, Event, Home, LlamaServer, Rating,
    ScriptName, SystemOne, decision_token, flags, to_printable, write_draft,
};

use crate::output::render_failure;
use crate::setup::prompt::Prompt;
use crate::{
    Format, context_from_environment, progress, provenance, script_tools,
};

pub(super) const USAGE: &str = "Usage: tmite script [--json] 'request'
       tmite scripts list
       tmite scripts show NAME
       tmite scripts install NAME
       tmite scripts remove NAME
";

/// `tmite script`: write a draft. Exit status: 0 saved, 2 not saved, 1 failed,
/// 64 usage.
pub(crate) fn write(arguments: &[String]) -> ExitCode {
    let (format, words) = match arguments.split_first() {
        Some((first, rest)) if first == "--json" => (Format::Json, rest),
        _ => (Format::Human, arguments),
    };
    if words.is_empty() || words[0].starts_with('-') {
        eprint!("{USAGE}");
        return ExitCode::from(64);
    }
    let request = words.join(" ");
    if let Ok(home) = Home::from_env()
        && let Err(message) =
            meant_as_a_request(&home, &request, format, &Prompt::new())
    {
        print!("{}", render_failure(&message, format));
        return ExitCode::from(2);
    }
    let (text, code) = answer(&request, format);
    print!("{text}");
    ExitCode::from(code)
}

/// One word that names a script is more likely a slip for `tmite scripts`
/// than a request, and a model would write something from it. So ask.
fn meant_as_a_request(
    home: &Home,
    request: &str,
    format: Format,
    prompt: &Prompt,
) -> Result<(), String> {
    let Some(found) = script_called(home, request) else {
        return Ok(());
    };
    if !matches!(format, Format::Human) || !prompt.is_interactive() {
        return Err(found);
    }
    println!("{found}");
    let question = format!("Write a new script from the words `{request}`?");
    if prompt.confirm(&question, false) {
        Ok(())
    } else {
        Err("nothing was written".to_owned())
    }
}

/// What is already called `request`, and how to use it.
fn script_called(home: &Home, request: &str) -> Option<String> {
    let name = ScriptName::parse(request).ok()?;
    let has = |folder: PathBuf| {
        fs::symlink_metadata(folder.join(name.as_str())).is_ok()
    };
    if has(home.drafts()) {
        Some(format!(
            "{name} is already a draft. To read it: tmite scripts show {name}. To install it: tmite scripts install {name}."
        ))
    } else if has(home.bin()) {
        Some(format!(
            "{name} is already installed. To run it: {name}. To read it: tmite scripts show {name}."
        ))
    } else {
        None
    }
}

/// Write a draft for `request` and say how it went.
pub(crate) fn answer(request: &str, format: Format) -> (String, u8) {
    match draft(request, format) {
        Ok(drafted) => describe(&drafted, format),
        Err(message) => (render_failure(&message, format), 1),
    }
}

fn draft(request: &str, format: Format) -> Result<Drafted, String> {
    let home = ready_home()?;
    script_tools::ready()?;
    if let Some(message) = crate::config_problem() {
        return Err(message);
    }
    let config = Config::load().map_err(|error| error.to_string())?;
    let source =
        LlamaServer::for_scripts(&config).map_err(|error| error.to_string())?;
    let decider = SystemOne::new(&config, decision_token())
        .map_err(|error| error.to_string())?;
    let context = context_from_environment();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let bin = home.bin();
    let user_home = user_home();
    let place = DraftPlace {
        bin: &bin,
        cwd: Path::new(context.cwd()),
        home: user_home.as_deref(),
    };
    let drafted = progress::watching(format, |progress| {
        runtime.block_on(write_draft(
            &source,
            &decider,
            request,
            &context,
            place,
            script_tools::problems,
            progress,
        ))
    })
    .map_err(|error| error.to_string())?;

    match &drafted {
        Drafted::Ready(ready) => save(&home, ready)?,
        Drafted::Withheld { reasons } => {
            // The script is already stopped; the alert only records that.
            let _ = provenance::alert(
                request,
                reasons.clone(),
                "withheld",
                "tmite script",
            );
        },
        Drafted::Rejected { .. } => {},
    }
    Ok(drafted)
}

pub(super) fn ready_home() -> Result<Home, String> {
    let home = Home::from_env().map_err(|error| error.to_string())?;
    home.ensure().map_err(|error| error.to_string())?;
    Ok(home)
}

pub(super) fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn save(home: &Home, ready: &Draft) -> Result<(), String> {
    termite::write_private(
        &home.drafts(),
        ready.name.as_str(),
        ready.script.as_bytes(),
        termite::PRIVATE_FILE_MODE,
    )
    .map_err(|error| error.to_string())?;
    provenance::record(
        home,
        Event::Drafted,
        &format!("drafts/{}", ready.name),
        ready.script.as_bytes(),
        warnings_of(&ready.script),
        "tmite script",
    )
}

/// The warnings on a script's risky lines, once each.
pub(super) fn warnings_of(script: &str) -> Vec<String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut found: Vec<String> = flags(script, &cwd, user_home().as_deref())
        .iter()
        .flat_map(|flag| flag.warnings.iter().map(|w| (*w).to_owned()))
        .collect();
    found.sort();
    found.dedup();
    found
}

pub(super) fn describe(drafted: &Drafted, format: Format) -> (String, u8) {
    match format {
        Format::Human => describe_human(drafted),
        Format::Json => describe_json(drafted),
        Format::Zle => describe_for_widget(drafted),
    }
}

fn describe_human(drafted: &Drafted) -> (String, u8) {
    match drafted {
        Drafted::Ready(ready) => {
            let name = &ready.name;
            let mut text = format!("Draft saved: {name}\n{}\n", ready.summary);
            text.push_str(&flag_lines(&ready.script));
            for note in &ready.notes {
                text.push_str(&format!("⚠ {note}\n"));
            }
            text.push_str(&format!(
                "Read it: tmite scripts show {name}\nInstall it: tmite scripts install {name}\nNothing was run.\n"
            ));
            (text, 0)
        },
        Drafted::Rejected { problems } => {
            let mut text =
                "✗ The script failed its checks twice. Nothing was saved.\n"
                    .to_owned();
            for problem in problems {
                text.push_str(&format!("  {}\n", to_printable(problem)));
            }
            (text, 2)
        },
        Drafted::Withheld { reasons } => (
            format!(
                "✗ Script withheld: {}. It isn't shown, and nothing was saved.\n",
                reasons.join(", ")
            ),
            2,
        ),
    }
}

/// The widgets show this like `?x`: the install command goes at your prompt,
/// and pressing Enter is up to you. That needs no script logic in any shell.
fn describe_for_widget(drafted: &Drafted) -> (String, u8) {
    let (text, code) = describe_human(drafted);
    match drafted {
        Drafted::Ready(ready) => {
            let rating = if ready.flags.is_empty() && ready.notes.is_empty() {
                Rating::Changes
            } else {
                Rating::Risky
            };
            let rating = rating.as_str();
            let install = format!("tmite scripts install {}", ready.name);
            (format!("straight_in\n{rating}\n{install}\n{text}"), code)
        },
        _ => (format!("refused\n\n\n{text}"), code),
    }
}

fn describe_json(drafted: &Drafted) -> (String, u8) {
    let (value, code) = match drafted {
        Drafted::Ready(ready) => (
            json!({
                "status": "draft",
                "name": ready.name.as_str(),
                "summary": ready.summary,
                "notes": ready.notes,
                "flags": ready.flags.iter().map(|flag| json!({
                    "line": flag.line,
                    "command": flag.command,
                    "severity": flag.severity,
                    "warnings": flag.warnings,
                })).collect::<Vec<_>>(),
            }),
            0,
        ),
        Drafted::Rejected { problems } => {
            (json!({ "status": "rejected", "problems": problems }), 2)
        },
        Drafted::Withheld { reasons } => {
            (json!({ "status": "withheld", "reasons": reasons }), 2)
        },
    };
    (format!("{value}\n"), code)
}

/// The risky lines of a script, for review.
pub(super) fn flag_lines(script: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = user_home();
    flags(script, &cwd, home.as_deref())
        .iter()
        .map(|flag| {
            format!(
                "⚠ line {}: {}\n    {}\n",
                flag.line,
                flag.warnings.join(", "),
                to_printable(&flag.command).trim()
            )
        })
        .collect()
}

mod manage;

pub(crate) use manage::manage;
