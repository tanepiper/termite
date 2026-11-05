//! Keeps the shape described in `docs/architecture.md` true: small files,
//! dependencies pointing one way, and side effects in the few places
//! allowed to have them. A failure here means the design changed, so change
//! `docs/architecture.md` in the same commit, or change the code back.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The most non-test lines in one source file.
const MAX_LINES: usize = 400;

/// Each library module and the modules it may use. Anything new must be
/// added here, on purpose.
const LAYERS: &[(&str, &[&str])] = &[
    ("candidate", &[]),
    ("hazard", &[]),
    ("place", &[]),
    ("command", &[]),
    ("context", &[]),
    ("error", &[]),
    ("home", &[]),
    ("provenance", &["home"]),
    ("progress", &[]),
    ("heartbeat", &["progress"]),
    ("query", &[]),
    ("sse", &["error"]),
    ("merge", &["candidate"]),
    ("policy", &["command"]),
    ("rating", &["candidate", "hazard", "policy"]),
    ("classify", &["candidate", "hazard", "policy"]),
    ("script", &["command", "hazard", "policy", "rating"]),
    ("config", &["error", "home"]),
    (
        "decide",
        &["candidate", "command", "config", "context", "error"],
    ),
    (
        "generate",
        &[
            "candidate",
            "command",
            "config",
            "context",
            "error",
            "heartbeat",
            "progress",
            "sse",
        ],
    ),
    (
        "draft",
        &[
            "command", "context", "decide", "error", "generate", "hazard",
            "home", "progress", "rating", "script",
        ],
    ),
    (
        "answer",
        &[
            "candidate",
            "classify",
            "command",
            "context",
            "decide",
            "error",
            "generate",
            "hazard",
            "merge",
            "policy",
            "progress",
            "query",
        ],
    ),
];

/// Modules that may use the HTTP client. `config` only parses URLs with it,
/// and `error` only reads its errors.
const NETWORK_MODULES: &[&str] = &["config", "decide", "error", "generate"];

fn rust_files(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("source folder") {
        let path = entry.expect("folder entry").path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

fn source_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    rust_files(Path::new("src"), &mut found);
    rust_files(Path::new("examples"), &mut found);
    found.sort();
    found
}

fn is_test_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        name.to_string_lossy().ends_with("tests.rs")
            || name == "test_support.rs"
    })
}

/// A file's text down to its test module.
fn code_of(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("source file");
    match text.find("\n#[cfg(test)]\nmod tests") {
        Some(end) => text[..end].to_owned(),
        None => text,
    }
}

/// `src/policy/shell.rs` and `src/policy.rs` are both `policy`.
fn module_of(path: &Path) -> Option<String> {
    let inside = path.strip_prefix("src").ok()?;
    if inside.starts_with("bin") {
        return None;
    }
    let first = inside.components().next()?.as_os_str().to_str()?;
    let name = first.strip_suffix(".rs").unwrap_or(first);
    (name != "lib").then(|| name.to_owned())
}

fn library_files() -> Vec<(String, PathBuf)> {
    source_files()
        .into_iter()
        .filter(|path| !is_test_file(path))
        .filter_map(|path| Some((module_of(&path)?, path)))
        .collect()
}

/// Every `crate::name` in `code`.
fn uses_of(code: &str) -> BTreeSet<String> {
    code.split("crate::")
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

#[test]
fn every_file_stays_small() {
    for path in source_files().iter().filter(|path| !is_test_file(path)) {
        let lines = code_of(path).lines().count();
        assert!(
            lines <= MAX_LINES,
            "{} has {lines} lines of code; the limit is {MAX_LINES}. \
             Split it along a boundary a reader would recognise.",
            path.display()
        );
    }
}

#[test]
fn dependencies_point_one_way() {
    for (module, path) in library_files() {
        let Some((_, allowed)) =
            LAYERS.iter().find(|(name, _)| *name == module)
        else {
            panic!(
                "{module} is new: add it to LAYERS and the architecture doc"
            );
        };
        for used in uses_of(&code_of(&path)) {
            assert!(
                used == module || allowed.contains(&used.as_str()),
                "{} uses `{used}`, which `{module}` may not depend on",
                path.display()
            );
        }
    }
}

#[test]
fn layers_have_no_cycles() {
    // Each module may only use modules listed before it.
    for (index, (module, allowed)) in LAYERS.iter().enumerate() {
        for used in *allowed {
            let earlier = LAYERS[..index].iter().any(|(name, _)| name == used);
            assert!(earlier, "`{module}` uses `{used}`, listed after it");
        }
    }
}

#[test]
fn the_library_prints_nothing_and_starts_nothing() {
    for (_, path) in library_files() {
        let code = code_of(&path);
        for forbidden in [
            "println!",
            "eprintln!",
            "print!(",
            "eprint!(",
            "process::Command",
            "Command::new",
            "thread::spawn",
        ] {
            assert!(
                !code.contains(forbidden),
                "{} uses `{forbidden}`: only the command line tool may",
                path.display()
            );
        }
    }
}

#[test]
fn only_the_clients_touch_the_network() {
    for (module, path) in library_files() {
        if !NETWORK_MODULES.contains(&module.as_str()) {
            assert!(
                !code_of(&path).contains("reqwest"),
                "{} uses reqwest",
                path.display()
            );
        }
    }
}

#[test]
fn the_model_evaluation_never_runs_anything() {
    let mut files = Vec::new();
    rust_files(Path::new("examples"), &mut files);
    for path in files {
        let code = code_of(&path);
        for forbidden in ["process::Command", "Command::new", "libc::"] {
            assert!(
                !code.contains(forbidden),
                "{} uses `{forbidden}`: the evaluation only reads and scores",
                path.display()
            );
        }
    }
}
