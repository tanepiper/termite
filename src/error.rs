//! Errors from talking to the local model and the decision endpoint.

use std::fmt;

use thiserror::Error;

/// The outside service an error came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// The local model (`llama-server`).
    Model,
    /// The decision model (llama.cpp `/v1/systemone`).
    Decision,
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Service::Model => f.write_str("the local model"),
            Service::Decision => f.write_str("the decision model"),
        }
    }
}

/// Why an answer could not be produced.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssistError {
    /// The config file is missing or not valid.
    #[error("config: {detail}")]
    Config {
        /// What is wrong, with the file path.
        detail: String,
    },
    /// The service did not answer within its time limit.
    #[error("{service} did not answer in time")]
    Timeout {
        /// The service that timed out.
        service: Service,
    },
    /// The service could not be reached.
    #[error("{service} could not be reached: {detail}")]
    Connection {
        /// The service that could not be reached.
        service: Service,
        /// The underlying error.
        detail: String,
    },
    /// The service answered with an error status.
    #[error("{service} answered with HTTP {status}")]
    Status {
        /// The service that answered.
        service: Service,
        /// The HTTP status code.
        status: u16,
    },
    /// The reply was not the expected JSON.
    #[error("{service} sent a reply that is not valid: {detail}")]
    InvalidReply {
        /// The service that answered.
        service: Service,
        /// What was wrong with it.
        detail: String,
    },
    /// The model said it will not do this, and wrote no commands.
    #[error("the local model declined: {reason}")]
    ModelDeclined {
        /// The model's own words. Treat as untrusted text.
        reason: String,
    },
    /// The model returned no commands, or more than five.
    #[error("the local model returned {count} commands; 1 to 5 are expected")]
    WrongCandidateCount {
        /// How many it returned.
        count: usize,
    },
    /// The ranking named a command that was not sent, or named one twice.
    #[error(
        "the decision endpoint ranked an unknown or repeated command `{id}`"
    )]
    UnknownCandidate {
        /// The id it named.
        id: String,
    },
    /// The ranking was empty.
    #[error("the decision model ranked no commands")]
    EmptyRanking,
    /// A risk label was not one of the known values.
    #[error("{service} sent an unknown risk `{value}`")]
    UnknownRisk {
        /// The service that sent it.
        service: Service,
        /// The value it sent.
        value: String,
    },
}

impl AssistError {
    /// Turn a `reqwest` error into a typed error for `service`.
    pub(crate) fn from_http(service: Service, error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            return AssistError::Timeout { service };
        }

        match error.status() {
            Some(status) => AssistError::Status {
                service,
                status: status.as_u16(),
            },
            None => AssistError::Connection {
                service,
                detail: error.to_string(),
            },
        }
    }

    pub(crate) fn invalid(service: Service, detail: impl ToString) -> Self {
        AssistError::InvalidReply {
            service,
            detail: detail.to_string(),
        }
    }
}
