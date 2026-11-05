//! Frees a model's memory as soon as eval is done with it. Servers otherwise
//! keep a model loaded for minutes, and the next one then loads beside it. A
//! failed unload is reported, and the evaluation carries on.

use std::time::Duration;

use serde_json::{Value, json};

use crate::types::ModelSpec;

const TIME_LIMIT: Duration = Duration::from_secs(30);

/// Unload `spec`'s model. A model reached through a socket lives in a job that
/// ends with the run, so there's nothing to do.
pub fn release(spec: &ModelSpec) {
    if spec.socket.is_none() {
        release_model(&spec.url, &spec.model);
    }
}

/// Unload a model from llama.cpp or Ollama, whichever is at `url`.
pub fn release_model(url: &str, model: &str) {
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| runtime.block_on(unload(url, model)));
    if let Err(problem) = result {
        println!("  {model} is still in memory: {problem}");
    }
}

async fn post(
    client: &reqwest::Client,
    url: &str,
    body: &Value,
) -> Result<u16, String> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    Ok(response.status().as_u16())
}

async fn unload(url: &str, model: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(TIME_LIMIT)
        .build()
        .map_err(|error| error.to_string())?;
    let url = url.trim_end_matches('/');

    // llama.cpp answers 400 when the model isn't loaded, which is fine.
    let llama = post(
        &client,
        &format!("{url}/models/unload"),
        &json!({ "model": model }),
    )
    .await?;
    if llama != 404 {
        return match llama {
            200..=299 | 400 => Ok(()),
            status => Err(format!("the server said {status}")),
        };
    }

    // Ollama has no such route. A keep_alive of 0 unloads at once.
    let ollama = post(
        &client,
        &format!("{url}/api/generate"),
        &json!({ "model": model, "keep_alive": 0 }),
    )
    .await?;
    match ollama {
        200..=299 => Ok(()),
        status => Err(format!("the server said {status}")),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use super::unload;

    /// A server giving `statuses` in turn. Returns its URL and what it heard.
    fn server(statuses: &[u16]) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&heard);
        let statuses = statuses.to_vec();
        thread::spawn(move || {
            for status in statuses {
                let (mut stream, _) = listener.accept().expect("accepts");
                let mut bytes = [0_u8; 4096];
                let read = stream.read(&mut bytes).expect("reads");
                let text = String::from_utf8_lossy(&bytes[..read]).into_owned();
                log.lock().expect("log").push(text);
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(reply.as_bytes()).expect("writes");
            }
        });
        (url, heard)
    }

    fn try_unload(url: &str) -> Result<(), String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(unload(url, "m"))
    }

    #[test]
    fn given_llama_cpp_then_one_call_unloads_it() {
        let (url, heard) = server(&[200]);

        assert_eq!(try_unload(&url), Ok(()));

        let heard = heard.lock().expect("log");
        assert_eq!(heard.len(), 1);
        assert!(heard[0].starts_with("POST /models/unload"));
    }

    #[test]
    fn given_a_model_that_is_not_loaded_then_that_is_fine() {
        let (url, _) = server(&[400]);

        assert_eq!(try_unload(&url), Ok(()));
    }

    #[test]
    fn given_ollama_then_keep_alive_0_unloads_it() {
        let (url, heard) = server(&[404, 200]);

        assert_eq!(try_unload(&url), Ok(()));

        let heard = heard.lock().expect("log");
        assert!(heard[1].starts_with("POST /api/generate"));
        assert!(heard[1].contains("\"keep_alive\":0"), "{}", heard[1]);
    }

    #[test]
    fn given_a_server_error_then_it_is_reported() {
        let (url, _) = server(&[500]);

        assert!(try_unload(&url).is_err());
    }
}
