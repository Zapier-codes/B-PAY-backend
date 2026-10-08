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
    types::{PaymentsAuthorizeType, PaymentsSyncType, Response},
    webhooks,
};
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as remita;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

// Remita's "Accept Online Payments" (Checkout Solutions) surface, per
// Task 50/c -- see remita/transformers.rs's own header comment for why this
// surface (not the classic RRR flow) is the one implemented, and for the
// three fields/paths this connector had to flag rather than settle
// (amount unit, the charge endpoint's prose-vs-curl path inconsistency, and
// the PSync verify response shape). Base URL and path are taken directly
// from Task 50/c's real worked example:
// `https://api-demo.systemspecsng.com/services/connect-gateway/api/v1/...`.
#[derive(Clone)]
pub struct Remita {
    amount_converter: &'static (dyn AmountConvertor<Output = FloatMajorUnit> + Sync),
}

impl Remita {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &FloatMajorUnitForConnector,
        }
    }
}

impl api::Payment for Remita {}
impl api::PaymentSession for Remita {}
impl api::ConnectorAccessToken for Remita {}
impl api::MandateSetup for Remita {}
impl api::PaymentAuthorize for Remita {}
impl api::PaymentSync for Remita {}
impl api::PaymentCapture for Remita {}
impl api::PaymentVoid for Remita {}
impl api::Refund for Remita {}
impl api::RefundExecute for Remita {}
impl api::RefundSync for Remita {}
impl api::PaymentToken for Remita {}

// Remita's Checkout Solutions surface has no payout flows at all (Task 50
// covers collection only) -- deliberately no `impl api::Payouts for Remita`,
// so this crate's own `default_imp_for_payouts*!` macros keep supplying
// Remita's no-op default for every payout flow, exactly as they already do
// for Flutterwave (the other Authorize+PSync-only Task 77 connector).

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Remita
{
    // Not Implemented (R) — Remita's `payment/charge` endpoint takes the
    // full request at Authorize time and hosts card entry itself; there is
    // no separate tokenization step on this surface.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Remita
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

impl ConnectorCommon for Remita {
    fn id(&self) -> &'static str {
        "remita"
    }

    // Unconfirmed by any primary source -- see remita/transformers.rs's
    // RemitaRouterData comment. Base (whole-Naira) is chosen to match
    // Korapay/Paystack on the same NGN rails; flagged for a live-call
    // confirmation before production use.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Base
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.remita.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = remita::RemitaAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        // Remita's Checkout Solutions surface uses a flat `secretKey`
        // header, NOT `Authorization: Bearer` -- Task 50/c confirms this
        // explicitly.
        Ok(vec![(
            "secretKey".to_string(),
            auth.secret_key.expose().into_masked(),
        )])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: remita::RemitaPaymentsResponse = res
            .response
            .parse_struct("RemitaErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response.status.clone(),
            message: response.message.clone(),
            reason: Some(response.message),
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

impl ConnectorValidation for Remita {
    fn validate_psync_reference_id(
        &self,
        _data: &PaymentsSyncData,
        _is_three_ds: bool,
        _status: enums::AttemptStatus,
        _connector_meta_data: Option<common_utils::pii::SecretSerdeValue>,
    ) -> CustomResult<(), errors::ConnectorError> {
        // Task 50/c confirms the verify call is made by the merchant's own
        // `paymentIdentifier` reference, so a connector_transaction_id is
        // not required to sync -- same posture as Korapay's own override.
        Ok(())
    }
}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Remita {
    // Remita has no session-token flow -- the `paymentLink` returned by
    // Authorize is Remita's whole "session".
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Remita {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData> for Remita {
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // No mandate/recurring-charge API observed anywhere in Task 50's
        // supplied material -- not wired rather than guessed.
        Err(
            errors::ConnectorError::NotImplemented("Setup Mandate flow for Remita".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Remita {
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

    // Task 50/c's prose path (the same doc's own curl example shows
    // `payment-engine/payment/charge` instead -- see remita/transformers.rs's
    // own note; flagged for live confirmation).
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!(
            "{}services/connect-gateway/api/v1/payment/charge",
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

        let connector_router_data = remita::RemitaRouterData::from((amount, req));
        let connector_req = remita::RemitaPaymentsRequest::try_from(&connector_router_data)?;
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
        let response: remita::RemitaPaymentsResponse = res
            .response
            .parse_struct("Remita PaymentsAuthorizeResponse")
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

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Remita {
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

    // Task 50/c: `GET .../payment/merchant/verify/{{transRef}}`, same
    // `secretKey` header, verifiable by the merchant's own
    // `paymentIdentifier` reference -- a real advantage over JuicyWay, whose
    // confirmed gap is having no way to verify by reference alone.
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
            "{}services/connect-gateway/api/v1/payment/merchant/verify/{}",
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
        let response: remita::RemitaPaymentsResponse = res
            .response
            .parse_struct("Remita PaymentsSyncResponse")
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

// Remita's `payment/charge` is a hosted-checkout, single-step flow (no
// separate authorize-then-capture endpoint on the Checkout Solutions
// surface) -- `FlowNotSupported` rather than guessing at an endpoint, same
// position as Korapay/Opennode elsewhere in this crate.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Remita {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Remita".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above -- no void/cancel endpoint on this
// surface.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Remita {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Remita".to_string(),
        }
        .into())
    }
}

// No refund method is documented on the Checkout Solutions surface (Task 50
// covers collection only) -- not wired rather than guessing at an
// unconfirmed `/refunds` endpoint.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Remita {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Remita".to_string()).into())
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Remita {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Remita".to_string()).into())
    }
}

impl webhooks::IncomingWebhook for Remita {
    // Remita's Checkout Solutions webhook signature scheme is not documented
    // in any supplied source (Task 50 covers the request/verify API only) --
    // left as WebhooksNotImplemented and flagged in handover.md as the next
    // natural follow-up, rather than half-ported here.
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

static REMITA_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> = LazyLock::new(|| {
    let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

    let mut remita_supported_payment_methods = SupportedPaymentMethods::new();

    remita_supported_payment_methods.add(
        enums::PaymentMethod::Card,
        enums::PaymentMethodType::Credit,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods: supported_capture_methods.clone(),
            specific_features: None,
        },
    );
    remita_supported_payment_methods.add(
        enums::PaymentMethod::BankTransfer,
        enums::PaymentMethodType::Ach,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods,
            specific_features: None,
        },
    );

    remita_supported_payment_methods
});

static REMITA_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "Remita",
    description: "Remita is a SystemSpecs-owned Nigerian payment gateway offering card and bank-transfer collection through its Accept Online Payments (Checkout Solutions) surface.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static REMITA_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Remita {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&REMITA_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*REMITA_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&REMITA_SUPPORTED_WEBHOOK_FLOWS)
    }
}
