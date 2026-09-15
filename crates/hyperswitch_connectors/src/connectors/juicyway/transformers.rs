use api_models::payments::OrderDetailsWithAmount;
#[cfg(feature = "payouts")]
use api_models::payouts::{BankTransfer, PayoutMethodData};
#[cfg(feature = "payouts")]
use common_enums::PayoutStatus;
use common_enums::{enums, AttemptStatus};
use common_utils::{pii::Email, types::MinorUnit};
#[cfg(feature = "payouts")]
use hyperswitch_domain_models::types::{PayoutsResponseData, PayoutsRouterData};
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_request_types::ResponseId,
    router_response_types::PaymentsResponseData,
    types::PaymentsAuthorizeRouterData,
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

// JuicyWay's `/payment-sessions` endpoint takes amount in minor units
// (subunit, ×100) across every supported currency, stablecoins included --
// confirmed against docs.juicyway.com/payments/initialize-payment.md's own
// "Universal Parameters" section (handover.md Task 49/a). MinorUnit is the
// matching hyperswitch amount type -- same choice Paystack's own connector
// in this crate already made for the same reason.
pub struct JuicywayRouterData<T> {
    pub amount: MinorUnit,
    pub router_data: T,
}

impl<T> From<(MinorUnit, T)> for JuicywayRouterData<T> {
    fn from((amount, router_data): (MinorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// Auth Struct
// JuicyWay authenticates REST calls with a single secret key, sent as the
// RAW key value in the Authorization header -- NO "Bearer " prefix.
// Confirmed against docs.juicyway.com/authentication.md directly
// (handover.md Task 45a / the "FULL API discovery pass"): every one of
// legacy-node/providers/juicyway.js's REST calls that still sent
// `Bearer ${apiKey}` was a confirmed bug, fixed in the legacy file by
// dropping the prefix. This connector starts from the corrected shape.
// (The webhook checksum uses a SEPARATE credential -- the merchant's
// "business ID", not this key -- and is out of scope for this leaf; see
// this crate's `juicyway.rs` IncomingWebhook note.)
pub struct JuicywayAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for JuicywayAuthType {
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
// Authorize (collection) — POST /payment-sessions
// ---------------------------------------------------------------------
//
// Request shape confirmed against docs.juicyway.com/payments/
// initialize-payment.md (handover.md's "JuicyWay — FULL API discovery
// pass", Task 8b/45b): the real required body is a deeply-nested object,
// not the flat `{ amount, email, reference, currency }` the legacy JS
// integration sent (a confirmed, real bug — Task 45b). Every field below
// is read from RouterData where a matching field exists; anything the
// docs mark required that RouterData has no generic equivalent for is
// filled with an explicitly-flagged default rather than left out (which
// would just trade "flat and wrong" for "nested and still wrong").
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JuicywayCustomerType {
    Individual,
    Business,
}

#[derive(Debug, Serialize)]
pub struct JuicywayBillingAddress {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line1: Option<Secret<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Secret<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip_code: Option<Secret<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<enums::CountryAlpha2>,
}

#[derive(Debug, Serialize)]
pub struct JuicywayCustomer {
    pub email: Email,
    pub first_name: Secret<String>,
    pub last_name: Secret<String>,
    pub phone_number: Secret<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_address: Option<JuicywayBillingAddress>,
    // ⚠️ Flagged, not confirmed: no field on `PaymentsAuthorizeData`
    // generically distinguishes a business from an individual payer.
    // Defaulting to `Individual` (the common case for card payments) —
    // verify against a live JuicyWay sandbox call before trusting this
    // for a genuinely business-type payer.
    #[serde(rename = "type")]
    pub customer_type: JuicywayCustomerType,
    // ⚠️ Not sent: no confirmed source field on generic RouterData for
    // the payer's IP address outside 3DS `browser_info`, and the docs
    // don't say what happens if this required field is omitted for a
    // non-3DS flow. Left unset here (JuicyWay will reject if it's truly
    // always required) rather than guessed at from an unrelated field —
    // flagged for a live sandbox check, same discipline as the
    // customer_type default above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<Secret<String, common_utils::pii::IpAddress>>,
}

#[derive(Debug, Serialize)]
pub struct JuicywayPaymentMethod {
    #[serde(rename = "type")]
    pub method_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JuicywayOrderItemType {
    Physical,
    Digital,
}

#[derive(Debug, Serialize)]
pub struct JuicywayOrderItem {
    pub name: String,
    // ⚠️ Flagged, not confirmed: `OrderDetailsWithAmount` has no
    // physical-vs-digital field. Defaulting every item to `Physical` —
    // verify against JuicyWay support/docs before relying on this for a
    // digital-goods merchant.
    #[serde(rename = "type")]
    pub item_type: JuicywayOrderItemType,
}

#[derive(Debug, Serialize)]
pub struct JuicywayOrder {
    pub identifier: String,
    pub items: Vec<JuicywayOrderItem>,
}

#[derive(Debug, Serialize)]
pub struct JuicywayPaymentsRequest {
    pub amount: MinorUnit,
    pub currency: enums::Currency,
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub payment_method: JuicywayPaymentMethod,
    pub order: JuicywayOrder,
    pub customer: JuicywayCustomer,
}

impl TryFrom<&JuicywayRouterData<&PaymentsAuthorizeRouterData>> for JuicywayPaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &JuicywayRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        // JuicyWay's `/payment-sessions` is a single hosted-checkout
        // endpoint (per legacy-node/providers/juicyway.js, the only
        // collection call this repo's legacy integration ever made) —
        // same "one endpoint takes whatever payment-method-data arrives"
        // shape as Korapay's own connector in this crate.
        match item.router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented("payment method via JuicyWay".to_string()),
            )),
        }?;

        let router_data = item.router_data;
        let email = router_data.request.get_email()?;
        let first_name = router_data.get_optional_billing_first_name().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.first_name".into(),
            },
        )?;
        let last_name = router_data.get_optional_billing_last_name().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.last_name".into(),
            },
        )?;
        let phone_number = router_data.get_optional_billing_phone_number().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.phone_number".into(),
            },
        )?;

        let billing_address = router_data
            .get_optional_billing()
            .map(|_| JuicywayBillingAddress {
                line1: router_data.get_optional_billing_line1(),
                city: router_data.get_optional_billing_city(),
                state: router_data.get_optional_billing_state(),
                zip_code: router_data.get_optional_billing_zip(),
                country: router_data.get_optional_billing_country(),
            });

        let items = match router_data.request.order_details.as_ref() {
            Some(order_details) if !order_details.is_empty() => order_details
                .iter()
                .map(|detail| JuicywayOrderItem {
                    name: detail.product_name.clone(),
                    item_type: JuicywayOrderItemType::Physical,
                })
                .collect(),
            // JuicyWay marks `order` (with at least one item) required
            // with no documented "omit if unknown" behavior. Rather than
            // fail every Authorize call that doesn't carry
            // `order_details` (most won't), a single generic fallback
            // item is sent — flagged, not a confirmed-correct substitute
            // for real order data.
            _ => vec![JuicywayOrderItem {
                name: "Payment".to_string(),
                item_type: JuicywayOrderItemType::Physical,
            }],
        };

        Ok(Self {
            amount: item.amount,
            currency: router_data.request.currency,
            reference: router_data.connector_request_reference_id.clone(),
            description: router_data.description.clone(),
            payment_method: JuicywayPaymentMethod {
                method_type: "card".to_string(),
            },
            order: JuicywayOrder {
                identifier: router_data.connector_request_reference_id.clone(),
                items,
            },
            customer: JuicywayCustomer {
                email,
                first_name,
                last_name,
                phone_number,
                billing_address,
                customer_type: JuicywayCustomerType::Individual,
                ip_address: None,
            },
        })
    }
}

// ---------------------------------------------------------------------
// Authorize response — POST /payment-sessions
// ---------------------------------------------------------------------
//
// Shape confirmed against docs.juicyway.com (handover.md's "FULL API
// discovery pass"): a successful `POST /payment-sessions` call returns
// `{ data: { status, auth_type, expires_at, links, message, payment: {
// id, amount, currency, status, customer, order, payment_method,
// reference, date, description, mode, cancellation_reason } } }`.
//
// No longer shared with PSync (see `JuicywayFetchPaymentResponse`
// below) — `GET /payments/{id}`'s real response has no `payment`
// sub-object at all, confirmed this session; reusing this struct for
// both was a real bug, not just an unconfirmed assumption. This
// nested-`payment` envelope itself remains specific to
// `/payment-sessions` and still has NOT been independently confirmed
// with its own worked example this session.
//
// ⚠️ The exact string values `payment.status` takes on for THIS
// endpoint (`/payment-sessions`, Authorize) are still NOT independently
// confirmed — no worked example for that specific endpoint was fetched
// this session either. What changed this session: `GET /payments/{id}`
// (the Fetch Payment endpoint, confirmed below for PSync) documents the
// same underlying payment resource's `status` field with a full worked
// example — `pending`, `processing`, `succeeded`, `failed`, `cancelled`.
// Since both endpoints describe the identical payment object's
// lifecycle state, this enum now uses that confirmed vocabulary
// (`succeeded`, not the previous unconfirmed `successful` guess; plus
// the previously-missing `cancelled`) on the reasonable assumption the
// two endpoints share one status vocabulary — flagged as an assumption,
// not re-guessed from nothing. `#[serde(other)]` still folds anything
// unrecognized to `Unknown` rather than failing deserialization.
//
// `links` (documented, presumably a redirect/checkout URL for
// non-instant payment methods) is deliberately NOT typed or used for
// `redirection_data` here — its own shape isn't confirmed by anything
// fetched this session, and guessing at it risks silently building a
// broken redirect rather than none at all. Real gap, flagged, not
// papered over — a future pass should fetch JuicyWay's own worked
// example for a redirect-based payment method to confirm this shape
// before wiring redirection.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JuicywayPaymentStatus {
    Succeeded,
    Failed,
    Cancelled,
    #[default]
    Pending,
    Processing,
    #[serde(other)]
    Unknown,
}

impl From<JuicywayPaymentStatus> for AttemptStatus {
    fn from(status: JuicywayPaymentStatus) -> Self {
        match status {
            JuicywayPaymentStatus::Succeeded => Self::Charged,
            JuicywayPaymentStatus::Failed => Self::Failure,
            // Confirmed via docs.juicyway.com/payment-transactions/fetch-payment
            // this session -- `cancelled` is a real, distinct terminal
            // state, not a synonym for `failed`. Mapped to `Voided`
            // rather than `Failure` so downstream reconciliation can
            // tell "the merchant/customer called it off" apart from
            // "the provider declined it" -- previously this state
            // wasn't representable at all and would have fallen into
            // `Unknown` -> `Pending`, which is the more dangerous
            // failure mode (a dead payment reported as still in flight).
            JuicywayPaymentStatus::Cancelled => Self::Voided,
            JuicywayPaymentStatus::Pending | JuicywayPaymentStatus::Processing => {
                Self::AuthenticationPending
            }
            JuicywayPaymentStatus::Unknown => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayPaymentObject {
    // JuicyWay's own UUID for this payment — separate from, and
    // different in shape from, the merchant-supplied `reference`. This
    // is the id `GET /payments/{id}` (used for PSync) actually takes —
    // see this module's own PSync note in `juicyway.rs`. Confirmed at
    // handover.md's "reference-vs-ID distinction" finding (Task 45d).
    pub id: String,
    pub status: JuicywayPaymentStatus,
    pub reference: String,
}

// CONFIRMED against docs.juicyway.com/payments/initialize-payment/cards
// (Task 77/a-3, this session) -- the worked "201 Success - Payment
// Session Created" response for `POST /payment-sessions` nests the
// payment object under `data.payment`, alongside a top-level
// `data.status`, exactly as coded below. Previously this shape was
// inferred-for-consistency with PSync's response, not verified against
// its own primary source; PSync has since been confirmed FLAT instead
// (see `JuicywayFetchPaymentData` below), so this struct staying
// nested was a real fact to check, not an assumption to carry over.
// No code change from this confirmation -- only fields this connector
// actually reads (`id`/`status`/`reference` on the nested `payment`
// object) are modeled; the real payload also carries `auth_type`,
// `expires_at`, `links`, `message`, `amount`, `currency`, `customer`,
// `date`, `description`, `order`, `mode`, `payment_method`, none of
// which anything here currently consumes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayPaymentSessionData {
    pub status: JuicywayPaymentStatus,
    pub payment: JuicywayPaymentObject,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayPaymentsResponse {
    pub data: JuicywayPaymentSessionData,
}

impl<F, T> TryFrom<ResponseRouterData<F, JuicywayPaymentsResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, JuicywayPaymentsResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        // Unlike Korapay/Paystack, JuicyWay's documented success envelope
        // carries no top-level `status: bool` "was this call accepted"
        // flag distinct from the payment's own lifecycle status — a 2xx
        // HTTP response IS the "call accepted" signal here;
        // `build_error_response` (juicyway.rs) already owns the non-2xx
        // path. `data.status` / `data.payment.status` are the payment's
        // real lifecycle state, mapped below, not an accept/reject flag.
        let status = AttemptStatus::from(item.response.data.payment.status.clone());

        Ok(Self {
            status,
            // JuicyWay's own `payment.id` (a UUID, distinct from the
            // merchant `reference`) is stored as the connector
            // transaction id — this is what makes PSync's `GET
            // /payments/{id}` call correct by construction, closing the
            // reference-vs-ID gap flagged in handover.md Task 45d
            // (legacy-node/providers/juicyway.js has no such storage and
            // still calls the wrong id-shaped endpoint with a reference).
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.data.payment.id),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: Some(item.response.data.payment.reference),
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
// PSync response — GET /payments/{id} (Fetch Payment)
// ---------------------------------------------------------------------
//
// CONFIRMED this session against
// docs.juicyway.com/payment-transactions/fetch-payment — the exact
// endpoint PSync calls (see juicyway.rs's own PSync `get_url`), with a
// full worked response example. This is a REAL BUG FIX, not just a
// confirmation: the shape below is FLAT (`data: { id, status,
// reference, ... }`), not nested under a `payment` sub-object the way
// `JuicywayPaymentSessionData` above is. PSync previously reused that
// nested-`payment` struct (this file's own former "shared by Authorize
// and PSync" comment) — since `payment` is a required field with no
// `#[serde(default)]`, every real `GET /payments/{id}` response would
// have failed deserialization outright with
// `ResponseDeserializationFailed`, since the real response has no
// `payment` key at all. PSync now has its own correctly-shaped struct
// instead of sharing Authorize's still-unconfirmed one.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct JuicywayFetchPaymentData {
    pub id: String,
    pub status: JuicywayPaymentStatus,
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct JuicywayFetchPaymentResponse {
    pub data: JuicywayFetchPaymentData,
}

impl<F, T> TryFrom<ResponseRouterData<F, JuicywayFetchPaymentResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, JuicywayFetchPaymentResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let status = AttemptStatus::from(item.response.data.status.clone());

        Ok(Self {
            status,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.data.id),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: item.response.data.reference,
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
// Error response — confirmed at docs.juicyway.com/errors.md
// ---------------------------------------------------------------------
//
// Every JuicyWay error is `{ error: { code, message, type, details } }`
// (validation errors additionally carry a top-level `errors: [{ field,
// message }]` array). The end-user-facing message lives at
// `error.message`, NESTED — confirmed as a real, live bug in
// legacy-node/providers/juicyway.js (Task 45c): that file's own
// `responseData.message || 'Juicyway ... failed'` fallback fires on
// every single real error, since `responseData.message` is always
// undefined for JuicyWay's actual envelope. This connector reads the
// correct nested field directly rather than repeating that bug.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JuicywayErrorBody {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JuicywayValidationError {
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JuicywayErrorResponse {
    #[serde(default)]
    pub error: Option<JuicywayErrorBody>,
    #[serde(default)]
    pub errors: Option<Vec<JuicywayValidationError>>,
}

impl JuicywayErrorResponse {
    /// Picks the most specific, safe-to-surface message JuicyWay gave us:
    /// `error.message` first (the documented primary field), then the
    /// first validation error's own message, then a generic fallback.
    /// Never falls back to a top-level `message` field the way the
    /// legacy JS bug did — that field does not exist in JuicyWay's real
    /// envelope.
    pub fn get_message(&self) -> String {
        self.error
            .as_ref()
            .and_then(|error| error.message.clone())
            .or_else(|| {
                self.errors
                    .as_ref()
                    .and_then(|errors| errors.first())
                    .and_then(|first| first.message.clone())
            })
            .unwrap_or_else(|| "JuicyWay payment failed".to_string())
    }

    pub fn get_code(&self) -> Option<String> {
        self.error.as_ref().and_then(|error| error.code.clone())
    }
}

// ---------------------------------------------------------------------
// Payout Recipient — POST /beneficiaries
// Payout Fulfill   — POST /payouts
// Payout Sync      — GET  /payouts/{id}
// ---------------------------------------------------------------------
//
// Task 77/a-3-iii. Ported from this repo's own
// legacy-node/providers/juicyway.js#createBeneficiary()/processPayout()/
// verifyPayout() -- Task 52's already-confirmed beneficiary-first,
// pin-gated shape. Unlike Korapay (Task 77/a-1-iii), which takes a raw
// bank_code/account_number pair inline on a single disburse call,
// JuicyWay requires a beneficiary to exist first and returns an `id`
// that the actual payout references -- the same two-call shape
// Paystack's own connector (Task 77/a-2-iii) already models as its own
// `PayoutRecipient` flow, not a new pattern invented here.
//
// `PayoutsData.connector_payout_id` is reused, in sequence, to carry two
// DIFFERENT JuicyWay-side identifiers across the three flows below --
// same relay Paystack's own chain already uses: `PoRecipient`'s
// response writes the beneficiary `id` into it; `PoFulfill` reads that
// beneficiary id back out to build the `/payouts` call, then overwrites
// `connector_payout_id` again with the real payout `id` so `PoSync` can
// poll `/payouts/{id}` against it (JuicyWay's worked example has no
// `reference` field at all -- only `id` -- so, unlike Korapay/Paystack's
// PoSync, this cannot key off the merchant reference; see
// verifyPayout()'s own docblock).

// Request shape CONFIRMED this session (Task 77/a-3-iii follow-up,
// 2026-09-14) against the actual primary source --
// docs.juicyway.com/transfers/beneficiaries/create-beneficiary -- which
// neither this file's original a-3-iii pass nor the legacy JS
// (juicyway.js#createBeneficiary) had fetched; both had only reached
// the parent /transfers/beneficiaries overview page, which documents
// field *names* but not the request envelope. The confirmed "Create
// NGN Bank Account Beneficiary" shape is FLAT -- no `account_details`
// wrapper -- and requires two fields neither the legacy JS nor the
// original Rust struct sent at all: `bank_name` and `rail` (must be
// literal `"nuban"`). The previous nested-`account_details` shape was
// never a confirmed guess in the first place; it doesn't match any
// worked example on this page. Scoped to NGN deliberately: the
// confirmed page also documents a completely different "Create USD
// Bank Account Beneficiary" shape (routing_number/rail "ach"|"wire"/
// address/bank_address, no bank_code at all) that this connector does
// not attempt -- consistent with this file's own already-flagged
// GET /payment-methods/banks being Nigeria-only, so NGN is the only
// bank_code-driven path this connector can realistically drive anyway.
#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum JuicywayBeneficiaryRequest {
    BankAccount {
        currency: enums::Currency,
        account_name: Secret<String>,
        account_number: Secret<String>,
        bank_name: Secret<String>,
        bank_code: Secret<String>,
        rail: String,
    },
}

#[cfg(feature = "payouts")]
pub struct JuicywayCreateBeneficiaryRequest(pub JuicywayBeneficiaryRequest);

impl Serialize for JuicywayCreateBeneficiaryRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

// ⚠️ Real, unresolved shape gap that survives this session's fix --
// same root cause and same stopgap already flagged in
// korapay/transformers.rs's own `get_korapay_payout_bank_account` and
// paystack/transformers.rs's own `get_paystack_payout_bank_account`:
// Hyperswitch's `PayoutMethodData` (api_models::payouts) has no
// NUBAN/bank-code-shaped variant. `BankTransfer::Ach` is reused purely
// because it is the one variant with two plain (non-IBAN,
// non-BIC-formatted) string fields -- `bank_account_number` carries the
// account number, `bank_routing_number` carries JuicyWay's own bank
// code (from JuicyWay's own `GET /payment-methods/banks` list, NOT a US
// ABA routing number, which is what that field is documented elsewhere
// in this same enum as). `bank_name` is genuinely available on this
// same `AchBankTransfer` struct (confirmed by reading
// api_models::payouts -- it is `Option<String>`, not a stopgap), so
// unlike account_number/bank_code it is not a mapping guess, only an
// optionality gap: if the caller didn't populate it, this fails loudly
// rather than sending JuicyWay a beneficiary it will reject anyway. Not
// a confirmed-correct mapping end-to-end -- do not trust this in
// production before either a live JuicyWay sandbox call confirms it
// round-trips, or a proper NUBAN-shaped `PayoutMethodData` variant is
// added upstream and every connector using this same stopgap (Korapay,
// Paystack, now JuicyWay) is switched to it together.
// `crypto_address`/`interac` beneficiaries are real per
// legacy-node/providers/juicyway.js#createBeneficiary() but rejected
// with `NotSupported` here, same discipline as every other
// non-buildable variant -- there is currently no input shape to build
// them from.
#[cfg(feature = "payouts")]
fn get_juicyway_payout_bank_account<F>(
    router_data: &PayoutsRouterData<F>,
) -> Result<JuicywayBeneficiaryRequest, error_stack::Report<errors::ConnectorError>> {
    match router_data.get_payout_method_data()? {
        PayoutMethodData::BankTransfer(BankTransfer::Ach(ach)) => {
            let account_name = router_data
                .request
                .customer_details
                .as_ref()
                .and_then(|customer| customer.name.clone())
                .or_else(|| ach.account_holder_name.clone())
                .unwrap_or_else(|| ach.bank_account_number.clone());
            let bank_name =
                ach.bank_name
                    .clone()
                    .ok_or(errors::ConnectorError::MissingRequiredField {
                        field_name: "bank_name (required by Juicyway's confirmed \
                        Create-NGN-Bank-Account-Beneficiary shape; not optional \
                        despite AchBankTransfer.bank_name being Option<String>)"
                            .into(),
                    })?;
            Ok(JuicywayBeneficiaryRequest::BankAccount {
                currency: router_data.request.destination_currency,
                account_number: ach.bank_account_number,
                account_name,
                bank_name: Secret::new(bank_name),
                bank_code: ach.bank_routing_number,
                rail: "nuban".to_string(),
            })
        }
        other => Err(errors::ConnectorError::NotSupported {
            message: format!(
                "{other:?} via Juicyway payouts (see juicyway/transformers.rs's own \
                 get_juicyway_payout_bank_account note on the real NUBAN/bank-code shape gap)"
            ),
            connector: "juicyway",
        }
        .into()),
    }
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<&PayoutsRouterData<F>> for JuicywayCreateBeneficiaryRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(router_data: &PayoutsRouterData<F>) -> Result<Self, Self::Error> {
        Ok(Self(get_juicyway_payout_bank_account(router_data)?))
    }
}

// Response envelope CONFIRMED this session (2026-09-14) against
// docs.juicyway.com/transfers/beneficiaries/create-beneficiary's own
// "Success Response Example" -- the previous `data.id` shape below was
// an inference-for-consistency with processPayout()'s response, never
// independently checked for this endpoint specifically. That inference
// turned out correct: the confirmed worked example nests the
// beneficiary `id` (and every other returned field) under a top-level
// `data` object, exactly as already coded. No structural change needed
// here -- flagging only removed, not the code.
#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayBeneficiaryData {
    pub id: String,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayBeneficiaryResponse {
    pub data: JuicywayBeneficiaryData,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<PayoutsResponseRouterData<F, JuicywayBeneficiaryResponse>>
    for PayoutsRouterData<F>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: PayoutsResponseRouterData<F, JuicywayBeneficiaryResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(PayoutsResponseData {
                status: Some(PayoutStatus::RequiresFulfillment),
                connector_payout_id: Some(item.response.data.id.clone()),
                payout_eligible: None,
                should_add_next_step_to_process_tracker: false,
                error_code: None,
                error_message: None,
                payout_connector_metadata: None,
                connector_eligibility_reference_id: None,
            }),
            ..item.data
        })
    }
}

// JuicyWay-specific per-payout secret that has no matching field
// anywhere on Hyperswitch's `PayoutsData` -- confirmed required by both
// worked examples on
// docs.juicyway.com/transfers/transfers/initiate-bank-transfer.md
// despite the page's own <ParamField> markup not clearly marking it
// required (Task 52/a-1's own reasoning, ported as-is). Read from
// `payout_connector_metadata`, the sanctioned generic per-request
// connector-metadata field every other payout-metadata-carrying
// connector in this crate (Korapay, Stripe Connect, Wise, Adyen
// Platform, etc.) already uses for exactly this kind of provider-
// specific extra that the shared request shape has no field for --
// deliberately NOT read from an env var the way the legacy JS's own
// `JUICYWAY_PAYOUT_PIN` fallback does, since a transfer PIN is
// merchant-transaction-specific, per-call data, not a process-wide
// default a stateless connector integration should assume.
#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Deserialize)]
pub struct JuicywayPayoutConnectorMetadata {
    pub pin: Secret<String>,
}

#[cfg(feature = "payouts")]
fn get_juicyway_payout_pin<F>(
    router_data: &PayoutsRouterData<F>,
) -> Result<Secret<String>, error_stack::Report<errors::ConnectorError>> {
    router_data
        .request
        .payout_connector_metadata
        .as_ref()
        .and_then(|metadata| {
            serde_json::from_value::<JuicywayPayoutConnectorMetadata>(metadata.clone().expose())
                .ok()
        })
        .map(|metadata| metadata.pin)
        .ok_or_else(|| {
            errors::ConnectorError::MissingRequiredField {
                field_name: "payout_connector_metadata.pin (Juicyway transfer PIN -- see \
                             juicyway/transformers.rs's own JuicywayPayoutConnectorMetadata note)"
                    .into(),
            }
            .into()
        })
}

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct JuicywayPayoutBeneficiaryRef {
    pub id: String,
    #[serde(rename = "type")]
    pub beneficiary_type: String,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct JuicywayPayoutFulfillRequest {
    pub amount: MinorUnit,
    pub beneficiary: JuicywayPayoutBeneficiaryRef,
    pub description: String,
    pub destination_currency: enums::Currency,
    pub pin: Secret<String>,
    pub reference: String,
    pub source_currency: enums::Currency,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<&JuicywayRouterData<&PayoutsRouterData<F>>> for JuicywayPayoutFulfillRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &JuicywayRouterData<&PayoutsRouterData<F>>) -> Result<Self, Self::Error> {
        let router_data = item.router_data;

        // The beneficiary must already exist -- `PoRecipient` runs
        // before `PoFulfill` in the payout flow chain (see this
        // section's own file-level comment above) and its response
        // leaves the beneficiary `id` here, in `connector_payout_id`. A
        // missing value here means `PoRecipient` was skipped or failed,
        // which is a real orchestration precondition, not something
        // this request can recover from -- fail loudly rather than
        // send JuicyWay an empty beneficiary reference.
        let beneficiary_id = router_data.request.connector_payout_id.clone().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "connector_payout_id (Juicyway beneficiary id from PoRecipient)".into(),
            },
        )?;

        let pin = get_juicyway_payout_pin(router_data)?;

        Ok(Self {
            amount: item.amount,
            beneficiary: JuicywayPayoutBeneficiaryRef {
                id: beneficiary_id,
                // Only `bank_account` is ever buildable by this
                // connector's own `PoRecipient` leaf (see
                // `get_juicyway_payout_bank_account`'s own note above),
                // so this is never anything else in practice.
                beneficiary_type: "bank_account".to_string(),
            },
            // Hyperswitch's `PayoutsData` carries no narration/
            // description field at all (unlike the legacy JS request,
            // which took a caller-supplied `narration`/`description` or
            // fell back to a Mavins-specific default) -- a generic,
            // connector-level default is used here instead, same
            // choice Korapay's/Paystack's own connectors already made.
            description: "Payout via Juicyway".to_string(),
            destination_currency: router_data.request.destination_currency,
            pin,
            reference: router_data.connector_request_reference_id.clone(),
            source_currency: router_data.request.source_currency,
        })
    }
}

// Real lifecycle state of the payout itself. Per
// legacy-node/providers/juicyway.js#processPayout()'s own docblock:
// "status is documented as 'pending' in both worked examples; no
// documented synchronous 'failed' outcome" -- so, unlike Korapay's/
// Paystack's payout status enums, `Success`/`Failed` here are
// speculative extension points for whatever `PoSync` may eventually
// observe, not independently confirmed strings from JuicyWay's own
// docs. `#[serde(other)]` folds anything unrecognized (including a
// first real sandbox response) to `Pending` rather than failing
// deserialization outright.
#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JuicywayPayoutStatus {
    Success,
    Failed,
    #[default]
    Pending,
    #[serde(other)]
    Unknown,
}

#[cfg(feature = "payouts")]
impl From<JuicywayPayoutStatus> for PayoutStatus {
    fn from(status: JuicywayPayoutStatus) -> Self {
        match status {
            JuicywayPayoutStatus::Success => Self::Success,
            JuicywayPayoutStatus::Failed => Self::Failed,
            // Per this enum's own docblock above: JuicyWay's confirmed
            // behavior never resolves synchronously to a terminal state
            // on the initiate call -- final outcome presumably arrives
            // via webhook, same caveat legacy-node's own processPayout()
            // logs at call time. Callers must not treat a `Pending`
            // result here as final, same discipline Korapay's/
            // Paystack's own Pending mappings already require.
            JuicywayPayoutStatus::Pending | JuicywayPayoutStatus::Unknown => Self::Pending,
        }
    }
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayPayoutData {
    pub id: String,
    #[serde(default)]
    pub status: JuicywayPayoutStatus,
    #[serde(default)]
    pub reason: Option<String>,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JuicywayPayoutResponse {
    #[serde(default)]
    pub message: Option<String>,
    pub data: JuicywayPayoutData,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<PayoutsResponseRouterData<F, JuicywayPayoutResponse>> for PayoutsRouterData<F> {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: PayoutsResponseRouterData<F, JuicywayPayoutResponse>,
    ) -> Result<Self, Self::Error> {
        let payout_status = PayoutStatus::from(item.response.data.status.clone());
        let error_message = payout_status
            .is_payout_failure()
            .then(|| {
                item.response
                    .data
                    .reason
                    .clone()
                    .or_else(|| item.response.message.clone())
            })
            .flatten();

        Ok(Self {
            response: Ok(PayoutsResponseData {
                status: Some(payout_status),
                // `PoFulfill`'s own response overwrites
                // `connector_payout_id` again here -- from the
                // beneficiary id it was reading in, to JuicyWay's own
                // payout `id` -- so `PoSync` (above, in juicyway.rs) has
                // the right identifier to poll `/payouts/{id}` against.
                // Per this section's own file-level comment: JuicyWay's
                // worked example has no `reference` field at all, only
                // `id`, matching legacy-node's own verifyPayout()
                // docblock exactly.
                connector_payout_id: Some(item.response.data.id.clone()),
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
