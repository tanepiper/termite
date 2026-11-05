//! The three ways `tmite` prints an answer: for people, as JSON for
//! scripts, and as lines for the zsh widget.
//!
//! Every piece of model-written text goes through `to_printable` first, so
//! nothing the model writes can move the cursor, change colours or add a
//! line to the line format. Colour is only ever added here, afterwards.
//! Every colour comes with a word, so nothing depends on seeing it.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use termite::{Answer, Ranked, Rating, rate, to_printable, warnings};

const RED: &str = "1;31";
const YELLOW: &str = "33";
const GREEN: &str = "32";

/// How an answer is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    /// Readable text.
    Human,
    /// One JSON object.
    Json,
    /// The line format read by `shell/tmite.zsh`.
    Zle,
}

/// Where the answer is being read: the folder a command would run in, and
/// whether to use colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct View {
    pub(crate) cwd: PathBuf,
    pub(crate) home: Option<PathBuf>,
    pub(crate) colour: bool,
}

impl View {
    fn rating(&self, ranked: &Ranked) -> Rating {
        rate(ranked.candidate(), &self.cwd, self.home.as_deref())
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.colour {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    fn tag(&self, rating: Rating) -> String {
        let (code, mark) = match rating {
            Rating::ReadsOnly => (GREEN, "✓"),
            Rating::Changes => (YELLOW, "⚠"),
            Rating::Risky => (RED, "✗"),
        };
        self.paint(code, &format!("{mark} {}", rating.label()))
    }

    /// The explanation, then the command, then termite's own warning.
    fn entry(&self, ranked: &Ranked, first: &str, rest: &str) -> String {
        let rating = self.rating(ranked);
        let description = to_printable(ranked.candidate().description());
        let shown = command(ranked);
        let mut text = format!("{first}{}  {description}\n", self.tag(rating));

        if rating == Rating::Risky {
            text.push_str(&format!("{rest}{}\n", self.paint(RED, &shown)));
            let warned = warnings(ranked.candidate().command()).join(", ");
            if !warned.is_empty() {
                text.push_str(&format!("{rest}termite says: {warned}\n"));
            }
        } else {
            text.push_str(&format!("{rest}{shown}\n"));
        }
        text
    }

    fn list(&self, ranked: &[Ranked]) -> String {
        ranked
            .iter()
            .enumerate()
            .map(|(index, ranked)| {
                self.entry(ranked, &format!("{}. ", index + 1), "   ")
            })
            .collect()
    }

    fn refused(&self, ranked: &Ranked, reason: &str) -> String {
        format!(
            "{}  {}\n  {}\n",
            self.paint(RED, "✗ refused"),
            to_printable(reason),
            self.paint(RED, &command(ranked))
        )
    }

    fn with_note(&self, ranked: &Ranked, note: &str, reason: &str) -> String {
        let reason = to_printable(reason);
        format!("{}  {note}: {reason}\n", self.entry(ranked, "", "  "))
    }
}

/// Render `answer` in `format`, ending with a new line.
pub(crate) fn render(answer: &Answer, format: Format, view: &View) -> String {
    match format {
        Format::Human => human(answer, view),
        Format::Json => format!("{}\n", json(answer, view)),
        Format::Zle => zle(answer, view),
    }
}

/// Render a failure that happened before there was an answer, such as a
/// missing config file, in the same shape as [`Answer::Failed`].
pub(crate) fn render_failure(message: &str, format: Format) -> String {
    let message = to_printable(message);

    match format {
        Format::Human => format!("tmite: {message}\n"),
        Format::Json => {
            format!("{}\n", json!({ "kind": "failed", "message": message }))
        },
        Format::Zle => format!("failed\n\n\ntmite: {message}\n"),
    }
}

/// The exit status for an answer: 0 when there is a command to use, 2 when
/// there is none or it was refused, 1 when something failed.
pub(crate) fn exit_code(answer: &Answer) -> u8 {
    match answer {
        Answer::Failed(_) => 1,
        Answer::NoUsableCommand { .. }
        | Answer::Refused { .. }
        | Answer::Declined { .. } => 2,
        _ => 0,
    }
}

const ASKING: &str = "Asking first";
const STRAIGHT: &str = "Straight to the prompt";

fn human(answer: &Answer, view: &View) -> String {
    match answer {
        Answer::Confirm(ranked) => view.entry(ranked, "", "  "),
        Answer::Choose(ranked) => view.list(ranked),
        Answer::StraightIn { ranked, reason } => {
            view.with_note(ranked, STRAIGHT, reason)
        },
        Answer::ConfirmWithReason { ranked, reason } => {
            view.with_note(ranked, ASKING, reason)
        },
        Answer::Refused { ranked, reason } => view.refused(ranked, reason),
        Answer::NoUsableCommand { rejected } => {
            let mut text = String::from("No usable command:\n");
            for (candidate, reason) in rejected {
                // Debug formatting escapes the bad characters instead of
                // printing them.
                let command = candidate.command();
                text.push_str(&format!("  - {command:?}: {reason}\n"));
            }
            text
        },
        Answer::Declined { reason } => declined(reason),
        Answer::Failed(error) => {
            render_failure(&error.to_string(), Format::Human)
        },
    }
}

fn json(answer: &Answer, view: &View) -> Value {
    let one = |ranked| ranked_json(ranked, view);
    match answer {
        Answer::Confirm(ranked) => {
            json!({ "kind": "confirm", "command": one(ranked) })
        },
        Answer::Choose(ranked) => json!({
            "kind": "choose",
            "commands": ranked.iter().map(one).collect::<Vec<_>>(),
        }),
        Answer::StraightIn { ranked, reason } => json!({
            "kind": "straight_in",
            "command": one(ranked),
            "reason": reason,
        }),
        Answer::ConfirmWithReason { ranked, reason } => json!({
            "kind": "confirm_reason",
            "command": one(ranked),
            "reason": reason,
        }),
        Answer::Refused { ranked, reason } => json!({
            "kind": "refused",
            "command": one(ranked),
            "reason": reason,
        }),
        Answer::NoUsableCommand { rejected } => json!({
            "kind": "no_command",
            "rejected": rejected
                .iter()
                .map(|(candidate, reason)| json!({
                    "command": candidate.command(),
                    "reason": reason.to_string(),
                }))
                .collect::<Vec<_>>(),
        }),
        Answer::Declined { reason } => {
            json!({ "kind": "declined", "reason": reason })
        },
        Answer::Failed(error) => {
            json!({ "kind": "failed", "message": error.to_string() })
        },
    }
}

/// Line 1 is the kind. For one command, line 2 is its rating and line 3 the
/// command, which is empty when it must not reach the prompt. For `choose`,
/// line 2 is the count, then one `rating<TAB>command` line per command, best
/// first. Whatever follows is what to show the user, as it stands.
fn zle(answer: &Answer, view: &View) -> String {
    let one = |kind: &str, ranked: &Ranked, shown: String| {
        let rating = view.rating(ranked).as_str();
        format!("{kind}\n{rating}\n{}\n{shown}", command(ranked))
    };
    match answer {
        Answer::Confirm(ranked) => {
            one("confirm", ranked, view.entry(ranked, "", "  "))
        },
        Answer::Choose(ranked) => {
            let mut text = format!("choose\n{}\n", ranked.len());
            for ranked in ranked {
                let rating = view.rating(ranked).as_str();
                text.push_str(&format!("{rating}\t{}\n", command(ranked)));
            }
            text.push_str(&view.list(ranked));
            text
        },
        Answer::StraightIn { ranked, reason } => one(
            "straight_in",
            ranked,
            view.with_note(ranked, STRAIGHT, reason),
        ),
        Answer::ConfirmWithReason { ranked, reason } => {
            one("confirm", ranked, view.with_note(ranked, ASKING, reason))
        },
        Answer::Refused { ranked, reason } => {
            format!("refused\n\n\n{}", view.refused(ranked, reason))
        },
        Answer::NoUsableCommand { rejected } => {
            let message = match rejected.first() {
                Some((_, reason)) => format!("No usable command: {reason}"),
                None => "No usable command".to_owned(),
            };
            format!("no_command\n\n\n{message}\n")
        },
        // The shells already know what to do with no command.
        Answer::Declined { reason } => {
            format!("no_command\n\n\n{}", declined(reason))
        },
        Answer::Failed(error) => {
            render_failure(&error.to_string(), Format::Zle)
        },
    }
}

/// The reason is the model's own text, so it can't add lines or colours.
fn declined(reason: &str) -> String {
    format!("The model declined: {}\n", to_printable(reason))
}

/// The command is already a single printable line (the command checks
/// guarantee it); `to_printable` keeps that true even if they change.
fn command(ranked: &Ranked) -> String {
    to_printable(ranked.candidate().command())
}

fn ranked_json(ranked: &Ranked, view: &View) -> Value {
    json!({
        "command": ranked.candidate().command(),
        "description": ranked.candidate().description(),
        "risk": ranked.candidate().risk().as_str(),
        "rating": view.rating(ranked).as_str(),
        "warnings": warnings(ranked.candidate().command()),
        "score": ranked.score(),
        "score_reason": ranked.reason(),
    })
}

/// The folder for rating commands, when the caller has none of its own.
pub(crate) fn view_of(cwd: &Path, colour: bool) -> View {
    View {
        cwd: cwd.to_owned(),
        home: std::env::var_os("HOME").map(PathBuf::from),
        colour,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;
    use termite::{
        Answer, AssistError, Candidate, CommandRejected, Ranked, Risk, Service,
    };

    use super::{Format, View, exit_code, render};

    fn view(colour: bool) -> View {
        View {
            cwd: PathBuf::from("/tmp"),
            home: None,
            colour,
        }
    }

    fn plain(answer: &Answer, format: Format) -> String {
        render(answer, format, &view(false))
    }

    fn ranked(command: &str, description: &str, score: f64) -> Ranked {
        Ranked::new(
            Candidate::new(command, description, Risk::Read),
            score,
            "fits the request",
        )
    }

    fn best() -> Ranked {
        ranked("ls -la ~/code", "List the code folder", 0.82)
    }

    fn risky() -> Ranked {
        ranked("rm -rf ~", "Tidy up", 0.4)
    }

    fn all_answers() -> Vec<Answer> {
        vec![
            Answer::Confirm(best()),
            Answer::Choose(vec![best(), risky()]),
            Answer::StraightIn {
                ranked: best(),
                reason: "It only lists files.".into(),
            },
            Answer::ConfirmWithReason {
                ranked: best(),
                reason: "the model labelled it `write`".into(),
            },
            Answer::Refused {
                ranked: best(),
                reason: "Do not touch this folder.".into(),
            },
            Answer::Declined {
                reason: "Not this one.\n\x1b[31m".into(),
            },
            Answer::NoUsableCommand {
                rejected: vec![(
                    Candidate::new("ls\nrm -rf ~", "List", Risk::Read),
                    CommandRejected::MultipleLines,
                )],
            },
            Answer::Failed(AssistError::Timeout {
                service: Service::Model,
            }),
        ]
    }

    const BEST: &str = "✓ reads only  List the code folder\n  ls -la ~/code\n";
    const LIST: &str = "1. ✓ reads only  List the code folder\n   ls -la ~/code\n2. ✗ risky  Tidy up\n   rm -rf ~\n   termite says: wrecks the machine, deletes data\n";

    #[test]
    fn given_each_answer_when_rendered_for_zle_then_the_line_format_is_used() {
        let expected = [
            format!("confirm\nreads\nls -la ~/code\n{BEST}"),
            format!("choose\n2\nreads\tls -la ~/code\nrisky\trm -rf ~\n{LIST}"),
            format!("straight_in\nreads\nls -la ~/code\n{BEST}  Straight to the prompt: It only lists files.\n"),
            format!("confirm\nreads\nls -la ~/code\n{BEST}  Asking first: the model labelled it `write`\n"),
            "refused\n\n\n✗ refused  Do not touch this folder.\n  ls -la ~/code\n".to_owned(),
            "no_command\n\n\nThe model declined: Not this one.  [31m\n".to_owned(),
            "no_command\n\n\nNo usable command: the command spans more than one line\n".to_owned(),
            "failed\n\n\ntmite: the local model did not answer in time\n".to_owned(),
        ];

        for (answer, expected) in all_answers().iter().zip(expected) {
            assert_eq!(plain(answer, Format::Zle), expected, "{answer:?}");
        }
    }

    #[test]
    fn given_a_refusal_when_rendered_for_zle_then_there_is_no_command_to_insert()
     {
        let answer = Answer::Refused {
            ranked: risky(),
            reason: "No.".into(),
        };

        let text = plain(&answer, Format::Zle);
        assert_eq!(text.lines().nth(2), Some(""), "{text}");
    }

    #[test]
    fn given_model_text_with_control_characters_then_zle_lines_stay_intact() {
        let answer =
            Answer::Choose(vec![ranked("ls", "List\nfiles\tnow\x1b[31m", 0.5)]);

        assert_eq!(
            plain(&answer, Format::Zle),
            "choose\n1\nreads\tls\n1. ✓ reads only  List files now [31m\n   ls\n"
        );
    }

    #[test]
    fn given_colour_then_risky_is_red_and_model_text_adds_none() {
        let text = render(
            &Answer::Confirm(ranked("rm -rf ~", "Tidy\x1b[32m", 0.4)),
            Format::Human,
            &view(true),
        );

        assert!(text.contains("\x1b[1;31m✗ risky\x1b[0m"), "{text:?}");
        assert!(text.contains("\x1b[1;31mrm -rf ~\x1b[0m"), "{text:?}");
        assert!(!text.contains("\x1b[32m"), "{text:?}");
    }

    #[test]
    fn given_no_colour_then_there_are_no_escapes() {
        for answer in all_answers() {
            assert!(!plain(&answer, Format::Human).contains('\x1b'));
        }
    }

    #[test]
    fn given_each_answer_when_rendered_as_json_then_it_has_a_kind() {
        let kinds = [
            "confirm",
            "choose",
            "straight_in",
            "confirm_reason",
            "refused",
            "declined",
            "no_command",
            "failed",
        ];

        for (answer, kind) in all_answers().iter().zip(kinds) {
            let value: serde_json::Value =
                serde_json::from_str(&plain(answer, Format::Json))
                    .expect("valid JSON");
            assert_eq!(value["kind"], kind, "{answer:?}");
        }
    }

    #[test]
    fn given_a_confirm_answer_when_rendered_as_json_then_it_has_the_command() {
        let value: serde_json::Value = serde_json::from_str(&plain(
            &Answer::Confirm(best()),
            Format::Json,
        ))
        .expect("valid JSON");

        assert_eq!(
            value,
            json!({
                "kind": "confirm",
                "command": {
                    "command": "ls -la ~/code",
                    "description": "List the code folder",
                    "risk": "read",
                    "rating": "reads",
                    "warnings": [],
                    "score": 0.82,
                    "score_reason": "fits the request",
                },
            })
        );
    }

    #[test]
    fn given_each_answer_when_rendered_for_people_then_it_is_readable() {
        let expected = [
            BEST.to_owned(),
            LIST.to_owned(),
            format!("{BEST}  Straight to the prompt: It only lists files.\n"),
            format!("{BEST}  Asking first: the model labelled it `write`\n"),
            "✗ refused  Do not touch this folder.\n  ls -la ~/code\n".to_owned(),
            "The model declined: Not this one.  [31m\n".to_owned(),
            "No usable command:\n  - \"ls\\nrm -rf ~\": the command spans more than one line\n".to_owned(),
            "tmite: the local model did not answer in time\n".to_owned(),
        ];

        for (answer, expected) in all_answers().iter().zip(expected) {
            assert_eq!(plain(answer, Format::Human), expected, "{answer:?}");
        }
    }

    #[test]
    fn given_each_answer_then_the_exit_code_matches() {
        let codes: Vec<u8> = all_answers().iter().map(exit_code).collect();

        assert_eq!(codes, [0, 0, 0, 0, 2, 2, 2, 1]);
    }
}
