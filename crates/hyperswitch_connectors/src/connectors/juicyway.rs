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
#[cfg(feature = "payouts")]
use hyperswitch_domain_models::{
    router_flow_types::{PoFulfill, PoRecipient, PoSync},
    types::{PayoutsData, PayoutsResponseData, PayoutsRouterData},
};
#[cfg(feature = "payouts")]
use hyperswitch_interfaces::types::{PayoutFulfillType, PayoutRecipientType, PayoutSyncType};
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
use hyperswitch_masking::{ExposeInterface, Mask, Maskable};
use transformers as juicyway;

use crate::{constants::headers, types::ResponseRouterData, utils::convert_amount};

#[derive(Clone)]
pub struct Juicyway {
    amount_converter: &'static (dyn AmountConvertor<Output = MinorUnit> + Sync),
}

impl Juicyway {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &MinorUnitForConnector,
        }
    }
}

impl api::Payment for Juicyway {}
impl api::PaymentSession for Juicyway {}
impl api::ConnectorAccessToken for Juicyway {}
impl api::MandateSetup for Juicyway {}
impl api::PaymentAuthorize for Juicyway {}
impl api::PaymentSync for Juicyway {}
impl api::PaymentCapture for Juicyway {}
impl api::PaymentVoid for Juicyway {}
impl api::Refund for Juicyway {}
impl api::RefundExecute for Juicyway {}
impl api::RefundSync for Juicyway {}
impl api::PaymentToken for Juicyway {}

// Task 77/a-3-iii — JuicyWay payout flows. `api::Payouts` itself (the
// supertrait requiring every payout flow at once) is only real when the
// `payouts` cargo feature is on -- see hyperswitch_interfaces::api::payouts,
// same split every other payout-capable connector in this crate follows.
//
// JuicyWay's real shape (legacy-node/providers/juicyway.js#processPayout,
// Task 52) needs a beneficiary created ahead of time -- it does NOT take
// raw bank_code/account_number the way Korapay's single-call disburse
// does -- so this is the same three-flow architecture Paystack's own
// connector (Task 77/a-2-iii) already uses for the identical reason, not
// a new pattern: `PayoutRecipient` creates the beneficiary,
// `PayoutFulfill` sends the actual payout referencing it, `PayoutSync`
// polls JuicyWay's own payout `id` (not the merchant reference -- see
// juicyway/transformers.rs's own note on why, ported directly from
// legacy-node's own verifyPayout() docblock). Create/Eligibility/Cancel/
// Quote/RecipientAccount are deliberately left on this crate's own
// `default_imp_for_payouts_*!` macros (JuicyWay was removed ONLY from
// the recipient/fulfill/retrieve macro lists in
// default_implementations.rs, same three Paystack was removed from) --
// each would need its own, separately-confirmed JuicyWay API shape
// before being built for real.
impl api::Payouts for Juicyway {}
#[cfg(feature = "payouts")]
impl api::PayoutRecipient for Juicyway {}
#[cfg(feature = "payouts")]
impl api::PayoutFulfill for Juicyway {}
#[cfg(feature = "payouts")]
impl api::PayoutSync for Juicyway {}

impl ConnectorIntegration<PaymentMethodToken, PaymentMethodTokenizationData, PaymentsResponseData>
    for Juicyway
{
    // Not Implemented (R) — JuicyWay has no separate tokenization step;
    // the one `/payment-sessions` endpoint takes the full request at
    // Authorize time (see transformers.rs's own note on this).
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Juicyway
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

impl ConnectorCommon for Juicyway {
    fn id(&self) -> &'static str {
        "juicyway"
    }

    // Confirmed against docs.juicyway.com/payments/initialize-payment.md's
    // own "Universal Parameters" section (handover.md Task 49/a): amount
    // is in minor units (×100) across every supported currency, including
    // the still-unresolved stablecoin entries — see this connector's own
    // currency-list gap, flagged in handover.md, not resolved here.
    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Minor
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a Connectors) -> &'a str {
        connectors.juicyway.base_url.as_ref()
    }

    // Confirmed against docs.juicyway.com/authentication.md: the raw
    // secret key, no "Bearer " prefix — unlike Korapay/Paystack in this
    // same crate. legacy-node/providers/juicyway.js's own `Bearer
    // ${apiKey}` calls were a confirmed, real 401-causing bug (Task 45a);
    // this connector starts from the corrected shape directly.
    fn get_auth_header(
        &self,
        auth_type: &ConnectorAuthType,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        let auth = juicyway::JuicywayAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        Ok(vec![(
            headers::AUTHORIZATION.to_string(),
            auth.api_key.expose().into_masked(),
        )])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: juicyway::JuicywayErrorResponse = res
            .response
            .parse_struct("JuicywayErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        let message = response.get_message();
        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response
                .get_code()
                .unwrap_or(consts::NO_ERROR_CODE.to_string()),
            message: message.clone(),
            reason: Some(message),
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

impl ConnectorValidation for Juicyway {
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

impl ConnectorIntegration<Session, PaymentsSessionData, PaymentsResponseData> for Juicyway {
    // JuicyWay has no separate session-token flow — the payment session
    // created by Authorize (see transformers.rs) is JuicyWay's whole
    // "session", same position as Korapay's own checkout_url in this
    // crate.
}

impl ConnectorIntegration<AccessTokenAuth, AccessTokenRequestData, AccessToken> for Juicyway {}

impl ConnectorIntegration<SetupMandate, SetupMandateRequestData, PaymentsResponseData>
    for Juicyway
{
    fn build_request(
        &self,
        _req: &RouterData<SetupMandate, SetupMandateRequestData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        // No mandate/recurring-charge API observed anywhere in
        // legacy-node/providers/juicyway.js — not wired rather than
        // guessed, same discipline as Korapay's own SetupMandate gap.
        Err(
            errors::ConnectorError::NotImplemented("Setup Mandate flow for JuicyWay".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<Authorize, PaymentsAuthorizeData, PaymentsResponseData> for Juicyway {
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

    // Real endpoint per docs.juicyway.com, confirmed against this repo's
    // own handover.md Task 8b full audit: legacy-node's `/v1/charges`
    // does not exist on JuicyWay's API — the real path is
    // `/payment-sessions`. This connector starts from the corrected path
    // directly rather than porting the known-wrong one.
    fn get_url(
        &self,
        _req: &PaymentsAuthorizeRouterData,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}payment-sessions", self.base_url(connectors)))
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

        let connector_router_data = juicyway::JuicywayRouterData::from((amount, req));
        let connector_req = juicyway::JuicywayPaymentsRequest::try_from(&connector_router_data)?;
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
        let response: juicyway::JuicywayPaymentsResponse = res
            .response
            .parse_struct("Juicyway PaymentsAuthorizeResponse")
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

// Real architectural fix over the legacy integration, not a straight
// port: JuicyWay's `GET /payments/{id}` (Fetch Payment) takes JuicyWay's
// own `id`, not the merchant-supplied `reference` — confirmed in
// handover.md's "reference-vs-ID distinction" finding (Task 45d, still
// open in legacy-node/providers/juicyway.js, which calls a
// reference-keyed endpoint that was never even the right path to begin
// with). This connector closes that gap by construction: Authorize's own
// TryFrom (transformers.rs) stores JuicyWay's `payment.id` as the
// connector_transaction_id, and Hyperswitch's own PSync flow always
// looks that field up for the sync call — so there is no reference-based
// lookup to get wrong here in the first place.
impl ConnectorIntegration<PSync, PaymentsSyncData, PaymentsResponseData> for Juicyway {
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
            "{}payments/{}",
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
        let response: juicyway::JuicywayFetchPaymentResponse = res
            .response
            .parse_struct("Juicyway PaymentsSyncResponse")
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

// JuicyWay's `/payment-sessions` is a single-step, presumably
// auto-capture flow — no separate authorize-then-capture endpoint
// appears anywhere in legacy-node/providers/juicyway.js or in the
// endpoint list this session's docs audit turned up. Same position as
// Korapay's own Capture gap in this crate: `FlowNotSupported` rather
// than a guessed-at endpoint.
impl ConnectorIntegration<Capture, PaymentsCaptureData, PaymentsResponseData> for Juicyway {
    fn build_request(
        &self,
        _req: &PaymentsCaptureRouterData,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Juicyway".to_string(),
        }
        .into())
    }
}

// Same reasoning as Capture above — no void/cancel endpoint observed for
// JuicyWay's collection flow in the legacy integration or this session's
// docs audit.
impl ConnectorIntegration<Void, PaymentsCancelData, PaymentsResponseData> for Juicyway {
    fn build_request(
        &self,
        _req: &RouterData<Void, PaymentsCancelData, PaymentsResponseData>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Void".to_string(),
            connector: "Juicyway".to_string(),
        }
        .into())
    }
}

// No refund method exists anywhere in
// legacy-node/providers/juicyway.js, and none of this session's docs
// fetches turned up a `/refunds`-shaped endpoint either — this connector
// does not guess at an unconfirmed refund shape, same discipline Korapay
// and Paystack's own Execute/RSync gaps in this crate already follow.
// Flagged as open follow-up work: confirm against docs.juicyway.com or
// JuicyWay support before wiring.
impl ConnectorIntegration<Execute, RefundsData, RefundsResponseData> for Juicyway {
    fn build_request(
        &self,
        _req: &RefundsRouterData<Execute>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Juicyway".to_string()).into())
    }
}

impl ConnectorIntegration<RSync, RefundsData, RefundsResponseData> for Juicyway {
    fn build_request(
        &self,
        _req: &RefundsRouterData<RSync>,
        _connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::NotImplemented("Refund flow for Juicyway".to_string()).into())
    }
}

#[async_trait::async_trait]
// Task 77/a-3-iii — JuicyWay beneficiary creation. Endpoint path is
// explicitly UNCONFIRMED -- ported as-is from
// legacy-node/providers/juicyway.js#createBeneficiary()'s own docblock
// caveat ("the exact endpoint PATH is the one thing here that is NOT
// confirmed"). Only the `bank_account` beneficiary type is buildable
// here; `crypto_address`/`interac` are real per that same legacy method
// but Hyperswitch's `PayoutMethodData` has no matching shape to build
// them from (same discipline as juicyway/transformers.rs's own
// `get_juicyway_payout_bank_account` note below).
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoRecipient, PayoutsData, PayoutsResponseData> for Juicyway {
    fn get_headers(
        &self,
        req: &PayoutsRouterData<PoRecipient>,
        connectors: &Connectors,
    ) -> CustomResult<Vec<(String, Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        _req: &PayoutsRouterData<PoRecipient>,
        connectors: &Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}beneficiaries", self.base_url(connectors)))
    }

    fn get_request_body(
        &self,
        req: &PayoutsRouterData<PoRecipient>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let connector_req = juicyway::JuicywayCreateBeneficiaryRequest::try_from(req)?;
        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &PayoutsRouterData<PoRecipient>,
        connectors: &Connectors,
    ) -> CustomResult<Option<Request>, errors::ConnectorError> {
        Ok(Some(
            RequestBuilder::new()
                .method(Method::Post)
                .url(&PayoutRecipientType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(PayoutRecipientType::get_headers(self, req, connectors)?)
                .set_body(PayoutRecipientType::get_request_body(
                    self, req, connectors,
                )?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PayoutsRouterData<PoRecipient>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PayoutsRouterData<PoRecipient>, errors::ConnectorError> {
        let response: juicyway::JuicywayBeneficiaryResponse = res
            .response
            .parse_struct("Juicyway PayoutRecipientResponse")
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

// Task 77/a-3-iii — JuicyWay payout fulfillment (`POST /payouts`).
// Request shape ported directly from
// legacy-node/providers/juicyway.js#processPayout() (Task 52/a-1,
// confirmed against docs.juicyway.com/reference/payouts/initiate-a-payout.md
// and the initiate-bank-transfer.md worked examples). See
// juicyway/transformers.rs's own note on the `pin` field for a real,
// flagged gap this leaf could NOT close cleanly: JuicyWay requires a
// per-transfer PIN that has no matching field anywhere on Hyperswitch's
// `PayoutsData`.
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoFulfill, PayoutsData, PayoutsResponseData> for Juicyway {
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
        Ok(format!("{}payouts", self.base_url(connectors)))
    }

    fn get_request_body(
        &self,
        req: &PayoutsRouterData<PoFulfill>,
        _connectors: &Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        // Confirmed (docs.juicyway.com/reference/payouts/initiate-a-payout.md):
        // payout amount is minor units, NOT run through this connector's
        // FloatMajorUnit-style `convert_amount` helper -- callers pass
        // minor units directly, same rule Task 49/a already established
        // for collection and legacy-node's own processPayout() already
        // follows (it forwards `data.amount` unconverted).
        let connector_router_data =
            juicyway::JuicywayRouterData::from((req.request.minor_amount, req));
        let connector_req =
            juicyway::JuicywayPayoutFulfillRequest::try_from(&connector_router_data)?;
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
                .set_body(PayoutFulfillType::get_request_body(self, req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &PayoutsRouterData<PoFulfill>,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<PayoutsRouterData<PoFulfill>, errors::ConnectorError> {
        let response: juicyway::JuicywayPayoutResponse = res
            .response
            .parse_struct("Juicyway PayoutFulfillResponse")
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

// Task 77/a-3-iii — JuicyWay payout verification (`GET /payouts/{id}`).
// Endpoint confidence is explicitly weaker than Fulfill's -- see
// juicyway/transformers.rs's own note, ported directly from
// legacy-node/providers/juicyway.js#verifyPayout()'s own docblock: this
// path was located via docs.juicyway.com/llms.txt as a sibling of the
// confirmed POST /payouts, not independently confirmed for GET. Same
// "verify against a live sandbox call before production trust" caveat
// as Korapay's own PoSync in this crate.
//
// Takes JuicyWay's own `id` (left in `connector_payout_id` by
// `PoFulfill`'s own response below), NOT the merchant reference --
// unlike Korapay/Paystack's PoSync, which both key off a reference. See
// legacy-node's own verifyPayout() docblock for why: JuicyWay's
// processPayout() worked-example response has no `reference` field at
// all, only its own `id`.
#[cfg(feature = "payouts")]
impl ConnectorIntegration<PoSync, PayoutsData, PayoutsResponseData> for Juicyway {
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
                field_name: "connector_payout_id (Juicyway payout id from PoFulfill)".into(),
            },
        )?;
        Ok(format!(
            "{}payouts/{}",
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
        let response: juicyway::JuicywayPayoutResponse = res
            .response
            .parse_struct("Juicyway PayoutSyncResponse")
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

impl webhooks::IncomingWebhook for Juicyway {
    // JuicyWay's webhook checksum scheme (checksum travels INSIDE the
    // JSON body, keyed by the merchant's separate "business ID", over an
    // alphabetically-key-sorted encoding of `data` — see
    // legacy-node/providers/juicyway.js#verifyWebhookSignature and
    // handover.md's own webhook-scheme confirmation) is real, working,
    // and well-documented — but genuinely out of scope for this leaf,
    // same boundary Korapay's own a-1-ii-X drew for `IncomingWebhook`.
    // Left as WebhooksNotImplemented and flagged as the natural next
    // follow-up (unusually low-risk to port, since the scheme is fully
    // confirmed already), not half-ported here.
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

static JUICYWAY_SUPPORTED_PAYMENT_METHODS: LazyLock<SupportedPaymentMethods> =
    LazyLock::new(|| {
        let supported_capture_methods = vec![enums::CaptureMethod::Automatic];

        let mut juicyway_supported_payment_methods = SupportedPaymentMethods::new();

        juicyway_supported_payment_methods.add(
            enums::PaymentMethod::Card,
            enums::PaymentMethodType::Credit,
            PaymentMethodDetails {
                mandates: enums::FeatureStatus::NotSupported,
                refunds: enums::FeatureStatus::NotSupported,
                supported_capture_methods,
                specific_features: None,
            },
        );

        juicyway_supported_payment_methods
    });

static JUICYWAY_CONNECTOR_INFO: ConnectorInfo = ConnectorInfo {
    display_name: "Juicyway",
    description: "JuicyWay is an international payment gateway offering card collection (via hosted payment sessions) and beneficiary-based cross-border payouts.",
    connector_type: enums::HyperswitchConnectorCategory::PaymentGateway,
    integration_status: enums::ConnectorIntegrationStatus::Beta,
};

static JUICYWAY_SUPPORTED_WEBHOOK_FLOWS: [enums::EventClass; 0] = [];

impl ConnectorSpecifications for Juicyway {
    fn get_connector_about(&self) -> Option<&'static ConnectorInfo> {
        Some(&JUICYWAY_CONNECTOR_INFO)
    }

    fn get_supported_payment_methods(&self) -> Option<&'static SupportedPaymentMethods> {
        Some(&*JUICYWAY_SUPPORTED_PAYMENT_METHODS)
    }

    fn get_supported_webhook_flows(&self) -> Option<&'static [enums::EventClass]> {
        Some(&JUICYWAY_SUPPORTED_WEBHOOK_FLOWS)
    }
}
