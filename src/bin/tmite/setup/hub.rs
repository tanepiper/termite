//! Models suggested from the Hugging Face Hub, and the commands that
//! download or serve them. Downloads go to the normal Hugging Face cache,
//! where llama.cpp (`-hf`) and the Llama app find them too.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use super::discover::ModelRole;

/// A GGUF model on the Hugging Face Hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HubModel {
    pub(crate) repo: &'static str,
    pub(crate) quant: &'static str,
    pub(crate) role: ModelRole,
    pub(crate) about: &'static str,
}

impl HubModel {
    /// The name llama.cpp uses for it with `-hf`, and that its router lists
    /// it under: `repo:quant`.
    pub(crate) fn id(&self) -> String {
        format!("{}:{}", self.repo, self.quant)
    }
}

/// The decision model termite is tuned for.
pub(crate) const DECISION_MODEL: HubModel = HubModel {
    repo: "ggml-org/Kev-4B-GGUF",
    quant: "Q8_0",
    role: ModelRole::Decision,
    about: "Kev-4B, a decision model for llama.cpp /v1/systemone",
};

/// A small chat model that is good at shell commands.
pub(crate) const CHAT_MODEL: HubModel = HubModel {
    repo: "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF",
    quant: "Q4_K_M",
    role: ModelRole::Chat,
    about: "Qwen2.5 Coder 7B Instruct",
};

/// The same chat model for Ollama.
pub(crate) const OLLAMA_CHAT_MODEL: &str = "qwen2.5-coder:7b";

/// A file in a Hub repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HubFile {
    pub(crate) path: String,
    pub(crate) size: u64,
}

/// Pick the GGUF files for `quant` from a Hub tree listing. A repository
/// may hold the same quant as one file and as split parts; one file is
/// preferred. Vision projectors (`mmproj`) are left out.
pub(crate) fn pick_files(tree: &str, quant: &str) -> Vec<HubFile> {
    let Ok(entries) = serde_json::from_str::<Vec<TreeEntry>>(tree) else {
        return Vec::new();
    };
    let quant = quant.to_lowercase();

    let matches: Vec<HubFile> = entries
        .into_iter()
        .filter(|entry| {
            let name = entry.path.to_lowercase();
            name.ends_with(".gguf")
                && !name.starts_with("mmproj")
                && name.contains(&quant)
        })
        .map(|entry| HubFile {
            size: entry.lfs.map(|lfs| lfs.size).unwrap_or(entry.size),
            path: entry.path,
        })
        .collect();

    match matches.iter().find(|file| !is_split_part(&file.path)) {
        Some(single) => vec![single.clone()],
        None => matches,
    }
}

/// Look up the files for `model` on the Hub. `None` when the Hub cannot be
/// reached; the download then picks files by pattern instead.
pub(crate) async fn files(model: &HubModel) -> Option<Vec<HubFile>> {
    let url =
        format!("https://huggingface.co/api/models/{}/tree/main", model.repo);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let tree = client.get(url).send().await.ok()?.text().await.ok()?;
    let files = pick_files(&tree, model.quant);

    (!files.is_empty()).then_some(files)
}

/// The command that downloads `model` into the Hugging Face cache, using
/// the `hf` CLI (or the older `huggingface-cli`). Exact file names are used
/// when known, a pattern otherwise.
pub(crate) fn download_command(
    downloader: &Path,
    model: &HubModel,
    files: Option<&[HubFile]>,
) -> Vec<String> {
    let mut command = vec![
        downloader.display().to_string(),
        "download".to_owned(),
        model.repo.to_owned(),
    ];

    match files {
        Some(files) => {
            command.extend(files.iter().map(|file| file.path.clone()))
        },
        None => command.extend([
            "--include".to_owned(),
            format!("*{}*.gguf", model.quant),
        ]),
    }

    command
}

/// The command that serves `model` with `llama-server`, downloading it
/// first if it is not in the cache yet. Bound to this machine only.
pub(crate) fn serve_command(
    llama_server: &Path,
    model: &HubModel,
    port: u16,
) -> Vec<String> {
    vec![
        llama_server.display().to_string(),
        "-hf".to_owned(),
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

fn is_split_part(path: &str) -> bool {
    path.to_lowercase().contains("-of-0")
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    size: u64,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{
        CHAT_MODEL, DECISION_MODEL, HubFile, download_command, human_size,
        pick_files, serve_command,
    };

    fn tree() -> String {
        json!([
            { "type": "file", "path": "README.md", "size": 100 },
            { "type": "file", "path": "qwen2.5-coder-7b-instruct-q4_k_m-00001-of-00002.gguf", "size": 3993201376u64 },
            { "type": "file", "path": "qwen2.5-coder-7b-instruct-q4_k_m-00002-of-00002.gguf", "size": 689872288 },
            { "type": "file", "path": "qwen2.5-coder-7b-instruct-q4_k_m.gguf", "size": 1, "lfs": { "size": 4683073536u64 } },
            { "type": "file", "path": "qwen2.5-coder-7b-instruct-q8_0.gguf", "size": 8098525184u64 },
            { "type": "file", "path": "mmproj-q4_k_m.gguf", "size": 600000000 },
        ])
        .to_string()
    }

    #[test]
    fn given_single_and_split_files_then_the_single_file_is_picked() {
        assert_eq!(
            pick_files(&tree(), "Q4_K_M"),
            vec![HubFile {
                path: "qwen2.5-coder-7b-instruct-q4_k_m.gguf".into(),
                size: 4683073536,
            }]
        );
    }

    #[test]
    fn given_only_split_files_then_every_part_is_picked() {
        let tree = json!([
            { "path": "big-q6_k-00001-of-00002.gguf", "size": 2 },
            { "path": "big-q6_k-00002-of-00002.gguf", "size": 3 },
        ])
        .to_string();

        let files = pick_files(&tree, "Q6_K");

        assert_eq!(files.len(), 2);
        assert_eq!(files.iter().map(|file| file.size).sum::<u64>(), 5);
    }

    #[test]
    fn given_no_match_or_bad_json_then_no_files_are_picked() {
        assert!(pick_files(&tree(), "IQ2_XS").is_empty());
        assert!(pick_files("not json", "Q8_0").is_empty());
    }

    #[test]
    fn given_known_files_then_the_download_names_them() {
        let files = [HubFile {
            path: "Kev-4B-Q8_0.gguf".into(),
            size: 4483801504,
        }];

        assert_eq!(
            download_command(
                Path::new("/bin/hf"),
                &DECISION_MODEL,
                Some(&files)
            ),
            [
                "/bin/hf",
                "download",
                "ggml-org/Kev-4B-GGUF",
                "Kev-4B-Q8_0.gguf"
            ]
        );
        assert_eq!(
            download_command(Path::new("/bin/hf"), &CHAT_MODEL, None),
            [
                "/bin/hf",
                "download",
                "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF",
                "--include",
                "*Q4_K_M*.gguf",
            ]
        );
    }

    #[test]
    fn given_a_model_then_the_serve_command_uses_hf_and_localhost() {
        assert_eq!(
            serve_command(
                Path::new("/bin/llama-server"),
                &DECISION_MODEL,
                8081
            ),
            [
                "/bin/llama-server",
                "-hf",
                "ggml-org/Kev-4B-GGUF:Q8_0",
                "--host",
                "127.0.0.1",
                "--port",
                "8081",
            ]
        );
    }

    #[test]
    fn given_sizes_then_they_read_in_gb_or_mb() {
        assert_eq!(human_size(4483801504), "4.5 GB");
        assert_eq!(human_size(689872288), "690 MB");
    }
}
