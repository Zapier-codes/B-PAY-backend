//! Maps an executed `BillOfExchange` onto what a Canton Network (Daml)
//! instrument contract needs to be created/disclosed with.
//!
//! HANDOVER.md originally spec'd this against ERC-3643/T-REX (EVM +
//! Solidity), but that path was dropped before any contract code was
//! written: T-REX's only actively-maintained implementation is GPLv3,
//! which doesn't fit this workspace's Apache-2.0 licensing, and no
//! permissively-licensed full substitute exists. Canton + Daml Finance
//! (both Apache-2.0) replace it — see HANDOVER.md's "chain choice
//! resolved as Canton/Daml" section for the full rationale.
//!
//! This module is still **ledger-call-free** by design, same reasoning as
//! the ERC-3643 version it replaces: it produces plain Rust data
//! describing what should be disclosed/created on the ledger, not a call
//! into any specific Ledger API client. Wiring this to Canton's actual
//! Ledger API / JSON API (which Daml template/choice this becomes a
//! `CreateCommand` for) is separate, not-yet-designed integration work.
//!
//! Where the ERC-3643 version needed an Identity Registry + Trusted
//! Issuers Registry because plain ERC-20 has no native holder-eligibility
//! concept, Canton doesn't: eligibility is enforced by the ledger's own
//! authorization model (who can be a signatory/observer on the instrument
//! contract, via each party's Account/custodian relationship in Daml
//! Finance's model), not by a separate on-chain registry contract. So
//! there is deliberately no `IdentityRegistrationRequest`-equivalent here
//! — the parties this module needs to know about are just the drawer/
//! drawee/payee's Canton party IDs, supplied by whichever service manages
//! party onboarding.

use crate::crypto_signal::CryptoSignal;
use crate::model::{BillOfExchange, InstrumentStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum CantonBridgeError {
    #[error("instrument must be Executed before it can be disclosed on Canton")]
    NotExecuted,
    #[error("instrument has no crypto_signal — was it executed via execute_instrument?")]
    MissingCryptoSignal,
    #[error("instrument has no content_hash — was it executed via execute_instrument?")]
    MissingContentHash,
    #[error("no Canton party id supplied for party with internal_ref {0:?}")]
    MissingPartyId(String),
}

/// Which of the instrument's three parties a party-id mapping is for. Kept
/// explicit for the same reason as the ERC-3643 version had `PartyRole`:
/// drawer/drawee/payee have different legal roles even though the mapping
/// treats their party-id shape identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartyRole {
    Drawer,
    Drawee,
    Payee,
}

/// Canton party IDs for whichever parties have been onboarded on the
/// ledger. A party with no id yet simply isn't ready to be a
/// signatory/observer on the instrument contract — that's a valid state,
/// not an error, except where `resolve_party_ids_strict` is asked to
/// require all three (see that function).
#[derive(Debug, Clone, Default)]
pub struct PartyIds {
    pub drawer: Option<String>,
    pub drawee: Option<String>,
    pub payee: Option<String>,
}

impl PartyIds {
    fn get(&self, role: PartyRole) -> Option<&String> {
        match role {
            PartyRole::Drawer => self.drawer.as_ref(),
            PartyRole::Drawee => self.drawee.as_ref(),
            PartyRole::Payee => self.payee.as_ref(),
        }
    }
}

/// One party's resolved Canton identity, alongside the platform's own
/// reference to them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CantonParty {
    pub role: PartyRole,
    /// `Party::internal_ref` — carried through so the caller can join back
    /// to this platform's own account record.
    pub internal_ref: String,
    pub canton_party_id: String,
}

/// The disclosed, instrument-identifying fields that belong on the Daml
/// instrument contract's view — the Canton-side equivalent of the
/// `MintRequest`/claim payload in the ERC-3643 version. `crypto_signal_hex`
/// and `content_hash` are carried as plain disclosed attributes rather than
/// a signing key or a registry claim: `crypto_signal` is Ed25519 and was
/// never meant to double as a Canton-transaction-signing key, same
/// reasoning as it not being an EVM management key in the ERC-3643 design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentDisclosure {
    pub instrument_id: String,
    pub crypto_signal_hex: String,
    pub content_hash: String,
}

/// Resolves the Canton party ids for whichever of the instrument's parties
/// have one in `parties`. Parties with none are silently omitted (returns
/// fewer than 3) — not every party may be onboarded on the ledger yet.
pub fn resolve_party_ids(
    boe: &BillOfExchange,
    parties: &PartyIds,
) -> Vec<CantonParty> {
    let mut resolved = Vec::with_capacity(3);
    for (role, party) in [
        (PartyRole::Drawer, &boe.drawer),
        (PartyRole::Drawee, &boe.drawee),
        (PartyRole::Payee, &boe.payee),
    ] {
        if let Some(canton_party_id) = parties.get(role) {
            resolved.push(CantonParty {
                role,
                internal_ref: party.internal_ref.clone(),
                canton_party_id: canton_party_id.clone(),
            });
        }
    }
    resolved
}

/// Same as `resolve_party_ids`, but requires all three parties to have a
/// Canton party id — use this immediately before creating the instrument
/// contract, where all three genuinely need to be resolvable, rather than
/// tolerating partial resolution.
pub fn resolve_party_ids_strict(
    boe: &BillOfExchange,
    parties: &PartyIds,
) -> Result<Vec<CantonParty>, CantonBridgeError> {
    for (role, party) in [
        (PartyRole::Drawer, &boe.drawer),
        (PartyRole::Drawee, &boe.drawee),
        (PartyRole::Payee, &boe.payee),
    ] {
        if parties.get(role).is_none() {
            return Err(CantonBridgeError::MissingPartyId(party.internal_ref.clone()));
        }
    }
    Ok(resolve_party_ids(boe, parties))
}

/// Builds the disclosed instrument fields for an executed instrument.
/// Refuses instruments that aren't `Executed` — same hard prerequisite as
/// the ERC-3643 version: nothing gets represented on Canton before the
/// consent gate in `execute_instrument` has actually run.
pub fn build_instrument_disclosure(
    boe: &BillOfExchange,
) -> Result<InstrumentDisclosure, CantonBridgeError> {
    if !matches!(boe.status, InstrumentStatus::Executed) {
        return Err(CantonBridgeError::NotExecuted);
    }
    let signal: &CryptoSignal = boe
        .crypto_signal
        .as_ref()
        .ok_or(CantonBridgeError::MissingCryptoSignal)?;
    let content_hash = boe
        .content_hash
        .clone()
        .ok_or(CantonBridgeError::MissingContentHash)?;
    Ok(InstrumentDisclosure {
        instrument_id: boe.instrument_id.clone(),
        crypto_signal_hex: signal.public_key_hex.clone(),
        content_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConsentRecord, Jurisdiction, Party, Tenor};
    use crate::{execute_instrument, hashing};
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

    fn draft_boe() -> BillOfExchange {
        BillOfExchange {
            instrument_id: hashing::generate_instrument_id(
                b"test-hmac-key",
                "user-1",
                "session-1",
                1_700_000_000,
                "nonce-1",
            )
            .expect("id generation"),
            jurisdiction: Jurisdiction::Decentralized,
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

    fn executed_boe() -> BillOfExchange {
        let mut boe = draft_boe();
        let hmac_key = b"test-hmac-key";
        let platform_key = SigningKey::generate(&mut OsRng);
        execute_instrument(&mut boe, hmac_key, &platform_key).expect("execution");
        boe
    }

    #[test]
    fn rejects_unexecuted_instrument() {
        let boe = draft_boe();
        let err = build_instrument_disclosure(&boe).unwrap_err();
        assert!(matches!(err, CantonBridgeError::NotExecuted));
    }

    #[test]
    fn disclosure_carries_signal_and_content_hash() {
        let boe = executed_boe();
        let disclosure = build_instrument_disclosure(&boe).unwrap();
        assert_eq!(disclosure.instrument_id, boe.instrument_id);
        assert_eq!(
            disclosure.crypto_signal_hex,
            boe.crypto_signal.unwrap().public_key_hex
        );
        assert_eq!(disclosure.content_hash, boe.content_hash.unwrap());
    }

    #[test]
    fn skips_parties_with_no_canton_id_yet() {
        let boe = executed_boe();
        let parties = PartyIds {
            drawer: Some("canton-party-drawer".to_string()),
            drawee: None,
            payee: Some("canton-party-payee".to_string()),
        };
        let resolved = resolve_party_ids(&boe, &parties);
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|p| p.role == PartyRole::Drawer));
        assert!(resolved.iter().any(|p| p.role == PartyRole::Payee));
        assert!(!resolved.iter().any(|p| p.role == PartyRole::Drawee));
    }

    #[test]
    fn strict_resolution_requires_all_three_party_ids() {
        let boe = executed_boe();
        let parties = PartyIds {
            drawer: Some("canton-party-drawer".to_string()),
            drawee: None,
            payee: Some("canton-party-payee".to_string()),
        };
        let err = resolve_party_ids_strict(&boe, &parties).unwrap_err();
        assert!(matches!(err, CantonBridgeError::MissingPartyId(_)));
    }
}
