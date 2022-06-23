use serde::Serialize;
use thiserror::Error;

/// The reasons a program may have died or not started to begin with.
#[derive(Serialize, Error, Clone, Debug)]
pub(crate) enum DeathReason {
    /// The information we received from the webhook did not match something we expected
    #[error("Received invalid data in webhook at path: {field_path}, value?: {value:?}")]
    InvalidWebhook {
        field_path: String,
        value: Option<String>,
    },

    /// We had some internal error when cloning from the repository
    #[error("Cloning the repository failed: {reason}")]
    FailedClone { reason: String },

    /// The keyring was unable to successfully verify a commit based on an invalid or missing
    /// signature on the keyring
    #[error("Error verifying commit from keyring: {reason}")]
    KeyringVerification { reason: String },
}
