//! Cryptographic hashing utilities for the BoE instrument module.
//!
//! Three distinct hash purposes are kept separate on purpose:
//!   1. `instrument_id`        – short, human-shareable alphanumeric reference.
//!   2. `identity_fingerprint` – internal fraud/dedup signal from session+device
//!      data. Never used as a substitute for the legal drawer/drawee/payee
//!      identity fields on the instrument itself.
//!   3. `content_hash`         – tamper-evidence over the *finalized* instrument
//!      fields, so any later edit is detectable.
//!
//! See `crypto_signal.rs` for the derivation of a public cryptographic
//! signal from the alphanumeric `instrument_id`, used to anchor the
//! instrument on a decentralized/smart-contract layer.
//!
//! All HMACs use a server-held secret (`hmac_key`), which must come from a
//! proper secrets manager / KMS in production — never hardcode it.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, thiserror::Error)]
pub enum HashingError {
    #[error("invalid HMAC key length")]
    InvalidKeyLength,
}

/// Raw signals collected from a user's session, used only for fraud
/// correlation / duplicate detection — never displayed on the instrument
/// and never treated as a legal identity.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSignals {
    pub device_fingerprint: String, // e.g. from a client-side FP library
    pub ip_address: String,
    pub user_agent: String,
    pub session_id: String,
}

/// Computes an HMAC-SHA256 over arbitrary parts, returned as lowercase hex.
pub(crate) fn hmac_hex(hmac_key: &[u8], parts: &[&str]) -> Result<String, HashingError> {
    let mut mac =
        HmacSha256::new_from_slice(hmac_key).map_err(|_| HashingError::InvalidKeyLength)?;
    for part in parts {
        mac.update(part.as_bytes());
        mac.update(b"\x1f"); // unit separator, avoids field-concatenation collisions
    }
    let result = mac.finalize().into_bytes();
    Ok(hex::encode(result))
}

/// Generates a short, alphanumeric, collision-resistant instrument reference
/// such as `BOE-9F3K2N8QZR1A`. Deterministic given the same inputs, which is
/// useful for idempotency (retrying a request won't mint a second instrument).
///
/// This is the *human-facing* identifier. For the decentralized/smart-contract
/// anchor, derive a `crypto_signal::CryptoSignal` from this ID instead of
/// using this string directly on-chain — see `crypto_signal.rs`.
pub fn generate_instrument_id(
    hmac_key: &[u8],
    user_id: &str,
    session_id: &str,
    timestamp_unix: i64,
    nonce: &str,
) -> Result<String, HashingError> {
    let ts = timestamp_unix.to_string();
    let full_hex = hmac_hex(hmac_key, &[user_id, session_id, &ts, nonce])?;

    // Base32-encode the first 8 bytes (16 hex chars) for a short, unambiguous
    // alphanumeric code (Crockford-style alphabet avoids 0/O, 1/I confusion).
    let raw_bytes = hex::decode(&full_hex[..16]).expect("valid hex slice");
    let encoded = data_encoding::BASE32_NOPAD.encode(&raw_bytes);

    Ok(format!("BOE-{}", encoded))
}

/// Hashes session/device signals into a stable fingerprint used purely for
/// internal fraud scoring and duplicate-instrument detection. This is a
/// pseudonymous identifier, not an anonymous one — treat it as personal data
/// under GDPR/NDPR/etc. for retention and access-control purposes.
pub fn hash_session_signals(
    hmac_key: &[u8],
    signals: &SessionSignals,
) -> Result<String, HashingError> {
    hmac_hex(
        hmac_key,
        &[
            &signals.device_fingerprint,
            &signals.ip_address,
            &signals.user_agent,
            &signals.session_id,
        ],
    )
}

/// Computes a tamper-evidence hash over the canonical JSON of the finalized
/// instrument. Call this only after all legal fields are set and immutable;
/// any later mutation must invalidate this hash.
pub fn content_hash(hmac_key: &[u8], canonical_json: &str) -> Result<String, HashingError> {
    hmac_hex(hmac_key, &[canonical_json])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_id_is_deterministic_and_alphanumeric() {
        let key = b"test-secret-key-do-not-use-in-prod";
        let id1 = generate_instrument_id(key, "user_1", "sess_1", 1_700_000_000, "n1").unwrap();
        let id2 = generate_instrument_id(key, "user_1", "sess_1", 1_700_000_000, "n1").unwrap();
        assert_eq!(id1, id2);
        assert!(id1.starts_with("BOE-"));
        assert!(id1[4..].chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn different_nonce_changes_id() {
        let key = b"test-secret-key-do-not-use-in-prod";
        let id1 = generate_instrument_id(key, "user_1", "sess_1", 1_700_000_000, "n1").unwrap();
        let id2 = generate_instrument_id(key, "user_1", "sess_1", 1_700_000_000, "n2").unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn session_fingerprint_is_stable() {
        let key = b"test-secret-key-do-not-use-in-prod";
        let signals = SessionSignals {
            device_fingerprint: "fp_abc123".into(),
            ip_address: "203.0.113.7".into(),
            user_agent: "Mozilla/5.0".into(),
            session_id: "sess_1".into(),
        };
        let h1 = hash_session_signals(key, &signals).unwrap();
        let h2 = hash_session_signals(key, &signals).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // sha256 hex
    }
}
