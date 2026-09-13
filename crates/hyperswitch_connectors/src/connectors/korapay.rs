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
#[cfg(feature = "payouts")]
use hyperswitch_domain_models::{
    router_flow_types::{PoFulfill, PoSync},
    types::{PayoutsData, PayoutsResponseData, PayoutsRouterData},
};
use hyperswitch_interfaces::{
    api::{
        self, ConnectorCommon, ConnectorCommonExt, ConnectorIntegration, ConnectorSpecifications,
        ConnectorValidation,
    },
    configs::Connectors,
    consts, errors,
    events::connector_api_logs::ConnectorEvent,
    types::{PaymentsAuthorizeType, PaymentsSyncType, Response},
    webhooks,
};
#[cfg(feature = "payouts")]
use hyperswitch_interfaces::types::{PayoutFulfillType, PayoutSyncType};
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as korapay;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

#[derive(Clone)]
pub struct Korapay {
    amount_converter: &'static (dyn AmountConvertor<Output = FloatMajorUnit> + Sync),
}

impl Korapay {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &FloatMajorUnitForConnector,
        }
    }
}

impl api::Payment for Korapay {}
impl api::PaymentSession for Korapay {}
impl api::ConnectorAccessToken for Korapay {}
impl api::MandateSetup for Korapay {}
impl api::PaymentAuthorize for Korapay {}
impl api::PaymentSync for Korapay {}
impl api::PaymentCapture for Korapay {}
impl api::PaymentVoid for Korapay {}
impl api::Refund for Korapay {}
impl api::RefundExecute for Korapay {}
impl api::RefundSync for Korapay {}
impl api::PaymentToken for Korapay {}

// Task 77/a-1-iii — Korapay payout flows. `api::Payouts` itself (the
// supertrait requiring every payout flow at once) is only real when the
// `payouts` cargo feature is on -- see hyperswitch_interfaces::api::payouts,
// same split Wise's connector already follows. Only `PayoutFulfill`/
// `PayoutSync` are implemented for real below, per Task 42's
// already-confirmed request/response shapes. Create/Cancel/Eligibility/
// Quote/Recipient/RecipientAccount are deliberately left on this crate's own
// `default_imp_for_payouts_*!` macros (Korapay was removed ONLY from the
// fulfill/retrieve macro lists in default_implementations.rs, so those
// still supply Korapay's no-op default for every other payout flow) --
// each would need its own, separately-confirmed Korapay API shape before
// being built for real, same "confirm before wiring" posture as the rest
// of this connector.
impl api::Payouts for Korapay {}
#[cfg(feature = "payouts")]
impl api::PayoutFulfill for Korapay {}
#[cfg(feature = "payouts")]
impl api::PayoutSync for Korapay {}

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Korapay
{
    // Not Implemented (R) — Korapay has no separate tokenization step; the
    // one `charges/initialize` endpoint takes the full request at Authorize
    // time (see transformers.rs's own note on this).
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Korapay
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

impl ConnectorCommon for Korapay {
    fn id(&self) -> &'static str {
        "korapay"
    }

    // Confirmed against legacy-node/providers/korapay.js (Task 7's own
    // comment: "Korapay wants base currency units, not subunits") — this is
    // the one currency-unit fact this connector carries over directly from
    // the legacy integration rather than from Korapay's public docs alone.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Base
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.korapay.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = korapay::KorapayAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        Ok(vec![(
            headers::AUTHORIZATION.to_string(),
            format!("Bearer {}", auth.api_key.expose()).into_masked(),
        )])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: korapay::KorapayErrorResponse = res
            .response
            .parse_struct("KorapayErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: consts::NO_ERROR_CODE.to_string(),
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

impl ConnectorValidation for Korapay {
    fn validate_psync_reference_id(
        &self,
        _data: &PaymentsSyncData,
        _is_three_ds: bool,
        _status: enums::AttemptStatus,
        _connector_meta_data: Option<common_utils::pii::SecretSerdeValue>,
    ) -> CustomResult<(), errors::ConnectorError> {
        Ok(())
    }
}

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Korapay {
    // Korapay has no session-token flow — the checkout URL returned by
    // Authorize (see transformers.rs) is Korapay's whole "session".
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Korapay {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData>
    for Korapay
{
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // No mandate/recurring-charge API observed anywhere in
        // legacy-node/providers/korapay.js — not wired rather than guessed.
        Err(errors::ConnectorError::NotImplemented(
            "Setup Mandate flow for Korapay".to_string(),
        )
        .into())
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Korapay {
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

    // Real endpoint per Korapay's own docs, confirmed against
    // legacy-node/providers/korapay.js's own comment: previously
    // `/transactions/charge`, which does not exist on Korapay's API — this
    // connector starts from the corrected path directly.
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!(
            "{}api/v1/charges/initialize",
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

        let connector_router_data = korapay::KorapayRouterData::from((amount, req));
        let connector_req = korapay::KorapayPaymentsRequest::try_from(&connector_router_data)?;
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
        let response: korapay::KorapayPaymentsResponse = res
            .response
            .parse_struct("Korapay PaymentsAuthorizeResponse")
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

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Korapay {
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

    // Real endpoint per Korapay's own docs, confirmed against
    // legacy-node/providers/korapay.js's own comment: previously
    // `/transactions/verify?reference=`, which does not exist on Korapay's
    // API — GET by path param is the corrected shape this connector uses.
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
            "{}api/v1/charges/{}",
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
        let response: korapay::KorapayPaymentsResponse = res
            .response
            .parse_struct("Korapay PaymentsSyncResponse")
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

// Korapay's `charges/initialize` is an auto-capture, single-step flow (no
// separate authorize-then-capture endpoint anywhere in
// legacy-node/providers/korapay.js) — same position as Opennode elsewhere
// in this crate, which also has no Capture API and returns
// `FlowNotSupported` rather than guessing at an endpoint.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Korapay {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Korapay".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above — no void/cancel endpoint observed for
// Korapay's collection flow in the legacy integration.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Korapay {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Korapay".to_string(),
        }
        .into())
    }
}

// No refund method exists anywhere in
// legacy-node/providers/korapay.js — this connector does not guess at an
// unconfirmed `/refunds` endpoint the way Task 42's payout-shape bug taught
// this codebase not to. Flagged in handover.md as open follow-up work
// (confirm against developers.korapay.com/docs/refunds or Korapay support
// before wiring), same discipline as `getCardEvents()`'s own "needs a
// direct answer before this is written" note in the legacy file.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Korapay {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented(
            "Refund flow for Korapay".to_string(),
        )
        .into())
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Korapay {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented(
            "Refund flow for Korapay".to_string(),
        )
        .into())
    }
}

// Task 77/a-1-iii — Korapay payout fulfillment. Endpoint, request shape,
// and the two-level response shape are all ported from
// legacy-node/providers/korapay.js#processPayout(), a real, already-
// battle-tested Task 42 Part B-a/b fix -- not re-derived from scratch here.
// See korapay/transformers.rs's own `get_korapay_payout_bank_account` note
// for a real, flagged gap this leaf could NOT close: Hyperswitch's
// `PayoutMethodData` has no NUBAN/Korapay-bank-code-shaped variant, so the
// bank-account mapping below is a stopgap, not a confirmed-correct one.
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoFulfill, PayoutsData, PayoutsResponseData> for Korapay {
    fn get_headers(
        &self,
        req: &PayoutsRouterData<PoFulfill>,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    // Real endpoint per Korapay's own docs (developers.korapay.com/docs/
    // payout-via-api), confirmed directly against
    // legacy-node/providers/korapay.js#processPayout()'s own fetch call.
    fn get_url(
        &self,
        _req: &PayoutsRouterData<PoFulfill>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!(
            "{}api/v1/transactions/disburse",
            self.base_url(connectors)
        ))
    }

    fn get_request_body(
        &self,
        req: &PayoutsRouterData<PoFulfill>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let amount = convert_amount(
            self.amount_converter,
            req.request.minor_amount,
            req.request.destination_currency,
        )?;

        let connector_router_data = korapay::KorapayRouterData::from((amount, req));
        let connector_req =
            korapay::KorapayPayoutFulfillRequest::try_from(&connector_router_data)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &PayoutsRouterData<PoFulfill>,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Post)
                .url(&PayoutFulfillType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(PayoutFulfillType::get_headers(self, req, connectors)?)
                .set_body(PayoutFulfillType::get_request_body(
                    self, req, connectors,
                )?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PayoutsRouterData<PoFulfill>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PayoutsRouterData<PoFulfill>, errors::ConnectorError> {
        let response: korapay::KorapayPayoutResponse = res
            .response
            .parse_struct("Korapay PayoutFulfillResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);
        RouterData::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

// Task 77/a-1-iii — Korapay payout verification. Endpoint confidence is
// explicitly weaker than Fulfill's -- see
// legacy-node/providers/korapay.js#verifyPayout()'s own comment (Task 42
// "the missing verification call — part i"): this path is a strong
// pattern-match off Korapay's own Bulk Payouts docs
// (`.../transactions/bulk/:batch_reference` verifies a bulk batch; dropping
// "bulk/" gives the single-payout path), not a directly-quoted single-payout
// endpoint from Korapay's own docs. Flagged there for a live sandbox call
// before production trust; still true here, ported as-is rather than
// re-guessed differently.
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoSync, PayoutsData, PayoutsResponseData> for Korapay {
    fn get_headers(
        &self,
        req: &PayoutsRouterData<PoSync>,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        req: &PayoutsRouterData<PoSync>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let connector_payout_id = req.request.connector_payout_id.clone().ok_or(
            errors::ConnectorError::MissingRequiredField {
                field_name: "connector_payout_id".into(),
            },
        )?;
        Ok(format!(
            "{}api/v1/transactions/{}",
            self.base_url(connectors),
            connector_payout_id
        ))
    }

    fn build_request(
        &self,
        req: &PayoutsRouterData<PoSync>,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Get)
                .url(&PayoutSyncType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(PayoutSyncType::get_headers(self, req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PayoutsRouterData<PoSync>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PayoutsRouterData<PoSync>, errors::ConnectorError> {
        let response: korapay::KorapayPayoutResponse = res
            .response
            .parse_struct("Korapay PayoutSyncResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);
        RouterData::try_from(ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

#[async_trait::async_trait]
impl webhooks::IncomingWebhook for Korapay {
    // Korapay webhook signature verification (hex HMAC-SHA256 of only the
    // `data` object, per legacy-node/providers/korapay.js's
    // `verifyWebhookSignature`) is real, working logic in the legacy stack
    // but genuinely out of scope for this leaf (a-1-ii-X covers
    // ConnectorIntegration, not IncomingWebhook) — left as
    // WebhooksNotImplemented and flagged in handover.md as the next natural
    // follow-up, rather than half-ported here.
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

static KORAPAY_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> = LazyLock::new(|| {
    let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

    let mut korapay_supported_payment_methods = SupportedPaymentMethods::new();

    korapay_supported_payment_methods.add(
        enums::PaymentMethod::Card,
        enums::PaymentMethodType::Credit,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods: supported_capture_methods.clone(),
            specific_features: None,
        },
    );
    korapay_supported_payment_methods.add(
        enums::PaymentMethod::BankTransfer,
        enums::PaymentMethodType::Ach,
        PaymentMethodDetails {
            mandates: enums::FeatureStatus::NotSupported,
            refunds: enums::FeatureStatus::NotSupported,
            supported_capture_methods,
            specific_features: None,
        },
    );

    korapay_supported_payment_methods
});

static KORAPAY_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "Korapay",
    description: "Korapay is a Nigeria-based payment gateway offering card, bank transfer, and mobile money collection and payout across African rails.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static KORAPAY_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Korapay {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&KORAPAY_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*KORAPAY_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&KORAPAY_SUPPORTED_WEBHOOK_FLOWS)
    }
}
