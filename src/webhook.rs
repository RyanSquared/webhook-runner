use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

use axum::{response::Html, Extension, Json};
use tempdir::TempDir;
use tokio::task;

use tracing::{debug, info, instrument};

use crate::cli::Args;
use crate::error::ProcessingError;
use crate::payload::{CommitStats, Payload, PushRepository};
use crate::status::{DeathReason, Status};

type Result<T> = std::result::Result<T, ProcessingError>;

async fn clone_repository(
    args: Extension<Arc<Args>>,
    commit: &CommitStats,
    repository: &PushRepository,
) -> Result<TempDir> {
    // Create a temporary directory for cloning the Git repository into, based on the
    // name of the current commit
    let tmp_dir =
        TempDir::new(format!("webhook-runner-{commit}", commit = commit.id.as_str()).as_ref())?;
    debug!(directory = ?tmp_dir.path(), "creating new directory to clone git repository");

    // Run the command to clone into the Git repository, capturing output into a pipe
    let mut clone_process = tokio::process::Command::new("git")
        .arg("clone")
        .arg("--recursive")
        .arg(
            args.git_repository
                .as_ref()
                .unwrap_or(&repository.clone_url),
        )
        .arg(tmp_dir.path())
        .stderr(Stdio::piped())
        .spawn()?;

    // Return errors depending on if a timeout was hit or a nonzero exit code was reached
    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(args.clone_timeout.into()),
        clone_process.wait_with_output(),
    )
    .await?;
    let result = timeout?;
    debug!(exit_status = ?result.status, "command has completed");
    ProcessingError::assert_exit_status(result.status)?;

    // Print the output of the command
    let clone_output = result.stderr;
    let mut lines = clone_output.lines();
    while let Some(line) = lines.next_line().await? {
        debug!("`git clone`: {}", line);
    }

    Ok(tmp_dir)
}

async fn handle_push(args: Extension<Arc<Args>>, payload: Payload) -> Result<Status> {
    if let Payload::Push {
        _ref,
        commits,
        repository,
        ..
    } = payload
    {
        let last_commit = commits.last().ok_or(ProcessingError::NoCommitsFound)?;
        debug!(commit = ?last_commit.id.as_str(), "determined head commit");

        // Determine whether the push was for a tag or a branch by checking if `ref` starts
        // with an identifier for either, and depending on those options, return a command and
        // optional keyring
        let (command, keyring_path) = if _ref.starts_with("refs/heads/") {
            // This is a commit pushed to a branch
            match &**args {
                // This double deref seems dangerous. Trusting the compiler.
                Args {
                    commit_keyring: keyring,
                    commit_command: Some(command),
                    ..
                } => (command, keyring),
                Args {
                    commit_keyring: Some(_),
                    commit_command: None,
                    ..
                } => {
                    unreachable!("a keyring was configured but a command was not")
                }
                _ => return Ok(Status::Death(DeathReason::NoCommandConfiguration)),
            }
        } else if _ref.starts_with("refs/tags/") {
            // This is a commit pushed to a tag
            match &**args {
                // This double deref seems dangerous. Trusting the compiler.
                Args {
                    tag_keyring: keyring,
                    tag_command: Some(command),
                    ..
                } => (command, keyring),
                Args {
                    tag_keyring: Some(_),
                    tag_command: None,
                    ..
                } => {
                    unreachable!("a keyring was configured but a command was not")
                }
                _ => return Ok(Status::Death(DeathReason::NoCommandConfiguration)),
            }
        } else {
            return Err(ProcessingError::BadCommitRef {
                _ref: _ref.to_string(),
            })
        };
        debug!(?command, ?keyring_path, "determined operation to run");

        let repository_directory = clone_repository(args, last_commit, &repository).await?;

        Ok(Status::Life)
    } else {
        panic!("must be called with Payload::Push value")
    }
}

/// Receive a webhook from a GitHub server indicating a change in code, match upon an event, and
/// dispatch the JSON blob to a configured script.
#[instrument(skip_all)]
pub(crate) async fn webhook(
    args: Extension<Arc<Args>>,
    Json(payload): Json<Payload>,
) -> Result<Json<Status>> {
    // TODO(RyanSquared): Implement battle plan for matching tags/releases and commits being pushed
    info!("received webhook from server: {payload:?}");
    match payload {
        Payload::Push { .. } => {
            return Ok(Json(handle_push(args, payload).await?));
        }
        _ => {}
    }
    Ok(Json(Status::Life))
}
