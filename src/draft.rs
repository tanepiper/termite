//! Writes a bash script as a draft. A model writes it, termite checks it, and
//! it is only ever saved as text. Nothing here runs it, makes it executable or
//! installs it. Contract: a script with a critical hazard is withheld, and the
//! decision model can only make the answer stricter.

use std::future::Future;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::command::to_printable;
use crate::context::Context;
use crate::decide::{
    DecisionClient, MAX_HARMFUL, MIN_NO_SECRETS, REFUSE_HARMFUL,
};
use crate::error::{AssistError, Service};
use crate::generate::LlamaServer;
use crate::hazard::Severity;
use crate::home::ScriptName;
use crate::progress::Progress;
use crate::rating::Rating;
use crate::script::{Finding, findings};

pub use self::rules::script_problems;

/// The largest script termite will save.
pub const MAX_SCRIPT_BYTES: usize = 64 * 1024;

/// How the first lines end. Everything up to here is the same in every script.
const PREAMBLE: &str = include_str!("../bash/preamble.sh");
const STYLE_GUIDE: &str = include_str!("../prompts/bash-script.md");
const EXAMPLE: &str = include_str!("../bash/examples/disk-report");
const SUMMARY_MARK: &str = "# Summary: ";

/// What the model sends back.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScriptReply {
    pub name: String,
    pub summary: String,
    pub script: String,
}

/// A script that failed its checks, sent back to the model once.
#[derive(Debug, Clone, Copy)]
pub struct Retry<'a> {
    pub script: &'a str,
    pub problems: &'a [String],
}

/// Something that writes a script. A trait because it crosses an HTTP
/// boundary and is mocked in tests.
pub trait ScriptSource {
    fn write(
        &self,
        request: &str,
        context: &Context,
        retry: Option<Retry<'_>>,
        progress: &dyn Progress,
    ) -> impl Future<Output = Result<ScriptReply, AssistError>>;
}

impl ScriptSource for LlamaServer {
    async fn write(
        &self,
        request: &str,
        context: &Context,
        retry: Option<Retry<'_>>,
        progress: &dyn Progress,
    ) -> Result<ScriptReply, AssistError> {
        let body = script_request(
            self.model(),
            request,
            context,
            retry,
            self.stream(),
        );
        let reply = self.chat(&body, progress).await?;
        serde_json::from_str(&reply)
            .map_err(|error| AssistError::invalid(Service::Model, error))
    }
}

/// A script that passed every check, ready to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub name: ScriptName,
    pub summary: String,
    /// The whole script, first lines included.
    pub script: String,
    /// The lines to look at when you review it.
    pub flags: Vec<Finding>,
    /// What the decision model said. These never approve anything.
    pub notes: Vec<String>,
}

/// How a request for a script ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drafted {
    Ready(Draft),
    /// It failed its checks twice. Nothing was saved.
    Rejected {
        problems: Vec<String>,
    },
    /// It is likely harmful. It isn't shown and nothing was saved.
    Withheld {
        reasons: Vec<String>,
    },
}

/// Everything [`write_draft`] needs to know about where it runs.
#[derive(Debug, Clone, Copy)]
pub struct DraftPlace<'a> {
    /// Where installed scripts go.
    pub bin: &'a Path,
    pub cwd: &'a Path,
    pub home: Option<&'a Path>,
}

/// Ask for a script and check it. `outside_checks` covers what needs another
/// program: syntax and lint. A script that fails gets one more try.
pub async fn write_draft(
    source: &impl ScriptSource,
    decider: &impl DecisionClient,
    request: &str,
    context: &Context,
    place: DraftPlace<'_>,
    outside_checks: impl Fn(&str) -> Vec<String>,
    progress: &dyn Progress,
) -> Result<Drafted, AssistError> {
    let mut reply = source.write(request, context, None, progress).await?;
    let mut script = assemble(&reply);
    if let Some(drafted) = withheld(&script, place) {
        return Ok(drafted);
    }
    let check = |reply: &ScriptReply, script: &str| {
        let mut problems = name_problems(&reply.name, place.bin);
        problems.extend(script_problems(script));
        if problems.is_empty() {
            problems = outside_checks(script);
        }
        problems
    };
    let mut problems = check(&reply, &script);
    if !problems.is_empty() {
        let retry = Retry {
            script: &reply.script,
            problems: &problems,
        };
        reply = source
            .write(request, context, Some(retry), progress)
            .await?;
        script = assemble(&reply);
        if let Some(drafted) = withheld(&script, place) {
            return Ok(drafted);
        }
        problems = check(&reply, &script);
    }
    if !problems.is_empty() {
        return Ok(Drafted::Rejected { problems });
    }
    let Ok(name) = ScriptName::parse(&reply.name) else {
        return Ok(Drafted::Rejected {
            problems: vec![format!("bad name: {}", to_printable(&reply.name))],
        });
    };

    let mut notes = Vec::new();
    match decider.safety(request, context, body_of(&script)).await {
        Ok(verdict) if verdict.harmful() >= REFUSE_HARMFUL => {
            return Ok(Drafted::Withheld {
                reasons: vec!["the decision model rated it harmful".to_owned()],
            });
        },
        Ok(verdict) => {
            if verdict.harmful() > MAX_HARMFUL {
                notes.push(
                    "The decision model says this could be harmful.".to_owned(),
                );
            }
            if verdict.no_secrets() < MIN_NO_SECRETS {
                notes.push(
                    "The decision model says this may show secrets.".to_owned(),
                );
            }
        },
        Err(_) => notes
            .push("The decision model did not rate this script.".to_owned()),
    }

    Ok(Drafted::Ready(Draft {
        name,
        summary: to_printable(&reply.summary),
        flags: flags(&script, place.cwd, place.home),
        script,
        notes,
    }))
}

/// The lines of `script` to look at, leaving out termite's own first lines.
pub fn flags(script: &str, cwd: &Path, home: Option<&Path>) -> Vec<Finding> {
    let first_lines = PREAMBLE.lines().count();
    findings(script, cwd, home)
        .into_iter()
        .filter(|finding| {
            finding.line > first_lines && finding.rating == Rating::Risky
        })
        .collect()
}

/// A critical hazard stops here, with no second try: the model must not get a
/// chance to reword it.
pub fn critical(script: &str, cwd: &Path, home: Option<&Path>) -> Vec<String> {
    let mut reasons: Vec<String> = flags(script, cwd, home)
        .iter()
        .filter(|finding| finding.severity == Some(Severity::Critical))
        .flat_map(|finding| finding.warnings.iter())
        .map(|warning| (*warning).to_owned())
        .collect();
    reasons.sort();
    reasons.dedup();
    reasons
}

fn withheld(script: &str, place: DraftPlace<'_>) -> Option<Drafted> {
    let reasons = critical(script, place.cwd, place.home);
    (!reasons.is_empty()).then_some(Drafted::Withheld { reasons })
}

/// The summary saved in a script, if it has one.
pub fn summary_of(script: &str) -> Option<&str> {
    script
        .strip_prefix(PREAMBLE)?
        .lines()
        .next()?
        .strip_prefix(SUMMARY_MARK)
}

fn name_problems(name: &str, bin: &Path) -> Vec<String> {
    let checked = ScriptName::parse(name).and_then(|name| name.check_free(bin));
    checked
        .err()
        .map(|taken| taken.to_string())
        .into_iter()
        .collect()
}

/// termite's first lines, then the model's script. The model can't change the
/// first lines, because they are never taken from its reply.
fn assemble(reply: &ScriptReply) -> String {
    let script = reply.script.trim_start();
    let script = script.strip_prefix(PREAMBLE).unwrap_or(script);
    let script = match script.strip_prefix("#!") {
        Some(rest) => rest.split_once('\n').map_or("", |(_, rest)| rest),
        None => script,
    };
    let summary = to_printable(&reply.summary);
    format!("{PREAMBLE}{SUMMARY_MARK}{summary}\n\n{}\n", script.trim())
}

fn body_of(script: &str) -> &str {
    script.strip_prefix(PREAMBLE).unwrap_or(script)
}

fn script_request(
    model: &str,
    request: &str,
    context: &Context,
    retry: Option<Retry<'_>>,
    stream: bool,
) -> Value {
    let mut messages = vec![
        json!({ "role": "system", "content": system_prompt(context) }),
        json!({ "role": "user", "content": to_printable(request) }),
    ];
    if let Some(retry) = retry {
        let problems: Vec<String> =
            retry.problems.iter().map(|p| to_printable(p)).collect();
        messages.push(json!({ "role": "assistant", "content": retry.script }));
        messages.push(json!({
            "role": "user",
            "content": format!(
                "That script failed these checks. Write the whole script again.\n- {}",
                problems.join("\n- ")
            ),
        }));
    }
    json!({
        "model": model,
        "temperature": 0.2,
        "stream": stream,
        "chat_template_kwargs": { "enable_thinking": false },
        "messages": messages,
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": "script", "strict": true, "schema": schema() },
        },
    })
}

fn system_prompt(context: &Context) -> String {
    let os = to_printable(context.os());
    let example = EXAMPLE.strip_prefix(PREAMBLE).unwrap_or(EXAMPLE).trim();
    format!(
        "{STYLE_GUIDE}\nThe script runs on {os}.\n\nA complete example:\n\n{example}\n"
    )
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "summary": { "type": "string" },
            "script": { "type": "string" },
        },
        "required": ["name", "summary", "script"],
        "additionalProperties": false,
    })
}

mod rules;
#[cfg(test)]
mod tests;
