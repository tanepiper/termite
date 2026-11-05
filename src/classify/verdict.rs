//! The last steps: what the whole line shows that no one stage does.

use super::walk::{ROOT, SECRETS, Walk};
use super::{Classification, Level};
use crate::hazard::hazards;

impl Walk<'_> {
    pub(super) fn finish(mut self, command: &str) -> Classification {
        if self.sensitive && self.sends {
            self.raise(Level::Never, "sends secrets off the machine");
        }
        if self.fetches && self.runs_fed_code {
            self.raise(Level::Never, "runs code it fetched or decoded");
        }
        if is_fork_bomb(command) {
            self.raise(
                Level::Never,
                "starts processes until the machine stops",
            );
        }
        self.floors(command);
        if self.doubt {
            self.level = self.level.doubted();
            self.reasons
                .push("uses text the shell changes before it runs");
        }
        self.tampering(command);
        self.reasons.reverse();
        Classification {
            level: self.level,
            reasons: self.reasons,
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
