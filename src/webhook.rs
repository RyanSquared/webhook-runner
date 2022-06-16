use std::process::Command;
use std::sync::Arc;

use axum::{
    response::Html,
    Json, Extension,
};
use tokio::task;

use crate::payload::Payload;
use crate::cli::Args;
use crate::error::ProcessingError;

/// Receive a webhook from a GitHub server indicating a change in code, match upon an event, and
/// dispatch the JSON blob to a configured script.
pub async fn webhook(args: Extension<Arc<Args>>, Json(payload): Json<Payload>)
        -> Result<Html<&'static str>, ProcessingError> {
    // TODO(RyanSquared): Tracing does not log here. I am unsure why.
    // TODO(RyanSquared): CORRECTION TRACING SOMETIMES WORKS HERE???
    tracing::info!("hello!");
    let mut c = Command::new(&args.command);
    c.args(&args.arguments);
    let res = task::spawn_blocking(move || {
        match c.status() {
            Ok(xs) => match xs.code() {
                Some(0) => Ok(()),
                Some(n) => Err(ProcessingError::Command { exit_code: n }),
                None => Ok(()), // don't err on the side of caution?
            },
            Err(e) => Err(ProcessingError::Io{ source: e }),
        }
    }).await??; // NOTE(RyanSquared): I don't know how to optimize this.
    Ok(Html::<&'static str>("hi"))
}
