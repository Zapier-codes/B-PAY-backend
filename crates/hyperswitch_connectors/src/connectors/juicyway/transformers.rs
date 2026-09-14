use api_models::payments::OrderDetailsWithAmount;
use common_enums::{enums, AttemptStatus};
use common_utils::{pii::Email, types::MinorUnit};
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_request_types::ResponseId,
    router_response_types::PaymentsResponseData,
    types::PaymentsAuthorizeRouterData,
};
use hyperswitch_interfaces::errors;
use hyperswitch_masking::Secret;
use serde::{Deserialize, Serialize};

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
                field_name: "billing.first_name",
            },
        )?;
        let last_name = router_data.get_optional_billing_last_name().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.last_name",
            },
        )?;
        let phone_number = router_data.get_optional_billing_phone_number().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.phone_number",
            },
        )?;

        let billing_address =
            router_data
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
// Response — shared by Authorize and PSync
// ---------------------------------------------------------------------
//
// Shape confirmed against docs.juicyway.com (handover.md's "FULL API
// discovery pass"): a successful `POST /payment-sessions` call returns
// `{ data: { status, auth_type, expires_at, links, message, payment: {
// id, amount, currency, status, customer, order, payment_method,
// reference, date, description, mode, cancellation_reason } } }`.
//
// ⚠️ The exact string values `payment.status` takes on are NOT quoted
// verbatim anywhere in this session's sources — only the field's
// existence and nesting are confirmed. The variants below are a
// reasonable, common-pattern guess (`#[serde(other)]` catches anything
// unrecognized as `Unknown` rather than failing deserialization), NOT a
// docs-confirmed enumeration. Flagged for a live sandbox call before
// this status mapping is trusted with real money.
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
    Successful,
    Failed,
    #[default]
    Pending,
    Processing,
    #[serde(other)]
    Unknown,
}

impl From<JuicywayPaymentStatus> for AttemptStatus {
    fn from(status: JuicywayPaymentStatus) -> Self {
        match status {
            JuicywayPaymentStatus::Successful => Self::Charged,
            JuicywayPaymentStatus::Failed => Self::Failure,
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
