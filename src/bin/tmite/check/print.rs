//! How `tmite check` prints one result and the closing summary.

use std::io::{self, Write};

use serde_json::json;
use termite::{Finding, Level, Rating, Severity, to_printable};

use super::{Style, Tally};

/// How much of a command is shown on a line.
const SHOWN_CHARS: usize = 120;

fn level(finding: &Finding) -> &'static str {
    match finding.severity {
        Some(Severity::Critical) => "critical",
        Some(Severity::High) => "high",
        Some(_) | None => finding.rating.label(),
    }
}

pub(super) fn human_line(
    finding: &Finding,
    gate: Level,
    source: &str,
    colour: bool,
) -> String {
    let (mark, code) = match finding.rating {
        Rating::Risky => ("✗", "31"),
        Rating::Changes => ("⚠", "33"),
        Rating::ReadsOnly => ("✓", "32"),
    };
    let command: String = to_printable(&finding.command)
        .chars()
        .take(SHOWN_CHARS)
        .collect();
    let head = format!(
        "{mark} {source}:{}  level {}, {}",
        finding.line,
        gate.number(),
        level(finding)
    );
    let head = paint(&head, code, colour);
    if finding.warnings.is_empty() {
        format!("{head}\n    {}", command.trim())
    } else {
        format!(
            "{head}: {}\n    {}",
            finding.warnings.join(", "),
            command.trim()
        )
    }
}

fn paint(text: &str, code: &str, colour: bool) -> String {
    if colour {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

pub(super) fn summary(
    tally: &Tally,
    style: Style,
    out: &mut impl Write,
) -> io::Result<()> {
    if style.json {
        let line = json!({
            "summary": true,
            "commands": tally.total(),
            "reads": tally.reads,
            "changes": tally.changes,
            "risky": tally.risky,
            "worst": tally.worst,
        });
        return writeln!(out, "{line}");
    }
    let noun = if tally.total() == 1 {
        "command"
    } else {
        "commands"
    };
    let counts = format!(
        "{} {noun}: {} risky, {} change things, {} read only.",
        tally.total(),
        tally.risky,
        tally.changes,
        tally.reads
    );
    let line = if tally.total() == 0 {
        paint("⚠ No commands found.", "33", style.colour)
    } else if tally.risky > 0 {
        paint(&format!("✗ {counts}"), "31", style.colour)
    } else {
        format!(
            "{} No known hazard found. That doesn't prove it's safe, so read it.",
            paint(&format!("✓ {counts}"), "32", style.colour)
        )
    };
    writeln!(out, "{line} Nothing was run.")
}
