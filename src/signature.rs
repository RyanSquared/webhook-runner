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
use crate::error::{ProcessingError, Result};

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
        dbg!(&received_hmac);

        // Extract and rebuild request, borrowing the body for generating the HMAC
        let (parts, body) = req.into_parts();
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        // Verify hmac using borrowed body
        received_hmac.verify(secret_key.into(), &body_bytes).map_err(|_| StatusCode::UNAUTHORIZED);

        // Rebuild request
        let req = Request::from_parts(parts, body::boxed(Full::from(body_bytes)));

        // All guards have successfully matched, time to move on
        Ok(next.run(req).await)
    }
}

impl TryFrom<&HeaderValue> for HubSignature256 {
    type Error = ProcessingError;

    fn try_from(value: &HeaderValue) -> Result<HubSignature256> {
        let value_str = value.to_str()?;
        if &value_str[0..7] == "sha256=" {
            return Ok(HubSignature256(hex::decode(&value_str[7..])?));
        } else {
            return Err(ProcessingError::HeaderValueParse {
                header: value_str.to_string(),
            });
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
