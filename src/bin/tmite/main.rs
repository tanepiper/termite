//! `tmite`: ask a local model for a shell command.
//!
//! ```text
//! tmite [--json | --zle] '<? | ?? | ?x> request'
//! tmite check [--json] [--all] [FILE...]
//! tmite script [--json] 'request'
//! tmite scripts list|show|install|remove
//! tmite setup
//! tmite shell [zsh|bash|fish]
//! ```
//!
//! `tmite` only prints, and termite never runs anything. The optional widgets
//! in `shell/` put a command at the prompt, and pressing Enter is up to you.
//! That is the last safety gate.

mod check;
mod library;
mod output;
mod progress;
mod provenance;
mod provenance_key;
mod script_tools;
mod scripts;
mod setup;
mod shell;

use std::env;
use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::ExitCode;

use termite::{
    Answer, Config, Context, Home, LlamaServer, Session, SystemOne, answer,
    decision_token, parse_query, script_request,
};

use crate::output::{Format, exit_code, render, render_failure, view_of};
use crate::shell::Shell;

const USAGE: &str = "\
Usage: tmite [--json | --zle] '<prefix> request'   prefix: ? ?? ?x ?b
       tmite check [--json] [--all] [FILE...]   rate a script or a pipe
       tmite script [--json] 'request'          write a script as a draft
       tmite scripts list|show|install|remove   manage your scripts
       tmite provenance verify|PATH|accept PATH what termite installed
       tmite alerts                             what its guards stopped
       tmite setup   find local models and write the config
       tmite shell [zsh|bash|fish]   print the optional prompt widget

Prefixes:
  ?   explains one command; say yes to put it at your prompt
  ??  explains every command, best first; pick one, ask again or cancel
  ?x  puts one command straight at your prompt, if every check passes

Nothing is ever run for you. Enter at the prompt is yours to press.

Options:
  --json  print one JSON object, for scripts
  --zle   print the line format read by shell/tmite.zsh, with progress
  -h, --help  show this help

Config: ~/.config/termite/config.json. Token: TERMITE_DECISION_TOKEN.
Exit status: 0 a command to use, 2 none or refused, 1 failed, 64 usage.

tmite check reads files, or a pipe if there are none, and rates each command
as it arrives. Piping into tmite with no request does the same. No model is used, and nothing is run. --all shows every command;
--json prints one JSON object per command. Exit status: 0 nothing risky found,
2 risky or no commands found, 1 failed, 64 usage.

tmite script saves a draft, which isn't executable. tmite scripts install shows
it, asks you, then installs it. termite never runs a script.
";

fn main() -> ExitCode {
    // The program path (argument 0) is skipped and never trusted; the rest
    // are the user's words, parsed and checked like any other input.
    // nosemgrep: rust.lang.security.args.args
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.first().is_some_and(|first| first == "setup") {
        if arguments.len() > 1 {
            eprint!("tmite: setup takes no arguments\n\n{USAGE}");
            return ExitCode::from(64);
        }
        return setup::run();
    }

    if arguments.first().is_some_and(|first| first == "check") {
        return check_input(&arguments[1..]);
    }

    // Piped into with no request: `curl … | tmite` means check this.
    let no_request = arguments
        .iter()
        .all(|word| word == "--json" || word == "--all");
    if no_request && piped_in() {
        return check_input(&arguments);
    }

    if arguments.first().is_some_and(|first| first == "script") {
        return scripts::write(&arguments[1..]);
    }

    if arguments.first().is_some_and(|first| first == "scripts") {
        return scripts::manage(&arguments[1..]);
    }

    if arguments.first().is_some_and(|first| first == "provenance") {
        return provenance::manage(&arguments[1..]);
    }

    if arguments.first().is_some_and(|first| first == "alerts") {
        return provenance::alerts();
    }

    if arguments.first().is_some_and(|first| first == "shell") {
        return print_widget(arguments.get(1).map(String::as_str));
    }

    let (format, input) = match read_arguments(&arguments) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        },
        Err(message) => {
            eprint!("tmite: {message}\n\n{USAGE}");
            return ExitCode::from(64);
        },
    };

    let (text, code) = run(format, &input);

    let mut stdout = io::stdout().lock();
    if stdout.write_all(text.as_bytes()).is_err() {
        return ExitCode::FAILURE;
    }

    ExitCode::from(code)
}

/// Is something being fed in? A pipe, a socket or a redirected file counts.
/// A terminal doesn't, and neither does `/dev/null`, which is what a background
/// job gets for nothing.
fn piped_in() -> bool {
    let stdin = io::stdin();
    let Ok(handle) = stdin.as_fd().try_clone_to_owned() else {
        return false;
    };
    File::from(handle).metadata().is_ok_and(|metadata| {
        let kind = metadata.file_type();
        kind.is_fifo() || kind.is_socket() || kind.is_file()
    })
}

/// `tmite check`: everything that isn't an option is a file.
fn check_input(arguments: &[String]) -> ExitCode {
    let mut style = check::Style {
        json: false,
        colour: wants_colour(Format::Human),
        all: false,
    };
    let mut files = Vec::new();
    let mut options_done = false;
    for word in arguments {
        match word.as_str() {
            _ if options_done => files.push(word.clone()),
            "--" => options_done = true,
            "--json" => style.json = true,
            "--all" => style.all = true,
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            },
            "-" => files.push(word.clone()),
            option if option.starts_with('-') => {
                eprint!("tmite: unknown option {option}\n\n{USAGE}");
                return ExitCode::from(64);
            },
            _ => files.push(word.clone()),
        }
    }
    ExitCode::from(check::run(&files, style))
}

/// Print the widget for `name`, or for the shell in use.
fn print_widget(name: Option<&str>) -> ExitCode {
    let shell = match name {
        Some(name) => Shell::from_name(name),
        None => Shell::detect(),
    };
    let Some(shell) = shell else {
        eprint!("tmite: there's a widget for zsh, bash and fish\n\n{USAGE}");
        return ExitCode::from(64);
    };
    print!("{}", shell.widget());
    ExitCode::SUCCESS
}

/// Parse options, then join the remaining words into the query. Returns
/// `Ok(None)` for `--help`.
fn read_arguments(
    arguments: &[String],
) -> Result<Option<(Format, String)>, String> {
    let mut format = Format::Human;
    let mut rest = arguments;

    while let Some((first, tail)) = rest.split_first() {
        match first.as_str() {
            "--json" => format = Format::Json,
            "--zle" => format = Format::Zle,
            "-h" | "--help" => return Ok(None),
            "--" => {
                rest = tail;
                break;
            },
            option if option.starts_with("--") => {
                return Err(format!("unknown option {option}"));
            },
            _ => break,
        }
        rest = tail;
    }

    if rest.is_empty() {
        return Err("missing request".to_owned());
    }

    Ok(Some((format, rest.join(" "))))
}

pub(crate) fn run(format: Format, input: &str) -> (String, u8) {
    if let Some(request) = script_request(input) {
        return scripts::answer(request, format);
    }

    let Some(query) = parse_query(input) else {
        let message = "start the request with ?, ??, ?x or ?b";
        return (render_failure(message, format), 1);
    };

    if let Some(message) = config_problem() {
        return (render_failure(&message, format), 1);
    }

    let clients = Config::load().and_then(|config| {
        let source = LlamaServer::new(&config)?;
        let decider = SystemOne::new(&config, decision_token())?;
        Ok((source, decider))
    });
    let (source, decider) = match clients {
        Ok(clients) => clients,
        Err(error) => return (render_failure(&error.to_string(), format), 1),
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let message = format!("cannot start: {error}");
            return (render_failure(&message, format), 1);
        },
    };

    let context = context_from_environment();
    let answer = progress::watching(format, |progress| {
        runtime.block_on(answer(&query, &context, &source, &decider, progress))
    });

    if matches!(answer, Answer::Refused { .. }) {
        // The command is already stopped. `tmite alerts` shows what this logs.
        let _ = provenance::alert(input, Vec::new(), "refused", "tmite");
    }

    let view = view_of(Path::new(context.cwd()), wants_colour(format));
    (render(&answer, format, &view), exit_code(&answer))
}

/// Why the config can't be used, if the log says it has changed.
pub(crate) fn config_problem() -> Option<String> {
    provenance::config_problem(&Home::from_env().ok()?)
}

/// Colour goes to a terminal, or to the widget that prints for one. It is
/// never sent to a pipe, and `NO_COLOR` and a dumb terminal turn it off.
fn wants_colour(format: Format) -> bool {
    let terminal = match format {
        Format::Human => io::stdout().is_terminal(),
        Format::Zle => true,
        Format::Json => false,
    };
    let dumb = env::var("TERM").is_ok_and(|term| term == "dumb");
    terminal && !dumb && env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// `tmite` runs in the user's own shell, which is waiting for it to finish.
/// An SSH login is detected from the variables sshd sets.
fn context_from_environment() -> Context {
    let shell = env::var("SHELL")
        .ok()
        .and_then(|shell| {
            Path::new(&shell)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "sh".to_owned());
    let cwd = env::current_dir()
        .map(|cwd| cwd.display().to_string())
        .unwrap_or_else(|_| ".".to_owned());

    Context::new(env::consts::OS, shell, cwd, session_from_environment())
}

/// [`Session::Ssh`] when sshd started this login (`SSH_CONNECTION`,
/// `SSH_CLIENT` or `SSH_TTY` is set and not empty).
fn session_from_environment() -> Session {
    session_from(|name| env::var_os(name))
}

fn session_from(
    variable: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Session {
    let over_ssh = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| variable(name).is_some_and(|value| !value.is_empty()));
    if over_ssh {
        Session::Ssh
    } else {
        Session::Local
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use termite::Session;

    use super::{read_arguments, session_from};
    use crate::output::Format;

    #[test]
    fn given_ssh_variables_when_read_then_the_session_is_ssh() {
        for name in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
            let session = session_from(|asked| {
                (asked == name).then(|| OsString::from("10.0.0.1 22"))
            });
            assert_eq!(session, Session::Ssh, "{name}");
        }
        assert_eq!(session_from(|_| None), Session::Local);
        assert_eq!(session_from(|_| Some(OsString::new())), Session::Local);
    }

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn given_arguments_when_read_then_format_and_query_are_returned() {
        let cases = [
            ("? list files", Format::Human, "? list files"),
            ("--json ?? list files", Format::Json, "?? list files"),
            ("--zle -- ?x list files", Format::Zle, "?x list files"),
            ("--json --zle ? x", Format::Zle, "? x"),
        ];

        for (line, format, query) in cases {
            assert_eq!(
                read_arguments(&words(line)),
                Ok(Some((format, query.to_owned()))),
                "{line}"
            );
        }
    }

    #[test]
    fn given_help_or_bad_arguments_when_read_then_they_are_reported() {
        assert_eq!(read_arguments(&words("--help")), Ok(None));
        assert_eq!(read_arguments(&words("-h")), Ok(None));
        assert!(read_arguments(&[]).is_err());
        assert!(read_arguments(&words("--zle")).is_err());
        assert!(read_arguments(&words("--yaml ? x")).is_err());
    }
}
