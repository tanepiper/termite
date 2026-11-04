//! `tmite setup`: find local model servers, pick a chat model and a
//! decision model, fetch or start what is missing, and write the config.

mod discover;
mod hub;

use std::cell::RefCell;
use std::io::{self, BufRead, IsTerminal, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use termite::{
    CONFIG_FILE_NAME, Config, Home, PRIVATE_FILE_MODE, create_private,
    write_private,
};
use tokio::runtime::Runtime;

use self::discover::{
    FoundServer, ModelRole, ServerKind, discover, find_on_path, installed,
};
use self::hub::{
    CHAT_MODEL, DECISION_MODEL, HubModel, OLLAMA_CHAT_MODEL, download_command,
    human_size, serve_command,
};
use crate::output::Format;

/// Local models load on first use; give them time.
const GENERATION_TIMEOUT_SECS: u64 = 120;
const DECISION_TIMEOUT_SECS: u64 = 60;

/// How long to wait for a server that setup started (it may download
/// several gigabytes first).
const START_TIMEOUT: Duration = Duration::from_secs(15 * 60);

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

    println!("\nTo use it at the zsh prompt, add this to ~/.zshrc:");
    println!("  source /path/to/termite/shell/tmite.zsh");
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

/// Offer the fixes that fit what is installed and running. Returns whether
/// something was fetched or started.
fn fix_missing(
    runtime: &Runtime,
    prompt: &Prompt,
    servers: &[FoundServer],
    role: ModelRole,
) -> bool {
    let model = match role {
        ModelRole::Chat => CHAT_MODEL,
        ModelRole::Decision => DECISION_MODEL,
    };
    let ollama_running = servers
        .iter()
        .any(|server| server.kind == ServerKind::Ollama);
    let llama_router = servers.iter().any(|server| {
        server.kind == ServerKind::LlamaCpp && server.models_max.is_some()
    });
    let downloader =
        find_on_path("hf").or_else(|| find_on_path("huggingface-cli"));
    let llama_server = find_on_path("llama-server");

    for app in installed(servers) {
        println!(
            "  {} is installed but not running: {}",
            app.kind.name(),
            app.start
        );
    }

    if let Some(done) = offer_ollama_pull(prompt, role, ollama_running) {
        return done;
    }

    let files = runtime.block_on(hub::files(&model));
    let size = files
        .as_ref()
        .map(|files| human_size(files.iter().map(|file| file.size).sum()))
        .unwrap_or_else(|| "size unknown".to_owned());

    if let (true, Some(downloader)) = (llama_router, &downloader) {
        let command = download_command(downloader, &model, files.as_deref());
        if let Some(done) = offer_download(prompt, &model, &command, &size) {
            return done;
        }
    }

    if let Some(llama_server) = &llama_server {
        let port = free_port(servers, role);
        let command = serve_command(llama_server, &model, port);
        if let Some(done) =
            offer_llama_server(runtime, prompt, &model, &command, port, &size)
        {
            return done;
        }
    }

    print_install_advice(
        role,
        llama_server.is_some(),
        downloader.is_some(),
        ollama_running,
    );
    false
}

/// Say what to install when no fix could be offered.
fn print_install_advice(
    role: ModelRole,
    has_llama_server: bool,
    has_downloader: bool,
    ollama_running: bool,
) {
    if !(has_llama_server || has_downloader || ollama_running) {
        println!(
            "  Install llama.cpp (`brew install llama.cpp`), then run `tmite setup` again."
        );
    }
    if role == ModelRole::Decision && !has_llama_server {
        println!(
            "  Decision models need llama.cpp, which serves /v1/systemone."
        );
    }
}

/// Offer `ollama pull` for the chat model, when Ollama is running. `None`
/// when it does not apply or the user says no.
fn offer_ollama_pull(
    prompt: &Prompt,
    role: ModelRole,
    ollama_running: bool,
) -> Option<bool> {
    if role != ModelRole::Chat || !ollama_running {
        return None;
    }
    let question =
        format!("Pull {OLLAMA_CHAT_MODEL} with Ollama (about 4.7 GB)?");
    prompt
        .confirm(&question, false)
        .then(|| run_visible(&["ollama", "pull", OLLAMA_CHAT_MODEL]))
}

/// Offer a Hugging Face download into the cache the Llama app reads.
/// `None` when the user says no or the download fails, so the next fix is
/// offered; `Some(false)` after a download, since the app must restart.
fn offer_download(
    prompt: &Prompt,
    model: &HubModel,
    command: &[String],
    size: &str,
) -> Option<bool> {
    let question = format!(
        "Download {} ({size}) from Hugging Face into its cache, for the Llama app?\n  {}",
        model.about,
        command.join(" ")
    );
    if !(prompt.confirm(&question, false) && run_visible(command)) {
        return None;
    }
    println!(
        "Downloaded. Restart the Llama app so it lists {}, then run `tmite setup` again.",
        model.id()
    );
    Some(false)
}

/// Offer to start llama-server with the model. `None` when the user says no.
fn offer_llama_server(
    runtime: &Runtime,
    prompt: &Prompt,
    model: &HubModel,
    command: &[String],
    port: u16,
    size: &str,
) -> Option<bool> {
    let question = format!(
        "Start local inference: run llama-server with {} on port {port} (downloads {size} the first time)?\n  {}",
        model.about,
        command.join(" ")
    );
    prompt
        .confirm(&question, false)
        .then(|| start_server(runtime, command, port))
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

/// Run a command with the terminal attached, so its progress shows.
fn run_visible<S: AsRef<str>>(command: &[S]) -> bool {
    let Some((program, args)) = command.split_first() else {
        return false;
    };

    match Command::new(program.as_ref())
        .args(args.iter().map(AsRef::as_ref))
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("tmite: {} failed ({status})", program.as_ref());
            false
        },
        Err(error) => {
            eprintln!("tmite: cannot run {}: {error}", program.as_ref());
            false
        },
    }
}

/// Start a server in the background, detached from this terminal, with
/// its output in a log file, and wait until it answers.
fn start_server(runtime: &Runtime, command: &[String], port: u16) -> bool {
    let Some((program, args)) = command.split_first() else {
        return false;
    };
    let log_name = format!("llama-server-{port}.log");
    let Some(log_folder) = log_folder() else {
        eprintln!(
            "tmite: HOME is not set, so there is nowhere to keep the server log"
        );
        return false;
    };
    let log_path = log_folder.join(&log_name);
    let log = match create_private(&log_folder, &log_name) {
        Ok(log) => log,
        Err(error) => {
            eprintln!("tmite: {error}");
            return false;
        },
    };
    let Ok(log_err) = log.try_clone() else {
        return false;
    };

    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_err)
        .process_group(0)
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(error) => {
            eprintln!("tmite: cannot start {program}: {error}");
            return false;
        },
    };

    println!(
        "Started (process {}). Log: {}\nWaiting for it to download and load the model; Ctrl C stops waiting, not the server.",
        child.id(),
        log_path.display()
    );
    let ready = runtime.block_on(wait_until_ready(port));
    if ready {
        println!(
            "Ready. It keeps running after setup; stop it with `kill {}`. It does not start again after a restart.",
            child.id()
        );
    } else {
        println!(
            "It did not answer in time. Check the log, then run `tmite setup` again."
        );
    }
    ready
}

async fn wait_until_ready(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/v1/models");
    let started = Instant::now();
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };

    while started.elapsed() < START_TIMEOUT {
        let answered = client
            .get(&url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if answered {
            println!();
            return true;
        }
        print!(".");
        let _ = io::stdout().flush();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    println!();
    false
}

/// 8080 for chat and 8081 for decisions, unless a found server uses it.
fn free_port(servers: &[FoundServer], role: ModelRole) -> u16 {
    let first = match role {
        ModelRole::Chat => 8080,
        ModelRole::Decision => 8081,
    };

    (first..first + 20)
        .find(|port| {
            !servers
                .iter()
                .any(|server| server.url.ends_with(&format!(":{port}")))
        })
        .unwrap_or(first)
}

/// Where server logs go: `~/.cache/termite`. `None` when `HOME` is not
/// set, rather than falling back to the current or a shared folder.
fn log_folder() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".cache/termite"))
}

/// Questions on the terminal. Without a terminal, choices take their
/// default and yes/no questions are answered no.
struct Prompt {
    interactive: bool,
    /// Where answers come from: stdin, or fixed text in tests.
    input: RefCell<Box<dyn BufRead>>,
}

impl Prompt {
    fn new() -> Self {
        Self {
            interactive: io::stdin().is_terminal(),
            input: RefCell::new(Box::new(io::stdin().lock())),
        }
    }

    /// A prompt that reads its answers from `answers`, as if typed.
    #[cfg(test)]
    fn answering(answers: &'static str) -> Self {
        Self {
            interactive: true,
            input: RefCell::new(Box::new(answers.as_bytes())),
        }
    }

    /// A prompt with no terminal: every question gets its default "no".
    #[cfg(test)]
    fn without_terminal() -> Self {
        Self {
            interactive: false,
            input: RefCell::new(Box::new(io::empty())),
        }
    }

    fn answer(&self, question: &str) -> Option<String> {
        print!("{question} ");
        let _ = io::stdout().flush();
        if !self.interactive {
            println!();
            return None;
        }

        let mut line = String::new();
        match self.input.borrow_mut().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_owned()),
        }
    }

    fn confirm(&self, question: &str, default: bool) -> bool {
        let hint = if default { "[Y/n]" } else { "[y/N]" };
        let answer = self.answer(&format!("{question} {hint}"));
        if !self.interactive {
            return false;
        }
        match answer {
            Some(answer)
                if answer.eq_ignore_ascii_case("y")
                    || answer.eq_ignore_ascii_case("yes") =>
            {
                true
            },
            Some(answer)
                if answer.eq_ignore_ascii_case("n")
                    || answer.eq_ignore_ascii_case("no") =>
            {
                false
            },
            _ => default,
        }
    }

    fn choose(&self, title: &str, options: &[String], default: usize) -> usize {
        println!("{title}");
        for (index, option) in options.iter().enumerate() {
            let marker = if index == default { " (default)" } else { "" };
            println!("  {}. {option}{marker}", index + 1);
        }

        loop {
            let Some(answer) =
                self.answer(&format!("Number [{}]:", default + 1))
            else {
                return default;
            };
            if answer.is_empty() {
                return default;
            }
            match answer.parse::<usize>() {
                Ok(number) if (1..=options.len()).contains(&number) => {
                    return number - 1;
                },
                _ => println!("Type a number from 1 to {}.", options.len()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use termite::Config;

    use super::{
        CHAT_MODEL, Choice, DECISION_MODEL, Prompt, choices_for, config_json,
        free_port, offer_download, offer_llama_server, offer_ollama_pull,
        swap_warning,
    };
    use crate::setup::discover::{
        FoundModel, FoundServer, ModelRole, ServerKind,
    };

    /// Without a terminal every offer is a no: nothing is pulled,
    /// downloaded or started, and the next fix is tried.
    #[test]
    fn offers_do_nothing_without_a_terminal() {
        let prompt = Prompt::without_terminal();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let command = vec!["/nonexistent/should-not-run".to_owned()];

        assert_eq!(offer_ollama_pull(&prompt, ModelRole::Chat, true), None);
        assert_eq!(
            offer_download(&prompt, &CHAT_MODEL, &command, "1 GB"),
            None
        );
        assert_eq!(
            offer_llama_server(
                &runtime,
                &prompt,
                &DECISION_MODEL,
                &command,
                8081,
                "1 GB"
            ),
            None
        );
    }

    /// A yes runs the download, shown in full, and then asks for a restart
    /// of the Llama app rather than carrying on.
    #[test]
    fn a_yes_runs_the_download() {
        let prompt = Prompt::answering("y\n");
        let command = vec!["true".to_owned()];
        assert_eq!(
            offer_download(&prompt, &CHAT_MODEL, &command, "1 GB"),
            Some(false)
        );
    }

    /// A failed download falls through to the next fix.
    #[test]
    fn a_failed_download_tries_the_next_fix() {
        let prompt = Prompt::answering("yes\n");
        let command = vec!["false".to_owned()];
        assert_eq!(
            offer_download(&prompt, &CHAT_MODEL, &command, "1 GB"),
            None
        );
    }

    #[test]
    fn confirm_reads_yes_no_and_default() {
        assert!(Prompt::answering("Y\n").confirm("q", false));
        assert!(!Prompt::answering("no\n").confirm("q", true));
        assert!(Prompt::answering("\n").confirm("q", true));
        assert!(!Prompt::answering("maybe\n").confirm("q", false));
        assert!(!Prompt::answering("").confirm("q", false));
    }

    #[test]
    fn choose_asks_again_until_a_number_fits() {
        let options = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(Prompt::answering("9\nx\n2\n").choose("t", &options, 0), 1);
        assert_eq!(Prompt::answering("\n").choose("t", &options, 1), 1);
        assert_eq!(Prompt::without_terminal().choose("t", &options, 0), 0);
    }

    #[test]
    fn ollama_is_offered_only_for_chat_when_running() {
        let prompt = Prompt::without_terminal();
        assert_eq!(offer_ollama_pull(&prompt, ModelRole::Decision, true), None);
        assert_eq!(offer_ollama_pull(&prompt, ModelRole::Chat, false), None);
    }

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

    #[test]
    fn given_a_port_in_use_when_a_port_is_picked_then_the_next_is_used() {
        let mut servers = servers();
        assert_eq!(free_port(&servers, ModelRole::Chat), 8080);
        assert_eq!(free_port(&servers, ModelRole::Decision), 8081);

        servers[0].url = "http://127.0.0.1:8081".to_owned();
        assert_eq!(free_port(&servers, ModelRole::Decision), 8082);
    }
}
