use std::sync::Arc;

use axum::{response::Html, Extension, Json};
use tempdir::TempDir;
use tracing::{debug, info, instrument};

use crate::cli::Args;
use crate::error::{ProcessingError, Result};
use crate::payload::{CommitStats, Payload, PushRepository};
use crate::status::{DeathReason, Status};
use crate::util::{assert_gpg_directory, clone_repository, verify_commit, KeyringDirs};

#[instrument(skip(args, payload))]
async fn handle_push(
    args: Extension<Arc<Args>>,
    keyring_dirs: Extension<Arc<KeyringDirs>>,
    payload: Payload,
) -> Result<Status> {
    if let Payload::Push {
        _ref,
        commits,
        repository,
        ..
    } = payload
    {
        // Determine whether the push was for a tag or a branch by checking if `ref` starts
        // with an identifier for either, and depending on those options, return a command and
        // optional keyring
        let (command, keyring_path) = if _ref.starts_with("refs/heads/") {
            // This is a commit pushed to a branch
            match &**args {
                // This double deref seems dangerous. Trusting the compiler.
                Args {
                    commit_command: Some(command),
                    ..
                } => (command, &keyring_dirs.commit),
                _ => return Ok(Status::Death(DeathReason::NoCommandConfiguration)),
            }
        } else if _ref.starts_with("refs/tags/") {
            // This is a commit pushed to a tag
            match &**args {
                // This double deref seems dangerous. Trusting the compiler.
                Args {
                    tag_command: Some(command),
                    ..
                } => (command, &keyring_dirs.tag),
                _ => return Ok(Status::Death(DeathReason::NoCommandConfiguration)),
            }
        } else {
            return Err(ProcessingError::BadCommitRef {
                _ref: _ref.to_string(),
            });
        };
        debug!(?command, ?keyring_path, "determined operation to run");

        let repository_url = args
            .git_repository
            .as_ref()
            .unwrap_or(&repository.clone_url);
        let repository_directory = clone_repository(repository_url, args.clone_timeout).await?;

        // Rebind keyring path to unwrap the Option<_>
        if let Some(keyring_path) = keyring_path {
            // Keyring directory exists via TempDir
            let commit = commits.last().expect("no commits were pushed");
            verify_commit(
                commit.id.as_str(),
                repository_directory.path(),
                keyring_path.path(),
            )
            .await?;
        }

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
    keyring_dirs: Extension<Arc<KeyringDirs>>,
    Json(payload): Json<Payload>,
) -> Result<Json<Status>> {
    match payload {
        Payload::Push { .. } => {
            return Ok(Json(handle_push(args, keyring_dirs, payload).await?));
        }
        _ => {}
    }
    Ok(Json(Status::Life))
}
