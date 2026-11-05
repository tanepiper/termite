//! `tmite setup`: find local model servers, pick a chat model and a
//! decision model, fetch or start what is missing, and write the config.

mod discover;
mod fix;
mod hub;
mod installed;
mod profile;
pub(crate) mod prompt;
mod widget;

use std::process::ExitCode;

use serde_json::json;
use termite::{
    CONFIG_FILE_NAME, Config, Home, PRIVATE_FILE_MODE, write_private,
};
use tokio::runtime::Runtime;

use self::discover::{FoundServer, ModelRole, ServerKind, discover};
use self::fix::fix_missing;
use self::prompt::Prompt;
use crate::output::Format;
use crate::shell::Shell;

/// Local models load on first use; give them time.
const GENERATION_TIMEOUT_SECS: u64 = 120;
const DECISION_TIMEOUT_SECS: u64 = 60;

/// The request used to try the setup out.
const TRY_REQUEST: &str = "? list the files in this folder";

/// A model the user can pick, with the server that offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    url: String,
    id: String,
    kind: ServerKind,
    loaded: bool,
    models_max: Option<u32>,
}

impl Choice {
    fn label(&self) -> String {
        let loaded = if self.loaded { ", loaded" } else { "" };
        format!(
            "{}  ({} at {}{loaded})",
            self.id,
            self.kind.name(),
            self.url
        )
    }
}

/// Run setup. Questions are asked on the terminal; without one, choices
/// take their default and every yes/no question is answered no, so nothing
/// is downloaded, started or written.
pub(crate) fn run() -> ExitCode {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        eprintln!("tmite: cannot start");
        return ExitCode::FAILURE;
    };
    let prompt = Prompt::new();

    let configured: Vec<String> = Config::load()
        .map(|config| {
            vec![
                config.model_url().to_owned(),
                config.decision_url().to_owned(),
            ]
        })
        .unwrap_or_default();

    println!("Looking for local model servers…");
    let mut servers = runtime.block_on(discover(&configured));
    print_servers(&servers);

    let Some(chat) = pick_or_fix(
        &runtime,
        &prompt,
        &mut servers,
        &configured,
        ModelRole::Chat,
    ) else {
        println!(
            "\nNo chat model, so setup stops here. Run `tmite setup` again once one is available."
        );
        return ExitCode::from(2);
    };
    let Some(decision) = pick_or_fix(
        &runtime,
        &prompt,
        &mut servers,
        &configured,
        ModelRole::Decision,
    ) else {
        println!(
            "\nNo decision model, so setup stops here. Run `tmite setup` again once one is available."
        );
        return ExitCode::from(2);
    };

    if let Some(warning) = swap_warning(&chat, &decision) {
        println!("\n{warning}");
    }

    match write_config(&prompt, &chat, &decision) {
        Ok(true) => {},
        Ok(false) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tmite: {message}");
            return ExitCode::FAILURE;
        },
    }

    if prompt.confirm(&format!("\nTry it now with `{TRY_REQUEST}`?"), true) {
        println!("(The first answer can be slow while the models load.)\n");
        let (text, _) = crate::run(Format::Human, TRY_REQUEST);
        print!("{text}");
    }

    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    widget::offer(&prompt, Shell::detect(), home.as_deref());
    profile::offer(&prompt, Shell::detect(), home.as_deref());
    ExitCode::SUCCESS
}

/// Let the user pick a model for `role`. When there is none, offer to fetch
/// or start one, then look again.
fn pick_or_fix(
    runtime: &Runtime,
    prompt: &Prompt,
    servers: &mut Vec<FoundServer>,
    configured: &[String],
    role: ModelRole,
) -> Option<Choice> {
    let what = match role {
        ModelRole::Chat => "chat model (writes the commands)",
        ModelRole::Decision => "decision model (ranks them and checks safety)",
    };

    let mut choices = choices_for(servers, role);
    if choices.is_empty() {
        println!("\nNo {what} found.");
        if !fix_missing(runtime, prompt, servers, role) {
            return None;
        }
        *servers = runtime.block_on(discover(configured));
        choices = choices_for(servers, role);
        if choices.is_empty() {
            println!("Still no {what}.");
            return None;
        }
    }

    let labels: Vec<String> = choices.iter().map(Choice::label).collect();
    let index = prompt.choose(&format!("\nPick the {what}:"), &labels, 0);
    choices.into_iter().nth(index)
}

/// The models for `role`, best guess first: models already loaded, then
/// llama.cpp, then the rest in the order found.
fn choices_for(servers: &[FoundServer], role: ModelRole) -> Vec<Choice> {
    let mut choices: Vec<Choice> = servers
        .iter()
        .flat_map(|server| {
            server
                .models
                .iter()
                .filter(move |model| model.role == role)
                .map(move |model| Choice {
                    url: server.url.clone(),
                    id: model.id.clone(),
                    kind: server.kind,
                    loaded: model.loaded == Some(true),
                    models_max: server.models_max,
                })
        })
        .collect();

    choices.sort_by_key(|choice| {
        (!choice.loaded, choice.kind != ServerKind::LlamaCpp)
    });
    choices
}

/// A router that keeps one model loaded swaps the two models on every
/// query, which is slow.
fn swap_warning(chat: &Choice, decision: &Choice) -> Option<String> {
    (chat.url == decision.url && chat.models_max == Some(1)).then(|| {
        format!(
            "Note: the server at {} keeps one model loaded at a time, so every query swaps \
             the chat and decision models and will be slow. Let it keep two loaded \
             (`--models-max 2`, or the Llama app's settings) if memory allows.",
            chat.url
        )
    })
}

/// The config file contents for the two picks.
fn config_json(chat: &Choice, decision: &Choice) -> String {
    let value = json!({
        "model_url": chat.url,
        "model": chat.id,
        "decision_url": decision.url,
        "decision_model": decision.id,
        "generation_timeout_secs": GENERATION_TIMEOUT_SECS,
        "decision_timeout_secs": DECISION_TIMEOUT_SECS,
    });

    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    )
}

/// Show the config and write it after asking. Returns whether it was
/// written.
fn write_config(
    prompt: &Prompt,
    chat: &Choice,
    decision: &Choice,
) -> Result<bool, String> {
    let home = Home::from_env().map_err(|error| error.to_string())?;
    let path = home.config_file();
    let text = config_json(chat, decision);
    Config::from_json(&text)
        .map_err(|error| format!("setup made a bad config: {error}"))?;

    println!("\n{text}");
    let verb = if path.exists() { "Replace" } else { "Write" };
    if !prompt.confirm(&format!("{verb} {}?", path.display()), true) {
        println!("Nothing written.");
        return Ok(false);
    }

    home.ensure()
        .and_then(|()| {
            write_private(
                home.root(),
                CONFIG_FILE_NAME,
                text.as_bytes(),
                PRIVATE_FILE_MODE,
            )
        })
        .map_err(|error| error.to_string())?;
    crate::provenance::record(
        &home,
        termite::Event::Installed,
        CONFIG_FILE_NAME,
        text.as_bytes(),
        Vec::new(),
        "tmite setup",
    )?;

    println!("Written.");
    Ok(true)
}

fn print_servers(servers: &[FoundServer]) {
    if servers.is_empty() {
        println!("  none running.");
        return;
    }

    for server in servers {
        println!("  {} at {}", server.kind.name(), server.url);
        for model in &server.models {
            let role = match model.role {
                ModelRole::Chat => "chat",
                ModelRole::Decision => "decision",
            };
            let loaded = if model.loaded == Some(true) {
                ", loaded"
            } else {
                ""
            };
            println!("    - {} ({role}{loaded})", model.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use termite::Config;

    use super::{Choice, choices_for, config_json, swap_warning};
    use crate::setup::discover::{
        FoundModel, FoundServer, ModelRole, ServerKind,
    };

    fn model(id: &str, role: ModelRole, loaded: Option<bool>) -> FoundModel {
        FoundModel {
            id: id.to_owned(),
            role,
            loaded,
        }
    }

    fn servers() -> Vec<FoundServer> {
        vec![
            FoundServer {
                kind: ServerKind::Ollama,
                url: "http://127.0.0.1:11434".to_owned(),
                models: vec![model("qwen2.5-coder:7b", ModelRole::Chat, None)],
                models_max: None,
            },
            FoundServer {
                kind: ServerKind::LlamaCpp,
                url: "http://127.0.0.1:9931".to_owned(),
                models: vec![
                    model("Qwen-27B", ModelRole::Chat, Some(false)),
                    model("Kev-4B", ModelRole::Decision, Some(true)),
                    model("Coder-7B", ModelRole::Chat, Some(true)),
                ],
                models_max: Some(1),
            },
        ]
    }

    fn ids(choices: &[Choice]) -> Vec<&str> {
        choices.iter().map(|choice| choice.id.as_str()).collect()
    }

    #[test]
    fn given_servers_when_choices_are_listed_then_loaded_and_llama_cpp_come_first()
     {
        let servers = servers();

        assert_eq!(
            ids(&choices_for(&servers, ModelRole::Chat)),
            ["Coder-7B", "Qwen-27B", "qwen2.5-coder:7b"]
        );
        assert_eq!(
            ids(&choices_for(&servers, ModelRole::Decision)),
            ["Kev-4B"]
        );
    }

    #[test]
    fn given_one_router_keeping_one_model_when_checked_then_it_warns() {
        let servers = servers();
        let chat = choices_for(&servers, ModelRole::Chat);
        let decision = choices_for(&servers, ModelRole::Decision);

        assert!(swap_warning(&chat[0], &decision[0]).is_some());
        assert!(swap_warning(&chat[2], &decision[0]).is_none());
    }

    #[test]
    fn given_two_picks_when_the_config_is_made_then_it_loads() {
        let servers = servers();
        let chat = &choices_for(&servers, ModelRole::Chat)[2];
        let decision = &choices_for(&servers, ModelRole::Decision)[0];

        let config = Config::from_json(&config_json(chat, decision))
            .expect("setup writes a valid config");

        assert_eq!(config.model_url(), "http://127.0.0.1:11434");
        assert_eq!(config.model(), "qwen2.5-coder:7b");
        assert_eq!(config.decision_url(), "http://127.0.0.1:9931");
        assert_eq!(config.decision_model(), "Kev-4B");
        assert_eq!(config.generation_timeout().as_secs(), 120);
    }
}
