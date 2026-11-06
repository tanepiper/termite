//! Fetching or starting what setup found missing. Contract: every command
//! is printed in full and runs only after the user says yes.

use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use termite::create_private;
use tokio::runtime::Runtime;

use super::discover::{FoundServer, ModelRole, ServerKind};
use super::fetch::{HUB, fetch};
use super::hub::{HubModel, download_command, human_size, serve_command};
use super::installed::{find_on_path, installed};
use super::machine::Machine;
use super::prompt::Prompt;

/// Offer the fixes that fit what is installed and running. Returns whether
/// something was fetched or started.
pub(super) fn fix_missing(
    runtime: &Runtime,
    prompt: &Prompt,
    servers: &[FoundServer],
    role: ModelRole,
    model: Option<&HubModel>,
    machine: &Machine,
) -> bool {
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

    let Some(model) = model else {
        println!("  There is no model on the list to offer.");
        return false;
    };
    let size = human_size(model.size);

    if let (true, Some(downloader)) = (llama_router, &downloader) {
        let command = download_command(downloader, model);
        if let Some(done) = offer_download(prompt, model, &command, &size) {
            return done;
        }
    }

    if let Some(llama_server) = &llama_server {
        let port = free_port(servers, role);
        if let Some(done) =
            offer_llama_server(runtime, prompt, model, llama_server, port, HUB)
        {
            return done;
        }
    }

    print_install_advice(role, llama_server.is_some(), machine);
    false
}

/// Say what to install when no fix could be offered.
pub(super) fn print_install_advice(
    role: ModelRole,
    has_llama_server: bool,
    machine: &Machine,
) {
    if !has_llama_server {
        println!("  {}", machine.llama_advice());
    }
    if role == ModelRole::Decision && !has_llama_server {
        println!(
            "  Decision models need llama.cpp, which serves /v1/systemone."
        );
    }
}

/// Offer a Hugging Face download into the cache the Llama app reads.
/// `None` when the user says no or the download fails, so the next fix is
/// offered; `Some(false)` after a download, since the app must restart.
pub(super) fn offer_download(
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
        model.about
    );
    Some(false)
}

/// Offer to fetch the model's pinned file and start llama-server with it.
/// `None` when the user says no or the file can't be fetched.
pub(super) fn offer_llama_server(
    runtime: &Runtime,
    prompt: &Prompt,
    model: &HubModel,
    llama_server: &std::path::Path,
    port: u16,
    base: &str,
) -> Option<bool> {
    let folder = model_folder()?;
    let path = super::fetch::file_path(&folder, model);
    let command = serve_command(llama_server, model, &path, port);
    let question = format!(
        "Fetch {} ({}) from {}\n  at revision {}\n  into {}\nand start local inference on port {port}?\n  {}",
        model.about,
        human_size(model.size),
        model.repo,
        model.revision,
        folder.display(),
        command.join(" ")
    );
    if !prompt.confirm(&question, false) {
        return None;
    }
    if let Err(problem) = runtime.block_on(fetch(model, &folder, base)) {
        println!("Couldn't fetch {}: {problem}", model.file);
        return None;
    }
    Some(start_server(runtime, &command, port))
}

/// Run a command with the terminal attached, so its progress shows.
pub(super) fn run_visible<S: AsRef<str>>(command: &[S]) -> bool {
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
pub(super) fn start_server(
    runtime: &Runtime,
    command: &[String],
    port: u16,
) -> bool {
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

pub(super) async fn wait_until_ready(port: u16) -> bool {
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
pub(super) fn free_port(servers: &[FoundServer], role: ModelRole) -> u16 {
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

/// Where model files go.
pub(super) fn model_folder() -> Option<PathBuf> {
    log_folder().map(|folder| folder.join("models"))
}

/// Where server logs go: `~/.cache/termite`. `None` when `HOME` is not
/// set, rather than falling back to the current or a shared folder.
pub(super) fn log_folder() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".cache/termite"))
}

/// How long to wait for a server that setup started (it may download
/// several gigabytes first).
pub(super) const START_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::discover::{FoundServer, ModelRole, ServerKind};
    use super::super::hub::{HubModel, catalog};
    use super::super::machine::{Gpu, Machine};
    use super::super::prompt::Prompt;
    use super::{
        fix_missing, free_port, offer_download, offer_llama_server,
        print_install_advice, run_visible,
    };

    fn model() -> HubModel {
        catalog().remove(0)
    }

    fn machine() -> Machine {
        Machine {
            os: "linux".to_owned(),
            arch: "x86_64".to_owned(),
            memory: 0,
            gpu: Gpu::None,
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    /// Port 1 refuses at once, so a fetch that was wrongly started fails
    /// loudly rather than downloading anything.
    const NOWHERE: &str = "http://127.0.0.1:1";

    /// Without a terminal every offer is a no: nothing is downloaded or
    /// started, and the next fix is tried.
    #[test]
    fn offers_do_nothing_without_a_terminal() {
        let prompt = Prompt::without_terminal();
        let command = vec!["/nonexistent/should-not-run".to_owned()];

        assert_eq!(offer_download(&prompt, &model(), &command, "1 GB"), None);
        assert_eq!(
            offer_llama_server(
                &runtime(),
                &prompt,
                &model(),
                Path::new("/nonexistent/should-not-run"),
                8081,
                NOWHERE,
            ),
            None
        );
    }

    /// Only a typed y counts. Pressing Enter is a no.
    #[test]
    fn enter_alone_or_no_starts_nothing() {
        for answer in ["\n", "n\n", "no\n", "maybe\n", ""] {
            let prompt = Prompt::answering(answer);
            let command = vec!["/nonexistent/should-not-run".to_owned()];

            assert_eq!(
                offer_download(&prompt, &model(), &command, "1 GB"),
                None
            );
        }
    }

    /// A y is what starts the fetch: here it fails at once, on purpose.
    #[test]
    fn a_y_tries_the_pinned_fetch_and_a_failure_tries_the_next_fix() {
        let prompt = Prompt::answering("y\n");
        assert_eq!(
            offer_llama_server(
                &runtime(),
                &prompt,
                &model(),
                Path::new("/nonexistent/should-not-run"),
                8081,
                NOWHERE,
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
            offer_download(&prompt, &model(), &command, "1 GB"),
            Some(false)
        );
    }

    /// A failed download falls through to the next fix.
    #[test]
    fn a_failed_download_tries_the_next_fix() {
        let prompt = Prompt::answering("yes\n");
        let command = vec!["false".to_owned()];
        assert_eq!(offer_download(&prompt, &model(), &command, "1 GB"), None);
    }

    /// With nothing running and no terminal, setup finds nothing to do
    /// and starts nothing.
    #[test]
    fn given_no_terminal_then_no_fix_is_made() {
        let prompt = Prompt::without_terminal();
        let ollama = FoundServer {
            kind: ServerKind::Ollama,
            url: "http://127.0.0.1:11434".to_owned(),
            models: Vec::new(),
            models_max: None,
        };

        assert!(!fix_missing(
            &runtime(),
            &prompt,
            &[ollama],
            ModelRole::Chat,
            Some(&model()),
            &machine(),
        ));
    }

    #[test]
    fn given_commands_then_success_and_failure_are_told_apart() {
        assert!(run_visible(&["true"]));
        assert!(!run_visible(&["false"]));
        assert!(!run_visible(&["/nonexistent/program"]));
        assert!(!run_visible::<&str>(&[]));
    }

    #[test]
    fn given_any_situation_then_install_advice_is_safe_to_print() {
        for role in [ModelRole::Chat, ModelRole::Decision] {
            for has in [true, false] {
                print_install_advice(role, has, &machine());
            }
        }
    }

    #[test]
    fn given_a_port_in_use_when_a_port_is_picked_then_the_next_is_used() {
        let mut servers = vec![FoundServer {
            kind: ServerKind::Ollama,
            url: "http://127.0.0.1:11434".to_owned(),
            models: Vec::new(),
            models_max: None,
        }];
        assert_eq!(free_port(&servers, ModelRole::Chat), 8080);
        assert_eq!(free_port(&servers, ModelRole::Decision), 8081);

        servers[0].url = "http://127.0.0.1:8081".to_owned();
        assert_eq!(free_port(&servers, ModelRole::Decision), 8082);
    }
}
