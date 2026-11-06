//! An agent can write a script and then run it, so the words of the command
//! say nothing. This reads the script, runs nothing, and only decides what an
//! agent may have. A person's level stays as it is, and this can't see what a
//! script imports or builds while it runs.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use super::Level;
use super::parse::Stage;
use super::programs::{LANGUAGES, SHELLS};
use super::walk::Walk;

/// The most of a script that is read. A longer one counts as unread.
const MAX_BYTES: u64 = 256 * 1024;

/// How many scripts one command may lead to.
pub(super) const MAX_FILES: usize = 8;

/// Words in Python, Node, Ruby, Perl or PHP code that reach another machine,
/// start a program, build code to run, or name a secret.
const CODE_WORDS: &[&str] = &[
    "__builtins__",
    "__import__",
    "aiohttp",
    "axios",
    "child_process",
    "ctypes",
    "curl_exec",
    "dgram",
    "eval(",
    "exec(",
    "fetch(",
    "fsockopen",
    "ftplib",
    "http.client",
    "httpx",
    "importlib",
    "io::socket",
    "lwp::",
    "net/http",
    "open-uri",
    "os.popen",
    "os.system",
    "paramiko",
    "popen(",
    "pty.",
    "require('http",
    "require('net",
    "require('tls",
    "require(\"http",
    "require(\"net",
    "require(\"tls",
    "requests",
    "shell_exec",
    "smtplib",
    "socket",
    "subprocess",
    "system(",
    "telnetlib",
    "urllib",
    "websocket",
    ".aws",
    ".env",
    ".gnupg",
    ".kube",
    ".ssh",
    "id_rsa",
    "keychain",
];

const UNREAD: &str = "runs a file that can't be read";
const UNFIT: &str = "runs a file that reaches out or reads secrets";

enum Script {
    Text(String),
    Compiled,
    Unread,
}

impl Walk<'_> {
    /// Read the script this stage runs, if it runs one.
    pub(super) fn looks_inside(
        &mut self,
        first: &str,
        name: &str,
        arguments: &[String],
        stage: &Stage,
        depth: usize,
    ) {
        let Some(word) = script_word(first, name, arguments, stage) else {
            if stage.piped_in && takes_a_script(name, arguments) {
                self.stop_agent(UNREAD);
            }
            return;
        };
        match self.read_script(word) {
            Script::Compiled => {},
            Script::Unread => self.stop_agent(UNREAD),
            Script::Text(text) => {
                let code = without_comments(&text);
                let unfit = if is_code(name, &text) {
                    has_code_word(&code)
                } else {
                    self.shell_is_unfit(&code, depth)
                };
                if unfit {
                    self.stop_agent(UNFIT);
                }
            },
        }
    }

    fn read_script(&mut self, word: &str) -> Script {
        if self.files_left == 0 {
            return Script::Unread;
        }
        self.files_left -= 1;
        let base = self.base.clone();
        let paths = self.place.resolve(word, base.as_deref());
        match paths.as_slice() {
            [Some(path)] => read(path),
            _ => Script::Unread,
        }
    }

    /// Variables are normal in a script, so only what the script clearly
    /// does counts here, not the doubt its text raises.
    fn shell_is_unfit(&mut self, code: &str, depth: usize) -> bool {
        let base = self.base.clone().unwrap_or_default();
        let mut inner = Walk::new(self.place, &base);
        inner.files_left = self.files_left;
        inner.line(code, depth + 1);
        self.files_left = inner.files_left;
        let sensitive = inner.sensitive;
        let found = inner.finish(code);
        sensitive || found.leaves || found.level >= Level::SystemDangerous
    }

    fn stop_agent(&mut self, reason: &'static str) {
        self.leaves = true;
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }
}

/// The word that names the script, if this stage runs one. `sh < file` runs
/// the file, so with no script named, a redirected file is the script.
fn script_word<'a>(
    first: &'a str,
    name: &str,
    arguments: &'a [String],
    stage: &'a Stage,
) -> Option<&'a str> {
    if first.contains('/') && !first.starts_with('/') {
        return Some(first);
    }
    if !takes_a_script(name, arguments) {
        return None;
    }
    let named = arguments.iter().find(|word| !word.starts_with('-'));
    named.or(stage.reads.first()).map(String::as_str)
}

/// A shell or a language, not given its code in the command itself.
fn takes_a_script(name: &str, arguments: &[String]) -> bool {
    let inline = ["-c", "-e", "-m", "-r"];
    (SHELLS.contains(&name)
        || matches!(name, "source" | ".")
        || is_language(name))
        && !arguments.iter().any(|word| inline.contains(&word.as_str()))
}

fn is_language(name: &str) -> bool {
    LANGUAGES.iter().any(|language| name.starts_with(language))
}

/// A language runs it, or its first line names one.
fn is_code(name: &str, text: &str) -> bool {
    let first = text.lines().next().unwrap_or_default();
    is_language(name)
        || (first.starts_with("#!")
            && LANGUAGES.iter().any(|language| first.contains(language)))
}

fn has_code_word(code: &str) -> bool {
    let lower = code.to_lowercase();
    CODE_WORDS.iter().any(|word| lower.contains(word))
}

fn without_comments(text: &str) -> String {
    let code = text.lines().filter(|line| {
        let line = line.trim_start();
        !line.starts_with('#') && !line.starts_with("//")
    });
    code.collect::<Vec<_>>().join("\n")
}

/// Only a plain file: a pipe or a device could hold the check open forever.
fn read(path: &Path) -> Script {
    let plain = fs::metadata(path).is_ok_and(|file| file.is_file());
    let Some(file) = plain.then(|| File::open(path).ok()).flatten() else {
        return Script::Unread;
    };
    let mut bytes = Vec::new();
    let limit = MAX_BYTES + 1;
    if file.take(limit).read_to_end(&mut bytes).is_err()
        || bytes.len() as u64 > MAX_BYTES
    {
        return Script::Unread;
    }
    if bytes.contains(&0) {
        return Script::Compiled;
    }
    String::from_utf8(bytes).map_or(Script::Unread, Script::Text)
}

#[cfg(test)]
mod tests;
