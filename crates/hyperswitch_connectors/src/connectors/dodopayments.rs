// DodoPayments connector — Checkout Sessions collection surface.
//
// DodoPayments is a Merchant-of-Record platform. Its recommended collection
// surface is Checkout Sessions (`POST /checkouts`), which is what this
// connector implements; the legacy `POST /payments` is deprecated. See
// dodopayments/transformers.rs for the full request/response contract.
//
// Base URL: https://test.dodopayments.com/
// Auth:     Authorization: Bearer {API_KEY}
// Authorize: POST /checkouts
// PSync:     GET  /checkouts/{id}
//
pub mod transformers;

use std::sync::LazyLock;

use common_enums::enums;
use common_utils::{
    errors::CustomResult,
    ext_traits::BytesExt,
    request::{Method, Request, RequestBuilder, RequestContent},
    types::{AmountConvertor, MinorUnit, MinorUnitForConnector},
};
use error_stack::ResultExt;
use hyperswitch_domain_models::{
    router_data::{AccessToken, ConnectorAuthType, ErrorResponse, RouterData},
    router_flow_types::{
        access_token_auth::AccessTokenAuth,
        payments::{Authorize, Capture, PSync, PaymentMethodToken, Session, SetupMandate, Void},
        refunds::{Execute, RSync},
    },
    router_request_types::{
        AccessTokenRequestData, PaymentMethodTokenizationData, PaymentsAuthorizeData,
        PaymentsCancelData, PaymentsCaptureData, PaymentsSessionData, PaymentsSyncData,
        RefundsData, SetupMandateRequestData,
    },
    router_response_types::{
        ConnectorInfo, PaymentMethodDetails, PaymentsResponseData, RefundsResponseData,
        SupportedPaymentMethods, SupportedPaymentMethodsExt,
    },
    types::{
        PaymentsAuthorizeRouterData, PaymentsCaptureRouterData, PaymentsSyncRouterData,
        RefundsRouterData,
    },
};
use hyperswitch_interfaces::{
    api::{
        self, ConnectorCommon, ConnectorCommonExt, ConnectorIntegration, ConnectorSpecifications,
        ConnectorValidation,
    },
    configs::Connectors,
    errors,
    events::connector_api_logs::ConnectorEvent,
    types::{PaymentsAuthorizeType, PaymentsSyncType, Response},
    webhooks,
};
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as dodopayments;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

// DodoPayments's Checkout Sessions surface — see dodopayments/transformers.rs
// for the contract sourced from docs.dodopayments.com. Collection-only; no
// payout flows.
#[derive(Clone)]
pub struct Dodopayments {
    amount_converter: &'static (dyn AmountConvertor<Output = MinorUnit> + Sync),
}

impl Dodopayments {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &MinorUnitForConnector,
        }
    }
}

impl api::Payment for Dodopayments {}
impl api::PaymentSession for Dodopayments {}
impl api::ConnectorAccessToken for Dodopayments {}
impl api::MandateSetup for Dodopayments {}
impl api::PaymentAuthorize for Dodopayments {}
impl api::PaymentSync for Dodopayments {}
impl api::PaymentCapture for Dodopayments {}
impl api::PaymentVoid for Dodopayments {}
impl api::Refund for Dodopayments {}
impl api::RefundExecute for Dodopayments {}
impl api::RefundSync for Dodopayments {}
impl api::PaymentToken for Dodopayments {}

// DodoPayments's Checkout Sessions surface has no payout flows (collection
// only) — deliberately no `impl api::Payouts for Dodopayments`, so this
// crate's own `default_imp_for_payouts*!` macros keep supplying
// Dodopayments's no-op default for every payout flow.

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Dodopayments
{
    // Not Implemented (R) — Dodo hosts card entry itself at the returned
    // `checkout_url`; there is no separate tokenization step.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Dodopayments
where
    Self: ConnectorIntegration<Flow, Request, Response>,
{
    fn build_headers(
        &self,
        req: &RouterData<Flow, Request, Response>,
        _connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let mut header = vec![(
            headers::CONTENT_TYPE.to_string(),
            self.get_content_type().to_string().into(),
        )];
        let mut api_key = self.get_auth_header(&req.connector_auth_type)?;
        header.append(&mut api_key);
        Ok(header)
    }
}

impl ConnectorCommon for Dodopayments {
    fn id(&self) -> &'static str {
        "dodopayments"
    }

    // Dodo's own `product_price` must be in "the lowest denomination of the
    // currency (e.g. cents for USD)" (docs.dodopayments.com preview docs) —
    // minor units, matching Hyperswitch's canonical amount.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Minor
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.dodopayments.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = dodopayments::DodopaymentsAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        // DodoPayments: `Authorization: Bearer {API_KEY}` on every request.
        Ok(vec![(
            "Authorization".to_string(),
            format!("Bearer {}", auth.api_key.expose()).into_masked(),
        )])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        // Dodo's real error envelope is flat `{ code, message }` with a
        // stable machine-readable `code` (docs.dodopayments.com/api-reference/
        // introduction).
        let response: dodopayments::DodopaymentsErrorResponse = res
            .response
            .parse_struct("DodopaymentsErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response
                .code
                .clone()
                .unwrap_or_else(|| "DODOPAYMENTS_ERROR".to_string()),
            message: response
                .message
                .clone()
                .unwrap_or_else(|| "DodoPayments request failed".to_string()),
            reason: response.message,
            attempt_status: None,
            connector_transaction_id: None,
            connector_response_reference_id: None,
            network_advice_code: None,
            network_decline_code: None,
            network_error_message: None,
            connector_metadata: None,
        })
    }
}

impl ConnectorValidation for Dodopayments {
    fn validate_psync_reference_id(
        &self,
        _data: &PaymentsSyncData,
        _is_three_ds: bool,
        _status: enums::AttemptStatus,
        _connector_meta_data: Option<common_utils::pii::SecretSerdeValue>,
    ) -> CustomResult<(), errors::ConnectorError> {
        // PSync reads the checkout session by its `session_id`, which
        // Authorize stores as part of the transaction id — so no separate
        // reference is required to sync.
        Ok(())
    }
}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Dodopayments {
    // Dodo has no session-token flow; the `checkout_url` returned by
    // Authorize is the whole hosted session.
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Dodopayments {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData>
    for Dodopayments
{
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // Dodo's Subscriptions surface (subscription_data on a checkout
        // session) exists, but mandate/setup-mandate as Hyperswitch models it
        // is not a documented single call — not wired rather than guessed.
        Err(errors::ConnectorError::NotImplemented(
            "Setup Mandate flow for Dodopayments".to_string(),
        )
        .into())
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Dodopayments {
    fn get_headers(
        &self,
        req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    // Dodo's recommended collection endpoint: POST /checkouts
    // (docs.dodopayments.com/api-reference/checkout-sessions/create).
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}checkouts", self.base_url(connectors)))
    }

    fn get_request_body(
        &self,
        req: &PaymentsAuthorizeRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let amount = convert_amount(
            self.amount_converter,
            req.request.minor_amount,
            req.request.currency,
        )?;

        let connector_router_data = dodopayments::DodopaymentsRouterData::from((amount, req));
        let connector_req =
            dodopayments::DodopaymentsPaymentsRequest::try_from(&connector_router_data)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Post)
                .url(&PaymentsAuthorizeType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(PaymentsAuthorizeType::get_headers(self, req, connectors)?)
                .set_body(PaymentsAuthorizeType::get_request_body(
                    self, req, connectors,
                )?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PaymentsAuthorizeRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PaymentsAuthorizeRouterData, errors::ConnectorError> {
        let response: dodopayments::DodopaymentsCheckoutSessionResponse = res
            .response
            .parse_struct("Dodopayments PaymentsAuthorizeResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);
        RouterData::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Dodopayments {
    fn get_headers(
        &self,
        req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    // Dodo's session-status lookup: GET /checkouts/{id}. Sync always calls
    // it by the stored connector transaction id (the checkout `session_id`),
    // and the session's `payment_status` gives the real attempt status.
    fn get_url(
        &self,
        req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let connector_id = req
            .request
            .connector_transaction_id
            .get_connector_transaction_id()
            .change_context(errors::ConnectorError::MissingConnectorTransactionID)?;
        Ok(format!(
            "{}checkouts/{}",
            self.base_url(connectors),
            connector_id
        ))
    }

    fn build_request(
        &self,
        req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Get)
                .url(&PaymentsSyncType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(PaymentsSyncType::get_headers(self, req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PaymentsSyncRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PaymentsSyncRouterData, errors::ConnectorError> {
        let response: dodopayments::DodopaymentsCheckoutSessionStatus = res
            .response
            .parse_struct("Dodopayments PaymentsSyncResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);
        RouterData::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

// Dodo's Checkout Sessions are auto-capture, single-step (the customer pays
// the full amount at the hosted page) — no separate authorize-then-capture
// endpoint is exposed, so `FlowNotSupported` rather than guessing one.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Dodopayments {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Dodopayments".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above -- no void/cancel endpoint on this
// surface.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Dodopayments {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Dodopayments".to_string(),
        }
        .into())
    }
}

// Dodo's refunds (POST /refunds) belong to a separate resource family this
// connector's audited contract did not cover — not wired rather than
// guessing at the request/response shape.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Dodopayments {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Refund flow for Dodopayments".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Dodopayments {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Refund flow for Dodopayments".to_string())
                .into(),
        )
    }
}

impl webhooks::IncomingWebhook for Dodopayments {
    // Dodo uses the Standard Webhooks scheme (three headers `webhook-id`/
    // `webhook-timestamp`/`webhook-signature`, base64 HMAC-SHA256 over
    // `${id}.${timestamp}.${raw-body}`) — structurally different from every
    // other connector's single-hex-header scheme. Deferred rather than
    // half-ported; flagged in handover.md as the next follow-up.
    fn get_webhook_object_reference_id(
        &self,
        _request: &webhooks::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<api_models::webhooks::ObjectReferenceId, errors::ConnectorError> {
        Err(error_stack::report!(
            errors::ConnectorError::WebhooksNotImplemented
        ))
    }

    fn get_webhook_event_type(
        &self,
        _request: &webhooks::IncomingWebhookRequestDetails<'_>,
        _context: Option<&webhooks::WebhookContext>,
    ) -> CustomResult<api_models::webhooks::IncomingWebhookEvent, errors::ConnectorError> {
        Err(error_stack::report!(
            errors::ConnectorError::WebhooksNotImplemented
        ))
    }

    fn get_webhook_resource_object(
        &self,
        _request: &webhooks::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<Box<dyn hyperswitch_masking::ErasedMaskSerialize>, errors::ConnectorError>
    {
        Err(error_stack::report!(
            errors::ConnectorError::WebhooksNotImplemented
        ))
    }
}

static DODOPAYMENTS_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> =
    LazyLock::new(|| {
        let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

        let mut dodopayments_supported_payment_methods = SupportedPaymentMethods::new();

        dodopayments_supported_payment_methods.add(
            enums::PaymentMethod::Card,
            enums::PaymentMethodType::Credit,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods: supported_capture_methods.clone(),
                specific_features: None,
            },
        );
        dodopayments_supported_payment_methods.add(
            enums::PaymentMethod::BankTransfer,
            enums::PaymentMethodType::Ach,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods,
                specific_features: None,
            },
        );

        dodopayments_supported_payment_methods
    });

static DODOPAYMENTS_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "DodoPayments",
    description: "DodoPayments is a Merchant-of-Record platform whose recommended collection surface is Checkout Sessions (POST /checkout-sessions).",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static DODOPAYMENTS_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Dodopayments {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&DODOPAYMENTS_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*DODOPAYMENTS_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&DODOPAYMENTS_SUPPORTED_WEBHOOK_FLOWS)
    }
}
