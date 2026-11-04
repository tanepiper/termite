//! The config file shared by `tmite` and `otty-assist`:
//! `config.json` in termite's folder (default `~/.config/termite`).

use std::path::PathBuf;
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
}

impl Config {
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

    /// Parse config JSON.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// The decision endpoint token from [`DECISION_TOKEN_VAR`], if set and not
/// empty.
pub fn decision_token() -> Option<String> {
    env::var(DECISION_TOKEN_VAR)
        .ok()
        .filter(|token| !token.is_empty())
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
    fn given_config_with_a_token_field_when_parsed_then_it_is_refused() {
        let result = Config::from_json(
            r#"{"model_url":"a","model":"b","decision_url":"c","decision_model":"d","decision_token":"secret"}"#,
        );

        assert!(result.is_err());
    }
}
