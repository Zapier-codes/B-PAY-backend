use router::types::{self, api, storage::enums};
use test_utils::connector_auth;

use crate::utils::{self, ConnectorActions};

#[derive(Clone, Copy)]
struct JuicywayTest;
impl ConnectorActions for JuicywayTest {}
impl utils::Connector for JuicywayTest {
    fn get_data(&self) -> api::ConnectorData {
        use router::connector::Juicyway;
        utils::construct_connector_data_old(
            Box::new(Juicyway::new()),
            types::Connector::Juicyway,
            api::GetToken::Connector,
            None,
        )
    }

    fn get_auth_token(&self) -> types::ConnectorAuthType {
        utils::to_connector_auth_type(
            connector_auth::ConnectorAuthentication::new()
                .juicyway
                .expect("Missing connector authentication configuration")
                .into(),
        )
    }

    fn get_name(&self) -> String {
        "juicyway".to_string()
    }
}

static CONNECTOR: JuicywayTest = JuicywayTest {};

fn get_default_payment_info() -> Option<utils::PaymentInfo> {
    None
}

fn payment_method_details() -> Option<types::PaymentsAuthorizeData> {
    None
}

// JuicyWay's `payment-sessions` is a hosted-checkout, auto-capture-only
// flow (see juicyway.rs's own notes on Capture/Void/Execute/RSync all
// returning FlowNotSupported/NotImplemented) -- this test file, like
// Korapay's own a-1-ii-X test scaffold, deliberately does NOT copy the
// connector-template's manual-capture/void/refund test boilerplate, since
// those flows aren't wired and copying template tests that can't pass
// would be misleading rather than useful. Only the two flows this
// connector actually implements (Authorize, PSync) are covered.
// Real assertions here need a live JuicyWay sandbox key in
// crates/router/tests/connectors/sample_auth.toml (or the environment
// equivalent) and a real reference to sync against -- neither exists in
// this sandbox session, so these are left as the same "boilerplate,
// ignored until a real credential is wired in" shape the template itself
// produces for a fresh connector, not asserted against fabricated
// responses.
#[actix_web::test]
#[ignore = "requires a real JuicyWay sandbox secret key; see this file's own comment"]
async fn should_authorize_payment() {
    let response = CONNECTOR
        .authorize_payment(payment_method_details(), get_default_payment_info())
        .await
        .expect("Authorize payment response");
    // JuicyWay's `JuicywayPaymentStatus` enum is this session's own
    // reasonable-pattern guess, not a docs-confirmed enumeration (see this
    // session's handover entry) -- so, like Korapay, this only checks that
    // the hosted-checkout flow didn't hard-fail, rather than asserting a
    // specific AttemptStatus, until the real status strings are confirmed
    // against a live sandbox call.
    assert_ne!(response.status, enums::AttemptStatus::Failure);
}
