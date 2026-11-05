//! The outside programs a script meets: `bash -n` and shellcheck, which check
//! it, and your pager, which shows it. Contract: they get the script as text on
//! standard input, and it is never run.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TIME_LIMIT: Duration = Duration::from_secs(20);
const MOST_PROBLEMS: usize = 20;

/// Stop early if a tool is missing, saying how to get it.
pub(crate) fn ready() -> Result<(), String> {
    let (status, version) = run_tool(
        Command::new("bash").args(["-c", "printf '%s' \"$BASH_VERSION\""]),
        "",
    )
    .map_err(|_| "bash wasn't found. Install bash 5.3 or later.".to_owned())?;
    if !status || !is_new_enough(&version) {
        return Err(format!(
            "scripts need bash 5.3 or later, and this is {version}. On a Mac, run: brew install bash"
        ));
    }
    if run_tool(Command::new("shellcheck").arg("--version"), "").is_err() {
        return Err(
            "shellcheck wasn't found. On a Mac, run: brew install shellcheck"
                .to_owned(),
        );
    }
    Ok(())
}

fn is_new_enough(version: &str) -> bool {
    let mut parts = version.split('.').map(str::parse::<u32>);
    match (parts.next(), parts.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => (major, minor) >= (5, 3),
        _ => false,
    }
}

/// Everything wrong with `script`, one line each.
pub(crate) fn problems(script: &str) -> Vec<String> {
    let mut found = Vec::new();
    // BASH_ENV would run a file when bash starts.
    let mut syntax = Command::new("bash");
    syntax.arg("-n").env_remove("BASH_ENV");
    match run_tool(&mut syntax, script) {
        Ok((true, _)) => {},
        Ok((false, output)) => {
            found.push(format!("syntax: {}", first_lines(&output)))
        },
        Err(problem) => found.push(problem),
    }

    // A project's own settings must not be able to turn checks off.
    let mut lint = Command::new("shellcheck");
    lint.args([
        "--norc",
        "--severity=warning",
        "--shell=bash",
        "--format=gcc",
        "-",
    ])
    .env_remove("SHELLCHECK_OPTS");
    match run_tool(&mut lint, script) {
        Ok((true, _)) => {},
        Ok((false, output)) => found.extend(
            output
                .lines()
                .take(MOST_PROBLEMS)
                .map(|line| line.trim_start_matches("-:").to_owned()),
        ),
        Err(problem) => found.push(problem),
    }
    found
}

fn first_lines(output: &str) -> String {
    let lines: Vec<&str> = output.lines().take(3).collect();
    lines.join(" ")
}

/// Run a tool with `input` on standard input. Returns whether it passed and
/// what it printed, or why it couldn't finish.
pub(crate) fn run_tool(
    command: &mut Command,
    input: &str,
) -> Result<(bool, String), String> {
    let name = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{name} couldn't start: {error}"))?;

    let stdin = child.stdin.take();
    let bytes = input.to_owned();
    let writer = thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            // A tool that stops reading closes the pipe, which is fine here.
            let _ = stdin.write_all(bytes.as_bytes());
        }
    });
    let readers = [
        child.stdout.take().map(|pipe| read_all(Box::new(pipe))),
        child.stderr.take().map(|pipe| read_all(Box::new(pipe))),
    ];

    let deadline = Instant::now() + TIME_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            },
            Ok(None) | Err(_) => {
                // It may already have gone, which is what's wanted.
                let _ = child.kill();
                let _ = child.wait();
                break None;
            },
        }
    };
    let _ = writer.join();
    let mut output = String::new();
    for reader in readers.into_iter().flatten() {
        output.push_str(&reader.join().unwrap_or_default());
    }
    match status {
        Some(status) => Ok((status.success(), output)),
        None => Err(format!("{name} took too long")),
    }
}

fn read_all(mut pipe: Box<dyn Read + Send>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        // What was read before an error is still worth showing.
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Show `text` in the reader's pager, then come back.
pub(crate) fn page(text: &str) {
    let pager = std::env::var("PAGER").unwrap_or_default();
    let words = shell_words::split(&pager).unwrap_or_default();
    let (program, options) = match words.split_first() {
        Some((program, options)) => (program.as_str(), options),
        None => ("less", &[][..]),
    };
    let child = Command::new(program)
        .args(options)
        .stdin(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        print!("{text}");
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        // Quitting the pager early closes the pipe, which is fine.
        let _ = stdin.write_all(text.as_bytes());
    }
    // How the pager ended changes nothing: the question comes next.
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools_here() -> bool {
        ready().is_ok()
    }

    #[test]
    fn given_bash_versions_then_only_5_3_and_later_pass() {
        for (version, passes) in [
            ("5.3.20(1)-release", true),
            ("5.10.0", true),
            ("6.0", true),
            ("5.2.37", false),
            ("3.2.57(1)-release", false),
            ("", false),
        ] {
            assert_eq!(is_new_enough(version), passes, "{version}");
        }
    }

    #[test]
    fn given_a_clean_script_then_there_are_no_problems() {
        if !tools_here() {
            return;
        }

        assert_eq!(
            problems("#!/usr/bin/env bash\nprintf 'hi\\n'\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn given_broken_syntax_then_it_is_reported() {
        if !tools_here() {
            return;
        }

        let found = problems("#!/usr/bin/env bash\nif then\n");

        assert!(found.iter().any(|p| p.starts_with("syntax")), "{found:?}");
    }

    #[test]
    fn given_a_lint_warning_then_it_is_reported() {
        if !tools_here() {
            return;
        }

        let found = problems("#!/usr/bin/env bash\ncd /tmp\nx=1\n");

        assert!(found.iter().any(|p| p.contains("SC2034")), "{found:?}");
    }

    #[test]
    fn given_a_script_that_would_write_a_file_then_checking_does_not_run_it() {
        if !tools_here() {
            return;
        }
        let folder = std::env::temp_dir()
            .join(format!("tmite-tools-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let marker = folder.join("ran");

        problems(&format!(
            "#!/usr/bin/env bash\n: > '{}'\n",
            marker.display()
        ));

        assert!(!marker.exists());
        std::fs::remove_dir_all(&folder).expect("clean up");
    }
}
