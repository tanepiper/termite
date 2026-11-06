//! Works out what installing or removing a hook changes. Contract: nothing
//! here writes. A plan is a list of files with their old and new text, shown
//! to the person before anything happens.

use std::fs;
use std::path::{Path, PathBuf};

use super::entries::{handler, hermes_block, own_files, shell_line};
use super::harness::{Harness, Install};
use super::json::Json;
use crate::setup::profile::{END, START, with_block, wrapped};

/// Where things are for one run.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub(crate) home: PathBuf,
    /// The `tmite` hooks will run.
    pub(crate) program: PathBuf,
}

/// One file to change. `new` of `None` removes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Change {
    pub(super) path: PathBuf,
    pub(super) old: Option<String>,
    pub(super) new: Option<String>,
}

/// Whether a harness has termite's hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Absent,
    /// There, and exactly what `install` would write.
    Current,
    /// There, but it differs: an older path, or edited by hand.
    Different,
}

impl State {
    pub(super) fn word(self) -> &'static str {
        match self {
            State::Absent => "not installed",
            State::Current => "installed",
            State::Different => "different from what tmite would write",
        }
    }
}

fn is_ours(harness: &Harness, command: &str) -> bool {
    command.contains("tmite")
        && command.contains(&format!(" hook {}", harness.id))
}

/// Remove termite's handlers from one event's list, and any group left empty
/// by that. Returns whether anything went.
fn prune(harness: &Harness, list: &mut Vec<Json>) -> bool {
    let before = list.len();
    let mut changed = false;
    list.retain_mut(|item| {
        if item
            .get("command")
            .and_then(Json::as_str)
            .is_some_and(|command| is_ours(harness, command))
        {
            return false;
        }
        let Some(Json::Array(inner)) = item.get_mut("hooks") else {
            return true;
        };
        let pruned = prune(harness, inner);
        changed |= pruned;
        !(pruned && inner.is_empty())
    });
    changed || list.len() != before
}

/// Every termite command in a shared settings file.
fn our_commands(harness: &Harness, item: &Json, found: &mut Vec<String>) {
    match item {
        Json::Array(items) => {
            items.iter().for_each(|i| our_commands(harness, i, found))
        },
        Json::Object(entries) => {
            for (key, value) in entries {
                match value.as_str() {
                    Some(text)
                        if key == "command" && is_ours(harness, text) =>
                    {
                        found.push(text.to_owned());
                    },
                    _ => our_commands(harness, value, found),
                }
            }
        },
        _ => {},
    }
}

/// The file's text, or `None` when it isn't there.
fn read(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn parse(
    harness: &Harness,
    path: &Path,
    text: &str,
    place: &Place,
) -> Result<Json, String> {
    let parsed = Json::parse(text)
        .ok()
        .filter(|root| matches!(root, Json::Object(_)));
    parsed.ok_or_else(|| {
        format!(
            "{} isn't plain JSON (it may have comments), so tmite won't \
             change it. Add this to it by hand:\n{}",
            path.display(),
            handler(harness, place).pretty()
        )
    })
}

/// A shared JSON file with termite's hook taken out.
fn without_hook(harness: &Harness, root: &mut Json) -> bool {
    let Some(Json::Object(events)) = root.get_mut("hooks") else {
        return false;
    };
    let mut changed = false;
    events.retain_mut(|(_, list)| {
        let Json::Array(list) = list else {
            return true;
        };
        let pruned = prune(harness, list);
        changed |= pruned;
        !(pruned && list.is_empty())
    });
    if changed && events.is_empty() {
        root.remove("hooks");
    }
    changed
}

fn with_hook(harness: &Harness, root: &mut Json, place: &Place) -> Option<()> {
    let (event, item) = match harness.install {
        Install::Grouped { event, matcher } => {
            let group = Json::Object(vec![
                ("matcher".to_owned(), Json::String(matcher.to_owned())),
                (
                    "hooks".to_owned(),
                    Json::Array(vec![handler(harness, place)]),
                ),
            ]);
            (event, group)
        },
        Install::Flat { event } => (event, handler(harness, place)),
        Install::OwnFiles | Install::YamlBlock => return None,
    };
    if harness.id == "cursor" {
        root.entry("version", Json::Number(1.into()));
    }
    let hooks = root.entry("hooks", Json::Object(Vec::new()))?;
    let Json::Array(list) = hooks.entry(event, Json::Array(Vec::new()))? else {
        return None;
    };
    list.push(item);
    Some(())
}

/// `text` with no top-level `hooks:` of anyone else's.
fn has_other_hooks(text: &str) -> bool {
    let outside = match (text.find(START), text.find(END)) {
        (Some(start), Some(end)) if start < end => {
            format!("{}{}", &text[..start], &text[end..])
        },
        _ => text.to_owned(),
    };
    outside.lines().any(|line| line.starts_with("hooks:"))
}

/// Whether `harness` has termite's hook.
pub(crate) fn state(harness: &Harness, place: &Place) -> Result<State, String> {
    let path = harness.path_in(&place.home);
    match harness.install {
        Install::OwnFiles => {
            let files = own_files(harness, place);
            let mut same = 0;
            let mut there = 0;
            for (file, wanted) in &files {
                if let Some(text) = read(file)? {
                    there += 1;
                    same += usize::from(&text == wanted);
                }
            }
            Ok(match (there, same == files.len()) {
                (0, _) => State::Absent,
                (_, true) => State::Current,
                _ => State::Different,
            })
        },
        Install::YamlBlock => {
            let text = read(&path)?.unwrap_or_default();
            let wanted = wrapped(&hermes_block(harness, place));
            Ok(if text.contains(&wanted) {
                State::Current
            } else if text.contains(START) {
                State::Different
            } else {
                State::Absent
            })
        },
        Install::Grouped { .. } | Install::Flat { .. } => {
            let Some(text) = read(&path)? else {
                return Ok(State::Absent);
            };
            let root = parse(harness, &path, &text, place)?;
            let mut found = Vec::new();
            our_commands(harness, &root, &mut found);
            let wanted = shell_line(harness, place);
            Ok(match found.as_slice() {
                [] => State::Absent,
                [only] if *only == wanted => State::Current,
                _ => State::Different,
            })
        },
    }
}

/// What installing changes. Empty when it is already in place.
pub(super) fn install(
    harness: &Harness,
    place: &Place,
) -> Result<Vec<Change>, String> {
    if state(harness, place)? == State::Current {
        return Ok(Vec::new());
    }
    let path = harness.path_in(&place.home);
    match harness.install {
        Install::OwnFiles => own_files(harness, place)
            .into_iter()
            .map(|(path, new)| {
                Ok(Change {
                    old: read(&path)?,
                    new: Some(new),
                    path,
                })
            })
            .collect(),
        Install::YamlBlock => {
            let old = read(&path)?;
            let text = old.clone().unwrap_or_default();
            let block = hermes_block(harness, place);
            if has_other_hooks(&text) {
                return Err(format!(
                    "{} already has a hooks section, so tmite won't change \
                     it. Add this to it by hand:\n{block}",
                    path.display()
                ));
            }
            Ok(vec![Change {
                new: Some(with_block(&text, &block)),
                old,
                path,
            }])
        },
        Install::Grouped { .. } | Install::Flat { .. } => {
            let old = read(&path)?;
            let mut root = match &old {
                Some(text) => parse(harness, &path, text, place)?,
                None => Json::Object(Vec::new()),
            };
            without_hook(harness, &mut root);
            with_hook(harness, &mut root, place).ok_or_else(|| {
                format!(
                    "{} has an unexpected shape, so it was left alone",
                    path.display()
                )
            })?;
            Ok(vec![Change {
                new: Some(root.pretty()),
                old,
                path,
            }])
        },
    }
}

/// What removing changes. Empty when there is nothing to remove.
pub(super) fn remove(
    harness: &Harness,
    place: &Place,
) -> Result<Vec<Change>, String> {
    let path = harness.path_in(&place.home);
    match harness.install {
        Install::OwnFiles => {
            let mut changes = Vec::new();
            for (path, _) in own_files(harness, place) {
                if let Some(old) = read(&path)? {
                    changes.push(Change {
                        path,
                        old: Some(old),
                        new: None,
                    });
                }
            }
            Ok(changes)
        },
        Install::YamlBlock => {
            let Some(text) = read(&path)? else {
                return Ok(Vec::new());
            };
            let (Some(start), Some(end)) = (text.find(START), text.find(END))
            else {
                return Ok(Vec::new());
            };
            let tail = text[end + END.len()..].trim_start_matches('\n');
            let head = text[..start].trim_end_matches('\n');
            let new = match (head.is_empty(), tail.is_empty()) {
                (true, _) => tail.to_owned(),
                (false, true) => format!("{head}\n"),
                (false, false) => format!("{head}\n{tail}"),
            };
            Ok(vec![Change {
                old: Some(text),
                new: Some(new),
                path,
            }])
        },
        Install::Grouped { .. } | Install::Flat { .. } => {
            let Some(text) = read(&path)? else {
                return Ok(Vec::new());
            };
            let mut root = parse(harness, &path, &text, place)?;
            if !without_hook(harness, &mut root) {
                return Ok(Vec::new());
            }
            Ok(vec![Change {
                old: Some(text),
                new: Some(root.pretty()),
                path,
            }])
        },
    }
}

#[cfg(test)]
mod tests;
