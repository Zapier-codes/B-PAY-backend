//! Converts the human-facing alphanumeric `instrument_id` (see `hashing.rs`)
//! into a **cryptographic signal**: a deterministic Ed25519 keypair whose
//! public half is what actually gets anchored on a decentralized ledger or
//! referenced by a smart contract, instead of the plain alphanumeric string.
//!
//! Why this exists: an alphanumeric ID like `BOE-9F3K2N8QZR1A` is a fine
//! *label*, but it isn't itself something a smart contract can use to
//! verify anything — it's just a string. A cryptographic signal is a public
//! key (and, when needed, a signature over it) that a contract or any third
//! party can verify without trusting a central server. Deriving it
//! deterministically from the alphanumeric ID (via HKDF, keyed on a
//! server-held secret) means:
//!   - the same instrument always maps to the same on-chain signal
//!     (idempotent — no accidental double-anchoring on retries), and
//!   - the mapping cannot be recomputed by anyone without the HMAC key,
//!     even though the alphanumeric ID itself may be public.
//!
//! This module does not talk to any blockchain — it only produces the
//! signal. Anchoring it (as a contract call, an event log entry, a Merkle
//! leaf, etc.) is the smart-contract layer's job and is intentionally out
//! of scope here so this crate stays chain-agnostic.

use ed25519_dalek::{SigningKey, VerifyingKey};
use hkdf::Hkdf;
use sha2::Sha256;

#[derive(Debug, thiserror::Error)]
pub enum CryptoSignalError {
    #[error("HKDF expand failed (unexpected output length request)")]
    ExpandFailed,
}

/// The cryptographic signal derived from an instrument's alphanumeric ID.
/// `public_key_hex` is what gets anchored/published; the corresponding
/// signing key stays server-side (or is handed to whichever party should
/// be able to sign on the instrument's behalf) and is never stored here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CryptoSignal {
    /// The alphanumeric ID this signal was derived from, kept alongside it
    /// so a human-facing UI and an on-chain reference can be cross-checked.
    pub instrument_id: String,
    /// Ed25519 public key, hex-encoded — the actual on-chain anchor.
    pub public_key_hex: String,
}

/// Derives a deterministic Ed25519 keypair from `instrument_id`, keyed on
/// `hmac_key` (the same server secret used elsewhere in this crate — reuse
/// it rather than introducing a second secret to manage). Returns the
/// public signal plus the signing key, so the caller can immediately sign
/// the instrument's `content_hash` if desired (see `lib.rs::execute_instrument`,
/// which does this for the decentralized jurisdiction path).
pub fn derive_crypto_signal(
    hmac_key: &[u8],
    instrument_id: &str,
) -> Result<(CryptoSignal, SigningKey), CryptoSignalError> {
    // HKDF-Extract-and-Expand: hmac_key is the input keying material, the
    // instrument_id is the (public) info/context string. This is the
    // standard, audited way to turn "a secret + a label" into fresh key
    // material — do not hand-roll this with a bare SHA256 concatenation.
    let hk = Hkdf::<Sha256>::new(None, hmac_key);
    let mut seed = [0u8; 32];
    hk.expand(instrument_id.as_bytes(), &mut seed)
        .map_err(|_| CryptoSignalError::ExpandFailed)?;

    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    Ok((
        CryptoSignal {
            instrument_id: instrument_id.to_string(),
            public_key_hex: hex::encode(verifying_key.to_bytes()),
        },
        signing_key,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_is_deterministic_for_same_id_and_key() {
        let key = b"test-secret-key-do-not-use-in-prod";
        let (sig1, _) = derive_crypto_signal(key, "BOE-9F3K2N8QZR1A").unwrap();
        let (sig2, _) = derive_crypto_signal(key, "BOE-9F3K2N8QZR1A").unwrap();
        assert_eq!(sig1.public_key_hex, sig2.public_key_hex);
    }

    #[test]
    fn different_instrument_ids_give_different_signals() {
        let key = b"test-secret-key-do-not-use-in-prod";
        let (sig1, _) = derive_crypto_signal(key, "BOE-AAAAAAAAAAAA").unwrap();
        let (sig2, _) = derive_crypto_signal(key, "BOE-BBBBBBBBBBBB").unwrap();
        assert_ne!(sig1.public_key_hex, sig2.public_key_hex);
    }

    #[test]
    fn public_key_hex_is_valid_ed25519_length() {
        let key = b"test-secret-key-do-not-use-in-prod";
        let (sig, _) = derive_crypto_signal(key, "BOE-9F3K2N8QZR1A").unwrap();
        // Ed25519 public keys are 32 bytes -> 64 hex chars.
        assert_eq!(sig.public_key_hex.len(), 64);
        assert!(sig.public_key_hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
