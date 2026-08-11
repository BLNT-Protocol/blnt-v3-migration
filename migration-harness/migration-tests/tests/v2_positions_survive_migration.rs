use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractclient, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    vec, Address, BytesN, Env, IntoVal, Map, String, Symbol, Val, Vec,
};

mod deployed_emitter {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/emitter.wasm");
}

use deployed_emitter::WASM as DEPLOYED_EMITTER_WASM;

mod v2_backstop {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/v2-backstop.wasm");
}

#[allow(clippy::too_many_arguments)]
mod v2_pool {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/v2-pool.wasm");
}

#[allow(clippy::too_many_arguments)]
mod v2_pool_factory {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/v2-pool-factory.wasm");
}

#[allow(clippy::too_many_arguments)]
mod v3_backstop {
    soroban_sdk::contractimport!(
        file = "../../blend-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
    );
}

const SCALAR_7: i128 = 10_000_000;

#[derive(Clone)]
#[contracttype]
struct QueuedSwap {
    new_backstop: Address,
    new_backstop_token: Address,
    unlock_time: u64,
}

#[contractclient(name = "DeployedEmitterClient")]
#[allow(dead_code)]
trait DeployedEmitter {
    fn initialize(env: Env, blnd_token: Address, backstop: Address, backstop_token: Address);
    fn distribute(env: Env) -> i128;
    fn get_last_distro(env: Env, backstop_id: Address) -> u64;
    fn get_backstop(env: Env) -> Address;
    fn get_queued_swap(env: Env) -> Option<QueuedSwap>;
    fn queue_swap_backstop(env: Env, new_backstop: Address, new_backstop_token: Address);
    fn swap_backstop(env: Env);
}

#[contract]
struct MockComet;

#[derive(Clone)]
#[contracttype]
enum MockCometKey {
    Balance(Address),
    Blnd,
    BlndReserve,
    Paired,
    PairedReserve,
    TotalSupply,
}

#[contractimpl]
impl MockComet {
    pub fn __constructor(
        env: Env,
        blnd: Address,
        paired: Address,
        total_supply: i128,
        blnd_reserve: i128,
        paired_reserve: i128,
    ) {
        env.storage().instance().set(&MockCometKey::Blnd, &blnd);
        env.storage().instance().set(&MockCometKey::Paired, &paired);
        env.storage()
            .instance()
            .set(&MockCometKey::TotalSupply, &total_supply);
        env.storage()
            .instance()
            .set(&MockCometKey::BlndReserve, &blnd_reserve);
        env.storage()
            .instance()
            .set(&MockCometKey::PairedReserve, &paired_reserve);
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        soroban_sdk::vec![
            &env,
            env.storage().instance().get(&MockCometKey::Blnd).unwrap(),
            env.storage().instance().get(&MockCometKey::Paired).unwrap(),
        ]
    }

    pub fn get_normalized_weight(env: Env, token: Address) -> i128 {
        let blnd: Address = env.storage().instance().get(&MockCometKey::Blnd).unwrap();
        if token == blnd {
            8_000_000
        } else {
            2_000_000
        }
    }

    pub fn get_total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&MockCometKey::TotalSupply)
            .unwrap()
    }

    pub fn get_balance(env: Env, token: Address) -> i128 {
        let blnd: Address = env.storage().instance().get(&MockCometKey::Blnd).unwrap();
        if token == blnd {
            return env
                .storage()
                .instance()
                .get(&MockCometKey::BlndReserve)
                .unwrap();
        }
        let paired: Address = env.storage().instance().get(&MockCometKey::Paired).unwrap();
        if token == paired {
            return env
                .storage()
                .instance()
                .get(&MockCometKey::PairedReserve)
                .unwrap();
        }
        0
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get(&MockCometKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        if amount < 0 {
            panic!("negative mint");
        }
        let balance = Self::balance(env.clone(), to.clone());
        env.storage().instance().set(
            &MockCometKey::Balance(to),
            &balance.checked_add(amount).unwrap(),
        );
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if amount < 0 {
            panic!("negative transfer");
        }
        let from_balance = Self::balance(env.clone(), from.clone());
        if amount > from_balance {
            panic!("insufficient balance");
        }
        let to_balance = Self::balance(env.clone(), to.clone());
        env.storage().instance().set(
            &MockCometKey::Balance(from),
            &from_balance.checked_sub(amount).unwrap(),
        );
        env.storage().instance().set(
            &MockCometKey::Balance(to),
            &to_balance.checked_add(amount).unwrap(),
        );
    }

    pub fn dep_tokn_amt_in_get_lp_tokns_out(
        env: Env,
        token: Address,
        amount: i128,
        min_lp_tokens_out: i128,
        to: Address,
    ) -> i128 {
        // A deterministic 1:1 LP output makes each claim return the exact BLND
        // amount consumed, so the differential test compares emission
        // accounting rather than Comet slippage.
        if amount <= 0 || amount < min_lp_tokens_out {
            panic!("invalid single-sided deposit");
        }
        let comet = env.current_contract_address();
        let approval_ledger = (env.ledger().sequence() / 100_000 + 1) * 100_000;
        TokenClient::new(&env, &token).approve(&to, &comet, &amount, &approval_ledger);
        let transfer_args: Vec<Val> = vec![
            &env,
            comet.clone().into_val(&env),
            to.clone().into_val(&env),
            comet.clone().into_val(&env),
            amount.into_val(&env),
        ];
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: token.clone(),
                    fn_name: Symbol::new(&env, "transfer_from"),
                    args: transfer_args,
                },
                sub_invocations: vec![&env],
            }),
        ]);
        TokenClient::new(&env, &token).transfer_from(&comet, &to, &comet, &amount);

        let balance = Self::balance(env.clone(), to.clone());
        env.storage().instance().set(
            &MockCometKey::Balance(to),
            &balance.checked_add(amount).unwrap(),
        );
        let supply: i128 = env
            .storage()
            .instance()
            .get(&MockCometKey::TotalSupply)
            .unwrap();
        env.storage().instance().set(
            &MockCometKey::TotalSupply,
            &supply.checked_add(amount).unwrap(),
        );

        let blnd: Address = env.storage().instance().get(&MockCometKey::Blnd).unwrap();
        let reserve_key = if token == blnd {
            MockCometKey::BlndReserve
        } else {
            let paired: Address = env.storage().instance().get(&MockCometKey::Paired).unwrap();
            if token != paired {
                panic!("unsupported single-sided token");
            }
            MockCometKey::PairedReserve
        };
        let reserve: i128 = env.storage().instance().get(&reserve_key).unwrap();
        env.storage()
            .instance()
            .set(&reserve_key, &reserve.checked_add(amount).unwrap());
        amount
    }
}

#[derive(Clone)]
#[contracttype]
enum OracleAsset {
    Stellar(Address),
    Other(Symbol),
}

#[derive(Clone)]
#[contracttype]
struct PriceData {
    price: i128,
    timestamp: u64,
}

#[contract]
struct MockOracle;

#[derive(Clone)]
#[contracttype]
enum MockOracleKey {
    Price(Address),
}

#[contractimpl]
impl MockOracle {
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage()
            .instance()
            .set(&MockOracleKey::Price(asset), &price);
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn lastprice(env: Env, asset: OracleAsset) -> Option<PriceData> {
        match asset {
            OracleAsset::Stellar(address) => env
                .storage()
                .instance()
                .get(&MockOracleKey::Price(address))
                .map(|price| PriceData {
                    price,
                    timestamp: env.ledger().timestamp(),
                }),
            OracleAsset::Other(_) => None,
        }
    }
}

#[contract]
struct MockV3PoolFactory;

#[derive(Clone)]
#[contracttype]
enum MockV3PoolFactoryKey {
    Backstop,
    Pool(Address),
}

#[contractimpl]
impl MockV3PoolFactory {
    pub fn __constructor(env: Env, backstop: Address) {
        env.storage()
            .instance()
            .set(&MockV3PoolFactoryKey::Backstop, &backstop);
    }

    pub fn backstop(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&MockV3PoolFactoryKey::Backstop)
            .unwrap()
    }

    pub fn is_pool(env: Env, pool: Address) -> bool {
        env.storage()
            .instance()
            .get(&MockV3PoolFactoryKey::Pool(pool))
            .unwrap_or(false)
    }

    pub fn set_pool(env: Env, pool: Address) {
        env.storage()
            .instance()
            .set(&MockV3PoolFactoryKey::Pool(pool), &true);
    }
}

#[contract]
struct MockV3Pool;

#[derive(Clone)]
#[contracttype]
struct MockPositions {
    liabilities: Map<u32, i128>,
    collateral: Map<u32, i128>,
    supply: Map<u32, i128>,
}

#[contractimpl]
impl MockV3Pool {
    pub fn get_positions(env: Env, _address: Address) -> MockPositions {
        MockPositions {
            liabilities: Map::new(&env),
            collateral: Map::new(&env),
            supply: Map::new(&env),
        }
    }
}

fn reserve_config() -> v2_pool::ReserveConfig {
    v2_pool::ReserveConfig {
        index: 0,
        decimals: 7,
        c_factor: 7_500_000,
        l_factor: 7_500_000,
        util: 7_500_000,
        max_util: 9_500_000,
        r_base: 100_000,
        r_one: 500_000,
        r_two: 5_000_000,
        r_three: 15_000_000,
        reactivity: 20,
        supply_cap: 1_000_000_000 * SCALAR_7,
        enabled: true,
    }
}

fn assert_positions_unchanged(before: &v2_pool::Positions, after: &v2_pool::Positions) {
    assert_eq!(after.liabilities, before.liabilities);
    assert_eq!(after.collateral, before.collateral);
    assert_eq!(after.supply, before.supply);
}

fn assert_reserve_unchanged(before: &v2_pool::Reserve, after: &v2_pool::Reserve) {
    assert_eq!(after.asset, before.asset);
    assert_eq!(after.scalar, before.scalar);
    assert_eq!(after.config.index, before.config.index);
    assert_eq!(after.config.decimals, before.config.decimals);
    assert_eq!(after.config.c_factor, before.config.c_factor);
    assert_eq!(after.config.l_factor, before.config.l_factor);
    assert_eq!(after.config.util, before.config.util);
    assert_eq!(after.config.max_util, before.config.max_util);
    assert_eq!(after.config.r_base, before.config.r_base);
    assert_eq!(after.config.r_one, before.config.r_one);
    assert_eq!(after.config.r_two, before.config.r_two);
    assert_eq!(after.config.r_three, before.config.r_three);
    assert_eq!(after.config.reactivity, before.config.reactivity);
    assert_eq!(after.config.supply_cap, before.config.supply_cap);
    assert_eq!(after.config.enabled, before.config.enabled);
    assert_eq!(after.data.d_rate, before.data.d_rate);
    assert_eq!(after.data.b_rate, before.data.b_rate);
    assert_eq!(after.data.ir_mod, before.data.ir_mod);
    assert_eq!(after.data.b_supply, before.data.b_supply);
    assert_eq!(after.data.d_supply, before.data.d_supply);
    assert_eq!(after.data.backstop_credit, before.data.backstop_credit);
    assert_eq!(after.data.last_time, before.data.last_time);
}

fn assert_backstop_user_unchanged(
    before: &v2_backstop::UserBalance,
    after: &v2_backstop::UserBalance,
) {
    assert_eq!(after.shares, before.shares);
    assert_eq!(after.q4w, before.q4w);
}

fn assert_backstop_pool_unchanged(
    before: &v2_backstop::PoolBackstopData,
    after: &v2_backstop::PoolBackstopData,
) {
    assert_eq!(after.tokens, before.tokens);
    assert_eq!(after.shares, before.shares);
    assert_eq!(after.q4w_pct, before.q4w_pct);
    assert_eq!(after.blnd, before.blnd);
    assert_eq!(after.usdc, before.usdc);
    assert_eq!(after.token_spot_price, before.token_spot_price);
}

const DAY: u64 = 24 * 60 * 60;
const USER_A_LP: i128 = 30_000 * SCALAR_7;
const USER_B_LP: i128 = 20_000 * SCALAR_7;
const QUEUED_LP: i128 = 10_000 * SCALAR_7;
const DEQUEUED_LP: i128 = 5_000 * SCALAR_7;
const REMOVAL_QUEUE_B_LP: i128 = 7_001 * SCALAR_7;

struct BackfillFixture {
    env: Env,
    blnd: Address,
    usdc: Address,
    xlm: Address,
    blnd_usdc: Address,
    blnd_xlm: Address,
    emitter: Address,
    candidate: Address,
    factory: Address,
    pool: Address,
    user_a: Address,
    user_b: Address,
}

#[derive(Debug)]
struct BackfillOutcome {
    scheduled: i128,
    pool_tranche: i128,
    user_a_lp: i128,
    user_b_lp: i128,
    withdrawn_lp: i128,
}

fn create_backfill_fixture() -> BackfillFixture {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000);

    let token_admin = Address::generate(&env);
    let blnd = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let usdc = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let xlm = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let blnd_usdc = env.register(
        MockComet,
        (
            &blnd,
            &usdc,
            &(120_000 * SCALAR_7),
            &(1_200_012 * SCALAR_7),
            &(30_000 * SCALAR_7),
        ),
    );
    let blnd_xlm = env.register(
        MockComet,
        (
            &blnd,
            &xlm,
            &(120_000 * SCALAR_7),
            &(960_000 * SCALAR_7),
            &(240_000 * SCALAR_7),
        ),
    );

    let incumbent = Address::generate(&env);
    let candidate = Address::generate(&env);
    let emitter = env.register(DEPLOYED_EMITTER_WASM, ());
    DeployedEmitterClient::new(&env, &emitter).initialize(&blnd, &incumbent, &blnd_usdc);
    StellarAssetClient::new(&env, &blnd).set_admin(&emitter);

    let factory = env.register(MockV3PoolFactory, (&candidate,));
    let pool = env.register(MockV3Pool, ());
    MockV3PoolFactoryClient::new(&env, &factory).set_pool(&pool);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let lp = MockCometClient::new(&env, &blnd_usdc);
    // The candidate withdraws all but 7,999 LP before replacement. Preserve
    // the emitter's strict direct-balance majority through those withdrawals.
    lp.mint(
        &incumbent,
        &(USER_A_LP + USER_B_LP - DEQUEUED_LP - USER_A_LP - REMOVAL_QUEUE_B_LP - 1),
    );
    lp.mint(&user_a, &USER_A_LP);
    lp.mint(&user_b, &USER_B_LP);

    BackfillFixture {
        env,
        blnd,
        usdc,
        xlm,
        blnd_usdc,
        blnd_xlm,
        emitter,
        candidate,
        factory,
        pool,
        user_a,
        user_b,
    }
}

fn run_v2_backfill_timeline() -> BackfillOutcome {
    let fixture = create_backfill_fixture();
    let env = &fixture.env;
    env.register_at(
        &fixture.candidate,
        v2_backstop::WASM,
        (
            &fixture.blnd_usdc,
            &fixture.emitter,
            &fixture.blnd,
            &fixture.usdc,
            &fixture.factory,
            Vec::<(Address, i128)>::new(env),
        ),
    );
    let backstop = v2_backstop::Client::new(env, &fixture.candidate);
    let emitter = DeployedEmitterClient::new(env, &fixture.emitter);
    assert_eq!(
        backstop.deposit(&fixture.user_a, &fixture.pool, &USER_A_LP),
        USER_A_LP
    );
    assert_eq!(
        backstop.deposit(&fixture.user_b, &fixture.pool, &USER_B_LP),
        USER_B_LP
    );
    backstop.add_reward(&fixture.pool, &None);

    let start = env.ledger().timestamp();
    assert_eq!(backstop.distribute(), 0);
    emitter.queue_swap_backstop(&fixture.candidate, &fixture.blnd_xlm);
    let unlock = emitter.get_queued_swap().unwrap().unlock_time;
    assert_eq!(unlock, start + 31 * DAY);

    let mut scheduled = 0_i128;
    let mut pool_tranche = 0_i128;

    env.ledger().set_timestamp(start + DAY);
    scheduled += backstop.distribute();
    pool_tranche += backstop.gulp_emissions(&fixture.pool);

    env.ledger().set_timestamp(start + 4 * DAY);
    scheduled += backstop.distribute();
    backstop.queue_withdrawal(&fixture.user_b, &fixture.pool, &QUEUED_LP);
    pool_tranche += backstop.gulp_emissions(&fixture.pool);

    env.ledger().set_timestamp(start + 5 * DAY);
    scheduled += backstop.distribute();
    backstop.dequeue_withdrawal(&fixture.user_b, &fixture.pool, &DEQUEUED_LP);
    pool_tranche += backstop.gulp_emissions(&fixture.pool);
    backstop.queue_withdrawal(&fixture.user_a, &fixture.pool, &USER_A_LP);
    backstop.queue_withdrawal(&fixture.user_b, &fixture.pool, &REMOVAL_QUEUE_B_LP);

    env.ledger().set_timestamp(start + 25 * DAY);
    scheduled += backstop.distribute();
    let mut withdrawn_lp = backstop.withdraw(&fixture.user_b, &fixture.pool, &DEQUEUED_LP);
    pool_tranche += backstop.gulp_emissions(&fixture.pool);

    env.ledger().set_timestamp(unlock);
    scheduled += backstop.distribute();
    pool_tranche += backstop.gulp_emissions(&fixture.pool);
    withdrawn_lp += backstop.withdraw(&fixture.user_a, &fixture.pool, &USER_A_LP);
    withdrawn_lp += backstop.withdraw(&fixture.user_b, &fixture.pool, &REMOVAL_QUEUE_B_LP);
    backstop.remove_reward(&fixture.pool);
    assert!(backstop.reward_zone().is_empty());
    emitter.swap_backstop();
    assert_eq!(backstop.distribute(), 0);
    backstop.drop();

    env.ledger().set_timestamp(unlock + 7 * DAY);
    let user_a_lp = backstop.claim(
        &fixture.user_a,
        &soroban_sdk::vec![env, fixture.pool.clone()],
        &0,
    );
    let user_b_lp = backstop.claim(
        &fixture.user_b,
        &soroban_sdk::vec![env, fixture.pool.clone()],
        &0,
    );

    BackfillOutcome {
        scheduled,
        pool_tranche,
        user_a_lp,
        user_b_lp,
        withdrawn_lp,
    }
}

fn run_v3_backfill_timeline() -> BackfillOutcome {
    let fixture = create_backfill_fixture();
    let env = &fixture.env;
    env.register_at(
        &fixture.candidate,
        v3_backstop::WASM,
        (
            &fixture.blnd_usdc,
            &fixture.blnd_xlm,
            &fixture.emitter,
            &fixture.blnd,
            &fixture.usdc,
            &fixture.xlm,
            &fixture.factory,
            Vec::<(Address, i128)>::new(env),
        ),
    );
    let backstop = v3_backstop::Client::new(env, &fixture.candidate);
    let emitter = DeployedEmitterClient::new(env, &fixture.emitter);
    assert_eq!(
        backstop.deposit(
            &v3_backstop::BackstopTier::BlndUsdc,
            &fixture.user_a,
            &fixture.pool,
            &USER_A_LP,
        ),
        USER_A_LP
    );
    assert_eq!(
        backstop.deposit(
            &v3_backstop::BackstopTier::BlndUsdc,
            &fixture.user_b,
            &fixture.pool,
            &USER_B_LP,
        ),
        USER_B_LP
    );
    backstop.add_reward(&fixture.pool, &None);

    let start = env.ledger().timestamp();
    assert_eq!(backstop.distribute(), 0);
    emitter.queue_swap_backstop(&fixture.candidate, &fixture.blnd_xlm);
    let unlock = emitter.get_queued_swap().unwrap().unlock_time;
    assert_eq!(unlock, start + 31 * DAY);

    let mut scheduled = 0_i128;
    let mut pool_tranche = 0_i128;

    env.ledger().set_timestamp(start + DAY);
    scheduled += backstop.distribute();
    pool_tranche += backstop.gulp_emissions(&fixture.pool);

    env.ledger().set_timestamp(start + 4 * DAY);
    scheduled += backstop.distribute();
    backstop.queue_withdrawal(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_b,
        &fixture.pool,
        &QUEUED_LP,
    );
    pool_tranche += backstop.gulp_emissions(&fixture.pool);

    env.ledger().set_timestamp(start + 5 * DAY);
    scheduled += backstop.distribute();
    backstop.dequeue_withdrawal(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_b,
        &fixture.pool,
        &DEQUEUED_LP,
    );
    pool_tranche += backstop.gulp_emissions(&fixture.pool);
    backstop.queue_withdrawal(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_a,
        &fixture.pool,
        &USER_A_LP,
    );
    backstop.queue_withdrawal(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_b,
        &fixture.pool,
        &REMOVAL_QUEUE_B_LP,
    );

    env.ledger().set_timestamp(start + 25 * DAY);
    scheduled += backstop.distribute();
    let mut withdrawn_lp = backstop.withdraw(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_b,
        &fixture.pool,
        &DEQUEUED_LP,
        &fixture.user_b,
    );
    pool_tranche += backstop.gulp_emissions(&fixture.pool);

    env.ledger().set_timestamp(unlock);
    scheduled += backstop.distribute();
    pool_tranche += backstop.gulp_emissions(&fixture.pool);
    withdrawn_lp += backstop.withdraw(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_a,
        &fixture.pool,
        &USER_A_LP,
        &fixture.user_a,
    );
    withdrawn_lp += backstop.withdraw(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_b,
        &fixture.pool,
        &REMOVAL_QUEUE_B_LP,
        &fixture.user_b,
    );
    backstop.remove_reward(&fixture.pool);
    assert!(backstop.reward_zone().is_empty());
    assert_eq!(backstop.migration_state().scheduled_backfill, scheduled);
    emitter.swap_backstop();
    assert_eq!(backstop.distribute(), 0);
    backstop.drop();
    assert_eq!(backstop.migration_state().funded_backfill, Some(scheduled));

    env.ledger().set_timestamp(unlock + 7 * DAY);
    let user_a_lp = backstop.claim(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_a,
        &vec![env, fixture.pool.clone()],
        &0,
    );
    let user_b_lp = backstop.claim(
        &v3_backstop::BackstopTier::BlndUsdc,
        &fixture.user_b,
        &vec![env, fixture.pool.clone()],
        &0,
    );

    BackfillOutcome {
        scheduled,
        pool_tranche,
        user_a_lp,
        user_b_lp,
        withdrawn_lp,
    }
}

fn assert_near(left: i128, right: i128, tolerance: i128, label: &str) {
    let difference = if left >= right {
        left - right
    } else {
        right - left
    };
    assert!(
        difference <= tolerance,
        "{label}: {left} differs from {right} by {difference}, tolerance {tolerance}"
    );
}

#[test]
fn deployed_v2_and_optimized_v3_backfill_accounting_remain_equivalent() {
    let v2 = run_v2_backfill_timeline();
    let v3 = run_v3_backfill_timeline();

    assert_eq!(v2.scheduled, i128::from(31 * DAY) * SCALAR_7);
    assert_eq!(v3.scheduled, v2.scheduled);
    assert_eq!(v3.pool_tranche, v2.pool_tranche);
    assert_eq!(
        v2.withdrawn_lp,
        DEQUEUED_LP + USER_A_LP + REMOVAL_QUEUE_B_LP
    );
    assert_eq!(v3.withdrawn_lp, v2.withdrawn_lp);

    // V3 carries stream, index, and user rounding remainders that deployed v2
    // floors away. Everything else in this matched BLND:USDC backfill timeline
    // must remain equal, and the carry improvement may differ only by dust.
    const ROUNDING_TOLERANCE: i128 = 16;
    assert_near(
        v3.user_a_lp,
        v2.user_a_lp,
        ROUNDING_TOLERANCE,
        "user A backfill claim",
    );
    assert_near(
        v3.user_b_lp,
        v2.user_b_lp,
        ROUNDING_TOLERANCE,
        "user B backfill claim",
    );
    let expected_backstop = v2.scheduled * 7 / 10;
    assert_near(
        v2.user_a_lp + v2.user_b_lp,
        expected_backstop,
        ROUNDING_TOLERANCE,
        "v2 total backfill claim",
    );
    assert_near(
        v3.user_a_lp + v3.user_b_lp,
        expected_backstop,
        ROUNDING_TOLERANCE,
        "v3 total backfill claim",
    );
    assert_eq!(v2.pool_tranche, v2.scheduled * 3 / 10);
}

#[test]
fn sdk_22_v2_positions_survive_accelerated_sdk_27_migration() {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    env.ledger().set_timestamp(1_000);
    #[allow(deprecated)]
    let protocol_version = env.ledger().protocol_version();
    assert_eq!(protocol_version, 27);

    let token_admin = Address::generate(&env);
    let blnd = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let usdc = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let xlm = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    // The BLND:USDC 80:20 fixture has enough reserves for the incumbent v2
    // backstop deposit to clear the real v2 activation threshold.
    let blnd_usdc_lp = env.register(
        MockComet,
        (
            &blnd,
            &usdc,
            &(120_000 * SCALAR_7),
            &(1_200_012 * SCALAR_7),
            &(30_000 * SCALAR_7),
        ),
    );
    let next_lp = env.register(
        MockComet,
        (&blnd, &xlm, &SCALAR_7, &(8 * SCALAR_7), &(2 * SCALAR_7)),
    );
    let blnd_usdc_lp_client = MockCometClient::new(&env, &blnd_usdc_lp);

    let v2_backstop_id = Address::generate(&env);
    let v2_pool_factory_id = Address::generate(&env);
    let emitter_id = env.register(DEPLOYED_EMITTER_WASM, ());
    let emitter = DeployedEmitterClient::new(&env, &emitter_id);
    emitter.initialize(&blnd, &v2_backstop_id, &blnd_usdc_lp);
    StellarAssetClient::new(&env, &blnd).set_admin(&emitter_id);

    env.register_at(
        &v2_backstop_id,
        v2_backstop::WASM,
        (
            &blnd_usdc_lp,
            &emitter_id,
            &blnd,
            &usdc,
            &v2_pool_factory_id,
            Vec::<(Address, i128)>::new(&env),
        ),
    );
    let v2_pool_hash = env.deployer().upload_contract_wasm(v2_pool::WASM);
    let v2_pool_factory = env.register_at(
        &v2_pool_factory_id,
        v2_pool_factory::WASM,
        (&v2_pool_factory::PoolInitMeta {
            pool_hash: v2_pool_hash,
            backstop: v2_backstop_id.clone(),
            blnd_id: blnd.clone(),
        },),
    );
    assert_eq!(v2_pool_factory, v2_pool_factory_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.set_price(&xlm, &SCALAR_7);
    oracle.set_price(&usdc, &SCALAR_7);

    let v2_pool_admin = Address::generate(&env);
    let v2_pool_id = v2_pool_factory::Client::new(&env, &v2_pool_factory_id).deploy(
        &v2_pool_admin,
        &String::from_str(&env, "accelerated-v2"),
        &BytesN::from_array(&env, &[7; 32]),
        &oracle_id,
        &2_000_000,
        &4,
        &0,
    );
    let v2_pool = v2_pool::Client::new(&env, &v2_pool_id);
    v2_pool.queue_set_reserve(&xlm, &reserve_config());
    let xlm_index = v2_pool.set_reserve(&xlm);
    v2_pool.queue_set_reserve(&usdc, &reserve_config());
    let usdc_index = v2_pool.set_reserve(&usdc);
    assert_eq!(xlm_index, 0);
    assert_eq!(usdc_index, 1);

    let v2_backstop_depositor = Address::generate(&env);
    let incumbent_lp_amount = 55_000 * SCALAR_7;
    blnd_usdc_lp_client.mint(&v2_backstop_depositor, &incumbent_lp_amount);
    let v2_backstop = v2_backstop::Client::new(&env, &v2_backstop_id);
    assert_eq!(
        v2_backstop.deposit(&v2_backstop_depositor, &v2_pool_id, &incumbent_lp_amount,),
        incumbent_lp_amount
    );
    v2_pool.set_status(&0);

    let lender = Address::generate(&env);
    let borrower = Address::generate(&env);
    let usdc_admin = StellarAssetClient::new(&env, &usdc);
    let xlm_admin = StellarAssetClient::new(&env, &xlm);
    usdc_admin.mint(&lender, &(1_000 * SCALAR_7));
    xlm_admin.mint(&borrower, &(1_000 * SCALAR_7));
    v2_pool.submit(
        &lender,
        &lender,
        &lender,
        &soroban_sdk::vec![
            &env,
            v2_pool::Request {
                request_type: 0,
                address: usdc.clone(),
                amount: 1_000 * SCALAR_7,
            },
        ],
    );
    v2_pool.submit(
        &borrower,
        &borrower,
        &borrower,
        &soroban_sdk::vec![
            &env,
            v2_pool::Request {
                request_type: 2,
                address: xlm.clone(),
                amount: 1_000 * SCALAR_7,
            },
            v2_pool::Request {
                request_type: 4,
                address: usdc.clone(),
                amount: 100 * SCALAR_7,
            },
        ],
    );

    let candidate_id = Address::generate(&env);
    let v3_pool_factory_id = env.register(MockV3PoolFactory, (&candidate_id,));
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &blnd_usdc_lp,
            &next_lp,
            &emitter_id,
            &blnd,
            &usdc,
            &xlm,
            &v3_pool_factory_id,
            Vec::<(Address, i128)>::new(&env),
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);
    let v3_pool_id = env.register(MockV3Pool, ());
    MockV3PoolFactoryClient::new(&env, &v3_pool_factory_id).set_pool(&v3_pool_id);
    let candidate_depositor = Address::generate(&env);
    let candidate_lp_amount = incumbent_lp_amount + 1;
    blnd_usdc_lp_client.mint(&candidate_depositor, &candidate_lp_amount);
    candidate.deposit(
        &v3_backstop::BackstopTier::BlndUsdc,
        &candidate_depositor,
        &v3_pool_id,
        &candidate_lp_amount,
    );
    assert_eq!(
        blnd_usdc_lp_client.balance(&candidate_id),
        blnd_usdc_lp_client.balance(&v2_backstop_id) + 1
    );

    let backfill_start = env.ledger().timestamp();
    assert_eq!(candidate.distribute(), 0);
    assert_eq!(
        candidate.migration_state().migration_epoch_start,
        Some(backfill_start)
    );

    emitter.queue_swap_backstop(&candidate_id, &next_lp);
    assert_eq!(candidate.distribute(), 0);
    let unlock_time = emitter.get_queued_swap().unwrap().unlock_time;
    env.ledger().set_timestamp(unlock_time);
    assert_eq!(candidate.distribute(), 0);

    // Snapshot all incumbent economic state immediately before the only call
    // that changes the emitter's registered backstop.
    let lender_before = v2_pool.get_positions(&lender);
    let borrower_before = v2_pool.get_positions(&borrower);
    let xlm_reserve_before = v2_pool.get_reserve(&xlm);
    let usdc_reserve_before = v2_pool.get_reserve(&usdc);
    let v2_backstop_user_before = v2_backstop.user_balance(&v2_pool_id, &v2_backstop_depositor);
    let v2_backstop_pool_before = v2_backstop.pool_data(&v2_pool_id);
    let v2_pool_xlm_before = TokenClient::new(&env, &xlm).balance(&v2_pool_id);
    let v2_pool_usdc_before = TokenClient::new(&env, &usdc).balance(&v2_pool_id);
    let v2_backstop_lp_before = blnd_usdc_lp_client.balance(&v2_backstop_id);
    let lender_usdc_before = TokenClient::new(&env, &usdc).balance(&lender);
    let borrower_xlm_before = TokenClient::new(&env, &xlm).balance(&borrower);
    let borrower_usdc_before = TokenClient::new(&env, &usdc).balance(&borrower);
    let incumbent_blnd_before = TokenClient::new(&env, &blnd).balance(&v2_backstop_id);
    let candidate_blnd_before = TokenClient::new(&env, &blnd).balance(&candidate_id);
    let incumbent_last_distro = emitter.get_last_distro(&v2_backstop_id);

    emitter.swap_backstop();
    assert_eq!(candidate.distribute(), 0);
    assert_eq!(emitter.get_backstop(), candidate_id);
    assert_eq!(emitter.get_last_distro(&candidate_id), unlock_time);

    assert_positions_unchanged(&lender_before, &v2_pool.get_positions(&lender));
    assert_positions_unchanged(&borrower_before, &v2_pool.get_positions(&borrower));
    assert_reserve_unchanged(&xlm_reserve_before, &v2_pool.get_reserve(&xlm));
    assert_reserve_unchanged(&usdc_reserve_before, &v2_pool.get_reserve(&usdc));
    assert_backstop_user_unchanged(
        &v2_backstop_user_before,
        &v2_backstop.user_balance(&v2_pool_id, &v2_backstop_depositor),
    );
    assert_backstop_pool_unchanged(
        &v2_backstop_pool_before,
        &v2_backstop.pool_data(&v2_pool_id),
    );
    assert_eq!(
        TokenClient::new(&env, &xlm).balance(&v2_pool_id),
        v2_pool_xlm_before
    );
    assert_eq!(
        TokenClient::new(&env, &usdc).balance(&v2_pool_id),
        v2_pool_usdc_before
    );
    assert_eq!(
        blnd_usdc_lp_client.balance(&v2_backstop_id),
        v2_backstop_lp_before
    );
    assert_eq!(
        TokenClient::new(&env, &usdc).balance(&lender),
        lender_usdc_before
    );
    assert_eq!(
        TokenClient::new(&env, &xlm).balance(&borrower),
        borrower_xlm_before
    );
    assert_eq!(
        TokenClient::new(&env, &usdc).balance(&borrower),
        borrower_usdc_before
    );

    // The sole incumbent balance change is the emitter's required final mint
    // through the exact finalization timestamp.
    let final_incumbent_distribution = i128::from(unlock_time - incumbent_last_distro) * SCALAR_7;
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&v2_backstop_id),
        incumbent_blnd_before + final_incumbent_distribution
    );
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&candidate_id),
        candidate_blnd_before
    );

    // One second later, ongoing emissions belong only to the candidate.
    env.ledger().set_timestamp(unlock_time + 1);
    assert_eq!(emitter.distribute(), SCALAR_7);
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&candidate_id),
        candidate_blnd_before + SCALAR_7
    );
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&v2_backstop_id),
        incumbent_blnd_before + final_incumbent_distribution
    );

    // Migration does not freeze or import v2 state. The historical contracts
    // still process an ordinary repay, supply, and backstop deposit afterward.
    let borrower_after_repay = v2_pool.submit(
        &borrower,
        &borrower,
        &borrower,
        &soroban_sdk::vec![
            &env,
            v2_pool::Request {
                request_type: 5,
                address: usdc.clone(),
                amount: SCALAR_7,
            },
        ],
    );
    assert!(
        borrower_after_repay.liabilities.get(usdc_index).unwrap()
            < borrower_before.liabilities.get(usdc_index).unwrap()
    );

    usdc_admin.mint(&lender, &SCALAR_7);
    let lender_after_supply = v2_pool.submit(
        &lender,
        &lender,
        &lender,
        &soroban_sdk::vec![
            &env,
            v2_pool::Request {
                request_type: 0,
                address: usdc,
                amount: SCALAR_7,
            },
        ],
    );
    assert!(
        lender_after_supply.supply.get(usdc_index).unwrap()
            > lender_before.supply.get(usdc_index).unwrap()
    );

    blnd_usdc_lp_client.mint(&v2_backstop_depositor, &SCALAR_7);
    assert!(v2_backstop.deposit(&v2_backstop_depositor, &v2_pool_id, &SCALAR_7,) > 0);
    assert!(
        v2_backstop
            .user_balance(&v2_pool_id, &v2_backstop_depositor)
            .shares
            > v2_backstop_user_before.shares
    );
}
