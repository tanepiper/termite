//! Repeats and context floods. Contract: the same cases, asked again and
//! again, and asked behind a long uncleared session, to see how stable a
//! model is and what fills up first. Written to `trial-` files so these
//! never mix with the core scores.

use crate::run;
use crate::steps;
use crate::types::{Case, CaseRun, ModelRun, ModelSpec, RunFile, Settings};
use crate::unload;
use crate::{MODELS, REQUESTS, RESULTS, now, read_lines, save};
use termite::Config;

/// About this many characters make one token.
const CHARS_PER_TOKEN: usize = 4;

/// An ordinary session, repeated. Harmless on purpose: the flood itself is
/// what is under test.
const SESSION: &str = "Earlier in this session the user listed the files in \
    the project, checked git status, read the README, and asked what the \
    build folder holds. ";

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub commit: String,
    pub only: Option<String>,
    /// Repeats with nothing in the context.
    pub repeats: u32,
    /// Repeats at each fill.
    pub flood_repeats: u32,
    /// Percentages of the model's limit.
    pub fills: Vec<u32>,
    /// Cases to take from each level.
    pub per_level: usize,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            commit: "unknown".to_owned(),
            only: None,
            repeats: 5,
            flood_repeats: 2,
            fills: vec![50, 100, 150],
            per_level: 2,
        }
    }
}

/// The first few cases of each level: every level, in a fixed order.
pub fn pick(cases: &[Case], per_level: usize) -> Vec<Case> {
    let mut picked: Vec<Case> = Vec::new();
    for case in cases {
        let taken = picked.iter().filter(|c| c.level == case.level).count();
        if taken < per_level {
            picked.push(case.clone());
        }
    }
    picked
}

/// `request` behind `fill` percent of `limit` tokens of old session.
pub fn flood(request: &str, limit: u32, fill: u32) -> String {
    let wanted = limit as usize * fill as usize / 100 * CHARS_PER_TOKEN;
    let mut text = SESSION.repeat(wanted / SESSION.len() + 1);
    text.truncate(wanted);
    if fill == 0 {
        return request.to_owned();
    }
    format!("{text}\nNow: {request}")
}

fn flooded(cases: &[Case], limit: u32, fill: u32) -> Vec<Case> {
    cases
        .iter()
        .map(|case| Case {
            request: flood(&case.request, limit, fill),
            ..case.clone()
        })
        .collect()
}

fn mark(cases: Vec<CaseRun>, repeat: u32, fill: u32) -> Vec<CaseRun> {
    cases
        .into_iter()
        .map(|case| CaseRun {
            repeat,
            fill,
            ..case
        })
        .collect()
}

/// Run the plan, saving after each model so a stopped run keeps its data.
pub fn write_trials(plan: &Plan) -> Result<(), String> {
    let cases = pick(&read_lines::<Case>(REQUESTS)?, plan.per_level);
    let specs: Vec<ModelSpec> = read_lines(MODELS)?;
    let decision = Config::load().map_err(|error| error.to_string())?;
    let started = now()?;
    let mut file = RunFile {
        started,
        commit: plan.commit.clone(),
        corpus: steps::corpus_fingerprint(REQUESTS)?,
        in_progress: true,
        settings: Settings::current(),
        ..RunFile::default()
    };
    let path = format!("{RESULTS}/trial-{started}-{}.json", plan.commit);

    let wanted = specs.iter().filter(|spec| {
        plan.only
            .as_ref()
            .is_none_or(|only| spec.name.contains(only))
    });
    for spec in wanted {
        println!("{}", spec.name);
        let mut model = ModelRun {
            name: spec.name.clone(),
            model: spec.model.clone(),
            ..ModelRun::default()
        };
        let mut conditions: Vec<(u32, u32)> =
            (0..plan.repeats).map(|repeat| (repeat, 0)).collect();
        for &fill in &plan.fills {
            conditions
                .extend((0..plan.flood_repeats).map(|repeat| (repeat, fill)));
        }
        for (repeat, fill) in conditions {
            let asked = flooded(&cases, spec.context_tokens, fill);
            let run = run::run_model(spec, &decision, &asked, None);
            if run.is_err() {
                unload::release(spec);
            }
            model.cases.extend(mark(run?.cases, repeat, fill));
            file.models.retain(|done| done.name != model.name);
            file.models.push(model.clone());
            save(&path, &file)?;
        }
        unload::release(spec);
    }
    unload::release_model(decision.decision_url(), decision.decision_model());
    file.in_progress = false;
    save(&path, &file)?;
    println!("saved {path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{flood, pick};
    use crate::types::{Case, Kind};

    fn case(id: &str, level: u8) -> Case {
        Case {
            id: id.to_owned(),
            level,
            kind: Kind::Everyday,
            request: String::new(),
            expect: Vec::new(),
            allow: Vec::new(),
            domain: String::new(),
            technique: String::new(),
            disclose: String::new(),
        }
    }

    #[test]
    fn given_cases_then_each_level_gives_the_same_number() {
        let cases = [case("a", 1), case("b", 1), case("c", 2)];

        let ids: Vec<String> =
            pick(&cases, 1).into_iter().map(|case| case.id).collect();

        assert_eq!(ids, ["a", "c"]);
    }

    #[test]
    fn given_a_fill_then_the_request_stays_at_the_end() {
        assert_eq!(flood("ls", 100, 0), "ls");

        let text = flood("ls", 100, 150);

        assert!(text.ends_with("Now: ls"));
        assert!(text.len() >= 600);
    }
}
