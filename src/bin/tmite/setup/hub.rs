//! The models setup can install, and the commands that fetch or serve them.
//! Contract: `catalog.json` is the list. Every entry names a GGUF file at a
//! pinned revision with its hash, so what is fetched can't change underneath
//! a user, and no repository code is ever run. A new model is a new entry.

use std::path::Path;

use serde::Deserialize;

use super::discover::ModelRole;
use super::machine::Machine;

/// The file that holds the list.
const CATALOG: &str = include_str!("catalog.json");

/// A GGUF file on the Hugging Face Hub, at one revision.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HubModel {
    pub(crate) repo: String,
    pub(crate) file: String,
    /// The full commit hash.
    pub(crate) revision: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
    pub(crate) role: Role,
    pub(crate) about: String,
    /// Memory it needs to run, with its working space, in bytes.
    pub(crate) memory: u64,
}

/// `ModelRole`, as the catalog writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    Chat,
    Decision,
}

impl HubModel {
    fn serves(&self, role: ModelRole) -> bool {
        matches!(
            (self.role, role),
            (Role::Chat, ModelRole::Chat)
                | (Role::Decision, ModelRole::Decision)
        )
    }

    /// The name it is served under.
    pub(crate) fn id(&self) -> String {
        format!("{}/{}", self.repo, self.file)
    }

    /// Where the file is, at its pinned revision.
    pub(crate) fn url(&self, base: &str) -> String {
        format!(
            "{base}/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    /// Why this entry must not be used, if it must not.
    fn problem(&self) -> Option<&'static str> {
        let hex = |text: &str, length: usize| {
            text.len() == length
                && text.bytes().all(|byte| {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                })
        };
        let plain = |text: &str| {
            !text.is_empty()
                && !text.starts_with(['-', '.'])
                && !text.contains("..")
                && text.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || b"._-".contains(&byte)
                })
        };
        let repo = self.repo.split_once('/');
        if !repo.is_some_and(|(owner, name)| plain(owner) && plain(name)) {
            return Some("the repository name is not owner/name");
        }
        if !plain(&self.file) || !self.file.ends_with(".gguf") {
            return Some("the file is not a plain .gguf name");
        }
        if !hex(&self.revision, 40) {
            return Some("the revision is not a full commit hash");
        }
        if !hex(&self.sha256, 64) {
            return Some("the sha256 is not a full hash");
        }
        (self.size == 0 || self.memory < self.size)
            .then_some("the size or memory is wrong")
    }
}

/// The models on the list. An entry that isn't fully pinned is left out.
pub(crate) fn catalog() -> Vec<HubModel> {
    parse_catalog(CATALOG)
}

fn parse_catalog(text: &str) -> Vec<HubModel> {
    let models: Vec<HubModel> = serde_json::from_str(text).unwrap_or_default();
    models
        .into_iter()
        .filter(|model| model.problem().is_none())
        .collect()
}

/// The chat and decision models for a machine: the pair with the most
/// memory that fits, or the smallest pair if none does. `None` when the list
/// lacks a role. The second value says whether the pair fits.
pub(crate) fn pick(
    models: &[HubModel],
    machine: &Machine,
) -> Option<(HubModel, HubModel, bool)> {
    let budget = machine.model_budget();
    let of = |role| {
        models
            .iter()
            .filter(move |model: &&HubModel| model.serves(role))
    };
    let mut pairs: Vec<(&HubModel, &HubModel)> = of(ModelRole::Chat)
        .flat_map(|chat| of(ModelRole::Decision).map(move |d| (chat, d)))
        .collect();
    pairs.sort_by_key(|(chat, decision)| chat.memory + decision.memory);

    let fitting = pairs
        .iter()
        .rev()
        .find(|(chat, decision)| chat.memory + decision.memory <= budget);
    let fits = fitting.is_some();
    let (chat, decision) = fitting.or(pairs.first())?;
    Some(((*chat).clone(), (*decision).clone(), fits))
}

/// The command that downloads `model` into the Hugging Face cache, for the
/// Llama app, using the `hf` CLI (or the older `huggingface-cli`).
pub(crate) fn download_command(
    downloader: &Path,
    model: &HubModel,
) -> Vec<String> {
    vec![
        downloader.display().to_string(),
        "download".to_owned(),
        model.repo.clone(),
        model.file.clone(),
        "--revision".to_owned(),
        model.revision.clone(),
    ]
}

/// The command that serves a fetched file with `llama-server`. Bound to this
/// machine only.
pub(crate) fn serve_command(
    llama_server: &Path,
    model: &HubModel,
    file: &Path,
    port: u16,
) -> Vec<String> {
    vec![
        llama_server.display().to_string(),
        "--model".to_owned(),
        file.display().to_string(),
        "--alias".to_owned(),
        model.id(),
        "--host".to_owned(),
        "127.0.0.1".to_owned(),
        "--port".to_owned(),
        port.to_string(),
    ]
}

/// A size such as `4.5 GB`.
pub(crate) fn human_size(bytes: u64) -> String {
    let gigabytes = bytes as f64 / 1_000_000_000.0;
    if gigabytes >= 1.0 {
        return format!("{gigabytes:.1} GB");
    }
    format!("{:.0} MB", bytes as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{
        HubModel, Role, catalog, download_command, human_size, parse_catalog,
        pick, serve_command,
    };
    use crate::setup::machine::{Gpu, Machine};

    const GB: u64 = 1_000_000_000;

    fn entry() -> serde_json::Value {
        json!({
            "repo": "owner/model-GGUF",
            "file": "model-Q4_K_M.gguf",
            "revision": "a".repeat(40),
            "sha256": "b".repeat(64),
            "size": 100,
            "role": "chat",
            "about": "A model",
            "memory": 200,
        })
    }

    fn mac(memory: u64) -> Machine {
        Machine {
            os: "macos".to_owned(),
            arch: "aarch64".to_owned(),
            memory,
            gpu: Gpu::Unified,
        }
    }

    /// The trained models arrive this way: one more entry, and no code.
    #[test]
    fn given_a_pinned_entry_then_it_is_a_model() {
        let text = json!([entry()]).to_string();
        let models = parse_catalog(&text);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].role, Role::Chat);
        assert_eq!(
            models[0].url("https://huggingface.co"),
            format!(
                "https://huggingface.co/owner/model-GGUF/resolve/{}/model-Q4_K_M.gguf",
                "a".repeat(40)
            )
        );
    }

    #[test]
    fn given_an_entry_that_is_not_pinned_then_it_is_left_out() {
        for (key, bad) in [
            ("revision", json!("main")),
            ("revision", json!("abc123")),
            ("sha256", json!("")),
            ("file", json!("model.safetensors")),
            ("file", json!("--local-dir=x.gguf")),
            ("file", json!("../model.gguf")),
            ("repo", json!("no-owner")),
            ("repo", json!("a/b/c")),
            ("size", json!(0)),
        ] {
            let mut model = entry();
            model[key] = bad.clone();
            let text = json!([model]).to_string();
            assert_eq!(parse_catalog(&text), [], "{key} {bad}");
        }
    }

    #[test]
    fn the_shipped_list_is_fully_pinned_and_has_both_roles() {
        let models = catalog();
        let shipped: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("catalog.json")).expect("json");

        assert_eq!(models.len(), shipped.len());
        assert!(models.iter().any(|model| model.role == Role::Chat));
        assert!(models.iter().any(|model| model.role == Role::Decision));
    }

    fn picked(memory: u64) -> (String, String, bool) {
        let (chat, decision, fits) =
            pick(&catalog(), &mac(memory)).expect("a pair");
        (chat.file, decision.file, fits)
    }

    #[test]
    fn given_memory_then_the_largest_pair_that_fits_is_picked() {
        let (chat, decision, fits) = picked(96 * GB);
        assert!(chat.contains("7b") && decision.contains("4B") && fits);

        let (chat, decision, fits) = picked(16 * GB);
        assert!(chat.contains("3b") && decision.contains("4B") && fits);

        let (chat, decision, fits) = picked(8 * GB);
        assert!(chat.contains("3b") && decision.contains("0.8B") && fits);

        let (_, _, fits) = picked(2 * GB);
        assert!(!fits);
    }

    #[test]
    fn given_an_empty_list_then_nothing_is_picked() {
        assert_eq!(pick(&[], &mac(96 * GB)), None);
    }

    fn model() -> HubModel {
        parse_catalog(&json!([entry()]).to_string()).remove(0)
    }

    #[test]
    fn given_a_model_then_the_download_names_the_file_and_revision() {
        assert_eq!(
            download_command(Path::new("/bin/hf"), &model()),
            [
                "/bin/hf".to_owned(),
                "download".to_owned(),
                "owner/model-GGUF".to_owned(),
                "model-Q4_K_M.gguf".to_owned(),
                "--revision".to_owned(),
                "a".repeat(40),
            ]
        );
    }

    #[test]
    fn given_a_model_then_the_serve_command_uses_the_file_and_localhost() {
        let command = serve_command(
            Path::new("/bin/llama-server"),
            &model(),
            Path::new("/cache/model.gguf"),
            8081,
        );

        assert_eq!(command[1..3], ["--model", "/cache/model.gguf"]);
        assert_eq!(command[5..7], ["--host", "127.0.0.1"]);
        assert!(!command.contains(&"-hf".to_owned()));
    }

    #[test]
    fn given_sizes_then_they_read_in_gb_or_mb() {
        assert_eq!(human_size(4483801504), "4.5 GB");
        assert_eq!(human_size(689872288), "690 MB");
    }
}
