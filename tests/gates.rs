//! Checks every row of `tests/gates/cases.jsonl`, the list of commands and the
//! level each must have. The rule: the more exactly a command says what it acts
//! on, the lower its level. Nothing here runs a command.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use termite::{Level, classify};

const CASES: &str = include_str!("gates/cases.jsonl");
const HOME: &str = "/Users/me";
const PROJECT: &str = "/Users/me/code/app";

/// Things put in front of a command that change nothing about what it does.
const WRAPPERS: &[&str] = &["time ", "nice ", "command ", "env ", "FOO=1 "];

struct Case {
    command: String,
    action: String,
    picks: String,
    level: u8,
    family: String,
}

fn cases() -> Vec<Case> {
    CASES
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).expect("JSON row");
            let text =
                |name: &str| row[name].as_str().expect("text field").to_owned();
            Case {
                command: text("command"),
                action: text("action"),
                picks: text("picks"),
                level: row["level"].as_u64().expect("level") as u8,
                family: text("family"),
            }
        })
        .collect()
}

fn level_of(command: &str) -> Level {
    classify(command, Path::new(PROJECT), Some(Path::new(HOME))).level
}

fn report(wrong: &[String]) {
    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn given_every_case_then_it_has_its_level() {
    let wrong: Vec<String> = cases()
        .iter()
        .filter(|case| level_of(&case.command).number() != case.level)
        .map(|case| {
            let found = level_of(&case.command).number();
            format!("{found} not {}: {}", case.level, case.command)
        })
        .collect();
    report(&wrong);
}

#[test]
fn given_a_family_then_every_member_has_the_same_level() {
    let mut families: BTreeMap<String, Vec<(u8, String)>> = BTreeMap::new();
    for case in cases() {
        let level = level_of(&case.command).number();
        families
            .entry(case.family)
            .or_default()
            .push((level, case.command));
    }
    let wrong: Vec<String> = families
        .iter()
        .filter(|(_, members)| {
            members.iter().any(|(level, _)| *level != members[0].0)
        })
        .map(|(family, members)| format!("{family}: {members:?}"))
        .collect();
    report(&wrong);
}

#[test]
fn given_a_choice_that_isnt_an_exact_name_then_a_person_must_type_it() {
    let wrong: Vec<String> = cases()
        .iter()
        .filter(|case| case.action == "delete" && case.picks != "exact")
        .filter(|case| level_of(&case.command) < Level::SystemDangerous)
        .map(|case| case.command.clone())
        .collect();
    report(&wrong);
}

#[test]
fn given_any_delete_then_it_asks_or_goes_higher() {
    let wrong: Vec<String> = cases()
        .iter()
        .filter(|case| case.action == "delete")
        .filter(|case| level_of(&case.command) < Level::ContainedDestructive)
        .map(|case| case.command.clone())
        .collect();
    report(&wrong);
}

#[test]
fn given_root_then_every_delete_is_dangerous() {
    let mut wrong = Vec::new();
    for case in cases().iter().filter(|case| case.action == "delete") {
        for root in ["sudo ", "sudo -n ", "doas ", "pkexec "] {
            let command = format!("{root}{}", case.command);
            if level_of(&command) < Level::SystemDangerous {
                wrong.push(command);
            }
        }
    }
    report(&wrong);
}

#[test]
fn given_a_wrapper_then_the_level_never_goes_down() {
    let mut wrong = Vec::new();
    for case in cases() {
        let plain = level_of(&case.command);
        for wrapper in WRAPPERS {
            let wrapped = format!("{wrapper}{}", case.command);
            if level_of(&wrapped) < plain {
                wrong.push(wrapped);
            }
        }
    }
    report(&wrong);
}

#[test]
fn given_a_full_path_to_the_program_then_the_level_never_goes_down() {
    let wrong: Vec<String> = cases()
        .iter()
        .filter(|case| !case.command.contains('|'))
        .map(|case| (case, format!("/usr/bin/{}", case.command)))
        .filter(|(case, full)| level_of(full) < level_of(&case.command))
        .map(|(_, full)| full)
        .collect();
    report(&wrong);
}
