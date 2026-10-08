// Prestmit transformers.
//
// Prestmit is a gift-card / crypto off-ramp, NOT a bank/card processor — it
// exposes no "charge a customer" primitive. Confirmed against its own docs
// (documentation.prestmit.io, 2026-10). Its only user-initiated collection
// flow is a gift-card SELL trade: the user hands the platform a gift card and
// Prestmit pays the user out via a `payoutMethod` (default NAIRA; also
// CEDIS/crypto). That is what maps onto Hyperswitch Authorize here.
//
//   POST /partners/v1/giftcard-trade/sell/create   (multipart/form-data)
//     { giftcard_id, amount, payoutMethod, payoutAddress?, comments?,
//       promoCode?, uniqueIdentifier?, attachments[]? }
//     -> { success: true, details, trade: { reference, rate, units,
//          totalAmount, status, comments, rejectionReason, createdAt,
//          category{...}, giftcard{...} } }
//
//   GET  /partners/v1/giftcard-trade/sell/history?referenceOrID={ref}
//     -> { data: [ { reference, rate, units, totalAmount, status, ... } ],
//          links, meta }
//
// Auth is NOT a plain bearer/API key: every request carries an `API-KEY`
// header AND an `API-Hash` header = HMAC-SHA256(`{API_KEY}:{json_body}`,
// API_SECRET) hex. Because the hash signs the exact serialized body, it must
// be built at request time from the same bytes that are sent — see
// prestmit.rs's own `build_headers`, which signs `get_request_body`'s output
// (the same string `RequestContent::Json` sends). Attachments are stripped
// from the body before hashing per Prestmit's own rule; this connector sends
// no attachments (gift-card photos are out of scope for an automated
// payment), so the rule is moot here.
//
// Credentials are carried via `SignatureKey`: `api_key` = API_KEY,
// `api_secret` = API_SECRET, `key1` = the account PIN required on the
// balance-debiting create call.
//
// Amounts: Prestmit's sell `amount` is the gift-card's face value in its own
// currency (e.g. USD), and the trade total is quoted in the payout currency.
// Hyperswitch amounts are `MinorUnit` here (the sell payload's `amount` is an
// integer, and minor units are the only Hyperswitch amount type that
// round-trips an integer exactly without a major/minor scaling guess).
//
// Status vocabulary (`PENDING` / `REJECTED` / `COMPLETED`) confirmed on
// Prestmit's own tracking page. Webhook signature scheme
// (`x-prestmit-signature`, HMAC-SHA256 over the raw body, BASE64 — unlike the
// hex scheme every other provider in this crate uses) is confirmed but its
// payload parsing is deferred, matching Korapay/DodoPayments.

use common_enums::AttemptStatus;
use common_utils::types::MinorUnit;
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_flow_types::payments::PSync,
    router_request_types::{PaymentsSyncData, ResponseId},
    router_response_types::PaymentsResponseData,
    types::PaymentsAuthorizeRouterData,
};
use hyperswitch_interfaces::errors;
use hyperswitch_masking::Secret;
use serde::{Deserialize, Serialize};

use crate::types::ResponseRouterData;

pub struct PrestmitRouterData<T> {
    pub amount: MinorUnit,
    pub router_data: T,
}

impl<T> From<(MinorUnit, T)> for PrestmitRouterData<T> {
    fn from((amount, router_data): (MinorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// `api_key` -> API-KEY header, `api_secret` -> HMAC signing key,
// `key1` -> the account PIN that authorizes the balance-debiting trade.
pub struct PrestmitAuthType {
    pub(super) api_key: Secret<String>,
    pub(super) api_secret: Secret<String>,
    pub(super) account_pin: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for PrestmitAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::SignatureKey {
                api_key,
                key1,
                api_secret,
            } => Ok(Self {
                api_key: api_key.to_owned(),
                api_secret: api_secret.to_owned(),
                account_pin: key1.to_owned(),
            }),
            _ => Err(errors::ConnectorError::FailedToObtainAuthType.into()),
        }
    }
}

// Prestmit errors: a Laravel-style `{ message, errors: { field: [..] } }`
// envelope (confirmed on the buy-create page and reused across the API).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrestmitErrorResponse {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub errors: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------
// Authorize (collection) — POST /partners/v1/giftcard-trade/sell/create
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PrestmitSellTradeRequest {
    pub giftcard_id: i64,
    pub amount: MinorUnit,
    #[serde(rename = "payoutMethod")]
    pub payout_method: String,
    #[serde(rename = "payoutAddress", skip_serializing_if = "Option::is_none")]
    pub payout_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
    #[serde(rename = "promoCode", skip_serializing_if = "Option::is_none")]
    pub promo_code: Option<String>,
    #[serde(rename = "uniqueIdentifier", skip_serializing_if = "Option::is_none")]
    pub unique_identifier: Option<String>,
    #[serde(rename = "currentAccountPIN")]
    pub current_account_pin: Secret<String>,
}

impl TryFrom<&PrestmitRouterData<&PaymentsAuthorizeRouterData>> for PrestmitSellTradeRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &PrestmitRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        // A sell trade is initiated off a physical/eCode gift card, not a
        // card/wallet charge — nothing in `PaymentMethodData` describes the
        // gift card, so any variant is accepted and the Dodo-style spec is
        // carried in `metadata`.
        match router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented(
                    "payment method via Prestmit (gift-card sell only)".to_string(),
                ),
            )),
        }?;

        let metadata = router_data.request.metadata.as_ref();
        // `giftcard_id` identifies the sellable card variant (from Prestmit's
        // own subcategories lookup); there is no first-class Hyperswitch field
        // for it, so it is required in request metadata.
        let giftcard_id = metadata
            .and_then(|m| m.get("giftcard_id"))
            .and_then(|v| v.as_i64())
            .ok_or(errors::ConnectorError::MissingRequiredField {
                field_name: "giftcard_id (Prestmit sellable card id — pass via request metadata.giftcard_id)"
                    .into(),
            })?;

        let payout_method = metadata
            .and_then(|m| m.get("payoutMethod"))
            .and_then(|v| v.as_str())
            .unwrap_or("NAIRA")
            .to_string();
        let payout_address = metadata
            .and_then(|m| m.get("payoutAddress"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        // Crypto payout methods require a destination address.
        if payout_method != "NAIRA" && payout_method != "CEDIS" && payout_address.is_none() {
            return Err(errors::ConnectorError::MissingRequiredField {
                field_name: "payoutAddress (required for Prestmit crypto payout methods)".into(),
            }
            .into());
        }

        let comments = metadata
            .and_then(|m| m.get("comments"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let account_pin = PrestmitAuthType::try_from(&router_data.connector_auth_type)?.account_pin;

        Ok(Self {
            giftcard_id,
            amount: item.amount,
            payout_method,
            payout_address,
            comments,
            promo_code: None,
            unique_identifier: Some(router_data.connector_request_reference_id.clone()),
            current_account_pin: account_pin,
        })
    }
}

// ---------------------------------------------------------------------
// Authorize response — POST .../sell/create
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrestmitSellTrade {
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub rate: Option<serde_json::Value>,
    #[serde(default)]
    pub units: Option<serde_json::Value>,
    #[serde(default, rename = "totalAmount")]
    pub total_amount: Option<serde_json::Value>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub comments: Option<String>,
    #[serde(default, rename = "rejectionReason")]
    pub rejection_reason: Option<String>,
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrestmitSellTradeResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub trade: Option<PrestmitSellTrade>,
}

impl PrestmitSellTradeResponse {
    fn attempt_status(&self) -> AttemptStatus {
        match self.trade.as_ref().and_then(|t| t.status.as_deref()) {
            Some("COMPLETED") => AttemptStatus::Charged,
            Some("REJECTED") => AttemptStatus::Failure,
            // PENDING, plus a response with no trade object at all, stays
            // non-terminal rather than being read as a failure.
            _ => AttemptStatus::Pending,
        }
    }
}

impl<F, T> TryFrom<ResponseRouterData<F, PrestmitSellTradeResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, PrestmitSellTradeResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let reference = item
            .response
            .trade
            .as_ref()
            .and_then(|trade| trade.reference.clone());
        Ok(Self {
            status: item.response.attempt_status(),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(
                    reference
                        .clone()
                        .unwrap_or_else(|| item.data.connector_request_reference_id.clone()),
                ),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: reference,
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
// PSync — GET /partners/v1/giftcard-trade/sell/history?referenceOrID={ref}
// ---------------------------------------------------------------------
//
// Prestmit has no single-trade GET; a specific trade is fetched from the
// paginated history with the `referenceOrID` filter (its own tracking page).
// The stored `connector_transaction_id` is the trade `reference`, so PSync
// filters on it and reads the single returned trade's `status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrestmitSellHistoryResponse {
    #[serde(default)]
    pub data: Vec<PrestmitSellTrade>,
}

impl PrestmitSellHistoryResponse {
    fn attempt_status(&self) -> AttemptStatus {
        match self.data.first().and_then(|t| t.status.as_deref()) {
            Some("COMPLETED") => AttemptStatus::Charged,
            Some("REJECTED") => AttemptStatus::Failure,
            _ => AttemptStatus::Pending,
        }
    }
}

impl
    TryFrom<
        ResponseRouterData<
            PSync,
            PrestmitSellHistoryResponse,
            PaymentsSyncData,
            PaymentsResponseData,
        >,
    > for RouterData<PSync, PaymentsSyncData, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<
            PSync,
            PrestmitSellHistoryResponse,
            PaymentsSyncData,
            PaymentsResponseData,
        >,
    ) -> Result<Self, Self::Error> {
        let response = item.response;
        let reference = item
            .data
            .request
            .connector_transaction_id
            .get_connector_transaction_id()
            .unwrap_or_else(|_| item.data.connector_request_reference_id.clone());
        Ok(Self {
            status: response.attempt_status(),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(reference),
                redirection_data: Box::new(None),
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
