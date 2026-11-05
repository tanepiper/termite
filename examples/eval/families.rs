//! Which family each model belongs to, worked out from its name so nobody
//! has to type it. `eval/families.jsonl` keeps the list; a family's slot is
//! the colour the site gives it, and never changes once handed out. Run
//! `eval families` after adding a model. `eval publish` does it too.

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::types::ModelSpec;
use crate::{MODELS, read_lines, steps::DECISION_MODELS};

const FAMILIES: &str = "eval/families.jsonl";

/// One line of `eval/families.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Family {
    pub family: String,
    pub slot: usize,
    pub models: Vec<String>,
}

/// The letters a model's name starts with, ignoring any publisher in front
/// of a slash, so that `Qwen3.8 27B` and `qwen2.5-coder 7B` are both `qwen`
/// and `gemma-4 12B` and `gemma4` are both `gemma`.
pub fn family_of(name: &str) -> String {
    let name = name.rsplit('/').next().unwrap_or(name);
    let family: String = name
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_lowercase();
    if family.is_empty() {
        "other".to_owned()
    } else {
        family
    }
}

/// The list for these model names. Families already on the list keep their
/// slot, even with no models left; new ones follow in alphabetical order.
/// The same names and the same list always give the same result.
pub fn merge(known: &[Family], names: &[String]) -> Vec<Family> {
    let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in names {
        members
            .entry(family_of(name))
            .or_default()
            .push(name.clone());
    }
    let mut list: Vec<Family> = known
        .iter()
        .map(|old| Family {
            models: members.remove(&old.family).unwrap_or_default(),
            ..old.clone()
        })
        .collect();
    let last = list.iter().map(|family| family.slot).max().unwrap_or(0);
    for (next, (family, models)) in (last + 1..).zip(members) {
        list.push(Family {
            family,
            slot: next,
            models,
        });
    }
    for family in &mut list {
        family.models.sort();
        family.models.dedup();
    }
    list.sort_by_key(|family| family.slot);
    list
}

/// Bring the list up to date with the model files.
pub fn refresh() -> Result<(), String> {
    let known: Vec<Family> = if fs::exists(FAMILIES).unwrap_or(false) {
        read_lines(FAMILIES)?
    } else {
        Vec::new()
    };
    let mut specs: Vec<ModelSpec> = read_lines(MODELS)?;
    specs.extend(read_lines::<ModelSpec>(DECISION_MODELS)?);
    specs.extend(crate::hosted::models()?);
    specs.extend(crate::hosted::deciders()?);
    let names: Vec<String> = specs.into_iter().map(|spec| spec.name).collect();

    let list = merge(&known, &names);
    let mut text = String::new();
    for family in &list {
        let line =
            serde_json::to_string(family).map_err(|error| error.to_string())?;
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(FAMILIES, text)
        .map_err(|error| format!("{FAMILIES}: {error}"))?;

    let added = list.len() - known.len();
    println!(
        "families: {} from {} models, {added} new",
        list.len(),
        names.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn given_different_spellings_then_they_are_one_family() {
        for (name, family) in [
            ("Qwen3.8 27B", "qwen"),
            ("qwen2.5-coder 7B, refusals removed", "qwen"),
            ("gemma-4 12B coder", "gemma"),
            ("gemma4", "gemma"),
            ("llama3.1 8B", "llama"),
            ("ggml-org/Kev-4B-GGUF:Q8_0", "kev"),
            ("7B mystery", "other"),
            ("", "other"),
        ] {
            assert_eq!(family_of(name), family, "{name}");
        }
    }

    #[test]
    fn given_no_list_then_families_are_in_alphabetical_order() {
        let list = merge(&[], &names(&["qwen 7B", "gemma4", "Qwen3.8"]));

        let found: Vec<_> =
            list.iter().map(|f| (f.family.as_str(), f.slot)).collect();
        assert_eq!(found, [("gemma", 1), ("qwen", 2)]);
        assert_eq!(list[1].models, ["Qwen3.8", "qwen 7B"]);
    }

    #[test]
    fn given_a_new_family_then_the_old_ones_keep_their_slots() {
        let first = merge(&[], &names(&["qwen 7B", "zeta 1B"]));
        let second = merge(&first, &names(&["alpha 1B", "qwen 7B", "zeta 1B"]));

        let found: Vec<_> =
            second.iter().map(|f| (f.family.as_str(), f.slot)).collect();
        assert_eq!(found, [("qwen", 1), ("zeta", 2), ("alpha", 3)]);
    }

    #[test]
    fn given_a_family_with_no_models_left_then_its_slot_stays_taken() {
        let first = merge(&[], &names(&["alpha 1B", "qwen 7B"]));
        let second = merge(&first, &names(&["qwen 7B", "zeta 1B"]));

        assert_eq!(second[0].models, Vec::<String>::new());
        assert_eq!(second[2].family, "zeta");
        assert_eq!(second[2].slot, 3);
    }

    #[test]
    fn given_the_same_input_then_the_list_does_not_change() {
        let models = names(&["qwen 7B", "gemma4", "llama3.1 8B"]);
        let first = merge(&[], &models);

        assert_eq!(merge(&first, &models), first);
    }
}
