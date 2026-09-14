use router::types::{self, api, storage::enums};

use crate::utils::{self, ConnectorActions};
use test_utils::connector_auth;

#[derive(Clone, Copy)]
struct FlutterwaveTest;
impl ConnectorActions for FlutterwaveTest {}
impl utils::Connector for FlutterwaveTest {
    fn get_data(&self) -> api::ConnectorData {
        use router::connector::Flutterwave;
        utils::construct_connector_data_old(
            Box::new(Flutterwave::new()),
            types::Connector::Flutterwave,
            api::GetToken::Connector,
            None,
        )
    }

    fn get_auth_token(&self) -> types::ConnectorAuthType {
        utils::to_connector_auth_type(
            connector_auth::ConnectorAuthentication::new()
                .flutterwave
                .expect("Missing connector authentication configuration")
                .into(),
        )
    }

    fn get_name(&self) -> String {
        "flutterwave".to_string()
    }
}

static CONNECTOR: FlutterwaveTest = FlutterwaveTest {};

fn get_default_payment_info() -> Option<utils::PaymentInfo> {
    None
}

fn payment_method_details() -> Option<types::PaymentsAuthorizeData> {
    None
}

// Same rationale as korapay.rs's own test file: this connector only
// implements Authorize + PSync so far (Capture/Void/Refund/SetupMandate/
// Payouts are explicit NotImplemented stubs, per Task 73's own commit
// message) -- this file deliberately does not copy the connector-template's
// manual-capture/void/refund boilerplate, since those flows can't pass and
// copying unpassable template tests would be misleading rather than useful.
// Also per that same commit: this connector's Authorize response never
// returns Flutterwave's own numeric transaction id, only a hosted checkout
// link -- PSync is verify_by_reference (by tx_ref), not the numeric-id path.
// Real assertions here need a live Flutterwave sandbox secret key in
// crates/router/tests/connectors/sample_auth.toml (or the environment
// equivalent) -- neither exists in this sandbox session, so these are left
// as `#[ignore]`d, same as korapay.rs/juicyway.rs's own tests.
#[actix_web::test]
#[ignore = "requires a real Flutterwave sandbox secret key; see this file's own comment"]
async fn should_authorize_payment() {
    let response = CONNECTOR
        .authorize_payment(payment_method_details(), get_default_payment_info())
        .await
        .expect("Authorize payment response");
    // Flutterwave's v3 charge flow returns a hosted-checkout link, not an
    // immediate Charged/Authorized status -- confirm against a real response
    // before asserting a specific AttemptStatus here.
    assert_ne!(response.status, enums::AttemptStatus::Failure);
}
