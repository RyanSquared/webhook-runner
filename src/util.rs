use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use git2::{Repository, Oid, Cred, RemoteCallbacks, FetchOptions, build::RepoBuilder};
use tempdir::TempDir;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::Command;
use tracing::{debug, instrument};

use crate::error::{ProcessingError, Result};

#[derive(Debug, Default)]
pub(crate) struct KeyringDirs {
    pub tag: Option<TempDir>,
    pub commit: Option<TempDir>,
}

/// Output the standard output and the standard error of a command's given Output. Does not allow
/// for configuration of the level because `tracing` requires a const Level.
pub(crate) async fn dump_output(command: &str, output: &std::process::Output) -> Result<()> {
    debug!(command = ?command);
    // Determine the actual command, skipping environment variables
    let mut iter = command.split_whitespace();
    let prefix = {
        loop {
            let word = iter.next().unwrap_or("undefined");
            if !word.chars().next().unwrap_or('_').is_uppercase() {
                break word;
            }
        }
    };

    // Print the output of the command
    let stdout = &output.stdout;
    let mut lines = stdout.lines();
    while let Some(line) = lines.next_line().await? {
        debug!("{prefix}: {line}");
    }
    let stderr = &output.stderr;
    let mut lines = stderr.lines();
    while let Some(line) = lines.next_line().await? {
        debug!("{prefix}: {line}");
    }

    Ok(())
}

const GPGCONF_HEADER: &str = "
# Note: DO NOT do this for your personal GPG configuration.
# This is SOLELY for a configuration designed to verify options using `gpgv` or
# other compatible solutions using an immutable keyring.";

/// Ensure that a directory exists with the gpg.conf file that configures how the program
/// looks for the keyring, using a TempDir that will be automatically removed when dropped.
#[instrument]
pub(crate) async fn assert_gpg_directory(keyring: &str) -> Result<TempDir> {
    // Ensure that the file exists by loading the file metadata, which returns std::io::Result<_>
    tokio::fs::metadata(keyring).await?;
    debug!(?keyring, "metadata exists");

    // Ensure that the keyring is valid
    let command = tokio::process::Command::new("gpg")
        .arg("--no-default-keyring")
        .arg(format!("--keyring={}", keyring).as_str())
        .arg("--list-keys")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let timeout = tokio::time::timeout(Duration::from_secs(1), command.wait_with_output()).await?;
    let result = timeout?;
    debug!(exit_status = ?result.status, "command has completed");

    dump_output(
        format!("gpg --no-default-keyring --keyring={keyring} --list-keys").as_str(),
        &result,
    )
    .await?;

    ProcessingError::assert_exit_status(result.status)?;

    // We need to define a GNUPGHOME loaded with a keyring
    // We can use the GPG configuration options `keyring <keyring_path>` and `trust-model always`
    // to configure GPG to use a specific file.
    let tmp_dir = TempDir::new("keyring")?;
    debug!(directory = ?tmp_dir.path(), "creating new directory for PGP keyring");

    // Create the configuration file
    let mut file = File::create(tmp_dir.path().join("gpg.conf")).await?;
    debug!(?file, "opened file");
    file.write_all(GPGCONF_HEADER.as_bytes()).await?;
    file.write_all(format!("\nkeyring {}", keyring).as_bytes())
        .await?;
    file.write_all("\ntrust-model always".as_bytes()).await?;
    debug!("finished writing gpg configuration to file");

    Ok(tmp_dir)
}

/// Clone a GitHub repository and ensure that a given commit ref matches what was expected,
/// including a check to ensure that the checkout was to a commit ref and not a branch.
#[instrument]
pub(crate) async fn clone_repository(
    repository_url: &str,
    commit_ref: &str,
    clone_timeout: u32,
    ssh_key: Option<&String>,
) -> Result<TempDir> {
    // Create a temporary directory for cloning the Git repository into

    let opts = (repository_url.to_string(),
                commit_ref.to_string(),
                ssh_key.cloned());

    let result: Result<_> = tokio::task::spawn_blocking(move || {
        let tmp_dir = TempDir::new("webhook-runner")?;
        debug!(directory = ?tmp_dir.path(), "creating new directory to clone git repository");

        let (repository_url, commit_ref, ssh_key) = opts;
        let repo = if let Some(ssh_key) = ssh_key {
            debug!(?ssh_key, "using ssh key authentication");
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key (
                    username_from_url.unwrap_or("git"),
                    None,
                    Path::new(&ssh_key),
                    None,
                )
            });

            let mut fetch_options = FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);

            let mut builder = RepoBuilder::new();
            builder.fetch_options(fetch_options);

            builder.clone(repository_url.as_str(), tmp_dir.path())?
        } else {
            debug!("using non-ssh key authentication");
            Repository::clone(repository_url.as_str(), tmp_dir.path())?
        };

        debug!("repository has been cloned");

        // This actually solves the old issue of bypassing `git checkout` using a branch name
        // instead of an exact ref. revparse_single never returns the branch, just the object
        // that it would point to.
        let revparse = repo.revparse_single(commit_ref.as_str())?;
        repo.checkout_tree(&revparse, None)?;
        repo.set_head_detached(revparse.id())?;

        Ok((revparse.id(), tmp_dir))
    }).await?;
    let (revparse, tmp_dir) = result?;

    if revparse != Oid::from_str(commit_ref)? {
        return Err(ProcessingError::RepositoryIntegrity {
            actual: revparse.to_string(),
            expected: commit_ref.to_string(),
        });
    }

    debug!(object = ?revparse, "repository has been checked out");

    Ok(tmp_dir)
}

/// Verify that the commit ref of a given Git directory is signed by a valid signature using the
/// GPG configuration in a given directory. Returns a Result to ensure the bad case is handled.
#[instrument]
pub(crate) async fn verify_commit(
    commit_ref: &str,
    directory: &Path,
    gpghome: &Path,
) -> Result<()> {
    let command = Command::new("git")
        .env("GNUPGHOME", gpghome)
        .arg("-C")
        .arg(directory)
        .arg("verify-commit")
        .arg(commit_ref)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let timeout = tokio::time::timeout(Duration::from_secs(1), command.wait_with_output()).await?;
    let result = timeout?;
    debug!(exit_status = ?result.status, "command has completed");

    dump_output(
        format!("GNUPGHOME={gpghome:?} git -C {directory:?} verify-commit {commit_ref}").as_str(),
        &result,
    )
    .await?;
    ProcessingError::assert_exit_status(result.status)?;

    Ok(())
}
