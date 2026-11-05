//! `tmite scripts`: list, show, install and remove. Contract: a script is only
//! made executable after you have seen it and typed `y`, and every check is
//! repeated first, because a draft may have been edited.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use termite::{
    Event, Home, MAX_SCRIPT_BYTES, ScriptName, critical, script_problems,
    summary_of, to_printable,
};

use super::{USAGE, flag_lines, ready_home, user_home, warnings_of};
use crate::setup::prompt::Prompt;
use crate::{library, provenance, script_tools};

const EXECUTABLE: u32 = 0o700;

/// `tmite scripts`.
pub(crate) fn manage(arguments: &[String]) -> ExitCode {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let result = match words.as_slice() {
        ["list"] => ready_home().map(|home| print!("{}", list(&home))),
        ["show", name] => ready_home()
            .and_then(|home| show(&home, name).map(|text| print!("{text}"))),
        ["install", name] => ready_home().and_then(|home| {
            let prompt = Prompt::new();
            install(
                &home,
                name,
                &prompt,
                &script_tools::problems,
                &script_tools::page,
            )
            .map(|text| print!("{text}"))
        }),
        ["remove", name] => ready_home()
            .and_then(|home| remove(&home, name).map(|text| print!("{text}"))),
        _ => {
            eprint!("{USAGE}");
            return ExitCode::from(64);
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tmite: {message}");
            ExitCode::FAILURE
        },
    }
}

/// Drafts and scripts, with their summaries.
fn list(home: &Home) -> String {
    let section =
        |heading: &str,
         folder: PathBuf,
         note: &dyn Fn(&str, &str) -> &'static str| {
            let mut names: Vec<String> = fs::read_dir(folder.as_path())
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| ScriptName::parse(name).is_ok())
                .collect();
            names.sort();
            let lines: String = names
                .iter()
                .map(|name| {
                    let text =
                        read_script(&folder.join(name)).unwrap_or_default();
                    let summary = summary_of(&text).unwrap_or("no summary");
                    let note = note(name, &text);
                    format!("  {name}  {}{note}\n", to_printable(summary))
                })
                .collect();
            if lines.is_empty() {
                String::new()
            } else {
                format!("{heading}\n{lines}")
            }
        };
    let installed = |name: &str, text: &str| {
        provenance::file_note(home, &format!("bin/{name}"), text.as_bytes())
    };
    let text = section("Installed:", home.bin(), &installed)
        + &section("Drafts:", home.drafts(), &|_, _| "");
    if text.is_empty() {
        "No scripts yet. Write one with: tmite script 'request'\n".to_owned()
    } else {
        text
    }
}

/// Read a script, refusing links and anything too big to be one.
fn read_script(path: &Path) -> Result<String, String> {
    let shown = path.display();
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| format!("{shown} doesn't exist"))?;
    if !metadata.is_file() {
        return Err(format!("{shown} isn't a plain file"));
    }
    if metadata.len() > MAX_SCRIPT_BYTES as u64 {
        return Err(format!("{shown} is over 64 KB"));
    }
    fs::read_to_string(path).map_err(|error| format!("{shown}: {error}"))
}

fn parse_name(name: &str) -> Result<ScriptName, String> {
    ScriptName::parse(name).map_err(|error| error.to_string())
}

/// Every line made safe to print, keeping the layout.
fn printable(script: &str) -> String {
    script
        .lines()
        .map(|line| to_printable(&line.replace('\t', "  ")) + "\n")
        .collect()
}

/// The draft if there is one, otherwise the installed script.
fn show(home: &Home, name: &str) -> Result<String, String> {
    let name = parse_name(name)?;
    let draft = home.drafts().join(name.as_str());
    let path = if draft.exists() {
        draft
    } else {
        home.bin().join(name.as_str())
    };
    let script = read_script(&path)
        .map_err(|_| format!("there's no script called {name}"))?;
    Ok(format!("{}{}", printable(&script), flag_lines(&script)))
}

fn install(
    home: &Home,
    name: &str,
    prompt: &Prompt,
    outside_checks: &dyn Fn(&str) -> Vec<String>,
    review: &dyn Fn(&str),
) -> Result<String, String> {
    let name = parse_name(name)?;
    name.check_free(&home.bin())
        .map_err(|error| error.to_string())?;
    let draft = home.drafts().join(name.as_str());
    let script = read_script(&draft)
        .map_err(|_| format!("there's no draft called {name}"))?;

    // The draft may have been edited since it was written.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let hazards = critical(&script, &cwd, user_home().as_deref());
    if !hazards.is_empty() {
        return Err(format!("not installed: {}", hazards.join(", ")));
    }
    let mut problems = script_problems(&script);
    if problems.is_empty() {
        problems = outside_checks(&script);
    }
    if !problems.is_empty() {
        return Err(format!("not installed: {}", problems.join("; ")));
    }

    provenance::ready(home)?;
    if !prompt.is_interactive() {
        return Err(
            "install needs a terminal, so you can read the script and answer"
                .to_owned(),
        );
    }
    review(&printable(&script));
    print!("{}", flag_lines(&script));
    // Nothing to do if stdout has gone: the answer is read either way.
    let _ = io::stdout().flush();
    if !prompt.confirm(&format!("Install as {name}?"), false) {
        return Ok("Nothing was installed.\n".to_owned());
    }

    let installed = home.bin().join(name.as_str());
    if fs::symlink_metadata(&installed).is_ok() {
        if !prompt.confirm(
            &format!("{name} is already installed. Replace it?"),
            false,
        ) {
            return Ok("Nothing was installed.\n".to_owned());
        }
        let old = read_script(&installed)?;
        let keep = format!("{name}.previous");
        termite::write_private(
            &home.drafts(),
            &keep,
            old.as_bytes(),
            termite::PRIVATE_FILE_MODE,
        )
        .map_err(|error| error.to_string())?;
    }

    library::install(home)?;
    termite::write_private(
        &home.bin(),
        name.as_str(),
        script.as_bytes(),
        EXECUTABLE,
    )
    .map_err(|error| error.to_string())?;
    provenance::record(
        home,
        Event::Installed,
        &format!("bin/{name}"),
        script.as_bytes(),
        warnings_of(&script),
        "tmite scripts install",
    )?;
    fs::remove_file(&draft).map_err(|error| error.to_string())?;

    let mut text =
        format!("Installed {name}. Nothing was run.\nTry: {name} --help\n");
    if !on_path(&home.bin()) {
        text.push_str(&format!(
            "{} isn't on your PATH. Run tmite setup to add it.\n",
            home.bin().display()
        ));
    }
    Ok(text)
}

fn on_path(folder: &Path) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|entry| entry == folder)
    })
}

/// Move a script out of the way. Nothing is deleted.
fn remove(home: &Home, name: &str) -> Result<String, String> {
    let name = parse_name(name)?;
    let kept = home.drafts().join(format!("{name}.removed"));
    for folder in [home.bin(), home.drafts()] {
        let path = folder.join(name.as_str());
        if fs::symlink_metadata(&path).is_ok() {
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            fs::rename(&path, &kept).map_err(|error| error.to_string())?;
            if folder == home.bin() {
                provenance::record(
                    home,
                    Event::Removed,
                    &format!("bin/{name}"),
                    &bytes,
                    Vec::new(),
                    "tmite scripts remove",
                )?;
            }
            return Ok(format!("Removed {name}. Kept at {}\n", kept.display()));
        }
    }
    Err(format!("there's no script called {name}"))
}

#[cfg(test)]
mod tests;
