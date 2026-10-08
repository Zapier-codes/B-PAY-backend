// Task 77 scaffold -- Xixapay transformers, generated from the
// compile-verified Remita transformers template. Request/response field
// shapes below are the generic hosted-checkout shape Remita confirmed; they
// are a PLACEHOLDER for Xixapay's real payload and must be replaced
// against Xixapay's own API docs (see the connector file's header)
// before this connector is used for real money.
//
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

// Xixapay's "Accept Online Payments" (Checkout Solutions) surface -- the
// First Gen surface Task 50/c identified as the recommended target, not the
// classic RRR Invoice-Generation flow (whose base URL/auth scheme is still
// unresolved per Task 49/b and Task 50/b). Amount unit is NOT confirmed by
// any primary source this connector had: Task 50/c records one worked
// example using `10000` for a "Test Transaction" with no stated
// currency-unit rule, and the product owner has not supplied the merchant
// onboarding email that would settle it. `FloatMajorUnit` (whole Naira, not
// kobo) is chosen to match Korapay/Paystack's own established
// major-unit behaviour for the same NGN rails, and is flagged in
// legacy-node/handover.md as the one field to re-confirm against a live
// sandbox call before production use -- same discipline as Korapay's own
// flagged-but-unconfirmed response-field note.
pub struct XixapayRouterData<T> {
    pub amount: FloatMajorUnit,
    pub router_data: T,
}

impl<T> From<(FloatMajorUnit, T)> for XixapayRouterData<T> {
    fn from((amount, router_data): (FloatMajorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// Auth Struct
// Xixapay's Checkout Solutions surface authenticates with a flat `secretKey`
// header -- Task 50/c confirms this explicitly and notes it is NOT either of
// the two hash schemes Xixapay's classic-RRR research previously guessed at.
// HeaderKey is hyperswitch's matching single-key auth type (same one
// Korapay/Paystack use for their single Bearer/HMAC key), and
// ConnectorCommon::get_auth_header (see xixapay.rs) emits it under Xixapay's
// own `secretKey` header name rather than `Authorization`.
pub struct XixapayAuthType {
    pub(super) secret_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for XixapayAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::HeaderKey { api_key } => Ok(Self {
                secret_key: api_key.to_owned(),
            }),
            _ => Err(errors::ConnectorError::FailedToObtainAuthType.into()),
        }
    }
}

// ---------------------------------------------------------------------
// Authorize (collection) — POST /services/connect-gateway/api/v1/payment/charge
// ---------------------------------------------------------------------
//
// Request shape confirmed against Task 50/c's real worked example:
// `firstName`, `lastName`, `email`, `phoneNumber`, `paymentIdentifier`,
// `currency`, `narration`, `amount`. `paymentIdentifier` is the
// merchant-generated reference and maps onto
// `connector_request_reference_id` exactly the way every other connector in
// this crate maps its own reference. The optional `split` object Task 50/c
// mentions is deliberately NOT modelled: its full schema is not explained in
// any supplied source, so forwarding it would be a guess -- callers who need
// sub-account splits need that schema confirmed first.
//
// The endpoint path itself is a real, confirmed inconsistency within
// Xixapay's own supplied doc: the prose says
// `.../services/connect-gateway/api/v1/payment/charge` while the same doc's
// own curl example shows `.../payment-engine/payment/charge` (Task 50/c,
// same class of finding as Flutterwave's inverted env-select ternary). The
// prose path is used here; the discrepancy is flagged in handover.md for a
// live-call confirmation rather than silently picking one as if settled.
#[derive(Debug, Serialize)]
pub struct XixapayPaymentsRequest {
    #[serde(rename = "firstName")]
    pub first_name: Secret<String>,
    #[serde(rename = "lastName")]
    pub last_name: Secret<String>,
    pub email: Email,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Secret<String>,
    #[serde(rename = "paymentIdentifier")]
    pub payment_identifier: String,
    pub currency: enums::Currency,
    pub narration: String,
    pub amount: FloatMajorUnit,
}

impl TryFrom<&XixapayRouterData<&PaymentsAuthorizeRouterData>> for XixapayPaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &XixapayRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        // Card/redirect/bank-transfer/mobile-money all funnel through
        // Xixapay's single hosted-checkout `payment/charge` endpoint -- there
        // is no separate direct-card API on this surface, so any
        // payment-method-data variant lands here the same way; nothing
        // card-specific is read out of `PaymentMethodData` because Xixapay
        // hosts card entry itself at the returned `paymentLink`.
        match item.router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented(
                    "payment method via Xixapay".to_string(),
                ),
            )),
        }?;

        let email: Email = item.router_data.request.get_email()?;
        let first_name = item
            .router_data
            .get_optional_billing_first_name()
            .unwrap_or_else(|| Secret::new("".to_string()));
        let last_name = item
            .router_data
            .get_optional_billing_last_name()
            .unwrap_or_else(|| Secret::new("".to_string()));
        // `phoneNumber` is required by Xixapay's own confirmed request shape
        // (Task 50/c lists it as a plain required field, not optional) --
        // fail loudly rather than send a placeholder Xixapay will reject.
        let phone_number = item.router_data.get_optional_billing_phone_number().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "phone_number (required by Xixapay's confirmed \
                    payment/charge shape)"
                    .into(),
            },
        )?;

        Ok(Self {
            first_name,
            last_name,
            email,
            phone_number,
            payment_identifier: item.router_data.connector_request_reference_id.clone(),
            currency: item.router_data.request.currency,
            narration: "Payment".to_string(),
            amount: item.amount,
        })
    }
}

// ---------------------------------------------------------------------
// Response — shared by Authorize and PSync
// ---------------------------------------------------------------------
//
// ⚠️ Shape below (`status: "00"`, `message`, `data.paymentLink`) is from
// Task 50/c's real worked example for the charge call -- NOT re-confirmed
// via a live sandbox call in this session. Xixapay's `status` is a
// two-character string code, not a boolean or an enum: `"00"` is the one
// confirmed success value ("Approved or Completed Successfully."). The full
// set of `status`/`message` values beyond that one success case is
// explicitly not documented in any supplied source (Task 50/c), so any
// non-`"00"` code maps to `Failure` conservatively rather than being
// pattern-matched as if the code table were known. The PSync verify
// response shape is likewise not independently documented -- the same
// envelope is reused here, flagged in handover.md for live confirmation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct XixapayChargeData {
    #[serde(rename = "paymentLink")]
    pub payment_link: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct XixapayPaymentsResponse {
    pub status: String,
    pub message: String,
    pub data: XixapayChargeData,
}

impl XixapayPaymentsResponse {
    fn attempt_status(&self) -> AttemptStatus {
        match self.status.as_str() {
            "00" => AttemptStatus::Charged,
            _ => AttemptStatus::Failure,
        }
    }
}

impl<F, T> TryFrom<ResponseRouterData<F, XixapayPaymentsResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, XixapayPaymentsResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let redirection_data =
            item.response
                .data
                .payment_link
                .clone()
                .map(|url| RedirectForm::Form {
                    endpoint: url,
                    method: common_utils::request::Method::Get,
                    form_fields: std::collections::HashMap::new(),
                });

        Ok(Self {
            status: item.response.attempt_status(),
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
