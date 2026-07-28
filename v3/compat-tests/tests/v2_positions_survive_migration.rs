// blend-contract-sdk 1.22.0 embeds the published legacy emitter WASM with
// SHA-256 438a5528cff17ede6fe515f095c43c5f15727af17d006971485e52462e7e7b89.
use blend_contract_sdk::emitter::WASM as LEGACY_EMITTER_WASM;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env, String, Symbol, Vec,
};

mod v2_backstop {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/optimized/backstop.wasm"
    );
}

#[allow(clippy::too_many_arguments)]
mod v2_pool {
    soroban_sdk::contractimport!(file = "../../target/wasm32-unknown-unknown/optimized/pool.wasm");
}

#[allow(clippy::too_many_arguments)]
mod v2_pool_factory {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/optimized/pool_factory.wasm"
    );
}

#[allow(clippy::too_many_arguments)]
mod v3_backstop {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/optimized/blend_v3_backstop.wasm");
}

mod v3_test_valuation_adapter {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/optimized/blend_v3_test_valuation_adapter.wasm"
    );
}

const SCALAR_7: i128 = 10_000_000;
const PREPARATION_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;

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

#[contractimpl]
impl MockV3Pool {
    pub fn backstop_withdrawal_allowed(_env: Env, _backstop: Address) -> bool {
        true
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

    // The legacy 80:20 fixture has enough reserves for the incumbent v2
    // backstop deposit to clear the real v2 activation threshold.
    let legacy_lp = env.register(
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
    let legacy_lp_client = MockCometClient::new(&env, &legacy_lp);

    let v2_backstop_id = Address::generate(&env);
    let v2_pool_factory_id = Address::generate(&env);
    let emitter_id = env.register(LEGACY_EMITTER_WASM, ());
    let emitter = LegacyEmitterClient::new(&env, &emitter_id);
    emitter.initialize(&blnd, &v2_backstop_id, &legacy_lp);
    StellarAssetClient::new(&env, &blnd).set_admin(&emitter_id);

    env.register_at(
        &v2_backstop_id,
        v2_backstop::WASM,
        (
            &legacy_lp,
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
    legacy_lp_client.mint(&v2_backstop_depositor, &incumbent_lp_amount);
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
    let adapter_admin = Address::generate(&env);
    let valuation_adapter_id = env.register(
        v3_test_valuation_adapter::WASM,
        (&adapter_admin, &blnd, &usdc, &legacy_lp, &next_lp),
    );
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &emitter_id,
            &blnd,
            &v3_pool_factory_id,
            &legacy_lp,
            &next_lp,
            &usdc,
            &valuation_adapter_id,
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);
    let v3_pool_id = env.register(MockV3Pool, ());
    MockV3PoolFactoryClient::new(&env, &v3_pool_factory_id).set_pool(&v3_pool_id);
    let candidate_depositor = Address::generate(&env);
    let candidate_lp_amount = incumbent_lp_amount + 1;
    legacy_lp_client.mint(&candidate_depositor, &candidate_lp_amount);
    candidate.deposit_blnd_usdc(&candidate_depositor, &v3_pool_id, &candidate_lp_amount);
    assert_eq!(
        legacy_lp_client.balance(&candidate_id),
        legacy_lp_client.balance(&v2_backstop_id) + 1
    );

    let queue_start = env.ledger().timestamp();
    assert_eq!(candidate.begin_migration(), queue_start);
    let unlock_time = emitter.get_queued_swap().unwrap().unlock_time;
    env.ledger()
        .set_timestamp(unlock_time - PREPARATION_WINDOW_SECONDS);
    assert_eq!(candidate.prepare_migration(), unlock_time);
    env.ledger().set_timestamp(unlock_time);

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
    let v2_backstop_lp_before = legacy_lp_client.balance(&v2_backstop_id);
    let lender_usdc_before = TokenClient::new(&env, &usdc).balance(&lender);
    let borrower_xlm_before = TokenClient::new(&env, &xlm).balance(&borrower);
    let borrower_usdc_before = TokenClient::new(&env, &usdc).balance(&borrower);
    let incumbent_blnd_before = TokenClient::new(&env, &blnd).balance(&v2_backstop_id);
    let candidate_blnd_before = TokenClient::new(&env, &blnd).balance(&candidate_id);
    let incumbent_last_distro = emitter.get_last_distro(&v2_backstop_id);

    assert_eq!(candidate.finalize_migration(), unlock_time);
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
        legacy_lp_client.balance(&v2_backstop_id),
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

    legacy_lp_client.mint(&v2_backstop_depositor, &SCALAR_7);
    assert!(v2_backstop.deposit(&v2_backstop_depositor, &v2_pool_id, &SCALAR_7,) > 0);
    assert!(
        v2_backstop
            .user_balance(&v2_pool_id, &v2_backstop_depositor)
            .shares
            > v2_backstop_user_before.shares
    );
}
