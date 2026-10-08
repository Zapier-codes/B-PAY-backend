// PaymentPoint connector — virtual-account collection surface.
//
// PaymentPoint authenticates with three simultaneous credentials: an
// `Authorization: Bearer {secret key}` header, a separate `api-key` header,
// and a `businessId` field inside the request body (audit a-8). Carried here
// via `SignatureKey` (api_secret = secret key, api_key = API key,
// key1 = business id).
//
// Base URL:  https://api.paymentpoint.co/
// Authorize: POST /api/v1/createVirtualAccount
// PSync:     not implemented (no documented transaction-lookup endpoint)
//
pub mod transformers;

use std::sync::LazyLock;

use common_enums::enums;
use common_utils::{
    errors::CustomResult,
    ext_traits::BytesExt,
    request::{Method, Request, RequestBuilder, RequestContent},
    types::{AmountConvertor, FloatMajorUnit, FloatMajorUnitForConnector},
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
    types::{PaymentsAuthorizeType, Response},
    webhooks,
};
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as paymentpoint;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

// PaymentPoint's virtual-account funding surface — see
// paymentpoint/transformers.rs for the three-credential auth and the
// confirmed request/response contract.
#[derive(Clone)]
pub struct Paymentpoint {
    amount_converter: &'static (dyn AmountConvertor<Output = FloatMajorUnit> + Sync),
}

impl Paymentpoint {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &FloatMajorUnitForConnector,
        }
    }
}

impl api::Payment for Paymentpoint {}
impl api::PaymentSession for Paymentpoint {}
impl api::ConnectorAccessToken for Paymentpoint {}
impl api::MandateSetup for Paymentpoint {}
impl api::PaymentAuthorize for Paymentpoint {}
impl api::PaymentSync for Paymentpoint {}
impl api::PaymentCapture for Paymentpoint {}
impl api::PaymentVoid for Paymentpoint {}
impl api::Refund for Paymentpoint {}
impl api::RefundExecute for Paymentpoint {}
impl api::RefundSync for Paymentpoint {}
impl api::PaymentToken for Paymentpoint {}

// Paymentpoint's Checkout Solutions surface has no payout flows at all (Task 50
// covers collection only) -- deliberately no `impl api::Payouts for Paymentpoint`,
// so this crate's own `default_imp_for_payouts*!` macros keep supplying
// Paymentpoint's no-op default for every payout flow, exactly as they already do
// for Flutterwave (the other Authorize+PSync-only Task 77 connector).

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Paymentpoint
{
    // Not Implemented (R) — Paymentpoint's `payment/charge` endpoint takes the
    // full request at Authorize time and hosts card entry itself; there is
    // no separate tokenization step on this surface.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Paymentpoint
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

impl ConnectorCommon for Paymentpoint {
    fn id(&self) -> &'static str {
        "paymentpoint"
    }

    // PaymentPoint amounts are whole Naira base units (webhook example:
    // amount_paid 100 == ₦100; audit a-8).
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Base
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.paymentpoint.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = paymentpoint::PaymentpointAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        // PaymentPoint requires BOTH the Bearer secret and the api-key header;
        // the third credential (`businessId`) is added to the request body in
        // the transformers.
        Ok(vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", auth.secret_key.expose()).into_masked(),
            ),
            ("api-key".to_string(), auth.api_key.expose().into_masked()),
        ])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: paymentpoint::PaymentpointErrorResponse = res
            .response
            .parse_struct("PaymentpointErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response
                .status
                .clone()
                .unwrap_or_else(|| "PAYMENTPOINT_ERROR".to_string()),
            message: response
                .message
                .clone()
                .unwrap_or_else(|| "PaymentPoint request failed".to_string()),
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

impl ConnectorValidation for Paymentpoint {
    fn validate_psync_reference_id(
        &self,
        _data: &PaymentsSyncData,
        _is_three_ds: bool,
        _status: enums::AttemptStatus,
        _connector_meta_data: Option<common_utils::pii::SecretSerdeValue>,
    ) -> CustomResult<(), errors::ConnectorError> {
        // PaymentPoint has no documented status-lookup endpoint at all (see
        // the PSync impl) — nothing to validate against.
        Ok(())
    }
}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Paymentpoint {
    // PaymentPoint has no session-token flow; the provisioned virtual account
    // is the whole "session".
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Paymentpoint {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData>
    for Paymentpoint
{
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // PaymentPoint has no mandate/recurring-charge API — not wired rather
        // than guessed.
        Err(errors::ConnectorError::NotImplemented(
            "Setup Mandate flow for Paymentpoint".to_string(),
        )
        .into())
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Paymentpoint {
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

    // PaymentPoint's only documented collection-adjacent endpoint:
    // POST /api/v1/createVirtualAccount (audit a-8).
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!(
            "{}api/v1/createVirtualAccount",
            self.base_url(connectors)
        ))
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

        let connector_router_data = paymentpoint::PaymentpointRouterData::from((amount, req));
        let connector_req =
            paymentpoint::PaymentpointVirtualAccountRequest::try_from(&connector_router_data)?;
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
        let response: paymentpoint::PaymentpointVirtualAccountResponse = res
            .response
            .parse_struct("Paymentpoint PaymentsAuthorizeResponse")
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

// PaymentPoint exposes no documented status lookup for a virtual account /
// funded payment (its other audited surfaces are identity/liveness
// verification), and its `payment/charge` endpoint is not documented. Sync,
// capture, void and refunds are therefore left unimplemented rather than
// pointed at a guessed path; the collectible state is observed via webhook.
impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Paymentpoint {
    fn build_request(
        &self,
        _req: &PaymentsSyncRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Sync flow for Paymentpoint".to_string()).into())
    }
}

// Virtual-account funding is a single-step inbound transfer — no separate
// authorize-then-capture step and no documented capture endpoint, so
// `FlowNotSupported` rather than guessing one.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Paymentpoint {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Paymentpoint".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above -- no void/cancel endpoint on this
// surface.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Paymentpoint {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Paymentpoint".to_string(),
        }
        .into())
    }
}

// No refund method is documented on the virtual-account surface — not wired
// rather than guessing at the request/response shape.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Paymentpoint {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Refund flow for Paymentpoint".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Paymentpoint {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Refund flow for Paymentpoint".to_string())
                .into(),
        )
    }
}

impl webhooks::IncomingWebhook for Paymentpoint {
    // PaymentPoint's webhook signature is confirmed: a `Paymentpoint-Signature`
    // header carrying HMAC-SHA256(raw body, secret key), hex — the same
    // algorithm/encoding as Korapay's verifier. No timestamp/nonce (no replay
    // protection — a provider-side gap, mitigated by this repo's dedupe).
    // Payload fields (notification_status, transaction_id,
    // transaction_status, amount_paid, ...) are confirmed; parsing into this
    // framework's domain types is deferred as WebhooksNotImplemented,
    // matching Korapay/DodoPayments' own deferral.
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

static PAYMENTPOINT_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> =
    LazyLock::new(|| {
        let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

        let mut paymentpoint_supported_payment_methods = SupportedPaymentMethods::new();

        paymentpoint_supported_payment_methods.add(
            enums::PaymentMethod::Card,
            enums::PaymentMethodType::Credit,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods: supported_capture_methods.clone(),
                specific_features: None,
            },
        );
        paymentpoint_supported_payment_methods.add(
            enums::PaymentMethod::BankTransfer,
            enums::PaymentMethodType::Ach,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods,
                specific_features: None,
            },
        );

        paymentpoint_supported_payment_methods
    });

static PAYMENTPOINT_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "PaymentPoint",
    description: "PaymentPoint is a Nigerian payments platform; its documented collection-adjacent surface is virtual-account creation plus wallet billing.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static PAYMENTPOINT_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Paymentpoint {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&PAYMENTPOINT_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*PAYMENTPOINT_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&PAYMENTPOINT_SUPPORTED_WEBHOOK_FLOWS)
    }
}
