use std::process::Stdio;
use std::time::Duration;

use tempdir::TempDir;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::Command;
use tracing::{debug, info, instrument};

use crate::error::{ProcessingError, Result};

const GPGCONF_HEADER: &'static str = "
# Note: DO NOT do this for your personal GPG configuration.
# This is SOLELY for a configuration designed to verify options using `gpgv` or
# other compatible solutions using an immutable keyring.";

#[instrument]
pub(crate) async fn assert_keyring(keyring: &str) -> Result<TempDir> {
    // Ensure that the file exists by loading the file metadata, which returns std::io::Result<_>
    tokio::fs::metadata(keyring).await?;

    // Ensure that the keyring is valid
    let mut command = tokio::process::Command::new("gpg")
        .arg("--no-default-keyring")
        .arg(format!("--keyring={}", keyring).as_str())
        .stderr(Stdio::piped())
        .spawn()?;

    let timeout = tokio::time::timeout(Duration::from_secs(1), command.wait_with_output()).await?;
    let result = timeout?;
    debug!(exit_status = ?result.status, "command has completed");

    // Print the output of the command
    let clone_output = result.stderr;
    let mut lines = clone_output.lines();
    while let Some(line) = lines.next_line().await? {
        debug!(
            "`gpg --no-default-keyring --keyring={} --list-keys`: {}",
            keyring, line
        );
    }

    ProcessingError::assert_exit_status(result.status)?;

    // We need to define a GNUPGHOME loaded with a keyring
    // We can use the GPG configuration options `keyring <keyring_path>` and `trust-model always`
    // to configure GPG to use a specific file.
    let tmp_dir = TempDir::new("keyring")?;
    debug!(directory = ?tmp_dir.path(), "creating new directory for PGP keyring");

    // Create the configuration file
    let mut file = File::create(tmp_dir.path().join("gpg.conf")).await?;
    file.write_all(GPGCONF_HEADER.as_bytes()).await?;
    file.write_all(format!("\nkeyring {}", keyring).as_bytes())
        .await?;
    file.write_all("trust-model always".as_bytes()).await?;

    // TODO: determine whether or not the keyring is valid? can require an invocation of
    // `gpg --list-keys`

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

    // Print the output of the command
    let clone_output = result.stderr;
    let mut lines = clone_output.lines();
    while let Some(line) = lines.next_line().await? {
        debug!("`git clone`: {}", line);
    }

    ProcessingError::assert_exit_status(result.status)?;
    Ok(tmp_dir)
}
