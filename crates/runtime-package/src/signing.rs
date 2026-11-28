//! RFC 8032 Ed25519 package signing and verification for format-2 runtime packages.
//!
//! This module implements standard RFC 8032 Ed25519 signatures (pure Ed25519,
//! 32-byte seed, 32-byte public key, 64-byte signature, standard Base64 encoding)
//! matching Google Tink 1.23.0 `Ed25519Sign`/`Ed25519Verify` used by the Kotlin
//! runtime on JVM and Android API 26+.
//!
//! **Scope boundary (O-8):** Only deterministic test-key primitives are exposed
//! here. Real publisher key storage, passphrase KDF, hardware-backed key stores,
//! and paired-device trust admission belong to REUSE-05 after the O-8 owner
//! decision and live Kotlin verification against the pinned private runtime.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Algorithm identifier emitted in format-2 `signature.json`.
pub const ED25519_ALGORITHM: &str = "Ed25519";

/// Payload-free signing and verification errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SigningError {
    #[error("publisher key identifier must be non-empty")]
    InvalidKeyId,
    #[error("unsupported signature algorithm")]
    UnsupportedAlgorithm,
    #[error("signature key identifier does not match expected publisher key")]
    KeyIdMismatch,
    #[error("invalid Ed25519 public key bytes")]
    InvalidPublicKey,
    #[error("invalid Base64 or length for Ed25519 signature")]
    InvalidSignatureEncoding,
    #[error("Ed25519 signature verification failed")]
    VerificationFailed,
}

/// Format-2 `signature.json` wire representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

/// Abstraction for signing format-2 manifest bytes.
pub trait ManifestSigner {
    /// Non-empty publisher key identifier recorded in `manifest.json` and `signature.json`.
    fn key_id(&self) -> &str;
    /// Sign exact UTF-8 `manifest.json` bytes and return `signature.json` metadata.
    fn sign_manifest(&self, manifest_bytes: &[u8]) -> Result<PackageSignature, SigningError>;
}

/// Deterministic RFC 8032 Ed25519 test-key signer.
///
/// `Debug` is intentionally redacted so secret seed bytes are never formatted into logs.
pub struct Ed25519TestSigner {
    key_id: String,
    signing_key: SigningKey,
}

impl std::fmt::Debug for Ed25519TestSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ed25519TestSigner")
            .field("key_id", &self.key_id)
            .field("public_key_sha256", &self.public_key_sha256())
            .finish()
    }
}

impl Ed25519TestSigner {
    /// Construct a deterministic test signer from a 32-byte RFC 8032 seed and key identifier.
    pub fn from_seed(key_id: impl Into<String>, seed: [u8; 32]) -> Result<Self, SigningError> {
        let key_id = key_id.into();
        if key_id.trim().is_empty() {
            return Err(SigningError::InvalidKeyId);
        }
        Ok(Self {
            key_id,
            signing_key: SigningKey::from_bytes(&seed),
        })
    }

    /// Raw 32-byte RFC 8032 Ed25519 public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Lowercase hex SHA-256 fingerprint of the 32-byte public key.
    pub fn public_key_sha256(&self) -> String {
        format!("{:x}", Sha256::digest(self.public_key_bytes()))
    }
}

impl ManifestSigner for Ed25519TestSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn sign_manifest(&self, manifest_bytes: &[u8]) -> Result<PackageSignature, SigningError> {
        let sig: Signature = self.signing_key.sign(manifest_bytes);
        Ok(PackageSignature {
            algorithm: ED25519_ALGORITHM.to_owned(),
            key_id: self.key_id.clone(),
            value: STANDARD.encode(sig.to_bytes()),
        })
    }
}

/// Verify a format-2 [`PackageSignature`] over exact `manifest.json` bytes using
/// a raw 32-byte RFC 8032 Ed25519 public key and expected publisher key identifier.
pub fn verify_manifest_signature(
    public_key_bytes: &[u8; 32],
    expected_key_id: &str,
    manifest_bytes: &[u8],
    signature: &PackageSignature,
) -> Result<(), SigningError> {
    if signature.algorithm != ED25519_ALGORITHM {
        return Err(SigningError::UnsupportedAlgorithm);
    }
    if expected_key_id.trim().is_empty() || signature.key_id != expected_key_id {
        return Err(SigningError::KeyIdMismatch);
    }
    let verifying_key =
        VerifyingKey::from_bytes(public_key_bytes).map_err(|_| SigningError::InvalidPublicKey)?;
    let decoded = STANDARD
        .decode(signature.value.as_bytes())
        .map_err(|_| SigningError::InvalidSignatureEncoding)?;
    let sig_bytes: [u8; 64] = decoded
        .try_into()
        .map_err(|_| SigningError::InvalidSignatureEncoding)?;
    let sig = Signature::from_bytes(&sig_bytes);
    verifying_key
        .verify(manifest_bytes, &sig)
        .map_err(|_| SigningError::VerificationFailed)
}
