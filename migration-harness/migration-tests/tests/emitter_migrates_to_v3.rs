use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, Vec,
};

mod support;
use support::{migration_state, MigrationStatus};

mod deployed_emitter {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/emitter.wasm");
}

#[allow(clippy::too_many_arguments)]
mod v3_backstop {
    soroban_sdk::contractimport!(
        file = "../../blend-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
    );
}

#[derive(Clone)]
#[contracttype]
struct QueuedSwap {
    new_backstop: Address,
    new_backstop_token: Address,
    unlock_time: u64,
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
    BlndXlm,
    BlndUsdc,
    Usdc,
    Xlm,
}

#[allow(dead_code)]
#[contractclient(name = "DeployedEmitterClient")]
trait DeployedEmitter {
    fn initialize(env: Env, blnd_token: Address, backstop: Address, backstop_token: Address);
    fn get_backstop(env: Env) -> Address;
    fn get_queued_swap(env: Env) -> Option<QueuedSwap>;
    fn queue_swap_backstop(env: Env, new_backstop: Address, new_backstop_token: Address);
    fn swap_backstop(env: Env);
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
                    asset: BackstopAsset::BlndXlm,
                    take_rate_weight: 4,
                },
                BackstopTierConfig {
                    asset: BackstopAsset::BlndUsdc,
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

fn create_asset(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

#[derive(Clone)]
#[contracttype]
enum CometKey {
    Balance(Address),
    Blnd,
    Pair,
}

#[contract]
struct MockComet;

#[contractimpl]
impl MockComet {
    pub fn __constructor(env: Env, blnd: Address, pair: Address) {
        env.storage().instance().set(&CometKey::Blnd, &blnd);
        env.storage().instance().set(&CometKey::Pair, &pair);
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        soroban_sdk::vec![
            &env,
            env.storage().instance().get(&CometKey::Blnd).unwrap(),
            env.storage().instance().get(&CometKey::Pair).unwrap(),
        ]
    }

    pub fn get_normalized_weight(env: Env, token: Address) -> i128 {
        let blnd: Address = env.storage().instance().get(&CometKey::Blnd).unwrap();
        if token == blnd {
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
fn optimized_candidate_migrates_through_the_deployed_emitter() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let token_admin = Address::generate(&env);
    let blnd = create_asset(&env, &token_admin);
    let usdc = create_asset(&env, &token_admin);
    let xlm = create_asset(&env, &token_admin);
    let blnd_usdc = env.register(MockComet, (&blnd, &usdc));
    let blnd_xlm = env.register(MockComet, (&blnd, &xlm));

    let incumbent = Address::generate(&env);
    let emitter_id = env.register(deployed_emitter::WASM, ());
    StellarAssetClient::new(&env, &blnd).set_admin(&emitter_id);
    let emitter = DeployedEmitterClient::new(&env, &emitter_id);
    emitter.initialize(&blnd, &incumbent, &blnd_usdc);

    let candidate_id = Address::generate(&env);
    let pool = Address::generate(&env);
    let factory = env.register(MockPoolFactory, (&candidate_id, &pool));
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &blnd_usdc,
            &blnd_xlm,
            &emitter_id,
            &blnd,
            &usdc,
            &xlm,
            &factory,
            Vec::<(Address, i128)>::new(&env),
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);

    let blnd_usdc_token = MockCometClient::new(&env, &blnd_usdc);
    blnd_usdc_token.mint(&incumbent, &(100 * SCALAR_7));
    blnd_usdc_token.mint(&candidate_id, &(100 * SCALAR_7 + 1));

    let backfill_start = env.ledger().timestamp();
    assert_eq!(candidate.distribute(), 0);
    assert_eq!(
        migration_state(&env, &candidate_id).migration_epoch_start,
        Some(backfill_start)
    );
    emitter.queue_swap_backstop(&candidate_id, &blnd_xlm);
    assert_eq!(candidate.distribute(), 0);
    let queued = emitter.get_queued_swap().unwrap();
    assert_eq!(queued.new_backstop, candidate_id);
    assert_eq!(queued.new_backstop_token, blnd_xlm);

    assert_eq!(
        migration_state(&env, &candidate_id).status,
        MigrationStatus::Open
    );

    env.ledger().set_timestamp(queued.unlock_time);
    assert_eq!(candidate.distribute(), 0);
    assert_eq!(
        migration_state(&env, &candidate_id).verified_queue_unlock,
        Some(queued.unlock_time)
    );
    emitter.swap_backstop();
    assert_eq!(candidate.distribute(), 0);

    assert_eq!(emitter.get_backstop(), candidate_id);
    assert!(emitter.get_queued_swap().is_none());
    assert_eq!(
        migration_state(&env, &candidate_id).status,
        MigrationStatus::Active
    );
    assert_eq!(
        migration_state(&env, &candidate_id).activated_at,
        Some(queued.unlock_time)
    );
    assert_eq!(
        candidate.backstop_token(&v3_backstop::BackstopTier::SecondLoss, &pool),
        blnd_usdc
    );
    assert_eq!(
        candidate.backstop_token(&v3_backstop::BackstopTier::FirstLoss, &pool),
        blnd_xlm
    );
}
