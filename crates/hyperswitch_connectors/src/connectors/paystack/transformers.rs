#[cfg(feature = "payouts")]
use api_models::payouts::{BankTransfer, PayoutMethodData};
#[cfg(feature = "payouts")]
use common_enums::PayoutStatus;
use common_enums::{enums, Currency};
use common_utils::{pii::Email, request::Method, types::MinorUnit};
use error_stack::ResultExt;
#[cfg(feature = "payouts")]
use hyperswitch_domain_models::types::{PayoutsResponseData, PayoutsRouterData};
use hyperswitch_domain_models::{
    payment_method_data::{BankRedirectData, PaymentMethodData},
    router_data::{ConnectorAuthType, ErrorResponse, RouterData},
    router_flow_types::refunds::{Execute, RSync},
    router_request_types::ResponseId,
    router_response_types::{PaymentsResponseData, RedirectForm, RefundsResponseData},
    types::{PaymentsAuthorizeRouterData, RefundsRouterData},
};
use hyperswitch_interfaces::errors;
use hyperswitch_masking::Secret;
use serde::{Deserialize, Serialize};
use url::Url;

#[cfg(feature = "payouts")]
use crate::types::PayoutsResponseRouterData;
use crate::{
    types::{RefundsResponseRouterData, ResponseRouterData},
    // `RouterData` here (aliased `OtherRouterData`) is the crate's own
    // `utils::RouterData` extension trait, where `get_payout_method_data()`
    // lives -- same import Korapay's a-1-iii transformers already needed
    // for the same reason.
    utils::{PaymentsAuthorizeRequestData, RouterData as OtherRouterData},
};

pub struct PaystackRouterData<T> {
    pub amount: MinorUnit,
    pub router_data: T,
}

impl<T> From<(MinorUnit, T)> for PaystackRouterData<T> {
    fn from((amount, item): (MinorUnit, T)) -> Self {
        //Todo :  use utils to convert the amount to the type of amount that a connector accepts
        Self {
            amount,
            router_data: item,
        }
    }
}

#[derive(Default, Debug, Serialize, PartialEq)]
pub struct PaystackEftProvider {
    provider: String,
}

#[derive(Default, Debug, Serialize, PartialEq)]
pub struct PaystackPaymentsRequest {
    amount: MinorUnit,
    currency: Currency,
    email: Email,
    eft: PaystackEftProvider,
}

impl TryFrom<&PaystackRouterData<&PaymentsAuthorizeRouterData>> for PaystackPaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &PaystackRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        match item.router_data.request.payment_method_data.clone() {
            PaymentMethodData::BankRedirect(BankRedirectData::Eft { provider }) => {
                let email = item.router_data.request.get_email()?;
                let eft = PaystackEftProvider { provider };
                Ok(Self {
                    amount: item.amount,
                    currency: item.router_data.request.currency,
                    email,
                    eft,
                })
            }
            _ => Err(errors::ConnectorError::NotImplemented("Payment method".to_string()).into()),
        }
    }
}

pub struct PaystackAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for PaystackAuthType {
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

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaystackEftRedirect {
    reference: String,
    status: String,
    url: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaystackPaymentsResponseData {
    status: bool,
    message: String,
    data: PaystackEftRedirect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PaystackPaymentsResponse {
    PaystackPaymentsData(PaystackPaymentsResponseData),
    PaystackPaymentsError(PaystackErrorResponse),
}

impl<F, T> TryFrom<ResponseRouterData<F, PaystackPaymentsResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, PaystackPaymentsResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let (status, response) = match item.response {
            PaystackPaymentsResponse::PaystackPaymentsData(resp) => {
                let redirection_url = Url::parse(resp.data.url.as_str())
                    .change_context(errors::ConnectorError::ParsingFailed)?;
                let redirection_data = RedirectForm::from((redirection_url, Method::Get));
                (
                    common_enums::AttemptStatus::AuthenticationPending,
                    Ok(PaymentsResponseData::TransactionResponse {
                        resource_id: ResponseId::ConnectorTransactionId(
                            resp.data.reference.clone(),
                        ),
                        redirection_data: Box::new(Some(redirection_data)),
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
                )
            }
            PaystackPaymentsResponse::PaystackPaymentsError(err) => {
                let err_msg = get_error_message(err.clone());
                (
                    common_enums::AttemptStatus::Failure,
                    Err(ErrorResponse {
                        code: err.code,
                        message: err_msg.clone(),
                        reason: Some(err_msg.clone()),
                        attempt_status: None,
                        connector_transaction_id: None,
                        connector_response_reference_id: None,
                        status_code: item.http_code,
                        network_advice_code: None,
                        network_decline_code: None,
                        network_error_message: None,
                        connector_metadata: None,
                    }),
                )
            }
        };
        Ok(Self {
            status,
            response,
            ..item.data
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PaystackPSyncStatus {
    Abandoned,
    Failed,
    Ongoing,
    Pending,
    Processing,
    Queued,
    Reversed,
    Success,
}

impl From<PaystackPSyncStatus> for common_enums::AttemptStatus {
    fn from(item: PaystackPSyncStatus) -> Self {
        match item {
            PaystackPSyncStatus::Success => Self::Charged,
            PaystackPSyncStatus::Abandoned => Self::AuthenticationPending,
            PaystackPSyncStatus::Ongoing
            | PaystackPSyncStatus::Pending
            | PaystackPSyncStatus::Processing
            | PaystackPSyncStatus::Queued => Self::Pending,
            PaystackPSyncStatus::Failed => Self::Failure,
            PaystackPSyncStatus::Reversed => Self::Voided,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaystackPSyncData {
    status: PaystackPSyncStatus,
    reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaystackPSyncResponseData {
    status: bool,
    message: String,
    data: PaystackPSyncData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PaystackPSyncResponse {
    PaystackPSyncData(PaystackPSyncResponseData),
    PaystackPSyncWebhook(PaystackPaymentWebhookData),
    PaystackPSyncError(PaystackErrorResponse),
}

impl<F, T> TryFrom<ResponseRouterData<F, PaystackPSyncResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, PaystackPSyncResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        match item.response {
            PaystackPSyncResponse::PaystackPSyncData(resp) => Ok(Self {
                status: common_enums::AttemptStatus::from(resp.data.status),
                response: Ok(PaymentsResponseData::TransactionResponse {
                    resource_id: ResponseId::ConnectorTransactionId(resp.data.reference.clone()),
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
            }),
            PaystackPSyncResponse::PaystackPSyncWebhook(resp) => Ok(Self {
                status: common_enums::AttemptStatus::from(resp.status),
                response: Ok(PaymentsResponseData::TransactionResponse {
                    resource_id: ResponseId::ConnectorTransactionId(resp.reference.clone()),
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
            }),
            PaystackPSyncResponse::PaystackPSyncError(err) => {
                let err_msg = get_error_message(err.clone());
                Ok(Self {
                    response: Err(ErrorResponse {
                        code: err.code,
                        message: err_msg.clone(),
                        reason: Some(err_msg.clone()),
                        attempt_status: None,
                        connector_transaction_id: None,
                        connector_response_reference_id: None,
                        status_code: item.http_code,
                        network_advice_code: None,
                        network_decline_code: None,
                        network_error_message: None,
                        connector_metadata: None,
                    }),
                    ..item.data
                })
            }
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaystackRefundRequest {
    pub transaction: String,
    pub amount: MinorUnit,
}

impl<F> TryFrom<&PaystackRouterData<&RefundsRouterData<F>>> for PaystackRefundRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &PaystackRouterData<&RefundsRouterData<F>>) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: item.router_data.request.connector_transaction_id.clone(),
            amount: item.amount.to_owned(),
        })
    }
}

#[derive(Debug, Serialize, Default, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PaystackRefundStatus {
    Processed,
    Failed,
    #[default]
    Processing,
    Pending,
}

impl From<PaystackRefundStatus> for enums::RefundStatus {
    fn from(item: PaystackRefundStatus) -> Self {
        match item {
            PaystackRefundStatus::Processed => Self::Success,
            PaystackRefundStatus::Failed => Self::Failure,
            PaystackRefundStatus::Processing | PaystackRefundStatus::Pending => Self::Pending,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaystackRefundsData {
    status: PaystackRefundStatus,
    id: i64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaystackRefundsResponseData {
    status: bool,
    message: String,
    data: PaystackRefundsData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PaystackRefundsResponse {
    PaystackRefundsData(PaystackRefundsResponseData),
    PaystackRSyncWebhook(PaystackRefundWebhookData),
    PaystackRefundsError(PaystackErrorResponse),
}

impl TryFrom<RefundsResponseRouterData<Execute, PaystackRefundsResponse>>
    for RefundsRouterData<Execute>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: RefundsResponseRouterData<Execute, PaystackRefundsResponse>,
    ) -> Result<Self, Self::Error> {
        match item.response {
            PaystackRefundsResponse::PaystackRefundsData(resp) => Ok(Self {
                response: Ok(RefundsResponseData {
                    connector_refund_id: resp.data.id.to_string(),
                    refund_status: enums::RefundStatus::from(resp.data.status),
                }),
                ..item.data
            }),
            PaystackRefundsResponse::PaystackRSyncWebhook(resp) => Ok(Self {
                response: Ok(RefundsResponseData {
                    connector_refund_id: resp.id,
                    refund_status: enums::RefundStatus::from(resp.status),
                }),
                ..item.data
            }),
            PaystackRefundsResponse::PaystackRefundsError(err) => {
                let err_msg = get_error_message(err.clone());
                Ok(Self {
                    response: Err(ErrorResponse {
                        code: err.code,
                        message: err_msg.clone(),
                        reason: Some(err_msg.clone()),
                        attempt_status: None,
                        connector_transaction_id: None,
                        connector_response_reference_id: None,
                        status_code: item.http_code,
                        network_advice_code: None,
                        network_decline_code: None,
                        network_error_message: None,
                        connector_metadata: None,
                    }),
                    ..item.data
                })
            }
        }
    }
}

impl TryFrom<RefundsResponseRouterData<RSync, PaystackRefundsResponse>>
    for RefundsRouterData<RSync>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: RefundsResponseRouterData<RSync, PaystackRefundsResponse>,
    ) -> Result<Self, Self::Error> {
        match item.response {
            PaystackRefundsResponse::PaystackRefundsData(resp) => Ok(Self {
                response: Ok(RefundsResponseData {
                    connector_refund_id: resp.data.id.to_string(),
                    refund_status: enums::RefundStatus::from(resp.data.status),
                }),
                ..item.data
            }),
            PaystackRefundsResponse::PaystackRSyncWebhook(resp) => Ok(Self {
                response: Ok(RefundsResponseData {
                    connector_refund_id: resp.id,
                    refund_status: enums::RefundStatus::from(resp.status),
                }),
                ..item.data
            }),
            PaystackRefundsResponse::PaystackRefundsError(err) => {
                let err_msg = get_error_message(err.clone());
                Ok(Self {
                    response: Err(ErrorResponse {
                        code: err.code,
                        message: err_msg.clone(),
                        reason: Some(err_msg.clone()),
                        attempt_status: None,
                        connector_transaction_id: None,
                        connector_response_reference_id: None,
                        status_code: item.http_code,
                        network_advice_code: None,
                        network_decline_code: None,
                        network_error_message: None,
                        connector_metadata: None,
                    }),
                    ..item.data
                })
            }
        }
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaystackErrorResponse {
    pub status: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub meta: serde_json::Value,
    pub code: String,
}

pub fn get_error_message(response: PaystackErrorResponse) -> String {
    if let Some(serde_json::Value::Object(err_map)) = response.data {
        err_map.get("message").map(|msg| msg.clone().to_string())
    } else {
        None
    }
    .unwrap_or(response.message)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaystackPaymentWebhookData {
    pub status: PaystackPSyncStatus,
    pub reference: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaystackRefundWebhookData {
    pub status: PaystackRefundStatus,
    pub id: String,
    pub transaction_reference: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum PaystackWebhookEventData {
    Payment(PaystackPaymentWebhookData),
    Refund(PaystackRefundWebhookData),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaystackWebhookData {
    pub event: String,
    pub data: PaystackWebhookEventData,
}

impl From<PaystackWebhookEventData> for api_models::webhooks::IncomingWebhookEvent {
    fn from(item: PaystackWebhookEventData) -> Self {
        match item {
            PaystackWebhookEventData::Payment(payment_data) => match payment_data.status {
                PaystackPSyncStatus::Success => Self::PaymentIntentSuccess,
                PaystackPSyncStatus::Failed => Self::PaymentIntentFailure,
                PaystackPSyncStatus::Abandoned
                | PaystackPSyncStatus::Ongoing
                | PaystackPSyncStatus::Pending
                | PaystackPSyncStatus::Processing
                | PaystackPSyncStatus::Queued => Self::PaymentIntentProcessing,
                PaystackPSyncStatus::Reversed => Self::EventNotSupported,
            },
            PaystackWebhookEventData::Refund(refund_data) => match refund_data.status {
                PaystackRefundStatus::Processed => Self::RefundSuccess,
                PaystackRefundStatus::Failed => Self::RefundFailure,
                PaystackRefundStatus::Processing | PaystackRefundStatus::Pending => {
                    Self::EventNotSupported
                }
            },
        }
    }
}

// ---------------------------------------------------------------------
// Payout Recipient — POST /transferrecipient
// Payout Fulfill   — POST /transfer
// Payout Sync      — GET  /transfer/verify/{reference}
// ---------------------------------------------------------------------
//
// Task 77/a-2-iii. Ported directly from this repo's own
// legacy-node/providers/paystack.js#createTransferRecipient()/
// processPayout()/verifyPayout() -- itself Task 51/c's already-battle-
// tested, primary-source-confirmed fix (paystack.com/docs/api/
// transfer-recipient/, paystack.com/docs/api/transfer/,
// paystack.com/docs/transfers/{creating-transfer-recipients,
// single-transfers,bulk-transfers}). Unlike Korapay (Task 77/a-1-iii),
// which takes a raw bank_code/account_number pair inline on a single
// disburse call, Paystack requires a transfer *recipient* to exist
// first and returns a `recipient_code` that the actual transfer
// references -- a genuinely different, two-call shape, not a
// one-call port. This is modeled here as its own domain flow
// (`PoRecipient`), the same architecture Wise's own connector already
// uses for its own "create the payee first" step (see wise.rs's
// `ConnectorIntegration<PoRecipient, ...>` and how its response's
// `connector_payout_id` is read back by a later flow) -- not a new
// pattern invented for this connector.
//
// `PayoutsData.connector_payout_id` is reused, in sequence, to carry
// two DIFFERENT provider-side identifiers across the three flows below
// (this is also exactly what Wise's own three-step chain does, not a
// Paystack-specific shortcut): `PoRecipient`'s response writes
// Paystack's `recipient_code` into it; `PoFulfill` reads that
// recipient_code back out to build the `/transfer` call, then
// overwrites `connector_payout_id` again with the real transfer
// `reference` so `PoSync` can poll `/transfer/verify/{reference}`
// against it.

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaystackRecipientType {
    Nuban,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct PaystackRecipientCreateRequest {
    #[serde(rename = "type")]
    pub recipient_type: PaystackRecipientType,
    pub name: Secret<String>,
    pub account_number: Secret<String>,
    pub bank_code: Secret<String>,
    pub currency: enums::Currency,
}

// ⚠️ Real, unresolved shape gap -- flagged, not guessed around, same
// root cause and same stopgap already flagged in
// korapay/transformers.rs's own `get_korapay_payout_bank_account`:
// Hyperswitch's `PayoutMethodData` (api_models::payouts) has no
// NUBAN/bank-code-shaped variant. `BankTransfer::Ach` is reused purely
// because it is the one variant with two plain (non-IBAN,
// non-BIC-formatted) string fields -- `bank_account_number` carries
// the NUBAN account number, `bank_routing_number` carries Paystack's
// own bank code (from Paystack's own `GET /bank` list, not a US ABA
// routing number, which is what that field is documented elsewhere in
// this same enum as). Not a confirmed-correct mapping -- do not trust
// this in production before either a live Paystack sandbox call
// confirms it round-trips, or a proper NUBAN-shaped `PayoutMethodData`
// variant is added upstream and both this connector and Korapay's are
// switched to it together. Every other `BankTransfer` variant, and
// every non-`BankTransfer` variant, is rejected with `NotSupported`
// rather than guessed at -- same discipline as Korapay's own helper.
#[cfg(feature = "payouts")]
fn get_paystack_payout_bank_account<F>(
    router_data: &PayoutsRouterData<F>,
) -> Result<(Secret<String>, Secret<String>), error_stack::Report<errors::ConnectorError>> {
    match router_data.get_payout_method_data()? {
        PayoutMethodData::BankTransfer(BankTransfer::Ach(ach)) => {
            Ok((ach.bank_account_number, ach.bank_routing_number))
        }
        other => Err(errors::ConnectorError::NotSupported {
            message: format!(
                "{other:?} via Paystack payouts (see paystack/transformers.rs's own \
                 get_paystack_payout_bank_account note on the real NUBAN/bank-code shape gap)"
            ),
            connector: "paystack",
        }
        .into()),
    }
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<&PayoutsRouterData<F>> for PaystackRecipientCreateRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(router_data: &PayoutsRouterData<F>) -> Result<Self, Self::Error> {
        let (account_number, bank_code) = get_paystack_payout_bank_account(router_data)?;

        // Paystack's own docs require a `name` on the recipient.
        // Mirrors legacy-node/providers/paystack.js#createTransferRecipient()'s
        // own fallback chain exactly (`data.customer?.name ||
        // data.account_name || data.account_number`) -- Hyperswitch's
        // `PayoutsData` has no separate `account_name` field, so that
        // middle fallback collapses, but the outer shape (prefer a real
        // customer name, fall back to the account number itself rather
        // than failing the request) is preserved.
        let name = router_data
            .request
            .customer_details
            .as_ref()
            .and_then(|customer| customer.name.clone())
            .unwrap_or_else(|| account_number.clone());

        Ok(Self {
            recipient_type: PaystackRecipientType::Nuban,
            name,
            account_number,
            bank_code,
            currency: router_data.request.destination_currency,
        })
    }
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaystackRecipientData {
    pub recipient_code: String,
    #[serde(default)]
    pub active: Option<bool>,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaystackRecipientResponse {
    pub status: bool,
    pub message: String,
    pub data: PaystackRecipientData,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<PayoutsResponseRouterData<F, PaystackRecipientResponse>> for PayoutsRouterData<F> {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: PayoutsResponseRouterData<F, PaystackRecipientResponse>,
    ) -> Result<Self, Self::Error> {
        // Same "outer `status: false` is a thrown API-call-level
        // failure" discipline as every other Paystack/Korapay response
        // handler in this file and legacy-node/providers/paystack.js's
        // own `if (!response.ok || !responseData.status) throw ...` --
        // Paystack's own docs confirm a duplicate `account_number`
        // returns the existing record rather than erroring, so a real
        // `status: false` here is a genuine failure, not a
        // resubmission concern.
        if !item.response.status {
            return Err(errors::ConnectorError::ResponseHandlingFailed.into());
        }
        Ok(Self {
            response: Ok(PayoutsResponseData {
                status: Some(PayoutStatus::RequiresFulfillment),
                connector_payout_id: Some(item.response.data.recipient_code.clone()),
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

#[cfg(feature = "payouts")]
#[derive(Debug, Serialize)]
pub struct PaystackPayoutFulfillRequest {
    pub source: String,
    pub amount: MinorUnit,
    pub recipient: String,
    pub reference: String,
    pub reason: String,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<&PaystackRouterData<&PayoutsRouterData<F>>> for PaystackPayoutFulfillRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &PaystackRouterData<&PayoutsRouterData<F>>) -> Result<Self, Self::Error> {
        let router_data = item.router_data;

        // The recipient must already exist -- `PoRecipient` runs before
        // `PoFulfill` in the payout flow chain (see this section's own
        // file-level comment above) and its response leaves the
        // `recipient_code` here, in `connector_payout_id`. A missing
        // value here means `PoRecipient` was skipped or failed, which
        // is a real orchestration precondition, not something this
        // request can recover from -- fail loudly rather than send
        // Paystack an empty `recipient`.
        let recipient = router_data.request.connector_payout_id.clone().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "connector_payout_id (Paystack recipient_code from PoRecipient)".into(),
            },
        )?;

        Ok(Self {
            source: "balance".to_string(),
            amount: item.amount,
            recipient,
            reference: router_data.connector_request_reference_id.clone(),
            // Hyperswitch's `PayoutsData` carries no narration/reason
            // field (unlike the legacy JS request, which took a
            // caller-supplied `narration`/`reason` or fell back to a
            // Mavins-specific default) -- a generic, connector-level
            // default is used here instead of guessing at a field this
            // request type doesn't have, same choice Korapay's own
            // `narration` field already made in a-1-iii.
            reason: "Payout via Paystack".to_string(),
        })
    }
}

// Real lifecycle state of the transfer itself -- distinct from the
// outer `status: bool` on `PaystackPayoutResponse`, which only ever
// means "did Paystack accept/find this API call" (same two-level
// shape as every other response type in this file).
//
// ⚠️ `Otp` is a real, flagged caveat carried over unchanged from
// legacy-node/providers/paystack.js#processPayout()'s own comment:
// Paystack's transfer `status` comes back `"otp"`, not `"pending"`,
// unless the Transfers OTP requirement is disabled on the
// integration's own dashboard -- and an `"otp"` transfer needs a
// human to finalize it with a one-time code
// (`/transfer/finalize_transfer`, deliberately NOT implemented here,
// same as the legacy code: a server-side integration has no way to
// receive or supply that code). `Otp` is mapped to
// `PayoutStatus::Pending` below because it is the closest available
// non-terminal status Hyperswitch's own `PayoutStatus` enum offers --
// but unlike a normal `Pending`, it will NOT resolve on its own.
// Disabling OTP account-side (a dashboard setting) or building
// `finalize_transfer` support are both open items for whichever
// session or the product owner picks this up next, not solved here.
#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PaystackPayoutTransactionStatus {
    Success,
    Failed,
    #[default]
    Pending,
    Otp,
    #[serde(other)]
    Unknown,
}

#[cfg(feature = "payouts")]
impl From<PaystackPayoutTransactionStatus> for PayoutStatus {
    fn from(status: PaystackPayoutTransactionStatus) -> Self {
        match status {
            PaystackPayoutTransactionStatus::Success => Self::Success,
            PaystackPayoutTransactionStatus::Failed => Self::Failed,
            // See this section's own file-level `Otp` comment above --
            // NOT equivalent to a normal `Pending` in practice, but the
            // closest terminal-vs-non-terminal fit Hyperswitch's own
            // enum has.
            PaystackPayoutTransactionStatus::Pending
            | PaystackPayoutTransactionStatus::Otp
            | PaystackPayoutTransactionStatus::Unknown => Self::Pending,
        }
    }
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaystackPayoutData {
    pub reference: String,
    pub status: PaystackPayoutTransactionStatus,
    #[serde(default)]
    pub transfer_code: Option<String>,
    // Per legacy-node's own `responseData.data?.message ||
    // responseData.message` fallback chain -- not confirmed against a
    // live Paystack response this session (no working rustc; see
    // New-Clone Checklist), so `#[serde(default)]` keeps this optional
    // rather than assuming the field is always present, same caveat
    // Korapay's own `KorapayPayoutData.message` already carries.
    #[serde(default)]
    pub message: Option<String>,
}

#[cfg(feature = "payouts")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaystackPayoutResponse {
    pub status: bool,
    pub message: String,
    pub data: PaystackPayoutData,
}

#[cfg(feature = "payouts")]
impl<F> TryFrom<PayoutsResponseRouterData<F, PaystackPayoutResponse>> for PayoutsRouterData<F> {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: PayoutsResponseRouterData<F, PaystackPayoutResponse>,
    ) -> Result<Self, Self::Error> {
        // Outer `status: false` is Paystack's own signal that the API
        // call itself was rejected -- a 2xx with `status: false` is
        // treated as a real, thrown failure here, same discipline as
        // every other response handler in this file and every method
        // in legacy-node/providers/paystack.js. This is deliberately
        // different from a `data.status: "failed"` outcome below,
        // which -- per that same legacy method's own comment -- is a
        // normal, successfully-verified terminal transfer state, not
        // an error calling this function.
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
                // `PoFulfill`'s own response overwrites
                // `connector_payout_id` again here -- from the
                // recipient_code it was reading in, to the real
                // transfer `reference` -- so `PoSync` (below) has the
                // right identifier to poll
                // `/transfer/verify/{reference}` against. Prefers
                // `reference` (what `verifyPayout`'s own endpoint takes)
                // over `transfer_code`, matching
                // legacy-node/providers/paystack.js#verifyPayout()'s own
                // reference-keyed lookup exactly.
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
