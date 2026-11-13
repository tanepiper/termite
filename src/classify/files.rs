//! An agent can write a script and then run it, so the words of the command
//! say nothing. This reads the script, runs nothing, and only decides what an
//! agent may have. A person's level stays as it is, and this can't see what a
//! script imports or builds while it runs.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use super::Level;
use super::parse::Stage;
use super::programs::{LANGUAGES, SHELLS, runs_inline_code};
use super::walk::Walk;
use crate::policy::read_only_in;

/// The most of a script that is read. A longer one counts as unread.
const MAX_BYTES: u64 = 256 * 1024;

/// How many scripts one command may lead to.
pub(super) const MAX_FILES: usize = 8;

/// Words in Python, Node, Ruby, Perl or PHP code that reach another machine,
/// start a program, build code to run, delete files, find the home folder,
/// or name a secret.
const CODE_WORDS: &[&str] = &[
    "__builtins__",
    "__import__",
    "aiohttp",
    "axios",
    "child_process",
    "ctypes",
    "curl_exec",
    "expanduser",
    "file.delete",
    "fs.rm",
    "fs.unlink",
    "homedir",
    "os.remove",
    "os.unlink",
    "rmtree",
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
    "requests.",
    "import requests",
    "from requests",
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
        let [Some(path)] = paths.as_slice() else {
            return Script::Unread;
        };
        let written = self.written.iter().rev().find(|(to, _)| to == path);
        match written {
            Some((_, text)) => Script::Text(text.clone()),
            None => read(path),
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

    /// Code given in the command itself. In a language it is searched as a
    /// script file in that language is, and the same words make it unfit
    /// for an agent. Given to any other program it can't be read at all.
    pub(super) fn inline_code(&mut self, name: &str, arguments: &[String]) {
        if !runs_inline_code(name, arguments) {
            return;
        }
        self.doubt = true;
        if !is_language(name) {
            self.unchecked = true;
        } else if has_code_word(&arguments.join(" ")) {
            self.stop_agent(UNFIT);
        }
    }

    /// The text of a here-document, judged by what receives it. A shell
    /// runs it, so it is read as commands. A language runs it, so it is
    /// searched as a script file in that language is. A program that only
    /// reads can't run it, so it is plain text and isn't searched at all.
    /// For any other program each line is searched for hazards.
    pub(super) fn fed_text(
        &mut self,
        name: &str,
        stage: &Stage,
        body: &str,
        depth: usize,
    ) {
        if SHELLS.contains(&name) || matches!(name, "source" | ".") {
            self.doubt = true;
            self.fed_code.push(body.to_owned());
            self.line(body, depth + 1);
        } else if is_language(name) {
            self.doubt = true;
            self.fed_code.push(body.to_owned());
            if has_code_word(&without_comments(body)) {
                self.stop_agent(UNFIT);
            }
        } else {
            if !self.only_reads(stage) {
                self.fed_code.push(body.to_owned());
            }
            self.keep_written(stage, body);
        }
    }

    /// `cat > run.py <<END` writes a file this command may run next, before
    /// it is on disk. Its text is kept, so it is read as the file will be.
    fn keep_written(&mut self, stage: &Stage, body: &str) {
        let base = self.base.clone();
        for target in &stage.writes {
            let paths = self.place.resolve(target, base.as_deref());
            if let [Some(path)] = paths.as_slice()
                && self.written.len() < MAX_FILES
            {
                self.written.push((path.clone(), body.to_owned()));
            }
        }
    }

    fn only_reads(&self, stage: &Stage) -> bool {
        self.base.as_deref().is_some_and(|folder| {
            read_only_in(&stage.text, folder, self.place.home.as_deref())
                .is_ok()
        })
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
