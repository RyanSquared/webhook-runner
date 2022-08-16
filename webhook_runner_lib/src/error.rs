/*
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
*/

use std::process::ExitStatus;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ProcessingError>;

/*
#[derive(Error, Debug)]
pub(crate) enum HeaderParseError {
    #[error("the http header value is not a valid str: {source}")]
    InvalidString {
        #[from]
        source: http::header::ToStrError,
    },

    #[error("the http header was malformed: {header}")]
    Content { header: String },

    #[error("header value for signature was incorrect size: {length} != {intended}")]
    Length { length: usize, intended: u32 },

    #[error("hex value was malformed: {source}")]
    HexDecode {
        #[from]
        source: hex::FromHexError,
    },
}
*/

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
    Command { exit_code: i32 },

    #[error("timeout expired: {timeout}")]
    Timeout {
        #[from]
        timeout: tokio::time::error::Elapsed,
    },

    #[error("the ref we're on ({actual}) is not the ref we expect: ({expected})")]
    RepositoryIntegrity {
        actual: String,
        expected: String,
    },

//  #[error("the http header could not be parsed: {0}")]
//  HeaderParse(#[from] HeaderParseError),

//  #[error("invalid length of hmac key: {source}")]
//  HmacKeyLength {
//      #[from]
//      source: crypto_common::InvalidLength,
//  },

//  #[error("hmac did not match expected: {source}")]
//  HmacVerification {
//      #[from]
//      source: digest::MacError,
//  },

    #[error("performing git operation on repository failed: {source}")]
    GitOperation {
        #[from]
        source: git2::Error,
    },

    #[error("loading openpgp certificates from file failed: {source}")]
    InvalidKeyringFile {
        source: anyhow::Error,
    },

    #[error("parsing gpgsig header as signature failed: {source}")]
    MalformedSignature {
        source: anyhow::Error,
    },

    #[error("verifying gpgsig header failed: {source}")]
    InvalidSignature {
        source: anyhow::Error,
    }
}

impl ProcessingError {
    /// Assert the program exited with an exit code of zero, assuming zero is a success case; if an
    /// exit code was unobtainable, don't err on the side of caution.
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

/*
impl IntoResponse for ProcessingError {
    fn into_response(self) -> Response {
        let body = format!("{}", self);

        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}
*/
