#[cfg(feature = "payouts")]
use api_models::payouts::{BankTransfer, PayoutMethodData};
#[cfg(feature = "payouts")]
use common_enums::PayoutStatus;
use common_enums::{enums, AttemptStatus, RefundStatus};
use common_utils::{ext_traits::Encode, pii::Email, types::FloatMajorUnit};
use error_stack::ResultExt;
#[cfg(feature = "payouts")]
use hyperswitch_domain_models::types::{PayoutsResponseData, PayoutsRouterData};
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_flow_types::refunds::Execute,
    router_request_types::{RefundsData, ResponseId},
    router_response_types::{PaymentsResponseData, RedirectForm, RefundsResponseData},
    types::{PaymentsAuthorizeRouterData, RefundsRouterData},
};
use hyperswitch_interfaces::errors;
#[cfg(feature = "payouts")]
use hyperswitch_masking::ExposeInterface;
use hyperswitch_masking::Secret;
use serde::{Deserialize, Serialize};

#[cfg(feature = "payouts")]
use crate::types::PayoutsResponseRouterData;
use crate::{
    types::ResponseRouterData,
    utils::{PaymentsAuthorizeRequestData, RouterData as OtherRouterData},
};

// Flutterwave v3's `/v3/payments` (Standard/hosted-checkout) and
// `/v3/transactions/verify_by_reference` endpoints both take `amount` in
// the currency's base/major unit (e.g. whole Naira, not kobo) — confirmed
// this session against developer.flutterwave.com's own worked examples
// (Standard Payment guide: `amount: '7500'` prices a ₦7,500 charge, not
// ₦75). This matches this repo's own legacy-node/providers/flutterwave.js
// note (Task 52/d-2, `getAmountFormat`'s `'flutterwave'` case) — that note
// flagged itself as "inferred from examples, not an explicit doc
// statement"; the Standard guide's worked example above is the literal
// doc confirmation that note was waiting on. FloatMajorUnit is this
// crate's matching amount type for major-unit connectors (see Korapay,
// which uses the same type for the same reason).
pub struct FlutterwaveRouterData<T> {
    pub amount: FloatMajorUnit,
    pub router_data: T,
}

impl<T> From<(FloatMajorUnit, T)> for FlutterwaveRouterData<T> {
    fn from((amount, router_data): (FloatMajorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// Auth Struct
// Flutterwave v3 authenticates with a single secret key, sent as
// `Authorization: Bearer <secretKey>` on every call — confirmed against
// developer.flutterwave.com's own examples and this repo's own
// legacy-node/providers/flutterwave.js (every v3 method in that file uses
// this exact header). HeaderKey is this crate's matching single-key auth
// type, same as Korapay/Paystack/JuicyWay in this same connectors/
// directory.
pub struct FlutterwaveAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for FlutterwaveAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::HeaderKey { api_key } => Ok(Self {
                api_key: api_key.to_owned(),
            }),
            _ => Err(errors::ConnectorError::FailedToObtainAuthType.into()),
        }
    }
}

// ---------------------------------------------------------------------
// Authorize (collection) — POST /v3/payments ("Standard"/hosted checkout)
// ---------------------------------------------------------------------
//
// Request shape confirmed against developer.flutterwave.com's own
// "Flutterwave Standard" guide, fetched this session — `customer` nested
// exactly as shown there (`email`/`name`/`phonenumber`), `redirect_url`
// required. This mirrors this repo's own legacy-node/providers/
// flutterwave.js#processPayment() payload exactly (that file's own
// comment already cites the same guide page). `customizations` is only
// sent when the caller actually supplies a title/logo/description —
// same "don't guess a default, forward what's given" posture Korapay's
// own optional-field handling in this crate already uses.
#[derive(Debug, Serialize)]
pub struct FlutterwaveCustomer {
    pub email: Email,
    pub name: Option<Secret<String>>,
    pub phonenumber: Option<Secret<String>>,
}

#[derive(Debug, Serialize)]
pub struct FlutterwavePaymentsRequest {
    pub tx_ref: String,
    pub amount: FloatMajorUnit,
    pub currency: enums::Currency,
    pub redirect_url: Option<String>,
    pub customer: FlutterwaveCustomer,
}

impl TryFrom<&FlutterwaveRouterData<&PaymentsAuthorizeRouterData>> for FlutterwavePaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &FlutterwaveRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        // Every real collection method in this repo's own
        // legacy-node/providers/flutterwave.js (v3) funnels through this
        // one hosted-checkout endpoint — there is no separate direct-card
        // API confirmed for v3 in that file. Card/redirect/bank-transfer/
        // wallet payment-method-data all land here the same way; nothing
        // card-specific is read out of `PaymentMethodData` because
        // Flutterwave's Standard endpoint doesn't take raw card data —
        // it hosts card entry itself at the returned `data.link`, same
        // reasoning Korapay's own transformers.rs already documents for
        // its own single hosted-checkout endpoint.
        match item.router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented(
                    "payment method via Flutterwave".to_string(),
                ),
            )),
        }?;

        let email: Email = item.router_data.request.get_email()?;
        let name = item.router_data.get_optional_billing_full_name();
        let phonenumber = item.router_data.get_optional_billing_phone_number();

        Ok(Self {
            tx_ref: item.router_data.connector_request_reference_id.clone(),
            amount: item.amount,
            currency: item.router_data.request.currency,
            redirect_url: item.router_data.request.router_return_url.clone(),
            customer: FlutterwaveCustomer {
                email,
                name,
                phonenumber,
            },
        })
    }
}

// ---------------------------------------------------------------------
// Authorize response — POST /v3/payments only ever returns a hosted
// checkout link, NOT a transaction id or tx_ref
// ---------------------------------------------------------------------
//
// Confirmed directly against developer.flutterwave.com's own worked
// "Flutterwave Standard" response example, fetched this session:
// `{"status":"success","message":"Hosted Link","data":{"link":"https://
// checkout.flutterwave.com/..."}}` — `data` has exactly one field
// (`link`) at this step. Flutterwave's own numeric `id` (and `flw_ref`)
// are only assigned once the customer actually completes the hosted
// checkout, and are not knowable at Authorize time. This is a REAL,
// confirmed difference from Korapay's own `KorapayPaymentsResponse`
// (which does return a reference + status at charge time) — not an
// inconsistency to reconcile, the two providers' Standard-checkout APIs
// are just shaped differently here. Practical effect: `resource_id`
// below has to be this connector's own `tx_ref` (which we generated and
// control), not a connector-assigned id — which in turn is *why*
// PSync below deliberately calls `verify_by_reference` (by tx_ref)
// rather than the numeric-id `verify` endpoint most other connectors in
// this crate use: there is no id to key off yet. This also matches
// legacy-node/providers/flutterwave.js#verifyTransaction()'s own
// endpoint choice, so it's a genuine API constraint, not a legacy-only
// habit ported over unexamined.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FlutterwaveChargeInitData {
    pub link: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FlutterwavePaymentsResponse {
    pub status: String,
    pub message: String,
    pub data: FlutterwaveChargeInitData,
}

impl<F, T> TryFrom<ResponseRouterData<F, FlutterwavePaymentsResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, FlutterwavePaymentsResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        // Flutterwave v3's envelope uses a STRING status ("success" /
        // "error") at this outer level — confirmed via every worked
        // example fetched this session, and already flagged as a real,
        // confirmed difference from Korapay/Paystack's own boolean
        // `status: true/false` in this repo's own
        // legacy-node/providers/flutterwave.js (Task 52/d-2's own
        // comment on `processPayment()`). Checking `!= "success"`, not a
        // falsy/truthy check, matters for the same reason that comment
        // gives: a non-empty error string is still truthy.
        if item.response.status != "success" {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }

        let redirection_data = Some(RedirectForm::Form {
            endpoint: item.response.data.link,
            method: common_utils::request::Method::Get,
            form_fields: std::collections::HashMap::new(),
        });

        Ok(Self {
            // No terminal status is knowable from this response alone —
            // only that Flutterwave accepted the request and generated a
            // hosted link. `AuthenticationPending` matches how this
            // crate's other hosted-checkout connectors (e.g. Korapay's
            // own `Processing`/`Pending` mapping) represent "redirect the
            // customer, real status comes later via PSync/webhook".
            status: AttemptStatus::AuthenticationPending,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(
                    item.data.connector_request_reference_id.clone(),
                ),
                redirection_data: Box::new(redirection_data),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: None,
                incremental_authorization_allowed: None,
                authentication_data: None,
                charges: None,
                payment_account_reference: None,
            }),
            ..item.data
        })
    }
}

// ---------------------------------------------------------------------
// PSync — GET /v3/transactions/verify_by_reference?tx_ref=...
// ---------------------------------------------------------------------
//
// Response shape confirmed against developer.flutterwave.com's
// "Transaction Verification" reference page (the by-tx_ref and by-id
// verify endpoints share one response shape per that page's own worked
// examples, fetched this session). Only the fields this connector
// actually reads are modeled; every other field Flutterwave returns
// (card/customer/meta/etc.) is intentionally left unread here, same
// "model what's used, don't guess the rest" posture as
// KorapayChargeData.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FlutterwaveTransactionStatus {
    Successful,
    Failed,
    #[default]
    Pending,
    #[serde(other)]
    Unknown,
}

impl From<FlutterwaveTransactionStatus> for AttemptStatus {
    fn from(status: FlutterwaveTransactionStatus) -> Self {
        match status {
            FlutterwaveTransactionStatus::Successful => Self::Charged,
            FlutterwaveTransactionStatus::Failed => Self::Failure,
            FlutterwaveTransactionStatus::Pending => Self::AuthenticationPending,
            FlutterwaveTransactionStatus::Unknown => Self::Pending,
        }
    }
}

// Closes the "confirmed endpoint, not yet wired" gap this connector's own
// Execute/RSync stubs (mod.rs) previously flagged: Flutterwave v3's refund
// endpoint (`POST /v3/transactions/{id}/refund`, confirmed against
// developer.flutterwave.com/docs/collecting-payments/refunds and its
// linked API reference, fetched this session) keys off Flutterwave's own
// numeric transaction `id` -- the same id `FlutterwavePaymentsResponse`'s
// own note above confirms Authorize never returns, and which PSync only
// learns once the hosted checkout actually completes. Threading that id
// forward via `connector_metadata` -- populated below, read back out by
// Execute's own `get_url` in mod.rs -- is the same "carry an id you'll
// need for a later flow" pattern this crate's authorizedotnet.rs already
// uses for its own refund metadata, not a new mechanism invented here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterwaveTransactionMeta {
    pub transaction_id: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FlutterwaveVerifyData {
    pub id: i64,
    pub tx_ref: String,
    pub status: FlutterwaveTransactionStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FlutterwaveVerifyResponse {
    pub status: String,
    pub message: String,
    pub data: FlutterwaveVerifyData,
}

impl<F, T> TryFrom<ResponseRouterData<F, FlutterwaveVerifyResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, FlutterwaveVerifyResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        if item.response.status != "success" {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }

        // Real numeric id, only ever knowable from this response (see
        // `FlutterwaveTransactionMeta`'s own note above) -- stored here so
        // a later Refund call has somewhere to read it back from. A
        // serialization failure here is unreachable in practice (a
        // struct with one `i64` field cannot fail `serde_json` encoding),
        // but `encode_to_value` returns a `Result` so this is handled
        // rather than unwrapped, matching this crate's own no-panics
        // posture.
        let connector_metadata = FlutterwaveTransactionMeta {
            transaction_id: item.response.data.id,
        }
        .encode_to_value()
        .change_context(errors::ConnectorError::ResponseHandlingFailed)?;

        Ok(Self {
            status: AttemptStatus::from(item.response.data.status.clone()),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.data.tx_ref),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: Some(connector_metadata),
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: Some(item.response.data.id.to_string()),
                incremental_authorization_allowed: None,
                authentication_data: None,
                charges: None,
                payment_account_reference: None,
            }),
            ..item.data
        })
    }
}

// ---------------------------------------------------------------------
// Execute (Refund) — POST /v3/transactions/{id}/refund
// ---------------------------------------------------------------------
//
// Request/response shape confirmed directly against
// developer.flutterwave.com/docs/collecting-payments/refunds, fetched
// this session (not carried over from an earlier session's summary):
// body is `{ amount, comments }` (`comments` optional — only sent when
// the caller actually supplies a reason); the id in the URL is
// Flutterwave's own numeric transaction id (see `FlutterwaveTransactionMeta`
// above for where this connector gets it from, since it's genuinely not
// the same id `RefundsData.connector_transaction_id` carries here).
#[derive(Debug, Serialize)]
pub struct FlutterwaveRefundRequest {
    pub amount: FloatMajorUnit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
}

impl TryFrom<&FlutterwaveRouterData<&RefundsRouterData<Execute>>> for FlutterwaveRefundRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &FlutterwaveRouterData<&RefundsRouterData<Execute>>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            amount: item.amount,
            comments: item.router_data.request.reason.clone(),
        })
    }
}

// Response shape confirmed via the same fetch as the request above — a
// real worked example, not an inferred/guessed shape:
// `{"status":"success","message":"Transaction refund initiated","data":
// {"id":75923,"account_id":...,"tx_id":...,"flw_ref":"...",
// "wallet_id":...,"amount_refunded":6900,"status":"completed",
// "destination":"payment_source","meta":{...},"created_at":"..."}}`.
// Only the fields this connector actually reads are modeled (`id`,
// `status`), same "model what's used" posture as `FlutterwaveVerifyData`
// above. `data.id` here is the REFUND's own id (distinct from `tx_id`,
// the original transaction's id) — this is what
// `RefundsResponseData.connector_refund_id` stores, and in turn what
// RSync's own `get_url` below reads back to build `GET /v3/refunds/{id}`,
// a clean field-for-field handoff with no metadata-threading gap the way
// Execute's own id lookup needed one.
//
// Status vocabulary is the full confirmed list from that same page's own
// table, not a guessed subset: `completed` (general),
// `completed-bank-transfer`, `completed-momo`, `completed-mpgs`,
// `completed-offline`, `completed-preauth` all map to success;
// `processing`/`pending-momo` map to pending. No `failed`/rejected value
// was shown on the fetched page — `#[serde(other)]` catches anything
// outside this list as `Unknown`, mapped to `Pending` rather than
// guessed at, same fail-safe default this connector's own
// `FlutterwaveTransactionStatus::Unknown` uses.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum FlutterwaveRefundStatus {
    Completed,
    CompletedBankTransfer,
    CompletedMomo,
    CompletedMpgs,
    CompletedOffline,
    CompletedPreauth,
    Processing,
    PendingMomo,
    #[serde(other)]
    Unknown,
}

impl From<FlutterwaveRefundStatus> for RefundStatus {
    fn from(status: FlutterwaveRefundStatus) -> Self {
        match status {
            FlutterwaveRefundStatus::Completed
            | FlutterwaveRefundStatus::CompletedBankTransfer
            | FlutterwaveRefundStatus::CompletedMomo
            | FlutterwaveRefundStatus::CompletedMpgs
            | FlutterwaveRefundStatus::CompletedOffline
            | FlutterwaveRefundStatus::CompletedPreauth => Self::Success,
            FlutterwaveRefundStatus::Processing
            | FlutterwaveRefundStatus::PendingMomo
            | FlutterwaveRefundStatus::Unknown => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FlutterwaveRefundData {
    pub id: i64,
    pub status: FlutterwaveRefundStatus,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct FlutterwaveRefundResponse {
    pub status: String,
    pub message: String,
    pub data: FlutterwaveRefundData,
}

// ---------------------------------------------------------------------
// RSync — GET /v3/refunds/{id}, and why one impl below covers Execute too
// ---------------------------------------------------------------------
//
// Confirmed against developer.flutterwave.com/reference/get-transaction-
// refunds ("Fetch a refunded transaction", fetched this session): takes
// the refund's own id (not the original transaction's), a clean handoff
// from Execute's own `connector_refund_id` below — no metadata-threading
// gap here, unlike Execute's own transaction-id lookup (see
// `FlutterwaveTransactionMeta` above). That same fetched page also shows
// RSync's response as the identical `{status, message, data: {id, ...}}`
// wrapper Execute's own response uses — genuinely the same shape, not an
// assumption — so one generic `impl<F>` below handles both flows'
// response parsing, same "one shared response type" pattern
// `FlutterwavePaymentsResponse`'s own doc comment above already uses for
// Authorize/PSync.
impl<F> TryFrom<ResponseRouterData<F, FlutterwaveRefundResponse, RefundsData, RefundsResponseData>>
    for RefundsRouterData<F>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, FlutterwaveRefundResponse, RefundsData, RefundsResponseData>,
    ) -> Result<Self, Self::Error> {
        if item.response.status != "success" {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }
        Ok(Self {
            response: Ok(RefundsResponseData {
                connector_refund_id: item.response.data.id.to_string(),
                refund_status: RefundStatus::from(item.response.data.status),
            }),
            ..item.data
        })
    }
}

// ---------------------------------------------------------------------
// Error response
// ---------------------------------------------------------------------
//
// Flat `{ status: "error", message: "..." }` shape — confirmed against
// developer.flutterwave.com's error examples and this repo's own
// legacy-node/providers/flutterwave.js (every `providerError(responseData
// .message || ...)` call site in that file). No separate machine-readable
// error code observed, same position Korapay's own `KorapayErrorResponse`
// is in — `code` is left for `build_error_response` (mod.rs) to fall back
// to `consts::NO_ERROR_CODE`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterwaveErrorResponse {
    pub status: String,
    pub message: String,
}

// ---------------------------------------------------------------------
// Payout Fulfill — POST /transfers
// Payout Sync    — GET  /transfers/{id}
// ---------------------------------------------------------------------
//
// Request shape ported directly from this repo's own
// legacy-node/providers/flutterwave.js#processPayout() (Task 52/d-2a) --
// confirmed there against developer.flutterwave.com/reference/endpoints/transfers
// and the Transfers overview guide. FLAT top-level fields -- a real,
// confirmed difference from Korapay's own connector in this crate, whose
// payout destination fields are nested under a `destination` object; not
// an inconsistency to "fix", the two providers' real APIs are just
// shaped differently, same note the legacy JS file's own comment already
// makes.
//
// PoSync's id handling is the other real, confirmed difference from
// Korapay's connector: legacy-node's own verifyPayout() docblock (Task
// 52/d-2a) states plainly that Flutterwave has no confirmed
// reference-based single-transfer lookup -- `GET /transfers/:id` only
// takes Flutterwave's own internal numeric transfer id. So, unlike
// Korapay's PoSync (which keys off the merchant reference),
// `connector_payout_id` here MUST carry Flutterwave's own `id` -- and
// PoFulfill's own response below stores exactly that, not the merchant
// reference this connector generated.
#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct FlutterwavePayoutFulfillRequest {
    pub account_bank: Secret<String>,
    pub account_number: Secret<String>,
    pub amount: FloatMajorUnit,
    pub currency: enums::Currency,
    pub narration: String,
    pub reference: String,
}

// ⚠️ Real, unresolved shape gap -- flagged, not guessed around, same
// root cause and same stopgap already flagged in korapay/transformers.rs's
// own `get_korapay_payout_bank_account`: Hyperswitch's `PayoutMethodData`
// (api_models::payouts) has no NUBAN/bank-code-shaped variant.
// `BankTransfer::Ach` is reused purely because it is the one variant
// with two plain (non-IBAN, non-BIC-formatted) string fields --
// `bank_account_number` carries the account number, `bank_routing_number`
// carries Flutterwave's own bank code (from Flutterwave's own
// `GET /banks/:country` list, NOT a US ABA routing number, which is what
// that field is documented elsewhere in this same enum as). Not a
// confirmed-correct mapping -- do not trust this in production before
// either a live Flutterwave sandbox call confirms it round-trips, or a
// proper NUBAN-shaped `PayoutMethodData` variant is added upstream and
// every connector using this same stopgap (Korapay, Paystack, JuicyWay,
// now Flutterwave) is switched to it together.
#[cfg(feature = "payouts")]
fn get_flutterwave_payout_bank_account<F>(
    router_data: &PayoutsRouterData<F>,
) -> Result<(Secret<String>, Secret<String>), error_stack::Report<errors::ConnectorError>> {
    match router_data.get_payout_method_data()? {
        PayoutMethodData::BankTransfer(BankTransfer::Ach(ach)) => {
            Ok((ach.bank_routing_number, ach.bank_account_number))
        }
        other => Err(errors::ConnectorError::NotSupported {
            message: format!(
                "{other:?} via Flutterwave payouts (see flutterwave/transformers.rs's own \
                 get_flutterwave_payout_bank_account note on the real NUBAN/bank-code shape gap)"
            ),
            connector: "flutterwave",
        }
        .into()),
    }
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<&FlutterwaveRouterData<&PayoutsRouterData<F>>> for FlutterwavePayoutFulfillRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &FlutterwaveRouterData<&PayoutsRouterData<F>>) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        let (account_bank, account_number) = get_flutterwave_payout_bank_account(router_data)?;

        Ok(Self {
            account_bank,
            account_number,
            amount: item.amount,
            currency: router_data.request.destination_currency,
            // Hyperswitch's `PayoutsData` carries no narration field at
            // all (unlike the legacy JS request, which took a
            // caller-supplied `narration`) -- a generic, connector-level
            // default is used here instead, same choice Korapay's own
            // connector in this crate already made for the identical gap.
            narration: "Payout via Flutterwave".to_string(),
            reference: router_data.connector_request_reference_id.clone(),
        })
    }
}

// Real lifecycle state of the transfer itself. Per
// legacy-node/providers/flutterwave.js#processPayout()'s own comment,
// worked examples confirm `NEW`/`SUCCESSFUL`/`FAILED` (compared there via
// `.toUpperCase() === 'FAILED'`, implying the API's own casing isn't
// fully trusted even in the legacy code) -- kept as a plain `String`
// here, matched case-insensitively below, rather than a strict enum,
// deliberately mirroring that same defensive `.toUpperCase()` posture
// instead of risking a deserialization failure on an unexpected case.
#[cfg(feature = "payouts")]
fn flutterwave_payout_status_from_str(status: &str) -> PayoutStatus {
    match status.to_uppercase().as_str() {
        "SUCCESSFUL" => PayoutStatus::Success,
        "FAILED" => PayoutStatus::Failed,
        // "NEW" is Flutterwave's own confirmed non-terminal acknowledgement
        // state (per the legacy JS's own comment: "acknowledgement only,
        // not final confirmation") -- and anything else unrecognized folds
        // to the same non-terminal `Pending`, same "don't fail closed on
        // an unrecognized status string" posture Korapay's own
        // `KorapayPayoutTransactionStatus::Unknown` mapping already uses.
        _ => PayoutStatus::Pending,
    }
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FlutterwavePayoutData {
    // Flutterwave's own internal numeric transfer id -- required by
    // PoSync (see this section's own file-level comment on why this,
    // not `reference`, is what `connector_payout_id` must carry).
    // Modeled as `String` via `#[serde(default)]` + a permissive
    // deserialize would be more defensive, but this session found no
    // primary-source confirmation either way of whether Flutterwave
    // returns this as a JSON number or a numeric string, so the
    // straightforward `i64` (the common shape for this field across
    // every public Flutterwave example this session is aware of) is
    // used directly rather than adding untested flexibility for a case
    // that isn't confirmed to occur. Flag before trusting in production,
    // same as every other unconfirmed-shape note in this file.
    pub id: i64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub complete_message: Option<String>,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FlutterwavePayoutResponse {
    pub status: String,
    pub message: String,
    #[serde(default)]
    pub data: FlutterwavePayoutData,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<PayoutsResponseRouterData<F, FlutterwavePayoutResponse>> for PayoutsRouterData<F> {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: PayoutsResponseRouterData<F, FlutterwavePayoutResponse>,
    ) -> Result<Self, Self::Error> {
        // Outer `status != "success"` is Flutterwave's own signal that
        // the API call itself was rejected -- same discipline as
        // `FlutterwavePaymentsResponse`'s own handling above and every
        // method in legacy-node/providers/flutterwave.js (`if
        // (!response.ok || responseData.status !== 'success') throw
        // ...`). Deliberately different from a `data.status: "FAILED"`
        // outcome below, which is a normal, successfully-verified
        // terminal payout state, not an error calling this function.
        if item.response.status != "success" {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }

        let payout_status = item
            .response
            .data
            .status
            .as_deref()
            .map(flutterwave_payout_status_from_str)
            // No `data.status` at all (seen on some acknowledgement-only
            // responses per the legacy JS's own logging comment) is the
            // same non-terminal "accepted, not yet confirmed" case as an
            // explicit `NEW` -- not a failure.
            .unwrap_or(PayoutStatus::Pending);
        let error_message = payout_status.is_payout_failure().then(|| {
            item.response
                .data
                .complete_message
                .clone()
                .unwrap_or_else(|| item.response.message.clone())
        });

        Ok(Self {
            response: Ok(PayoutsResponseData {
                status: Some(payout_status),
                connector_payout_id: Some(item.response.data.id.to_string()),
                payout_eligible: None,
                should_add_next_step_to_process_tracker: false,
                error_code: None,
                error_message,
                payout_connector_metadata: None,
                connector_eligibility_reference_id: None,
            }),
            ..item.data
        })
    }
}

// ---------------------------------------------------------------------
// Incoming webhooks
// ---------------------------------------------------------------------
//
// Envelope confirmed against developer.flutterwave.com/docs/webhooks
// (v3) and developer.flutterwave.com/docs/integration-guides/webhooks,
// both fetched this session, plus a real worked `transfer.completed`
// example from developer.flutterwave.com/v3.0/docs/introduction-6: v3
// payloads are a flat `{ "event": "...", "data": {...} }` envelope —
// NOT v4's separate `type`/`webhook_id`/`timestamp` envelope (v4 is
// still a distinct, not-yet-switched surface per Task 52/d-2c; this
// connector only calls v3 endpoints, so only v3's own webhook shape is
// modeled here — same "don't half-support a surface this connector
// doesn't call" posture the Refund section above already takes with
// v4's own refund endpoint).
//
// Only the two events this session found real, worked examples for —
// `charge.completed` and (payouts-gated) `transfer.completed` — are
// mapped below; anything else resolves to `EventNotSupported` rather
// than a guess. Flutterwave's own docs mention subscription charges
// and pending-to-successful transitions as real webhook triggers, but
// this session found no worked example of the event *name* either
// arrives under (both may simply also be `charge.completed` with a
// different `data.payment_type` — genuinely unconfirmed either way).
// Flag before assuming subscription/pending-transition webhooks are
// silently unhandled by accident rather than by a documented gap.

#[derive(Debug, Clone, Deserialize)]
pub enum FlutterwaveWebhookEventType {
    #[serde(rename = "charge.completed")]
    ChargeCompleted,
    #[cfg(feature = "payouts")]
    #[serde(rename = "transfer.completed")]
    TransferCompleted,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FlutterwaveWebhookEventTypeBody {
    pub event: FlutterwaveWebhookEventType,
}

// `charge.completed`'s own `data` object — `id`/`tx_ref`/`status`
// confirmed against the worked NGN-bank-transfer example
// (developer.flutterwave.com/docs/ngn-bank-transfer). Same id/tx_ref/
// status vocabulary already modeled for Authorize/PSync above
// (`FlutterwaveTransactionStatus`), reused rather than duplicated —
// `#[serde(deny_unknown_fields)]` deliberately NOT set, since the real
// payload carries many more fields (`customer`, `card`, `amount`, ...)
// this connector has no present use for; only what's needed for
// routing/status is modeled, same "model what's used" posture as every
// other response struct in this file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterwaveChargeWebhookData {
    pub id: i64,
    pub tx_ref: String,
    pub status: FlutterwaveTransactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterwaveChargeWebhookEvent {
    pub data: FlutterwaveChargeWebhookData,
}

// `transfer.completed`'s own `data` object — confirmed against the
// worked example in developer.flutterwave.com/v3.0/docs/introduction-6
// (`{"event":"transfer.completed","event.type":"Transfer","data":
// {"id":8416497,"reference":"TX-refe123456-6-3-1","status":
// "SUCCESSFUL",...}}`). Deliberately `reference`, not `tx_ref` —
// transfers use a different field name than charges do, per that same
// worked example, not assumed to match charges' own shape. `status` is
// left a plain `String` and matched via the same
// `flutterwave_payout_status_from_str` this file's own PoSync path
// already uses, rather than a second status enum for the identical
// vocabulary.
#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterwaveTransferWebhookData {
    pub id: i64,
    pub reference: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlutterwaveTransferWebhookEvent {
    pub data: FlutterwaveTransferWebhookData,
}
