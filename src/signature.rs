use axum::{
    http::{Request, StatusCode},
    body::{self, BoxBody, Bytes, Full},
    middleware::{self, Next},
    response::Response,
};
use base64::decode_config_buf;
use headers::{Header, HeaderName, HeaderValue};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{debug, instrument};

use crate::cli::Args;

pub(crate) struct HubSignature256(String);

static HUB_SIGNATURE_256: HeaderName = HeaderName::from_static("x-hub-signature-256");

#[derive(Clone, Debug)]
pub(crate) struct Key(Vec<u8>);

impl Key {
    /// Create a key from a base64-encoded string
    pub(crate) fn new(key: &str) -> Self {
        // I am proud of this small optimization.
        let mut vec: Vec<u8> = Vec::with_capacity(32);
        decode_config_buf(key, base64::STANDARD, &mut vec);
        Key(vec)
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

impl HubSignature256 {
    // TODO(RyanSquared): impl
    pub(crate) fn verify(key: Key) -> bool {
        true
    }

    pub(crate) async fn verify_middleware(
        mut req: Request<BoxBody>,
        next: Next<BoxBody>,
    ) -> Result<Response, StatusCode> {
        let args = req
            .extensions_mut()
            .get::<Arc<Args>>()
            .expect("uninitialized args")
            .clone();
        let secret_key = match &args.webhook_secret_key {
            Some(k) => k,
            None => return Ok(next.run(req).await),
        };

        // Extract and rebuild request, borrowing the body for generating the HMAC
        let (parts, body) = req.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let hmac = {
            let mut mac = hmac::Hmac::<Sha256>::new_from_slice(secret_key.into())
                .map_err(|_| StatusCode::UNAUTHORIZED)?;
            mac.update(&body_bytes);
            mac.finalize().into_bytes()
        };

        let req = Request::from_parts(parts, body::boxed(Full::from(body_bytes)));

        let sent_hmac = match req.headers().get(&HUB_SIGNATURE_256) {
            Some(h) => hex::decode(h).map_err(|_| StatusCode::UNAUTHORIZED)?,
            None => return Err(StatusCode::UNAUTHORIZED),
        };

        if hmac[..] == sent_hmac[..] {
            return Ok(next.run(req).await);
        }

        Err(StatusCode::UNAUTHORIZED)
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
        let value_str = value.to_str().expect("not utf8 string");
        if &value_str[0..7] == "sha256=" {
            return Ok(HubSignature256(value_str[8..].to_string()));
        }
        Err(headers::Error::invalid())
    }

    fn encode<E>(&self, values: &mut E)
    where
        E: Extend<HeaderValue>,
    {
        if let Ok(value) = HeaderValue::from_str(self.0.as_str()) {
            values.extend(std::iter::once(value));
        }
    }
}
