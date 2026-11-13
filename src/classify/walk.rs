//! Follows a command line stage by stage and keeps the worst of what it finds.

use std::path::{Path, PathBuf};

use super::Level;
use super::deletes::{LOOSE, find_names_exactly};
use super::files::MAX_FILES;
use super::overwrites::destinations;
use super::parse::{Heredoc, Stage, parse};
use super::paths::{Place, is_scratch_file, is_secret, is_system, path_forms};
use super::programs::{
    DELETERS, SEARCHERS, Unwrapped, hostile, is_recursive, level_of, name_of,
    shows_secrets, targets, unwrap,
};
use super::reads::{as_plain_name, quoted_words};
use crate::policy::{PolicyRefusal, read_only_in};

/// How deep commands inside commands are followed.
const MAX_DEPTH: usize = 8;

pub(super) const SECRETS: &str = "reads secrets or private data";
pub(super) const ROOT: &str = "needs root";

/// What has been seen so far along a command line.
pub(super) struct Walk<'a> {
    pub(super) place: &'a Place,
    /// The folder the next stage runs in. `None` after `cd` to a place that
    /// can't be worked out.
    pub(super) base: Option<PathBuf>,
    pub(super) level: Level,
    pub(super) reasons: Vec<&'static str>,
    pub(super) doubt: bool,
    pub(super) sensitive: bool,
    pub(super) sends: bool,
    pub(super) leaves: bool,
    pub(super) names_network: bool,
    pub(super) files_left: usize,
    pub(super) fetches: bool,
    pub(super) runs_fed_code: bool,
    /// Stages that aren't deletes inside the project.
    pub(super) uncontained: Vec<String>,
    /// Files this command wrote from a here-document, with their text.
    pub(super) written: Vec<(PathBuf, String)>,
    /// Something in it may show a secret or private data.
    pub(super) private: bool,
    /// It runs code that can't be read here, or changes which code runs.
    pub(super) unchecked: bool,
    /// Here-document text that something runs, as a shell or in another
    /// language. Text that nothing can run isn't kept.
    pub(super) fed_code: Vec<String>,
}

impl<'a> Walk<'a> {
    pub(super) fn new(place: &'a Place, cwd: &'a Path) -> Self {
        Self {
            place,
            base: Some(cwd.to_path_buf()),
            level: Level::Read,
            reasons: Vec::new(),
            doubt: false,
            sensitive: false,
            sends: false,
            leaves: false,
            names_network: false,
            files_left: MAX_FILES,
            fetches: false,
            runs_fed_code: false,
            uncontained: Vec::new(),
            written: Vec::new(),
            private: false,
            unchecked: false,
            fed_code: Vec::new(),
        }
    }

    pub(super) fn raise(&mut self, level: Level, reason: &'static str) {
        self.level = self.level.raised_by(level);
        self.private |= level == Level::SensitiveRead;
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    pub(super) fn line(&mut self, command: &str, depth: usize) {
        if depth > MAX_DEPTH {
            self.raise(
                Level::SystemDangerous,
                "hides commands inside commands",
            );
            return;
        }
        let parsed = parse(command);
        self.doubt |= parsed.doubt;
        if parsed.broken {
            self.raise(
                Level::SystemDangerous,
                "its quotes or brackets don't close",
            );
        }
        for stage in &parsed.stages {
            self.stage(stage, &parsed.bodies, depth);
        }
        for code in &parsed.inner {
            self.line(code, depth + 1);
        }
    }

    fn stage(&mut self, stage: &Stage, bodies: &[Heredoc], depth: usize) {
        let words = shell_words::split(&stage.text).unwrap_or_else(|_| {
            self.doubt = true;
            stage.text.split_whitespace().map(str::to_owned).collect()
        });
        let found = unwrap(words);
        self.doubt |= found.doubt;
        self.unchecked |= found.doubt;
        self.redirects(stage);
        if found.root {
            self.raise(Level::SystemDangerous, ROOT);
        }
        let Some((first, arguments)) = found.words.split_first() else {
            self.raise(Level::Change, "changes the shell's settings");
            return;
        };
        let is_test = matches!(first.as_str(), "[" | "[[");
        if !is_test && first.contains(['$', '{', '*', '?', '[']) {
            self.raise(
                Level::SystemDangerous,
                "the program is only known when it runs",
            );
        }
        let name = name_of(first);
        if self.changed_folder(&name, arguments) {
            return;
        }

        self.notice(&name, arguments, stage);
        self.overwrites(destinations(&name, arguments));
        self.looks_inside(first, &name, arguments, stage, depth);
        self.inline_code(&name, arguments);
        for place in &stage.heredocs {
            self.fed_text(&name, stage, &bodies[*place].body, depth);
        }
        if let Some(reason) = hostile(&name, arguments) {
            self.raise(Level::Never, reason);
        }
        let contained_delete = self.deletes(&name, arguments, &found);
        if !contained_delete {
            self.uncontained.push(stage.text.clone());
        }
        self.nested(&name, arguments, depth);
        self.secrets(&name, arguments, stage);
        self.baseline(&name, arguments, stage);
    }

    /// `cd` changes what the words after it mean.
    fn changed_folder(&mut self, name: &str, arguments: &[String]) -> bool {
        if !matches!(name, "cd" | "pushd" | "popd" | "chdir") {
            return false;
        }
        let target = targets(arguments).first().copied();
        let base = self.base.clone();
        let next = target.and_then(|word| {
            let mut all = self.place.resolve(word, base.as_deref());
            (all.len() == 1).then(|| all.remove(0)).flatten()
        });
        self.base = match (name, target) {
            ("cd", None) => self.place.home.clone(),
            _ => next,
        };
        true
    }

    /// True for a delete with every target inside the project.
    fn deletes(
        &mut self,
        name: &str,
        arguments: &[String],
        found: &Unwrapped,
    ) -> bool {
        let finds =
            name == "find" && arguments.iter().any(|word| word == "-delete");
        let git_rm = name == "git"
            && arguments.first().is_some_and(|w| w == "rm")
            && !arguments.iter().any(|w| w == "--cached");
        if !finds && !git_rm && !DELETERS.contains(&name) {
            if matches!(name, "chmod" | "chown" | "chgrp") {
                self.ownership(name, arguments);
            }
            return false;
        }
        if finds && !find_names_exactly(arguments) {
            self.raise(Level::SystemDangerous, LOOSE);
            return false;
        }
        let words = if finds {
            find_roots(arguments)
        } else if git_rm {
            targets(&arguments[1..])
        } else {
            targets(arguments)
        };
        let contained = !found.unknown_arguments
            && !words.is_empty()
            && words.iter().all(|word| self.is_contained(word, finds));
        if contained {
            self.raise(Level::ContainedDestructive, "deletes data");
        } else {
            self.raise(Level::SystemDangerous, "deletes outside the project");
        }
        contained
    }

    fn is_contained(&self, word: &str, may_be_root: bool) -> bool {
        self.place
            .resolve(word, self.base.as_deref())
            .iter()
            .all(|path| match path {
                Some(path) if may_be_root => self.place.in_project(path),
                Some(path) => {
                    self.place.contained(path)
                        || (word.starts_with('/')
                            && is_scratch_file(path)
                            && !self.place.holds_work(path))
                },
                None => false,
            })
    }

    fn ownership(&mut self, name: &str, arguments: &[String]) {
        let outside = targets(arguments)
            .iter()
            .skip(1)
            .any(|word| !self.is_contained(word, true));
        if outside && (name != "chmod" || is_recursive(arguments)) {
            self.raise(
                Level::SystemDangerous,
                "changes owners or modes outside the project",
            );
        }
    }

    fn secrets(&mut self, name: &str, arguments: &[String], stage: &Stage) {
        let base = self.base.clone();
        let keys_only = matches!(name, "ssh" | "scp" | "sftp");
        let mut words: Vec<&str> = Vec::new();
        let mut skip = false;
        for word in arguments {
            if std::mem::take(&mut skip) {
                continue;
            }
            skip = keys_only && word == "-i";
            if !skip {
                words.extend(path_forms(word));
            }
        }
        words.extend(stage.reads.iter().map(String::as_str));

        let quoted = quoted_words(&stage.text);
        let names_one = words.iter().any(|word| {
            let plain = quoted
                .iter()
                .any(|one| one == word)
                .then(|| as_plain_name(word));
            let word = plain.as_deref().unwrap_or(word);
            self.place
                .resolve(word, base.as_deref())
                .iter()
                .any(|path| is_secret(word, path.as_deref()))
        });
        let in_secret_folder = base
            .as_deref()
            .is_some_and(|folder| is_secret("", Some(folder)));
        if names_one
            || in_secret_folder
            || shows_secrets(name, arguments)
            || self.searches_widely(name, arguments)
        {
            self.sensitive = true;
            self.raise(Level::SensitiveRead, SECRETS);
        }
    }

    /// Reading everything below `~` or `/` reads the secrets there too.
    fn searches_widely(&self, name: &str, arguments: &[String]) -> bool {
        if !SEARCHERS.contains(&name) {
            return false;
        }
        let recursive = matches!(name, "rg" | "ag" | "ack" | "find")
            || is_recursive(arguments);
        let base = self.base.as_deref();
        let wide = |path: &Path| self.place.is_wide(path);
        recursive
            && (base.is_some_and(wide)
                || targets(arguments).iter().any(|word| {
                    self.place
                        .resolve(word, base)
                        .iter()
                        .flatten()
                        .any(|p| wide(p))
                }))
    }

    /// Level 0 if the policy says it only reads, otherwise what the program
    /// does.
    fn baseline(&mut self, name: &str, arguments: &[String], stage: &Stage) {
        let judged = self.base.as_deref().map(|folder| {
            read_only_in(&stage.text, folder, self.place.home.as_deref())
        });
        if matches!(judged, Some(Ok(()))) && stage.writes.is_empty() {
            return;
        }
        let everything = matches!(
            &judged,
            Some(Err(PolicyRefusal::SearchesEverything { .. }))
        );
        let reads_all = matches!(
            &judged,
            Some(Err(refusal)) if reads_everything(refusal)
        );
        if reads_all && (!everything || self.search_may_read_secrets(arguments))
        {
            self.sensitive = true;
            self.raise(Level::SensitiveRead, SECRETS);
        }
        if let Some((level, reason)) = level_of(name, arguments) {
            self.raise(level, reason);
        } else if self.level <= Level::Change
            && !shows_secrets(name, arguments)
            && !matches!(judged, Some(Err(refusal)) if only_reads(&refusal))
        {
            self.raise(Level::Change, "may change something");
        }
        self.reaches(name, arguments);
    }

    fn redirects(&mut self, stage: &Stage) {
        self.overwrites(stage.replaces.iter().map(String::as_str));
        let base = self.base.clone();
        for target in &stage.writes {
            self.raise(Level::Change, "writes a file");
            let paths = self.place.resolve(target, base.as_deref());
            self.starts_with_the_shell(&paths);
            for path in paths {
                let outside = path.as_deref().is_none_or(is_system);
                let secret = is_secret(target, path.as_deref());
                if outside || secret {
                    self.raise(
                        Level::SystemDangerous,
                        "writes outside the project",
                    );
                }
            }
        }
    }
}

/// A search that takes in hidden files, or a file that holds secrets.
fn reads_everything(refusal: &PolicyRefusal) -> bool {
    !matches!(refusal, PolicyRefusal::ExpandsBraces) && only_reads(refusal)
}

/// The policy turned it down for what it reads, not for changing anything.
/// Those reads have their own level, and braces count as doubt.
fn only_reads(refusal: &PolicyRefusal) -> bool {
    matches!(
        refusal,
        PolicyRefusal::ReadsSecret { .. }
            | PolicyRefusal::SearchesEverything { .. }
            | PolicyRefusal::SearchesTooWidely { .. }
            | PolicyRefusal::ExpandsBraces
    )
}

/// The folders `find` starts from: the words before the first option.
fn find_roots(arguments: &[String]) -> Vec<&str> {
    let roots: Vec<&str> = arguments
        .iter()
        .take_while(|word| !word.starts_with('-'))
        .map(String::as_str)
        .collect();
    if roots.is_empty() { vec!["."] } else { roots }
}
