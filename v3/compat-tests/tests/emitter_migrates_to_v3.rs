// blend-contract-sdk 1.22.0 embeds the published legacy emitter WASM with
// SHA-256 438a5528cff17ede6fe515f095c43c5f15727af17d006971485e52462e7e7b89.
use blend_contract_sdk::emitter::WASM as LEGACY_EMITTER_WASM;
use soroban_sdk::{
    contractclient, contracttype,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

mod v3_backstop {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/optimized/blend_v3_backstop.wasm");
}

#[derive(Clone)]
#[contracttype]
struct QueuedSwap {
    new_backstop: Address,
    new_backstop_token: Address,
    unlock_time: u64,
}

#[contractclient(name = "LegacyEmitterClient")]
#[allow(dead_code)]
trait LegacyEmitter {
    fn initialize(env: Env, blnd_token: Address, backstop: Address, backstop_token: Address);
    fn distribute(env: Env) -> i128;
    fn get_last_distro(env: Env, backstop_id: Address) -> u64;
    fn get_backstop(env: Env) -> Address;
    fn get_queued_swap(env: Env) -> Option<QueuedSwap>;
    fn queue_swap_backstop(env: Env, new_backstop: Address, new_backstop_token: Address);
    fn cancel_swap_backstop(env: Env);
    fn swap_backstop(env: Env);
}

const SCALAR_7: i128 = 10_000_000;
const QUEUE_SECONDS: u64 = 31 * 24 * 60 * 60;
const PREPARATION_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;
const SYNC_GRACE_SECONDS: u64 = 7 * 24 * 60 * 60;
const ABSOLUTE_MIGRATION_CEILING_SECONDS: u64 = 138 * 24 * 60 * 60;

struct Fixture {
    env: Env,
    incumbent: Address,
    candidate: v3_backstop::Client<'static>,
    emitter: LegacyEmitterClient<'static>,
    blnd: TokenClient<'static>,
    legacy_token: StellarAssetClient<'static>,
    next_token: Address,
    prefund_token: Address,
}

impl Fixture {
    fn create() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let token_admin = Address::generate(&env);
        let blnd_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let legacy_token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let next_token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let prefund_token_id = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();

        let incumbent = Address::generate(&env);
        let emitter_id = env.register(LEGACY_EMITTER_WASM, ());
        StellarAssetClient::new(&env, &blnd_id).set_admin(&emitter_id);
        let emitter = LegacyEmitterClient::new(&env, &emitter_id);
        emitter.initialize(&blnd_id, &incumbent, &legacy_token_id);

        let candidate_id = env.register(
            v3_backstop::WASM,
            (
                &emitter_id,
                &legacy_token_id,
                &next_token_id,
                &prefund_token_id,
            ),
        );
        let candidate = v3_backstop::Client::new(&env, &candidate_id);
        let blnd = TokenClient::new(&env, &blnd_id);
        let legacy_token = StellarAssetClient::new(&env, &legacy_token_id);

        Self {
            env,
            incumbent,
            candidate,
            emitter,
            blnd,
            legacy_token,
            next_token: next_token_id,
            prefund_token: prefund_token_id,
        }
    }

    fn fund_qualifying_balances(&self) {
        self.legacy_token.mint(&self.incumbent, &(100 * SCALAR_7));
        self.legacy_token
            .mint(&self.candidate.address, &(100 * SCALAR_7 + 1));
    }

    fn queue(&self) {
        self.candidate.begin_migration();
    }

    fn queue_direct(&self) {
        self.emitter
            .queue_swap_backstop(&self.candidate.address, &self.next_token);
    }

    fn enter_preparation_window(&self) {
        let unlock_time = self.emitter.get_queued_swap().unwrap().unlock_time;
        self.env
            .ledger()
            .set_timestamp(unlock_time - PREPARATION_WINDOW_SECONDS);
    }

    fn unlock(&self) {
        let unlock_time = self.emitter.get_queued_swap().unwrap().unlock_time;
        self.env.ledger().set_timestamp(unlock_time);
    }
}

#[test]
fn legacy_emitter_finalizes_migration_to_v3_wasm() {
    let fixture = Fixture::create();
    assert_eq!(
        fixture.candidate.backstop_token(),
        fixture.legacy_token.address
    );
    assert_eq!(fixture.candidate.next_backstop_token(), fixture.next_token);
    assert_eq!(fixture.candidate.prefund_token(), fixture.prefund_token);
    assert_ne!(fixture.candidate.prefund_token(), fixture.next_token);
    assert_eq!(fixture.candidate.emitter(), fixture.emitter.address);
    assert_eq!(
        fixture.candidate.absolute_migration_deadline(),
        1_000 + ABSOLUTE_MIGRATION_CEILING_SECONDS
    );
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let original_unlock = fixture.candidate.prepare_migration();
    fixture.unlock();

    let finalization_timestamp = fixture.env.ledger().timestamp();
    assert_eq!(original_unlock, finalization_timestamp);
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Prepared
    );

    assert_eq!(
        fixture.candidate.finalize_migration(),
        finalization_timestamp
    );
    assert_eq!(fixture.emitter.get_backstop(), fixture.candidate.address);
    assert_eq!(
        fixture.emitter.get_last_distro(&fixture.candidate.address),
        finalization_timestamp
    );
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Active
    );
    assert_eq!(
        fixture.candidate.original_unlock(),
        Some(finalization_timestamp)
    );
    assert_eq!(
        fixture.candidate.backfill_end(),
        Some(finalization_timestamp)
    );
    assert_eq!(fixture.candidate.migration_epoch_start(), Some(1_000));
    assert_eq!(
        fixture.candidate.scheduled_backfill(),
        QUEUE_SECONDS as i128 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.activated_at(),
        Some(finalization_timestamp)
    );

    let incumbent_emissions = (finalization_timestamp - 1_000) as i128 * SCALAR_7;
    assert_eq!(
        fixture.blnd.balance(&fixture.incumbent),
        incumbent_emissions
    );
    assert_eq!(fixture.blnd.balance(&fixture.candidate.address), 0);

    fixture
        .env
        .ledger()
        .set_timestamp(finalization_timestamp.checked_add(1).unwrap());
    assert_eq!(fixture.emitter.distribute(), SCALAR_7);
    assert_eq!(fixture.blnd.balance(&fixture.candidate.address), SCALAR_7);

    let successor = Address::generate(&fixture.env);
    StellarAssetClient::new(&fixture.env, &fixture.next_token).mint(&successor, &1);
    fixture
        .emitter
        .queue_swap_backstop(&successor, &fixture.next_token);
    assert!(fixture.candidate.try_finalize_migration().is_err());
}

#[test]
fn emitter_rejects_equal_balance() {
    let fixture = Fixture::create();
    fixture
        .legacy_token
        .mint(&fixture.incumbent, &(100 * SCALAR_7));
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(100 * SCALAR_7));

    assert!(fixture
        .emitter
        .try_queue_swap_backstop(&fixture.candidate.address, &fixture.next_token)
        .is_err());
}

#[test]
fn emitter_rejects_finalization_before_unlock() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();

    assert!(fixture.candidate.try_finalize_migration().is_err());
    assert_eq!(fixture.emitter.get_backstop(), fixture.incumbent);
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Prepared
    );
}

#[test]
fn balance_deficit_prevents_finalization_and_permits_cancellation() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture
        .legacy_token
        .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture.unlock();

    assert!(fixture.candidate.try_finalize_migration().is_err());
    assert_eq!(fixture.emitter.get_backstop(), fixture.incumbent);
    fixture.emitter.cancel_swap_backstop();
}

#[test]
fn candidate_rejects_a_different_next_designated_token() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    let unexpected_token = Address::generate(&fixture.env);
    fixture
        .emitter
        .queue_swap_backstop(&fixture.candidate.address, &unexpected_token);
    fixture.enter_preparation_window();

    assert!(fixture.candidate.try_prepare_migration().is_err());
    assert_eq!(fixture.emitter.get_backstop(), fixture.incumbent);
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Pending
    );
    assert_eq!(fixture.candidate.original_unlock(), None);
}

#[test]
fn candidate_synchronizes_after_emitter_was_finalized_directly() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let original_unlock = fixture.candidate.prepare_migration();
    fixture.unlock();
    fixture.emitter.swap_backstop();
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp().checked_add(10).unwrap());
    assert_eq!(fixture.emitter.distribute(), 10 * SCALAR_7);
    let synchronized_at = fixture.env.ledger().timestamp();

    assert_eq!(fixture.emitter.get_backstop(), fixture.candidate.address);
    assert!(fixture.candidate.try_finalize_migration().is_err());
    assert_eq!(fixture.candidate.sync_migration(), synchronized_at);
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Active
    );
    assert_eq!(fixture.candidate.original_unlock(), Some(original_unlock));
    assert_eq!(
        fixture.candidate.sync_deadline(),
        Some(original_unlock + SYNC_GRACE_SECONDS)
    );
    assert_eq!(fixture.candidate.activated_at(), Some(synchronized_at));
    assert_eq!(
        fixture.blnd.balance(&fixture.candidate.address),
        10 * SCALAR_7
    );
}

#[test]
fn late_sync_succeeds_after_post_swap_qualification_loss() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let original_unlock = fixture.candidate.prepare_migration();
    fixture.unlock();
    fixture.emitter.swap_backstop();
    fixture
        .legacy_token
        .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture
        .env
        .ledger()
        .set_timestamp(original_unlock + SYNC_GRACE_SECONDS - 1);

    assert_eq!(
        fixture.candidate.sync_migration(),
        original_unlock + SYNC_GRACE_SECONDS - 1
    );
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Active
    );
}

#[test]
fn candidate_requires_preparation_before_finalization() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.unlock();

    assert!(fixture.candidate.try_finalize_migration().is_err());
    assert_eq!(fixture.emitter.get_backstop(), fixture.incumbent);
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Open
    );
}

#[test]
fn direct_queue_requires_the_recovery_epoch_before_preparation() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    let queue_start = fixture.env.ledger().timestamp();
    fixture.queue_direct();
    fixture.enter_preparation_window();

    assert!(fixture.candidate.try_prepare_migration().is_err());
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Pending
    );
    assert_eq!(fixture.candidate.open_migration_epoch(), queue_start);
    assert!(fixture.candidate.prepare_migration() > queue_start);
}

#[test]
fn verified_requeue_preserves_original_unlock_and_gets_its_own_sync_window() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let first_cutoff = fixture.candidate.prepare_migration();

    fixture
        .legacy_token
        .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture.emitter.cancel_swap_backstop();
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp().checked_add(100).unwrap());
    fixture.queue_direct();

    fixture.enter_preparation_window();
    let retry_unlock = fixture.emitter.get_queued_swap().unwrap().unlock_time;
    assert_eq!(fixture.candidate.prepare_migration(), first_cutoff);
    assert_eq!(fixture.candidate.retry_count(), 1);
    assert_eq!(
        fixture.candidate.verified_queue_unlock(),
        Some(retry_unlock)
    );
    assert_eq!(
        fixture.candidate.sync_deadline(),
        Some(retry_unlock + SYNC_GRACE_SECONDS)
    );
    fixture.unlock();
    let activated_at = fixture.env.ledger().timestamp();
    assert_eq!(fixture.candidate.finalize_migration(), activated_at);
    assert_eq!(fixture.candidate.original_unlock(), Some(first_cutoff));
    assert_eq!(fixture.candidate.backfill_end(), Some(activated_at));
    assert!(activated_at > first_cutoff);
}

#[test]
fn candidate_rejects_preparation_before_the_final_queue_window() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    let unlock_time = fixture.emitter.get_queued_swap().unwrap().unlock_time;
    assert_eq!(
        unlock_time,
        fixture.env.ledger().timestamp() + QUEUE_SECONDS
    );

    assert!(fixture.candidate.try_prepare_migration().is_err());
    assert_eq!(fixture.candidate.original_unlock(), Some(unlock_time));

    fixture.enter_preparation_window();
    assert_eq!(fixture.candidate.prepare_migration(), unlock_time);
}

#[test]
fn synchronization_rejects_a_directly_finalized_replacement_queue() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let first_cutoff = fixture.candidate.prepare_migration();

    fixture
        .legacy_token
        .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture.emitter.cancel_swap_backstop();
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    let unexpected_token = Address::generate(&fixture.env);
    fixture
        .emitter
        .queue_swap_backstop(&fixture.candidate.address, &unexpected_token);
    fixture.unlock();
    assert!(fixture.env.ledger().timestamp() > first_cutoff + SYNC_GRACE_SECONDS);
    fixture.emitter.swap_backstop();

    assert_eq!(fixture.emitter.get_backstop(), fixture.candidate.address);
    assert!(fixture.candidate.try_sync_migration().is_err());
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Prepared
    );
}

#[test]
fn two_verified_retries_each_receive_a_safe_direct_sync_window() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let original_unlock = fixture.candidate.prepare_migration();

    for expected_retry in 1..=2 {
        fixture
            .legacy_token
            .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
        fixture.emitter.cancel_swap_backstop();
        fixture
            .legacy_token
            .mint(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
        fixture.queue_direct();
        fixture.enter_preparation_window();
        let retry_unlock = fixture.emitter.get_queued_swap().unwrap().unlock_time;
        assert_eq!(fixture.candidate.prepare_migration(), original_unlock);
        assert_eq!(fixture.candidate.retry_count(), expected_retry);
        assert_eq!(
            fixture.candidate.verified_queue_unlock(),
            Some(retry_unlock)
        );
    }

    fixture.unlock();
    fixture.emitter.swap_backstop();
    assert_eq!(
        fixture.candidate.sync_migration(),
        fixture.env.ledger().timestamp()
    );
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Active
    );
    assert!(
        fixture.candidate.backfill_end().unwrap()
            <= original_unlock + 2 * (QUEUE_SECONDS + SYNC_GRACE_SECONDS)
    );
}

#[test]
fn third_replacement_queue_is_rejected() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();

    for expected_retry in 1..=2 {
        fixture
            .legacy_token
            .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
        fixture.emitter.cancel_swap_backstop();
        fixture
            .legacy_token
            .mint(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
        fixture.queue_direct();
        fixture.enter_preparation_window();
        fixture.candidate.prepare_migration();
        assert_eq!(fixture.candidate.retry_count(), expected_retry);
    }

    fixture
        .legacy_token
        .burn(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture.emitter.cancel_swap_backstop();
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(100 * SCALAR_7 + 1));
    fixture.queue_direct();
    fixture.enter_preparation_window();

    assert!(fixture.candidate.try_prepare_migration().is_err());
    assert_eq!(fixture.candidate.retry_count(), 2);
}

#[test]
fn original_queue_must_begin_within_the_31_day_prefunding_campaign() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp() + QUEUE_SECONDS + 1);
    fixture.queue_direct();

    assert!(fixture.candidate.try_open_migration_epoch().is_err());
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Pending
    );
}

#[test]
fn late_canonical_begin_rolls_back_the_emitter_queue() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp() + QUEUE_SECONDS + 1);

    assert!(fixture.candidate.try_begin_migration().is_err());
    assert!(fixture.emitter.get_queued_swap().is_none());
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Pending
    );
}

#[test]
fn queue_at_prefunding_boundary_reaches_but_does_not_exceed_absolute_deadline() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp() + QUEUE_SECONDS);

    fixture.queue();

    assert_eq!(
        fixture.candidate.backfill_cap(),
        Some(fixture.candidate.absolute_migration_deadline())
    );
}

#[test]
fn canonical_finalization_rejects_after_the_absolute_recovery_horizon() {
    let fixture = Fixture::create();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.candidate.backfill_cap().unwrap() + 1);

    assert!(fixture.candidate.try_finalize_migration().is_err());
    assert_eq!(fixture.emitter.get_backstop(), fixture.incumbent);
}

#[test]
fn withdrawn_shares_forfeit_weight_to_time_weighted_survivors() {
    let fixture = Fixture::create();
    let pool = Address::generate(&fixture.env);
    let alice = Address::generate(&fixture.env);
    let bob = Address::generate(&fixture.env);
    let carol = Address::generate(&fixture.env);
    fixture
        .legacy_token
        .mint(&fixture.incumbent, &(100 * SCALAR_7));
    fixture.legacy_token.mint(&alice, &(200 * SCALAR_7));
    fixture.legacy_token.mint(&bob, &(200 * SCALAR_7));
    fixture.legacy_token.mint(&carol, &(100 * SCALAR_7));

    let alice_start = fixture.env.ledger().timestamp();
    fixture
        .candidate
        .deposit_migration(&alice, &pool, &(200 * SCALAR_7));
    fixture
        .candidate
        .deposit_migration(&carol, &pool, &(100 * SCALAR_7));
    fixture
        .env
        .ledger()
        .set_timestamp(alice_start.checked_add(100).unwrap());
    let queue_start = fixture.env.ledger().timestamp();
    fixture.queue();
    assert_eq!(fixture.candidate.open_migration_epoch(), queue_start);
    fixture
        .env
        .ledger()
        .set_timestamp(queue_start.checked_add(100).unwrap());
    let bob_start = fixture.env.ledger().timestamp();
    fixture
        .candidate
        .deposit_migration(&bob, &pool, &(200 * SCALAR_7));

    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture
        .candidate
        .withdraw_migration(&alice, &pool, &(50 * SCALAR_7), &alice);
    fixture
        .candidate
        .withdraw_migration(&carol, &pool, &(100 * SCALAR_7), &carol);
    fixture.unlock();
    let migration_time = fixture.env.ledger().timestamp();
    fixture.candidate.finalize_migration();

    let alice_weight = (150 * SCALAR_7) * (migration_time - alice_start) as i128;
    let bob_weight = (200 * SCALAR_7) * (migration_time - bob_start) as i128;
    assert_eq!(
        fixture.candidate.migration_weight(&alice, &pool),
        alice_weight
    );
    assert_eq!(fixture.candidate.migration_weight(&bob, &pool), bob_weight);
    assert_eq!(fixture.candidate.migration_weight(&carol, &pool), 0);
    assert_eq!(
        fixture.candidate.total_migration_weight(),
        alice_weight + bob_weight
    );

    let backfill = fixture.candidate.scheduled_backfill();
    assert_eq!(backfill, QUEUE_SECONDS as i128 * SCALAR_7);
    assert_eq!(
        fixture.candidate.quote_backfill(&alice, &pool),
        backfill * alice_weight / (alice_weight + bob_weight)
    );
    assert_eq!(
        fixture.candidate.quote_backfill(&bob, &pool),
        backfill * bob_weight / (alice_weight + bob_weight)
    );
}

#[test]
fn post_migration_prefunding_is_conserved_and_isolated_from_migration() {
    let fixture = Fixture::create();
    let pool = Address::generate(&fixture.env);
    let depositor = Address::generate(&fixture.env);
    let recipient = Address::generate(&fixture.env);
    let post_activation_depositor = Address::generate(&fixture.env);
    let prefund_token = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);
    prefund_token.mint(&depositor, &(300 * SCALAR_7));

    fixture
        .candidate
        .deposit_prefund(&depositor, &pool, &(300 * SCALAR_7));

    assert_eq!(
        fixture.candidate.prefund_shares(&depositor, &pool),
        300 * SCALAR_7
    );
    assert_eq!(fixture.candidate.pool_prefund_shares(&pool), 300 * SCALAR_7);
    assert_eq!(fixture.candidate.total_prefund_shares(), 300 * SCALAR_7);
    assert_eq!(
        TokenClient::new(&fixture.env, &fixture.prefund_token).balance(&fixture.candidate.address),
        300 * SCALAR_7
    );
    prefund_token.mint(&fixture.candidate.address, &(7 * SCALAR_7));
    assert_eq!(fixture.candidate.total_prefund_shares(), 300 * SCALAR_7);
    assert_eq!(
        TokenClient::new(&fixture.env, &fixture.prefund_token).balance(&fixture.candidate.address),
        307 * SCALAR_7
    );
    assert_eq!(fixture.legacy_token.balance(&fixture.candidate.address), 0);
    assert_eq!(
        fixture
            .candidate
            .migration_position(&depositor, &pool)
            .amount,
        0
    );
    assert_eq!(
        fixture.candidate.migration_status(),
        v3_backstop::MigrationStatus::Pending
    );
    assert!(fixture.candidate.try_begin_migration().is_err());

    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture.unlock();
    fixture.candidate.finalize_migration();

    assert_eq!(
        fixture.candidate.scheduled_backfill(),
        QUEUE_SECONDS as i128 * SCALAR_7
    );
    assert_eq!(fixture.candidate.total_prefund_shares(), 300 * SCALAR_7);

    fixture
        .candidate
        .withdraw_prefund(&depositor, &pool, &(100 * SCALAR_7), &recipient);
    assert_eq!(
        fixture.candidate.prefund_shares(&depositor, &pool),
        200 * SCALAR_7
    );
    assert_eq!(
        TokenClient::new(&fixture.env, &fixture.prefund_token).balance(&recipient),
        100 * SCALAR_7
    );

    prefund_token.mint(&post_activation_depositor, &(50 * SCALAR_7));
    fixture
        .candidate
        .deposit_prefund(&post_activation_depositor, &pool, &(50 * SCALAR_7));
    assert_eq!(fixture.candidate.pool_prefund_shares(&pool), 250 * SCALAR_7);
    assert_eq!(fixture.candidate.total_prefund_shares(), 250 * SCALAR_7);
    assert!(fixture
        .candidate
        .try_withdraw_prefund(&depositor, &pool, &(200 * SCALAR_7 + 1), &recipient,)
        .is_err());
}
