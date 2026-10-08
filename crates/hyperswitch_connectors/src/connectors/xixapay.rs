// Xixapay connector — virtual-account collection surface.
//
// Xixapay authenticates with three simultaneous credentials: an
// `Authorization: Bearer {secret key}` header, a separate `api-key` header,
// and a `businessId` field inside the request body (audit a-7). This
// connector carries them via `SignatureKey` (api_secret = secret key,
// api_key = API key, key1 = business id).
//
// Base URL:  https://api.xixapay.com/
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
use transformers as xixapay;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

// Xixapay's virtual-account funding surface — see xixapay/transformers.rs for
// the three-credential auth and the confirmed request/response contract.
#[derive(Clone)]
pub struct Xixapay {
    amount_converter: &'static (dyn AmountConvertor<Output = FloatMajorUnit> + Sync),
}

impl Xixapay {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &FloatMajorUnitForConnector,
        }
    }
}

impl api::Payment for Xixapay {}
impl api::PaymentSession for Xixapay {}
impl api::ConnectorAccessToken for Xixapay {}
impl api::MandateSetup for Xixapay {}
impl api::PaymentAuthorize for Xixapay {}
impl api::PaymentSync for Xixapay {}
impl api::PaymentCapture for Xixapay {}
impl api::PaymentVoid for Xixapay {}
impl api::Refund for Xixapay {}
impl api::RefundExecute for Xixapay {}
impl api::RefundSync for Xixapay {}
impl api::PaymentToken for Xixapay {}

// Xixapay's Checkout Solutions surface has no payout flows at all (Task 50
// covers collection only) -- deliberately no `impl api::Payouts for Xixapay`,
// so this crate's own `default_imp_for_payouts*!` macros keep supplying
// Xixapay's no-op default for every payout flow, exactly as they already do
// for Flutterwave (the other Authorize+PSync-only Task 77 connector).

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Xixapay
{
    // Not Implemented (R) — Xixapay's `payment/charge` endpoint takes the
    // full request at Authorize time and hosts card entry itself; there is
    // no separate tokenization step on this surface.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Xixapay
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

impl ConnectorCommon for Xixapay {
    fn id(&self) -> &'static str {
        "xixapay"
    }

    // Xixapay amounts are whole Naira base units, NGN-only (no currency field
    // anywhere in Virtual Account / Payout; audit a-7).
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Base
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.xixapay.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = xixapay::XixapayAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        // Xixapay requires BOTH the Bearer secret and the api-key header; the
        // third credential (`businessId`) is added to the request body in the
        // transformers.
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
        let response: xixapay::XixapayErrorResponse = res
            .response
            .parse_struct("XixapayErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response
                .status
                .clone()
                .unwrap_or_else(|| "XIXAPAY_ERROR".to_string()),
            message: response
                .message
                .clone()
                .unwrap_or_else(|| "Xixapay request failed".to_string()),
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

impl ConnectorValidation for Xixapay {
    fn validate_psync_reference_id(
        &self,
        _data: &PaymentsSyncData,
        _is_three_ds: bool,
        _status: enums::AttemptStatus,
        _connector_meta_data: Option<common_utils::pii::SecretSerdeValue>,
    ) -> CustomResult<(), errors::ConnectorError> {
        // Xixapay has no documented status-lookup endpoint at all (see the
        // PSync impl) — nothing to validate against.
        Ok(())
    }
}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Xixapay {
    // Xixapay has no session-token flow; the provisioned virtual account is
    // the whole "session".
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Xixapay {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData> for Xixapay {
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // Xixapay has no mandate/recurring-charge API — not wired rather than
        // guessed.
        Err(
            errors::ConnectorError::NotImplemented("Setup Mandate flow for Xixapay".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Xixapay {
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

    // Xixapay's only documented collection-adjacent endpoint:
    // POST /api/v1/createVirtualAccount (audit a-7).
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

        let connector_router_data = xixapay::XixapayRouterData::from((amount, req));
        let connector_req =
            xixapay::XixapayVirtualAccountRequest::try_from(&connector_router_data)?;
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
        let response: xixapay::XixapayVirtualAccountResponse = res
            .response
            .parse_struct("Xixapay PaymentsAuthorizeResponse")
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

// Xixapay exposes no documented status lookup for a virtual account / funded
// payment (its Payout/Verify pages are payout-side only), and its
// `payment/charge` endpoint is not documented. Sync, capture, void and
// refunds are therefore left unimplemented rather than pointed at a guessed
// path. The collectible state is observed via webhook instead.
impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Xixapay {
    fn build_request(
        &self,
        _req: &PaymentsSyncRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Sync flow for Xixapay".to_string()).into())
    }
}

// Virtual-account funding is a single-step inbound transfer — there is no
// separate authorize-then-capture step, and no capture endpoint is
// documented, so `FlowNotSupported` rather than guessing one.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Xixapay {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Xixapay".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above -- no void/cancel endpoint on this
// surface.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Xixapay {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Xixapay".to_string(),
        }
        .into())
    }
}

// Xixapay's refunds are not documented on the virtual-account surface — not
// wired rather than guessing at the request/response shape.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Xixapay {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Xixapay".to_string()).into())
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Xixapay {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Xixapay".to_string()).into())
    }
}

impl webhooks::IncomingWebhook for Xixapay {
    // Xixapay's webhook signature scheme is confirmed on the Virtual/Dynamic
    // Account page: a raw `xixapay` header carrying HMAC-SHA256(raw body,
    // secret key), hex, with NO timestamp/nonce (no replay protection — a
    // provider-side gap). Payload fields (notification_status,
    // transaction_id, transaction_status, amount_paid, ...) are confirmed
    // too. Payload parsing into this framework's domain types is left as
    // WebhooksNotImplemented, matching Korapay/DodoPayments' own deferral,
    // rather than half-ported here.
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

static XIXAPAY_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> = LazyLock::new(|| {
    let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

    let mut xixapay_supported_payment_methods = SupportedPaymentMethods::new();

    xixapay_supported_payment_methods.add(
        enums::PaymentMethod::Card,
        enums::PaymentMethodType::Credit,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods: supported_capture_methods.clone(),
            specific_features: None,
        },
    );
    xixapay_supported_payment_methods.add(
        enums::PaymentMethod::BankTransfer,
        enums::PaymentMethodType::Ach,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods,
            specific_features: None,
        },
    );

    xixapay_supported_payment_methods
});

static XIXAPAY_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "Xixapay",
    description: "Xixapay is a Nigerian payments platform authenticating with three simultaneous credentials (Bearer secret, api-key header, businessId body).",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static XIXAPAY_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Xixapay {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&XIXAPAY_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*XIXAPAY_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&XIXAPAY_SUPPORTED_WEBHOOK_FLOWS)
    }
}
