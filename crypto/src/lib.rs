//! [Scrooje](https://scroo.je/) cryptographic primitives for client
//! authentication flow and blob decryption. This implementation maps the
//! cryptographic operations performed by a web client.
use std::fmt::Write;

use chacha20poly1305::{KeyInit, aead::Aead};
use secrecy::ExposeSecret;

pub mod blob;
mod key;

pub use key::{
    BookEncryptionKey, BookSecretKey, BookSecretKeyError, PbkdfParams, RootKey, RootKeyError,
    SigningKeys,
};

pub(crate) fn encode_hex(bytes: &[u8]) -> Result<String, std::fmt::Error> {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        write!(&mut hex, "{byte:02x}")?;
    }
    Ok(hex)
}

/// Decodes hex string into bytes.
///
/// ## Errors
///
/// If the input string is of odd length, or includes non-hex characters.
pub fn decode_hex(hex: &str) -> Result<Vec<u8>, HexError> {
    if !hex.len().is_multiple_of(2) {
        return Err(HexError::OddLength);
    }

    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }

    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let h = nibble(pair[0]).ok_or(HexError::InvalidCharacter(pair[0] as char))?;
        let l = nibble(pair[1]).ok_or(HexError::InvalidCharacter(pair[1] as char))?;
        out.push((h << 4) | l);
    }

    Ok(out)
}

/// Error returned when hex decoding fails.
#[derive(Debug, thiserror::Error)]
pub enum HexError {
    /// Uneven hex length supplied.
    #[error("hex string must have even length")]
    OddLength,
    /// Something other than `0..9` `a..f` appeared in the hex string.
    #[error("invalid hex character: {0:?}")]
    InvalidCharacter(char),
}

/// Decrypted bytes. This is a wrapper, such that the underlying data memory is
/// cleared on `Drop`.
pub struct DecryptedBytes(pub(crate) secrecy::SecretSlice<u8>);

impl DecryptedBytes {
    /// Returns the decrypted bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.expose_secret()
    }
}

pub(crate) fn xchacha20poly1305_open(
    key: &[u8],
    aad: &[u8],
    nonce_and_ciphertext: &[u8],
) -> Result<Vec<u8>, DecryptionError> {
    const NONCE_LENGTH: usize = size_of::<chacha20poly1305::XNonce>();
    const TAG_LENGTH: usize = size_of::<chacha20poly1305::Tag>();

    if nonce_and_ciphertext.len() < NONCE_LENGTH + TAG_LENGTH {
        return Err(DecryptionError::MalformedInput);
    }
    let nonce = chacha20poly1305::XNonce::try_from(&nonce_and_ciphertext[..NONCE_LENGTH])
        .expect("nonce length should match");
    let chacha = chacha20poly1305::XChaCha20Poly1305::new_from_slice(key)
        .expect("key should be of correct size");
    chacha
        .decrypt(
            &nonce,
            chacha20poly1305::aead::Payload {
                msg: &nonce_and_ciphertext[NONCE_LENGTH..],
                aad,
            },
        )
        .map_err(|_| DecryptionError::Decrypt)
}

/// Error returned when the decryption fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DecryptionError {
    /// Decryption error, likely a key mismatch.
    #[error("decryption failed")]
    Decrypt,
    /// An IO error happening while writting to the buffer.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The input is unrecognizable or malformed.
    #[error("malformed bytes input")]
    MalformedInput,
    /// The format version is unrecognizable.
    #[error("unrecognized version: {0}")]
    UnrecognizedVersion(u8),
}
