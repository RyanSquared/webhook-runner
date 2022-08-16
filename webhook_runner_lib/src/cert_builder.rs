use std::path::Path;
use tracing::{debug, error};

use openpgp::cert::prelude::*;
use openpgp::parse::{
    stream::{MessageLayer, MessageStructure, VerificationHelper},
    PacketParser, Parse,
};
use sequoia_openpgp as openpgp;

use crate::error::{ProcessingError, Result};

#[derive(Debug)]
pub struct KeyringFile {
    certs: Vec<Cert>,
}

impl KeyringFile {
    /// Load `OpenPGP` certificates ("pubkeys") from a file
    ///
    /// # Errors
    ///
    /// This function can return an error when there is an error reading data from the file, but
    /// does not error when there is a malformed packet inside of the file. This means that a file
    /// can exist with no valid `Packet` and the function will not error. This is mostly done to
    /// ensure that a single invalid `Packet` does not break the ability to run the webhook runner,
    /// which may be self-hosting and therefore somewhat irreparable if broken.
    pub fn from_path<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<Self> {
        debug!(?path, "loading keyrings from path");
        let ppr = PacketParser::from_file(path)
            .map_err(|e| ProcessingError::InvalidKeyringFile { source: e })?;
        let mut certs = vec![];
        for cert in CertParser::from(ppr) {
            match cert {
                Ok(cert) => certs.push(cert),
                // Parsing an invalid packet should not cause a fatal error. The worst thing that
                // could happen is that keyring verification fails. Report early, but don't
                // terminate because of an invalid packet... since that could break the ability
                // to add a *working* packet.
                Err(e) => error!(e = ?e, "error parsing OpenPGP packet"),
            }
        }
        for cert in &certs {
            // print the first ID of a cert
            match cert.userids().next() {
                Some(uid) => debug!(uid = %uid.userid(), "found cert"),
                None => debug!(fp = %cert.fingerprint(), "found cert"),
            }
        }
        Ok(KeyringFile { certs })
    }
}

// Note: This should be & to be usable with VerifierBuilder; all methods take &Self or &mut Self
impl VerificationHelper for &KeyringFile {
    fn get_certs(&mut self, _ids: &[openpgp::KeyHandle]) -> openpgp::Result<Vec<openpgp::Cert>> {
        Ok(self.certs.clone())
    }

    fn check(&mut self, structure: MessageStructure) -> openpgp::Result<()> {
        let mut good = false;
        for (i, layer) in structure.into_iter().enumerate() {
            match (i, layer) {
                (0, MessageLayer::SignatureGroup { results }) => match results.into_iter().next() {
                    Some(Ok(_)) => good = true,
                    Some(Err(e)) => return Err(openpgp::Error::from(e).into()),
                    None => return Err(anyhow::anyhow!("No signature")),
                },
                _ => return Err(anyhow::anyhow!("Unexpected message structure")),
            }
        }
        if good {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Signature verification failed"))
        }
    }
}
