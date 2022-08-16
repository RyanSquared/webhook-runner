use std::io::{Cursor, Read};
use std::path::Path;

use git2::{
    build::RepoBuilder, Commit, Cred, FetchOptions, Oid, RemoteCallbacks, Repository, Signature,
};
use tempdir::TempDir;
use tracing::{debug, instrument};

use openpgp::armor::{Kind, Reader, ReaderMode};
use openpgp::parse::{stream::DetachedVerifierBuilder, Parse};
use openpgp::policy::StandardPolicy;
use sequoia_openpgp as openpgp;

use crate::cert_builder::KeyringFile;
use crate::error::{ProcessingError, Result};

fn format_signature(header: &str, sig: &Signature) -> String {
    let offset = sig.when().offset_minutes();
    let (sign, offset) = if offset < 0 {
        ('-', -offset)
    } else {
        ('+', offset)
    };
    let (hours, minutes) = (offset / 60, offset % 60);
    format!(
        "{} {} {} {}{:02}{:02}",
        header,
        sig,
        sig.when().seconds(),
        sign,
        hours,
        minutes
    )
}

/// Clone a GitHub repository and ensure that a given commit ref matches what was expected,
/// including a check to ensure that the checkout was to a commit ref and not a branch.
#[instrument]
pub async fn clone_repository(
    repository_url: &str,
    commit_ref: &str,
    clone_timeout: u32,
    ssh_key: Option<&String>,
) -> Result<(Repository, TempDir)> {
    // Create a temporary directory for cloning the Git repository into

    let opts = (
        repository_url.to_string(),
        commit_ref.to_string(),
        ssh_key.cloned(),
    );

    let result: Result<_> = tokio::task::spawn_blocking(move || {
        let tmp_dir = TempDir::new("webhook-runner")?;
        debug!(directory = ?tmp_dir.path(), "creating new directory to clone git repository");

        let (repository_url, commit_ref, ssh_key) = opts;
        let repo = if let Some(ssh_key) = ssh_key {
            debug!(?ssh_key, "using ssh key authentication");
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key(
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
        let id = revparse.id();

        // TODO: Can I avoid having to manually drop here?
        drop(revparse);

        Ok((id, repo, tmp_dir))
    })
    .await?;
    let (revparse, repo, tmp_dir) = result?;

    if revparse != Oid::from_str(commit_ref)? {
        return Err(ProcessingError::RepositoryIntegrity {
            actual: revparse.to_string(),
            expected: commit_ref.to_string(),
        });
    }

    debug!(object = ?revparse, "repository has been checked out");

    Ok((repo, tmp_dir))
}

/// Verify that the commit ref of a given Git directory is signed by a valid signature using the
/// GPG configuration in a given directory. Returns a Result to ensure the bad case is handled.
#[instrument(skip_all)]
pub fn verify_commit(commit: Commit<'_>, keyring: &KeyringFile) -> Result<()> {
    // Get the commit object
    let gpgsig_header = commit.header_field_bytes("gpgsig")?;

    let mut cursor = Cursor::new(&gpgsig_header[..]);
    let mut reader = Reader::from_reader(&mut cursor, ReaderMode::Tolerant(Some(Kind::Signature)));

    let mut buf = vec![];
    reader.read_to_end(&mut buf)?;

    debug!("building commit message to verify against");

    let commit_message = {
        let mut lines = vec![format!("tree {tree}", tree = commit.tree_id())];
        for parent in commit.parent_ids() {
            lines.push(format!("parent {parent}"));
        }
        lines.push(format_signature("author", &commit.author()));
        lines.push(format_signature("committer", &commit.committer()));
        lines.push("".to_string());
        if let Some(message) = commit.message() {
            lines.extend(message.lines().map(String::from));
        }
        // significant trailing EOL is the bane of my existence
        lines.push("".to_string());
        lines.join("\n")
    };

    debug!("building verifier with KeyringFile");

    let policy = StandardPolicy::new();
    let mut verifier = DetachedVerifierBuilder::from_bytes(&gpgsig_header[..])
        .map_err(|e| ProcessingError::MalformedSignature { source: e })?
        .with_policy(&policy, None, keyring)
        .map_err(|e| ProcessingError::InvalidSignature { source: e })?;

    debug!("verifying bytes");

    verifier
        .verify_bytes(commit_message)
        .map_err(|e| ProcessingError::InvalidSignature { source: e })?;

    Ok(())
}
