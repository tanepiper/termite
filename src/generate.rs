//! Asks the local model (`llama-server`) for possible commands.

use std::future::Future;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::candidate::{Candidate, Risk};
use crate::config::Config;
use crate::context::Context;
use crate::error::{AssistError, Service};

/// The most commands the model may return for one request.
pub const MAX_CANDIDATES: usize = 5;

/// Something that turns a request into possible commands. A trait because
/// it crosses an HTTP boundary and is mocked in tests.
#[cfg_attr(test, mockall::automock)]
pub trait CommandSource {
    /// Ask for 1 to [`MAX_CANDIDATES`] commands for `request`.
    fn generate(
        &self,
        request: &str,
        context: &Context,
    ) -> impl Future<Output = Result<Vec<Candidate>, AssistError>> + Send;
}

/// `llama-server` from llama.cpp, through its OpenAI-compatible
/// `/v1/chat/completions` endpoint.
pub struct LlamaServer {
    client: reqwest::Client,
    url: String,
    model: String,
}

impl LlamaServer {
    /// Create a client with the generation time limit from `config`.
    pub fn new(config: &Config) -> Result<Self, AssistError> {
        let client = reqwest::Client::builder()
            .timeout(config.generation_timeout())
            .build()
            .map_err(|error| AssistError::from_http(Service::Model, &error))?;

        Ok(Self {
            client,
            url: config.model_url().trim_end_matches('/').to_owned(),
            model: config.model().to_owned(),
        })
    }
}

impl CommandSource for LlamaServer {
    async fn generate(
        &self,
        request: &str,
        context: &Context,
    ) -> Result<Vec<Candidate>, AssistError> {
        let body = generation_request(&self.model, request, context);
        let to_error = |error| AssistError::from_http(Service::Model, &error);

        let reply = self
            .client
            .post(format!("{}/v1/chat/completions", self.url))
            .json(&body)
            .send()
            .await
            .map_err(to_error)?
            .error_for_status()
            .map_err(to_error)?
            .text()
            .await
            .map_err(to_error)?;

        parse_model_reply(&reply)
    }
}

/// The chat request: a short system prompt with the context, the request
/// as the user message, and a JSON schema so the reply is always valid.
pub(crate) fn generation_request(
    model: &str,
    request: &str,
    context: &Context,
) -> Value {
    json!({
        "model": model,
        "temperature": 0.2,
        // Reasoning models think for minutes before a one-line command;
        // the answer is the same without it. llama.cpp reads this, and
        // other servers ignore it.
        "chat_template_kwargs": { "enable_thinking": false },
        "messages": [
            { "role": "system", "content": system_prompt(context) },
            { "role": "user", "content": request },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "candidates",
                "strict": true,
                "schema": candidates_schema(),
            },
        },
    })
}

/// Read the commands out of a chat completion reply.
pub(crate) fn parse_model_reply(
    reply: &str,
) -> Result<Vec<Candidate>, AssistError> {
    let chat: ChatReply = serde_json::from_str(reply)
        .map_err(|error| AssistError::invalid(Service::Model, error))?;
    let content = chat
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| AssistError::invalid(Service::Model, "no choices"))?;

    let reply: CandidatesReply = serde_json::from_str(&content)
        .map_err(|error| AssistError::invalid(Service::Model, error))?;

    let count = reply.candidates.len();
    if count == 0 || count > MAX_CANDIDATES {
        return Err(AssistError::WrongCandidateCount { count });
    }

    reply
        .candidates
        .into_iter()
        .map(|candidate| {
            let risk = Risk::from_label(&candidate.risk).ok_or_else(|| {
                AssistError::UnknownRisk {
                    service: Service::Model,
                    value: candidate.risk.clone(),
                }
            })?;
            Ok(Candidate::new(
                candidate.command,
                candidate.description,
                risk,
            ))
        })
        .collect()
}

/// Kept short on purpose: the model is 7–8B, and the schema already forces
/// the shape of the reply.
fn system_prompt(context: &Context) -> String {
    let shell = context.shell();
    let os = context.os();
    let cwd = context.cwd();
    let session = context.session().as_str();

    format!(
        "You turn a request into shell commands for {shell} on {os}. \
         The current folder is {cwd}. The session is {session}.\n\
         Rules:\n\
         - Give 1 to {MAX_CANDIDATES} different commands that each do the whole request. \
         Give only one if there is only one sensible way.\n\
         - Each command is one line. Do not chain commands unless the request needs it.\n\
         - Prefer standard tools that are installed by default.\n\
         - description: one short sentence saying what the command does.\n\
         - risk: \"read\" if it only looks, \"write\" if it changes something that \
         can be undone, \"destructive\" if it deletes data or cannot be undone."
    )
}

fn candidates_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "candidates": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_CANDIDATES,
                "items": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "description": { "type": "string" },
                        "risk": {
                            "type": "string",
                            "enum": ["read", "write", "destructive"],
                        },
                    },
                    "required": ["command", "description", "risk"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["candidates"],
        "additionalProperties": false,
    })
}

#[derive(Deserialize)]
struct ChatReply {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Deserialize)]
struct CandidatesReply {
    candidates: Vec<ReplyCandidate>,
}

#[derive(Deserialize)]
struct ReplyCandidate {
    command: String,
    description: String,
    risk: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CommandSource, MockCommandSource, candidates_schema,
        generation_request, parse_model_reply, system_prompt,
    };
    use crate::candidate::{Candidate, Risk};
    use crate::context::{Context, Session};
    use crate::error::{AssistError, Service};
    use crate::test_support::ready;

    fn context() -> Context {
        Context::new("macos", "zsh", "/Users/me/code", Session::Local, true)
    }

    fn chat_reply(content: &serde_json::Value) -> String {
        json!({ "choices": [{ "message": { "content": content.to_string() } }] })
            .to_string()
    }

    fn candidates(count: usize) -> serde_json::Value {
        let items: Vec<_> = (0..count)
            .map(|n| json!({ "command": format!("ls {n}"), "description": "List", "risk": "read" }))
            .collect();
        json!({ "candidates": items })
    }

    #[test]
    fn given_a_mock_source_when_asked_then_it_returns_what_it_was_given() {
        let mut source = MockCommandSource::new();
        source.expect_generate().returning(|_, _| {
            Box::pin(async {
                Ok(vec![Candidate::new("ls", "List files", Risk::Read)])
            })
        });

        let result = ready(source.generate("list files", &context()));

        assert_eq!(
            result,
            Ok(vec![Candidate::new("ls", "List files", Risk::Read)])
        );
    }

    #[test]
    fn given_a_request_when_built_then_it_matches_the_spec() {
        let body = generation_request("qwen", "list my code", &context());

        assert_eq!(
            body,
            json!({
                "model": "qwen",
                "temperature": 0.2,
                "chat_template_kwargs": { "enable_thinking": false },
                "messages": [
                    { "role": "system", "content": system_prompt(&context()) },
                    { "role": "user", "content": "list my code" },
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "candidates",
                        "strict": true,
                        "schema": candidates_schema(),
                    },
                },
            })
        );
        assert_eq!(
            candidates_schema()["properties"]["candidates"]["maxItems"],
            5
        );
    }

    #[test]
    fn given_a_context_when_prompted_then_the_prompt_names_it() {
        let prompt = system_prompt(&context());

        for expected in ["zsh", "macos", "/Users/me/code", "local"] {
            assert!(prompt.contains(expected), "prompt names {expected}");
        }
    }

    #[test]
    fn given_a_valid_reply_when_parsed_then_candidates_are_returned() {
        let reply = chat_reply(&json!({ "candidates": [
            { "command": "docker restart web", "description": "Restart web", "risk": "write" },
            { "command": "docker ps", "description": "List containers", "risk": "read" },
        ]}));

        assert_eq!(
            parse_model_reply(&reply),
            Ok(vec![
                Candidate::new(
                    "docker restart web",
                    "Restart web",
                    Risk::Write
                ),
                Candidate::new("docker ps", "List containers", Risk::Read),
            ])
        );
    }

    #[test]
    fn given_a_bad_reply_when_parsed_then_a_typed_error_is_returned() {
        let unknown_risk = chat_reply(&json!({ "candidates": [
            { "command": "ls", "description": "List", "risk": "maybe" },
        ]}));

        assert_eq!(
            parse_model_reply(&chat_reply(&candidates(0))),
            Err(AssistError::WrongCandidateCount { count: 0 })
        );
        assert_eq!(
            parse_model_reply(&chat_reply(&candidates(6))),
            Err(AssistError::WrongCandidateCount { count: 6 })
        );
        assert_eq!(
            parse_model_reply(&unknown_risk),
            Err(AssistError::UnknownRisk {
                service: Service::Model,
                value: "maybe".into(),
            })
        );

        for reply in [
            "not json".to_owned(),
            json!({ "choices": [] }).to_string(),
            json!({ "choices": [{ "message": { "content": "not json" } }] })
                .to_string(),
        ] {
            assert!(
                matches!(
                    parse_model_reply(&reply),
                    Err(AssistError::InvalidReply {
                        service: Service::Model,
                        ..
                    })
                ),
                "{reply}"
            );
        }
    }
}
