//! Client key derivation.
use base64::engine::Engine;
use ml_dsa::signature::Signer as MlDsaSigner;
use secrecy::{zeroize::Zeroize, ExposeSecret};

use crate::{
    decode_hex, encode_hex, xchacha20poly1305_open, DecryptedBytes, DecryptionError, HexError,
};

/// PBKDF parameters for deriving [RootKey] from username and password.
pub enum PbkdfParams {
    /// Argon2id[m=128MiB; i=4; p=2], hash of username as salt.
    V1,
    // Next version should pick a different KDF context for salt, e.g. PbkdfSaltRootKeyAlg2.
}

/// The root key, from which subkeys are derived. Can either be constructed from
/// a passkey [PRF](https://w3c.github.io/webauthn/#prf-extension) or a password.
///
/// The key itself is zeroized on drop.
pub struct RootKey(secrecy::SecretSlice<u8>);

impl RootKey {
    // Only the first 64 bytes are in use, the remaining 64 bytes are reserved.
    const ROOT_LENGTH: usize = 128;
    const SECRET_LENGTH: usize = 64;

    #[cfg(feature = "passkey")]
    const PASSKEY_SALT: &[u8] = &[
        0x23, 0x92, 0x47, 0x7c, 0x02, 0x37, 0x53, 0x95, 0xe3, 0x41, 0x21, 0x8d, 0xb3, 0x86, 0x84,
        0x04, 0xa0, 0x8d, 0x00, 0xc6, 0xc7, 0xd6, 0x57, 0x62, 0xf4, 0x18, 0x71, 0xdd, 0xb7, 0xfe,
        0x54, 0xeb,
    ];

    /// Derives the root key from a hardware passkey. This function performs PRF
    /// as done by WebAuthn API. This call will ask for a tap by printing to
    /// stdout and blocking until tapped.
    ///
    /// `rp_id` is typically a domain name of the site where the resident key
    /// was created. In our case, that's most likely `app.scroo.je`.
    ///
    /// ## Errors
    ///
    /// First, an error is returned if there is no, or more than 1 USB device
    /// found. Next, if no credentials found, or more than 1 credential match
    /// the `rp_id` and the `username`. Pin unlock failure will also result in
    /// an error, as well as if device does not support the HMAC Secret
    /// extension, or the attempt to get the secret via extension resulted in
    /// more than one response.
    #[cfg(feature = "passkey")]
    pub fn from_passkey(rp_id: &str, username: &str, pin: &str) -> Result<RootKey, RootKeyError> {
        let cfg = ctap_hid_fido2::Cfg::init();
        let device = ctap_hid_fido2::FidoKeyHidFactory::create(&cfg)
            .map_err(|err| RootKeyError::Passkey(err.to_string()))?;

        let rps = device
            .credential_management_enumerate_rps(Some(pin))
            .map_err(|err| RootKeyError::Passkey(err.to_string()))?;

        let mut cred_ids = vec![];
        for rp in rps {
            if rp.public_key_credential_rp_entity.id != rp_id {
                continue;
            }

            let creds = device
                .credential_management_enumerate_credentials(Some(pin), &rp.rpid_hash)
                .map_err(|err| RootKeyError::Passkey(err.to_string()))?;

            cred_ids.extend(creds.iter().filter_map(|cred_id| {
                if cred_id.public_key_credential_user_entity.name == username {
                    Some(cred_id.public_key_credential_descriptor.id.clone())
                } else {
                    None
                }
            }));
        }

        if cred_ids.is_empty() {
            return Err(RootKeyError::MissingCredentials);
        }

        if cred_ids.len() > 1 {
            return Err(RootKeyError::MultipleCredentials(cred_ids.len()));
        }

        let mut challenge = vec![0u8; 32];
        rand::fill(&mut challenge);
        let args = ctap_hid_fido2::fidokey::GetAssertionArgsBuilder::new(rp_id, &challenge)
            .credential_id(&cred_ids[0])
            .extensions(&[ctap_hid_fido2::fidokey::AssertionExtension::HmacSecret(
                Some(webauthn_prf_salt(Self::PASSKEY_SALT)),
            )])
            .pin(pin)
            .build();
        let assertions = device
            .get_assertion_with_args(&args)
            .map_err(|err| RootKeyError::Passkey(err.to_string()))?;

        let mut root_key = vec![0u8; Self::ROOT_LENGTH];
        let mut matches = assertions
            .into_iter()
            .flat_map(|assertion| assertion.extensions)
            .filter_map(|extension| {
                if let ctap_hid_fido2::fidokey::AssertionExtension::HmacSecret(Some(secret)) =
                    extension
                {
                    Some(secret)
                } else {
                    None
                }
            });
        let mut secret = matches
            .next()
            .ok_or_else(|| RootKeyError::Passkey("missing hmac assertion response".into()))?;
        if matches.next().is_some() {
            return Err(RootKeyError::Passkey(
                "multiple hmac assertion responses".into(),
            ));
        }
        let mut hasher = blake3::Hasher::new_derive_key(KdfContexts::PasskeyRootKey.into());
        hasher.update(&secret);
        hasher.finalize_xof().fill(&mut root_key);
        secret.zeroize();

        Ok(RootKey(root_key.into()))
    }

    /// Derives the root key from username and password. The `params` is
    /// required and signals which key derivation parameters should be used.
    ///
    /// ## Errors
    ///
    /// Fails if argon2id key derivation fails for whatever reason. This
    /// typically should not happen. If the process runs out of memory, it will
    /// likely crash, rather than return an error.
    pub fn from_password(
        username: &str,
        password: &str,
        params: PbkdfParams,
    ) -> Result<RootKey, RootKeyError> {
        let root_key = match params {
            PbkdfParams::V1 => {
                let salt = blake3::derive_key(
                    KdfContexts::PbkdfSaltRootKeyAlg1.into(),
                    username.as_bytes(),
                );

                let argon = argon2::Argon2::new(
                    argon2::Algorithm::Argon2id,
                    argon2::Version::V0x13,
                    argon2::Params::new(131072, 4, 2, Some(Self::ROOT_LENGTH))
                        .map_err(RootKeyError::Argon2)?,
                );

                let mut root_key = vec![0u8; Self::ROOT_LENGTH];
                argon
                    .hash_password_into(password.as_bytes(), &salt, &mut root_key)
                    .map_err(RootKeyError::Argon2)?;
                root_key
            }
        };

        Ok(RootKey(root_key.into()))
    }

    /// Derives book encryption key from the root key. This key is used for
    /// wrapping the key before it is stored on the server side. The key
    /// derivation is tied to the `book_uuid`, i.e. every book gets a unique
    /// book encryption key.
    pub fn book_encryption_key(&self, book_uuid: uuid::Uuid) -> BookEncryptionKey {
        let secret = &self.0.expose_secret()[..Self::SECRET_LENGTH];
        let uuid_bytes = book_uuid.as_bytes();
        let mut input = Vec::with_capacity(Self::SECRET_LENGTH + uuid_bytes.len());
        input.extend_from_slice(secret);
        input.extend_from_slice(uuid_bytes);

        let mut key = blake3::derive_key(KdfContexts::BookEncryptionKeyAlg1.into(), &input);
        let result = key.to_vec().into();

        input.zeroize();
        key.zeroize();
        BookEncryptionKey(result)
    }

    /// Derives signing keys used for HTTP request signatures.
    pub fn signing_keys(&self) -> SigningKeys {
        let secret = &self.0.expose_secret()[..Self::SECRET_LENGTH];
        let ed25519 = {
            let mut secret = blake3::derive_key(KdfContexts::SigningKeyAlg1.into(), secret);
            let key = ed25519_dalek::SigningKey::from_bytes(&secret);
            secret.zeroize();
            key
        };

        let mldsa65 = {
            let mut seed = blake3::derive_key(KdfContexts::SigningKeyAlg2.into(), secret);
            let key = ml_dsa::SigningKey::<ml_dsa::MlDsa65>::from_seed(&seed.into());
            seed.zeroize();
            key
        };

        SigningKeys { ed25519, mldsa65 }
    }
}

/// Error returned when [RootKey] derivation fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RootKeyError {
    /// Error returned by `argon2id`.
    #[error("argon2 error: {0}")]
    Argon2(argon2::Error),
    /// No credentials matched.
    #[error("no matching credentials found")]
    MissingCredentials,
    /// More than one credential matched.
    #[error("found {0} credentials, expected 1")]
    MultipleCredentials(usize),
    /// Error occurred during passkey interaction.
    #[error("passkey error: {0}")]
    Passkey(String),
}

/// Book encryption key, used for wrapping the per-book secret key before
/// storing it on the server side.
///
/// The key is zeroized on `Drop`.
pub struct BookEncryptionKey(secrecy::SecretSlice<u8>);

impl BookEncryptionKey {
    /// Decrypts the `data` and returns it as a zeroize-able on `Drop` type.
    ///
    /// The format is encoded in a first byte of the input ciphertext.
    ///
    /// ## Errors
    ///
    /// If the `data` passed is empty, of unrecognized version, or the format of
    /// the input is otherwise malformed. Returns [DecryptionError::Decrypt] if
    /// the decryption fails, which typically means that the key does not match.
    pub fn decrypt(&self, data: &[u8]) -> Result<DecryptedBytes, DecryptionError> {
        if data.is_empty() {
            return Err(DecryptionError::MalformedInput);
        }

        // So far only a single version is supported - XChaCha20Poly1305 with 24-byte Nonce.
        match data[0] {
            0x01 => Ok(DecryptedBytes(
                xchacha20poly1305_open(self.0.expose_secret(), b"", &data[1..])?.into(),
            )),
            version => Err(DecryptionError::UnrecognizedVersion(version)),
        }
    }
}

/// Signing keys used for client-side request signatures.
///
/// The keys are zeroized on `Drop`.
pub struct SigningKeys {
    ed25519: ed25519_dalek::SigningKey,
    mldsa65: ml_dsa::SigningKey<ml_dsa::MlDsa65>,
}

impl SigningKeys {
    const REQUEST_NONCE_LENGTH: usize = 16;

    /// Signs the request, returning a JWT string. The JWT should be included
    /// in a request to the application server.
    ///
    /// The `method` is a request method. `endpoint` is a full URI with path
    /// and query string. Optional body can be supplied if the request includes
    /// it. The blake3 hash will be taken of the body and thus bound to the
    /// request.
    ///
    /// This function uses `EdDSA+MLDSA65` custom JWT algorithm for hybrid
    /// post-quantum dual signature. While the server supports plain `EdDSA`
    /// signatures too, it should be encouraged to use this call instead.
    pub fn sign_request(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<&[u8]>,
        exp_after: std::time::Duration,
    ) -> String {
        #[derive(serde::Serialize)]
        struct JwtHeader {
            alg: String,
            typ: String,
        }

        #[derive(serde::Serialize)]
        struct JwtPayload {
            exp: i64,
            iat: i64,
            nonce: String,
            htm: String,
            hte: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            htb_blake3: Option<String>,
        }

        debug_assert!(
            exp_after.as_secs() <= 60,
            "experiation should be at most 60 seconds"
        );
        let exp_secs: i64 = exp_after
            .as_secs()
            .try_into()
            .expect("u64 should be a valid i64");

        let header = JwtHeader {
            alg: "EdDSA+MLDSA65".to_string(),
            typ: "JWT".to_string(),
        };
        let header_json = serde_json::to_vec(&header).expect("jwt header should serialize");
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&header_json);

        let mut nonce = [0u8; Self::REQUEST_NONCE_LENGTH];
        rand::fill(&mut nonce);
        let now = chrono::Utc::now().timestamp();
        let payload = JwtPayload {
            iat: now,
            exp: now + exp_secs,
            nonce: encode_hex(&nonce).expect("nonce should encode as hex"),
            hte: endpoint.to_owned(),
            htm: method.to_owned(),
            htb_blake3: body.map(|body| {
                encode_hex(blake3::hash(body).as_bytes()).expect("blake3 hash should encode as hex")
            }),
        };
        let payload_json = serde_json::to_vec(&payload).expect("serializable client jwt payload");
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&payload_json);

        let message = [header, payload].join(".");

        let ed25519_signature = self.ed25519.sign(message.as_bytes());
        let mldsa65_signature = self.mldsa65.sign(message.as_bytes()).encode();
        let mut signature = vec![0u8; ed25519_dalek::SIGNATURE_LENGTH + mldsa65_signature.len()];
        signature[..ed25519_dalek::SIGNATURE_LENGTH].copy_from_slice(&ed25519_signature.to_bytes());
        signature[ed25519_dalek::SIGNATURE_LENGTH..].copy_from_slice(mldsa65_signature.as_slice());
        let signature =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.as_slice());

        [message, signature].join(".")
    }
}

/// Book secret key. Generated randomly when the book is created.
///
/// This key is what appears in the backup of the key performed during the book
/// setup. Since the backup is hex-encoded, the [BookSecretKey::from_hex] is
/// provided.
///
/// The key is zeroized on `Drop`.
pub struct BookSecretKey(secrecy::SecretSlice<u8>);

impl BookSecretKey {
    const KEY_LENGTH: usize = 64;

    /// Constructs the key from raw bytes.
    ///
    /// Consider zeroizing the input memory after constructing the key.
    ///
    /// ## Errors
    ///
    /// Returns an error if the key is of incorrect size.
    pub fn from_slice(key: &[u8]) -> Result<BookSecretKey, BookSecretKeyError> {
        if key.len() != Self::KEY_LENGTH {
            return Err(BookSecretKeyError::BadLength(key.len()));
        }
        Ok(BookSecretKey(key.to_vec().into()))
    }

    /// Constructs the key from hex-encoded bytes.
    ///
    /// Consider zeroizing the input memory after constructing the key.
    ///
    /// ## Errors
    ///
    /// Returns an error if the key is of incorrect size, or hex string is
    /// malformed.
    pub fn from_hex(hex: &str) -> Result<BookSecretKey, BookSecretKeyError> {
        let mut decoded = decode_hex(hex)?;
        let key = BookSecretKey::from_slice(&decoded)?;
        decoded.zeroize();
        Ok(key)
    }

    pub(crate) fn derive_data_encryption_key(&self) -> secrecy::SecretSlice<u8> {
        let mut secret = blake3::derive_key(
            KdfContexts::BookDataEncryptionKeyAlg1.into(),
            self.0.expose_secret(),
        );
        let key = secret.to_vec();
        secret.zeroize();
        key.into()
    }
}

/// Error returned when constructing [BookSecretKey].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BookSecretKeyError {
    /// Unexpected book secret key length supplied. Must be 64 bytes long.
    #[error("bad book secret key length: expected {0} == {len}", len = BookSecretKey::KEY_LENGTH)]
    BadLength(usize),
    /// Supplied hex string is malformed.
    #[error("malformed hex: {0}")]
    Hex(#[from] HexError),
}

enum KdfContexts {
    BookDataEncryptionKeyAlg1,
    BookEncryptionKeyAlg1,
    #[cfg(feature = "passkey")]
    PasskeyRootKey,
    PbkdfSaltRootKeyAlg1,
    SigningKeyAlg1,
    SigningKeyAlg2,
}

impl From<KdfContexts> for &str {
    fn from(value: KdfContexts) -> Self {
        match value {
            KdfContexts::BookDataEncryptionKeyAlg1 => {
                "scrooje:v1:client 2026-07-12 10:30:32 book-data-encryption-key:alg1"
            }
            KdfContexts::BookEncryptionKeyAlg1 => {
                "scrooje:v1:client 2026-07-12 10:30:01 book-encryption-key:alg1"
            }
            #[cfg(feature = "passkey")]
            KdfContexts::PasskeyRootKey => "scrooje:v1:client 2026-07-12 10:31:26 passkey-root-key",
            KdfContexts::PbkdfSaltRootKeyAlg1 => {
                "scrooje:v1:client 2026-07-12 10:26:13 pbkdf-salt-root-key:alg1"
            }
            KdfContexts::SigningKeyAlg1 => "scrooje:v1:client 2026-07-12 10:27:45 signing-key:alg1",
            KdfContexts::SigningKeyAlg2 => "scrooje:v1:client 2026-07-12 10:28:20 signing-key:alg2",
        }
    }
}

#[cfg(feature = "passkey")]
fn webauthn_prf_salt(salt: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"WebAuthn PRF\x00");
    hasher.update(salt);
    hasher.finalize().into()
}
