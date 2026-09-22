pub mod canton_bridge;
pub mod crypto_signal;
pub mod hashing;
pub mod model;
pub mod template;

use ed25519_dalek::{Signature, Signer, SigningKey};
use model::{BillOfExchange, InstrumentStatus, Jurisdiction};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BoeError {
    #[error("hashing error: {0}")]
    Hashing(#[from] hashing::HashingError),
    #[error("crypto signal error: {0}")]
    CryptoSignal(#[from] crypto_signal::CryptoSignalError),
    #[error("instrument is not ready for execution: consent missing or status is not PendingConsent")]
    NotReadyForExecution,
    #[error("instrument already executed; instruments are immutable once executed")]
    AlreadyExecuted,
}

/// Finalizes a BoE: verifies consent has been captured, computes the
/// tamper-evidence content hash, derives the cryptographic signal from the
/// alphanumeric `instrument_id`, and signs the content hash with the
/// derived (or platform) Ed25519 key. Transitions status to `Executed`.
///
/// - For `Jurisdiction::Decentralized`, the signal is derived from
///   `instrument_id` via `crypto_signal::derive_crypto_signal` and its
///   signing key is used directly — the alphanumeric ID's own hash IS the
///   source of the on-chain identity for this instrument.
/// - For the national-law jurisdictions, an externally supplied
///   `platform_signing_key` (from your KMS/HSM) is used instead, and the
///   crypto signal is attached for reference but isn't load-bearing for
///   enforceability there.
///
/// `hmac_key` should come from a KMS/HSM in production, not an in-memory
/// value.
pub fn execute_instrument(
    boe: &mut BillOfExchange,
    hmac_key: &[u8],
    platform_signing_key: &SigningKey,
) -> Result<Signature, BoeError> {
    if matches!(boe.status, InstrumentStatus::Executed) {
        return Err(BoeError::AlreadyExecuted);
    }
    if !boe.ready_for_execution() {
        return Err(BoeError::NotReadyForExecution);
    }

    let canonical = boe.canonical_content();
    let hash = hashing::content_hash(hmac_key, &canonical)?;
    boe.content_hash = Some(hash.clone());

    let (signal, derived_signing_key) =
        crypto_signal::derive_crypto_signal(hmac_key, &boe.instrument_id)?;
    boe.crypto_signal = Some(signal);
    boe.status = InstrumentStatus::Executed;

    let signature = match boe.jurisdiction {
        Jurisdiction::Decentralized => derived_signing_key.sign(hash.as_bytes()),
        _ => platform_signing_key.sign(hash.as_bytes()),
    };

    Ok(signature)
}

/// Verifies a previously computed signature against the instrument's
/// current content hash — use this to detect tampering after the fact
/// (e.g. before honoring the instrument in a settlement flow, or before a
/// smart contract accepts it as valid).
///
/// For `Jurisdiction::Decentralized` instruments, pass the `VerifyingKey`
/// recovered from `boe.crypto_signal.public_key_hex` — that's the actual
/// verification anchor, not `platform_signing_key`'s public half.
pub fn verify_instrument(
    boe: &BillOfExchange,
    verifying_key: &ed25519_dalek::VerifyingKey,
    signature: &Signature,
) -> bool {
    match &boe.content_hash {
        Some(hash) => verifying_key.verify_strict(hash.as_bytes(), signature).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConsentRecord, Party, Tenor};
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn sample_party(name: &str) -> Party {
        Party {
            full_name: name.to_string(),
            address: "123 Example St".to_string(),
            internal_ref: format!("user_{}", name),
        }
    }

    fn draft_boe(jurisdiction: Jurisdiction) -> BillOfExchange {
        BillOfExchange {
            instrument_id: "BOE-TESTID1234".to_string(),
            jurisdiction,
            drawer: sample_party("Drawer"),
            drawee: sample_party("Drawee"),
            payee: sample_party("Payee"),
            amount_minor_units: 150_000,
            currency: "USD".to_string(),
            tenor: Tenor::DaysAfterSight(30),
            date_of_issue: Utc::now().date_naive(),
            place_of_issue: "Remote / Global".to_string(),
            status: InstrumentStatus::PendingConsent,
            consent: Some(ConsentRecord {
                consent_event_id: "consent_1".to_string(),
                method: "otp_confirmed".to_string(),
                timestamp: Utc::now(),
            }),
            identity_fingerprint: None,
            content_hash: None,
            crypto_signal: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn execution_requires_consent() {
        let mut boe = draft_boe(Jurisdiction::Decentralized);
        boe.consent = None;
        let hmac_key = b"test-hmac-key";
        let signing_key = SigningKey::generate(&mut OsRng);
        let result = execute_instrument(&mut boe, hmac_key, &signing_key);
        assert!(matches!(result, Err(BoeError::NotReadyForExecution)));
    }

    #[test]
    fn decentralized_execution_derives_signal_and_verifies_with_it() {
        let mut boe = draft_boe(Jurisdiction::Decentralized);
        let hmac_key = b"test-hmac-key";
        let platform_key = SigningKey::generate(&mut OsRng); // unused for this jurisdiction's signing

        let signature = execute_instrument(&mut boe, hmac_key, &platform_key).unwrap();
        assert!(boe.content_hash.is_some());
        assert!(matches!(boe.status, InstrumentStatus::Executed));

        let signal = boe.crypto_signal.clone().expect("signal derived on execution");
        let recomputed_signal =
            crypto_signal::derive_crypto_signal(hmac_key, &boe.instrument_id).unwrap();
        assert_eq!(signal.public_key_hex, recomputed_signal.0.public_key_hex);

        let key_bytes: [u8; 32] = hex::decode(&signal.public_key_hex)
            .unwrap()
            .try_into()
            .unwrap();
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes).unwrap();
        assert!(verify_instrument(&boe, &verifying_key, &signature));
    }

    #[test]
    fn national_jurisdiction_uses_platform_key_but_still_attaches_signal() {
        let mut boe = draft_boe(Jurisdiction::International);
        let hmac_key = b"test-hmac-key";
        let platform_key = SigningKey::generate(&mut OsRng);
        let platform_verifying_key = platform_key.verifying_key();

        let signature = execute_instrument(&mut boe, hmac_key, &platform_key).unwrap();
        assert!(boe.crypto_signal.is_some()); // attached for reference
        assert!(verify_instrument(&boe, &platform_verifying_key, &signature));
    }
}
