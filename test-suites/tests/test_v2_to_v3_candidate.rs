#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Vec};
use test_suites::{
    backstop::create_backstop,
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

/// Establishes the v2-to-v3 emitter-migration harness.
///
/// Until the v3 backstop exists, a second v2 backstop is used as the candidate.
/// The candidate constructor in this test is the replacement point for the
/// first v3 backstop implementation.
#[test]
fn test_v2_to_v3_candidate_migration_baseline() {
    let fixture = TestFixture::create(false);
    let env = &fixture.env;
    let depositor = Address::generate(env);
    let candidate_id = Address::generate(env);

    let _candidate = create_backstop(
        env,
        &candidate_id,
        false,
        &fixture.lp.address,
        &fixture.emitter.address,
        &fixture.tokens[TokenIndex::BLND].address,
        &fixture.tokens[TokenIndex::USDC].address,
        &fixture.pool_factory.address,
        &Vec::new(env),
    );

    // Mint enough Comet LP for both the incumbent and candidate. The emitter
    // compares their direct token balances rather than internal backstop shares.
    fixture.tokens[TokenIndex::BLND].mint(&depositor, &(20_000 * SCALAR_7));
    fixture.tokens[TokenIndex::USDC].mint(&depositor, &(500 * SCALAR_7));
    fixture.lp.join_pool(
        &(1_000 * SCALAR_7),
        &vec![env, 20_000 * SCALAR_7, 500 * SCALAR_7],
        &depositor,
    );

    let incumbent_balance = 100 * SCALAR_7;
    let candidate_balance = incumbent_balance + 1;
    fixture
        .lp
        .transfer(&depositor, &fixture.backstop.address, &incumbent_balance);
    fixture
        .lp
        .transfer(&depositor, &candidate_id, &candidate_balance);

    // The candidate wins this migration using the incumbent BLND:USDC token,
    // while choosing XLM as the token used to compare the next migration.
    let next_designated_token = fixture.tokens[TokenIndex::XLM].address.clone();
    fixture
        .emitter
        .queue_swap_backstop(&candidate_id, &next_designated_token);

    let queued_swap = fixture.emitter.get_queued_swap().unwrap();
    assert_eq!(queued_swap.new_backstop, candidate_id);
    assert_eq!(queued_swap.new_backstop_token, next_designated_token);

    fixture.jump(31 * 24 * 60 * 60 + 1);
    fixture.emitter.swap_backstop();

    assert_eq!(fixture.emitter.get_backstop(), candidate_id);
    assert!(fixture.emitter.get_queued_swap().is_none());
}
