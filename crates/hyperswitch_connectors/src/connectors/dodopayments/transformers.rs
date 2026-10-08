// DodoPayments transformers.
//
// DodoPayments is a Merchant-of-Record platform whose recommended collection
// surface is Checkout Sessions (`POST /checkouts`), not the legacy
// (deprecated) `POST /payments`. Contract confirmed 2026-10 against
// docs.dodopayments.com/api-reference/checkout-sessions/create and
// /developer-resources/checkout-session:
//
//   POST /checkouts
//     { product_cart: [{ product_id, quantity, amount? }], customer: {...},
//       billing_currency?, return_url?, metadata?, ... }
//     -> { session_id, checkout_url, client_secret?, payment_id?, publishable_key? }
//   GET  /checkouts/{id}
//     -> { id, created_at, customer_email, customer_name,
//          payment_id?, payment_status? }   // payment_status null until paid
//
// Dodo is product-catalog-driven: the price comes from a `product_id`
// pre-provisioned in the Dodo dashboard, not from an arbitrary per-call
// amount like most connectors here. This connector therefore maps
// Hyperswitch's `order_details` onto `product_cart` (each line's
// `product_id` + `quantity` + per-unit `amount`), and requires at least one
// line item. It does NOT invent a product id from the amount.
//
// Auth: `Authorization: Bearer {API_KEY}` on every request.
//
// Error envelope: flat `{ code, message }` with stable machine-readable
// `code` values (docs.dodopayments.com/api-reference/introduction) —
// modeled separately from the success envelope.
//
// NOT modeled here (flagged in handover.md, not guessed at): the Standard
// Webhooks signature scheme (3 headers, base64 HMAC over
// `id.timestamp.body`) and the known currency discrepancy between Dodo's own
// primary sources (USD/INR-only vs. a wider native-settlement set).

use common_enums::AttemptStatus;
use common_utils::{pii::Email, types::MinorUnit};
use hyperswitch_domain_models::{
    payment_method_data::PaymentMethodData,
    router_data::{ConnectorAuthType, RouterData},
    router_flow_types::payments::PSync,
    router_request_types::{PaymentsSyncData, ResponseId},
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

pub struct DodopaymentsRouterData<T> {
    pub amount: MinorUnit,
    pub router_data: T,
}

impl<T> From<(MinorUnit, T)> for DodopaymentsRouterData<T> {
    fn from((amount, router_data): (MinorUnit, T)) -> Self {
        Self {
            amount,
            router_data,
        }
    }
}

// DodoPayments authenticates with a single Bearer API key — `HeaderKey` is
// the matching single-key hyperswitch auth type.
pub struct DodopaymentsAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for DodopaymentsAuthType {
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

// Dodo's real error envelope: `{ code, message }`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DodopaymentsErrorResponse {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

// ---------------------------------------------------------------------
// Authorize (collection) — POST /checkouts
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DodopaymentsProductItem {
    pub product_id: String,
    pub quantity: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<MinorUnit>,
}

#[derive(Debug, Serialize)]
pub struct DodopaymentsCustomer {
    pub email: Email,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Secret<String>>,
}

#[derive(Debug, Serialize)]
pub struct DodopaymentsPaymentsRequest {
    pub product_cart: Vec<DodopaymentsProductItem>,
    pub customer: DodopaymentsCustomer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_currency: Option<common_enums::Currency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_url: Option<String>,
}

impl TryFrom<&DodopaymentsRouterData<&PaymentsAuthorizeRouterData>>
    for DodopaymentsPaymentsRequest
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: &DodopaymentsRouterData<&PaymentsAuthorizeRouterData>,
    ) -> Result<Self, Self::Error> {
        let router_data = item.router_data;
        // Dodo hosts the entire payment page itself; any card/wallet/redirect
        // method lands on the same Checkout Session. The method data is not
        // read out here because it is collected by Dodo at the checkout_url.
        match router_data.request.payment_method_data {
            PaymentMethodData::Card(_)
            | PaymentMethodData::BankRedirect(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Wallet(_) => Ok(()),
            _ => Err(error_stack::Report::from(
                errors::ConnectorError::NotImplemented(
                    "payment method via DodoPayments".to_string(),
                ),
            )),
        }?;

        let email: Email = router_data.request.get_email()?;

        // Product-catalog-driven: each Hyperswitch order line maps onto one
        // Dodo `product_cart` entry. A caller sends `order_details` with the
        // Dodo `product_id` in each line's `product_id`.
        let order_details = router_data.request.order_details.clone().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "order_details (DodoPayments is product-catalog-driven — each line's product_id must be a Dodo product id)".into(),
            },
        )?;
        let product_cart = order_details
            .into_iter()
            .map(|line| {
                Ok(DodopaymentsProductItem {
                    product_id: line.product_id.ok_or(
                        errors::ConnectorError::MissingRequiredField {
                            field_name: "order_details[].product_id".into(),
                        },
                    )?,
                    quantity: line.quantity,
                    amount: Some(line.amount * line.quantity),
                })
            })
            .collect::<Result<Vec<_>, error_stack::Report<errors::ConnectorError>>>()?;

        Ok(Self {
            product_cart,
            customer: DodopaymentsCustomer {
                email,
                name: router_data.get_optional_billing_full_name(),
            },
            billing_currency: Some(router_data.request.currency),
            return_url: router_data.request.router_return_url.clone(),
        })
    }
}

// ---------------------------------------------------------------------
// Authorize response — POST /checkouts success envelope
// ---------------------------------------------------------------------
//
// `{ session_id, checkout_url, client_secret, payment_id, publishable_key }`
// — only `session_id` is guaranteed. `checkout_url` is null when a
// `payment_method_id` was supplied (immediate charge), and `client_secret`/
// `publishable_key` are only present for `confirm: true`. The connector
// always creates a hosted session (no payment_method_id is sent), so
// `checkout_url` is present and the customer is redirected there.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DodopaymentsCheckoutSessionResponse {
    pub session_id: String,
    #[serde(default)]
    pub checkout_url: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub payment_id: Option<String>,
    #[serde(default)]
    pub publishable_key: Option<String>,
}

impl<F, T>
    TryFrom<ResponseRouterData<F, DodopaymentsCheckoutSessionResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, DodopaymentsCheckoutSessionResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let redirection_data = item
            .response
            .checkout_url
            .clone()
            .map(|url| RedirectForm::Form {
                endpoint: url,
                method: common_utils::request::Method::Get,
                form_fields: std::collections::HashMap::new(),
            });
        Ok(Self {
            // The customer has not paid yet — they must complete the hosted
            // checkout at `checkout_url`.
            status: AttemptStatus::AuthenticationPending,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.session_id.clone()),
                redirection_data: Box::new(redirection_data),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: Some(item.response.session_id.clone()),
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
// PSync — GET /checkouts/{id}
// ---------------------------------------------------------------------
//
// The stored `connector_transaction_id` is the checkout `session_id`, so
// PSync reads the session and then inspects `payment_status`
// (`succeeded` / `failed` / `processing`; null while the customer is still
// entering details).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DodopaymentsCheckoutSessionStatus {
    pub id: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub customer_email: Option<Email>,
    #[serde(default)]
    pub customer_name: Option<Secret<String>>,
    #[serde(default)]
    pub payment_id: Option<String>,
    #[serde(default)]
    pub payment_status: Option<String>,
}

impl DodopaymentsCheckoutSessionStatus {
    fn attempt_status(&self) -> AttemptStatus {
        match self.payment_status.as_deref() {
            Some("succeeded") => AttemptStatus::Charged,
            Some("failed") => AttemptStatus::Failure,
            // Null (still entering details) and anything unrecognized stays
            // non-terminal rather than being read as a terminal state.
            _ => AttemptStatus::Pending,
        }
    }
}

impl
    TryFrom<
        ResponseRouterData<
            PSync,
            DodopaymentsCheckoutSessionStatus,
            PaymentsSyncData,
            PaymentsResponseData,
        >,
    > for RouterData<PSync, PaymentsSyncData, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<
            PSync,
            DodopaymentsCheckoutSessionStatus,
            PaymentsSyncData,
            PaymentsResponseData,
        >,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            status: item.response.attempt_status(),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.id.clone()),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                network_txn_link_id: None,
                connector_response_reference_id: item.response.payment_id.clone(),
                incremental_authorization_allowed: None,
                authentication_data: None,
                charges: None,
                payment_account_reference: None,
            }),
            ..item.data
        })
    }
}
