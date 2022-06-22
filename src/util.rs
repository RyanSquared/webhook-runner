use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use tempdir::TempDir;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::Command;
use tracing::{debug, error, info, instrument};

use crate::error::{ProcessingError, Result};

#[derive(Debug, Default)]
pub(crate) struct KeyringDirs {
    pub tag: Option<TempDir>,
    pub commit: Option<TempDir>,
}
pub(crate) type Keyrings = (Option<TempDir>, Option<TempDir>);

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

const GPGCONF_HEADER: &'static str = "
# Note: DO NOT do this for your personal GPG configuration.
# This is SOLELY for a configuration designed to verify options using `gpgv` or
# other compatible solutions using an immutable keyring.";

#[instrument]
pub(crate) async fn assert_gpg_directory(keyring: &str) -> Result<TempDir> {
    // Ensure that the file exists by loading the file metadata, which returns std::io::Result<_>
    tokio::fs::metadata(keyring).await?;
    debug!(?keyring, "metadata exists");

    // Ensure that the keyring is valid
    let mut command = tokio::process::Command::new("gpg")
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

#[instrument]
pub(crate) async fn clone_repository(repository_url: &str, clone_timeout: u32) -> Result<TempDir> {
    // Create a temporary directory for cloning the Git repository into, based on the
    // name of the current commit
    let tmp_dir = TempDir::new("webhook-runner")?;
    debug!(directory = ?tmp_dir.path(), "creating new directory to clone git repository");

    // Run the command to clone into the Git repository, capturing output into a pipe
    let mut clone_process = Command::new("git")
        .arg("clone")
        .arg("--recursive")
        .arg(repository_url)
        .arg(tmp_dir.path())
        .stderr(Stdio::piped())
        .spawn()?;

    // Return errors depending on if a timeout was hit or a nonzero exit code was reached
    let timeout = tokio::time::timeout(
        Duration::from_secs(clone_timeout.into()),
        clone_process.wait_with_output(),
    )
    .await?;
    let result = timeout?;
    debug!(exit_status = ?result.status, "command has completed");

    dump_output(
        format!(
            "git clone --recursive {repository_url} {:?}",
            tmp_dir.path()
        )
        .as_str(),
        &result,
    )
    .await?;

    ProcessingError::assert_exit_status(result.status)?;
    Ok(tmp_dir)
}

/// Verify that the commit ref of a given Git directory is signed by a valid signature using the
/// GPG configuration in a given directory. Returns a Result to ensure the bad case is handled.
#[instrument]
#[must_use]
pub(crate) async fn verify_commit(
    commit_ref: &str,
    directory: &Path,
    gpghome: &Path,
) -> Result<()> {
    let mut command = Command::new("git")
        .arg("-C")
        .arg(directory)
        .arg("checkout")
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

    // There's a nasty trick you can do when using `git checkout` where you can create a branch
    // with the same name as a git commit, leading to a situation where you're not checked out
    // where you want to be checked out. This is a simple fix for it.
    debug!("assuring we have switched to the right commit");
    let mut command = Command::new("git")
        .arg("-C")
        .arg(directory)
        .arg("rev-parse")
        .arg("HEAD")
        .stdout(Stdio::piped())
        .spawn()?;
    let timeout = tokio::time::timeout(Duration::from_secs(1), command.wait_with_output()).await?;
    let result = timeout?;
    ProcessingError::assert_exit_status(result.status)?;
    if std::str::from_utf8(&result.stdout)
        .map_err(|e| {
            error!("unable to decode utf8 from command output: {e}");
            ProcessingError::RepositoryIntegrity
        })?
        .trim()
        != commit_ref
    {
        return Err(ProcessingError::RepositoryIntegrity);
    }

    let mut command = Command::new("git")
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
