//! The three ways `tmite` prints an answer: for people, as JSON for
//! scripts, and as plain lines for the zsh widget.
//!
//! Every piece of model-written text goes through `to_printable` first, so
//! nothing the model writes can move the cursor, change colours or add a
//! line to the line format.

use serde_json::{Value, json};
use termite::{Answer, Ranked, to_printable};

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

/// Render `answer` in `format`, ending with a new line.
pub(crate) fn render(answer: &Answer, format: Format) -> String {
    match format {
        Format::Human => human(answer),
        Format::Json => format!("{}\n", json(answer)),
        Format::Zle => zle(answer),
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
        Format::Zle => format!("failed\n\ntmite: {message}\n"),
    }
}

/// The exit status for an answer: 0 when there is a command to use, 2 when
/// there is none or it was refused, 1 when something failed.
pub(crate) fn exit_code(answer: &Answer) -> u8 {
    match answer {
        Answer::Failed(_) => 1,
        Answer::NoUsableCommand { .. } | Answer::Refused { .. } => 2,
        _ => 0,
    }
}

fn human(answer: &Answer) -> String {
    match answer {
        Answer::Confirm(ranked) => {
            format!("{}\n  {}\n", command(ranked), details(ranked))
        },
        Answer::Choose(ranked) => ranked
            .iter()
            .enumerate()
            .map(|(index, ranked)| {
                let number = index + 1;
                let best = if index == 0 { "   (best)" } else { "" };
                format!(
                    "{number}. {}{best}\n   {}\n",
                    command(ranked),
                    details(ranked)
                )
            })
            .collect(),
        Answer::RunNow { ranked, reason } => format!(
            "{}\n  Safe to run without asking: {}\n",
            command(ranked),
            to_printable(reason)
        ),
        Answer::ConfirmWithReason { ranked, reason } => format!(
            "{}\n  Not run automatically: {}\n",
            command(ranked),
            to_printable(reason)
        ),
        Answer::Refused { ranked, reason } => format!(
            "Refused: {}\n  {}\n",
            to_printable(reason),
            command(ranked)
        ),
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
        Answer::Failed(error) => {
            render_failure(&error.to_string(), Format::Human)
        },
    }
}

fn json(answer: &Answer) -> Value {
    match answer {
        Answer::Confirm(ranked) => {
            json!({ "kind": "confirm", "command": ranked_json(ranked) })
        },
        Answer::Choose(ranked) => json!({
            "kind": "choose",
            "commands": ranked.iter().map(ranked_json).collect::<Vec<_>>(),
        }),
        Answer::RunNow { ranked, reason } => json!({
            "kind": "run",
            "command": ranked_json(ranked),
            "reason": reason,
        }),
        Answer::ConfirmWithReason { ranked, reason } => json!({
            "kind": "confirm_reason",
            "command": ranked_json(ranked),
            "reason": reason,
        }),
        Answer::Refused { ranked, reason } => json!({
            "kind": "refused",
            "command": ranked_json(ranked),
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
        Answer::Failed(error) => {
            json!({ "kind": "failed", "message": error.to_string() })
        },
    }
}

/// Line 1 is the kind. For one command, line 2 is the command and line 3
/// the message. For `choose`, line 2 is the count, then one
/// `command<TAB>description` line per command, best first. For
/// `no_command` and `failed`, line 2 is empty and line 3 the message.
fn zle(answer: &Answer) -> String {
    match answer {
        Answer::Confirm(ranked) => {
            format!("confirm\n{}\n{}\n", command(ranked), details(ranked))
        },
        Answer::Choose(ranked) => {
            let mut text = format!("choose\n{}\n", ranked.len());
            for ranked in ranked {
                text.push_str(&format!(
                    "{}\t{}\n",
                    command(ranked),
                    details(ranked)
                ));
            }
            text
        },
        Answer::RunNow { ranked, reason } => format!(
            "run\n{}\nRan without asking: {}\n",
            command(ranked),
            to_printable(reason)
        ),
        Answer::ConfirmWithReason { ranked, reason } => format!(
            "confirm_reason\n{}\nNot run automatically: {}\n",
            command(ranked),
            to_printable(reason)
        ),
        Answer::Refused { ranked, reason } => format!(
            "refused\n{}\nRefused: {}\n",
            command(ranked),
            to_printable(reason)
        ),
        Answer::NoUsableCommand { rejected } => {
            let message = match rejected.first() {
                Some((_, reason)) => format!("No usable command: {reason}"),
                None => "No usable command".to_owned(),
            };
            format!("no_command\n\n{message}\n")
        },
        Answer::Failed(error) => {
            render_failure(&error.to_string(), Format::Zle)
        },
    }
}

/// The command is already a single printable line (the command checks
/// guarantee it); `to_printable` keeps that true even if they change.
fn command(ranked: &Ranked) -> String {
    to_printable(ranked.candidate().command())
}

fn details(ranked: &Ranked) -> String {
    let description = to_printable(ranked.candidate().description());
    let risk = ranked.candidate().risk().as_str();
    let score = ranked.score();

    format!("{description} ({risk}, score {score:.2})")
}

fn ranked_json(ranked: &Ranked) -> Value {
    json!({
        "command": ranked.candidate().command(),
        "description": ranked.candidate().description(),
        "risk": ranked.candidate().risk().as_str(),
        "score": ranked.score(),
        "score_reason": ranked.reason(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use termite::{
        Answer, AssistError, Candidate, CommandRejected, Ranked, Risk, Service,
    };

    use super::{Format, exit_code, render};

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

    fn all_answers() -> Vec<Answer> {
        vec![
            Answer::Confirm(best()),
            Answer::Choose(vec![
                best(),
                ranked("tree ~/code", "Show a tree", 0.4),
            ]),
            Answer::RunNow {
                ranked: best(),
                reason: "It only lists files.".into(),
            },
            Answer::ConfirmWithReason {
                ranked: best(),
                reason: "it runs over SSH".into(),
            },
            Answer::Refused {
                ranked: best(),
                reason: "Do not touch this folder.".into(),
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

    #[test]
    fn given_each_answer_when_rendered_for_zle_then_the_line_format_is_used() {
        let expected = [
            "confirm\nls -la ~/code\nList the code folder (read, score 0.82)\n",
            "choose\n2\nls -la ~/code\tList the code folder (read, score 0.82)\ntree ~/code\tShow a tree (read, score 0.40)\n",
            "run\nls -la ~/code\nRan without asking: It only lists files.\n",
            "confirm_reason\nls -la ~/code\nNot run automatically: it runs over SSH\n",
            "refused\nls -la ~/code\nRefused: Do not touch this folder.\n",
            "no_command\n\nNo usable command: the command spans more than one line\n",
            "failed\n\ntmite: the local model did not answer in time\n",
        ];

        for (answer, expected) in all_answers().iter().zip(expected) {
            assert_eq!(render(answer, Format::Zle), expected, "{answer:?}");
        }
    }

    #[test]
    fn given_model_text_with_control_characters_then_zle_lines_stay_intact() {
        let answer =
            Answer::Choose(vec![ranked("ls", "List\nfiles\tnow\x1b[31m", 0.5)]);

        assert_eq!(
            render(&answer, Format::Zle),
            "choose\n1\nls\tList files now [31m (read, score 0.50)\n"
        );
    }

    #[test]
    fn given_each_answer_when_rendered_as_json_then_it_has_a_kind() {
        let kinds = [
            "confirm",
            "choose",
            "run",
            "confirm_reason",
            "refused",
            "no_command",
            "failed",
        ];

        for (answer, kind) in all_answers().iter().zip(kinds) {
            let value: serde_json::Value =
                serde_json::from_str(&render(answer, Format::Json))
                    .expect("valid JSON");
            assert_eq!(value["kind"], kind, "{answer:?}");
        }
    }

    #[test]
    fn given_a_confirm_answer_when_rendered_as_json_then_it_has_the_command() {
        let value: serde_json::Value = serde_json::from_str(&render(
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
                    "score": 0.82,
                    "score_reason": "fits the request",
                },
            })
        );
    }

    #[test]
    fn given_each_answer_when_rendered_for_people_then_it_is_readable() {
        let expected = [
            "ls -la ~/code\n  List the code folder (read, score 0.82)\n",
            "1. ls -la ~/code   (best)\n   List the code folder (read, score 0.82)\n2. tree ~/code\n   Show a tree (read, score 0.40)\n",
            "ls -la ~/code\n  Safe to run without asking: It only lists files.\n",
            "ls -la ~/code\n  Not run automatically: it runs over SSH\n",
            "Refused: Do not touch this folder.\n  ls -la ~/code\n",
            "No usable command:\n  - \"ls\\nrm -rf ~\": the command spans more than one line\n",
            "tmite: the local model did not answer in time\n",
        ];

        for (answer, expected) in all_answers().iter().zip(expected) {
            assert_eq!(render(answer, Format::Human), expected, "{answer:?}");
        }
    }

    #[test]
    fn given_each_answer_then_the_exit_code_matches() {
        let codes: Vec<u8> = all_answers().iter().map(exit_code).collect();

        assert_eq!(codes, [0, 0, 0, 0, 2, 2, 1]);
    }
}
