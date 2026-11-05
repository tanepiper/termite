//! The config file shared by `tmite` and `otty-assist`:
//! `config.json` in termite's folder (default `~/.config/termite`).

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs};

use serde::Deserialize;

use crate::error::AssistError;
use crate::home::Home;

/// The environment variable holding the decision endpoint's bearer token.
/// The token is never read from the config file.
pub const DECISION_TOKEN_VAR: &str = "TERMITE_DECISION_TOKEN";

/// Where the two services are and how long to wait for them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    model_url: String,
    model: String,
    decision_url: String,
    decision_model: String,
    #[serde(default = "default_generation_timeout_secs")]
    generation_timeout_secs: u64,
    #[serde(default = "default_decision_timeout_secs")]
    decision_timeout_secs: u64,
    #[serde(default = "default_stream")]
    stream: bool,
    #[serde(default)]
    script_model: Option<String>,
    #[serde(default = "default_script_timeout_secs")]
    script_timeout_secs: u64,
    /// A Unix socket to reach the servers through, for a place where TCP is
    /// blocked. The URLs still name them.
    #[serde(default)]
    model_socket: Option<PathBuf>,
    /// The same, for the decision server.
    #[serde(default)]
    decision_socket: Option<PathBuf>,
}

impl Config {
    /// The Unix socket for the decision server, if any.
    pub fn decision_socket(&self) -> Option<&Path> {
        self.decision_socket.as_deref()
    }

    /// A client for the model server.
    pub(crate) fn model_client(&self) -> reqwest::ClientBuilder {
        client_through(self.model_socket.as_deref())
    }

    /// A client for the decision server.
    pub(crate) fn decision_client(&self) -> reqwest::ClientBuilder {
        client_through(self.decision_socket.as_deref())
    }

    /// Base URL of `llama-server`, such as `http://127.0.0.1:8080`.
    pub fn model_url(&self) -> &str {
        &self.model_url
    }

    /// Model name sent with each request.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Base URL of the llama.cpp server with the decision model.
    pub fn decision_url(&self) -> &str {
        &self.decision_url
    }

    /// Name of the decision model, such as `ggml-org/Kev-4B-GGUF:Q8_0`.
    pub fn decision_model(&self) -> &str {
        &self.decision_model
    }

    /// Time limit for generating commands (default 20 s).
    pub fn generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_secs)
    }

    /// Time limit for each decision call (default 5 s).
    pub fn decision_timeout(&self) -> Duration {
        Duration::from_secs(self.decision_timeout_secs)
    }

    /// The model that writes scripts (default [`Config::model`]).
    pub fn script_model(&self) -> &str {
        self.script_model.as_deref().unwrap_or(&self.model)
    }

    /// Time limit for writing a script (default 300 s).
    pub fn script_timeout(&self) -> Duration {
        Duration::from_secs(self.script_timeout_secs)
    }

    /// Whether to ask the chat model to stream its reply (default yes).
    /// Off, the reply comes in one piece; the wait looks the same apart
    /// from the running character count.
    pub fn stream(&self) -> bool {
        self.stream
    }

    /// Where the config file lives: `config.json` in [`Home`]
    /// (`$TERMITE_HOME`, default `~/.config/termite`).
    pub fn path() -> Result<PathBuf, AssistError> {
        Home::from_env()
            .map(|home| home.config_file())
            .map_err(|error| AssistError::Config {
                detail: error.to_string(),
            })
    }

    /// Read the config file from [`Config::path`].
    pub fn load() -> Result<Self, AssistError> {
        let path = Self::path()?;

        let text =
            fs::read_to_string(&path).map_err(|error| AssistError::Config {
                detail: format!("cannot read {}: {error}", path.display()),
            })?;

        Self::from_json(&text).map_err(|error| AssistError::Config {
            detail: format!("{}: {error}", path.display()),
        })
    }

    /// Parse config JSON. Each URL must be `https`, or plain `http` to this
    /// machine only: requests carry what you typed, your current folder and
    /// the decision token, which must not cross a network in the clear.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let config: Self = serde_json::from_str(text)?;
        for (name, url) in [
            ("model_url", &config.model_url),
            ("decision_url", &config.decision_url),
        ] {
            check_url(url).map_err(|problem| {
                serde::de::Error::custom(format!("{name} {problem}"))
            })?;
        }
        Ok(config)
    }
}

fn client_through(socket: Option<&Path>) -> reqwest::ClientBuilder {
    let client = reqwest::Client::builder();
    match socket {
        Some(socket) => client.unix_socket(socket),
        None => client,
    }
}

/// The decision endpoint token from [`DECISION_TOKEN_VAR`], if set and not
/// empty.
pub fn decision_token() -> Option<String> {
    env::var(DECISION_TOKEN_VAR)
        .ok()
        .filter(|token| !token.is_empty())
}

/// `https` anywhere, or `http` to a loopback address; never a user name or
/// password inside the URL.
fn check_url(url: &str) -> Result<(), &'static str> {
    let url = reqwest::Url::parse(url).map_err(|_| "is not a URL")?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not hold a user name or password");
    }
    let host = url.host_str().ok_or("has no host")?;
    let address = host.trim_start_matches('[').trim_end_matches(']');
    let loopback = host.eq_ignore_ascii_case("localhost")
        || address
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    match url.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        "http" => Err("uses plain http to another machine; use https"),
        _ => Err("must be http or https"),
    }
}

fn default_script_timeout_secs() -> u64 {
    300
}

fn default_stream() -> bool {
    true
}

fn default_generation_timeout_secs() -> u64 {
    20
}

fn default_decision_timeout_secs() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Config;

    #[test]
    fn given_config_without_timeouts_when_parsed_then_defaults_are_used() {
        let config = Config::from_json(
            r#"{"model_url":"http://127.0.0.1:8080","model":"qwen2.5-coder-7b","decision_url":"http://127.0.0.1:9000","decision_model":"kev"}"#,
        )
        .expect("config should parse");

        assert_eq!(config.generation_timeout(), Duration::from_secs(20));
        assert_eq!(config.decision_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn given_urls_when_parsed_then_only_https_or_local_http_is_accepted() {
        let config = |model_url: &str, decision_url: &str| {
            Config::from_json(&format!(
                r#"{{"model_url":"{model_url}","model":"m","decision_url":"{decision_url}","decision_model":"d"}}"#
            ))
        };
        let local = "http://127.0.0.1:8080";
        for good in [
            "http://127.0.0.1:9000",
            "http://localhost:11434",
            "http://LOCALHOST:1",
            "http://[::1]:8080",
            "http://127.1.2.3:8080",
            "https://models.example.com",
        ] {
            assert!(config(local, good).is_ok(), "{good}");
            assert!(config(good, local).is_ok(), "{good}");
        }
        for bad in [
            "http://192.168.1.20:8080",
            "http://models.example.com",
            "http://localhost.example.com",
            "https://user:pass@models.example.com",
            "ftp://127.0.0.1",
            "file:///tmp/socket",
            "not a url",
        ] {
            assert!(config(local, bad).is_err(), "{bad}");
            assert!(config(bad, local).is_err(), "{bad}");
        }
    }

    #[test]
    fn given_config_with_a_token_field_when_parsed_then_it_is_refused() {
        let result = Config::from_json(
            r#"{"model_url":"a","model":"b","decision_url":"c","decision_model":"d","decision_token":"secret"}"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn given_urls_then_only_https_or_this_machine_is_accepted() {
        for good in [
            "http://127.0.0.1:8080",
            "http://localhost:8080",
            "http://LOCALHOST",
            "http://[::1]:8080",
            "https://models.example.com",
        ] {
            assert_eq!(super::check_url(good), Ok(()), "{good}");
        }
        for bad in [
            "http://example.com",
            "http://192.168.1.2:8080",
            "http://127.0.0.1.example.com",
            "http://localhost.example.com",
            "ftp://127.0.0.1",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://user@example.com",
            "not a url",
            "",
        ] {
            assert!(super::check_url(bad).is_err(), "{bad}");
        }
    }
}
