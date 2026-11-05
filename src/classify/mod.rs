//! Puts every command at one of six levels, in plain code. No model and no
//! setting can lower a level: a model's label can only raise it.
//!
//! Each stage of a pipe or chain counts, and the worst one wins. Doubt raises
//! the level. The read-only policy is the test for level 0, and the hazard
//! signatures set a floor, so a shape this code misses is still caught.

mod parse;
mod paths;
mod programs;
mod verdict;
mod walk;

use std::path::Path;

use crate::candidate::{Candidate, Risk};

use paths::Place;
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
    /// Why, in plain words.
    pub reasons: Vec<&'static str>,
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
    walk.finish(command)
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
    found
}

#[cfg(test)]
mod tests;
