use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use std::fmt::Display;
use std::process::ExitStatus;
use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, ProcessingError>;

#[derive(Error, Debug)]
pub(crate) enum ProcessingError {
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

    #[error("no commits were found")]
    NoCommitsFound,

    #[error("bad ref in commit push: {_ref}")]
    BadCommitRef { _ref: String },

    #[error("timeout expired: {timeout}")]
    Timeout {
        #[from]
        timeout: tokio::time::error::Elapsed,
    },

    #[error("the integrity of the git repository was compromised")]
    RepositoryIntegrity,
}

impl ProcessingError {
    /// Assert from exit status
    pub(crate) fn assert_exit_status(xs: ExitStatus) -> Result<ExitStatus> {
        if let Some(n) = xs.code() {
            if n != 0 {
                return Err(ProcessingError::Command { exit_code: n });
            }
        }
        // Either an exit code was zero or (unlikely) didn't exist
        Ok(xs)
    }
}

impl IntoResponse for ProcessingError {
    fn into_response(self) -> Response {
        let body = format!("{}", self);

        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}
