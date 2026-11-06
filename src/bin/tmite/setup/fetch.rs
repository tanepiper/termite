//! Fetches a model file. Contract: the file must match its pinned size and
//! hash before it gets its name, so a part-download or a changed file is
//! never served. Runs only after the person has said yes.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use termite::create_private;

use super::hub::{HubModel, human_size};

/// Where model files come from.
pub(crate) const HUB: &str = "https://huggingface.co";

/// How long the Hub may stay silent before the download is given up.
const STALLED: Duration = Duration::from_secs(60);

/// The longest a whole download may take, on the slowest line worth using.
const WHOLE_DOWNLOAD: Duration = Duration::from_secs(6 * 60 * 60);

/// Where a model's file lives: named by its hash, so two revisions never mix.
pub(crate) fn file_path(folder: &Path, model: &HubModel) -> PathBuf {
    folder.join(format!("{}-{}", &model.sha256[..16], model.file))
}

/// The hash of a file, as hex.
fn hash_of(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Make sure `model` is in `folder`, fetching it from `base` if needed.
/// Returns the file's path.
pub(crate) async fn fetch(
    model: &HubModel,
    folder: &Path,
    base: &str,
) -> Result<PathBuf, String> {
    let path = file_path(folder, model);
    if path.exists() {
        println!("Checking the copy already at {}…", path.display());
        if hash_of(&path).is_ok_and(|hash| hash == model.sha256) {
            return Ok(path);
        }
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(folder).map_err(|error| error.to_string())?;

    let part_name = format!("{}.part", model.sha256);
    let part = folder.join(&part_name);
    let _ = fs::remove_file(&part);
    let file = create_private(folder, &part_name)
        .map_err(|error| error.to_string())?;
    let result = download(model, file, base).await;
    match result {
        Ok(()) => fs::rename(&part, &path)
            .map(|()| path)
            .map_err(|error| error.to_string()),
        Err(problem) => {
            let _ = fs::remove_file(&part);
            Err(problem)
        },
    }
}

async fn download(
    model: &HubModel,
    mut file: fs::File,
    base: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(WHOLE_DOWNLOAD)
        .read_timeout(STALLED)
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(model.url(base))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?;

    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut shown = 0;
    while let Some(chunk) =
        response.chunk().await.map_err(|error| error.to_string())?
    {
        received += chunk.len() as u64;
        if received > model.size {
            return Err("the download is larger than it should be".to_owned());
        }
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|error| error.to_string())?;
        let percent = received * 100 / model.size;
        if percent >= shown + 10 {
            shown = percent;
            println!("  {percent}% of {}", human_size(model.size));
        }
    }
    file.sync_all().map_err(|error| error.to_string())?;

    if received != model.size {
        return Err("the download stopped early".to_owned());
    }
    if hex(&hasher.finalize()) != model.sha256 {
        return Err("the download doesn't match its pinned hash".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;

    use sha2::{Digest, Sha256};

    use super::{fetch, file_path, hex};
    use crate::setup::hub::{HubModel, Role};

    fn model(body: &[u8]) -> HubModel {
        HubModel {
            repo: "owner/model".to_owned(),
            file: "model.gguf".to_owned(),
            revision: "a".repeat(40),
            sha256: hex(&Sha256::digest(body)),
            size: body.len() as u64,
            role: Role::Chat,
            about: "A model".to_owned(),
            memory: body.len() as u64,
        }
    }

    /// A server that answers one request with `body`, and the path asked for.
    fn serve(body: &'static [u8]) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("port");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = [0; 2048];
            let read = stream.read(&mut request).expect("read");
            let text = String::from_utf8_lossy(&request[..read]).into_owned();
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).expect("head");
            stream.write_all(body).expect("body");
            text.split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned()
        });
        (url, server)
    }

    fn folder(name: &str) -> PathBuf {
        // Test-only: nothing here is trusted, and it is removed afterwards.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let temp = std::env::temp_dir();
        let folder =
            temp.join(format!("termite-fetch-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    #[test]
    fn given_the_right_bytes_then_the_file_arrives_at_its_pinned_revision() {
        let model = model(b"weights");
        let (url, server) = serve(b"weights");
        let folder = folder("good");

        let path = block_on(fetch(&model, &folder, &url)).expect("fetched");

        assert_eq!(fs::read(&path).expect("file"), b"weights");
        assert_eq!(path, file_path(&folder, &model));
        let asked = server.join().expect("server");
        assert!(asked.contains(&"a".repeat(40)), "{asked}");
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn given_other_bytes_then_nothing_is_kept() {
        let model = model(b"weights");
        let (url, _server) = serve(b"WEIGHTS");
        let folder = folder("bad");

        let problem =
            block_on(fetch(&model, &folder, &url)).expect_err("refused");

        assert!(problem.contains("hash"), "{problem}");
        assert_eq!(fs::read_dir(&folder).expect("folder").count(), 0);
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn given_a_good_copy_then_nothing_is_downloaded() {
        let model = model(b"weights");
        let folder = folder("kept");
        fs::create_dir_all(&folder).expect("folder");
        let path = file_path(&folder, &model);
        let name = path.file_name().expect("name").to_string_lossy();
        termite::write_private(
            &folder,
            &name,
            b"weights",
            termite::PRIVATE_FILE_MODE,
        )
        .expect("file");

        let path = block_on(fetch(&model, &folder, "http://127.0.0.1:1"));

        assert!(path.is_ok());
        fs::remove_dir_all(folder).expect("clean up");
    }
}
