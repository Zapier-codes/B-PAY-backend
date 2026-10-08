// Xixapay transformers.
//
// Xixapay is a Nigerian payments platform whose only fully-documented
// collection-adjacent surface is virtual-account provisioning
// (`POST /api/v1/createVirtualAccount`) — confirmed 2026-10 against
// documentation.xixapay.com (Authentication, Error Codes, Virtual Account).
// There is no documented "charge a customer" endpoint, so Authorize maps onto
// virtual-account creation: the returned NUBAN account(s) are the funding
// destination, and the payment stays non-terminal until funds arrive.
//
// Auth is three simultaneous credentials on every endpoint:
//   - `Authorization: Bearer {secret key}` header
//   - `api-key: {API key}` header
//   - `businessId` field inside the request body
// (audit a-7). This connector carries them via `SignatureKey`
// (api_key = API key, api_secret = secret key, key1 = business id).
//
// Key confirmed contract details:
//   - `bankCode` is an ARRAY — one call can provision accounts across several
//     partner banks at once.
//   - Partner-bank codes are a closed list: 20867 (PalmPay), 20987 (Kolomoni),
//     29007 (Safehaven), 100004 (Opay, dynamic-only).
//   - `accountType` is `static` (permanent, requires idType/idNumber) or
//     `dynamic` (temporary, requires `amount`).
//   - Response `status` is the STRING `"success"`/`"failed"` on this endpoint
//     (Xixapay is confirmed inconsistent: customer endpoints use a boolean).
//   - Amounts are whole Naira, NGN-only (no currency field anywhere).
//
// Webhook: signature header literally `xixapay`, HMAC-SHA256(raw body, secret)
// hex, with NO timestamp/nonce (no replay protection) — see xixapay.rs.

use common_enums::AttemptStatus;
use common_utils::{pii::Email, types::FloatMajorUnit};
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

// Three-credential auth: `api_secret` -> Bearer header, `api_key` -> api-key
// header, `key1` -> body `businessId`.
pub struct XixapayAuthType {
    pub(super) api_key: Secret<String>,
    pub(super) secret_key: Secret<String>,
    pub(super) business_id: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for XixapayAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::SignatureKey {
                api_key,
                key1,
                api_secret,
            } => Ok(Self {
                api_key: api_key.to_owned(),
                secret_key: api_secret.to_owned(),
                business_id: key1.to_owned(),
            }),
            _ => Err(errors::ConnectorError::FailedToObtainAuthType.into()),
        }
    }
}

// Xixapay's documented error taxonomy: stable snake_case codes
// (bad_request, unauthorized, ...). Modeled separately from the success shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct XixapayErrorResponse {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

// ---------------------------------------------------------------------
// Authorize (collection) — POST /api/v1/createVirtualAccount
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct XixapayVirtualAccountRequest {
    pub email: Email,
    pub name: Secret<String>,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Secret<String>,
    #[serde(rename = "bankCode")]
    pub bank_code: Vec<String>,
    #[serde(rename = "businessId")]
    pub business_id: Secret<String>,
    #[serde(rename = "accountType")]
    pub account_type: String,
    #[serde(rename = "idType", skip_serializing_if = "Option::is_none")]
    pub id_type: Option<String>,
    #[serde(rename = "idNumber", skip_serializing_if = "Option::is_none")]
    pub id_number: Option<Secret<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<FloatMajorUnit>,
}

impl TryFrom<&XixapayRouterData<&PaymentsAuthorizeRouterData>> for XixapayVirtualAccountRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &XixapayRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        match router_data.request.payment_method_data {
            PaymentMethodData::BankTransfer(_) | PaymentMethodData::BankRedirect(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented(
                    "payment method via Xixapay (virtual-account funding only)".to_string(),
                ),
            )),
        }?;

        let metadata = router_data.request.metadata.as_ref();
        let account_type = metadata
            .and_then(|m| m.get("accountType"))
            .and_then(|v| v.as_str())
            .unwrap_or("dynamic")
            .to_string();

        // bankCode is a required array of partner-bank codes; taken from
        // request metadata (no first-class Hyperswitch field carries it).
        let bank_code = metadata
            .and_then(|m| m.get("bankCode"))
            .and_then(|v| v.as_array())
            .map(|codes| {
                codes
                    .iter()
                    .filter_map(|c| c.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .filter(|codes| !codes.is_empty())
            .ok_or(errors::ConnectorError::MissingRequiredField {
                field_name:
                    "bankCode (Xixapay partner-bank codes — pass via request metadata.bankCode)"
                        .into(),
            })?;

        let name = router_data.get_optional_billing_full_name().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.full_name (Xixapay virtual-account `name`)".into(),
            },
        )?;
        let email: Email = router_data.request.get_email()?;
        let phone_number = router_data.get_billing_phone_number()?;

        // `businessId` is the third credential, carried in the body.
        let business_id = XixapayAuthType::try_from(&router_data.connector_auth_type)?.business_id;

        // static accounts require idType/idNumber; dynamic accounts require
        // the amount (already carried on XixapayRouterData).
        let (id_type, id_number) = if account_type == "static" {
            let id_type = metadata
                .and_then(|m| m.get("idType"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or(errors::ConnectorError::MissingRequiredField {
                    field_name: "idType (required when accountType=static)".into(),
                })?;
            let id_number = metadata
                .and_then(|m| m.get("idNumber"))
                .and_then(|v| v.as_str())
                .map(|v| Secret::new(v.to_owned()))
                .ok_or(errors::ConnectorError::MissingRequiredField {
                    field_name: "idNumber (required when accountType=static)".into(),
                })?;
            (Some(id_type), Some(id_number))
        } else {
            (None, None)
        };

        Ok(Self {
            email,
            name,
            phone_number,
            bank_code,
            business_id,
            account_type,
            id_type,
            id_number,
            amount: Some(item.amount),
        })
    }
}

// ---------------------------------------------------------------------
// Authorize response — virtual-account creation
// ---------------------------------------------------------------------
//
// `status` is the string "success"/"failed". A successfully-provisioned
// account means funds have not yet arrived, so the attempt is left in a
// non-terminal `Pending` state; the provisioned account number(s) are
// surfaced for the caller to fund.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct XixapayBankAccount {
    #[serde(default, rename = "bankCode")]
    pub bank_code: Option<String>,
    #[serde(default, rename = "accountNumber")]
    pub account_number: Option<String>,
    #[serde(default, rename = "accountName")]
    pub account_name: Option<String>,
    #[serde(default, rename = "bankName")]
    pub bank_name: Option<String>,
    #[serde(default, rename = "accountTitle")]
    pub account_title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct XixapayVirtualAccountResponse {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default, rename = "bankAccounts")]
    pub bank_accounts: Vec<XixapayBankAccount>,
}

impl XixapayVirtualAccountResponse {
    fn attempt_status(&self) -> AttemptStatus {
        match self.status.as_str() {
            "success" => AttemptStatus::Pending,
            _ => AttemptStatus::Failure,
        }
    }

    fn first_account_number(&self) -> Option<String> {
        self.bank_accounts
            .iter()
            .find_map(|account| account.account_number.clone())
    }
}

impl<F, T> TryFrom<ResponseRouterData<F, XixapayVirtualAccountResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, XixapayVirtualAccountResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let resource_id = item
            .response
            .first_account_number()
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
                connector_response_reference_id: item.response.first_account_number(),
                incremental_authorization_allowed: None,
                authentication_data: None,
                charges: None,
                payment_account_reference: None,
            }),
            ..item.data
        })
    }
}
