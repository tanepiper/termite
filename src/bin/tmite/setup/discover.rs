//! Finds local model servers and the models they offer: llama.cpp
//! (including the Llama app's router), Ollama, LM Studio, and any other
//! OpenAI-compatible server already in the config.

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

/// Where each kind of server listens by default.
const DEFAULT_URLS: &[&str] = &[
    "http://127.0.0.1:8080",  // llama.cpp llama-server
    "http://127.0.0.1:1234",  // LM Studio
    "http://127.0.0.1:11434", // Ollama
];

/// How long to wait for a server that may not be there.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Model names that cannot answer a chat request.
const NOT_CHAT: &[&str] = &["embed", "rerank", "image"];

/// The software behind a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerKind {
    LlamaCpp,
    Ollama,
    LmStudio,
    OpenAiCompatible,
}

impl ServerKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ServerKind::LlamaCpp => "llama.cpp",
            ServerKind::Ollama => "Ollama",
            ServerKind::LmStudio => "LM Studio",
            ServerKind::OpenAiCompatible => "OpenAI-compatible server",
        }
    }
}

/// What a model can be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelRole {
    /// Writes text: used to generate commands.
    Chat,
    /// A llama.cpp decision model: used through `/v1/systemone`.
    Decision,
}

/// A model a server offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FoundModel {
    pub(crate) id: String,
    pub(crate) role: ModelRole,
    /// `Some` only where the server reports it (the llama.cpp router).
    pub(crate) loaded: Option<bool>,
}

/// A running server and its models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FoundServer {
    pub(crate) kind: ServerKind,
    pub(crate) url: String,
    pub(crate) models: Vec<FoundModel>,
    /// For a llama.cpp router, how many models it keeps loaded at once.
    pub(crate) models_max: Option<u32>,
}

/// A running llama.cpp server process, read from `ps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlamaProcess {
    pub(crate) port: u16,
    pub(crate) models_max: Option<u32>,
}

/// A server program that is installed but not running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) kind: ServerKind,
    pub(crate) path: PathBuf,
    pub(crate) start: String,
}

/// Find the llama.cpp servers in `ps -axo pid,ppid,args` output. A router
/// starts one child server per loaded model; children are skipped, because
/// the router already lists their models.
pub(crate) fn llama_processes(ps_output: &str) -> Vec<LlamaProcess> {
    let rows: Vec<(u32, u32, Vec<&str>)> = ps_output
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let pid = words.next()?.parse().ok()?;
            let ppid = words.next()?.parse().ok()?;
            let args: Vec<&str> = words.collect();
            is_llama_server(&args).then_some((pid, ppid, args))
        })
        .collect();
    let pids: Vec<u32> = rows.iter().map(|(pid, _, _)| *pid).collect();

    rows.into_iter()
        .filter(|(_, ppid, _)| !pids.contains(ppid))
        .map(|(_, _, args)| LlamaProcess {
            port: option_value(&args, "--port")
                .and_then(|port| port.parse().ok())
                .unwrap_or(8080),
            models_max: option_value(&args, "--models-max")
                .and_then(|max| max.parse().ok()),
        })
        .collect()
}

/// The URLs to probe, without repeats: the configured ones first, then
/// running llama.cpp servers, then the default ports.
pub(crate) fn candidate_urls(
    configured: &[String],
    processes: &[LlamaProcess],
) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    let process_urls = processes
        .iter()
        .map(|process| format!("http://127.0.0.1:{}", process.port));
    let defaults = DEFAULT_URLS.iter().map(|url| (*url).to_owned());

    for url in configured
        .iter()
        .cloned()
        .chain(process_urls)
        .chain(defaults)
    {
        let url = url.trim_end_matches('/').replace("localhost", "127.0.0.1");
        if !urls.contains(&url) {
            urls.push(url);
        }
    }

    urls
}

/// Read a `/v1/models` reply. Returns the models, and whether the server
/// is llama.cpp (it names itself as the owner).
pub(crate) fn parse_models(body: &str) -> Option<(Vec<FoundModel>, bool)> {
    let reply: ModelsReply = serde_json::from_str(body).ok()?;
    let is_llama_cpp = reply
        .data
        .iter()
        .any(|model| model.owned_by.as_deref() == Some("llamacpp"));

    let models = reply
        .data
        .into_iter()
        .filter_map(|model| {
            let is_decision = model.tags.iter().any(|tag| tag == "decision")
                || model.architecture.as_ref().is_some_and(|architecture| {
                    architecture
                        .output_modalities
                        .iter()
                        .any(|modality| modality == "decisions")
                });
            let lower = model.id.to_lowercase();
            if !is_decision && NOT_CHAT.iter().any(|word| lower.contains(word))
            {
                return None;
            }

            Some(FoundModel {
                role: if is_decision {
                    ModelRole::Decision
                } else {
                    ModelRole::Chat
                },
                loaded: model.status.map(|status| status.value == "loaded"),
                id: model.id,
            })
        })
        .collect();

    Some((models, is_llama_cpp))
}

/// Probe one URL. `None` when nothing answers `/v1/models`.
pub(crate) async fn probe(
    client: &reqwest::Client,
    url: &str,
    processes: &[LlamaProcess],
) -> Option<FoundServer> {
    let body = get_text(client, &format!("{url}/v1/models")).await?;
    let (models, is_llama_cpp) = parse_models(&body)?;

    let mut models = models;
    let kind = if is_llama_cpp {
        ServerKind::LlamaCpp
    } else if get_text(client, &format!("{url}/api/version"))
        .await
        .is_some()
    {
        for model in &mut models {
            if let Some(role) = ollama_role(client, url, &model.id).await {
                model.role = role;
            }
        }
        ServerKind::Ollama
    } else if get_text(client, &format!("{url}/api/v0/models"))
        .await
        .is_some()
    {
        ServerKind::LmStudio
    } else {
        ServerKind::OpenAiCompatible
    };
    let models_max = processes
        .iter()
        .find(|process| url.ends_with(&format!(":{}", process.port)))
        .and_then(|process| process.models_max);

    Some(FoundServer {
        kind,
        url: url.to_owned(),
        models,
        models_max,
    })
}

/// Probe every candidate URL.
pub(crate) async fn discover(configured: &[String]) -> Vec<FoundServer> {
    let processes = llama_processes(&ps_output());
    let Ok(client) = reqwest::Client::builder().timeout(PROBE_TIMEOUT).build()
    else {
        return Vec::new();
    };

    let mut servers = Vec::new();
    for url in candidate_urls(configured, &processes) {
        if let Some(server) = probe(&client, &url, &processes).await {
            servers.push(server);
        }
    }

    servers
}

/// Server programs that are installed, with the command that starts each.
/// Only kinds with no running server are worth showing.
pub(crate) fn installed(running: &[FoundServer]) -> Vec<Installed> {
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let is_running =
        |kind: ServerKind| running.iter().any(|server| server.kind == kind);
    let mut found = Vec::new();

    if !is_running(ServerKind::LlamaCpp) {
        if let Some(path) = find_on_path("llama-server") {
            found.push(Installed {
                kind: ServerKind::LlamaCpp,
                path,
                start: "tmite setup can start it for you".to_owned(),
            });
        }
        let app = PathBuf::from("/Applications/Llama.app");
        if app.exists() {
            found.push(Installed {
                kind: ServerKind::LlamaCpp,
                path: app,
                start: "open -a Llama".to_owned(),
            });
        }
    }

    if !is_running(ServerKind::Ollama) {
        if let Some(path) = find_on_path("ollama") {
            found.push(Installed {
                kind: ServerKind::Ollama,
                path,
                start: "ollama serve".to_owned(),
            });
        }
    }

    if !is_running(ServerKind::LmStudio) {
        let lms = [
            home.join(".cache/lm-studio/bin/lms"),
            home.join(".lmstudio/bin/lms"),
        ]
        .into_iter()
        .find(|path| path.exists())
        .or_else(|| find_on_path("lms"));
        let app = PathBuf::from("/Applications/LM Studio.app");

        match lms {
            Some(path) => found.push(Installed {
                kind: ServerKind::LmStudio,
                start: format!("{} server start", path.display()),
                path,
            }),
            None if app.exists() => found.push(Installed {
                kind: ServerKind::LmStudio,
                path: app,
                start: "open LM Studio and turn on its local server".to_owned(),
            }),
            None => {},
        }
    }

    found
}

/// Find an executable on `PATH`.
pub(crate) fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata().is_ok_and(|meta| {
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    })
}

fn is_llama_server(args: &[&str]) -> bool {
    let Some(program) = args.first() else {
        return false;
    };
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    name == "llama-server" || (name == "llama" && args.get(1) == Some(&"serve"))
}

fn option_value<'a>(args: &[&'a str], option: &str) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(index, arg)| {
        if *arg == option {
            return args.get(index + 1).copied();
        }
        arg.strip_prefix(option)?.strip_prefix('=')
    })
}

fn ps_output() -> String {
    std::process::Command::new("ps")
        .args(["-axo", "pid,ppid,args"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

/// Ollama lists decision models among the others in `/v1/models`; its
/// `/api/show` capabilities tell them apart.
async fn ollama_role(
    client: &reqwest::Client,
    url: &str,
    model: &str,
) -> Option<ModelRole> {
    let response = client
        .post(format!("{url}/api/show"))
        .json(&serde_json::json!({ "model": model }))
        .send()
        .await
        .ok()?;
    let body = response.text().await.ok()?;
    role_from_capabilities(&body)
}

/// Read an Ollama `/api/show` reply. `None` when it lists no capabilities.
pub(crate) fn role_from_capabilities(body: &str) -> Option<ModelRole> {
    #[derive(Deserialize)]
    struct Show {
        capabilities: Vec<String>,
    }

    let show: Show = serde_json::from_str(body).ok()?;
    Some(if show.capabilities.iter().any(|c| c == "decision") {
        ModelRole::Decision
    } else {
        ModelRole::Chat
    })
}

async fn get_text(client: &reqwest::Client, url: &str) -> Option<String> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().await.ok()
}

#[derive(Deserialize)]
struct ModelsReply {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    architecture: Option<Architecture>,
    #[serde(default)]
    status: Option<ModelStatus>,
}

#[derive(Deserialize)]
struct Architecture {
    #[serde(default)]
    output_modalities: Vec<String>,
}

#[derive(Deserialize)]
struct ModelStatus {
    value: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        FoundModel, LlamaProcess, ModelRole, candidate_urls, llama_processes,
        parse_models, role_from_capabilities,
    };

    const PS: &str = "  PID  PPID ARGS
  733     1 /opt/homebrew/opt/ollama/bin/ollama serve
 1095 70031 /Users/me/.llama-app/llama serve --host 127.0.0.1 --port 51729 --tags decision
70031 69860 /Users/me/.llama-app/llama serve --models-preset /Users/me/Library/Application Support/Llama/models.ini --port 9931 --models-max 1
 4242     1 /opt/homebrew/bin/llama-server -hf ggml-org/Kev-4B-GGUF:Q8_0 --port=8081
 4243     1 /opt/homebrew/bin/llama-server -m model.gguf
 5000     1 vim llama-server.md
";

    #[test]
    fn given_ps_output_when_read_then_llama_servers_without_children_are_found()
    {
        assert_eq!(
            llama_processes(PS),
            vec![
                LlamaProcess {
                    port: 9931,
                    models_max: Some(1),
                },
                LlamaProcess {
                    port: 8081,
                    models_max: None,
                },
                LlamaProcess {
                    port: 8080,
                    models_max: None,
                },
            ]
        );
    }

    #[test]
    fn given_config_and_processes_then_urls_are_listed_once_in_order() {
        let processes = llama_processes(PS);
        let configured = vec!["http://localhost:9931/".to_owned()];

        assert_eq!(
            candidate_urls(&configured, &processes),
            [
                "http://127.0.0.1:9931",
                "http://127.0.0.1:8081",
                "http://127.0.0.1:8080",
                "http://127.0.0.1:1234",
                "http://127.0.0.1:11434",
            ]
        );
    }

    #[test]
    fn given_a_llama_router_reply_then_roles_and_load_state_are_read() {
        let body = json!({ "data": [
            {
                "id": "ggml-org/Kev-4B-GGUF:Q8_0", "owned_by": "llamacpp",
                "tags": ["decision"], "status": { "value": "unloaded" },
                "architecture": { "output_modalities": ["decisions"] },
            },
            {
                "id": "ggml-org/Qwen3.8-27B-GGUF:Q8_0", "owned_by": "llamacpp",
                "tags": ["chat"], "status": { "value": "loaded" },
            },
        ]})
        .to_string();

        assert_eq!(
            parse_models(&body),
            Some((
                vec![
                    FoundModel {
                        id: "ggml-org/Kev-4B-GGUF:Q8_0".into(),
                        role: ModelRole::Decision,
                        loaded: Some(false),
                    },
                    FoundModel {
                        id: "ggml-org/Qwen3.8-27B-GGUF:Q8_0".into(),
                        role: ModelRole::Chat,
                        loaded: Some(true),
                    },
                ],
                true
            ))
        );
    }

    #[test]
    fn given_an_ollama_reply_then_models_that_cannot_chat_are_left_out() {
        let body = json!({ "object": "list", "data": [
            { "id": "qwen2.5-coder:latest", "owned_by": "library" },
            { "id": "nomic-embed-text:latest", "owned_by": "library" },
            { "id": "x/z-image-turbo:latest", "owned_by": "x" },
            { "id": "bge-reranker:latest", "owned_by": "library" },
        ]})
        .to_string();

        let (models, is_llama_cpp) = parse_models(&body).expect("models");

        assert!(!is_llama_cpp);
        assert_eq!(
            models,
            vec![FoundModel {
                id: "qwen2.5-coder:latest".into(),
                role: ModelRole::Chat,
                loaded: None,
            }]
        );
    }

    #[test]
    fn given_something_that_is_not_a_model_list_then_nothing_is_found() {
        for body in ["", "not json", r#"{"error":"nope"}"#] {
            assert_eq!(parse_models(body), None, "{body}");
        }
    }

    #[test]
    fn given_ollama_capabilities_then_decision_models_are_told_apart() {
        assert_eq!(
            role_from_capabilities(r#"{"capabilities":["decision"]}"#),
            Some(ModelRole::Decision)
        );
        assert_eq!(
            role_from_capabilities(
                r#"{"capabilities":["completion","vision"]}"#
            ),
            Some(ModelRole::Chat)
        );
        assert_eq!(role_from_capabilities(r#"{"error":"not found"}"#), None);
    }
}
