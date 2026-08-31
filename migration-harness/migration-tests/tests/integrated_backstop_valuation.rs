use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, vec, Address, Env, Map, Vec,
};

#[allow(clippy::too_many_arguments)]
mod backstop {
    soroban_sdk::contractimport!(
        file = "../../blnt-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
    );
}

const SCALAR_7: i128 = 10_000_000;

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
const BLNT_WEIGHT: i128 = 8_000_000;
const PAIR_WEIGHT: i128 = 2_000_000;

#[derive(Clone)]
#[contracttype]
enum CometKey {
    Balance(Address),
    Blnt,
    BlntReserve,
    Pair,
    PairReserve,
    TotalSupply,
}

#[contract]
struct MockComet;

#[contractimpl]
impl MockComet {
    pub fn __constructor(
        env: Env,
        blnt: Address,
        pair: Address,
        total_supply: i128,
        blnt_reserve: i128,
        pair_reserve: i128,
    ) {
        env.storage().instance().set(&CometKey::Blnt, &blnt);
        env.storage().instance().set(&CometKey::Pair, &pair);
        env.storage()
            .instance()
            .set(&CometKey::TotalSupply, &total_supply);
        env.storage()
            .instance()
            .set(&CometKey::BlntReserve, &blnt_reserve);
        env.storage()
            .instance()
            .set(&CometKey::PairReserve, &pair_reserve);
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        vec![
            &env,
            env.storage().instance().get(&CometKey::Blnt).unwrap(),
            env.storage().instance().get(&CometKey::Pair).unwrap(),
        ]
    }

    pub fn get_balance(env: Env, token: Address) -> i128 {
        let blnt: Address = env.storage().instance().get(&CometKey::Blnt).unwrap();
        let key = if token == blnt {
            CometKey::BlntReserve
        } else {
            CometKey::PairReserve
        };
        env.storage().instance().get(&key).unwrap()
    }

    pub fn get_total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&CometKey::TotalSupply)
            .unwrap()
    }

    pub fn get_normalized_weight(env: Env, token: Address) -> i128 {
        let blnt: Address = env.storage().instance().get(&CometKey::Blnt).unwrap();
        if token == blnt {
            BLNT_WEIGHT
        } else {
            PAIR_WEIGHT
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

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let from_balance = Self::balance(env.clone(), from.clone());
        if amount < 0 || amount > from_balance {
            panic!("invalid transfer")
        }
        let to_balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&CometKey::Balance(from), &(from_balance - amount));
        env.storage()
            .instance()
            .set(&CometKey::Balance(to), &(to_balance + amount));
    }
}

#[derive(Clone)]
#[contracttype]
enum FactoryKey {
    Backstop,
    Pool(Address),
}

#[contract]
struct MockPoolFactory;

#[contractimpl]
impl MockPoolFactory {
    pub fn __constructor(env: Env, backstop: Address) {
        env.storage()
            .instance()
            .set(&FactoryKey::Backstop, &backstop);
    }

    pub fn backstop(env: Env) -> Address {
        env.storage().instance().get(&FactoryKey::Backstop).unwrap()
    }

    pub fn is_pool(env: Env, pool: Address) -> bool {
        env.storage()
            .instance()
            .get(&FactoryKey::Pool(pool))
            .unwrap_or(false)
    }

    pub fn set_pool(env: Env, pool: Address) {
        env.storage().instance().set(&FactoryKey::Pool(pool), &true);
    }

    pub fn backstop_config(env: Env, _pool: Address) -> PoolBackstopConfig {
        PoolBackstopConfig {
            access_controller: None,
            tiers: vec![
                &env,
                BackstopTierConfig {
                    asset: BackstopAsset::BlntXlm,
                    take_rate_weight: 4,
                },
                BackstopTierConfig {
                    asset: BackstopAsset::BlntUsdc,
                    take_rate_weight: 3,
                },
            ],
        }
    }
}

#[contract]
struct MockPool;

#[derive(Clone)]
#[contracttype]
struct MockPositions {
    liabilities: Map<u32, i128>,
    collateral: Map<u32, i128>,
    supply: Map<u32, i128>,
}

#[contractimpl]
impl MockPool {
    pub fn get_positions(env: Env, _address: Address) -> MockPositions {
        MockPositions {
            liabilities: Map::new(&env),
            collateral: Map::new(&env),
            supply: Map::new(&env),
        }
    }
}

#[contract]
struct MockEmitter;

#[derive(Clone)]
#[contracttype]
enum EmitterKey {
    Backstop,
}

#[derive(Clone)]
#[contracttype]
struct EmitterSwap {
    new_backstop: Address,
    new_backstop_token: Address,
    unlock_time: u64,
}

#[contractimpl]
impl MockEmitter {
    pub fn __constructor(env: Env, backstop: Address) {
        env.storage()
            .instance()
            .set(&EmitterKey::Backstop, &backstop);
    }

    pub fn get_queued_swap(_env: Env) -> Option<EmitterSwap> {
        None
    }

    pub fn get_backstop(env: Env) -> Address {
        env.storage().instance().get(&EmitterKey::Backstop).unwrap()
    }
}

fn register_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

#[test]
fn optimized_candidate_values_integrated_comet_quotes_with_mainnet_headroom() {
    let env = Env::default();
    env.mock_all_auths();
    let blnt = register_token(&env);
    let usdc = register_token(&env);
    let xlm = register_token(&env);
    let blnt_usdc = env.register(
        MockComet,
        (
            &blnt,
            &usdc,
            &(100 * SCALAR_7),
            &(1_600 * SCALAR_7),
            &(20 * SCALAR_7),
        ),
    );
    let blnt_xlm = env.register(
        MockComet,
        (
            &blnt,
            &xlm,
            &(100 * SCALAR_7),
            &(1_600 * SCALAR_7),
            &(200 * SCALAR_7),
        ),
    );
    let candidate_id = Address::generate(&env);
    let factory_id = env.register(MockPoolFactory, (&candidate_id,));
    let incumbent = Address::generate(&env);
    let emitter_id = env.register(MockEmitter, (&incumbent,));
    env.register_at(
        &candidate_id,
        backstop::WASM,
        (
            &blnt_usdc,
            &blnt_xlm,
            &emitter_id,
            &blnt,
            &usdc,
            &xlm,
            &factory_id,
            Vec::<(Address, i128)>::new(&env),
        ),
    );
    let pool = env.register(MockPool, ());
    MockPoolFactoryClient::new(&env, &factory_id).set_pool(&pool);
    let provider = Address::generate(&env);
    let blnt_usdc_client = MockCometClient::new(&env, &blnt_usdc);
    let blnt_xlm_client = MockCometClient::new(&env, &blnt_xlm);
    blnt_usdc_client.mint(&provider, &(20 * SCALAR_7));
    blnt_xlm_client.mint(&provider, &(20 * SCALAR_7));
    let candidate = backstop::Client::new(&env, &candidate_id);
    candidate.deposit(
        &backstop::BackstopTier::SecondLoss,
        &provider,
        &pool,
        &(20 * SCALAR_7),
    );
    candidate.deposit(
        &backstop::BackstopTier::FirstLoss,
        &provider,
        &pool,
        &(20 * SCALAR_7),
    );
    candidate.queue_withdrawal(
        &backstop::BackstopTier::SecondLoss,
        &provider,
        &pool,
        &(5 * SCALAR_7),
    );
    candidate.queue_withdrawal(
        &backstop::BackstopTier::FirstLoss,
        &provider,
        &pool,
        &(5 * SCALAR_7),
    );

    env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        candidate.pool_data(&pool),
        backstop::PoolBackstopData {
            active_value: 30 * SCALAR_7,
            q4w_pct: 2_500_000,
            tiers: vec![
                &env,
                backstop::PoolTierData {
                    asset: backstop::BackstopAsset::BlntXlm,
                    blnt_emission_eligible: true,
                    take_rate_weight: 4,
                    token: blnt_xlm,
                    tokens: 20 * SCALAR_7,
                    shares: 20 * SCALAR_7,
                    value: 20 * SCALAR_7,
                },
                backstop::PoolTierData {
                    asset: backstop::BackstopAsset::BlntUsdc,
                    blnt_emission_eligible: true,
                    take_rate_weight: 3,
                    token: blnt_usdc,
                    tokens: 20 * SCALAR_7,
                    shares: 20 * SCALAR_7,
                    value: 20 * SCALAR_7,
                },
            ],
        }
    );
    let resources = env.cost_estimate().resources();
    assert!(resources.instructions <= 600_000_000);
    assert!(resources.mem_bytes <= 41_943_040);
    assert!(resources.disk_read_entries <= 100);
    assert!(resources.write_entries <= 50);
    assert!(
        resources.disk_read_entries + resources.memory_read_entries + resources.write_entries <= 90
    );
    assert!(resources.disk_read_bytes <= 200_000);
    assert!(resources.write_bytes <= 132_096);
    assert!(resources.contract_events_size_bytes <= 16_384);
}
