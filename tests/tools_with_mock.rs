use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use flute2_mcp::config::{Config, Profile};
use flute2_mcp::error::FluteError;
use flute2_mcp::runner::MockRunner;
use flute2_mcp::server::{FluteServer, screen_arguments};
use flute2_mcp::tools::{Empty, Id};
use pretty_assertions::assert_eq;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

pub fn cfg(profile: Profile, allow_prod_writes: bool, merchant_id: Option<&str>) -> Arc<Config> {
    Arc::new(Config {
        profile,
        binary: PathBuf::from("/bin/false"),
        merchant_id: merchant_id.map(String::from),
        timeout: Duration::from_secs(30),
        debug: false,
        allow_prod_writes,
    })
}

pub fn svec<const N: usize>(a: [&str; N]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// The tool's JSON payload, decoded out of the MCP content envelope.
pub fn body(res: &rmcp::model::CallToolResult) -> serde_json::Value {
    let v = serde_json::to_value(res).unwrap();
    serde_json::from_str(v["content"][0]["text"].as_str().unwrap()).unwrap()
}

/// A sandbox server wired to a MockRunner seeded with `n` empty-object results.
pub fn sandbox(n: usize) -> (FluteServer, Arc<MockRunner>) {
    let mock = MockRunner::new((0..n).map(|_| Ok(json!({"object": "x"}))).collect());
    (
        FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone()),
        mock,
    )
}

#[tokio::test]
async fn util_tools_build_expected_argv() {
    let (srv, mock) = sandbox(2);
    srv.ping(Parameters(Empty {})).await.unwrap();
    srv.version(Parameters(Empty {})).await.unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec(["--profile", "sandbox", "--output", "json", "ping"])
    );
    assert_eq!(
        calls[1],
        svec(["--profile", "sandbox", "--output", "json", "version"])
    );
}

#[tokio::test]
async fn auth_status_maps_the_cli_shape() {
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "auth_status",
        "data": {
            "profile": "sandbox",
            "api_base_url": "https://sandbox.api.flute.com",
            "authenticated": true,
            "client_id": "cid-server",
        }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let res = srv.auth_status(Parameters(Empty {})).await.unwrap();
    assert_eq!(res.is_error, Some(false));
    let v = body(&res);
    assert_eq!(v["authenticated"], true);
    assert_eq!(v["profile"], "sandbox");
    assert_eq!(v["api_base_url"], "https://sandbox.api.flute.com");
    assert_eq!(v["client_id"], "cid-server");
    assert_eq!(
        mock.calls()[0],
        svec(["--profile", "sandbox", "--output", "json", "auth", "status"])
    );
}

/// `client_id` is null when no credentials are configured, and `api_base_url`
/// may be absent. Neither may be forwarded as a JSON null.
#[tokio::test]
async fn auth_status_omits_absent_optional_fields() {
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "auth_status",
        "data": { "profile": "sandbox", "authenticated": false, "client_id": null }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock);
    let v = body(&srv.auth_status(Parameters(Empty {})).await.unwrap());
    assert_eq!(v["authenticated"], false);
    assert!(v.get("client_id").is_none());
    assert!(v.get("api_base_url").is_none());
}

#[tokio::test]
async fn api_errors_pass_through_as_is_error() {
    let mock = MockRunner::new(vec![Err(flute2_mcp::error::FluteError::Api {
        status: 422,
        message: "validation failed".into(),
        correlation_id: Some("c-9".into()),
    })]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock);
    let res = srv.ping(Parameters(Empty {})).await.unwrap();
    assert_eq!(res.is_error, Some(true));
    let v = body(&res);
    assert_eq!(v["kind"], "api");
    assert_eq!(v["status"], 422);
    assert_eq!(v["correlation_id"], "c-9");
}

/// The auth error names the two variables an operator sets, and the login
/// command, so the agent's message is actionable without reading the readme.
#[tokio::test]
async fn auth_errors_name_the_credential_variables() {
    let mock = MockRunner::new(vec![Err(flute2_mcp::error::FluteError::Auth {
        message: "no credentials".into(),
    })]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock);
    let v = body(&srv.ping(Parameters(Empty {})).await.unwrap());
    assert_eq!(v["kind"], "auth");
    let msg = v["message"].as_str().unwrap();
    assert!(msg.contains("FLUTE2_CLIENT_ID") && msg.contains("FLUTE2_CLIENT_SECRET"));
    assert!(msg.contains("flute2 auth login"));
}

/// `bad_output` carries a `message` like every other failure kind, and a
/// truncation flag per stream so a client can tell a capped stream from a
/// complete one.
#[tokio::test]
async fn bad_output_errors_carry_a_message_and_truncation_flags() {
    let mock = MockRunner::new(vec![Err(flute2_mcp::error::FluteError::BadOutput {
        exit_code: 9,
        stdout: "x".repeat(5000),
        stderr: "short".into(),
    })]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock);
    let v = body(&srv.ping(Parameters(Empty {})).await.unwrap());
    assert_eq!(v["kind"], "bad_output");
    assert_eq!(v["exit_code"], 9);
    assert!(v["message"].as_str().unwrap().contains("exit 9"));
    assert_eq!(v["stdout"].as_str().unwrap().len(), 4096);
    assert_eq!(v["stdout_truncated"], true);
    assert_eq!(v["stderr"], "short");
    assert_eq!(v["stderr_truncated"], false);
}

#[tokio::test]
async fn reads_are_never_guarded_on_production() {
    let mock = MockRunner::new(vec![Ok(json!({"object": "ping"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    let res = srv.ping(Parameters(Empty {})).await.unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(mock.calls().len(), 1);
}

#[test]
fn page_args_push_the_cli_flag_spellings() {
    use flute2_mcp::tools::PageArgs;
    let mut args = Vec::new();
    PageArgs {
        page_index: Some(0),
        page_size: Some(25),
        all: Some(true),
        sort_by: Some("createdOn".into()),
        asc: None,
        desc: Some(true),
    }
    .push(&mut args);
    assert_eq!(
        args,
        svec([
            "--page-index",
            "0",
            "--page-size",
            "25",
            "--all",
            "--sort-by",
            "createdOn",
            "--desc"
        ])
    );
}

#[test]
fn page_args_omit_everything_unset_and_every_false_switch() {
    use flute2_mcp::tools::PageArgs;
    let mut args = Vec::new();
    PageArgs {
        page_index: None,
        page_size: None,
        all: Some(false),
        sort_by: None,
        asc: Some(false),
        desc: Some(false),
    }
    .push(&mut args);
    assert!(
        args.is_empty(),
        "unset page fields must add no argv: {args:?}"
    );
}

/// The write guard's refusal names the tool and the variable that lifts it, and
/// it fires only on a production profile that has not enabled writes.
#[test]
fn guard_write_refuses_unguarded_production_with_the_exact_message() {
    let srv = FluteServer::new(
        cfg(Profile::Production, false, None),
        MockRunner::new(vec![]),
    );
    let res = srv
        .guard_write("customers_delete")
        .expect("a production profile without allow_prod_writes refuses writes");
    assert_eq!(res.is_error, Some(true));
    let v = body(&res);
    assert_eq!(v["kind"], "client");
    assert_eq!(
        v["message"],
        "refusing customers_delete on production profile; \
         set FLUTE2_MCP_ALLOW_PROD_WRITES=1 to enable writes"
    );
}

#[test]
fn guard_write_admits_enabled_production_and_sandbox() {
    let enabled = FluteServer::new(
        cfg(Profile::Production, true, None),
        MockRunner::new(vec![]),
    );
    assert!(enabled.guard_write("customers_delete").is_none());
    let sandboxed = FluteServer::new(cfg(Profile::Sandbox, false, None), MockRunner::new(vec![]));
    assert!(sandboxed.guard_write("customers_delete").is_none());
}

/// A production server with the guard on, wired to a MockRunner holding no
/// responses: if a guarded write reaches the runner the mock panics, which is
/// exactly the failure we want to see.
pub fn guarded_production() -> (FluteServer, Arc<MockRunner>) {
    let mock = MockRunner::new(vec![]);
    (
        FluteServer::new(cfg(Profile::Production, false, None), mock.clone()),
        mock,
    )
}

#[tokio::test]
async fn customers_reads_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::customers::CustomersList;
    let (srv, mock) = sandbox(2);
    srv.customers_get(Parameters(Id { id: "c1".into() }))
        .await
        .unwrap();
    srv.customers_list(Parameters(CustomersList {
        page_index: Some(1),
        page_size: Some(50),
        all: None,
        sort_by: Some("createdOn".into()),
        asc: None,
        desc: Some(true),
        full_name: Some("Ada".into()),
        email: None,
        company_name: None,
        mobile: None,
        created_from: Some("2026-01-01".into()),
        created_to: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "get",
            "c1"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "list",
            "--page-index",
            "1",
            "--page-size",
            "50",
            "--sort-by",
            "createdOn",
            "--desc",
            "--full-name",
            "Ada",
            "--created-from",
            "2026-01-01"
        ])
    );
}

#[tokio::test]
async fn customers_create_emits_bare_switches_and_both_addresses() {
    use flute2_mcp::tools::customers::CustomerCreate;
    let (srv, mock) = sandbox(1);
    srv.customers_create(Parameters(CustomerCreate {
        first_name: "Ada".into(),
        last_name: "Lovelace".into(),
        company: None,
        email: Some("ada@example.com".into()),
        mobile: None,
        sms_consent: Some(true),
        use_billing_as_shipping: Some(false),
        billing_line1: Some("123 Test St".into()),
        billing_line2: None,
        billing_city: Some("Denver".into()),
        billing_state: Some("CO".into()),
        billing_postal_code: Some("80202".into()),
        billing_country: Some("US".into()),
        shipping_line1: Some("456 Ship Rd".into()),
        shipping_line2: None,
        shipping_city: None,
        shipping_state: None,
        shipping_postal_code: None,
        shipping_country: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "create",
            "--first-name",
            "Ada",
            "--last-name",
            "Lovelace",
            "--email",
            "ada@example.com",
            "--sms-consent",
            "--billing-line1",
            "123 Test St",
            "--billing-city",
            "Denver",
            "--billing-state",
            "CO",
            "--billing-postal-code",
            "80202",
            "--billing-country",
            "US",
            "--shipping-line1",
            "456 Ship Rd"
        ])
    );
}

/// On `update` the two booleans take a value, so `false` is an instruction
/// rather than silence.
#[tokio::test]
async fn customers_update_emits_valued_booleans() {
    use flute2_mcp::tools::customers::CustomerUpdate;
    let (srv, mock) = sandbox(1);
    srv.customers_update(Parameters(CustomerUpdate {
        id: "c1".into(),
        first_name: None,
        last_name: None,
        company: None,
        email: None,
        mobile: None,
        sms_consent: Some(false),
        use_billing_as_shipping: Some(true),
        billing_line1: None,
        billing_line2: None,
        billing_city: None,
        billing_state: None,
        billing_postal_code: None,
        billing_country: None,
        shipping_line1: None,
        shipping_line2: None,
        shipping_city: None,
        shipping_state: None,
        shipping_postal_code: None,
        shipping_country: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "update",
            "c1",
            "--sms-consent",
            "false",
            "--use-billing-as-shipping",
            "true"
        ])
    );
}

/// The CLI refuses an update naming no field; the server answers the same
/// `client` refusal without spawning it.
#[tokio::test]
async fn customers_update_with_no_fields_is_refused_before_the_spawn() {
    use flute2_mcp::tools::customers::CustomerUpdate;
    let (srv, mock) = sandbox(0);
    let res = srv
        .customers_update(Parameters(CustomerUpdate {
            id: "c1".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    assert_eq!(body(&res)["kind"], "client");
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn customers_delete_passes_yes_and_forwards_the_confirmation() {
    use flute2_mcp::tools::Id;
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "customer",
        "data": { "customerId": "c1", "deleted": true },
        "meta": { "environment": "sandbox" }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let res = srv
        .customers_delete(Parameters(Id { id: "c1".into() }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    // The CLI already answers with a confirmation envelope; it is forwarded
    // whole rather than replaced with a synthesized one.
    assert_eq!(body(&res)["data"]["deleted"], true);
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "delete",
            "c1",
            "--yes"
        ])
    );
}

#[tokio::test]
async fn guarded_production_refuses_customer_writes_without_spawning() {
    use flute2_mcp::tools::Id;
    let (srv, mock) = guarded_production();
    let res = srv
        .customers_delete(Parameters(Id { id: "c1".into() }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    let v = body(&res);
    assert_eq!(v["kind"], "client");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("FLUTE2_MCP_ALLOW_PROD_WRITES")
    );
    assert!(
        mock.calls().is_empty(),
        "the guard must run before the spawn"
    );
}

#[tokio::test]
async fn the_prod_write_override_lifts_the_guard() {
    use flute2_mcp::tools::Id;
    let mock = MockRunner::new(vec![Ok(json!({"object": "customer"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, true, None), mock.clone());
    let res = srv
        .customers_delete(Parameters(Id { id: "c1".into() }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "production",
            "--output",
            "json",
            "customers",
            "delete",
            "c1",
            "--yes"
        ])
    );
}

#[tokio::test]
async fn payment_methods_reads_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::payment_methods::PaymentMethodsList;
    let (srv, mock) = sandbox(2);
    srv.payment_methods_get(Parameters(Id { id: "pm1".into() }))
        .await
        .unwrap();
    srv.payment_methods_list(Parameters(PaymentMethodsList {
        page_index: None,
        page_size: Some(10),
        all: Some(true),
        sort_by: None,
        asc: Some(true),
        desc: None,
        search: Some("visa".into()),
        customer_id: Some("c1".into()),
        created_from: None,
        created_to: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "get",
            "pm1"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "list",
            "--page-size",
            "10",
            "--all",
            "--asc",
            "--search",
            "visa",
            "--customer-id",
            "c1"
        ])
    );
}

#[tokio::test]
async fn payment_methods_add_card_and_add_ach_build_expected_argv() {
    use flute2_mcp::tools::payment_methods::{AddAch, AddCard};
    let (srv, mock) = sandbox(2);
    srv.payment_methods_add_card(Parameters(AddCard {
        card: "4111111111111111".into(),
        exp: "12/26".into(),
        cvv: Some("123".into()),
        customer_id: Some("c1".into()),
        name: Some("Ada's Visa".into()),
    }))
    .await
    .unwrap();
    srv.payment_methods_add_ach(Parameters(AddAch {
        account: "123456789".into(),
        routing: "021000021".into(),
        account_holder_type: "personal".into(),
        account_type: Some("checking".into()),
        tax_id: None,
        customer_id: Some("c1".into()),
        name: None,
        company_name: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "add-card",
            "--card",
            "4111111111111111",
            "--exp",
            "12/26",
            "--cvv",
            "123",
            "--customer-id",
            "c1",
            "--name",
            "Ada's Visa"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "add-ach",
            "--account",
            "123456789",
            "--routing",
            "021000021",
            "--account-holder-type",
            "personal",
            "--account-type",
            "checking",
            "--customer-id",
            "c1"
        ])
    );
}

#[tokio::test]
async fn payment_methods_update_delete_and_set_default_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::payment_methods::{PaymentMethodUpdate, SetDefault};
    let (srv, mock) = sandbox(3);
    srv.payment_methods_update(Parameters(PaymentMethodUpdate {
        id: "pm1".into(),
        name: "Renamed".into(),
    }))
    .await
    .unwrap();
    srv.payment_methods_delete(Parameters(Id { id: "pm1".into() }))
        .await
        .unwrap();
    srv.payment_methods_set_default(Parameters(SetDefault {
        id: "pm1".into(),
        customer_id: "c1".into(),
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "update",
            "pm1",
            "--name",
            "Renamed"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "delete",
            "pm1",
            "--yes"
        ])
    );
    assert_eq!(
        calls[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-methods",
            "set-default",
            "pm1",
            "--customer-id",
            "c1"
        ])
    );
}

#[tokio::test]
async fn guarded_production_refuses_every_payment_method_write() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::payment_methods::{AddAch, AddCard, PaymentMethodUpdate, SetDefault};
    let (srv, mock) = guarded_production();
    let results = vec![
        srv.payment_methods_add_card(Parameters(AddCard {
            card: "4111111111111111".into(),
            exp: "12/26".into(),
            cvv: None,
            customer_id: None,
            name: None,
        }))
        .await
        .unwrap(),
        srv.payment_methods_add_ach(Parameters(AddAch {
            account: "1".into(),
            routing: "2".into(),
            account_holder_type: "personal".into(),
            account_type: None,
            tax_id: None,
            customer_id: None,
            name: None,
            company_name: None,
        }))
        .await
        .unwrap(),
        srv.payment_methods_update(Parameters(PaymentMethodUpdate {
            id: "pm1".into(),
            name: "x".into(),
        }))
        .await
        .unwrap(),
        srv.payment_methods_delete(Parameters(Id { id: "pm1".into() }))
            .await
            .unwrap(),
        srv.payment_methods_set_default(Parameters(SetDefault {
            id: "pm1".into(),
            customer_id: "c1".into(),
        }))
        .await
        .unwrap(),
    ];
    for res in &results {
        assert_eq!(res.is_error, Some(true));
        assert_eq!(body(res)["kind"], "client");
    }
    assert!(
        mock.calls().is_empty(),
        "no guarded write may spawn the CLI"
    );
}

#[tokio::test]
async fn transactions_reads_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::transactions::{CalculateAmount, TransactionsList};
    let (srv, mock) = sandbox(3);
    srv.transactions_get(Parameters(Id { id: "t1".into() }))
        .await
        .unwrap();
    srv.transactions_list(Parameters(TransactionsList {
        page_index: Some(0),
        page_size: Some(25),
        all: None,
        sort_by: None,
        asc: None,
        desc: Some(true),
        from: Some("2026-01-01".into()),
        to: None,
        source_type: None,
        source_id: None,
        batch_id: None,
        status: Some("settled".into()),
        payment_method_type: None,
        customer_id: None,
        merchant_id: None,
        min_amount: Some("10.00".into()),
        max_amount: None,
        reference_id: None,
    }))
    .await
    .unwrap();
    srv.transactions_calculate_amount(Parameters(CalculateAmount {
        amount: "100.00".into(),
        currency_code: Some("USD".into()),
        pricing_type: None,
        tip_amount: None,
        tip_rate: Some("18.5".into()),
        discount_amount: None,
        discount_rate: None,
        surcharge_rate: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "get",
            "t1"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "list",
            "--page-index",
            "0",
            "--page-size",
            "25",
            "--desc",
            "--from",
            "2026-01-01",
            "--status",
            "settled",
            "--min-amount",
            "10.00"
        ])
    );
    assert_eq!(
        calls[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "calculate-amount",
            "--amount",
            "100.00",
            "--currency-code",
            "USD",
            "--tip-rate",
            "18.5"
        ])
    );
}

/// `calculate-amount` is a POST that changes nothing, so it is a read and runs
/// on a guarded production instance.
#[tokio::test]
async fn calculate_amount_is_a_read() {
    use flute2_mcp::tools::transactions::CalculateAmount;
    let mock = MockRunner::new(vec![Ok(json!({"object": "amount_calculation"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    let res = srv
        .transactions_calculate_amount(Parameters(CalculateAmount {
            amount: "1.00".into(),
            currency_code: None,
            pricing_type: None,
            tip_amount: None,
            tip_rate: None,
            discount_amount: None,
            discount_rate: None,
            surcharge_rate: None,
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(mock.calls().len(), 1);
}

#[tokio::test]
async fn transactions_create_builds_a_card_charge_with_level_three_and_addresses() {
    use flute2_mcp::tools::transactions::TransactionCreate;
    let (srv, mock) = sandbox(1);
    srv.transactions_create(Parameters(TransactionCreate {
        amount: "10.50".into(),
        payment_processor_id: "pp1".into(),
        capture_method: Some("manual".into()),
        reference_id: Some("ref-1".into()),
        currency_code: Some("USD".into()),
        customer_id: None,
        card: Some("4111111111111111".into()),
        exp: Some("12/26".into()),
        cvv: Some("123".into()),
        payment_method_id: None,
        instrument: None,
        ach_account_number: None,
        ach_routing_number: None,
        ach_account_type: None,
        ach_account_holder_type: None,
        ach_tax_id: None,
        sec_code: None,
        requester_ip: None,
        same_day: None,
        contact_first_name: Some("Ada".into()),
        contact_last_name: None,
        contact_company: None,
        contact_email: None,
        contact_phone: None,
        contact_sms_consent: None,
        tip_amount: None,
        tip_rate: None,
        discount_amount: None,
        discount_rate: None,
        surcharge_rate: None,
        l2_tax_rate: Some("7.25".into()),
        l3_invoice: Some("INV-9".into()),
        l3_po: None,
        l3_duty_rate: Some("2.5".into()),
        l3_shipping: None,
        l3_product: vec![
            "productName=Widget,quantity=2".into(),
            "productName=Bolt".into(),
        ],
        customer_initiated: Some(true),
        pricing_type: None,
        billing_line1: Some("123 Test St".into()),
        billing_line2: None,
        billing_city: None,
        billing_state: None,
        billing_postal_code: Some("80202".into()),
        billing_country: None,
        shipping_line1: None,
        shipping_line2: None,
        shipping_city: None,
        shipping_state: None,
        shipping_postal_code: None,
        shipping_country: None,
        idempotency_key: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "create",
            "--amount",
            "10.50",
            "--payment-processor-id",
            "pp1",
            "--capture-method",
            "manual",
            "--reference-id",
            "ref-1",
            "--currency-code",
            "USD",
            "--card",
            "4111111111111111",
            "--exp",
            "12/26",
            "--cvv",
            "123",
            "--contact-first-name",
            "Ada",
            "--l2-tax-rate",
            "7.25",
            "--l3-invoice",
            "INV-9",
            "--l3-duty-rate",
            "2.5",
            "--l3-product",
            "productName=Widget,quantity=2",
            "--l3-product",
            "productName=Bolt",
            "--customer-initiated",
            "--billing-line1",
            "123 Test St",
            "--billing-postal-code",
            "80202"
        ])
    );
}

#[tokio::test]
async fn transactions_credit_requires_a_reference_id_and_shares_the_instrument_flags() {
    use flute2_mcp::tools::transactions::TransactionCredit;
    let (srv, mock) = sandbox(1);
    srv.transactions_credit(Parameters(TransactionCredit {
        amount: "500.00".into(),
        payment_processor_id: "pp1".into(),
        reference_id: "ref-2".into(),
        currency_code: None,
        customer_id: Some("c1".into()),
        card: None,
        exp: None,
        cvv: None,
        payment_method_id: Some("pm1".into()),
        instrument: Some("ach".into()),
        ach_account_number: None,
        ach_routing_number: None,
        ach_account_type: None,
        ach_account_holder_type: None,
        ach_tax_id: None,
        sec_code: Some("ppd".into()),
        requester_ip: None,
        same_day: Some(true),
        contact_first_name: None,
        contact_last_name: None,
        contact_company: None,
        contact_email: None,
        contact_phone: Some("+14155552309".into()),
        contact_sms_consent: Some(true),
        billing_line1: None,
        billing_line2: None,
        billing_city: None,
        billing_state: None,
        billing_postal_code: None,
        billing_country: None,
        shipping_line1: None,
        shipping_line2: None,
        shipping_city: None,
        shipping_state: None,
        shipping_postal_code: None,
        shipping_country: None,
        idempotency_key: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "credit",
            "--amount",
            "500.00",
            "--payment-processor-id",
            "pp1",
            "--reference-id",
            "ref-2",
            "--customer-id",
            "c1",
            "--payment-method-id",
            "pm1",
            "--instrument",
            "ach",
            "--sec-code",
            "ppd",
            "--same-day",
            "--contact-phone",
            "+14155552309",
            "--contact-sms-consent"
        ])
    );
}

/// The flag shapes are not regular: `capture`, `reversal` and `tip-adjust` take
/// `--transaction-id`, while `ach-hold`, `ach-release` and `share-receipt` take
/// the id positionally.
#[tokio::test]
async fn transaction_verbs_use_the_flag_shape_each_one_declares() {
    use flute2_mcp::tools::transactions::{ShareReceipt, TipAdjust, TransactionAmount};
    let (srv, mock) = sandbox(6);
    srv.transactions_capture(Parameters(TransactionAmount {
        transaction_id: "t1".into(),
        amount: Some("5.00".into()),
        idempotency_key: Some("k-cap".into()),
    }))
    .await
    .unwrap();
    srv.transactions_reversal(Parameters(TransactionAmount {
        transaction_id: "t1".into(),
        amount: None,
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.transactions_tip_adjust(Parameters(TipAdjust {
        transaction_id: "t1".into(),
        tip_amount: Some("3.50".into()),
        tip_rate: None,
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.transactions_ach_hold(Parameters(flute2_mcp::tools::KeyedId {
        id: "t2".into(),
        idempotency_key: Some("k-hold".into()),
    }))
    .await
    .unwrap();
    srv.transactions_ach_release(Parameters(flute2_mcp::tools::KeyedId {
        id: "t2".into(),
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.transactions_share_receipt(Parameters(ShareReceipt {
        id: "t1".into(),
        share_by: "sms".into(),
        recipient: "+14155552309".into(),
        consent: Some(true),
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    let base = ["--profile", "sandbox", "--output", "json", "transactions"];
    assert_eq!(
        calls[0],
        svec([
            base[0],
            base[1],
            base[2],
            base[3],
            base[4],
            "capture",
            "--transaction-id",
            "t1",
            "--amount",
            "5.00",
            "--idempotency-key",
            "k-cap"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            base[0],
            base[1],
            base[2],
            base[3],
            base[4],
            "reversal",
            "--transaction-id",
            "t1"
        ])
    );
    assert_eq!(
        calls[2],
        svec([
            base[0],
            base[1],
            base[2],
            base[3],
            base[4],
            "tip-adjust",
            "--transaction-id",
            "t1",
            "--tip-amount",
            "3.50"
        ])
    );
    assert_eq!(
        calls[3],
        svec([
            base[0],
            base[1],
            base[2],
            base[3],
            base[4],
            "ach-hold",
            "t2",
            "--idempotency-key",
            "k-hold"
        ])
    );
    assert_eq!(
        calls[4],
        svec([
            base[0],
            base[1],
            base[2],
            base[3],
            base[4],
            "ach-release",
            "t2"
        ])
    );
    assert_eq!(
        calls[5],
        svec([
            base[0],
            base[1],
            base[2],
            base[3],
            base[4],
            "share-receipt",
            "t1",
            "--share-by",
            "sms",
            "--recipient",
            "+14155552309",
            "--consent"
        ])
    );
}

/// A decline is a success with `transactionStatus: "Declined"`, and the tool
/// must forward it as one rather than converting it to an error.
#[tokio::test]
async fn a_declined_charge_is_forwarded_as_a_success() {
    use flute2_mcp::tools::transactions::TransactionCreate;
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "transaction",
        "data": {
            "transactionId": "t9",
            "transactionStatus": "Declined",
            "declineDetails": { "message": "Do not honour" }
        }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock);
    let res = srv
        .transactions_create(Parameters(TransactionCreate {
            amount: "10.00".into(),
            payment_processor_id: "pp1".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(body(&res)["data"]["transactionStatus"], "Declined");
}

#[tokio::test]
async fn payment_links_create_and_list_build_expected_argv() {
    use flute2_mcp::tools::payment_links::{PaymentLinkCreate, PaymentLinksList};
    let (srv, mock) = sandbox(2);
    srv.payment_links_create(Parameters(PaymentLinkCreate {
        currency_code: "USD".into(),
        card_enabled: Some(true),
        card_processor_id: Some("pp-card".into()),
        ach_enabled: Some(false),
        ach_processor_id: None,
        amount: Some("100.00".into()),
        link_type: Some("single-use".into()),
        customer_id: None,
        reference_id: Some("ref-3".into()),
        name: Some("Invoice 9".into()),
        description: None,
        expires_on: None,
        mode: Some("payment-and-save".into()),
        metadata: vec!["order=9".into()],
        tax_amount: None,
        tax_rate: Some("8.5".into()),
        tax_mode: Some("exclusive".into()),
        ach_faster_processing: Some(false),
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.payment_links_list(Parameters(PaymentLinksList {
        page_index: None,
        page_size: None,
        all: None,
        sort_by: None,
        asc: None,
        desc: None,
        search: None,
        link_type: Some("multi-use".into()),
        status: Some("active".into()),
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-links",
            "create",
            "--currency-code",
            "USD",
            "--card-enabled",
            "--card-processor-id",
            "pp-card",
            "--amount",
            "100.00",
            "--link-type",
            "single-use",
            "--reference-id",
            "ref-3",
            "--name",
            "Invoice 9",
            "--mode",
            "payment-and-save",
            "--metadata",
            "order=9",
            "--tax-rate",
            "8.5",
            "--tax-mode",
            "exclusive"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-links",
            "list",
            "--link-type",
            "multi-use",
            "--status",
            "active"
        ])
    );
}

/// On `update` the enable flags take a value and `--clear` repeats.
#[tokio::test]
async fn payment_links_update_emits_valued_toggles_and_repeated_clear() {
    use flute2_mcp::tools::payment_links::PaymentLinkUpdate;
    let (srv, mock) = sandbox(1);
    srv.payment_links_update(Parameters(PaymentLinkUpdate {
        id: "pl1".into(),
        card_enabled: Some(false),
        card_processor_id: None,
        ach_enabled: Some(true),
        ach_processor_id: None,
        amount: None,
        currency_code: None,
        link_type: None,
        status: Some("inactive".into()),
        customer_id: None,
        reference_id: None,
        name: None,
        description: None,
        expires_on: None,
        metadata: vec!["a=1".into(), "b=".into()],
        tax_amount: Some("1.00".into()),
        tax_rate: None,
        tax_mode: Some("inclusive".into()),
        ach_faster_processing: Some(false),
        clear: vec!["amount".into(), "description".into()],
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-links",
            "update",
            "pl1",
            "--card-enabled",
            "false",
            "--ach-enabled",
            "true",
            "--status",
            "inactive",
            "--metadata",
            "a=1",
            "--metadata",
            "b=",
            "--tax-amount",
            "1.00",
            "--tax-mode",
            "inclusive",
            "--ach-faster-processing",
            "false",
            "--clear",
            "amount",
            "--clear",
            "description"
        ])
    );
}

#[tokio::test]
async fn payment_links_delete_and_share_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::payment_links::ShareLink;
    let (srv, mock) = sandbox(2);
    srv.payment_links_delete(Parameters(Id { id: "pl1".into() }))
        .await
        .unwrap();
    srv.payment_links_share(Parameters(ShareLink {
        id: "pl1".into(),
        share_by: "email".into(),
        recipient: "ada@example.com".into(),
        consent: None,
        idempotency_key: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-links",
            "delete",
            "pl1",
            "--yes"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-links",
            "share",
            "pl1",
            "--share-by",
            "email",
            "--recipient",
            "ada@example.com"
        ])
    );
}

#[tokio::test]
async fn payment_sessions_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::payment_sessions::PaymentSessionCreate;
    let (srv, mock) = sandbox(3);
    srv.payment_sessions_create(Parameters(PaymentSessionCreate {
        mode: Some("payment".into()),
        amount: Some("10.50".into()),
        tip_amount: None,
        customer_id: None,
        customer_handling: Some("token-only".into()),
        reference_id: None,
        return_url: None,
        skip_address_verification: Some(true),
        page_name: None,
        payment_notes: None,
        after_completion_message: None,
        expires_at: None,
        metadata: vec!["order=9".into(), "desk=3".into()],
        card_enabled: Some(true),
        card_processor_id: None,
        ach_enabled: Some(true),
        ach_processor_id: None,
        tax_amount: Some("0.50".into()),
        tax_rate: None,
        tax_mode: Some("exclusive".into()),
        ach_faster_processing: Some(true),
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.payment_sessions_get(Parameters(Id { id: "ps1".into() }))
        .await
        .unwrap();
    srv.payment_sessions_cancel(Parameters(flute2_mcp::tools::KeyedId {
        id: "ps1".into(),
        idempotency_key: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-sessions",
            "create",
            "--mode",
            "payment",
            "--amount",
            "10.50",
            "--customer-handling",
            "token-only",
            "--skip-address-verification",
            "--metadata",
            "order=9",
            "--metadata",
            "desk=3",
            "--card-enabled",
            "--ach-enabled",
            "--tax-amount",
            "0.50",
            "--tax-mode",
            "exclusive",
            "--ach-faster-processing"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-sessions",
            "get",
            "ps1"
        ])
    );
    assert_eq!(
        calls[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "payment-sessions",
            "cancel",
            "ps1",
            "--yes"
        ])
    );
}

/// Cancelling an already-cancelled session is a 400, which is exit 3 and a
/// `client` envelope.
#[tokio::test]
async fn cancelling_a_live_session_surfaces_the_state_error() {
    let mock = MockRunner::new(vec![Err(flute2_mcp::error::FluteError::Api {
        status: 400,
        message: "session is not cancellable".into(),
        correlation_id: None,
    })]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock);
    let res = srv
        .payment_sessions_cancel(Parameters(flute2_mcp::tools::KeyedId {
            id: "ps1".into(),
            idempotency_key: None,
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    assert_eq!(body(&res)["status"], 400);
}

/// `--wait` and `--wait-timeout` are never emitted: a long poll would outlive
/// the per-call timeout, so the agent polls `pos_get` instead.
#[tokio::test]
async fn pos_tools_never_emit_wait() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::pos::{PosCreate, PosList, PrintReceipt};
    let (srv, mock) = sandbox(5);
    srv.pos_create(Parameters(PosCreate {
        terminal_id: "term1".into(),
        pos_device_id: "dev1".into(),
        amount: "10.50".into(),
        currency_code: "USD".into(),
        capture_method: Some("auto".into()),
        initiation_channel: None,
        reading_method: None,
        pricing_type: None,
        payment_processor_id: Some("pp1".into()),
        customer_id: None,
        reference_id: Some("ref-4".into()),
        request_storage_consent: Some(true),
        tip_amount: None,
        tip_rate: None,
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.pos_get(Parameters(Id { id: "pt1".into() }))
        .await
        .unwrap();
    srv.pos_list(Parameters(PosList {
        page_index: None,
        page_size: None,
        all: None,
        sort_by: None,
        asc: None,
        desc: None,
        terminal_id: Some("term1".into()),
        from: None,
        to: None,
        status: None,
    }))
    .await
    .unwrap();
    srv.pos_cancel(Parameters(flute2_mcp::tools::KeyedId {
        id: "pt1".into(),
        idempotency_key: None,
    }))
    .await
    .unwrap();
    srv.pos_print_receipt(Parameters(PrintReceipt {
        id: "pt1".into(),
        terminal_id: "term1".into(),
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    for call in &calls {
        assert!(
            !call.iter().any(|a| a == "--wait" || a == "--wait-timeout"),
            "a pos tool emitted a wait flag: {call:?}"
        );
    }
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            "term1",
            "--pos-device-id",
            "dev1",
            "--amount",
            "10.50",
            "--currency-code",
            "USD",
            "--capture-method",
            "auto",
            "--payment-processor-id",
            "pp1",
            "--reference-id",
            "ref-4",
            "--request-storage-consent"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "get",
            "pt1"
        ])
    );
    assert_eq!(
        calls[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "list",
            "--terminal-id",
            "term1"
        ])
    );
    assert_eq!(
        calls[3],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "cancel",
            "pt1",
            "--yes"
        ])
    );
    assert_eq!(
        calls[4],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "print-receipt",
            "pt1",
            "--terminal-id",
            "term1"
        ])
    );
}

#[tokio::test]
async fn terminals_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::terminals::TerminalsList;
    let (srv, mock) = sandbox(2);
    srv.terminals_list(Parameters(TerminalsList {
        page_index: None,
        page_size: Some(5),
        all: None,
        sort_by: None,
        asc: Some(true),
        desc: None,
        status: Some("ready".into()),
        mode: None,
        connection: Some("online".into()),
        serial_number: None,
        search: None,
    }))
    .await
    .unwrap();
    srv.terminals_status(Parameters(Id { id: "term1".into() }))
        .await
        .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "terminals",
            "list",
            "--page-size",
            "5",
            "--asc",
            "--status",
            "ready",
            "--connection",
            "online"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "terminals",
            "status",
            "term1"
        ])
    );
}

#[tokio::test]
async fn guarded_production_refuses_pos_writes_but_allows_pos_reads() {
    use flute2_mcp::tools::Id;
    let (srv, mock) = guarded_production();
    let res = srv
        .pos_cancel(Parameters(flute2_mcp::tools::KeyedId {
            id: "pt1".into(),
            idempotency_key: None,
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    assert!(mock.calls().is_empty());

    let mock = MockRunner::new(vec![Ok(json!({"object": "pos_transaction"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    let res = srv
        .pos_get(Parameters(Id { id: "pt1".into() }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(mock.calls().len(), 1);
}

/// `settlements list` is the one paginated read that spells its sort reversal
/// `--asc` rather than `--desc`.
#[tokio::test]
async fn settlements_list_uses_asc_and_repeats_its_id_filters() {
    use flute2_mcp::tools::settlements::SettlementsList;
    let (srv, mock) = sandbox(1);
    srv.settlements_list(Parameters(SettlementsList {
        page_index: Some(2),
        page_size: None,
        all: None,
        sort_by: Some("batchDate".into()),
        asc: Some(true),
        from: None,
        to: None,
        batch_ids: vec!["b1".into(), "b2".into()],
        processor_ids: vec!["pp1".into()],
        status: Some("settled".into()),
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settlements",
            "list",
            "--page-index",
            "2",
            "--sort-by",
            "batchDate",
            "--asc",
            "--batch-ids",
            "b1",
            "--batch-ids",
            "b2",
            "--processor-ids",
            "pp1",
            "--status",
            "settled"
        ])
    );
}

#[tokio::test]
async fn settlements_get_and_close_build_expected_argv() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::settlements::SettlementClose;
    let (srv, mock) = sandbox(2);
    srv.settlements_get(Parameters(Id { id: "b1".into() }))
        .await
        .unwrap();
    srv.settlements_close(Parameters(SettlementClose {
        payment_processor_id: "pp1".into(),
        idempotency_key: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settlements",
            "get",
            "b1"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settlements",
            "close",
            "--payment-processor-id",
            "pp1"
        ])
    );
}

#[tokio::test]
async fn settings_reads_and_update_build_expected_argv() {
    use flute2_mcp::tools::Empty;
    use flute2_mcp::tools::settings::UpdateAutofill;
    let (srv, mock) = sandbox(4);
    srv.settings_payment_config(Parameters(Empty {}))
        .await
        .unwrap();
    srv.settings_contact_info(Parameters(Empty {}))
        .await
        .unwrap();
    srv.settings_autofill(Parameters(Empty {})).await.unwrap();
    srv.settings_update_autofill(Parameters(UpdateAutofill {
        l2_tax_rate: Some("7.25".into()),
        l3_shipping_rate: None,
        l3_duty_rate: None,
        product_name: Some("Widget".into()),
        product_code: None,
        product_unit: Some("EA".into()),
        product_unit_price: None,
        product_quantity: Some("1".into()),
        product_discount: None,
    }))
    .await
    .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settings",
            "payment-config"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settings",
            "contact-info"
        ])
    );
    assert_eq!(
        calls[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settings",
            "autofill"
        ])
    );
    assert_eq!(
        calls[3],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settings",
            "update-autofill",
            "--l2-tax-rate",
            "7.25",
            "--product-name",
            "Widget",
            "--product-unit",
            "EA",
            "--product-quantity",
            "1"
        ])
    );
}

#[tokio::test]
async fn guarded_production_refuses_close_and_update_autofill() {
    use flute2_mcp::tools::settings::UpdateAutofill;
    use flute2_mcp::tools::settlements::SettlementClose;
    let (srv, mock) = guarded_production();
    let close = srv
        .settlements_close(Parameters(SettlementClose {
            payment_processor_id: "pp1".into(),
            idempotency_key: None,
        }))
        .await
        .unwrap();
    let autofill = srv
        .settings_update_autofill(Parameters(UpdateAutofill::default()))
        .await
        .unwrap();
    assert_eq!(close.is_error, Some(true));
    assert_eq!(autofill.is_error, Some(true));
    assert!(mock.calls().is_empty());
}

/// `list` treats the merchant id as an optional filter: the per-call value
/// wins, then the pinned one, and with neither the flag is left off.
#[tokio::test]
async fn api_keys_list_resolves_the_merchant_filter_in_order() {
    use flute2_mcp::tools::api_keys::ApiKeysList;
    let mock = MockRunner::new(vec![
        Ok(json!({"object": "api_token_list"})),
        Ok(json!({"object": "api_token_list"})),
    ]);
    let pinned = FluteServer::new(cfg(Profile::Sandbox, false, Some("m-pinned")), mock.clone());
    pinned
        .api_keys_list(Parameters(ApiKeysList {
            merchant_id: Some("m-call".into()),
        }))
        .await
        .unwrap();
    pinned
        .api_keys_list(Parameters(ApiKeysList { merchant_id: None }))
        .await
        .unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "api-keys",
            "list",
            "--merchant-id",
            "m-call"
        ])
    );
    assert_eq!(
        calls[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "api-keys",
            "list",
            "--merchant-id",
            "m-pinned"
        ])
    );

    let bare_mock = MockRunner::new(vec![Ok(json!({"object": "api_token_list"}))]);
    let bare = FluteServer::new(cfg(Profile::Sandbox, false, None), bare_mock.clone());
    bare.api_keys_list(Parameters(ApiKeysList { merchant_id: None }))
        .await
        .unwrap();
    assert_eq!(
        bare_mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "api-keys",
            "list"
        ])
    );
}

/// `create` requires one. With neither source the tool answers a `client`
/// error and never spawns.
#[tokio::test]
async fn api_keys_create_requires_a_merchant_id() {
    use flute2_mcp::tools::api_keys::ApiKeyCreate;
    let mock = MockRunner::new(vec![Ok(json!({"object": "api_token"}))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, Some("m-pinned")), mock.clone());
    srv.api_keys_create(Parameters(ApiKeyCreate {
        name: "ci runner".into(),
        merchant_id: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "api-keys",
            "create",
            "--merchant-id",
            "m-pinned",
            "--name",
            "ci runner"
        ])
    );

    let bare_mock = MockRunner::new(vec![]);
    let bare = FluteServer::new(cfg(Profile::Sandbox, false, None), bare_mock.clone());
    let res = bare
        .api_keys_create(Parameters(ApiKeyCreate {
            name: "ci runner".into(),
            merchant_id: None,
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    let v = body(&res);
    assert_eq!(v["kind"], "client");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("FLUTE2_MERCHANT_ID")
    );
    assert!(bare_mock.calls().is_empty());
}

/// `revoke` takes no merchant id at all in v2.
#[tokio::test]
async fn api_keys_revoke_sends_only_the_client_id_and_yes() {
    use flute2_mcp::tools::api_keys::ApiKeyRevoke;
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "api_token",
        "data": { "clientId": "cid-1", "revoked": true }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, Some("m-pinned")), mock.clone());
    let res = srv
        .api_keys_revoke(Parameters(ApiKeyRevoke {
            client_id: "cid-1".into(),
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(body(&res)["data"]["revoked"], true);
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "api-keys",
            "revoke",
            "--client-id",
            "cid-1",
            "--yes"
        ])
    );
}

/// Calls each named tool with the given params, pairing every result with the
/// tool's own name so an assertion can name the tool that failed.
macro_rules! call_each {
    ($srv:expr, $($tool:ident($params:expr)),+ $(,)?) => {
        vec![$((stringify!($tool), $srv.$tool(Parameters($params)).await.unwrap())),+]
    };
}

/// Every write tool refuses on a production profile without
/// `FLUTE2_MCP_ALLOW_PROD_WRITES`, with the mandated message naming itself,
/// and none of them reaches the CLI.
#[tokio::test]
async fn every_write_refuses_on_guarded_production_without_spawning() {
    use flute2_mcp::tools::Id;
    use flute2_mcp::tools::api_keys::{ApiKeyCreate, ApiKeyRevoke};
    use flute2_mcp::tools::customers::{CustomerCreate, CustomerUpdate};
    use flute2_mcp::tools::payment_links::{PaymentLinkCreate, PaymentLinkUpdate, ShareLink};
    use flute2_mcp::tools::payment_methods::{AddAch, AddCard, PaymentMethodUpdate, SetDefault};
    use flute2_mcp::tools::payment_sessions::PaymentSessionCreate;
    use flute2_mcp::tools::pos::{PosCreate, PrintReceipt};
    use flute2_mcp::tools::settings::UpdateAutofill;
    use flute2_mcp::tools::settlements::SettlementClose;
    use flute2_mcp::tools::transactions::{
        ShareReceipt, TipAdjust, TransactionAmount, TransactionCreate, TransactionCredit,
    };

    let (srv, mock) = guarded_production();
    let results = call_each!(
        srv,
        customers_create(CustomerCreate::default()),
        customers_update(CustomerUpdate::default()),
        customers_delete(Id::default()),
        payment_methods_add_card(AddCard::default()),
        payment_methods_add_ach(AddAch::default()),
        payment_methods_update(PaymentMethodUpdate::default()),
        payment_methods_delete(Id::default()),
        payment_methods_set_default(SetDefault::default()),
        transactions_create(TransactionCreate::default()),
        transactions_credit(TransactionCredit::default()),
        transactions_capture(TransactionAmount::default()),
        transactions_reversal(TransactionAmount::default()),
        transactions_tip_adjust(TipAdjust::default()),
        transactions_ach_hold(flute2_mcp::tools::KeyedId::default()),
        transactions_ach_release(flute2_mcp::tools::KeyedId::default()),
        transactions_share_receipt(ShareReceipt::default()),
        payment_links_create(PaymentLinkCreate::default()),
        payment_links_update(PaymentLinkUpdate::default()),
        payment_links_delete(Id::default()),
        payment_links_share(ShareLink::default()),
        payment_sessions_create(PaymentSessionCreate::default()),
        payment_sessions_cancel(flute2_mcp::tools::KeyedId::default()),
        pos_create(PosCreate::default()),
        pos_cancel(flute2_mcp::tools::KeyedId::default()),
        pos_print_receipt(PrintReceipt::default()),
        settlements_close(SettlementClose::default()),
        settings_update_autofill(UpdateAutofill::default()),
        api_keys_create(ApiKeyCreate::default()),
        api_keys_revoke(ApiKeyRevoke::default()),
    );
    assert_eq!(results.len(), 29);

    for (tool, res) in &results {
        assert_eq!(res.is_error, Some(true), "{tool}");
        let v = body(res);
        assert_eq!(v["kind"], "client", "{tool}");
        assert_eq!(
            v["message"],
            format!(
                "refusing {tool} on production profile; \
                 set FLUTE2_MCP_ALLOW_PROD_WRITES=1 to enable writes"
            ),
            "{tool}"
        );
    }
    assert!(mock.calls().is_empty());
}

/// The guard covers writes only: every read still runs on that same production
/// profile, `transactions_calculate_amount` among them.
#[tokio::test]
async fn every_read_runs_on_guarded_production() {
    use flute2_mcp::tools::api_keys::ApiKeysList;
    use flute2_mcp::tools::customers::CustomersList;
    use flute2_mcp::tools::payment_links::PaymentLinksList;
    use flute2_mcp::tools::payment_methods::PaymentMethodsList;
    use flute2_mcp::tools::pos::PosList;
    use flute2_mcp::tools::settlements::SettlementsList;
    use flute2_mcp::tools::terminals::TerminalsList;
    use flute2_mcp::tools::transactions::{CalculateAmount, TransactionsList};
    use flute2_mcp::tools::{Empty, Id};

    let mock = MockRunner::new((0..23).map(|_| Ok(json!({"object": "x"}))).collect());
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    let results = call_each!(
        srv,
        ping(Empty {}),
        version(Empty {}),
        auth_status(Empty {}),
        customers_get(Id::default()),
        customers_list(CustomersList::default()),
        payment_methods_get(Id::default()),
        payment_methods_list(PaymentMethodsList::default()),
        transactions_get(Id::default()),
        transactions_list(TransactionsList::default()),
        transactions_calculate_amount(CalculateAmount::default()),
        payment_links_get(Id::default()),
        payment_links_list(PaymentLinksList::default()),
        payment_sessions_get(Id::default()),
        pos_get(Id::default()),
        pos_list(PosList::default()),
        terminals_list(TerminalsList::default()),
        terminals_status(Id::default()),
        settlements_list(SettlementsList::default()),
        settlements_get(Id::default()),
        settings_payment_config(Empty {}),
        settings_contact_info(Empty {}),
        settings_autofill(Empty {}),
        api_keys_list(ApiKeysList::default()),
    );
    assert_eq!(results.len(), 23);

    for (tool, res) in &results {
        assert_eq!(res.is_error, Some(false), "{tool}");
    }
    assert_eq!(mock.calls().len(), 23);
}

/// A value the CLI's parser reads as an option is refused before the argv is
/// assembled, and the refusal names the field and the value so the caller can
/// correct it.
#[test]
fn the_screen_refuses_an_option_shaped_value() {
    for value in [
        "--profile=production",
        "--output=table",
        "--help",
        "-h",
        "-pproduction",
    ] {
        let args = json!({ "id": value }).as_object().unwrap().clone();
        let Err(FluteError::Client { message }) = screen_arguments(&args) else {
            panic!("{value} must be refused as a client error");
        };
        assert!(message.contains("`id`"), "{message}");
        assert!(message.contains(value), "{message}");
    }
}

/// Fields that carry a list of values are screened element by element, and the
/// message names the element.
#[test]
fn the_screen_reaches_into_arrays_and_nested_objects() {
    let args = json!({ "clear": ["amount", "--output=table"] })
        .as_object()
        .unwrap()
        .clone();
    let Err(FluteError::Client { message }) = screen_arguments(&args) else {
        panic!("an option inside an array must be refused");
    };
    assert!(message.contains("`clear[1]`"), "{message}");

    let nested = json!({ "meta": { "note": "--profile=production" } })
        .as_object()
        .unwrap()
        .clone();
    let Err(FluteError::Client { message }) = screen_arguments(&nested) else {
        panic!("an option inside a nested object must be refused");
    };
    assert!(message.contains("`meta.note`"), "{message}");
}

/// Ordinary caller values pass the screen — including a negative amount, which
/// the CLI's own parser answers for — and the call reaches the runner with its
/// argv unchanged.
#[tokio::test]
async fn ordinary_values_pass_the_screen_and_reach_the_runner() {
    let args = json!({
        "id": "cus_01HZ",
        "first_name": "Jean-Luc",
        "search": "refund - urgent",
        "amount": "-5.00",
        "page_size": 25,
        "all": true,
        "clear": ["amount", "reference-id"],
    });
    screen_arguments(args.as_object().unwrap()).expect("ordinary values pass");

    let (srv, mock) = sandbox(1);
    srv.customers_get(Parameters(Id {
        id: "cus_01HZ".into(),
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "get",
            "cus_01HZ",
        ])
    );
}

#[tokio::test]
async fn pos_reversal_builds_expected_argv() {
    use flute2_mcp::tools::pos::PosReversal;
    let (srv, mock) = sandbox(1);
    srv.pos_reversal(Parameters(PosReversal {
        terminal_id: "t1".into(),
        pos_device_id: "POS-1".into(),
        reference_id: "rev-1".into(),
        original_transaction_id: Some("tx1".into()),
        amount: Some("2.50".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "reversal",
            "--terminal-id",
            "t1",
            "--pos-device-id",
            "POS-1",
            "--reference-id",
            "rev-1",
            "--original-transaction-id",
            "tx1",
            "--amount",
            "2.50"
        ])
    );
}
