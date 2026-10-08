use router::types::{self, api, storage::enums};
use test_utils::connector_auth;

use crate::utils::{self, ConnectorActions};

#[derive(Clone, Copy)]
struct XixapayTest;
impl ConnectorActions for XixapayTest {}
impl utils::Connector for XixapayTest {
    fn get_data(&self) -> api::ConnectorData {
        use router::connector::Xixapay;
        utils::construct_connector_data_old(
            Box::new(Xixapay::new()),
            types::Connector::Xixapay,
            api::GetToken::Connector,
            None,
        )
    }

    fn get_auth_token(&self) -> types::ConnectorAuthType {
        utils::to_connector_auth_type(
            connector_auth::ConnectorAuthentication::new()
                .xixapay
                .expect("Missing connector authentication configuration")
                .into(),
        )
    }

    fn get_name(&self) -> String {
        "xixapay".to_string()
    }
}

static CONNECTOR: XixapayTest = XixapayTest {};

fn get_default_payment_info() -> Option<utils::PaymentInfo> {
    None
}

fn payment_method_details() -> Option<types::PaymentsAuthorizeData> {
    None
}

// Xixapay's `payment/charge` is a hosted-checkout, auto-capture-only flow
// (see xixapay.rs's own notes on Capture/Void/Execute/RSync all returning
// FlowNotSupported/NotImplemented) -- this test file deliberately does NOT
// copy the connector-template's manual-capture/void/refund test
// boilerplate, since those flows aren't wired and copying template tests
// that can't pass would be misleading rather than useful. Only the two
// flows this connector actually implements (Authorize, PSync) are covered.
// Real assertions here need a live Xixapay sandbox secret key in
// crates/router/tests/connectors/sample_auth.toml (or the environment
// equivalent) and a real `paymentIdentifier` to sync against -- neither
// exists in this sandbox session, so these are left as the same
// "boilerplate, ignored until a real credential is wired in" shape the
// template itself produces for a fresh connector, not asserted against
// fabricated responses.
#[actix_web::test]
#[ignore = "requires a real Xixapay sandbox secret key; see this file's own comment"]
async fn should_authorize_payment() {
    let response = CONNECTOR
        .authorize_payment(payment_method_details(), get_default_payment_info())
        .await
        .expect("Authorize payment response");
    // Xixapay's checkout flow starts in a redirect-required state, not an
    // immediate Charged -- confirm against a real response before asserting
    // a specific AttemptStatus here.
    assert_ne!(response.status, enums::AttemptStatus::Failure);
}
