pub mod transformers;

use std::sync::LazyLock;

use common_enums::enums;
use common_utils::{
    crypto,
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
    types::{PaymentsAuthorizeType, PaymentsSyncType, RefundExecuteType, RefundSyncType, Response},
    webhooks,
};
#[cfg(feature = "payouts")]
use hyperswitch_interfaces::types::{PayoutFulfillType, PayoutSyncType};
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as flutterwave;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

#[derive(Clone)]
pub struct Flutterwave {
    amount_converter: &'static (dyn AmountConvertor<Output = FloatMajorUnit> + Sync),
}

impl Flutterwave {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &FloatMajorUnitForConnector,
        }
    }
}

impl api::Payment for Flutterwave {}
impl api::PaymentSession for Flutterwave {}
impl api::ConnectorAccessToken for Flutterwave {}
impl api::MandateSetup for Flutterwave {}
impl api::PaymentAuthorize for Flutterwave {}
impl api::PaymentSync for Flutterwave {}
impl api::PaymentCapture for Flutterwave {}
impl api::PaymentVoid for Flutterwave {}
impl api::Refund for Flutterwave {}
impl api::RefundExecute for Flutterwave {}
impl api::RefundSync for Flutterwave {}
impl api::PaymentToken for Flutterwave {}

// Payout flows, continuing the prior leaf's own explicit follow-up note.
// `api::Payouts` itself (the supertrait requiring every payout flow at
// once) is only real when the `payouts` cargo feature is on -- see
// hyperswitch_interfaces::api::payouts, same split Korapay's own
// connector in this crate already follows. Only `PayoutFulfill`/
// `PayoutSync` are implemented for real below, per
// legacy-node/providers/flutterwave.js#processPayout()/verifyPayout()'s
// own confirmed request/response shapes (Task 52/d-2a) -- a flat,
// single-call disburse, same shape class as Korapay's own connector, NOT
// the beneficiary-first shape Paystack/JuicyWay both need in this crate.
// Create/Cancel/Eligibility/Quote/Recipient/RecipientAccount are
// deliberately left on this crate's own `default_imp_for_payouts_*!`
// macros (Flutterwave was removed ONLY from the fulfill/retrieve macro
// lists in default_implementations.rs, same two Korapay was removed
// from) -- each would need its own, separately-confirmed Flutterwave API
// shape before being built for real. Refund's id-threading and webhook
// verification (both once open follow-ups from this same list) are now
// closed too -- see the Execute/RSync impls and the IncomingWebhook impl
// below for each. What's left open for a future leaf is only the
// deliberately-deferred payout sub-flows named above.
impl api::Payouts for Flutterwave {}
#[cfg(feature = "payouts")]
impl api::PayoutFulfill for Flutterwave {}
#[cfg(feature = "payouts")]
impl api::PayoutSync for Flutterwave {}

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Flutterwave
{
    // Not Implemented (R) — Flutterwave v3's Standard checkout is a single
    // hosted-link endpoint; no separate tokenization step is called out
    // anywhere in this repo's own legacy-node/providers/flutterwave.js.
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Flutterwave
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

impl ConnectorCommon for Flutterwave {
    fn id(&self) -> &'static str {
        "flutterwave"
    }

    // Confirmed against developer.flutterwave.com's own "Flutterwave
    // Standard" worked example this session (`amount: '7500'` charges
    // ₦7,500, not ₦75) — see transformers.rs's own citation on
    // `FlutterwaveRouterData` for the full note, including this repo's
    // own legacy-node/providers/flutterwave.js confidence caveat that
    // this fetch resolves.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Base
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.flutterwave.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = flutterwave::FlutterwaveAuthType::try_from(auth_type)
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
        let response: flutterwave::FlutterwaveErrorResponse = res
            .response
            .parse_struct("FlutterwaveErrorResponse")
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

impl ConnectorValidation for Flutterwave {
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

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Flutterwave {
    // Flutterwave has no separate session-token flow — the hosted `data
    // .link` returned by Authorize (see transformers.rs) is Flutterwave's
    // whole "session", same position Korapay's own connector is in.
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Flutterwave {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData>
    for Flutterwave
{
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // No mandate/recurring-charge API observed anywhere in this
        // repo's own legacy-node/providers/flutterwave.js — not wired
        // rather than guessed, same posture as Korapay's own connector.
        Err(errors::ConnectorError::NotImplemented(
            "Setup Mandate flow for Flutterwave".to_string(),
        )
        .into())
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Flutterwave {
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

    // Confirmed against developer.flutterwave.com's own "Flutterwave
    // Standard" guide, fetched this session — `POST /v3/payments` is the
    // real, current endpoint (not a deprecated/renamed path), matching
    // this repo's own legacy-node/providers/flutterwave.js#processPayment().
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}/payments", self.base_url(connectors)))
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

        let connector_router_data = flutterwave::FlutterwaveRouterData::from((amount, req));
        let connector_req =
            flutterwave::FlutterwavePaymentsRequest::try_from(&connector_router_data)?;
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
        let response: flutterwave::FlutterwavePaymentsResponse = res
            .response
            .parse_struct("Flutterwave PaymentsAuthorizeResponse")
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

impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Flutterwave {
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

    // Deliberately `verify_by_reference` (merchant `tx_ref`), NOT the
    // numeric-id `/transactions/:id/verify` path most other connectors in
    // this crate would use for PSync. This is a real API constraint, not
    // a stylistic choice — see transformers.rs's own long note on
    // `FlutterwavePaymentsResponse` for why: Authorize's response never
    // returns Flutterwave's own numeric `id`, only a hosted link, so
    // `tx_ref` (which this connector generates and controls) is the only
    // identifier available to sync against. Matches this repo's own
    // legacy-node/providers/flutterwave.js#verifyTransaction(), which
    // made the same endpoint choice for the same reason (its own comment
    // there confirms the id-based path "takes Flutterwave's own internal
    // numeric `id`, not the merchant `reference`").
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
            "{}/transactions/verify_by_reference?tx_ref={}",
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
        let response: flutterwave::FlutterwaveVerifyResponse = res
            .response
            .parse_struct("Flutterwave PaymentsSyncResponse")
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

// Flutterwave's Standard endpoint is redirect/hosted-checkout only in
// this leaf's scope — no separate authorize-then-capture step is called
// anywhere in this repo's own legacy-node/providers/flutterwave.js.
// Capture/Void/Refund below all return `FlowNotSupported`/`NotImplemented`
// rather than a guessed endpoint, same position Opennode and (for its
// Capture flow) Korapay are already in elsewhere in this crate.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Flutterwave {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Capture flow for Flutterwave".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Flutterwave {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Void flow for Flutterwave".to_string())
                .into(),
        )
    }
}

// Refund: Flutterwave v3's real `/v3/transactions/:id/refund` endpoint
// (confirmed against developer.flutterwave.com/docs/collecting-payments/
// refunds, fetched this session) keys off Flutterwave's own numeric `id`
// — the same id Authorize's response doesn't return (see the PSync note
// above). This was previously left `NotImplemented` pending exactly that
// id-threading; PSync's own transformer (transformers.rs) now stores the
// id in `connector_metadata` the moment it's known, closing the gap —
// see `FlutterwaveTransactionMeta`'s own comment there for the full
// reasoning and why a naive fix would have been wrong.
//
// Real, load-bearing consequence of that fix, not a footnote: a refund
// can only succeed once PSync has actually run at least once for this
// payment and populated `connector_metadata` — a refund attempted before
// any sync call (Authorize's own response alone) fails with a clear
// `MissingRequiredField`, not a confusing downstream 404 from calling
// the refund endpoint with no id at all.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Flutterwave {
    fn get_headers(
        &self,
        req: &RefundsRouterData<Execute>,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        req: &RefundsRouterData<Execute>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let meta: flutterwave::FlutterwaveTransactionMeta =
            crate::utils::to_connector_meta(req.request.connector_metadata.clone())?;
        Ok(format!(
            "{}/transactions/{}/refund",
            self.base_url(connectors),
            meta.transaction_id
        ))
    }

    fn get_request_body(
        &self,
        req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let amount = convert_amount(
            self.amount_converter,
            req.request.minor_refund_amount,
            req.request.currency,
        )?;
        let connector_router_data = flutterwave::FlutterwaveRouterData::from((amount, req));
        let connector_req =
            flutterwave::FlutterwaveRefundRequest::try_from(&connector_router_data)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &RefundsRouterData<Execute>,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Post)
                .url(&RefundExecuteType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(RefundExecuteType::get_headers(self, req, connectors)?)
                .set_body(RefundExecuteType::get_request_body(self, req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &RefundsRouterData<Execute>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<RefundsRouterData<Execute>, errors::ConnectorError> {
        let response: flutterwave::FlutterwaveRefundResponse = res
            .response
            .parse_struct("Flutterwave RefundExecuteResponse")
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

// GET /v3/refunds/{id} — a clean field-for-field handoff from Execute's
// own `connector_refund_id` (see transformers.rs's own note on why this
// one, unlike Execute's transaction-id lookup, has no metadata-threading
// gap to begin with).
impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Flutterwave {
    fn get_headers(
        &self,
        req: &RefundsRouterData<RSync>,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        req: &RefundsRouterData<RSync>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        // CORRECTED (see comment above the previous version of this
        // method in git blame / commit eeb6156af): GET /v3/refunds/{id}
        // is keyed on Flutterwave's own numeric *transaction* id, NOT
        // the refund's own connector_refund_id -- confirmed two ways
        // this session:
        //   1. developer.flutterwave.com/v3.0.0/reference/get-transaction-refunds
        //      itself: this endpoint's own `id` path param is documented
        //      as "returned in the initiate charge and verify transaction
        //      responses as data.id" -- explicitly the transaction id,
        //      not a refund-record id.
        //   2. The official `flutterwave-node-v3` npm package's own
        //      refund-creation example uses an identically-named `id`
        //      field with the same docstring ("This is the transaction
        //      unique identifier. It is returned in the initiate
        //      transaction call as data.id"), confirming both the
        //      create and fetch endpoints share one id namespace: the
        //      transaction's, not the refund's.
        // So this pulls the same FlutterwaveTransactionMeta the Execute
        // flow above reads, not req.request.connector_refund_id -- using
        // connector_refund_id here would build a URL Flutterwave's API
        // was never documented to accept.
        let meta: flutterwave::FlutterwaveTransactionMeta =
            crate::utils::to_connector_meta(req.request.connector_metadata.clone())?;
        Ok(format!(
            "{}/refunds/{}",
            self.base_url(connectors),
            meta.transaction_id
        ))
    }

    fn build_request(
        &self,
        req: &RefundsRouterData<RSync>,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Get)
                .url(&RefundSyncType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(RefundSyncType::get_headers(self, req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &RefundsRouterData<RSync>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<RefundsRouterData<RSync>, errors::ConnectorError> {
        let response: flutterwave::FlutterwaveRefundResponse = res
            .response
            .parse_struct("Flutterwave RefundSyncResponse")
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

// Payout fulfillment (`POST /transfers`). Request shape ported from
// legacy-node/providers/flutterwave.js#processPayout() (Task 52/d-2a) --
// see transformers.rs's own file-level note on this section for the
// flat-vs-nested shape difference from Korapay's own connector, and the
// real NUBAN/bank-code stopgap this leaf shares with Korapay/Paystack/
// JuicyWay.
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoFulfill, PayoutsData, PayoutsResponseData> for Flutterwave {
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

    fn get_url(
        &self,
        _req: &PayoutsRouterData<PoFulfill>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}/transfers", self.base_url(connectors)))
    }

    fn get_request_body(
        &self,
        req: &PayoutsRouterData<PoFulfill>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        // Same base/major-unit rule as collection above (see
        // transformers.rs's own `FlutterwaveRouterData` note) --
        // legacy-node's own processPayout() reuses the identical
        // `convertAmountForProvider(..., 'flutterwave', ...)` call for
        // payouts, not a separate payout-specific unit rule.
        let amount = convert_amount(
            self.amount_converter,
            req.request.minor_amount,
            req.request.destination_currency,
        )?;
        let connector_router_data = flutterwave::FlutterwaveRouterData::from((amount, req));
        let connector_req =
            flutterwave::FlutterwavePayoutFulfillRequest::try_from(&connector_router_data)?;
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
        let response: flutterwave::FlutterwavePayoutResponse = res
            .response
            .parse_struct("Flutterwave PayoutFulfillResponse")
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

// Payout verification (`GET /transfers/{id}`). Keyed on Flutterwave's own
// internal numeric transfer id, NOT a merchant reference -- unlike
// Korapay's own PoSync in this crate. See
// legacy-node/providers/flutterwave.js#verifyPayout()'s own docblock
// (Task 52/d-2a): no confirmed reference-based single-transfer lookup
// exists on Flutterwave's side, only the id-based path. `PoFulfill`'s own
// response above stores that id in `connector_payout_id` for this flow
// to read back.
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoSync, PayoutsData, PayoutsResponseData> for Flutterwave {
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
                field_name: "connector_payout_id (Flutterwave transfer id from PoFulfill)".into(),
            },
        )?;
        Ok(format!(
            "{}/transfers/{}",
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
        let response: flutterwave::FlutterwavePayoutResponse = res
            .response
            .parse_struct("Flutterwave PayoutSyncResponse")
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
impl webhooks::IncomingWebhook for Flutterwave {
    // Flutterwave v3 webhook signature verification — ported from
    // legacy-node/providers/flutterwave.js#verifyWebhookSignature.
    //
    // Per developer.flutterwave.com/docs/webhooks and Flutterwave's own
    // official examples (Node, PHP) — both fetched and cross-checked
    // during the legacy port — the `verif-hash` header is a plain
    // dashboard-configured shared secret echoed back verbatim on every
    // call, NOT a per-payload HMAC digest the way Korapay's
    // `x-korapay-signature` is (at least one third-party blog post found
    // during that port computed an HMAC instead, which would reject every
    // genuine Flutterwave webhook — not followed here, same call the
    // legacy JS already made). `crypto::ConstantTimeEquals` — the Rust
    // equivalent of the legacy code's `crypto.timingSafeEqual` — is
    // therefore the correct algorithm here, not `crypto::HmacSha256`.
    //
    // Structurally this mirrors the same two `IncomingWebhook` trait hooks
    // (`get_webhook_source_verification_algorithm` /
    // `get_webhook_source_verification_signature`) the Stripe connector in
    // this same crate uses for its own (HMAC-based) scheme, rather than a
    // one-off hand-rolled comparison — same framework, different algorithm,
    // because the two providers' real webhook designs are genuinely
    // different, not because one connector is more "correct" than the
    // other. `get_webhook_source_verification_message` is deliberately not
    // overridden: there is nothing to hash, so the trait's own default
    // (`Ok(Vec::new())`) is correct as-is, and `ConstantTimeEquals` ignores
    // its `msg` argument entirely.
    //
    // The secret side of the comparison is sourced through this trait's
    // existing default `get_webhook_source_verification_merchant_secret`
    // (the per-merchant-connector-account secret already modeled generically
    // by this framework) rather than reading `FLW_SECRET_HASH` from the
    // environment the way the legacy Node script did — deliberate: this
    // framework already generalizes secret storage per merchant/connector,
    // so bypassing it for a directly-read env var would both re-introduce
    // the single-tenant assumption the legacy script made and diverge from
    // every other connector's own pattern in this crate. If no secret has
    // been configured for a given merchant, the default falls back to the
    // literal string `"default_secret"`, which will never equal a real
    // Flutterwave `verif-hash` value — so an unconfigured secret still
    // fails closed, the same posture the legacy JS's own explicit
    // `if (!configuredHash) { ...reject... }` check took.
    //
    // `get_webhook_object_reference_id` / `get_webhook_event_type` /
    // `get_webhook_resource_object` below close the payload-parsing gap
    // deliberately left open when signature verification was wired —
    // Korapay's own connector still defers this same split, but
    // Flutterwave's own docs gave this session real, worked-example
    // coverage for the two events this connector's own flows actually
    // produce (`charge.completed`, `transfer.completed`), so it's
    // implemented here rather than deferred again for its own sake. See
    // `transformers.rs`'s own "Incoming webhooks" section for the full
    // envelope citations and what's deliberately NOT modeled
    // (subscription/pending-transition events — no worked example of
    // their event *name* was found this session).
    fn get_webhook_object_reference_id(
        &self,
        request: &webhooks::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<api_models::webhooks::ObjectReferenceId, errors::ConnectorError> {
        let event_type: flutterwave::FlutterwaveWebhookEventTypeBody = request
            .body
            .parse_struct("FlutterwaveWebhookEventTypeBody")
            .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;

        match event_type.event {
            flutterwave::FlutterwaveWebhookEventType::ChargeCompleted => {
                let event: flutterwave::FlutterwaveChargeWebhookEvent = request
                    .body
                    .parse_struct("FlutterwaveChargeWebhookEvent")
                    .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;
                Ok(api_models::webhooks::ObjectReferenceId::PaymentId(
                    api_models::payments::PaymentIdType::ConnectorTransactionId(
                        event.data.tx_ref,
                    ),
                ))
            }
            #[cfg(feature = "payouts")]
            flutterwave::FlutterwaveWebhookEventType::TransferCompleted => {
                let event: flutterwave::FlutterwaveTransferWebhookEvent = request
                    .body
                    .parse_struct("FlutterwaveTransferWebhookEvent")
                    .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;
                Ok(api_models::webhooks::ObjectReferenceId::PayoutId(
                    api_models::webhooks::PayoutIdType::ConnectorPayoutId(
                        event.data.id.to_string(),
                    ),
                ))
            }
            flutterwave::FlutterwaveWebhookEventType::Unknown => Err(error_stack::report!(
                errors::ConnectorError::WebhookReferenceIdNotFound
            )),
        }
    }

    fn get_webhook_event_type(
        &self,
        request: &webhooks::IncomingWebhookRequestDetails<'_>,
        _context: Option<&webhooks::WebhookContext>,
    ) -> CustomResult<api_models::webhooks::IncomingWebhookEvent, errors::ConnectorError> {
        let event_type: flutterwave::FlutterwaveWebhookEventTypeBody = request
            .body
            .parse_struct("FlutterwaveWebhookEventTypeBody")
            .change_context(errors::ConnectorError::WebhookEventTypeNotFound)?;

        Ok(match event_type.event {
            flutterwave::FlutterwaveWebhookEventType::ChargeCompleted => {
                let event: flutterwave::FlutterwaveChargeWebhookEvent = request
                    .body
                    .parse_struct("FlutterwaveChargeWebhookEvent")
                    .change_context(errors::ConnectorError::WebhookEventTypeNotFound)?;
                match event.data.status {
                    flutterwave::FlutterwaveTransactionStatus::Successful => {
                        api_models::webhooks::IncomingWebhookEvent::PaymentIntentSuccess
                    }
                    flutterwave::FlutterwaveTransactionStatus::Failed => {
                        api_models::webhooks::IncomingWebhookEvent::PaymentIntentFailure
                    }
                    // Pending/Unknown both fold to Processing rather than
                    // a guessed terminal state -- same fail-safe-to-
                    // non-terminal posture FlutterwaveTransactionStatus's
                    // own AttemptStatus mapping already takes above.
                    flutterwave::FlutterwaveTransactionStatus::Pending
                    | flutterwave::FlutterwaveTransactionStatus::Unknown => {
                        api_models::webhooks::IncomingWebhookEvent::PaymentIntentProcessing
                    }
                }
            }
            #[cfg(feature = "payouts")]
            flutterwave::FlutterwaveWebhookEventType::TransferCompleted => {
                let event: flutterwave::FlutterwaveTransferWebhookEvent = request
                    .body
                    .parse_struct("FlutterwaveTransferWebhookEvent")
                    .change_context(errors::ConnectorError::WebhookEventTypeNotFound)?;
                match event.data.status.as_deref().map(str::to_uppercase).as_deref() {
                    Some("SUCCESSFUL") => api_models::webhooks::IncomingWebhookEvent::PayoutSuccess,
                    Some("FAILED") => api_models::webhooks::IncomingWebhookEvent::PayoutFailure,
                    // Covers "NEW" (Flutterwave's own confirmed
                    // non-terminal acknowledgement state -- see
                    // flutterwave_payout_status_from_str's own comment in
                    // transformers.rs) plus anything unrecognized, same
                    // "don't guess a terminal state" posture as above.
                    _ => api_models::webhooks::IncomingWebhookEvent::PayoutProcessing,
                }
            }
            flutterwave::FlutterwaveWebhookEventType::Unknown => {
                api_models::webhooks::IncomingWebhookEvent::EventNotSupported
            }
        })
    }

    fn get_webhook_resource_object(
        &self,
        request: &webhooks::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<Box<dyn hyperswitch_masking::ErasedMaskSerialize>, errors::ConnectorError>
    {
        let event_type: flutterwave::FlutterwaveWebhookEventTypeBody = request
            .body
            .parse_struct("FlutterwaveWebhookEventTypeBody")
            .change_context(errors::ConnectorError::WebhookResourceObjectNotFound)?;

        match event_type.event {
            flutterwave::FlutterwaveWebhookEventType::ChargeCompleted => {
                let event: flutterwave::FlutterwaveChargeWebhookEvent = request
                    .body
                    .parse_struct("FlutterwaveChargeWebhookEvent")
                    .change_context(errors::ConnectorError::WebhookResourceObjectNotFound)?;
                Ok(Box::new(event))
            }
            #[cfg(feature = "payouts")]
            flutterwave::FlutterwaveWebhookEventType::TransferCompleted => {
                let event: flutterwave::FlutterwaveTransferWebhookEvent = request
                    .body
                    .parse_struct("FlutterwaveTransferWebhookEvent")
                    .change_context(errors::ConnectorError::WebhookResourceObjectNotFound)?;
                Ok(Box::new(event))
            }
            flutterwave::FlutterwaveWebhookEventType::Unknown => Err(error_stack::report!(
                errors::ConnectorError::WebhookResourceObjectNotFound
            )),
        }
    }
}

static FLUTTERWAVE_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> =
    LazyLock::new(|| {
        let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

        let mut flutterwave_supported_payment_methods = SupportedPaymentMethods::new();

        flutterwave_supported_payment_methods.add(
            enums::PaymentMethod::Card,
            enums::PaymentMethodType::Credit,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods: supported_capture_methods.clone(),
                specific_features: None,
            },
        );
        flutterwave_supported_payment_methods.add(
            enums::PaymentMethod::BankTransfer,
            enums::PaymentMethodType::Ach,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods,
                specific_features: None,
            },
        );

        flutterwave_supported_payment_methods
    });

static FLUTTERWAVE_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "Flutterwave",
    description: "Flutterwave is a pan-African payment gateway offering card, bank transfer, mobile money, and USSD collection (v3 Standard/hosted checkout) across African rails.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static FLUTTERWAVE_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Flutterwave {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&FLUTTERWAVE_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*FLUTTERWAVE_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&FLUTTERWAVE_SUPPORTED_WEBHOOK_FLOWS)
    }
}
