use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    vec, Address, Env, Vec,
};

#[allow(clippy::too_many_arguments)]
mod adapter {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/optimized/blend_v3_valuation_adapter.wasm"
    );
}

#[allow(clippy::too_many_arguments)]
mod backstop {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/optimized/blend_v3_backstop.wasm");
}

const SCALAR_7: i128 = 10_000_000;
const BLND_WEIGHT: i128 = 8_000_000;
const PAIR_WEIGHT: i128 = 2_000_000;

#[derive(Clone)]
#[contracttype]
enum OracleKey {
    Assets,
    Base,
    Decimals,
    Price(Address, u32),
    Resolution,
}

#[contract]
struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn __constructor(
        env: Env,
        base: adapter::OracleAsset,
        assets: Vec<adapter::OracleAsset>,
        resolution: u32,
    ) {
        env.storage().instance().set(&OracleKey::Base, &base);
        env.storage().instance().set(&OracleKey::Assets, &assets);
        env.storage().instance().set(&OracleKey::Decimals, &7_u32);
        env.storage()
            .instance()
            .set(&OracleKey::Resolution, &resolution);
    }

    pub fn base(env: Env) -> adapter::OracleAsset {
        env.storage().instance().get(&OracleKey::Base).unwrap()
    }

    pub fn assets(env: Env) -> Vec<adapter::OracleAsset> {
        env.storage().instance().get(&OracleKey::Assets).unwrap()
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&OracleKey::Decimals).unwrap()
    }

    pub fn resolution(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&OracleKey::Resolution)
            .unwrap()
    }

    pub fn prices(
        env: Env,
        asset: adapter::OracleAsset,
        records: u32,
    ) -> Option<Vec<adapter::PriceData>> {
        let adapter::OracleAsset::Stellar(address) = asset else {
            panic!("unsupported asset")
        };
        let mut prices = Vec::new(&env);
        for index in 0..records {
            let price = env
                .storage()
                .persistent()
                .get(&OracleKey::Price(address.clone(), index))?;
            prices.push_back(price);
        }
        Some(prices)
    }

    pub fn set_prices(env: Env, asset: Address, prices: Vec<adapter::PriceData>) {
        for (index, price) in prices.iter().enumerate() {
            env.storage()
                .persistent()
                .set(&OracleKey::Price(asset.clone(), index as u32), &price);
        }
    }
}

#[derive(Clone)]
#[contracttype]
enum CometKey {
    Balance(Address),
    Blnd,
    BlndReserve,
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
        blnd: Address,
        pair: Address,
        total_supply: i128,
        blnd_reserve: i128,
        pair_reserve: i128,
    ) {
        env.storage().instance().set(&CometKey::Blnd, &blnd);
        env.storage().instance().set(&CometKey::Pair, &pair);
        env.storage()
            .instance()
            .set(&CometKey::TotalSupply, &total_supply);
        env.storage()
            .instance()
            .set(&CometKey::BlndReserve, &blnd_reserve);
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
            env.storage().instance().get(&CometKey::Blnd).unwrap(),
            env.storage().instance().get(&CometKey::Pair).unwrap(),
        ]
    }

    pub fn get_balance(env: Env, token: Address) -> i128 {
        let blnd: Address = env.storage().instance().get(&CometKey::Blnd).unwrap();
        let key = if token == blnd {
            CometKey::BlndReserve
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
        let blnd: Address = env.storage().instance().get(&CometKey::Blnd).unwrap();
        if token == blnd {
            BLND_WEIGHT
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
}

#[contract]
struct MockPool;

#[contractimpl]
impl MockPool {
    pub fn backstop_withdrawal_allowed(_env: Env, _backstop: Address) -> bool {
        true
    }
}

#[contract]
struct MockEmitter;

#[contractimpl]
impl MockEmitter {
    pub fn __constructor(_env: Env) {}

    pub fn get_queued_swap(_env: Env) -> Option<backstop::EmitterSwap> {
        None
    }

    pub fn get_backstop(_env: Env) -> Address {
        panic!("not migrated")
    }
}

fn register_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn prices(env: &Env, value: i128) -> Vec<adapter::PriceData> {
    let mut result = Vec::new(env);
    for index in 0_u64..25 {
        result.push_back(adapter::PriceData {
            price: value,
            timestamp: index * 150,
        });
    }
    result
}

#[test]
fn optimized_production_adapter_quotes_separate_tick_history_with_mainnet_headroom() {
    let env = Env::default();
    env.ledger().set_timestamp(3_600);
    let blnd = register_token(&env);
    let usdc = register_token(&env);
    let xlm = register_token(&env);
    let blnd_usdc = env.register(
        MockComet,
        (
            &blnd,
            &usdc,
            &(100 * SCALAR_7),
            &(1_600 * SCALAR_7),
            &(20 * SCALAR_7),
        ),
    );
    let blnd_xlm = env.register(
        MockComet,
        (
            &blnd,
            &xlm,
            &(100 * SCALAR_7),
            &(1_600 * SCALAR_7),
            &(200 * SCALAR_7),
        ),
    );
    let base = adapter::OracleAsset::Stellar(usdc.clone());
    let oracle = env.register(
        MockOracle,
        (
            &base,
            &vec![
                &env,
                adapter::OracleAsset::Stellar(blnd.clone()),
                adapter::OracleAsset::Stellar(xlm.clone()),
            ],
            &150_u32,
        ),
    );
    let oracle_client = MockOracleClient::new(&env, &oracle);
    oracle_client.set_prices(&blnd, &prices(&env, 500_000));
    oracle_client.set_prices(&xlm, &prices(&env, 1_000_000));
    let adapter_id = env.register(
        adapter::WASM,
        (
            &oracle, &base, &blnd, &usdc, &xlm, &blnd_usdc, &blnd_xlm, &25_u32, &600_u64,
        ),
    );
    let client = adapter::Client::new(&env, &adapter_id);

    env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        client.quote(&blnd_usdc, &(10 * SCALAR_7)),
        adapter::AssetValuation {
            underlying_blnd: 160 * SCALAR_7,
            usdc_value: 10 * SCALAR_7,
            valid_until: 4_200,
        }
    );
    assert_eq!(
        client.quote(&blnd_xlm, &(10 * SCALAR_7)),
        adapter::AssetValuation {
            underlying_blnd: 160 * SCALAR_7,
            usdc_value: 10 * SCALAR_7,
            valid_until: 4_200,
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

#[test]
fn optimized_candidate_values_four_quotes_with_separate_tick_headroom() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(3_600);
    let blnd = register_token(&env);
    let usdc = register_token(&env);
    let xlm = register_token(&env);
    let blnd_usdc = env.register(
        MockComet,
        (
            &blnd,
            &usdc,
            &(100 * SCALAR_7),
            &(1_600 * SCALAR_7),
            &(20 * SCALAR_7),
        ),
    );
    let blnd_xlm = env.register(
        MockComet,
        (
            &blnd,
            &xlm,
            &(100 * SCALAR_7),
            &(1_600 * SCALAR_7),
            &(200 * SCALAR_7),
        ),
    );
    let base = adapter::OracleAsset::Stellar(usdc.clone());
    let oracle = env.register(
        MockOracle,
        (
            &base,
            &vec![
                &env,
                adapter::OracleAsset::Stellar(blnd.clone()),
                adapter::OracleAsset::Stellar(xlm.clone()),
            ],
            &150_u32,
        ),
    );
    let oracle_client = MockOracleClient::new(&env, &oracle);
    oracle_client.set_prices(&blnd, &prices(&env, 500_000));
    oracle_client.set_prices(&xlm, &prices(&env, 1_000_000));
    let adapter_id = env.register(
        adapter::WASM,
        (
            &oracle, &base, &blnd, &usdc, &xlm, &blnd_usdc, &blnd_xlm, &25_u32, &600_u64,
        ),
    );

    let candidate_id = Address::generate(&env);
    let factory_id = env.register(MockPoolFactory, (&candidate_id,));
    let emitter_id = env.register(MockEmitter, ());
    env.register_at(
        &candidate_id,
        backstop::WASM,
        (
            &emitter_id,
            &blnd,
            &factory_id,
            &blnd_usdc,
            &blnd_xlm,
            &usdc,
            &adapter_id,
        ),
    );
    let pool = env.register(MockPool, ());
    MockPoolFactoryClient::new(&env, &factory_id).set_pool(&pool);
    let provider = Address::generate(&env);
    let blnd_usdc_client = MockCometClient::new(&env, &blnd_usdc);
    let blnd_xlm_client = MockCometClient::new(&env, &blnd_xlm);
    blnd_usdc_client.mint(&provider, &(20 * SCALAR_7));
    blnd_xlm_client.mint(&provider, &(20 * SCALAR_7));
    let candidate = backstop::Client::new(&env, &candidate_id);
    candidate.deposit_blnd_usdc(&provider, &pool, &(20 * SCALAR_7));
    candidate.deposit_blnd_xlm(&provider, &pool, &(20 * SCALAR_7));
    candidate.queue_blnd_usdc_withdrawal(&provider, &pool, &(5 * SCALAR_7));
    candidate.queue_blnd_xlm_withdrawal(&provider, &pool, &(5 * SCALAR_7));

    env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        candidate.pool_valuation(&pool),
        backstop::PoolValuation {
            active_blnd: backstop::BlndEmissionValues {
                blnd_usdc: 240 * SCALAR_7,
                blnd_xlm: 240 * SCALAR_7,
            },
            active_values: backstop::ActivationValues {
                blnd_usdc: 15 * SCALAR_7,
                blnd_xlm: 15 * SCALAR_7,
                usdc: 0,
            },
            queued_values: backstop::ActivationValues {
                blnd_usdc: 5 * SCALAR_7,
                blnd_xlm: 5 * SCALAR_7,
                usdc: 0,
            },
            valid_until: 4_200,
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
