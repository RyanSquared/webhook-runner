use thiserror::Error;
use std::fmt::Display;

use axum::{response::{Response, IntoResponse}, http::StatusCode, body::Body};

#[derive(Error, Debug)]
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
    Command {
        exit_code: i32,
    },
}

impl IntoResponse for ProcessingError {
    fn into_response(self) -> Response {
        let body = format!("{}", self);

        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}
