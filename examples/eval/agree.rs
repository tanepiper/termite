//! Sends the same commands to the policy on each build, and finds any build
//! that disagrees. One build writes one file; `compare` reads them all.
//! No model is involved, and nothing is run.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use termite::{read_only, read_only_in};

use crate::edges::{LENGTHS, PADDINGS, padded};
use crate::history::fingerprint;

/// The same path on every build, because a refusal can name it.
const FOLDER: &str = "/tmp/termite-agree";
/// Folders for `read_only_in`, as paths under `FOLDER`.
const PLACES: [&str; 4] = ["repo", "tree", "link", "home/.ssh"];

/// What one build did with every command. Each result is `ok`, or a short
/// fingerprint of the refusal, so no command is stored here.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct AgreeFile {
    pub build: String,
    pub started: u64,
    pub commit: String,
    pub corpus: String,
    pub results: Vec<String>,
}

/// The scripted commands, the dataset commands, and each scripted command
/// padded to the edges.
pub fn inputs(scripted: &[String], outside: &[String]) -> Vec<String> {
    let mut all: Vec<String> =
        scripted.iter().chain(outside).cloned().collect();
    for command in scripted {
        for padding in PADDINGS {
            all.extend(
                LENGTHS
                    .iter()
                    .filter_map(|&length| padded(command, padding, length)),
            );
        }
    }
    all
}

fn word<E: ToString>(result: Result<(), E>) -> String {
    match result {
        Ok(()) => "ok".to_owned(),
        Err(error) => fingerprint(&[&error.to_string()])[..8].to_owned(),
    }
}

/// A repository whose config runs a program, a worktree of it, a link to
/// it, and a secret folder. Made the same way on every build.
pub fn make_folder(root: &Path) -> Result<(), String> {
    let write = |path: PathBuf, text: &str| -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap_or(root))?;
        fs::write(path, text)
    };
    let made = (|| {
        if root.exists() {
            fs::remove_dir_all(root)?;
        }
        fs::create_dir_all(root.join("home/.ssh"))?;
        write(root.join("repo/.git/config"), "[core]\n\tfsmonitor = ./x\n")?;
        write(root.join("repo/.git/worktrees/t/commondir"), "../..\n")?;
        let pointer =
            format!("gitdir: {}/repo/.git/worktrees/t\n", root.display());
        write(root.join("tree/.git"), &pointer)?;
        std::os::unix::fs::symlink(root.join("repo"), root.join("link"))
    })();
    made.map_err(|error| error.to_string())
}

/// One line for each command: the text check, then each place in turn.
pub fn results(commands: &[String], root: &Path) -> Vec<String> {
    let home = root.join("home");
    commands
        .iter()
        .map(|command| {
            let mut words = vec![word(read_only(command))];
            words.extend(PLACES.iter().map(|place| {
                word(read_only_in(command, &root.join(place), Some(&home)))
            }));
            words.join(" ")
        })
        .collect()
}

pub fn folder() -> PathBuf {
    PathBuf::from(FOLDER)
}

/// Where the builds disagree: the input's position, and each build's result.
pub type Difference = (usize, BTreeMap<String, String>);

/// Compare builds. Files must share a commit and a test set, so `compare`
/// takes the newest of each build for the newest commit and set.
pub fn compare(files: &[AgreeFile]) -> Vec<Difference> {
    let Some(first) = files.first() else {
        return Vec::new();
    };
    (0..first.results.len())
        .filter_map(|index| {
            let seen: BTreeMap<String, String> = files
                .iter()
                .map(|file| {
                    let result = file.results.get(index).cloned();
                    (file.build.clone(), result.unwrap_or_default())
                })
                .collect();
            let same =
                seen.values().all(|result| *result == first.results[index]);
            (!same).then_some((index, seen))
        })
        .collect()
}

/// The newest file from each build that matches the newest file overall.
pub fn latest(mut files: Vec<AgreeFile>) -> Vec<AgreeFile> {
    files.sort_by_key(|file| file.started);
    let Some(newest) = files.last().cloned() else {
        return Vec::new();
    };
    let mut builds = BTreeMap::new();
    for file in files {
        if file.commit == newest.commit && file.corpus == newest.corpus {
            builds.insert(file.build.clone(), file);
        }
    }
    builds.into_values().collect()
}

/// Counts only: the public view.
#[derive(Debug, Serialize)]
pub struct PublicAgreement {
    pub started: u64,
    pub commit: String,
    pub builds: Vec<String>,
    pub commands: usize,
    pub checks: usize,
    pub disagreements: usize,
}

pub fn public(files: &[AgreeFile]) -> Option<PublicAgreement> {
    let newest = files.iter().max_by_key(|file| file.started)?;
    Some(PublicAgreement {
        started: newest.started,
        commit: newest.commit.clone(),
        builds: files.iter().map(|file| file.build.clone()).collect(),
        commands: newest.results.len(),
        checks: PLACES.len() + 1,
        disagreements: compare(files).len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(
        build: &str,
        commit: &str,
        started: u64,
        results: &[&str],
    ) -> AgreeFile {
        AgreeFile {
            build: build.to_owned(),
            started,
            commit: commit.to_owned(),
            corpus: "c".to_owned(),
            results: results.iter().map(|&line| line.to_owned()).collect(),
        }
    }

    #[test]
    fn given_builds_that_agree_then_there_is_no_difference() {
        let files = [file("a", "x", 1, &["ok"]), file("b", "x", 2, &["ok"])];
        assert!(compare(&files).is_empty());
    }

    #[test]
    fn given_a_build_that_differs_then_its_input_is_named() {
        let files = [
            file("a", "x", 1, &["ok", "ok"]),
            file("b", "x", 2, &["ok", "12345678"]),
        ];
        let found = compare(&files);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 1);
        assert_eq!(found[0].1["b"], "12345678");
    }

    #[test]
    fn given_a_build_with_fewer_results_then_it_counts_as_a_difference() {
        let files = [file("a", "x", 1, &["ok"]), file("b", "x", 2, &[])];
        assert_eq!(compare(&files).len(), 1);
    }

    #[test]
    fn given_an_older_commit_then_it_is_not_compared() {
        let files = vec![
            file("a", "old", 1, &["ok"]),
            file("a", "new", 2, &["ok"]),
            file("b", "new", 3, &["ok"]),
            file("c", "old", 1, &["no"]),
        ];
        let kept = latest(files);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|file| file.commit == "new"));
    }

    #[test]
    fn given_the_folder_then_each_place_gives_a_result() {
        let root = std::env::temp_dir()
            .join(format!("termite-agree-test-{}", std::process::id()));
        make_folder(&root).expect("folder");
        let found = results(&["git status".to_owned()], &root);
        fs::remove_dir_all(&root).ok();
        assert_eq!(found[0].split(' ').count(), PLACES.len() + 1);
    }
}
