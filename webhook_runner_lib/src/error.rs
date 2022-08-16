use thiserror::Error;

pub type Result<T> = std::result::Result<T, ProcessingError>;

#[derive(Error, Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum ProcessingError {
    #[error("thread was unable to join: {source}")]
    Join {
        #[from]
        source: tokio::task::JoinError,
    },

    #[error("io error while running command: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    #[error("process returned nonzero exit code: {exit_code}")]
    Command { exit_code: i32 },

    #[error("timeout expired: {timeout}")]
    Timeout {
        #[from]
        timeout: tokio::time::error::Elapsed,
    },

    #[error("the ref we're on ({actual}) is not the ref we expect: ({expected})")]
    RepositoryIntegrity { actual: String, expected: String },

    #[error("performing git operation on repository failed: {source}")]
    GitOperation {
        #[from]
        source: git2::Error,
    },

    #[error("loading openpgp certificates from file failed: {source}")]
    InvalidKeyringFile { source: anyhow::Error },

    #[error("parsing gpgsig header as signature failed: {source}")]
    MalformedSignature { source: anyhow::Error },

    #[error("verifying gpgsig header failed: {source}")]
    InvalidSignature { source: anyhow::Error },
}
