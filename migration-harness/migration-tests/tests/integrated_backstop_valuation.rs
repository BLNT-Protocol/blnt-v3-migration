use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, vec, Address, Env, Vec,
};

#[allow(clippy::too_many_arguments)]
mod backstop {
    soroban_sdk::contractimport!(
        file = "../../blend-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
    );
}

const SCALAR_7: i128 = 10_000_000;
const BLND_WEIGHT: i128 = 8_000_000;
const PAIR_WEIGHT: i128 = 2_000_000;

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

#[derive(Clone)]
#[contracttype]
struct EmitterSwap {
    new_backstop: Address,
    new_backstop_token: Address,
    unlock_time: u64,
}

#[contractimpl]
impl MockEmitter {
    pub fn __constructor(_env: Env) {}

    pub fn get_queued_swap(_env: Env) -> Option<EmitterSwap> {
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

#[test]
fn optimized_candidate_values_integrated_comet_quotes_with_mainnet_headroom() {
    let env = Env::default();
    env.mock_all_auths();
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
    let candidate_id = Address::generate(&env);
    let factory_id = env.register(MockPoolFactory, (&candidate_id,));
    let emitter_id = env.register(MockEmitter, ());
    env.register_at(
        &candidate_id,
        backstop::WASM,
        (
            &blnd_usdc,
            &blnd_xlm,
            &emitter_id,
            &blnd,
            &usdc,
            &xlm,
            &factory_id,
            Vec::<(Address, i128)>::new(&env),
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
    candidate.deposit(
        &backstop::BackstopTier::BlndUsdc,
        &provider,
        &pool,
        &(20 * SCALAR_7),
    );
    candidate.deposit(
        &backstop::BackstopTier::BlndXlm,
        &provider,
        &pool,
        &(20 * SCALAR_7),
    );
    candidate.queue_withdrawal(
        &backstop::BackstopTier::BlndUsdc,
        &provider,
        &pool,
        &(5 * SCALAR_7),
    );
    candidate.queue_withdrawal(
        &backstop::BackstopTier::BlndXlm,
        &provider,
        &pool,
        &(5 * SCALAR_7),
    );

    env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        candidate.pool_data(&pool),
        backstop::PoolData {
            blnd_usdc: backstop::PoolTierData {
                active_blnd: 240 * SCALAR_7,
                active_value: 15 * SCALAR_7,
                assets: 20 * SCALAR_7,
                queued_shares: 5 * SCALAR_7,
                queued_value: 5 * SCALAR_7,
                shares: 20 * SCALAR_7,
                total_value: 20 * SCALAR_7,
            },
            blnd_xlm: backstop::PoolTierData {
                active_blnd: 240 * SCALAR_7,
                active_value: 15 * SCALAR_7,
                assets: 20 * SCALAR_7,
                queued_shares: 5 * SCALAR_7,
                queued_value: 5 * SCALAR_7,
                shares: 20 * SCALAR_7,
                total_value: 20 * SCALAR_7,
            },
            q4w_percentage: 2_500_000,
            usdc: backstop::PoolTierData {
                active_blnd: 0,
                active_value: 0,
                assets: 0,
                queued_shares: 0,
                queued_value: 0,
                shares: 0,
                total_value: 0,
            },
            valuation_valid_until: u64::MAX,
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
