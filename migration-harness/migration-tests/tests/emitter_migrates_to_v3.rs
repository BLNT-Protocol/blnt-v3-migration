use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, Vec,
};

mod support;
use support::{migration_state, MigrationStatus};

mod legacy_emitter {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/emitter.wasm");
}

mod v3_emitter {
    soroban_sdk::contractimport!(
        file = "../../blnt-contracts-v3/target/wasm32v1-none/optimized/emitter.wasm"
    );
}

mod blnt_backfill {
    soroban_sdk::contractimport!(
        file = "../../blnt-backfill-contract/target/wasm32v1-none/optimized/blnt_backfill_contract.wasm"
    );
}

#[allow(clippy::too_many_arguments)]
mod v3_backstop {
    soroban_sdk::contractimport!(
        file = "../../blnt-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
    );
}

#[derive(Clone)]
#[contracttype]
struct BackstopTierConfig {
    asset: BackstopAsset,
    take_rate_weight: u32,
}

#[derive(Clone)]
#[contracttype]
struct PoolBackstopConfig {
    access_controller: Option<Address>,
    tiers: Vec<BackstopTierConfig>,
}

#[derive(Clone)]
#[contracttype]
enum BackstopAsset {
    BlntXlm,
    BlntUsdc,
    Usdc,
    Xlm,
}

#[derive(Clone)]
#[contracttype]
enum FactoryKey {
    Backstop,
    Pool,
}

#[contract]
struct MockPoolFactory;

#[contractimpl]
impl MockPoolFactory {
    pub fn __constructor(env: Env, backstop: Address, pool: Address) {
        env.storage()
            .instance()
            .set(&FactoryKey::Backstop, &backstop);
        env.storage().instance().set(&FactoryKey::Pool, &pool);
    }

    pub fn backstop(env: Env) -> Address {
        env.storage().instance().get(&FactoryKey::Backstop).unwrap()
    }

    pub fn is_pool(env: Env, pool: Address) -> bool {
        env.storage()
            .instance()
            .get::<_, Address>(&FactoryKey::Pool)
            .unwrap()
            == pool
    }

    pub fn backstop_config(env: Env, _pool: Address) -> PoolBackstopConfig {
        PoolBackstopConfig {
            access_controller: None,
            tiers: soroban_sdk::vec![
                &env,
                BackstopTierConfig {
                    asset: BackstopAsset::BlntXlm,
                    take_rate_weight: 4,
                },
                BackstopTierConfig {
                    asset: BackstopAsset::BlntUsdc,
                    take_rate_weight: 3,
                },
                BackstopTierConfig {
                    asset: BackstopAsset::Usdc,
                    take_rate_weight: 2,
                },
            ],
        }
    }
}

const SCALAR_7: i128 = 10_000_000;
const SWAP_QUEUE_SECONDS: u64 = 31 * 24 * 60 * 60;
const INITIAL_DROP: i128 = 150_000_000 * SCALAR_7;
const SWAP_CAP: i128 = 120_000_000 * SCALAR_7;
const BACKFILL_CLAIMANTS: u32 = 434;
const GRANT_CLAIMANTS: u32 = 100;
const VESTING_DURATION_SECONDS: u64 = 180 * 24 * 60 * 60;
const GRANT_VESTING_DURATION_SECONDS: u64 = 720 * 24 * 60 * 60;
const SWAP_DURATION_SECONDS: u64 = 270 * 24 * 60 * 60;

fn create_asset(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

#[derive(Clone)]
#[contracttype]
enum CometKey {
    Balance(Address),
    Blnt,
    Pair,
}

#[contract]
struct MockComet;

#[contractimpl]
impl MockComet {
    pub fn __constructor(env: Env, blnt: Address, pair: Address) {
        env.storage().instance().set(&CometKey::Blnt, &blnt);
        env.storage().instance().set(&CometKey::Pair, &pair);
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        soroban_sdk::vec![
            &env,
            env.storage().instance().get(&CometKey::Blnt).unwrap(),
            env.storage().instance().get(&CometKey::Pair).unwrap(),
        ]
    }

    pub fn get_normalized_weight(env: Env, token: Address) -> i128 {
        let blnt: Address = env.storage().instance().get(&CometKey::Blnt).unwrap();
        if token == blnt {
            8_000_000
        } else {
            2_000_000
        }
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&CometKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&CometKey::Balance(to), &(balance + amount));
    }
}

#[test]
fn optimized_backfill_handles_all_backfill_and_grant_claimants_partially_vested() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let legacy_blnd = create_asset(&env, &token_admin);
    let blnt = create_asset(&env, &token_admin);
    let mut claims = Vec::new(&env);
    let mut claimants = Vec::new(&env);
    for _ in 0..BACKFILL_CLAIMANTS {
        let claimant = Address::generate(&env);
        claims.push_back((claimant.clone(), 2_i128));
        claimants.push_back(claimant);
    }
    let mut grants = Vec::new(&env);
    let mut grantees = Vec::new(&env);
    for _ in 0..GRANT_CLAIMANTS {
        let grantee = Address::generate(&env);
        grants.push_back((grantee.clone(), 8_i128));
        grantees.push_back(grantee);
    }

    let backfill_id = env.register(blnt_backfill::WASM, (&legacy_blnd, &blnt, claims, grants));
    StellarAssetClient::new(&env, &blnt).mint(
        &backfill_id,
        &(2 * i128::from(BACKFILL_CLAIMANTS) + 8 * i128::from(GRANT_CLAIMANTS)),
    );
    let backfill = blnt_backfill::Client::new(&env, &backfill_id);

    assert_eq!(
        backfill.get_total_allocated(),
        2 * i128::from(BACKFILL_CLAIMANTS) + 8 * i128::from(GRANT_CLAIMANTS)
    );
    assert_eq!(
        backfill.get_backfill_allocated(),
        2 * i128::from(BACKFILL_CLAIMANTS)
    );
    assert_eq!(
        backfill.get_grant_allocated(),
        8 * i128::from(GRANT_CLAIMANTS)
    );
    assert_eq!(
        backfill.get_vesting_end(),
        backfill.get_vesting_start() + VESTING_DURATION_SECONDS
    );
    assert_eq!(
        backfill.get_grant_vesting_end(),
        backfill.get_grant_vesting_start() + GRANT_VESTING_DURATION_SECONDS
    );
    assert_eq!(
        backfill.get_swap_deadline(),
        backfill.get_vesting_start() + SWAP_DURATION_SECONDS
    );
    env.ledger()
        .set_timestamp(backfill.get_vesting_start() + VESTING_DURATION_SECONDS / 2);
    for claimant in claimants.iter() {
        assert_eq!(backfill.claim_backfill(&claimant, &claimant), 1);
    }
    for grantee in grantees.iter() {
        assert_eq!(backfill.claim_grant(&grantee, &grantee), 1);
    }

    let first = claimants.first().unwrap();
    env.ledger().set_timestamp(backfill.get_vesting_end());
    assert_eq!(backfill.claim_backfill(&first, &first), 1);
    assert_eq!(backfill.get_backfill_claimable(&first), 0);

    let first_grantee = grantees.first().unwrap();
    env.ledger().set_timestamp(backfill.get_grant_vesting_end());
    assert_eq!(backfill.claim_grant(&first_grantee, &first_grantee), 7);
    assert_eq!(backfill.get_grant_claimable(&first_grantee), 0);
}

#[test]
fn v3_starts_with_blnt_while_legacy_blnd_emissions_continue_independently() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let token_admin = Address::generate(&env);
    let legacy_blnd = create_asset(&env, &token_admin);
    let usdc = create_asset(&env, &token_admin);
    let xlm = create_asset(&env, &token_admin);

    let incumbent = Address::generate(&env);
    let legacy_blnd_usdc = env.register(MockComet, (&legacy_blnd, &usdc));
    let legacy_emitter_id = env.register(legacy_emitter::WASM, ());
    StellarAssetClient::new(&env, &legacy_blnd).set_admin(&legacy_emitter_id);
    let legacy = legacy_emitter::Client::new(&env, &legacy_emitter_id);
    legacy.initialize(&legacy_blnd, &incumbent, &legacy_blnd_usdc);

    let initializer = Address::generate(&env);
    let v3_emitter_id = env.register(v3_emitter::WASM, (&initializer,));
    let blnt = create_asset(&env, &v3_emitter_id);
    let claimant = Address::generate(&env);
    let claim_recipient = Address::generate(&env);
    let backfill_id = env.register(
        blnt_backfill::WASM,
        (
            &legacy_blnd,
            &blnt,
            soroban_sdk::vec![&env, (claimant.clone(), SCALAR_7)],
            Vec::<(Address, i128)>::new(&env),
        ),
    );
    let blnt_usdc = env.register(MockComet, (&blnt, &usdc));
    let blnt_xlm = env.register(MockComet, (&blnt, &xlm));

    let candidate_id = Address::generate(&env);
    let pool = Address::generate(&env);
    let factory = env.register(MockPoolFactory, (&candidate_id, &pool));
    let v3 = v3_emitter::Client::new(&env, &v3_emitter_id);
    v3.initialize(&blnt, &candidate_id, &blnt_usdc);
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &blnt_usdc,
            &blnt_xlm,
            &v3_emitter_id,
            &blnt,
            &usdc,
            &xlm,
            &factory,
            soroban_sdk::vec![&env, (backfill_id.clone(), INITIAL_DROP)],
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);

    assert_eq!(candidate.distribute(), 0);
    assert_eq!(
        migration_state(&env, &candidate_id).status,
        MigrationStatus::Active
    );
    assert_eq!(
        migration_state(&env, &candidate_id).migration_epoch_start,
        None
    );
    assert_eq!(legacy.get_backstop(), incumbent);
    assert_eq!(v3.get_backstop(), candidate_id);
    candidate.drop();
    assert_eq!(
        TokenClient::new(&env, &blnt).balance(&backfill_id),
        INITIAL_DROP
    );

    let backfill = blnt_backfill::Client::new(&env, &backfill_id);
    assert_eq!(backfill.get_backfill_claimable(&claimant), 0);
    assert!(backfill
        .try_claim_backfill(&claimant, &claim_recipient)
        .is_err());

    env.ledger().set_timestamp(1_005);
    let vested = SCALAR_7 * 5 / i128::from(VESTING_DURATION_SECONDS);
    assert_eq!(backfill.claim_backfill(&claimant, &claim_recipient), vested);
    assert_eq!(backfill.get_backfill_claimable(&claimant), 0);
    assert_eq!(
        TokenClient::new(&env, &blnt).balance(&claim_recipient),
        vested
    );

    assert_eq!(legacy.distribute(), 5 * SCALAR_7);
    assert_eq!(v3.distribute(), 5 * SCALAR_7);
    assert_eq!(
        TokenClient::new(&env, &legacy_blnd).balance(&incumbent),
        5 * SCALAR_7
    );
    assert_eq!(
        TokenClient::new(&env, &blnt).balance(&candidate_id),
        5 * SCALAR_7
    );
    assert_eq!(
        TokenClient::new(&env, &legacy_blnd).balance(&candidate_id),
        0
    );
    assert_eq!(TokenClient::new(&env, &blnt).balance(&incumbent), 0);

    assert_eq!(
        candidate.backstop_token(&v3_backstop::BackstopTier::SecondLoss, &pool),
        blnt_usdc
    );
    assert_eq!(
        candidate.backstop_token(&v3_backstop::BackstopTier::FirstLoss, &pool),
        blnt_xlm
    );

    let holder = Address::generate(&env);
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &legacy_blnd).mint(&holder, &(10 * SCALAR_7));
    assert_eq!(
        backfill.swap_blnd_for_blnt(&holder, &recipient, &(4 * SCALAR_7)),
        4 * SCALAR_7
    );
    assert_eq!(
        TokenClient::new(&env, &legacy_blnd).balance(&holder),
        6 * SCALAR_7
    );
    assert_eq!(
        TokenClient::new(&env, &legacy_blnd).balance(&backfill_id),
        0
    );
    assert_eq!(
        TokenClient::new(&env, &blnt).balance(&recipient),
        4 * SCALAR_7
    );
    assert_eq!(backfill.get_total_swapped(), 4 * SCALAR_7);
    assert_eq!(
        backfill.get_remaining_swap_capacity(),
        119_999_996 * SCALAR_7
    );
    assert_eq!(backfill.get_legacy_blnd_token(), legacy_blnd);
    assert_eq!(backfill.get_blnt_token(), blnt);

    let future_backstop = Address::generate(&env);
    MockCometClient::new(&env, &blnt_usdc).mint(&candidate_id, &(100 * SCALAR_7));
    MockCometClient::new(&env, &blnt_usdc).mint(&future_backstop, &(100 * SCALAR_7 + 1));
    let queued_at = env.ledger().timestamp();
    v3.queue_swap_backstop(&future_backstop, &blnt_usdc);
    let queued = v3.get_queued_swap().unwrap();
    assert_eq!(queued.new_backstop, future_backstop);
    assert_eq!(queued.new_backstop_token, blnt_usdc);
    assert_eq!(queued.unlock_time, queued_at + SWAP_QUEUE_SECONDS);

    env.ledger().set_timestamp(queued.unlock_time);
    v3.swap_backstop();
    assert_eq!(v3.get_backstop(), future_backstop);
    assert!(v3.get_queued_swap().is_none());

    env.ledger().set_timestamp(backfill.get_swap_deadline());
    assert!(backfill
        .try_swap_blnd_for_blnt(&holder, &recipient, &SCALAR_7)
        .is_err());
    assert_eq!(backfill.get_remaining_swap_capacity(), 0);
    assert_eq!(backfill.burn_expired(), SWAP_CAP - 4 * SCALAR_7);
    assert_eq!(backfill.get_expired_swap_burned(), SWAP_CAP - 4 * SCALAR_7);
    assert_eq!(backfill.burn_expired(), 0);
}
