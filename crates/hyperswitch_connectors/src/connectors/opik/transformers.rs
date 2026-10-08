// telcos.opik.net (opik) transformers — VTU (airtime/data) rail.
//
// Unlike the hosted-checkout providers in this crate, opik is a direct
// VTU purchase API: set up a business account, fund the wallet, then
// `POST /purchase/airtime` (or `/purchase/data`) to buy airtime/data for a
// phone number. Every endpoint shares one envelope, `{ success, data }`
// (docs/guides/01-getting-started.md), and all amounts are whole Naira
// (docs/guides/06-transactions.md's own example shows `amount: 100` for
// what reads as NGN 100) — `FloatMajorUnit`, matching Korapay/Paystack on
// the same NGN rails.
//
// Auth is a raw `X-API-Key` header, no `Bearer` prefix — confirmed against
// the live Swagger UI 2026-09-09 (docs/guides/02-authentication.md and
// docs/openapi/components/schemas.yaml#/securitySchemes/apiKeyAuth).
//
// Source material is this repo's own `the port's ` audit of
// `https://telco.opik.net/api/v1/docs`, which is authoritative for this
// provider (the product owner operates the rail) — see
// `the port's conventions-and-open-items notes` for the
// items that audit still leaves unconfirmed (webhook signing scheme, full
// transaction-status/type enums, error envelope shape, insufficient-balance
// behaviour). Those are flagged in-code below rather than guessed at.

use common_enums::AttemptStatus;
use common_utils::types::FloatMajorUnit;
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_flow_types::payments::PSync,
    router_request_types::{PaymentsSyncData, ResponseId},
    router_response_types::PaymentsResponseData,
    types::{PaymentsAuthorizeRouterData, PaymentsSyncRouterData},
};
use hyperswitch_interfaces::errors;
use hyperswitch_masking::Secret;
use serde::{Deserialize, Serialize};

use crate::{types::ResponseRouterData, utils::RouterData as OtherRouterData};

pub struct OpikRouterData<T> {
    pub amount: FloatMajorUnit,
    pub router_data: T,
}

impl<T> From<(FloatMajorUnit, T)> for OpikRouterData<T> {
    fn from((amount, router_data): (FloatMajorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// opik's real error envelope was never captured from the live server
// (docs/guides/09-conventions-and-open-items.md, item #2) — its audit
// documents a tolerant `{ success, message }` shape as the placeholder.
// Modeled with both fields optional so a bare 4xx/5xx body still parses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OpikErrorResponse {
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
}

// Auth — single `api_key` sent verbatim as the `X-API-Key` header (see
// opik.rs's `get_auth_header`). `HeaderKey` is the matching single-key
// hyperswitch auth type.
pub struct OpikAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for OpikAuthType {
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
// Authorize (collection) — POST /api/v1/purchase/airtime
// ---------------------------------------------------------------------
//
// opik's airtime purchase is the closest analog to a generic "collect from
// a customer" call on this API: `{ network, phoneNumber, amount }` →
// `{ success, data: { reference, amount, phone_number, message } }`
// (docs/guides/05-purchasing-data-airtime.md). The data-bundle purchase
// (`/purchase/data`) is deliberately NOT the Authorize target because it
// requires a pre-provisioned `planId` rather than an amount, which the
// generic Hyperswitch payment request has no first-class field for.
//
// This API has no first-class `network` field in a Hyperswitch payment
// request either, so `network` is read out of the request's `metadata`
// object (one of `MTN | AIRTEL | GLO | 9MOBILE`, per
// docs/openapi/components/schemas.yaml#/schemas/Network). Callers routing a
// VTU payment through this connector must set `metadata.network`; missing it
// fails loudly rather than defaulting to a guessed network.
#[derive(Debug, Serialize)]
pub struct OpikAirtimePurchaseRequest {
    pub network: String,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Secret<String>,
    pub amount: FloatMajorUnit,
}

impl TryFrom<&OpikRouterData<&PaymentsAuthorizeRouterData>> for OpikAirtimePurchaseRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &OpikRouterData<&PaymentsAuthorizeRouterData>) -> Result<Self, Self::Error> {
        // opik hosts no card entry: the call site always supplies a
        // phone-number rail (airtime/data). Any payment-method-data variant
        // is accepted here because the only data this API needs beyond the
        // amount is the network + phone number, both taken from the request
        // rather than from card details.
        match item.router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented("payment method via opik".to_string()),
            )),
        }?;

        let network = item
            .router_data
            .request
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("network"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .ok_or(errors::ConnectorError::MissingRequiredField {
                field_name: "network (opik VTU network — pass via request metadata.network)".into(),
            })?;

        let phone_number = item.router_data.get_billing_phone_number()?;

        Ok(Self {
            network,
            phone_number,
            amount: item.amount,
        })
    }
}

// ---------------------------------------------------------------------
// Response — shared by Authorize and PSync
// ---------------------------------------------------------------------
//
// `{ success, data: { reference, plan_name?, amount, phone_number, message } }`
// per docs/guides/05-purchasing-data-airtime.md. `success` is a boolean
// (opik's own confirmed convention, unlike Xixapay/PaymentPoint's mixed
// boolean-vs-string `status`, and unlike DodoPayments' flat error envelope).
//
// The purchase endpoint's own doc flags that a `200` may mean the purchase
// is already final *or* still `pending` (docs/guides/05, and the
// transaction-status enum is marked CONFIRM in the spec for the same
// reason). `reference` is what `GET /transactions` is then used to
// reconcile. This connector maps `success: true` to `Charged` because that
// is opik's own affirmative signal on the purchase call; the pending
// nuance is flagged in handover.md for a live confirmation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OpikPurchaseData {
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default, rename = "plan_name")]
    pub plan_name: Option<String>,
    #[serde(default)]
    pub amount: Option<FloatMajorUnit>,
    #[serde(default, rename = "phone_number")]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OpikPurchaseResponse {
    pub success: bool,
    #[serde(default)]
    pub data: Option<OpikPurchaseData>,
    #[serde(default)]
    pub message: Option<String>,
}

impl OpikPurchaseResponse {
    fn attempt_status(&self) -> AttemptStatus {
        if self.success {
            AttemptStatus::Charged
        } else {
            AttemptStatus::Failure
        }
    }

    fn reference(&self) -> Option<String> {
        self.data
            .as_ref()
            .and_then(|data| data.reference.clone())
            .filter(|reference| !reference.is_empty())
    }
}

impl<F, T> TryFrom<ResponseRouterData<F, OpikPurchaseResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, OpikPurchaseResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        // opik returns no per-transaction id on the purchase call — the
        // `reference` is the handle `GET /transactions` reconciles against,
        // so it is what this connector stores as the connector transaction
        // id (falling back to the merchant reference when the response
        // omits it, so PSync still has something to look up).
        let resource_id = item
            .response
            .reference()
            .unwrap_or_else(|| item.data.connector_request_reference_id.clone());

        Ok(Self {
            status: item.response.attempt_status(),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(resource_id),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: item.response.reference(),
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
// PSync — GET /api/v1/transactions, reconciled by reference
// ---------------------------------------------------------------------
//
// opik has no single-transaction lookup endpoint; `GET /transactions`
// returns the authenticated business's history
// (docs/guides/06-transactions.md). This connector fetches the default
// page and matches the stored `reference`. The full transaction `status`
// enum is only partially confirmed (`pending` observed; the spec marks the
// value set CONFIRM), so unknown values map to `Pending` rather than being
// treated as terminal — fail-safe-to-non-terminal, matching Flutterwave's
// own posture in this crate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpikTransaction {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub transaction_type: Option<String>,
    #[serde(default)]
    pub amount: Option<FloatMajorUnit>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpikTransactionsResponse {
    pub success: bool,
    #[serde(default)]
    pub data: Vec<OpikTransaction>,
    #[serde(default)]
    pub message: Option<String>,
}

impl OpikTransactionsResponse {
    fn attempt_status(&self, reference: &str) -> AttemptStatus {
        let status = self
            .data
            .iter()
            .find(|transaction| transaction.reference.as_deref() == Some(reference))
            .and_then(|transaction| transaction.status.as_deref());
        match status {
            Some("success") => AttemptStatus::Charged,
            Some("failed") => AttemptStatus::Failure,
            // "pending" plus anything not yet confirmed by opik's own
            // partially-documented enum.
            _ => AttemptStatus::Pending,
        }
    }
}

impl
    TryFrom<
        ResponseRouterData<PSync, OpikTransactionsResponse, PaymentsSyncData, PaymentsResponseData>,
    > for RouterData<PSync, PaymentsSyncData, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<
            PSync,
            OpikTransactionsResponse,
            PaymentsSyncData,
            PaymentsResponseData,
        >,
    ) -> Result<Self, Self::Error> {
        let reference = item
            .data
            .request
            .connector_transaction_id
            .get_connector_transaction_id()
            .unwrap_or_else(|_| item.data.connector_request_reference_id.clone());
        let status = item.response.attempt_status(&reference);
        Ok(Self {
            status,
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
