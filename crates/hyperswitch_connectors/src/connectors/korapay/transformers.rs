#[cfg(feature = "payouts")]
use api_models::payouts::{BankTransfer, PayoutMethodData};
use common_enums::{enums, AttemptStatus};
#[cfg(feature = "payouts")]
use common_enums::PayoutStatus;
use common_utils::{pii::Email, types::FloatMajorUnit};
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_request_types::ResponseId,
    router_response_types::{PaymentsResponseData, RedirectForm},
    types::PaymentsAuthorizeRouterData,
};
#[cfg(feature = "payouts")]
use hyperswitch_domain_models::types::{PayoutsResponseData, PayoutsRouterData};
use hyperswitch_interfaces::errors;
use hyperswitch_masking::Secret;
#[cfg(feature = "payouts")]
use hyperswitch_masking::ExposeInterface;
use serde::{Deserialize, Serialize};

#[cfg(feature = "payouts")]
use crate::types::PayoutsResponseRouterData;
use crate::{
    types::ResponseRouterData,
    // `OtherRouterData` (the crate's own `utils::RouterData` extension
    // trait) is also where `get_payout_method_data()` lives -- one
    // import covers both the Payments helpers already used below and
    // the Payout helpers the new sections need.
    utils::{PaymentsAuthorizeRequestData, RouterData as OtherRouterData},
};

// Korapay's `/api/v1/charges/initialize` and `/api/v1/transactions/disburse`
// endpoints both take amount in the currency's base/major unit (e.g. whole
// Naira, not kobo) -- confirmed against developers.korapay.com/docs and
// against this repo's own legacy-node/providers/korapay.js (Task 7's
// `convertAmountForProvider(..., 'korapay', ...)` no-op). FloatMajorUnit is
// the corresponding hyperswitch amount type for "major-unit, not minor-unit"
// connectors -- see AuthipayRouterData / other FloatMajorUnit connectors in
// this same crate for the established pattern this mirrors.
pub struct KorapayRouterData<T> {
    pub amount: FloatMajorUnit,
    pub router_data: T,
}

impl<T> From<(FloatMajorUnit, T)> for KorapayRouterData<T> {
    fn from((amount, router_data): (FloatMajorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// Auth Struct
// Korapay authenticates with a single secret key, sent as
// `Authorization: Bearer <secretKey>` -- confirmed directly against
// legacy-node/providers/korapay.js, every method in that file uses this
// exact header. HeaderKey is hyperswitch's matching single-key auth type.
pub struct KorapayAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for KorapayAuthType {
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
// Authorize (collection) — POST /api/v1/charges/initialize
// ---------------------------------------------------------------------
//
// Request shape confirmed against legacy-node/providers/korapay.js
// (Korapay's initialize-charge endpoint requires `customer` as a nested
// object; a flat top-level `email` is rejected). Optional
// dynamic-currency-conversion and channel-preference fields are only sent
// when the caller actually supplies them, matching the legacy code's own
// "don't guess, let the provider decide" posture from Task 10/30 --
// Korapay's own docs (Dynamic Currency Conversion, Checkout & Redirect
// pages) require `channels`/`default_channel` to travel together, and
// `payment_currency`/`settlement_currency` to travel together.
#[derive(Debug, Serialize)]
pub struct KorapayCustomer {
    pub email: Email,
    pub name: Option<Secret<String>>,
}

#[derive(Debug, Serialize)]
pub struct KorapayPaymentsRequest {
    pub amount: FloatMajorUnit,
    pub currency: enums::Currency,
    pub reference: String,
    pub customer: KorapayCustomer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_currency: Option<enums::Currency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_currency: Option<enums::Currency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_url: Option<String>,
}

impl TryFrom<&KorapayRouterData<&PaymentsAuthorizeRouterData>> for KorapayPaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &KorapayRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        // Card/redirect/bank-transfer/mobile-money are all funneled through
        // Korapay's single hosted-checkout `charges/initialize` endpoint --
        // there is no separate direct-card API in the legacy integration
        // this connector is replacing (see handover.md Task 77's
        // "one-engine" note: legacy-node/providers/korapay.js only ever
        // called this one endpoint for collection). Any payment-method-data
        // variant lands here the same way; nothing card-specific is read
        // out of `PaymentMethodData` because Korapay's own API doesn't take
        // raw card data on this endpoint -- it hosts card entry itself at
        // the returned checkout_url.
        match item.router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented("payment method via Korapay".to_string()),
            )),
        }?;

        let email: Email = item.router_data.request.get_email()?;
        let name = item.router_data.get_optional_billing_full_name();

        Ok(Self {
            amount: item.amount,
            currency: item.router_data.request.currency,
            reference: item.router_data.connector_request_reference_id.clone(),
            customer: KorapayCustomer { email, name },
            // Task 16/Task 9b companion fields — only forwarded via
            // connector_metadata today in the legacy stack; left `None`
            // here deliberately rather than guessed at from RouterData
            // fields that don't yet carry Korapay-specific DCC/channel
            // intent. Flagged in handover.md as a follow-up, same
            // "confirm before wiring" posture as the rest of this file.
            payment_currency: None,
            settlement_currency: None,
            channels: None,
            default_channel: None,
            redirect_url: item.router_data.request.router_return_url.clone(),
        })
    }
}

// ---------------------------------------------------------------------
// Response — shared by Authorize and PSync
// ---------------------------------------------------------------------
//
// ⚠️ Field names below (`data.reference`, `data.checkout_url`,
// `data.status`) are Korapay's documented `/charges/initialize` and
// `/charges/:reference` (GET) response shape per developers.korapay.com --
// NOT re-confirmed via a live sandbox call in this session (no working
// `rustc` this session to build a throwaway test harness against; see
// handover.md's New-Clone Checklist). legacy-node/providers/korapay.js
// only ever checked the boolean `status`/`data.status` fields and passed
// the rest through untyped, so this struct is new, not a straight port --
// flagging for a live-call confirmation pass before this goes to
// production, same discipline as every other "first real call" note
// elsewhere in this file's history (e.g. Task 42 Part B's payout shape
// fix, which *was* caught this same way).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KorapayTransactionStatus {
    Success,
    Failed,
    #[default]
    Processing,
    Pending,
    #[serde(other)]
    Unknown,
}

impl From<KorapayTransactionStatus> for AttemptStatus {
    fn from(status: KorapayTransactionStatus) -> Self {
        match status {
            KorapayTransactionStatus::Success => Self::Charged,
            KorapayTransactionStatus::Failed => Self::Failure,
            KorapayTransactionStatus::Processing | KorapayTransactionStatus::Pending => {
                Self::AuthenticationPending
            }
            KorapayTransactionStatus::Unknown => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KorapayChargeData {
    pub reference: String,
    pub status: KorapayTransactionStatus,
    pub checkout_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KorapayPaymentsResponse {
    pub status: bool,
    pub message: String,
    pub data: KorapayChargeData,
}

impl<F, T> TryFrom<ResponseRouterData<F, KorapayPaymentsResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, KorapayPaymentsResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        // Outer `status: false` is Korapay's own signal that the API call
        // itself was rejected -- build_error_response (see mod.rs) handles
        // that path for non-2xx responses; a 2xx with `status: false` is
        // treated the same way here rather than silently mapped to a
        // "successful" attempt status, matching every other method in
        // legacy-node/providers/korapay.js (`if (!response.ok ||
        // !responseData.status) throw ...`).
        if !item.response.status {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }

        let redirection_data = item.response.data.checkout_url.clone().map(|url| {
            RedirectForm::Form {
                endpoint: url,
                method: common_utils::request::Method::Get,
                form_fields: std::collections::HashMap::new(),
            }
        });

        Ok(Self {
            status: AttemptStatus::from(item.response.data.status.clone()),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.data.reference),
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
// Error response
// ---------------------------------------------------------------------
//
// Korapay's error shape (per every `providerError(responseData.message ||
// '...')` call site in legacy-node/providers/korapay.js) is a flat
// `{ status: false, message: "..." }` — no separate machine-readable error
// code field observed anywhere in the legacy integration, so `code` is
// left unset (`build_error_response` in mod.rs falls back to
// `consts::NO_ERROR_CODE`, matching connectors like Opennode that are in
// the same position).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KorapayErrorResponse {
    pub status: bool,
    pub message: String,
}

// ---------------------------------------------------------------------
// Payout Fulfill — POST /api/v1/transactions/disburse
// Payout Sync    — GET  /api/v1/transactions/{reference}
// ---------------------------------------------------------------------
//
// Task 77/a-1-iii. Request shape ported directly from this repo's own
// legacy-node/providers/korapay.js#processPayout() -- itself a Task 42
// Part B-a fix, independently confirmed at the time against
// developers.korapay.com/docs/payout-via-api AND a community Elixir
// client library's own published type spec (two independent sources
// agreeing). `destination.type` is always sent explicitly rather than
// relying on Korapay's own "defaults to bank_account if omitted" note,
// same reasoning the legacy code already applied.
//
// Response shape shared by both flows -- Korapay's own GET
// `.../transactions/{reference}` (used here for Sync) returns the same
// two-level `{ status, data: { status, ... } }` shape as the POST
// disburse call (used for Fulfill); legacy-node's own
// processPayout()/verifyPayout() parse it identically. See Task 42's
// "The 'b' this split implies" and "the missing verification call"
// entries in handover.md for how that two-level shape was confirmed.
#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct KorapayPayoutBankAccount {
    pub bank: String,
    pub account: String,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct KorapayPayoutCustomer {
    pub email: Email,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Secret<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<Secret<String>>,
}

// Korapay's real schema supports `mobile_money` as a second destination
// type alongside `bank_account` (see legacy-node's own
// `payment_method === 'mobile_money'` branch). Deliberately NOT
// buildable here: Hyperswitch's own `PayoutMethodData` enum
// (api_models::payouts) has no wallet/mobile-money variant that could
// carry a mobile-money destination at all (its `Wallet` variant is
// ApplePay/GooglePay/Paypal/Venmo only) -- so this connector can only
// ever construct `BankAccount`. Not a guess to leave `MobileMoney`
// unbuilt; there is currently no input shape to build it from.
#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KorapayPayoutDestinationType {
    BankAccount,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct KorapayPayoutDestination {
    #[serde(rename = "type")]
    pub destination_type: KorapayPayoutDestinationType,
    pub amount: FloatMajorUnit,
    pub currency: enums::Currency,
    pub narration: String,
    pub bank_account: KorapayPayoutBankAccount,
    pub customer: KorapayPayoutCustomer,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct KorapayPayoutFulfillRequest {
    pub reference: String,
    pub destination: KorapayPayoutDestination,
}

// ⚠️ Real, unresolved shape gap -- flagged, not guessed around.
// Korapay's real payout `destination.bank_account` is `{ bank, account
// }`: `bank` is a Korapay-specific bank *code* from Korapay's own
// `/api/v1/banks` list, `account` is a Nigerian NUBAN account number.
// Hyperswitch's `PayoutMethodData` (api_models::payouts) has no variant
// shaped like that -- its `BankTransfer` variants are
// Ach/Bacs/Sepa/Pix(+Key/Emv)/Trustly/OpenBanking, all built around
// IBAN/BIC/US-routing-number/UK-sort-code conventions, none of which is
// "a provider-specific bank code". `BankTransfer::Ach` is the closest
// structural fit purely because it is the one variant with two plain
// (non-IBAN, non-BIC-formatted) string fields -- `bank_account_number`
// and `bank_routing_number` -- so this function reuses
// `bank_routing_number` to carry Korapay's bank code and
// `bank_account_number` to carry the account number. This is a real,
// flagged stopgap, not a confirmed-correct mapping: `bank_routing_number`
// is documented elsewhere in this same enum as a US ABA routing number,
// a different real-world value with a different format, and nothing in
// this session confirmed Korapay's API tolerates whatever a caller
// happens to put in that field. Do not trust this in production before
// either (a) a real Korapay sandbox call confirms this passes through
// correctly, or (b) a proper NUBAN-shaped `PayoutMethodData` variant is
// added upstream and this is switched to it. Every other `BankTransfer`
// variant, and every non-`BankTransfer` variant (including the
// mobile-money gap noted above), is rejected with `NotSupported` rather
// than guessed at.
#[cfg(feature = "payouts")]
fn get_korapay_payout_bank_account<F>(
    router_data: &PayoutsRouterData<F>,
) -> Result<KorapayPayoutBankAccount, error_stack::Report<errors::ConnectorError>> {
    match router_data.get_payout_method_data()? {
        PayoutMethodData::BankTransfer(BankTransfer::Ach(ach)) => Ok(KorapayPayoutBankAccount {
            bank: ach.bank_routing_number.expose(),
            account: ach.bank_account_number.expose(),
        }),
        other => Err(errors::ConnectorError::NotSupported {
            message: format!(
                "{other:?} via Korapay payouts (see korapay/transformers.rs's own \
                 get_korapay_payout_bank_account note on the real NUBAN/bank-code shape gap)"
            ),
            connector: "korapay",
        }
        .into()),
    }
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<&KorapayRouterData<&PayoutsRouterData<F>>> for KorapayPayoutFulfillRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &KorapayRouterData<&PayoutsRouterData<F>>) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        let bank_account = get_korapay_payout_bank_account(router_data)?;

        // `customer.email` is REQUIRED by Korapay's real schema -- same
        // fact Task 42 Part B-a's fix already established for the
        // legacy JS integration (`if (!data.customer?.email) throw
        // providerError(...)` in processPayout()). Failing loudly here,
        // before a request is built, rather than letting Korapay reject
        // an incomplete request with a less specific error.
        let email = router_data
            .request
            .customer_details
            .as_ref()
            .and_then(|customer| customer.email.clone())
            .ok_or(errors::ConnectorError::MissingRequiredField {
                field_name: "customer_details.email".into(),
            })?;
        let name = router_data
            .request
            .customer_details
            .as_ref()
            .and_then(|customer| customer.name.clone());
        let phone = router_data
            .request
            .customer_details
            .as_ref()
            .and_then(|customer| customer.phone.clone());

        Ok(Self {
            reference: router_data.connector_request_reference_id.clone(),
            destination: KorapayPayoutDestination {
                destination_type: KorapayPayoutDestinationType::BankAccount,
                amount: item.amount,
                currency: router_data.request.destination_currency,
                // Hyperswitch's `PayoutsData` carries no narration/
                // description field at all (unlike the legacy JS
                // request, which took a caller-supplied `narration` or
                // fell back to a Mavins-specific default) -- a generic,
                // connector-level default is used here instead of
                // guessing at a field this request type doesn't have.
                narration: "Payout via Korapay".to_string(),
                bank_account,
                customer: KorapayPayoutCustomer {
                    email,
                    name,
                    phone,
                },
            },
        })
    }
}

// Real lifecycle state of the transaction itself -- distinct from the
// outer `status: bool` on `KorapayPayoutResponse`, which only ever
// means "did Kora accept/find this API call" (see
// KorapayPaymentsResponse's own note above, and Task 42's "the 'b' this
// split implies" entry in handover.md, which is where this two-level
// shape was first confirmed for the payout side specifically).
// "processing" is Kora's own normal, expected, asynchronous
// acknowledgement state per their own docs -- not an error.
#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum KorapayPayoutTransactionStatus {
    Success,
    Failed,
    #[default]
    Processing,
    Pending,
    #[serde(other)]
    Unknown,
}

#[cfg(feature = "payouts")]
impl From<KorapayPayoutTransactionStatus> for PayoutStatus {
    fn from(status: KorapayPayoutTransactionStatus) -> Self {
        match status {
            KorapayPayoutTransactionStatus::Success => Self::Success,
            KorapayPayoutTransactionStatus::Failed => Self::Failed,
            // Deliberately NOT mirrored on the legacy JS's own behavior
            // here: processPayout() in legacy-node/providers/korapay.js
            // treats "processing" as "request accepted, no error
            // thrown" because JS has no separate typed non-terminal
            // payout state to put it in. Hyperswitch's `PayoutStatus`
            // does have one (`Pending`), which is the more correct,
            // idiomatic representation for an asynchronous
            // acknowledgement -- so both `Processing` and `Pending`
            // map to `PayoutStatus::Pending` rather than being folded
            // into a success/no-op the way the JS code's control flow
            // effectively did.
            KorapayPayoutTransactionStatus::Processing
            | KorapayPayoutTransactionStatus::Pending
            | KorapayPayoutTransactionStatus::Unknown => Self::Pending,
        }
    }
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KorapayPayoutData {
    pub reference: String,
    pub status: KorapayPayoutTransactionStatus,
    // Per legacy-node's own `responseData.data?.message ||
    // responseData.message` fallback chain in both processPayout() and
    // the error path generally -- not confirmed against a live Korapay
    // response this session (no working rustc; see New-Clone
    // Checklist), so `#[serde(default)]` keeps this optional rather
    // than assuming the field is always present.
    #[serde(default)]
    pub message: Option<String>,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KorapayPayoutResponse {
    pub status: bool,
    pub message: String,
    pub data: KorapayPayoutData,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<PayoutsResponseRouterData<F, KorapayPayoutResponse>> for PayoutsRouterData<F> {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: PayoutsResponseRouterData<F, KorapayPayoutResponse>,
    ) -> Result<Self, Self::Error> {
        // Outer `status: false` is Korapay's own signal that the API
        // call itself was rejected (bad reference, auth failure, an
        // immediately-invalid request, etc.) -- a 2xx with `status:
        // false` is treated as a real, thrown failure here, same
        // discipline as KorapayPaymentsResponse's own handling above
        // and every method in legacy-node/providers/korapay.js
        // (`if (!response.ok || !responseData.status) throw ...`).
        // This is deliberately different from a `data.status: "failed"`
        // outcome below, which is a normal, successfully-verified
        // terminal payout state, not an error calling this function.
        if !item.response.status {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }

        let payout_status = PayoutStatus::from(item.response.data.status.clone());
        let error_message = payout_status.is_payout_failure().then(|| {
            item.response
                .data
                .message
                .clone()
                .unwrap_or_else(|| item.response.message.clone())
        });

        Ok(Self {
            response: Ok(PayoutsResponseData {
                status: Some(payout_status),
                connector_payout_id: Some(item.response.data.reference.clone()),
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
