use common_enums::{enums, AttemptStatus};
use common_utils::{pii::Email, types::FloatMajorUnit};
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_request_types::ResponseId,
    router_response_types::{PaymentsResponseData, RedirectForm},
    types::PaymentsAuthorizeRouterData,
};
use hyperswitch_interfaces::errors;
use hyperswitch_masking::Secret;
use serde::{Deserialize, Serialize};

use crate::{
    types::ResponseRouterData,
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
