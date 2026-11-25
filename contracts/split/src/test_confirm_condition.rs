#![cfg(test)]

//! Regression coverage for issue #277: repeated payment-condition
//! confirmations must not produce duplicate audit-trail transition entries.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

fn conditional_invoice(
    env: &Env,
    c: &SplitContractClient,
    creator: &Address,
    recipient: &Address,
    oracle: &Address,
    token_id: &Address,
) -> u64 {
    let mut recipients = Vec::new(env);
    recipients.push_back(recipient.clone());
    let mut amounts = Vec::new(env);
    amounts.push_back(100_i128);
    let mut options = default_options(env);
    options.condition_oracle = Some(oracle.clone());
    c.create_invoice(
        creator,
        &recipients,
        &amounts,
        token_id,
        &9_999,
        &options,
    )
}

#[test]
fn test_confirm_condition_appends_one_transition_entry() {
    let (env, contract_id, token_id) = setup_initialized();
    let c = client(&env, &contract_id);
    let creator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let id = conditional_invoice(&env, &c, &creator, &recipient, &oracle, &token_id);

    c.confirm_condition(&id);

    assert!(c.get_invoice(&id).condition_met);
    let log = c.get_audit_log(&id);
    assert_eq!(log.len(), 1);
    assert_eq!(log.get_unchecked(0).action, symbol_short!("oracle_ok"));
    assert_eq!(log.get_unchecked(0).actor, oracle);
    assert_eq!(log.get_unchecked(0).timestamp, 1_000);
}

#[test]
fn test_repeated_confirm_condition_is_a_noop() {
    let (env, contract_id, token_id) = setup_initialized();
    let c = client(&env, &contract_id);
    let creator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.ledger().set_timestamp(1_000);
    let id = conditional_invoice(&env, &c, &creator, &recipient, &oracle, &token_id);
    c.confirm_condition(&id);

    let events_after_first = env.events().all().len();
    let log_after_first = c.get_audit_log(&id);

    // Repeats at later ledger timestamps remain no-ops: no state write, no
    // second audit row and no new contract event.
    env.ledger().set_timestamp(2_000);
    c.confirm_condition(&id);
    env.ledger().set_timestamp(3_000);
    c.confirm_condition(&id);

    assert!(c.get_invoice(&id).condition_met);
    assert_eq!(env.events().all().len(), events_after_first);

    let log = c.get_audit_log(&id);
    assert_eq!(log.len(), log_after_first.len());
    assert_eq!(log.len(), 1);
    assert_eq!(log.get_unchecked(0).action, symbol_short!("oracle_ok"));
    assert_eq!(log.get_unchecked(0).timestamp, 1_000);
}

#[test]
fn test_repeat_noop_is_scoped_to_the_invoice() {
    let (env, contract_id, token_id) = setup_initialized();
    let c = client(&env, &contract_id);
    let creator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let oracle = Address::generate(&env);

    let first = conditional_invoice(&env, &c, &creator, &recipient, &oracle, &token_id);
    let second = conditional_invoice(&env, &c, &creator, &recipient, &oracle, &token_id);

    c.confirm_condition(&first);
    c.confirm_condition(&first);
    c.confirm_condition(&second);

    assert_eq!(c.get_audit_log(&first).len(), 1);
    assert_eq!(c.get_audit_log(&second).len(), 1);
    assert!(c.get_invoice(&first).condition_met);
    assert!(c.get_invoice(&second).condition_met);
}
