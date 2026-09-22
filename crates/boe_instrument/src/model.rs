//! Data model for a Bill of Exchange (BoE) instrument.
//!
//! A bill of exchange is, at its legal core, an unconditional written order
//! from one party (the drawer) instructing another (the drawee) to pay a
//! certain sum, on demand or at a determinable future time, to a named
//! payee or to bearer. These required elements are common across most
//! jurisdictions (UK Bills of Exchange Act 1882 s.3; US UCC Art. 3-104;
//! the 1988 UNCITRAL Convention on International Bills of Exchange), and
//! this model treats them as mandatory regardless of `Jurisdiction`.

use crate::crypto_signal::CryptoSignal;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Jurisdiction determines which legal template/wording is rendered, and
/// which verification path the instrument relies on.
///
/// `Decentralized` is not a jurisdiction in the legal sense at all — it
/// means the instrument makes **no claim** to a national legal system and
/// is instead designed to be anchored on a smart-contract/DLT layer, with
/// `BillOfExchange::crypto_signal` as its verifiable anchor instead of a
/// court-recognized legal form. Be precise with anyone relying on this:
/// choosing "no jurisdiction" does not mean "recognized everywhere" — it
/// means "recognized only by whatever the counterparties' contract or
/// platform terms say it means," which is a narrower guarantee than a
/// national instrument backed by that country's courts. Where cross-border
/// legal recognition of an *electronic* instrument is the actual goal
/// (rather than avoiding legal recognition entirely), the real-world path
/// is jurisdictions adopting the UNCITRAL Model Law on Electronic
/// Transferable Records (MLETR) — e.g. the UK's Electronic Trade Documents
/// Act 2023, Singapore's ETA amendments — which give an electronic
/// instrument the same legal status as its paper equivalent. `Decentralized`
/// mode here does not depend on MLETR, but pairing the two (an MLETR-eligible
/// electronic record whose integrity is anchored via `crypto_signal`) is the
/// route to something both decentralized *and* legally enforceable, if that
/// combination is actually what's wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Jurisdiction {
    International,
    UnitedKingdom,
    UnitedStates,
    Nigeria,
    India,
    /// No national legal system claimed; verified via `crypto_signal` and
    /// whatever smart contract/platform terms the counterparties agreed to.
    Decentralized,
}

/// A named party to the instrument. Real name/address are required for
/// legal validity — these are never replaced by a hash, even in
/// `Jurisdiction::Decentralized` mode (a counterparty still needs to know
/// who they're dealing with; the hash/signal secures the instrument's
/// *integrity*, not its parties' identities).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub full_name: String,
    pub address: String,
    /// Internal reference to the party's account/user record. Not the same
    /// as `identity_fingerprint` on the instrument — this is a plain FK,
    /// used for lookups, not a security control.
    pub internal_ref: String,
}

/// When payment falls due.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tenor {
    OnDemand,
    FixedDate(NaiveDate),
    DaysAfterSight(u32),
    DaysAfterDate(u32),
}

/// Provenance of the explicit consent that authorized this instrument.
/// This must be populated from a real user action (e.g. an e-signature
/// event, an authenticated confirmation step) — not inferred from passive
/// session tracking. Passive signals belong in `identity_fingerprint`
/// (fraud/dedup only), never here. This requirement applies in
/// `Jurisdiction::Decentralized` mode too: "decentralized" changes who
/// verifies the instrument, not whether the drawer actually authorized it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub consent_event_id: String,
    pub method: String, // e.g. "otp_confirmed", "esignature", "authenticated_click"
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstrumentStatus {
    Draft,
    /// Ready for signature; legal fields locked but not yet content-hashed.
    PendingConsent,
    /// Consent captured, content hash + signature computed; immutable from
    /// here on. Only `content_hash`-verified copies should be treated as
    /// authoritative.
    Executed,
    Void,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillOfExchange {
    /// Alphanumeric reference, e.g. "BOE-9F3K2N8QZR1A" (see hashing::generate_instrument_id).
    /// This is the human-facing label. For `Jurisdiction::Decentralized`
    /// instruments, `crypto_signal` (derived from this ID) is the value
    /// actually anchored on-chain — the two are linked but serve different
    /// audiences (people vs. contracts).
    pub instrument_id: String,
    pub jurisdiction: Jurisdiction,

    pub drawer: Party,
    pub drawee: Party,
    pub payee: Party,

    pub amount_minor_units: i64, // store money as integer minor units, never float
    pub currency: String,        // ISO 4217, e.g. "USD", "NGN"

    pub tenor: Tenor,
    pub date_of_issue: NaiveDate,
    pub place_of_issue: String,

    pub status: InstrumentStatus,
    pub consent: Option<ConsentRecord>,

    /// Internal fraud/dedup signal only (see hashing::hash_session_signals).
    /// Never rendered on the instrument document.
    pub identity_fingerprint: Option<String>,

    /// Tamper-evidence hash over the canonical serialization of the fields
    /// above, computed once at execution time (see hashing::content_hash).
    pub content_hash: Option<String>,

    /// The public cryptographic signal derived from `instrument_id` (see
    /// `crypto_signal::derive_crypto_signal`), populated at execution time.
    /// This is what a smart contract or third party verifies against —
    /// required for `Jurisdiction::Decentralized`, optional (but harmless)
    /// for the national-law jurisdictions, since it doesn't replace their
    /// verification path.
    pub crypto_signal: Option<CryptoSignal>,

    pub created_at: DateTime<Utc>,
}

impl BillOfExchange {
    /// Returns the subset of fields that make up the instrument's legal
    /// content, in a stable field order, as canonical JSON — the input to
    /// `hashing::content_hash`. Keeping this separate from `serde_json` on
    /// the whole struct avoids the hash silently changing if you add
    /// bookkeeping fields (status, timestamps) later.
    pub fn canonical_content(&self) -> String {
        serde_json::json!({
            "instrument_id": self.instrument_id,
            "jurisdiction": self.jurisdiction,
            "drawer": self.drawer,
            "drawee": self.drawee,
            "payee": self.payee,
            "amount_minor_units": self.amount_minor_units,
            "currency": self.currency,
            "tenor": self.tenor,
            "date_of_issue": self.date_of_issue,
            "place_of_issue": self.place_of_issue,
        })
        .to_string()
    }

    /// Guards against finalizing an instrument without explicit consent.
    /// Call this before computing the content hash / signature / signal.
    pub fn ready_for_execution(&self) -> bool {
        self.consent.is_some() && matches!(self.status, InstrumentStatus::PendingConsent)
    }
}
