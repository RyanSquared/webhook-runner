use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum HubSignatureValidationError {
    #[error("the http header value is not a valid str: {source}")]
    InvalidString {
        #[from]
        source: http::header::ToStrError,
    },

    #[error("the http header was malformed: {header}")]
    Content { header: String },

    #[error("header value for signature was incorrect size: {length} != {intended}")]
    Length { length: usize, intended: u32 },

    #[error("hex value was malformed: {source}")]
    HexDecode {
        #[from]
        source: hex::FromHexError,
    },

    #[error("invalid length of hmac key: {source}")]
    HmacKeyLength {
        #[from]
        source: crypto_common::InvalidLength,
    },

    #[error("hmac did not match expected: {source}")]
    HmacVerification {
        #[from]
        source: digest::MacError,
    },
}
