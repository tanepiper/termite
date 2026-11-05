//! How risky a command looks, in termite's own judgement. The model's label
//! can only make this worse: a model that calls `rm -rf ~` a read still gets
//! a red line.

use std::path::Path;

use crate::candidate::{Candidate, Risk};
use crate::hazard::{Severity, hazards};
use crate::policy::read_only_in;

/// What to tell the user about a command before it reaches the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    /// Passes the read-only policy and shows no hazard.
    ReadsOnly,
    /// Not known to be read-only, but no hazard found.
    Changes,
    /// A hazard was found, or the model itself says it is destructive.
    Risky,
}

impl Rating {
    /// The word for the line format and JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Rating::ReadsOnly => "reads",
            Rating::Changes => "changes",
            Rating::Risky => "risky",
        }
    }

    /// The words shown to people.
    pub fn label(self) -> &'static str {
        match self {
            Rating::ReadsOnly => "reads only",
            Rating::Changes => "changes things",
            Rating::Risky => "risky",
        }
    }
}

/// Rate `candidate` as it would run in `cwd`.
pub fn rate(candidate: &Candidate, cwd: &Path, home: Option<&Path>) -> Rating {
    let serious = hazards(candidate.command())
        .iter()
        .any(|hazard| hazard.severity >= Severity::Medium);
    if serious || candidate.risk() == Risk::Destructive {
        return Rating::Risky;
    }

    let read_only = read_only_in(candidate.command(), cwd, home).is_ok();
    if read_only && candidate.risk() == Risk::Read {
        Rating::ReadsOnly
    } else {
        Rating::Changes
    }
}

/// What `command` would do, in plain words, worst first. The model's own
/// description can't be trusted for this.
pub fn warnings(command: &str) -> Vec<&'static str> {
    let mut found = hazards(command);
    found.sort_by_key(|hazard| std::cmp::Reverse(hazard.severity));
    found
        .iter()
        .filter(|hazard| hazard.severity >= Severity::Medium)
        .map(|hazard| warning(hazard.category))
        .collect()
}

fn warning(category: &str) -> &'static str {
    match category {
        "system_destroy" => "destroys the system",
        "delete" => "deletes data",
        "reverse_shell" => "hands your shell to someone else",
        "download_exec" => "runs code from the internet",
        "exfiltration" => "sends data off the machine",
        "persistence" => "leaves something behind that survives a restart",
        "disable_guard" => "tampers with termite",
        "security_off" => "switches off your protection",
        "obfuscation" => "hides what it really does",
        "secret_read" => "reads your secrets",
        "privilege" => "needs root",
        _ => "looks dangerous",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Rating, rate, warnings};
    use crate::candidate::{Candidate, Risk};

    fn rated(command: &str, risk: Risk) -> Rating {
        let candidate = Candidate::new(command, "Do it", risk);
        rate(&candidate, Path::new("/tmp"), None)
    }

    #[test]
    fn given_commands_when_rated_then_the_worst_view_wins() {
        assert_eq!(rated("ls", Risk::Read), Rating::ReadsOnly);
        assert_eq!(rated("ls", Risk::Write), Rating::Changes);
        assert_eq!(rated("touch a", Risk::Read), Rating::Changes);
        assert_eq!(rated("ls", Risk::Destructive), Rating::Risky);
    }

    #[test]
    fn given_a_model_that_lies_when_rated_then_the_command_is_still_risky() {
        assert_eq!(rated("rm -rf ~", Risk::Read), Rating::Risky);
        assert_eq!(rated("cat ~/.ssh/id_rsa", Risk::Read), Rating::Risky);
    }

    #[test]
    fn given_a_hazard_when_warned_then_the_worst_comes_first() {
        assert_eq!(
            warnings("rm -rf ~")[0],
            "destroys the system",
            "{:?}",
            warnings("rm -rf ~")
        );
        assert!(warnings("ls").is_empty());
    }
}
