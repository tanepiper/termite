//! A possible command for a request, as the local model proposed it.

/// How much a command can change, as labelled by the model. The order goes
/// from least to most harmful, so the larger of two values is the safer
/// label to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// Only looks at the machine.
    Read,
    /// Changes files or state, but can be undone.
    Write,
    /// Deletes data or makes changes that are hard to undo.
    Destructive,
}

impl Risk {
    /// The label used in JSON: `read`, `write` or `destructive`.
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Read => "read",
            Risk::Write => "write",
            Risk::Destructive => "destructive",
        }
    }

    /// Read a JSON label; `None` for anything unknown.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "read" => Some(Risk::Read),
            "write" => Some(Risk::Write),
            "destructive" => Some(Risk::Destructive),
            _ => None,
        }
    }
}

/// One possible command for a request, with a short description and the
/// model's risk label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    command: String,
    description: String,
    risk: Risk,
}

impl Candidate {
    /// The shell command, exactly as it would be written to the shell.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// A short, human description of what the command does.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The model's risk label.
    pub fn risk(&self) -> Risk {
        self.risk
    }

    /// Create a candidate.
    ///
    /// ```
    /// use termite::{Candidate, Risk};
    ///
    /// let candidate = Candidate::new("ls -la", "List files", Risk::Read);
    /// assert_eq!(candidate.command(), "ls -la");
    /// ```
    pub fn new(
        command: impl Into<String>,
        description: impl Into<String>,
        risk: Risk,
    ) -> Self {
        Self {
            command: command.into(),
            description: description.into(),
            risk,
        }
    }

    /// Fold in a candidate that was found to be the same command: keep this
    /// command text, take the longer description and the more harmful risk.
    pub(crate) fn absorb_duplicate(&mut self, duplicate: Candidate) {
        if duplicate.description.chars().count()
            > self.description.chars().count()
        {
            self.description = duplicate.description;
        }

        self.risk = self.risk.max(duplicate.risk);
    }
}
