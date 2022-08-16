use std::sync::Arc;

use axum::{Extension, Json};
use git2::Oid;
use tracing::{debug, instrument};

use crate::cli::Args;
use crate::payload::Payload;
use crate::repository::{clone_repository, verify_commit};
use crate::status::DeathReason;
use crate::KeyringFiles;

#[instrument(skip_all)]
async fn handle_push(
    args: Extension<Arc<Args>>,
    keyring_files: Extension<Arc<KeyringFiles>>,
    payload: Payload,
) -> Result<(), DeathReason> {
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
        let (command, keyring_file) = if _ref.starts_with("refs/heads/") {
            // This is a commit pushed to a branch
            match &**args {
                // This double deref seems dangerous. Trusting the compiler.
                Args {
                    commit_command: Some(command),
                    ..
                } => (command, &keyring_files.commit),
                _ => return Ok(()),
            }
        } else if _ref.starts_with("refs/tags/") {
            // This is a commit pushed to a tag
            match &**args {
                // This double deref seems dangerous. Trusting the compiler.
                Args {
                    tag_command: Some(command),
                    ..
                } => (command, &keyring_files.tag),
                _ => return Ok(()),
            }
        } else {
            return Err(DeathReason::InvalidWebhook {
                field_path: "_ref".to_string(),
                value: Some(_ref.to_string()),
            });
        };
        debug!(?command, "determined operation to run");

        let commit = match commits.last() {
            Some(c) => c,
            None => {
                return Err(DeathReason::InvalidWebhook {
                    field_path: "commits".to_string(),
                    value: None,
                })
            }
        };
        let repository_url = args
            .git_repository
            .as_ref()
            .unwrap_or(&repository.clone_url);
        let ssh_key = args.ssh_key.as_ref();
        let (repository, repository_directory) = match clone_repository(
            repository_url,
            commit.id.as_str(),
            args.clone_timeout,
            ssh_key,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                return Err(DeathReason::FailedClone {
                    reason: e.to_string(),
                })
            }
        };

        // Rebind keyring path to unwrap the Option<_>
        if let Some(keyring_file) = keyring_file {
            let commit = {
                let oid = Oid::from_str(commit.id.as_str()).map_err(|e| {
                    DeathReason::RepositoryError {
                        reason: e.to_string(),
                    }
                })?;
                repository.find_commit(oid).map_err(|e| {
                    DeathReason::RepositoryError {
                        reason: e.to_string(),
                    }
                })?
            };

            // Keyring directory exists via TempDir
            let result = verify_commit(commit, keyring_file);
            result.map_err(|e| DeathReason::KeyringVerification {
                reason: e.to_string(),
            })?;
        }

        Ok(())
    } else {
        panic!("must be called with Payload::Push value")
    }
}

/// Receive a webhook from a GitHub server indicating a change in code, match upon an event, and
/// dispatch the JSON blob to a configured script.
#[instrument(skip_all)]
#[axum_macros::debug_handler]
pub(crate) async fn webhook(
    args: Extension<Arc<Args>>,
    keyring_dirs: Extension<Arc<KeyringFiles>>,
    Json(payload): Json<Payload>,
) -> Result<Json<()>, Json<DeathReason>> {
    /*
    match payload {
        Payload::Push { .. } => {
            return Ok(Json(handle_push(args, keyring_dirs, payload).await?));
        }
        _ => {}
    }
    */
    if let Payload::Push { .. } = payload {
        return Ok(Json(handle_push(args, keyring_dirs, payload).await?));
    }
    Ok(Json(()))
}
