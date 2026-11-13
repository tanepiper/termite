//! The last steps: what the whole line shows that no one stage does.

use std::env;
use std::path::Path;

use super::leaves::LEAVES;
use super::parse::parse;
use super::walk::{ROOT, SECRETS, Walk};
use super::{Classification, Level};
use crate::hazard::hazards;

pub(super) const NO_REPOSITORY: &str =
    "there is no git repository here, so this would do nothing in this folder";

/// A plain `git clean` in a folder that no repository covers. This only
/// changes the explanation, never the level. Anything that could point git
/// at another folder (`-C`, `GIT_DIR=`, `cd`, a second stage) doesn't count.
pub(super) fn cleans_outside_a_repository(command: &str, cwd: &Path) -> bool {
    let parsed = parse(command);
    let [stage] = parsed.stages.as_slice() else {
        return false;
    };
    let mut words = stage.text.split_whitespace();
    let plain = words.next() == Some("git") && words.next() == Some("clean");
    plain
        && parsed.inner.is_empty()
        && env::var_os("GIT_DIR").is_none()
        && !cwd.ancestors().any(|folder| folder.join(".git").exists())
}

impl Walk<'_> {
    /// The texts the hazard signatures read: each line of the command with
    /// the text of its here-documents taken out, then each line of that
    /// text that something runs. Text that nothing can run is left out, so words in a
    /// commit message or a note are never read as a command.
    pub(super) fn texts(&self, command: &str) -> Vec<String> {
        let outline = parse(command).outline;
        let code = self.fed_code.iter().flat_map(|code| code.lines());
        outline.lines().chain(code).map(str::to_owned).collect()
    }

    pub(super) fn finish(mut self, command: &str) -> Classification {
        let texts = self.texts(command);
        if self.sensitive && self.sends {
            self.raise(Level::Never, "sends secrets off the machine");
        }
        if self.fetches && self.runs_fed_code {
            self.raise(Level::Never, "runs code it fetched or decoded");
        }
        if texts.iter().any(|text| is_fork_bomb(text)) {
            self.raise(
                Level::Never,
                "starts processes until the machine stops",
            );
        }
        for text in &texts {
            self.floors(text);
        }
        let before = self.level;
        if self.doubt {
            self.level = self.level.doubted();
            self.reasons
                .push("uses text the shell changes before it runs");
        }
        let doubted = self.level;
        for text in &texts {
            self.tampering(text);
        }
        let sure = if self.level == doubted {
            before
        } else {
            self.level
        };
        self.leaves |= self.names_network && self.level > Level::Read;
        if self.leaves {
            self.reasons.push(LEAVES);
        }
        self.reasons.reverse();
        Classification {
            private: self.private || self.sensitive,
            unchecked: self.unchecked,
            level: self.level,
            sure,
            reasons: self.reasons,
            leaves: self.leaves,
        }
    }

    /// The hazard signatures set a floor, so what the parser misses still
    /// counts. `rm` of a project folder was judged above, by where it points.
    fn floors(&mut self, command: &str) {
        for hazard in hazards(command) {
            let floor = match hazard.category {
                "download_exec" => Level::Never,
                "reverse_shell" | "exfiltration" | "persistence"
                | "security_off" | "privilege" => Level::SystemDangerous,
                "system_destroy" if self.destroys_outside() => {
                    Level::SystemDangerous
                },
                "obfuscation" | "delete" => Level::ContainedDestructive,
                "secret_read" => {
                    self.sensitive = true;
                    Level::SensitiveRead
                },
                _ => continue,
            };
            self.raise(floor, floor_reason(hazard.category));
        }
        if command.contains("/dev/tcp/") || command.contains("/dev/udp/") {
            self.raise(Level::Never, "hands your shell to someone else");
        }
        if self.sensitive && self.sends {
            self.raise(Level::Never, "sends secrets off the machine");
        }
    }

    fn destroys_outside(&self) -> bool {
        self.uncontained.iter().any(|stage| {
            hazards(stage)
                .iter()
                .any(|h| h.category == "system_destroy")
        })
    }

    /// Reading termite's files is a sensitive read. Changing them is never
    /// allowed.
    fn tampering(&mut self, command: &str) {
        let touches = hazards(command)
            .iter()
            .any(|hazard| hazard.category == "disable_guard");
        if touches && self.level > Level::SensitiveRead {
            self.raise(Level::Never, "tampers with termite");
        } else if touches {
            self.raise(Level::SensitiveRead, SECRETS);
        }
    }
}

fn floor_reason(category: &str) -> &'static str {
    match category {
        "download_exec" => "runs code from the internet",
        "reverse_shell" => "hands your shell to someone else",
        "exfiltration" => "sends data off the machine",
        "persistence" => "leaves something behind that survives a restart",
        "security_off" => "switches off your protection",
        "privilege" => ROOT,
        "system_destroy" => "destroys the system",
        "obfuscation" => "hides what it really does",
        "secret_read" => SECRETS,
        _ => "deletes data",
    }
}

/// `name(){ name|name& };name`, whatever the name and the spacing.
fn is_fork_bomb(command: &str) -> bool {
    let tight: String = command.split_whitespace().collect();
    tight.split_once("(){").is_some_and(|(name, body)| {
        let name = name.rsplit([';', '&', '|']).next().unwrap_or(name);
        !name.is_empty() && body.contains(&format!("{name}|{name}"))
    })
}
