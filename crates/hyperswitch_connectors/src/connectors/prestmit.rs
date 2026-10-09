// Prestmit connector — gift-card SELL trade (a collection-shaped flow onto a
// gift-card/crypto off-ramp; Prestmit has no bank/card "charge" primitive).
//
// Auth is not a plain bearer key: every request carries an `API-KEY` header
// AND an `API-Hash` header = HMAC-SHA256(`{API_KEY}:{json_body}`, API_SECRET)
// hex. The hash signs the exact serialized body, so this connector builds it
// at request time from the same bytes `RequestContent::Json` sends — see
// `build_headers`. Credentials are carried via `SignatureKey`
// (api_key = API_KEY, api_secret = API_SECRET, key1 = account PIN).
//
// Base URL:  https://dev-api.prestmit.io/
// Authorize: POST /partners/v1/giftcard-trade/sell/create
// PSync:     GET  /partners/v1/giftcard-trade/sell/history?referenceOrID={ref}
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
    router_data::{AccessToken, ErrorResponse, RouterData},
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
use ring::hmac;
use transformers as prestmit;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

// Prestmit's gift-card sell surface — see prestmit/transformers.rs for the
// confirmed request/response contract.
#[derive(Clone)]
pub struct Prestmit {
    amount_converter: &'static (dyn AmountConvertor<Output = MinorUnit> + Sync),
}

impl Prestmit {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &MinorUnitForConnector,
        }
    }
}

impl api::Payment for Prestmit {}
impl api::PaymentSession for Prestmit {}
impl api::ConnectorAccessToken for Prestmit {}
impl api::MandateSetup for Prestmit {}
impl api::PaymentAuthorize for Prestmit {}
impl api::PaymentSync for Prestmit {}
impl api::PaymentCapture for Prestmit {}
impl api::PaymentVoid for Prestmit {}
impl api::Refund for Prestmit {}
impl api::RefundExecute for Prestmit {}
impl api::RefundSync for Prestmit {}
impl api::PaymentToken for Prestmit {}

// Prestmit's gift-card sell surface has no payout flows exposed by this
// connector -- deliberately no `impl api::Payouts for Prestmit`, so this
// crate's own `default_imp_for_payouts*!` macros keep supplying Prestmit's
// no-op default for every payout flow.

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Prestmit
{
    // Not Implemented (R) — Prestmit's sell-trade endpoint takes the full
    // request at Authorize time; there is no separate tokenization step.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Prestmit
where
    Self: ConnectorIntegration<Flow, Request, Response>,
{
    fn build_headers(
        &self,
        req: &RouterData<Flow, Request, Response>,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = prestmit::PrestmitAuthType::try_from(&req.connector_auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        // `API-Hash` signs the exact serialized body that will be sent, so it
        // is computed here from `get_request_body`'s own output (the same
        // string `RequestContent::Json` serializes into the request), not
        // from a separately re-serialized copy.
        let request_payload = self
            .get_request_body(req, connectors)?
            .get_inner_value()
            .expose();
        let api_key = auth.api_key.expose();
        let payload = format!("{}:{}", api_key, request_payload);
        let key = hmac::Key::new(hmac::HMAC_SHA256, auth.api_secret.expose().as_bytes());
        let tag = hmac::sign(&key, payload.as_bytes());
        let api_hash = hex::encode(tag);
        Ok(vec![
            (
                headers::CONTENT_TYPE.to_string(),
                self.get_content_type().to_string().into(),
            ),
            ("API-KEY".to_string(), api_key.into_masked()),
            ("API-Hash".to_string(), api_hash.into_masked()),
        ])
    }
}

impl ConnectorCommon for Prestmit {
    fn id(&self) -> &'static str {
        "prestmit"
    }

    // Prestmit's sell `amount` is an integer (gift-card face value) and the
    // Hyperswitch request amount is passed through as-is in minor units.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Minor
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.prestmit.base_url.as_ref()
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: prestmit::PrestmitErrorResponse = res
            .response
            .parse_struct("PrestmitErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: "PRESTMIT_ERROR".to_string(),
            message: response
                .message
                .clone()
                .unwrap_or_else(|| "Prestmit request failed".to_string()),
            reason: response.errors.map(|errors| errors.to_string()),
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

// PSync filters Prestmit's sell history by the stored trade reference, so the
// framework default (which requires a `connector_transaction_id`) is the
// correct validation posture — no override needed.
impl ConnectorValidation for Prestmit {}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Prestmit {
    // Prestmit has no session-token flow; a sell trade is created directly.
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Prestmit {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData>
    for Prestmit
{
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // Prestmit has no mandate/recurring-charge API -- not wired rather
        // than guessed.
        Err(
            errors::ConnectorError::NotImplemented("Setup Mandate flow for Prestmit".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Prestmit {
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

    // Prestmit's gift-card sell-trade creation endpoint.
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!(
            "{}partners/v1/giftcard-trade/sell/create",
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

        let connector_router_data = prestmit::PrestmitRouterData::from((amount, req));
        let connector_req = prestmit::PrestmitSellTradeRequest::try_from(&connector_router_data)?;
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
        let response: prestmit::PrestmitSellTradeResponse = res
            .response
            .parse_struct("Prestmit PaymentsAuthorizeResponse")
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

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Prestmit {
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

    // Prestmit has no single-trade GET; a specific trade is fetched from the
    // paginated sell history using Prestmit's own `referenceOrID` filter.
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
            "{}partners/v1/giftcard-trade/sell/history?referenceOrID={}",
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
        let response: prestmit::PrestmitSellHistoryResponse = res
            .response
            .parse_struct("Prestmit PaymentsSyncResponse")
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

// A gift-card sell trade is a single-step submission with no separate
// authorize-then-capture endpoint -- `FlowNotSupported` rather than guessing
// at an endpoint.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Prestmit {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Prestmit".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above -- no void/cancel endpoint on this
// surface.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Prestmit {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Prestmit".to_string(),
        }
        .into())
    }
}

// Prestmit exposes no refund method on the sell-trade surface -- not wired
// rather than guessing at the request/response shape.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Prestmit {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Prestmit".to_string()).into())
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Prestmit {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Prestmit".to_string()).into())
    }
}

impl webhooks::IncomingWebhook for Prestmit {
    // Prestmit's webhook signature scheme is confirmed: an
    // `x-prestmit-signature` header carrying HMAC-SHA256(raw body, API_SECRET)
    // BASE64-encoded (a real difference from the hex scheme every other
    // provider here uses). Payload parsing into this framework's domain types
    // is deferred as WebhooksNotImplemented, matching Korapay/DodoPayments.
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

static PRESTMIT_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> =
    LazyLock::new(|| {
        let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

        let mut prestmit_supported_payment_methods = SupportedPaymentMethods::new();

        prestmit_supported_payment_methods.add(
            enums::PaymentMethod::Card,
            enums::PaymentMethodType::Credit,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods: supported_capture_methods.clone(),
                specific_features: None,
            },
        );
        prestmit_supported_payment_methods.add(
            enums::PaymentMethod::BankTransfer,
            enums::PaymentMethodType::Ach,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods,
                specific_features: None,
            },
        );

        prestmit_supported_payment_methods
    });

static PRESTMIT_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "Prestmit",
    description: "Prestmit is a gift-card / crypto off-ramp trading platform; it has no charge-a-customer endpoint, so Authorize maps onto a gift-card SELL trade (POST /partners/v1/giftcard-trade/sell/create) with PSync by reference.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static PRESTMIT_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Prestmit {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&PRESTMIT_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*PRESTMIT_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&PRESTMIT_SUPPORTED_WEBHOOK_FLOWS)
    }
}
