//! Code held in an argument, which is read as a command line of its own.

use super::programs::SHELLS;
use super::walk::Walk;

impl Walk<'_> {
    /// Code held in an argument: `sh -c`, `eval`, `find -exec`.
    pub(super) fn nested(
        &mut self,
        name: &str,
        arguments: &[String],
        depth: usize,
    ) {
        let code = if SHELLS.contains(&name) {
            let flag = arguments.iter().position(|word| {
                word.starts_with('-')
                    && !word.starts_with("--")
                    && word.contains('c')
            });
            flag.and_then(|at| arguments.get(at + 1)).cloned()
        } else if name == "fd" {
            fd_code(arguments)
        } else if name == "eval" {
            Some(arguments.join(" "))
        } else if name == "find" {
            find_code(arguments)
        } else {
            None
        };
        if let Some(code) = code {
            self.doubt = true;
            self.line(&code, depth + 1);
        }
    }
}

/// The command after `fd -x`, which is the rest of the line.
fn fd_code(arguments: &[String]) -> Option<String> {
    let start = arguments.iter().position(|word| {
        matches!(word.as_str(), "-x" | "-X" | "--exec" | "--exec-batch")
    })?;
    Some(arguments[start + 1..].join(" "))
}

/// The command after `find -exec`, up to `;` or `+`.
fn find_code(arguments: &[String]) -> Option<String> {
    let start = arguments.iter().position(|word| {
        matches!(word.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir")
    })?;
    let body: Vec<&str> = arguments[start + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|word| !matches!(*word, ";" | "+"))
        .collect();
    Some(body.join(" "))
}
