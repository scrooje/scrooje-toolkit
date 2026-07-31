//! Functions and types related to encrypted blobs as stored on the application
//! server side.
use std::io::Write;

use secrecy::ExposeSecret;

use crate::{DecryptedBytes, DecryptionError, key, xchacha20poly1305_open};

/// Extracts the contents of the blob from ciphertext.
///
/// This function precisely matches what happens on the web client side. The
/// format of the encrypted blob may or may not evolve over time. In either case
/// the implementation should be transparent to the caller.
///
/// ## Errors
///
/// If the `data` passed is empty, of unrecognized version, or the format of
/// the blob is otherwise malformed. Returns [DecryptionError::Decrypt] if the
/// decryption fails, which typically means that the key does not match.
pub fn extract(
    key: &key::BookSecretKey,
    name: &str,
    data: &[u8],
) -> Result<DecryptedBytes, DecryptionError> {
    if data.is_empty() {
        return Err(DecryptionError::MalformedInput);
    }

    // The only supported version: XChaCha20Poly1305Nonce24Gzip
    match data[0] {
        0x01 => {
            // Limit decompressed blob size to 64 MiB.
            const DECOMPRESSED_LIMIT: usize = 1 << 26;

            let key = key.derive_data_encryption_key();
            let compressed: secrecy::SecretSlice<u8> =
                xchacha20poly1305_open(key.expose_secret(), name.as_bytes(), &data[1..])?.into();

            let mut result = Vec::new();
            let writer = LimitedWriter {
                buf: &mut result,
                limit: DECOMPRESSED_LIMIT,
            };
            let mut decoder = flate2::write::GzDecoder::new(writer);
            decoder.write_all(compressed.expose_secret())?;
            decoder.finish()?;

            Ok(DecryptedBytes(result.into()))
        }
        version => Err(DecryptionError::UnrecognizedVersion(version)),
    }
}

struct LimitedWriter<'a> {
    buf: &'a mut Vec<u8>,
    limit: usize,
}

impl std::io::Write for LimitedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.buf.len() + data.len() > self.limit {
            return Err(std::io::Error::other(
                "decompressed blob exceeds size limit",
            ));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
