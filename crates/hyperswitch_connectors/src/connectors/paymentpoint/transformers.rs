// PaymentPoint transformers.
//
// PaymentPoint is a Nigerian payments platform. Its only documented
// collection-adjacent surface is virtual-account provisioning
// (`POST /api/v1/createVirtualAccount`) — confirmed against the product
// owner-supplied docs (audit a-8). There is no documented "charge a
// customer" endpoint, so Authorize maps onto virtual-account creation: the
// returned NUBAN account(s) are the funding destination and the attempt
// stays non-terminal until funds arrive.
//
// Auth is three simultaneous credentials on every endpoint:
//   - `Authorization: Bearer {secret key}` header
//   - `api-key: {API key}` header
//   - `businessId` field inside the request body
// carried here via `SignatureKey` (api_key = API key, api_secret = secret
// key, key1 = business id).
//
// Confirmed contract details:
//   - `bankCode` is an ARRAY of partner-bank codes — confirmed values
//     20946 (PalmPay) and 20897 (OPay).
//   - `idType` (`bvn`/`nin`) + `idNumber` (11 digits) are optional; required
//     only when `idType` is set.
//   - Response `status` is the STRING `"success"`; `bankAccounts` is a list
//     (a single call can return more than one funded account).
//   - Amounts are whole Naira base units (webhook example: 100 == ₦100).
//
// Webhook: `Paymentpoint-Signature` header, HMAC-SHA256(raw body, secret
// key) hex — same algorithm/encoding as Korapay's verifier. No replay
// protection (provider-side gap). See paymentpoint.rs.

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

pub struct PaymentpointRouterData<T> {
    pub amount: FloatMajorUnit,
    pub router_data: T,
}

impl<T> From<(FloatMajorUnit, T)> for PaymentpointRouterData<T> {
    fn from((amount, router_data): (FloatMajorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// Three-credential auth: `api_secret` -> Bearer header, `api_key` -> api-key
// header, `key1` -> body `businessId`.
pub struct PaymentpointAuthType {
    pub(super) api_key: Secret<String>,
    pub(super) secret_key: Secret<String>,
    pub(super) business_id: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for PaymentpointAuthType {
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

// PaymentPoint's Errors page gives no stable machine-readable error code —
// only an HTTP status and a human-readable meaning. `status`/`message` are
// modeled here, with the HTTP status carried on `ErrorResponse`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaymentpointErrorResponse {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

// ---------------------------------------------------------------------
// Authorize (collection) — POST /api/v1/createVirtualAccount
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PaymentpointVirtualAccountRequest {
    pub email: Email,
    pub name: Secret<String>,
    #[serde(rename = "phoneNumber")]
    pub phone_number: Secret<String>,
    #[serde(rename = "bankCode")]
    pub bank_code: Vec<String>,
    #[serde(rename = "businessId")]
    pub business_id: Secret<String>,
    #[serde(rename = "idType", skip_serializing_if = "Option::is_none")]
    pub id_type: Option<String>,
    #[serde(rename = "idNumber", skip_serializing_if = "Option::is_none")]
    pub id_number: Option<Secret<String>>,
}

impl TryFrom<&PaymentpointRouterData<&PaymentsAuthorizeRouterData>>
    for PaymentpointVirtualAccountRequest
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &PaymentpointRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        match router_data.request.payment_method_data {
            PaymentMethodData::BankTransfer(_) | PaymentMethodData::BankRedirect(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented(
                    "payment method via PaymentPoint (virtual-account funding only)".to_string(),
                ),
            )),
        }?;

        let metadata = router_data.request.metadata.as_ref();
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
                field_name: "bankCode (PaymentPoint partner-bank codes — pass via request metadata.bankCode)"
                    .into(),
            })?;

        let name = router_data.get_optional_billing_full_name().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "billing.full_name (PaymentPoint virtual-account `name`)".into(),
            },
        )?;
        let email: Email = router_data.request.get_email()?;
        let phone_number = router_data.get_billing_phone_number()?;

        // `businessId` is the third credential, carried in the body.
        let business_id =
            PaymentpointAuthType::try_from(&router_data.connector_auth_type)?.business_id;

        // idType/idNumber are optional; when idType is present the doc
        // requires an 11-digit idNumber.
        let id_type = metadata
            .and_then(|m| m.get("idType"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let id_number = metadata
            .and_then(|m| m.get("idNumber"))
            .and_then(|v| v.as_str())
            .map(|v| Secret::new(v.to_owned()));
        match (&id_type, &id_number) {
            (Some(_), None) => Err::<(), error_stack::Report<errors::ConnectorError>>(
                errors::ConnectorError::MissingRequiredField {
                    field_name: "idNumber (required when idType is supplied)".into(),
                }
                .into(),
            )?,
            (None, Some(_)) => Err::<(), error_stack::Report<errors::ConnectorError>>(
                errors::ConnectorError::MissingRequiredField {
                    field_name: "idType (required when idNumber is supplied)".into(),
                }
                .into(),
            )?,
            _ => {}
        }

        Ok(Self {
            email,
            name,
            phone_number,
            bank_code,
            business_id,
            id_type,
            id_number,
        })
    }
}

// ---------------------------------------------------------------------
// Authorize response — virtual-account creation
// ---------------------------------------------------------------------
//
// `{ status: "success", message, customer, business, bankAccounts: [...],
// errors: [] }`. A provisioned account means funds have not yet arrived, so
// the attempt stays non-terminal (`Pending`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaymentpointBankAccount {
    #[serde(default, rename = "bankCode")]
    pub bank_code: Option<String>,
    #[serde(default, rename = "accountNumber")]
    pub account_number: Option<String>,
    #[serde(default, rename = "accountName")]
    pub account_name: Option<String>,
    #[serde(default, rename = "bankName")]
    pub bank_name: Option<String>,
    #[serde(default, rename = "Reserved_Account_Id")]
    pub reserved_account_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaymentpointVirtualAccountResponse {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default, rename = "bankAccounts")]
    pub bank_accounts: Vec<PaymentpointBankAccount>,
}

impl PaymentpointVirtualAccountResponse {
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

impl<F, T>
    TryFrom<ResponseRouterData<F, PaymentpointVirtualAccountResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, PaymentpointVirtualAccountResponse, T, PaymentsResponseData>,
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
