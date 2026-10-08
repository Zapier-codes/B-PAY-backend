// telcos.opik.net (opik) connector — VTU (airtime/data) rail.
//
// Unlike the hosted-checkout providers in this crate, opik is a direct VTU
// purchase API (register a business account, fund the wallet, buy
// airtime/data for a phone number). Authorize maps onto
// `POST /api/v1/purchase/airtime`; PSync reconciles through
// `GET /api/v1/transactions` by reference (opik has no single-transaction
// lookup endpoint). See opik/transformers.rs for the full contract and the
// items this connector still flags as unconfirmed.
//
// Base URL: https://telco.opik.net/
// Auth:     raw `X-API-Key` header (no `Bearer` prefix)
// Authorize: POST api/v1/purchase/airtime
// PSync:     GET  api/v1/transactions
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
    types::{PaymentsAuthorizeType, PaymentsSyncType, Response},
    webhooks,
};
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as opik;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

// opik's VTU rail — see opik/transformers.rs for the full purchase and
// transaction contract. Collection-only; no payout flows.
#[derive(Clone)]
pub struct Opik {
    amount_converter: &'static (dyn AmountConvertor<Output = FloatMajorUnit> + Sync),
}

impl Opik {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &FloatMajorUnitForConnector,
        }
    }
}

impl api::Payment for Opik {}
impl api::PaymentSession for Opik {}
impl api::ConnectorAccessToken for Opik {}
impl api::MandateSetup for Opik {}
impl api::PaymentAuthorize for Opik {}
impl api::PaymentSync for Opik {}
impl api::PaymentCapture for Opik {}
impl api::PaymentVoid for Opik {}
impl api::Refund for Opik {}
impl api::RefundExecute for Opik {}
impl api::RefundSync for Opik {}
impl api::PaymentToken for Opik {}

// opik's VTU rail has no payout/bank-transfer surface in its audited
// contract (docs/guides/ covers plans, wallet, purchases, transactions and
// webhooks only) — deliberately no `impl api::Payouts for Opik`, so this
// crate's own `default_imp_for_payouts*!` macros keep supplying Opik's
// no-op default for every payout flow, exactly as they already do for
// Flutterwave (the other Authorize+PSync-only Task 77 connector).

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Opik
{
    // Not Implemented (R) — opik has no tokenization step; the purchase
    // endpoint takes the network + phone number + amount directly.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Opik
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

impl ConnectorCommon for Opik {
    fn id(&self) -> &'static str {
        "opik"
    }

    // opik's own transaction example (`amount: 100` for NGN 100,
    // docs/guides/06) uses whole Naira, not kobo — Base unit, matching
    // Korapay/Paystack on the same rails.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Base
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.opik.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = opik::OpikAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        // Confirmed 2026-09-09 directly against the live Swagger UI's
        // "Available authorizations" modal: raw `X-API-Key`, no `Bearer`
        // prefix (docs/guides/02-authentication.md,
        // docs/openapi/components/schemas.yaml#/securitySchemes/apiKeyAuth).
        Ok(vec![(
            "X-API-Key".to_string(),
            auth.api_key.expose().into_masked(),
        )])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        // opik's real error envelope was not captured from the live server
        // (docs/guides/09, item #2) — the shared `{ success, message }`
        // shape is what its audit documents as the placeholder, so it is
        // parsed tolerantly here rather than as a hard-required struct.
        let response: opik::OpikErrorResponse = res
            .response
            .parse_struct("OpikErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: "OPIK_ERROR".to_string(),
            message: response
                .message
                .clone()
                .unwrap_or_else(|| "opik request failed".to_string()),
            reason: response.message.clone(),
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

impl ConnectorValidation for Opik {
    fn validate_psync_reference_id(
        &self,
        _data: &PaymentsSyncData,
        _is_three_ds: bool,
        _status: enums::AttemptStatus,
        _connector_meta_data: Option<common_utils::pii::SecretSerdeValue>,
    ) -> CustomResult<(), errors::ConnectorError> {
        // Sync is keyed off the stored connector transaction id (opik's
        // `reference`), so nothing extra to validate.
        Ok(())
    }
}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Opik {
    // opik has no session-token flow; the airtime purchase is a direct call.
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Opik {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData> for Opik {
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // opik has no mandate/recurring-charge API — not wired rather than
        // guessed.
        Err(
            errors::ConnectorError::NotImplemented("Setup Mandate flow for Opik".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Opik {
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

    // opik's airtime purchase endpoint (docs/guides/05-purchasing-data-
    // airtime.md). Chosen over `/purchase/data` because data bundles need a
    // pre-provisioned `planId`, which the generic payment request cannot
    // carry — see opik/transformers.rs's own note.
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!(
            "{}api/v1/purchase/airtime",
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

        let connector_router_data = opik::OpikRouterData::from((amount, req));
        let connector_req = opik::OpikAirtimePurchaseRequest::try_from(&connector_router_data)?;
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
        let response: opik::OpikPurchaseResponse = res
            .response
            .parse_struct("Opik PaymentsAuthorizeResponse")
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

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Opik {
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

    // opik has no single-transaction lookup; `GET /api/v1/transactions`
    // returns the business's history and the connector reconciles by
    // reference (docs/guides/06-transactions.md). `req` is unused because
    // the reference is matched client-side in the response mapping.
    fn get_url(
        &self,
        _req: &PaymentsSyncRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}api/v1/transactions", self.base_url(connectors)))
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
        let response: opik::OpikTransactionsResponse = res
            .response
            .parse_struct("Opik PaymentsSyncResponse")
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

// opik's airtime purchase is a single-step direct debit — there is no
// separate authorize-then-capture step and no capture endpoint, so
// `FlowNotSupported` rather than guessing one.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Opik {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Opik".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above -- no void/cancel endpoint on this
// surface.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Opik {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Opik".to_string(),
        }
        .into())
    }
}

// No refund method is documented on opik's VTU surface — not wired rather
// than guessing at the request/response shape.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Opik {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Opik".to_string()).into())
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Opik {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Opik".to_string()).into())
    }
}

impl webhooks::IncomingWebhook for Opik {
    // opik's webhook signing scheme was not captured in this repo's own
    // audit (the port's conventions-and-open-items notes,
    // item #1) — left as WebhooksNotImplemented rather than half-ported,
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

static OPIK_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> = LazyLock::new(|| {
    let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

    let mut opik_supported_payment_methods = SupportedPaymentMethods::new();

    opik_supported_payment_methods.add(
        enums::PaymentMethod::Card,
        enums::PaymentMethodType::Credit,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods: supported_capture_methods.clone(),
            specific_features: None,
        },
    );
    opik_supported_payment_methods.add(
        enums::PaymentMethod::BankTransfer,
        enums::PaymentMethodType::Ach,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods,
            specific_features: None,
        },
    );

    opik_supported_payment_methods
});

static OPIK_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "telcos.opik.net",
    description: "telcos.opik.net is an operator-owned VTU (airtime/data) rail; purchase is via POST /api/v1/purchase/airtime and PSync reconciles through GET /api/v1/transactions.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static OPIK_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Opik {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&OPIK_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*OPIK_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&OPIK_SUPPORTED_WEBHOOK_FLOWS)
    }
}
