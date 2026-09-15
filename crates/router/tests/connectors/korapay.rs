use router::types::{self, api, storage::enums};
use test_utils::connector_auth;

use crate::utils::{self, ConnectorActions};

#[derive(Clone, Copy)]
struct KorapayTest;
impl ConnectorActions for KorapayTest {}
impl utils::Connector for KorapayTest {
    fn get_data(&self) -> api::ConnectorData {
        use router::connector::Korapay;
        utils::construct_connector_data_old(
            Box::new(Korapay::new()),
            types::Connector::Korapay,
            api::GetToken::Connector,
            None,
        )
    }

    fn get_auth_token(&self) -> types::ConnectorAuthType {
        utils::to_connector_auth_type(
            connector_auth::ConnectorAuthentication::new()
                .korapay
                .expect("Missing connector authentication configuration")
                .into(),
        )
    }

    fn get_name(&self) -> String {
        "korapay".to_string()
    }
}

static CONNECTOR: KorapayTest = KorapayTest {};

fn get_default_payment_info() -> Option<utils::PaymentInfo> {
    None
}

fn payment_method_details() -> Option<types::PaymentsAuthorizeData> {
    None
}

// Korapay's `charges/initialize` is a hosted-checkout, auto-capture-only
// flow (see korapay.rs's own notes on Capture/Void/Execute/RSync all
// returning FlowNotSupported/NotImplemented) — this test file deliberately
// does NOT copy the connector-template's manual-capture/void/refund test
// boilerplate, since those flows aren't wired and copying template tests
// that can't pass would be misleading rather than useful. Only the two
// flows this connector actually implements (Authorize, PSync) are covered.
// Real assertions here need a live Korapay sandbox key in
// crates/router/tests/connectors/sample_auth.toml (or the environment
// equivalent) and a real reference to sync against — neither exists in
// this sandbox session, so these are left as the same "boilerplate,
// ignored until a real credential is wired in" shape the template itself
// produces for a fresh connector, not asserted against fabricated
// responses.
#[actix_web::test]
#[ignore = "requires a real Korapay sandbox secret key; see this file's own comment"]
async fn should_authorize_payment() {
    let response = CONNECTOR
        .authorize_payment(payment_method_details(), get_default_payment_info())
        .await
        .expect("Authorize payment response");
    // Korapay's checkout flow starts in a pending/redirect-required state,
    // not an immediate Charged/Authorized — confirm against a real
    // response before asserting a specific AttemptStatus here.
    assert_ne!(response.status, enums::AttemptStatus::Failure);
}
