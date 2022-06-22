use axum::{
    body::{self, BoxBody, Bytes, Full},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use headers::{Header, HeaderName, HeaderValue};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{debug, instrument};

use crate::cli::Args;
use crate::error::{HeaderParseError, ProcessingError, Result};

#[derive(Clone, Debug)]
pub(crate) struct Key(Vec<u8>);

impl Key {
    /// Create a key from a str
    pub(crate) fn new(key: &str) -> Self {
        Key(key.bytes().collect())
    }
}

impl<'a> From<&'a Key> for &'a [u8] {
    fn from(key: &'a Key) -> Self {
        key.0.as_slice()
    }
}

impl clap::builder::ValueParserFactory for Key {
    type Parser = KeyValueParser;
    fn value_parser() -> Self::Parser {
        KeyValueParser
    }
}

#[derive(Clone, Debug)]
pub(crate) struct KeyValueParser;
impl clap::builder::TypedValueParser for KeyValueParser {
    type Value = Key;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> std::result::Result<Self::Value, clap::Error> {
        let value = value
            .to_str()
            .ok_or_else(|| clap::Error::raw(clap::ErrorKind::InvalidUtf8, "utf8 decode error"))?;
        Ok(Key::new(value))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HubSignature256(Vec<u8>);

static HUB_SIGNATURE_256: HeaderName = HeaderName::from_static("x-hub-signature-256");

impl HubSignature256 {
    #[must_use]
    pub(crate) fn verify(&self, key: &Key, content: &Bytes) -> Result<()> {
        let tested_hmac = {
            let mut mac = hmac::Hmac::<Sha256>::new_from_slice(key.into())?;
            mac.update(&content);
            mac.finalize().into_bytes()
        };
        if &tested_hmac[..] != &self.0[..] {
            return Err(ProcessingError::HmacNotEqual {
                tested_hmac: hex::encode(&tested_hmac[..]),
                good_hmac: hex::encode(&self.0[..]),
            });
        }
        Ok(())
    }

    #[instrument(skip_all)]
    pub(crate) async fn verify_middleware(
        mut req: Request<BoxBody>,
        next: Next<BoxBody>,
    ) -> std::result::Result<Response, StatusCode> {
        let args = req
            .extensions_mut()
            .get::<Arc<Args>>()
            .expect("uninitialized args")
            .clone();
        let secret_key = match &args.webhook_secret_key {
            Some(k) => k,
            None => return Ok(next.run(req).await),
        };

        let received_hmac = match req.headers().get(&HUB_SIGNATURE_256) {
            Some(header) => {
                HubSignature256::try_from(header).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            }
            None => return Err(StatusCode::UNAUTHORIZED),
        };

        // Extract and rebuild request, borrowing the body for generating the HMAC
        let (parts, body) = req.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Verify hmac using borrowed body
        received_hmac
            .verify(secret_key.into(), &body_bytes)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        // Rebuild request
        let req = Request::from_parts(parts, body::boxed(Full::from(body_bytes)));

        // All guards have successfully matched, time to move on
        Ok(next.run(req).await)
    }
}

impl TryFrom<&HeaderValue> for HubSignature256 {
    type Error = HeaderParseError;

    fn try_from(value: &HeaderValue) -> std::result::Result<HubSignature256, HeaderParseError> {
        value.to_str()?.try_into()
    }
}

impl TryFrom<&str> for HubSignature256 {
    type Error = HeaderParseError;

    fn try_from(value: &str) -> std::result::Result<HubSignature256, HeaderParseError> {
        let len = value.len();
        if len != (64 + 7) {
            return Err(HeaderParseError::Length {
                length: len,
                intended: (64 + 7),
            });
        }
        if &value[0..7] != "sha256=" {
            return Err(HeaderParseError::Content {
                header: value.to_string(),
            });
        }
        let hex_decode = hex::decode(&value[7..]);
        match hex_decode {
            Ok(hex) => Ok(HubSignature256(hex)),
            Err(e) => Err(HeaderParseError::from(e)),
        }
    }
}

impl Header for HubSignature256 {
    fn name() -> &'static HeaderName {
        &HUB_SIGNATURE_256
    }

    fn decode<'i, I>(values: &mut I) -> std::result::Result<Self, headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(headers::Error::invalid)?;
        if let Ok(value) = HubSignature256::try_from(value) {
            return Ok(value);
        }
        Err(headers::Error::invalid())
    }

    fn encode<E>(&self, values: &mut E)
    where
        E: Extend<HeaderValue>,
    {
        if let Ok(value) =
            HeaderValue::from_str(format!("sha256={}", hex::encode(&self.0)).as_str())
        {
            values.extend(std::iter::once(value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // {{{ HubSignature256 decoding

    #[test]
    fn can_decode_signature_header_from_str() {
        HubSignature256::try_from(
            "sha256=2ed61cca0a6e94c01c51ab6d396b4308f12fe39d0daffc5738fab9285ec56f9c",
        )
        .expect("signature was not correctly parsed");
    }

    #[test]
    fn will_error_on_invalid_length() {
        assert!(
            HubSignature256::try_from(
                "sha256=2ed61cca0a6e94c01c51ab6d396b4308f12fe39d0daffc5738fa5ec56f9",
            )
            .is_err(),
            "length should be too short"
        );
        assert!(
            HubSignature256::try_from(
                "sha256=2ed61cca0a6e94c01c51ab6d396b4308f12fe39d0daffc5738fab9285ec56f9ca",
            )
            .is_err(),
            "length should be too long"
        );
        let err = HubSignature256::try_from("");
        match err {
            Err(HeaderParseError::Length { .. }) => (),
            e => {
                assert!(e.is_err(), "length should be too short");
                e.expect("incorrect error variant from HubSignature256::<&str>::try_from");
            }
        }
    }

    #[test]
    fn will_error_on_malformed_header() {
        let err = HubSignature256::try_from(
            "sha255=2ed61cca0a6e94c01c51ab6d396b4308f12fe39d0daffc5738fab9285ec56f9c",
        );
        match err {
            Err(HeaderParseError::Content { .. }) => (),
            e => {
                assert!(e.is_err(), "content should be invalid");
                e.expect("incorrect error variant from HubSignature256::<&str>::try_from");
            }
        }
    }

    #[test]
    fn will_error_on_invalid_hex() {
        let err = HubSignature256::try_from(
            "sha256=2gd61cca0a6e94c01c51ab6d396b4308f12fe39d0daffc5738fab9285ec56f9c",
        );
        match err {
            Err(HeaderParseError::HexDecode { .. }) => (),
            e => {
                assert!(e.is_err(), "content should be invalid");
                e.expect("incorrect error variant from HubSignature256::<&str>::try_from");
            }
        }
    }

    // }}}

    // {{{ HubSignature256 verifying
    #[test]
    fn can_verify_valid_signature() {
        let signature = HubSignature256::try_from(
            "sha256=aa5f1f4ddf25689f59c16b7caef668db08d6c2656d85c899df8457d32d771d72",
        ).expect("unable to parse signature header");
        let key = Key::new("testingkey");
        let test_body = axum::body::Bytes::from_static(b"hello");
        signature.verify(&key, &test_body).expect("invalid signature verification");
    }

    #[test]
    fn will_error_on_incorrect_signature() {
        let signature = HubSignature256::try_from(
            "sha256=aa5f1f4ddf25689f59c16b7caef668db08d6c2656d85c899df8457d32d771d73",
        ).expect("unable to parse signature header");
        let key = Key::new("testingkey");
        let test_body = axum::body::Bytes::from_static(b"hello");
        assert!(signature.verify(&key, &test_body).is_err(), "didn't error on modified signature");

        let signature = HubSignature256::try_from(
            "sha256=aa5f1f4ddf25689f59c16b7caef668db08d6c2656d85c899df8457d32d771d72",
        ).expect("unable to parse signature header");
        let key = Key::new("testingkey");
        let test_body = axum::body::Bytes::from_static(b"heloo");
        assert!(signature.verify(&key, &test_body).is_err(), "didn't error on modified body");
    }
    // }}}

}
