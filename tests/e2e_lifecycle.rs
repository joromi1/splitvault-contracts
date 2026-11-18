//! End-to-end invoice lifecycle scenarios using the native contract client.

use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Events as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    vec,
    xdr::{ContractEventBody, ScVal},
    Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

use split::{
    types::{InvoiceOptions, InvoiceOptions2, InvoiceStatus, OverflowBehavior},
    SplitContract, SplitContractClient,
};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let contract_id = env.register(SplitContract, ());
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    SplitContractClient::new(&env, &contract_id)
        .initialize(&admin, &0, &treasury, &token_id, &0, &None, &0, &0, &0);

    (env, contract_id, token_id, admin)
}

fn default_options(env: &Env) -> InvoiceOptions {
    InvoiceOptions {
        co_creators: Vec::new(env),
        allow_early_withdrawal: false,
        bonus_pool: 0,
        bonus_max_payers: 0,
        creator_cosigner: None,
        velocity_limit: 0,
        velocity_window: 0,
        prerequisite_id: None,
        tranches: Vec::new(env),
        co_signers: Vec::new(env),
        required_signatures: 0,
        penalty_bps: None,
        penalty_deadline: None,
        min_funding_bps: None,
        release_stages: Vec::new(env),
        price_oracle: None,
        swap_tokens: Vec::new(env),
        tax_bps: None,
        tax_authority: None,
        insurance_premium_bps: None,
        smart_route: None,
        notification_contract: None,
        overflow_behavior: OverflowBehavior::Reject,
        convert_to_stream: false,
        accepted_tokens: Vec::new(env),
        forward_to: None,
        forward_invoice_id: None,
        split_rules: Vec::new(env),
        auto_resolve_rules: Vec::new(env),
        condition_oracle: None,
        cross_chain_ref: None,
        allowed_payers: None,
        refund_grace_secs: None,
        priorities: Vec::new(env),
        require_kyc: false,
        scheduled_release_at: None,
        ratios: Vec::new(env),
        cosigners: None,
        cosigner_threshold: None,
        ext: InvoiceOptions2::default(),
    }
}

fn assert_event(env: &Env, contract_id: &Address, topics: Vec<Val>) {
    let expected: std::vec::Vec<_> = topics
        .iter()
        .map(|topic| ScVal::try_from_val(env, &topic).unwrap())
        .collect();
    let events = env.events().all().filter_by_contract(contract_id);
    assert!(
        events.events().iter().any(|event| {
            let ContractEventBody::V0(body) = &event.body;
            body.topics.as_slice() == expected.as_slice()
        }),
        "missing contract event with topics {expected:?}"
    );
}

// ---------------------------------------------------------------------------
// Issue #529 — Full lifecycle: 3 contributors, 3 recipients
// ---------------------------------------------------------------------------

/// End-to-end test:
/// 1. Deploy contract and initialise with zero fees.
/// 2. Create an invoice split across 3 recipients (100 / 200 / 300 = 600 total).
/// 3. Three contributors each fund a portion of the invoice.
/// 4. Verify that once the invoice is fully funded it auto-releases.
/// 5. Verify token balances of all three recipients after payout.
/// 6. Verify the invoice status is Released.
/// 7. Verify at least one `invoice_released` event was emitted.
#[test]
fn test_full_invoice_lifecycle_three_contributors() {
    let (env, contract_id, token_id, _admin) = setup();
    let c = SplitContractClient::new(&env, &contract_id);
    let tk = TokenClient::new(&env, &token_id);
    let sa = StellarAssetClient::new(&env, &token_id);

    // ---- Participants --------------------------------------------------------
    let creator = Address::generate(&env);
    let recipient_a = Address::generate(&env);
    let recipient_b = Address::generate(&env);
    let recipient_c = Address::generate(&env);

    let contributor_1 = Address::generate(&env);
    let contributor_2 = Address::generate(&env);
    let contributor_3 = Address::generate(&env);

    sa.mint(&contributor_1, &200);
    sa.mint(&contributor_2, &200);
    sa.mint(&contributor_3, &200);

    // ---- Balances before invoice --------------------------------------------
    let bal_a_before = tk.balance(&recipient_a); // 0
    let bal_b_before = tk.balance(&recipient_b); // 0
    let bal_c_before = tk.balance(&recipient_c); // 0
    assert_eq!(bal_a_before, 0);
    assert_eq!(bal_b_before, 0);
    assert_eq!(bal_c_before, 0);

    // ---- Create invoice -----------------------------------------------------
    env.ledger().set_timestamp(1_000);
    let deadline: u64 = 99_999;

    let mut recipients = Vec::new(&env);
    recipients.push_back(recipient_a.clone());
    recipients.push_back(recipient_b.clone());
    recipients.push_back(recipient_c.clone());

    let mut amounts = Vec::new(&env);
    amounts.push_back(100_i128);
    amounts.push_back(200_i128);
    amounts.push_back(300_i128);

    // Use default options — build a minimal InvoiceOptions value matching the
    // contract's expected type.  The contract enforces a minimum of 2 recipients
    // by default (Issue #526), so 3 recipients satisfies that constraint.
    let invoice_id = c.create_invoice(
        &creator,
        &recipients,
        &amounts,
        &token_id,
        &deadline,
        &default_options(&env),
    );

    // Invoice created successfully.
    assert_eq!(invoice_id, 1);

    // Verify initial state.
    let invoice = c.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.funded, 0);

    // ---- Contributor 1 pays 200 (partial) -----------------------------------
    c.pay(
        &contributor_1,
        &invoice_id,
        &200_i128,
        &0_u64, // nonce
        &false, // auto_convert
        &false, // donate_on_failure
        &None,  // commitment
    );

    let invoice = c.get_invoice(&invoice_id);
    assert_eq!(invoice.funded, 200);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // ---- Contributor 2 pays 200 (partial) -----------------------------------
    c.pay(
        &contributor_2,
        &invoice_id,
        &200_i128,
        &0_u64,
        &false,
        &false,
        &None,
    );

    let invoice = c.get_invoice(&invoice_id);
    assert_eq!(invoice.funded, 400);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // ---- Contributor 3 pays 200 — reaches total of 600, triggers auto-release
    c.pay(
        &contributor_3,
        &invoice_id,
        &200_i128,
        &0_u64,
        &false,
        &false,
        &None,
    );

    assert_event(
        &env,
        &contract_id,
        vec![
            &env,
            Symbol::new(&env, "split").into_val(&env),
            Symbol::new(&env, "released").into_val(&env),
            invoice_id.into_val(&env),
        ],
    );

    // ---- Verify invoice is fully released -----------------------------------
    let invoice = c.get_invoice(&invoice_id);
    assert_eq!(invoice.funded, 600);
    assert_eq!(invoice.status, InvoiceStatus::Released);

    // ---- Verify recipient balances after auto-release -----------------------
    assert_eq!(tk.balance(&recipient_a), 100, "recipient_a should have 100");
    assert_eq!(tk.balance(&recipient_b), 200, "recipient_b should have 200");
    assert_eq!(tk.balance(&recipient_c), 300, "recipient_c should have 300");

    // ---- Verify contract holds no residual balance --------------------------
    assert_eq!(
        tk.balance(&contract_id),
        0,
        "contract should have zero balance after release"
    );
}

// ---------------------------------------------------------------------------
// Verify refund path: deadline passes before full funding
// ---------------------------------------------------------------------------

/// When the deadline passes and the invoice is not fully funded, contributors
/// can be refunded via `refund()`.
#[test]
fn test_refund_after_deadline() {
    let (env, contract_id, token_id, _admin) = setup();
    let c = SplitContractClient::new(&env, &contract_id);
    let tk = TokenClient::new(&env, &token_id);
    let sa = StellarAssetClient::new(&env, &token_id);

    let creator = Address::generate(&env);
    let recipient_a = Address::generate(&env);
    let recipient_b = Address::generate(&env);
    let contributor = Address::generate(&env);

    sa.mint(&contributor, &50);

    // Set up timeline: current timestamp = 1_000, deadline = 5_000.
    env.ledger().set_timestamp(1_000);
    let deadline: u64 = 5_000;

    let mut recipients = Vec::new(&env);
    recipients.push_back(recipient_a.clone());
    recipients.push_back(recipient_b.clone());
    let mut amounts = Vec::new(&env);
    amounts.push_back(100_i128);
    amounts.push_back(100_i128);

    let invoice_id = c.create_invoice(
        &creator,
        &recipients,
        &amounts,
        &token_id,
        &deadline,
        &default_options(&env),
    );

    // Partially fund the invoice (50 of 200).
    c.pay(
        &contributor,
        &invoice_id,
        &50_i128,
        &0_u64,
        &false,
        &false,
        &None,
    );
    assert_eq!(tk.balance(&contributor), 0);

    // Fast-forward past the deadline.
    env.ledger().set_timestamp(6_000);

    // Refund should succeed.
    c.refund(&invoice_id);

    // Contributor recovers their 50 tokens.
    assert_eq!(
        tk.balance(&contributor),
        50,
        "contributor should be refunded"
    );

    // Invoice should be in Refunded state.
    let invoice = c.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
}

// ---------------------------------------------------------------------------
// Verify admin transfer events (Issue #528)
// ---------------------------------------------------------------------------

/// Verify that propose_admin and accept_admin emit the correct events.
#[test]
fn test_admin_transfer_events() {
    let (env, contract_id, _token_id, admin) = setup();
    let c = SplitContractClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);
    let namespace = Symbol::new(&env, "split");

    c.propose_admin(&admin, &new_admin);
    assert_event(
        &env,
        &contract_id,
        vec![
            &env,
            namespace.into_val(&env),
            Symbol::new(&env, "adm_prop").into_val(&env),
        ],
    );

    c.accept_admin();
    assert_event(
        &env,
        &contract_id,
        vec![
            &env,
            namespace.into_val(&env),
            Symbol::new(&env, "adm_done").into_val(&env),
        ],
    );
}
