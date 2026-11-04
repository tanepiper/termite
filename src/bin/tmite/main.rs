//! `tmite`: ask a local model for a shell command.
//!
//! ```text
//! tmite [--json | --zle] '<? | ?? | ?x> request'
//! tmite setup
//! ```
//!
//! `tmite` only prints. Running a command is left to the shell: the zsh
//! widget in `shell/tmite.zsh` puts it at the prompt, or runs it for `?x`
//! when every safety check passed.

mod output;
mod setup;

use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use termite::{
    Config, Context, LlamaServer, Session, SystemOne, answer, decision_token,
    parse_query,
};

use crate::output::{Format, exit_code, render, render_failure};

const USAGE: &str = "\
Usage: tmite [--json | --zle] '<prefix> request'
       tmite setup   find local models and write the config

Prefixes:
  ?   one command, best first; you confirm it
  ??  every distinct command, best first; you choose
  ?x  one command; reported safe to run only if every safety check passes

Options:
  --json  print one JSON object, for scripts
  --zle   print the line format read by shell/tmite.zsh
  -h, --help  show this help

Config: ~/.config/termite/config.json. Token: TERMITE_DECISION_TOKEN.
Exit status: 0 a command to use, 2 none or refused, 1 failed, 64 usage.
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
    let Some(query) = parse_query(input) else {
        let message = "start the request with ?, ?? or ?x";
        return (render_failure(message, format), 1);
    };

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
    let answer = runtime.block_on(answer(&query, &context, &source, &decider));

    (render(&answer, format), exit_code(&answer))
}

/// `tmite` runs in the user's own shell, so the session is local and the
/// shell is waiting for this command to finish.
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

    Context::new(env::consts::OS, shell, cwd, Session::Local, true)
}

#[cfg(test)]
mod tests {
    use super::read_arguments;
    use crate::output::Format;

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
