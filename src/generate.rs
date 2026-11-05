//! Asks the local model (`llama-server`) for possible commands.

use std::future::Future;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::candidate::{Candidate, Risk};
use crate::command::to_printable;
use crate::config::Config;
use crate::context::Context;
use crate::error::{AssistError, Service};
use crate::heartbeat::{self, CHECK_EVERY};
use crate::progress::{Progress, Stage};
use crate::sse::{ChatStream, MAX_REPLY_BYTES, too_long};

/// The most commands the model may return for one request.
pub const MAX_CANDIDATES: usize = 5;

/// How long a connection check may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(3);

/// Something that turns a request into possible commands. A trait because
/// it crosses an HTTP boundary and is mocked in tests.
#[cfg_attr(test, mockall::automock)]
pub trait CommandSource {
    /// Ask for 1 to [`MAX_CANDIDATES`] commands for `request`, reporting to
    /// `progress` as the reply arrives.
    fn generate(
        &self,
        request: &str,
        context: &Context,
        progress: &dyn Progress,
    ) -> impl Future<Output = Result<Vec<Candidate>, AssistError>> + Send;
}

/// `llama-server` from llama.cpp, through its OpenAI-compatible
/// `/v1/chat/completions` endpoint.
pub struct LlamaServer {
    client: reqwest::Client,
    url: String,
    model: String,
    stream: bool,
}

impl LlamaServer {
    /// Create a client with the generation time limit from `config`.
    pub fn new(config: &Config) -> Result<Self, AssistError> {
        Self::with(config, config.model(), config.generation_timeout())
    }

    /// Create a client for writing scripts, which take longer than commands
    /// and may use a stronger model.
    pub fn for_scripts(config: &Config) -> Result<Self, AssistError> {
        Self::with(config, config.script_model(), config.script_timeout())
    }

    fn with(
        config: &Config,
        model: &str,
        timeout: Duration,
    ) -> Result<Self, AssistError> {
        let client = config
            .model_client()
            .timeout(timeout)
            .build()
            .map_err(|error| AssistError::from_http(Service::Model, &error))?;

        Ok(Self {
            client,
            url: config.model_url().trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            stream: config.stream(),
        })
    }

    pub(crate) fn model(&self) -> &str {
        &self.model
    }

    pub(crate) fn stream(&self) -> bool {
        self.stream
    }

    /// Send a chat request and return the reply text, watching the server
    /// during the wait.
    pub(crate) async fn chat(
        &self,
        body: &Value,
        progress: &dyn Progress,
    ) -> Result<String, AssistError> {
        heartbeat::watch(
            self.send(body, progress),
            CHECK_EVERY,
            || self.answers_check(),
            progress,
        )
        .await
    }

    /// Any reply at all means the server is there, however busy it is.
    async fn answers_check(&self) -> bool {
        self.client
            .get(format!("{}/v1/models", self.url))
            .timeout(CHECK_TIMEOUT)
            .send()
            .await
            .is_ok()
    }

    async fn send(
        &self,
        body: &Value,
        progress: &dyn Progress,
    ) -> Result<String, AssistError> {
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.url))
            .json(body)
            .send()
            .await
            .map_err(model_error)?
            .error_for_status()
            .map_err(model_error)?;

        if is_event_stream(&response) {
            read_stream(response, progress).await
        } else {
            reply_content(&read_whole(response).await?)
        }
    }
}

impl CommandSource for LlamaServer {
    async fn generate(
        &self,
        request: &str,
        context: &Context,
        progress: &dyn Progress,
    ) -> Result<Vec<Candidate>, AssistError> {
        let body =
            generation_request(&self.model, request, context, self.stream);
        parse_candidates(&self.chat(&body, progress).await?)
    }
}

/// Servers that do not stream ignore `stream` and send one JSON reply.
fn is_event_stream(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"))
}

/// The reply text of a stream, reporting as it grows.
async fn read_stream(
    mut response: reqwest::Response,
    progress: &dyn Progress,
) -> Result<String, AssistError> {
    let mut stream = ChatStream::default();
    while let Some(bytes) = response.chunk().await.map_err(model_error)? {
        stream.push(&bytes)?;
        progress.stage(Stage::Writing {
            chars: stream.chars(),
        });
    }
    stream.finish()
}

/// A reply sent in one piece, with the same limit as a stream.
async fn read_whole(
    mut response: reqwest::Response,
) -> Result<String, AssistError> {
    let mut body = Vec::new();
    while let Some(bytes) = response.chunk().await.map_err(model_error)? {
        if body.len() + bytes.len() > MAX_REPLY_BYTES {
            return Err(too_long());
        }
        body.extend_from_slice(&bytes);
    }
    String::from_utf8(body)
        .map_err(|error| AssistError::invalid(Service::Model, error))
}

fn model_error(error: reqwest::Error) -> AssistError {
    AssistError::from_http(Service::Model, &error)
}

/// The chat request: a short system prompt with the context, the request
/// as the user message, and a JSON schema so the reply is always valid.
pub(crate) fn generation_request(
    model: &str,
    request: &str,
    context: &Context,
    stream: bool,
) -> Value {
    json!({
        "model": model,
        "temperature": 0.2,
        "stream": stream,
        // Reasoning models think for minutes before a one-line command;
        // the answer is the same without it. llama.cpp reads this, and
        // other servers ignore it.
        "chat_template_kwargs": { "enable_thinking": false },
        "messages": [
            { "role": "system", "content": system_prompt(context) },
            { "role": "user", "content": to_printable(request) },
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

/// Read the commands out of a whole chat completion reply.
#[cfg(test)]
pub(crate) fn parse_model_reply(
    reply: &str,
) -> Result<Vec<Candidate>, AssistError> {
    parse_candidates(&reply_content(reply)?)
}

/// The text of a whole chat completion reply.
fn reply_content(reply: &str) -> Result<String, AssistError> {
    let chat: ChatReply = serde_json::from_str(reply)
        .map_err(|error| AssistError::invalid(Service::Model, error))?;
    chat.choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| AssistError::invalid(Service::Model, "no choices"))
}

/// Read the commands out of the reply text.
fn parse_candidates(content: &str) -> Result<Vec<Candidate>, AssistError> {
    let reply: CandidatesReply = serde_json::from_str(content)
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
    // A folder name may hold a line break or escape codes; it must not add
    // lines to the prompt.
    let shell = to_printable(context.shell());
    let os = to_printable(context.os());
    let cwd = to_printable(context.cwd());
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    use serde_json::json;

    use super::{
        CommandSource, LlamaServer, MAX_REPLY_BYTES, MockCommandSource,
        candidates_schema, generation_request, parse_model_reply,
        system_prompt,
    };
    use crate::candidate::{Candidate, Risk};
    use crate::config::Config;
    use crate::context::{Context, Session};
    use crate::error::{AssistError, Service};
    use crate::progress::{NoProgress, Progress, Stage};
    use crate::test_support::ready;

    const CONFIG: &str = r#"{"model_url":"http://127.0.0.1:1","model":"m","decision_url":"http://127.0.0.1:1","decision_model":"d"}"#;

    fn context() -> Context {
        Context::new("macos", "zsh", "/Users/me/code", Session::Local)
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
        source.expect_generate().returning(|_, _, _| {
            Box::pin(async {
                Ok(vec![Candidate::new("ls", "List files", Risk::Read)])
            })
        });

        let result =
            ready(source.generate("list files", &context(), &NoProgress));

        assert_eq!(
            result,
            Ok(vec![Candidate::new("ls", "List files", Risk::Read)])
        );
    }

    #[test]
    fn given_a_request_when_built_then_it_matches_the_spec() {
        let body = generation_request("qwen", "list my code", &context(), true);

        assert_eq!(
            body,
            json!({
                "model": "qwen",
                "temperature": 0.2,
                "stream": true,
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

    /// Serve one canned HTTP reply on a local port, and return the URL.
    fn serve_once(content_type: &str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 8192];
            let _ = socket.read(&mut request);
            socket.write_all(head.as_bytes()).expect("write head");
            socket.write_all(body.as_bytes()).expect("write body");
        });
        format!("http://127.0.0.1:{port}")
    }

    fn ask(
        url: &str,
        progress: &dyn Progress,
    ) -> Result<Vec<Candidate>, AssistError> {
        let config = Config::from_json(&format!(
            r#"{{"model_url":"{url}","model":"m","decision_url":"{url}","decision_model":"d"}}"#
        ))
        .expect("config");
        let server = LlamaServer::new(&config).expect("client");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(server.generate("list", &context(), progress))
    }

    #[derive(Default)]
    struct LastStage(Mutex<Option<Stage>>);

    impl Progress for LastStage {
        fn stage(&self, stage: Stage) {
            *self.0.lock().expect("not poisoned") = Some(stage);
        }
    }

    #[test]
    fn given_a_streaming_server_then_the_reply_is_read_as_it_grows() {
        let content = candidates(1).to_string();
        let (first, second) = content.split_at(5);
        let event = |text: &str| {
            let event =
                json!({ "choices": [{ "delta": { "content": text } }] });
            format!("data: {event}\n\n")
        };
        let body = format!("{}{}data: [DONE]\n\n", event(first), event(second));
        let url = serve_once("text/event-stream; charset=utf-8", body);
        let progress = LastStage::default();

        let reply = ask(&url, &progress);

        assert_eq!(reply, Ok(vec![Candidate::new("ls 0", "List", Risk::Read)]));
        assert_eq!(
            *progress.0.lock().expect("not poisoned"),
            Some(Stage::Writing {
                chars: content.len()
            })
        );
    }

    #[test]
    fn given_a_server_that_ignores_stream_then_one_reply_is_read() {
        let url = serve_once("application/json", chat_reply(&candidates(1)));

        assert_eq!(
            ask(&url, &NoProgress),
            Ok(vec![Candidate::new("ls 0", "List", Risk::Read)])
        );
    }

    #[test]
    fn given_a_whole_reply_over_the_limit_then_it_is_refused() {
        let url =
            serve_once("application/json", "x".repeat(MAX_REPLY_BYTES + 1));

        assert_eq!(
            ask(&url, &NoProgress),
            Err(AssistError::invalid(
                Service::Model,
                "the reply is too long"
            ))
        );
    }

    #[test]
    fn given_streaming_is_off_then_the_request_says_so() {
        let body = generation_request("qwen", "x", &context(), false);
        assert_eq!(body["stream"], false);

        let on = Config::from_json(CONFIG).expect("config");
        assert!(on.stream());
        let off = CONFIG.replace('}', r#","stream":false}"#);
        let off = Config::from_json(&off).expect("config");
        assert!(!off.stream());
    }

    #[test]
    fn given_a_server_then_the_check_says_whether_it_is_there() {
        let check = |url: &str| {
            let config = CONFIG.replace("http://127.0.0.1:1", url);
            let config = Config::from_json(&config).expect("config");
            let server = LlamaServer::new(&config).expect("client");
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime")
                .block_on(server.answers_check())
        };

        assert!(check(&serve_once("text/plain", "busy".to_owned())));
        assert!(!check("http://127.0.0.1:1"));
    }
}
