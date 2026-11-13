//! Puts every command at one of six levels, in plain code. No model and no
//! setting can lower a level: a model's label can only raise it.
//!
//! Each stage of a pipe or chain counts, and the worst one wins. Doubt raises
//! the level. The read-only policy is the test for level 0, and the hazard
//! signatures set a floor, so a shape this code misses is still caught.

mod deletes;
mod files;
mod git;
mod leaves;
mod nested;
mod overwrites;
mod parse;
mod paths;
mod programs;
mod reads;
mod real;
mod system;
mod verdict;
mod walk;

use std::path::Path;

use crate::candidate::{Candidate, Risk};

use paths::Place;
pub use real::{NotReal, not_real};
use verdict::{NO_REPOSITORY, cleans_outside_a_repository};
use walk::Walk;

/// How far a command may go. Larger is stricter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Only looks, and touches nothing sensitive.
    Read,
    /// Only reads, but what it reads may hold secrets.
    SensitiveRead,
    /// Changes something local, and it can be undone.
    Change,
    /// Loses data, but only inside the project.
    ContainedDestructive,
    /// Needs root, or reaches outside the project.
    SystemDangerous,
    /// Hostile, however it is asked for.
    Never,
}

impl Level {
    /// The level's number, 0 to 5.
    pub fn number(self) -> u8 {
        self as u8
    }

    /// A person must say yes before it reaches the prompt.
    pub fn asks(self) -> bool {
        matches!(self, Self::SensitiveRead | Self::ContainedDestructive)
    }

    /// It may reach the prompt at all, after a yes if [`Level::asks`].
    pub fn may_insert(self) -> bool {
        self < Self::SystemDangerous
    }

    /// It may go to an agent or CI, where nobody can answer a question.
    pub fn allowed_unattended(self) -> bool {
        matches!(self, Self::Read | Self::Change)
    }

    /// The stricter of two levels. Level 1 asks and level 2 doesn't, so a
    /// sensitive read that also changes something goes to level 3, or the
    /// question would be lost.
    pub fn raised_by(self, other: Self) -> Self {
        let worst = self.max(other);
        if worst == Self::Change && self.min(other) == Self::SensitiveRead {
            Self::ContainedDestructive
        } else {
            worst
        }
    }

    /// One level up for doubt, at least to 2. Doubt alone never makes a
    /// command hostile, and level 1 skips to 3 so that it still asks.
    fn doubted(self) -> Self {
        match self {
            Self::Read => Self::Change,
            Self::SensitiveRead | Self::Change => Self::ContainedDestructive,
            Self::ContainedDestructive => Self::SystemDangerous,
            other => other,
        }
    }
}

/// A command's level, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    /// The level the command is at.
    pub level: Level,
    /// The level of what the command clearly does. `level` is one higher
    /// when the command holds text that is only known when it runs.
    pub sure: Level,
    /// Why, in plain words.
    pub reasons: Vec<&'static str>,
    /// It can carry data to another machine, whatever its level, or it runs
    /// a script that does or that can't be read.
    pub leaves: bool,
    /// It may show a secret or private data, whatever else it does.
    pub private: bool,
    /// It runs code given in the command in another language, or changes
    /// which code runs.
    pub unchecked: bool,
}

impl Classification {
    /// It may go to an agent or CI. Nobody reads these commands first, so
    /// an agent is stopped by anything serious: a command that reaches
    /// outside the project or needs root, one that may show a secret, one
    /// that can carry data off this machine, and code that can't be read.
    /// Work that stays inside the project is left alone, as an agent's own
    /// file tools are. Unknown text alone doesn't stop an agent: a path, a
    /// program or code that can't be read is counted where it is used.
    pub fn fit_for_agent(&self) -> bool {
        self.sure < Level::SystemDangerous
            && !self.private
            && !self.leaves
            && !self.unchecked
    }
}

/// Put `command` at a level, as it would run in `cwd`.
pub fn classify(
    command: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Classification {
    let place = Place::find(cwd, home);
    let mut walk = Walk::new(&place, cwd);
    walk.line(command, 0);
    let mut found = walk.finish(command);
    if cleans_outside_a_repository(command, cwd) {
        found.reasons.push(NO_REPOSITORY);
    }
    found
}

/// The texts to search for hazards in `command`: the command without the
/// text of its here-documents, then each line of such text that something
/// runs. Text that nothing can run is left out.
pub fn hazard_texts(
    command: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Vec<String> {
    let place = Place::find(cwd, home);
    let mut walk = Walk::new(&place, cwd);
    walk.line(command, 0);
    walk.texts(command)
}

/// [`classify`], raised by the level the model gave the command.
pub fn classify_candidate(
    candidate: &Candidate,
    cwd: &Path,
    home: Option<&Path>,
) -> Classification {
    let mut found = classify(candidate.command(), cwd, home);
    let labelled = match candidate.risk() {
        Risk::Read => Level::Read,
        Risk::Write => Level::Change,
        Risk::Destructive => Level::ContainedDestructive,
    };
    found.level = found.level.raised_by(labelled);
    found.sure = found.sure.raised_by(labelled);
    found
}

#[cfg(test)]
mod tests;
