use serde::Serialize;
use thiserror::Error;

/// The reasons a program may have died or not started to begin with.
#[derive(Serialize, Error, Clone, Debug)]
pub(crate) enum DeathReason {
    /// No command was ever configured to run in the first place
    #[error("no command was configured")]
    NoCommandConfiguration,

    /// The keyring was unable to successfully verify a commit based on an error within the keyring
    /// itself
    #[error("error loading keyring: {reason}")]
    KeyringError { reason: String },

    /// The keyring was unable to successfully verify a commit based on an invalid or missing
    /// signature on the keyring
    #[error("error verifying from keyring: {reason}")]
    KeyringVerificationError { reason: String },
}

/// Determine whether or not a command was successful based on multiple determining factors, such
/// as whether a command was invoked in the first place, the reasons why a command may not have
/// been invoked, and if a command was invoked, whether or not it had terminated within a certain
/// timeout.
#[derive(Serialize, Clone, Debug)]
pub(crate) enum Status {
    /// The program has either died or has never lived
    Death(DeathReason),

    /// The program has successfully started
    Life,
}
