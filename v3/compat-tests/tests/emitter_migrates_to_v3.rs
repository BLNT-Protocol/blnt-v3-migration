// blend-contract-sdk 1.22.0 embeds the published legacy emitter WASM with
// SHA-256 438a5528cff17ede6fe515f095c43c5f15727af17d006971485e52462e7e7b89.
use blend_contract_sdk::emitter::WASM as LEGACY_EMITTER_WASM;
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env, IntoVal,
};

#[allow(clippy::too_many_arguments)]
mod v3_backstop {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/optimized/blend_v3_backstop.wasm");
}

mod v3_pool {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/optimized/blend_v3_pool.wasm");
}

mod v3_pool_factory {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/optimized/blend_v3_pool_factory.wasm"
    );
}

mod v3_test_valuation_adapter {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/optimized/blend_v3_test_valuation_adapter.wasm"
    );
}

fn register_test_valuation_adapter(
    env: &Env,
    blnd: &Address,
    usdc: &Address,
    blnd_usdc: &Address,
    blnd_xlm: &Address,
) -> Address {
    let admin = Address::generate(env);
    env.register(
        v3_test_valuation_adapter::WASM,
        (&admin, blnd, usdc, blnd_usdc, blnd_xlm),
    )
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

#[contract]
struct MockPoolFactory;

#[contract]
struct MockFlashLoanReceiver;

#[contractimpl]
impl MockFlashLoanReceiver {
    pub fn exec_op(_env: Env, _caller: Address, _token: Address, _amount: i128, fee: i128) {
        assert_eq!(fee, 0);
    }
}

#[derive(Clone)]
#[contracttype]
enum MockPoolFactoryKey {
    Backstop,
    Pool(Address),
}

#[contractimpl]
impl MockPoolFactory {
    pub fn __constructor(env: Env, backstop: Address) {
        env.storage()
            .instance()
            .set(&MockPoolFactoryKey::Backstop, &backstop);
    }

    pub fn backstop(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&MockPoolFactoryKey::Backstop)
            .unwrap()
    }

    pub fn is_pool(env: Env, pool: Address) -> bool {
        env.storage()
            .instance()
            .get(&MockPoolFactoryKey::Pool(pool))
            .unwrap_or(false)
    }

    pub fn set_pool(env: Env, pool: Address) {
        env.storage()
            .instance()
            .set(&MockPoolFactoryKey::Pool(pool), &true);
    }
}

#[contract]
struct MockComet;

#[derive(Clone)]
#[contracttype]
enum MockCometKey {
    Balance(Address),
    Blnd,
    BlndReserve,
    TotalSupply,
}

#[contractimpl]
impl MockComet {
    pub fn __constructor(env: Env, blnd: Address, total_supply: i128, blnd_reserve: i128) {
        env.storage().instance().set(&MockCometKey::Blnd, &blnd);
        env.storage()
            .instance()
            .set(&MockCometKey::TotalSupply, &total_supply);
        env.storage()
            .instance()
            .set(&MockCometKey::BlndReserve, &blnd_reserve);
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
            env.storage()
                .instance()
                .get(&MockCometKey::BlndReserve)
                .unwrap()
        } else {
            0
        }
    }

    pub fn set_composition(env: Env, total_supply: i128, blnd_reserve: i128) {
        env.storage()
            .instance()
            .set(&MockCometKey::TotalSupply, &total_supply);
        env.storage()
            .instance()
            .set(&MockCometKey::BlndReserve, &blnd_reserve);
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

#[contract]
struct MockLossAwarePool;

#[derive(Clone)]
#[contracttype]
enum MockLossAwarePoolKey {
    Backstop,
    WithdrawalAllowed,
}

#[contractimpl]
impl MockLossAwarePool {
    pub fn __constructor(env: Env, backstop: Address, withdrawal_allowed: bool) {
        env.storage()
            .instance()
            .set(&MockLossAwarePoolKey::Backstop, &backstop);
        env.storage().instance().set(
            &MockLossAwarePoolKey::WithdrawalAllowed,
            &withdrawal_allowed,
        );
    }

    pub fn backstop_withdrawal_allowed(env: Env, backstop: Address) -> bool {
        let expected_backstop: Address = env
            .storage()
            .instance()
            .get(&MockLossAwarePoolKey::Backstop)
            .unwrap();
        backstop == expected_backstop
            && env
                .storage()
                .instance()
                .get(&MockLossAwarePoolKey::WithdrawalAllowed)
                .unwrap_or(false)
    }

    pub fn set_withdrawal_allowed(env: Env, withdrawal_allowed: bool) {
        env.storage().instance().set(
            &MockLossAwarePoolKey::WithdrawalAllowed,
            &withdrawal_allowed,
        );
    }

    pub fn commit_bad_debt_lot(
        env: Env,
        backstop: Address,
        auction_id: BytesN<32>,
        debt_value: i128,
    ) -> i128 {
        v3_backstop::Client::new(&env, &backstop)
            .commit_bad_debt_lot(&env.current_contract_address(), &auction_id, &debt_value)
            .committed_value
    }

    pub fn release_bad_debt_lot(env: Env, backstop: Address, auction_id: BytesN<32>) {
        v3_backstop::Client::new(&env, &backstop)
            .release_bad_debt_lot(&env.current_contract_address(), &auction_id);
    }

    pub fn settle_bad_debt_lot(
        env: Env,
        backstop: Address,
        auction_id: BytesN<32>,
        base_lot_amount: i128,
        lot_amount: i128,
        to: Address,
    ) -> Option<i128> {
        v3_backstop::Client::new(&env, &backstop)
            .settle_bad_debt_lot(
                &env.current_contract_address(),
                &auction_id,
                &base_lot_amount,
                &lot_amount,
                &to,
            )
            .map(|quote| quote.lot_amount)
    }
}

const SCALAR_7: i128 = 10_000_000;

fn test_lending_config(oracle: &Address) -> v3_pool_factory::LendingConfig {
    v3_pool_factory::LendingConfig {
        backstop_take_rate: 0,
        max_positions: 4,
        min_collateral: 0,
        oracle: oracle.clone(),
        price_decimals: 7,
    }
}

fn lending_reserve_config(asset: &Address, index: u32) -> v3_pool_factory::ReserveConfig {
    v3_pool_factory::ReserveConfig {
        asset: asset.clone(),
        c_factor: 8_000_000,
        decimals: 7,
        enabled: true,
        index,
        l_factor: 9_000_000,
        max_util: 9_500_000,
        r_base: 100_000,
        r_one: 500_000,
        r_three: 15_000_000,
        r_two: 5_000_000,
        reactivity: 20,
        supply_cap: 1_000_000_000_000_000,
        util: 7_500_000,
    }
}

fn direct_lending_config(oracle: &Address) -> v3_pool::LendingConfig {
    v3_pool::LendingConfig {
        backstop_take_rate: 0,
        max_positions: 4,
        min_collateral: 0,
        oracle: oracle.clone(),
        price_decimals: 7,
    }
}

fn direct_lending_reserve_config(asset: &Address, index: u32) -> v3_pool::ReserveConfig {
    v3_pool::ReserveConfig {
        asset: asset.clone(),
        c_factor: 8_000_000,
        decimals: 7,
        enabled: true,
        index,
        l_factor: 9_000_000,
        max_util: 9_500_000,
        r_base: 100_000,
        r_one: 500_000,
        r_three: 15_000_000,
        r_two: 5_000_000,
        reactivity: 20,
        supply_cap: 1_000_000_000_000_000,
        util: 7_500_000,
    }
}

const QUEUE_SECONDS: u64 = 31 * 24 * 60 * 60;
const PREPARATION_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;
const SYNC_GRACE_SECONDS: u64 = 7 * 24 * 60 * 60;
const ABSOLUTE_MIGRATION_CEILING_SECONDS: u64 = 138 * 24 * 60 * 60;

macro_rules! assert_mainnet_invocation_limits {
    ($resources:expr) => {{
        let resources = $resources;
        assert!(resources.instructions <= 600_000_000);
        assert!(resources.mem_bytes <= 41_943_040);
        assert!(resources.disk_read_entries <= 100);
        assert!(resources.write_entries <= 50);
        assert!(
            resources.disk_read_entries + resources.memory_read_entries + resources.write_entries
                <= 90
        );
        assert!(resources.disk_read_bytes <= 200_000);
        assert!(resources.write_bytes <= 132_096);
        assert!(resources.contract_events_size_bytes <= 16_384);
    }};
}

struct Fixture {
    env: Env,
    incumbent: Address,
    candidate: v3_backstop::Client<'static>,
    emitter: LegacyEmitterClient<'static>,
    pool_factory: MockPoolFactoryClient<'static>,
    blnd: TokenClient<'static>,
    legacy_token: StellarAssetClient<'static>,
    next_token: Address,
    prefund_token: Address,
    valuation_adapter: Address,
}

impl Fixture {
    fn create() -> Self {
        Self::create_with_blnd_binding(true)
    }

    fn create_with_blnd_binding(use_emitter_blnd: bool) -> Self {
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
        let wrong_blnd_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let prefund_token_id = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let configured_blnd_id = if use_emitter_blnd {
            blnd_id.clone()
        } else {
            wrong_blnd_id
        };

        let incumbent = Address::generate(&env);
        let emitter_id = env.register(LEGACY_EMITTER_WASM, ());
        StellarAssetClient::new(&env, &blnd_id).set_admin(&emitter_id);
        let emitter = LegacyEmitterClient::new(&env, &emitter_id);
        emitter.initialize(&blnd_id, &incumbent, &legacy_token_id);
        let candidate_id = Address::generate(&env);
        let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
        let valuation_adapter_id = register_test_valuation_adapter(
            &env,
            &configured_blnd_id,
            &prefund_token_id,
            &legacy_token_id,
            &next_token_id,
        );
        env.register_at(
            &candidate_id,
            v3_backstop::WASM,
            (
                &emitter_id,
                &configured_blnd_id,
                &pool_factory_id,
                &legacy_token_id,
                &next_token_id,
                &prefund_token_id,
                &valuation_adapter_id,
            ),
        );
        let candidate = v3_backstop::Client::new(&env, &candidate_id);
        let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
        let blnd = TokenClient::new(&env, &blnd_id);
        let legacy_token = StellarAssetClient::new(&env, &legacy_token_id);

        Self {
            env,
            incumbent,
            candidate,
            emitter,
            pool_factory,
            blnd,
            legacy_token,
            next_token: next_token_id,
            prefund_token: prefund_token_id,
            valuation_adapter: valuation_adapter_id,
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

    fn loss_aware_pool(&self, withdrawal_allowed: bool) -> MockLossAwarePoolClient<'static> {
        let pool_id = self.env.register(
            MockLossAwarePool,
            (&self.candidate.address, withdrawal_allowed),
        );
        self.pool_factory.set_pool(&pool_id);
        MockLossAwarePoolClient::new(&self.env, &pool_id)
    }
}

#[test]
fn v3_wasm_exposes_asset_neutral_activation_hysteresis_quote() {
    let fixture = Fixture::create();
    let entry_threshold = 12_500 * SCALAR_7;
    let maintenance_threshold = 10_000 * SCALAR_7;

    assert_eq!(
        fixture.candidate.activation_entry_threshold(),
        entry_threshold
    );
    assert_eq!(
        fixture.candidate.activation_maintenance_threshold(),
        maintenance_threshold
    );
    assert_eq!(
        fixture.candidate.pool_factory(),
        fixture.pool_factory.address
    );
    assert_eq!(
        fixture.candidate.valuation_adapter(),
        fixture.valuation_adapter
    );

    let values = v3_backstop::ActivationValues {
        blnd_usdc: 4_000 * SCALAR_7,
        blnd_xlm: 4_000 * SCALAR_7,
        usdc: 4_000 * SCALAR_7,
    };
    let inactive_quote = fixture.candidate.quote_activation(&values, &false);
    assert_eq!(inactive_quote.eligible_value, 12_000 * SCALAR_7);
    assert_eq!(inactive_quote.required_value, entry_threshold);
    assert!(!inactive_quote.meets_threshold);

    let active_quote = fixture.candidate.quote_activation(&values, &true);
    assert_eq!(active_quote.eligible_value, 12_000 * SCALAR_7);
    assert_eq!(active_quote.required_value, maintenance_threshold);
    assert!(active_quote.meets_threshold);
}

#[test]
fn v3_wasm_exposes_bounded_prefunding_reward_zone() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let candidate_id = Address::generate(&env);
    let incumbent = Address::generate(&env);
    let emitter_id = env.register(LEGACY_EMITTER_WASM, ());
    let blnd = env
        .register_stellar_asset_contract_v2(emitter_id.clone())
        .address();
    let blnd_usdc = env.register(
        MockComet,
        (&blnd, &(1_000_000 * SCALAR_7), &(4_000_000 * SCALAR_7)),
    );
    let blnd_xlm = env.register(
        MockComet,
        (&blnd, &(1_000_000 * SCALAR_7), &(2_000_000 * SCALAR_7)),
    );
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    LegacyEmitterClient::new(&env, &emitter_id).initialize(&blnd, &incumbent, &blnd_usdc);
    let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
    let valuation_adapter =
        register_test_valuation_adapter(&env, &blnd, &usdc, &blnd_usdc, &blnd_xlm);
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &emitter_id,
            &blnd,
            &pool_factory_id,
            &blnd_usdc,
            &blnd_xlm,
            &usdc,
            &valuation_adapter,
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);
    let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
    let first = env.register(MockLossAwarePool, (&candidate_id, false));
    let second = env.register(MockLossAwarePool, (&candidate_id, false));
    pool_factory.set_pool(&first);
    pool_factory.set_pool(&second);

    let depositor = Address::generate(&env);
    let usdc_token = StellarAssetClient::new(&env, &usdc);
    let blnd_usdc_token = MockCometClient::new(&env, &blnd_usdc);
    let blnd_xlm_token = MockCometClient::new(&env, &blnd_xlm);
    let qualifying_amount = 12_500 * SCALAR_7;

    usdc_token.mint(&depositor, &(2 * qualifying_amount));
    candidate.deposit_usdc(&depositor, &first, &qualifying_amount);
    candidate.deposit_usdc(&depositor, &second, &qualifying_amount);

    let usdc_only_valuation = candidate.pool_valuation(&first);
    assert!(usdc_only_valuation.active_values.usdc >= qualifying_amount);
    assert!(
        candidate
            .quote_activation(&usdc_only_valuation.active_values, &false)
            .meets_threshold
    );
    assert!(candidate.try_add_reward(&first, &None).is_err());
    assert!(candidate.reward_zone().is_empty());

    // USDC alone activates the pool, while either BLND-bearing tier can make
    // it eligible for permissionless reward-zone admission.
    blnd_xlm_token.mint(&depositor, &SCALAR_7);
    candidate.deposit_blnd_xlm(&depositor, &first, &SCALAR_7);
    env.cost_estimate().budget().reset_unlimited();
    candidate.add_reward(&first, &None);
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(
        candidate.reward_zone(),
        soroban_sdk::vec![&env, first.clone()]
    );
    assert!(candidate.try_add_reward(&first, &None).is_err());

    // Before migration activation, another qualified pool may enter without
    // a checkpoint because candidate emissions cannot yet accrue.
    blnd_usdc_token.mint(&depositor, &SCALAR_7);
    candidate.deposit_blnd_usdc(&depositor, &second, &SCALAR_7);
    assert!(candidate.pool_spot_blnd_emission_values(&second).blnd_usdc > 0);
    env.cost_estimate().budget().reset_unlimited();
    candidate.add_reward(&second, &None);
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    let reward_zone = candidate.reward_zone();
    assert_eq!(reward_zone.len(), 2);
    assert!(reward_zone.contains(first));
    assert!(reward_zone.contains(second));
}

#[test]
fn optimized_v3_distributes_ongoing_blnd_across_maximum_reward_zone() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let incumbent = Address::generate(&env);
    let emitter_id = env.register(LEGACY_EMITTER_WASM, ());
    let blnd = env
        .register_stellar_asset_contract_v2(emitter_id.clone())
        .address();
    let blnd_usdc = env.register(
        MockComet,
        (&blnd, &(1_000_000 * SCALAR_7), &(4_000_000 * SCALAR_7)),
    );
    let blnd_xlm = env.register(
        MockComet,
        (&blnd, &(1_000_000 * SCALAR_7), &(2_000_000 * SCALAR_7)),
    );
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let emitter = LegacyEmitterClient::new(&env, &emitter_id);
    emitter.initialize(&blnd, &incumbent, &blnd_usdc);

    let candidate_id = Address::generate(&env);
    let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
    let valuation_adapter =
        register_test_valuation_adapter(&env, &blnd, &usdc, &blnd_usdc, &blnd_xlm);
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &emitter_id,
            &blnd,
            &pool_factory_id,
            &blnd_usdc,
            &blnd_xlm,
            &usdc,
            &valuation_adapter,
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);
    let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
    let blnd_usdc_token = MockCometClient::new(&env, &blnd_usdc);
    let depositor = Address::generate(&env);
    let per_pool = 12_500 * SCALAR_7;
    blnd_usdc_token.mint(&incumbent, &per_pool);
    blnd_usdc_token.mint(&depositor, &(30 * per_pool));

    let pool_admin = Address::generate(&env);
    let emission_asset_admin = Address::generate(&env);
    let mut emission_assets = soroban_sdk::Vec::new(&env);
    let mut emission_reserves = soroban_sdk::Vec::new(&env);
    let mut emission_metadata = soroban_sdk::Vec::new(&env);
    let mut emission_claim_ids = soroban_sdk::Vec::new(&env);
    for index in 0..30_u32 {
        let asset = if index == 0 {
            usdc.clone()
        } else {
            env.register_stellar_asset_contract_v2(emission_asset_admin.clone())
                .address()
        };
        emission_reserves.push_back(direct_lending_reserve_config(&asset, index));
        for res_type in 0..=1_u32 {
            emission_metadata.push_back(v3_pool::ReserveEmissionMetadata {
                res_index: index,
                res_type,
                share: 1,
            });
            emission_claim_ids.push_back(index * 2 + res_type);
        }
        emission_assets.push_back(asset);
    }
    assert_eq!(emission_metadata.len(), 60);
    assert_eq!(emission_claim_ids.len(), 60);
    let mut emission_lending_config = direct_lending_config(&valuation_adapter);
    emission_lending_config.max_positions = 60;
    let emission_pool_id = env.register(
        v3_pool::WASM,
        (
            &candidate_id,
            &pool_admin,
            &emission_lending_config,
            &emission_reserves,
        ),
    );
    let mut pools = soroban_sdk::Vec::new(&env);
    for index in 0..30 {
        let pool = if index == 0 {
            emission_pool_id.clone()
        } else {
            env.register(MockLossAwarePool, (&candidate_id, false))
        };
        pool_factory.set_pool(&pool);
        candidate.deposit_blnd_usdc(&depositor, &pool, &per_pool);
        pools.push_back(pool);
    }
    let first = pools.get(0).unwrap();
    let emission_pool = v3_pool::Client::new(&env, &emission_pool_id);
    assert_eq!(emission_pool.set_pool_status(&0), 0);
    let supplier = Address::generate(&env);
    for asset in emission_assets.iter() {
        StellarAssetClient::new(&env, &asset).mint(&supplier, &(1_000 * SCALAR_7));
        emission_pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &soroban_sdk::vec![
                &env,
                v3_pool::Request {
                    address: asset,
                    amount: 1_000 * SCALAR_7,
                    request_type: 0,
                },
            ],
        );
    }
    emission_pool.set_emissions_config(&emission_metadata);
    candidate.add_reward(&first, &None);

    candidate.begin_migration();
    let unlock = emitter.get_queued_swap().unwrap().unlock_time;
    env.ledger()
        .set_timestamp(unlock - PREPARATION_WINDOW_SECONDS);
    candidate.prepare_migration();
    env.ledger().set_timestamp(unlock);
    emitter.swap_backstop();

    env.ledger().set_timestamp(unlock + 10);
    assert_eq!(emitter.distribute(), 10 * SCALAR_7);
    assert_eq!(candidate.sync_migration(), unlock + 10);
    assert_eq!(candidate.backfill_end(), Some(unlock));
    assert_eq!(
        candidate.scheduled_backfill(),
        QUEUE_SECONDS as i128 * SCALAR_7
    );
    assert!(!candidate.blnd_binding_verified());
    assert_eq!(candidate.fund_backfill(), QUEUE_SECONDS as i128 * SCALAR_7);
    assert!(candidate.blnd_binding_verified());

    env.ledger().set_timestamp(unlock + 15);
    assert_eq!(candidate.distribute().received, 15 * SCALAR_7);
    for index in 1..30 {
        candidate.add_reward(&pools.get(index).unwrap(), &None);
    }
    assert_eq!(candidate.reward_zone().len(), 30);

    env.ledger().set_timestamp(unlock + 20);
    env.cost_estimate().budget().reset_unlimited();
    let distribution = candidate.distribute();
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(distribution.received, 5 * SCALAR_7);
    assert_eq!(distribution.eligible_blnd, 1_500_000 * SCALAR_7);
    assert_eq!(
        candidate.ongoing_emission_state().total_received,
        20 * SCALAR_7
    );
    for pool in pools.iter() {
        let emissions = candidate.pool_ongoing_emissions(&pool);
        assert!(emissions.accrued_backstop > 0);
        assert!(emissions.accrued_pool > 0);
        assert_eq!(emissions.active_blnd_usdc, per_pool);
        assert_eq!(emissions.active_blnd_xlm, 0);
    }

    env.ledger().set_timestamp(unlock + 25);
    blnd_usdc_token.mint(&depositor, &SCALAR_7);
    env.cost_estimate().budget().reset_unlimited();
    candidate.deposit_blnd_usdc(&depositor, &first, &SCALAR_7);
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(
        candidate.ongoing_emission_state().total_received,
        20 * SCALAR_7
    );
    assert_eq!(
        candidate.pool_ongoing_emissions(&first).active_blnd_usdc,
        per_pool + SCALAR_7
    );

    env.ledger().set_timestamp(unlock + 30);
    let first_before = candidate.pool_ongoing_emissions(&first);
    let second_before = candidate.pool_ongoing_emissions(&pools.get(1).unwrap());
    assert_eq!(candidate.distribute().received, 10 * SCALAR_7);
    let first_after = candidate.pool_ongoing_emissions(&first);
    let second_after = candidate.pool_ongoing_emissions(&pools.get(1).unwrap());
    assert!(
        first_after.accrued_backstop - first_before.accrued_backstop
            > second_after.accrued_backstop - second_before.accrued_backstop
    );
    assert_eq!(
        candidate.ongoing_emission_state().total_received,
        30 * SCALAR_7
    );

    let recipient = Address::generate(&env);
    let claimable = candidate
        .user_ongoing_emissions(&depositor, &first, &v3_backstop::BackstopTier::BlndUsdc)
        .accrued;
    let accrued_pool_before = first_after.accrued_pool;
    let candidate_balance_before = TokenClient::new(&env, &blnd).balance(&candidate_id);
    assert!(claimable > 0);
    env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        candidate.claim_ongoing_blnd(&depositor, &first, &recipient),
        claimable
    );
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(TokenClient::new(&env, &blnd).balance(&recipient), claimable);
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&candidate_id),
        candidate_balance_before - claimable
    );
    let first_claimed = candidate.pool_ongoing_emissions(&first);
    assert_eq!(first_claimed.accrued_pool, accrued_pool_before);
    assert_eq!(
        first_claimed.accrued_backstop,
        first_after.accrued_backstop - claimable
    );
    let ongoing = candidate.ongoing_emission_state();
    assert_eq!(ongoing.backstop_claimed, claimable);
    assert_eq!(ongoing.total_claimed, claimable);

    // The optimized pool reserves its 30% tranche through contract
    // authorization, streams it for seven days, and has the candidate pay the
    // eventual user claim directly without an allowance.
    env.cost_estimate().budget().reset_unlimited();
    let pool_tranche = emission_pool.gulp_emissions();
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert!(pool_tranche >= SCALAR_7);
    assert_eq!(
        candidate
            .pool_emission_reservation(&emission_pool_id)
            .available,
        pool_tranche
    );
    env.ledger().set_timestamp(unlock + 30 + 7 * 24 * 60 * 60);
    let supply_recipient = Address::generate(&env);
    env.cost_estimate().budget().reset_unlimited();
    let supply_claim = emission_pool.claim(&supplier, &emission_claim_ids, &supply_recipient);
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert!(supply_claim > 0);
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&supply_recipient),
        supply_claim
    );
    assert_eq!(
        candidate
            .pool_emission_reservation(&emission_pool_id)
            .available,
        pool_tranche - supply_claim
    );
}

#[test]
fn direct_transfer_only_migration_binds_blnd_through_first_distribution() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let incumbent = Address::generate(&env);
    let emitter_id = env.register(LEGACY_EMITTER_WASM, ());
    let blnd = env
        .register_stellar_asset_contract_v2(emitter_id.clone())
        .address();
    let blnd_usdc = env.register(
        MockComet,
        (&blnd, &(1_000_000 * SCALAR_7), &(4_000_000 * SCALAR_7)),
    );
    let blnd_xlm = env.register(
        MockComet,
        (&blnd, &(1_000_000 * SCALAR_7), &(2_000_000 * SCALAR_7)),
    );
    let usdc = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let emitter = LegacyEmitterClient::new(&env, &emitter_id);
    emitter.initialize(&blnd, &incumbent, &blnd_usdc);

    let candidate_id = Address::generate(&env);
    let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
    let valuation_adapter =
        register_test_valuation_adapter(&env, &blnd, &usdc, &blnd_usdc, &blnd_xlm);
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &emitter_id,
            &blnd,
            &pool_factory_id,
            &blnd_usdc,
            &blnd_xlm,
            &usdc,
            &valuation_adapter,
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);
    let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
    let blnd_usdc_token = MockCometClient::new(&env, &blnd_usdc);

    // Raw LP balances satisfy the immutable emitter, but deliberately create
    // no candidate shares or migration-backfill weight.
    blnd_usdc_token.mint(&incumbent, &(100 * SCALAR_7));
    blnd_usdc_token.mint(&candidate_id, &(100 * SCALAR_7 + 1));
    candidate.begin_migration();
    let unlock = emitter.get_queued_swap().unwrap().unlock_time;
    env.ledger()
        .set_timestamp(unlock - PREPARATION_WINDOW_SECONDS);
    candidate.prepare_migration();
    env.ledger().set_timestamp(unlock);
    candidate.finalize_migration();

    assert_eq!(candidate.total_migration_weight(), 0);
    assert!(candidate.try_fund_backfill().is_err());
    assert_eq!(candidate.funded_backfill(), None);
    assert!(!candidate.blnd_binding_verified());

    env.ledger().set_timestamp(unlock + 10);
    assert_eq!(emitter.distribute(), 10 * SCALAR_7);

    // Ordinary post-migration capital can establish a reward-zone weight. The
    // first positive emitter distribution then proves the configured BLND
    // binding by exact current-call balance delta, without minting backfill,
    // while reconciling the earlier direct distribution exactly once.
    let pool = env.register(MockLossAwarePool, (&candidate_id, false));
    pool_factory.set_pool(&pool);
    let depositor = Address::generate(&env);
    blnd_usdc_token.mint(&depositor, &(12_500 * SCALAR_7));
    candidate.deposit_blnd_usdc(&depositor, &pool, &(12_500 * SCALAR_7));
    candidate.add_reward(&pool, &None);

    env.ledger().set_timestamp(unlock + 15);
    env.cost_estimate().budget().reset_unlimited();
    let distribution = candidate.distribute();
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(distribution.received, 15 * SCALAR_7);
    assert!(candidate.blnd_binding_verified());
    assert_eq!(candidate.funded_backfill(), None);
    assert_eq!(
        TokenClient::new(&env, &blnd).balance(&candidate_id),
        15 * SCALAR_7
    );
}

#[test]
fn v3_wasm_derives_canonical_pool_valuation_from_accounted_shares() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(false).address.clone();
    let depositor = Address::generate(&fixture.env);
    let blnd_xlm = StellarAssetClient::new(&fixture.env, &fixture.next_token);
    let usdc = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);

    fixture.legacy_token.mint(&depositor, &(5_000 * SCALAR_7));
    blnd_xlm.mint(&depositor, &(5_000 * SCALAR_7));
    usdc.mint(&depositor, &(5_500 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&depositor, &pool, &(5_000 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_xlm(&depositor, &pool, &(5_000 * SCALAR_7));
    fixture
        .candidate
        .deposit_usdc(&depositor, &pool, &(5_500 * SCALAR_7));
    fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&depositor, &pool, &(1_000 * SCALAR_7));
    fixture
        .candidate
        .queue_blnd_xlm_withdrawal(&depositor, &pool, &(1_000 * SCALAR_7));
    fixture
        .candidate
        .queue_usdc_withdrawal(&depositor, &pool, &(1_000 * SCALAR_7));

    for (tier, assets) in [
        (v3_backstop::BackstopTier::BlndUsdc, 5_000 * SCALAR_7),
        (v3_backstop::BackstopTier::BlndXlm, 5_000 * SCALAR_7),
        (v3_backstop::BackstopTier::Usdc, 5_500 * SCALAR_7),
    ] {
        assert_eq!(
            fixture.candidate.pool_tier_state(&tier, &pool),
            v3_backstop::PoolTierState {
                assets,
                queued_shares: 1_000 * SCALAR_7,
                shares: assets,
            }
        );
        assert_eq!(fixture.candidate.total_tier_assets(&tier), assets);
        assert_eq!(
            fixture
                .candidate
                .preview_tier_deposit(&tier, &pool, &SCALAR_7),
            SCALAR_7
        );
        assert_eq!(
            fixture
                .candidate
                .preview_tier_withdrawal(&tier, &pool, &SCALAR_7),
            SCALAR_7
        );
    }

    let valuation = fixture.candidate.pool_valuation(&pool);
    assert_eq!(
        valuation.active_blnd,
        v3_backstop::BlndEmissionValues {
            blnd_usdc: 4_000 * SCALAR_7,
            blnd_xlm: 4_000 * SCALAR_7,
        }
    );
    assert_eq!(
        valuation.active_values,
        v3_backstop::ActivationValues {
            blnd_usdc: 4_000 * SCALAR_7,
            blnd_xlm: 4_000 * SCALAR_7,
            usdc: 4_500 * SCALAR_7,
        }
    );
    assert_eq!(
        valuation.queued_values,
        v3_backstop::ActivationValues {
            blnd_usdc: 1_000 * SCALAR_7,
            blnd_xlm: 1_000 * SCALAR_7,
            usdc: 1_000 * SCALAR_7,
        }
    );
    assert_eq!(valuation.valid_until, u64::MAX);
    assert!(
        fixture
            .candidate
            .quote_pool_activation(&pool, &false)
            .meets_threshold
    );
    assert!(fixture
        .candidate
        .try_pool_valuation(&Address::generate(&fixture.env))
        .is_err());
}

#[test]
fn v3_wasm_commits_and_settles_strict_pool_authorized_bad_debt_lot() {
    let fixture = Fixture::create();
    let pool_client = fixture.loss_aware_pool(true);
    let pool = pool_client.address.clone();
    let depositor = Address::generate(&fixture.env);
    let next_token = StellarAssetClient::new(&fixture.env, &fixture.next_token);
    let usdc = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);
    fixture.legacy_token.mint(&depositor, &(50 * SCALAR_7));
    next_token.mint(&depositor, &(100 * SCALAR_7));
    usdc.mint(&depositor, &(500 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&depositor, &pool, &(50 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_xlm(&depositor, &pool, &(100 * SCALAR_7));
    fixture
        .candidate
        .deposit_usdc(&depositor, &pool, &(500 * SCALAR_7));

    let debt_value = 200 * SCALAR_7;
    let auction_id = BytesN::from_array(&fixture.env, &[3; 32]);
    let quote = fixture
        .candidate
        .quote_bad_debt_lot(&pool, &debt_value)
        .unwrap();
    assert_eq!(quote.target_value, 240 * SCALAR_7);
    assert_eq!(quote.committed_value, 240 * SCALAR_7);
    assert_eq!(quote.tier, v3_backstop::BackstopTier::Usdc);
    assert_eq!(quote.lot_amount, 240 * SCALAR_7);
    assert_eq!(quote.unfilled_target_value, 0);

    assert_eq!(
        pool_client.commit_bad_debt_lot(&fixture.candidate.address, &auction_id, &debt_value,),
        240 * SCALAR_7
    );
    assert_eq!(fixture.candidate.pool_bad_debt_commitment_count(&pool), 1);
    assert_eq!(
        fixture
            .candidate
            .pool_tier_committed_assets(&v3_backstop::BackstopTier::BlndUsdc, &pool),
        0
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_committed_assets(&v3_backstop::BackstopTier::BlndXlm, &pool),
        0
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_committed_assets(&v3_backstop::BackstopTier::Usdc, &pool),
        240 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_valuation(&pool).active_values,
        v3_backstop::ActivationValues {
            blnd_usdc: 50 * SCALAR_7,
            blnd_xlm: 100 * SCALAR_7,
            usdc: 260 * SCALAR_7,
        }
    );

    let recipient = Address::generate(&fixture.env);
    assert_eq!(
        pool_client.settle_bad_debt_lot(
            &fixture.candidate.address,
            &auction_id,
            &(120 * SCALAR_7),
            &(60 * SCALAR_7),
            &recipient,
        ),
        Some(120 * SCALAR_7)
    );
    assert_eq!(
        TokenClient::new(&fixture.env, &fixture.prefund_token).balance(&recipient),
        60 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_state(&v3_backstop::BackstopTier::Usdc, &pool),
        v3_backstop::PoolTierState {
            assets: 440 * SCALAR_7,
            queued_shares: 0,
            shares: 500 * SCALAR_7,
        }
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_committed_assets(&v3_backstop::BackstopTier::Usdc, &pool),
        120 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_valuation(&pool).active_values,
        v3_backstop::ActivationValues {
            blnd_usdc: 50 * SCALAR_7,
            blnd_xlm: 100 * SCALAR_7,
            usdc: 320 * SCALAR_7,
        }
    );

    assert_eq!(
        pool_client.settle_bad_debt_lot(
            &fixture.candidate.address,
            &auction_id,
            &(120 * SCALAR_7),
            &(120 * SCALAR_7),
            &recipient,
        ),
        None
    );
    assert_eq!(fixture.candidate.pool_bad_debt_commitment_count(&pool), 0);
    assert_eq!(
        TokenClient::new(&fixture.env, &fixture.prefund_token).balance(&recipient),
        180 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_state(&v3_backstop::BackstopTier::Usdc, &pool),
        v3_backstop::PoolTierState {
            assets: 320 * SCALAR_7,
            queued_shares: 0,
            shares: 500 * SCALAR_7,
        }
    );
    assert_eq!(
        fixture.candidate.pool_valuation(&pool).active_values,
        v3_backstop::ActivationValues {
            blnd_usdc: 50 * SCALAR_7,
            blnd_xlm: 100 * SCALAR_7,
            usdc: 320 * SCALAR_7,
        }
    );

    let release_auction_id = BytesN::from_array(&fixture.env, &[4; 32]);
    assert_eq!(
        pool_client.commit_bad_debt_lot(
            &fixture.candidate.address,
            &release_auction_id,
            &(100 * SCALAR_7),
        ),
        120 * SCALAR_7
    );
    pool_client.release_bad_debt_lot(&fixture.candidate.address, &release_auction_id);
    assert_eq!(fixture.candidate.pool_bad_debt_commitment_count(&pool), 0);
    assert_eq!(
        fixture
            .candidate
            .pool_tier_assets(&v3_backstop::BackstopTier::Usdc, &pool),
        320 * SCALAR_7
    );
}

#[test]
fn v3_wasm_exposes_v2_compatible_pool_status_quotes() {
    let fixture = Fixture::create();
    let values = |amount| v3_backstop::ActivationValues {
        blnd_usdc: amount,
        blnd_xlm: 0,
        usdc: 0,
    };
    let total = 20_000 * SCALAR_7;

    let on_ice = fixture.candidate.quote_update_pool_status(
        &1,
        &values(14_000 * SCALAR_7),
        &values(6_000 * SCALAR_7),
    );
    assert!(on_ice.transition_allowed);
    assert_eq!(on_ice.q4w_percentage, 3_000_000);
    assert_eq!(on_ice.status, 3);
    assert_eq!(on_ice.eligible_value, 14_000 * SCALAR_7);

    let admin_active = fixture.candidate.quote_set_pool_status(
        &3,
        &0,
        &values(13_000 * SCALAR_7),
        &values(total - 13_000 * SCALAR_7),
    );
    assert!(admin_active.transition_allowed);
    assert_eq!(admin_active.q4w_percentage, 3_500_000);
    assert_eq!(admin_active.status, 0);
}

#[test]
fn v3_factory_deploys_registered_loss_aware_pool_wasm() {
    let env = Env::default();
    let backstop = Address::generate(&env);
    let pool_hash = env.deployer().upload_contract_wasm(v3_pool::WASM);
    let factory_id = env.register(v3_pool_factory::WASM, (&backstop, &pool_hash));
    let factory = v3_pool_factory::Client::new(&env, &factory_id);
    let admin = Address::generate(&env);
    let lending_config = test_lending_config(&backstop);
    let reserves = soroban_sdk::Vec::<v3_pool_factory::ReserveConfig>::new(&env);
    let salt = soroban_sdk::BytesN::from_array(&env, &[11; 32]);

    assert!(factory
        .try_deploy(&admin, &lending_config, &reserves, &salt)
        .is_err());
    let pool_id = factory
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &factory_id,
                fn_name: "deploy",
                args: (&admin, &lending_config, &reserves, salt.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .deploy(&admin, &lending_config, &reserves, &salt);
    let pool = v3_pool::Client::new(&env, &pool_id);

    assert!(factory.is_pool(&pool_id));
    assert_eq!(factory.backstop(), backstop);
    assert_eq!(factory.pool_wasm_hash(), pool_hash);
    assert_eq!(pool.backstop(), backstop);
    assert_eq!(pool.admin(), admin);
    assert_eq!(pool.status(), 6);
    assert_eq!(
        pool.backstop_loss_state(),
        v3_pool::BackstopLossState {
            committed_loss_entries: 0,
            liability_entries: 0,
            unresolved_bad_debt_entries: 0,
        }
    );
    assert!(pool.backstop_withdrawal_allowed(&backstop));
    assert!(!pool.backstop_withdrawal_allowed(&Address::generate(&env)));
}

#[test]
fn deployed_v3_pool_provides_the_backstop_withdrawal_safety_boundary() {
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
    LegacyEmitterClient::new(&env, &emitter_id).initialize(&blnd_id, &incumbent, &legacy_token_id);

    let deployer = Address::generate(&env);
    let candidate_salt = soroban_sdk::BytesN::from_array(&env, &[21; 32]);
    let candidate_deployer = env
        .deployer()
        .with_address(deployer.clone(), candidate_salt);
    let candidate_id = candidate_deployer.deployed_address();
    let pool_hash = env.deployer().upload_contract_wasm(v3_pool::WASM);
    let factory_id = env.register(v3_pool_factory::WASM, (&candidate_id, &pool_hash));
    let valuation_adapter_id = register_test_valuation_adapter(
        &env,
        &blnd_id,
        &prefund_token_id,
        &legacy_token_id,
        &next_token_id,
    );
    let candidate_hash = env.deployer().upload_contract_wasm(v3_backstop::WASM);
    assert_eq!(
        candidate_deployer.deploy_v2(
            candidate_hash,
            (
                &emitter_id,
                &blnd_id,
                &factory_id,
                &legacy_token_id,
                &next_token_id,
                &prefund_token_id,
                &valuation_adapter_id,
            ),
        ),
        candidate_id
    );

    let factory = v3_pool_factory::Client::new(&env, &factory_id);
    let lending_config = test_lending_config(&valuation_adapter_id);
    let reserves = soroban_sdk::vec![
        &env,
        lending_reserve_config(&legacy_token_id, 0),
        lending_reserve_config(&next_token_id, 1)
    ];
    let pool_id = factory.deploy(
        &deployer,
        &lending_config,
        &reserves,
        &soroban_sdk::BytesN::from_array(&env, &[22; 32]),
    );
    let pool = v3_pool::Client::new(&env, &pool_id);
    let candidate = v3_backstop::Client::new(&env, &candidate_id);
    let user = Address::generate(&env);
    let legacy_token = StellarAssetClient::new(&env, &legacy_token_id);
    legacy_token.mint(&user, &SCALAR_7);

    candidate.deposit_blnd_usdc(&user, &pool_id, &SCALAR_7);
    let queued = candidate.queue_blnd_usdc_withdrawal(&user, &pool_id, &SCALAR_7);
    env.ledger().set_timestamp(queued.expires_at);

    assert!(pool.backstop_withdrawal_allowed(&candidate_id));
    candidate.withdraw_blnd_usdc(&user, &pool_id, &SCALAR_7, &user);
    assert_eq!(legacy_token.balance(&user), SCALAR_7);
    assert_eq!(candidate.blnd_usdc_shares(&user, &pool_id), 0);

    legacy_token.mint(&user, &(12_500 * SCALAR_7));
    candidate.deposit_blnd_usdc(&user, &pool_id, &(12_500 * SCALAR_7));
    assert_eq!(pool.set_pool_status(&0), 0);
    assert_eq!(pool.status(), 0);
    assert!(pool.action_allowed(&4));

    let lender = Address::generate(&env);
    let next_token = StellarAssetClient::new(&env, &next_token_id);
    next_token.mint(&lender, &(1_000 * SCALAR_7));
    legacy_token.mint(&user, &(100 * SCALAR_7));
    pool.submit(
        &lender,
        &lender,
        &lender,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: 1_000 * SCALAR_7,
                request_type: 0,
            }
        ],
    );
    let borrower_positions = pool.submit(
        &user,
        &user,
        &user,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: legacy_token_id.clone(),
                amount: 100 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: 20 * SCALAR_7,
                request_type: 4,
            }
        ],
    );
    assert_eq!(borrower_positions.collateral.get(0), Some(100 * SCALAR_7));
    assert_eq!(borrower_positions.liabilities.get(1), Some(20 * SCALAR_7));
    assert!(pool.position_health(&user).healthy);
    let repaid_positions = pool.submit(
        &user,
        &user,
        &user,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: 5 * SCALAR_7,
                request_type: 5,
            }
        ],
    );
    assert_eq!(repaid_positions.liabilities.get(1), Some(15 * SCALAR_7));
    let reduced_collateral = pool.submit(
        &user,
        &user,
        &user,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: legacy_token_id.clone(),
                amount: SCALAR_7,
                request_type: 3,
            }
        ],
    );
    assert_eq!(reduced_collateral.collateral.get(0), Some(99 * SCALAR_7));
    let reduced_supply = pool.submit(
        &lender,
        &lender,
        &lender,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: 10 * SCALAR_7,
                request_type: 1,
            }
        ],
    );
    assert_eq!(reduced_supply.supply.get(1), Some(990 * SCALAR_7));

    let next_token_client = TokenClient::new(&env, &next_token_id);
    next_token_client.approve(&user, &pool_id, &SCALAR_7, &1_000);
    let allowance_repaid = pool.submit_with_allowance(
        &user,
        &user,
        &user,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: SCALAR_7,
                request_type: 5,
            }
        ],
    );
    assert_eq!(allowance_repaid.liabilities.get(1), Some(14 * SCALAR_7));

    let flash_borrower = Address::generate(&env);
    let flash_receiver_id = env.register(MockFlashLoanReceiver, ());
    next_token.mint(&flash_borrower, &SCALAR_7);
    next_token_client.approve(&flash_borrower, &pool_id, &SCALAR_7, &1_000);
    let flash_result = pool.flash_loan(
        &flash_borrower,
        &v3_pool::FlashLoan {
            amount: SCALAR_7,
            asset: next_token_id.clone(),
            contract: flash_receiver_id.clone(),
        },
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: SCALAR_7,
                request_type: 5,
            }
        ],
    );
    assert!(flash_result.liabilities.is_empty());
    assert_eq!(next_token_client.balance(&flash_receiver_id), SCALAR_7);

    candidate.queue_blnd_usdc_withdrawal(&user, &pool_id, &(6_250 * SCALAR_7));
    assert_eq!(pool.update_pool_status(), 3);
    assert_eq!(pool.status(), 3);
    assert!(!pool.action_allowed(&4));
    assert!(!pool.action_allowed(&9));
    assert!(pool.action_allowed(&0));
    assert!(pool
        .try_submit(
            &user,
            &user,
            &user,
            &soroban_sdk::vec![
                &env,
                v3_pool::Request {
                    address: next_token_id.clone(),
                    amount: SCALAR_7,
                    request_type: 4,
                }
            ],
        )
        .is_err());

    candidate.dequeue_blnd_usdc_withdrawal(&user, &pool_id, &(6_250 * SCALAR_7));
    assert_eq!(pool.update_pool_status(), 1);
    assert_eq!(pool.status(), 1);
    assert!(pool.action_allowed(&4));

    let risky_borrower = Address::generate(&env);
    legacy_token.mint(&risky_borrower, &(100 * SCALAR_7));
    pool.submit(
        &risky_borrower,
        &risky_borrower,
        &risky_borrower,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: legacy_token_id.clone(),
                amount: 100 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: 70 * SCALAR_7,
                request_type: 4,
            }
        ],
    );
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 2 * 31_536_000);
    assert!(!pool.position_health(&risky_borrower).healthy);
    let liquidation = pool.new_liquidation_auction(
        &risky_borrower,
        &soroban_sdk::vec![&env, next_token_id.clone()],
        &soroban_sdk::vec![&env, legacy_token_id.clone()],
        &30,
    );
    env.ledger().set_sequence_number(liquidation.block + 200);
    let risky_before = pool.positions(&risky_borrower);
    let filler_after = pool.submit(
        &user,
        &user,
        &user,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: risky_borrower.clone(),
                amount: 100,
                request_type: 6,
            }
        ],
    );
    assert!(pool.try_get_liquidation_auction(&risky_borrower).is_err());
    assert!(
        pool.positions(&risky_borrower).liabilities.get(1).unwrap()
            < risky_before.liabilities.get(1).unwrap()
    );
    assert!(filler_after.collateral.get(0).unwrap() > 99 * SCALAR_7);
    assert!(pool.position_health(&user).healthy);

    let bad_debt_filler = Address::generate(&env);
    legacy_token.mint(&bad_debt_filler, &(200 * SCALAR_7));
    pool.submit(
        &bad_debt_filler,
        &bad_debt_filler,
        &bad_debt_filler,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: legacy_token_id.clone(),
                amount: 200 * SCALAR_7,
                request_type: 2,
            }
        ],
    );

    legacy_token.mint(&candidate_id, &(100 * SCALAR_7));
    let backstop_positions = pool.submit(
        &candidate_id,
        &candidate_id,
        &candidate_id,
        &soroban_sdk::vec![
            &env,
            v3_pool::Request {
                address: legacy_token_id.clone(),
                amount: 100 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: next_token_id.clone(),
                amount: 50 * SCALAR_7,
                request_type: 4,
            }
        ],
    );
    let backstop_debt = backstop_positions.liabilities.get(1).unwrap();
    let bad_debt_auction_id = BytesN::from_array(&env, &[23; 32]);
    let bad_debt_auction = pool.new_bad_debt_auction(
        &bad_debt_auction_id,
        &soroban_sdk::vec![&env, next_token_id.clone()],
    );
    assert_eq!(
        bad_debt_auction.bid.get(next_token_id.clone()),
        Some(backstop_debt)
    );
    assert_eq!(
        bad_debt_auction.lot_quote.tier,
        v3_pool::BackstopTier::BlndUsdc
    );
    assert_eq!(candidate.pool_bad_debt_commitment_count(&pool_id), 1);

    env.ledger()
        .set_sequence_number(bad_debt_auction.block + 100);
    let first_fill = pool.fill_bad_debt_auction(&bad_debt_filler, &50);
    let expected_first_bid = (backstop_debt + 1) / 2;
    let expected_base_lot = bad_debt_auction.lot_quote.lot_amount / 2;
    let expected_first_lot = expected_base_lot / 2;
    assert_eq!(
        first_fill.bid.get(next_token_id.clone()),
        Some(expected_first_bid)
    );
    assert_eq!(first_fill.base_lot_amount, expected_base_lot);
    assert_eq!(first_fill.lot_amount, expected_first_lot);
    assert!(!first_fill.complete);
    assert_eq!(
        candidate
            .bad_debt_commitment(&pool_id, &bad_debt_auction_id)
            .unwrap()
            .lot_amount,
        bad_debt_auction.lot_quote.lot_amount - expected_base_lot
    );
    assert_eq!(
        TokenClient::new(&env, &legacy_token_id).balance(&bad_debt_filler),
        expected_first_lot
    );

    env.ledger()
        .set_sequence_number(bad_debt_auction.block + 300);
    let second_fill = pool.fill_bad_debt_auction(&bad_debt_filler, &100);
    assert!(second_fill.complete);
    let debt_before_second = backstop_debt - expected_first_bid;
    let expected_second_bid = (debt_before_second + 1) / 2;
    assert_eq!(
        second_fill.bid.get(next_token_id.clone()),
        Some(expected_second_bid)
    );
    assert_eq!(
        second_fill.lot_amount,
        bad_debt_auction.lot_quote.lot_amount - expected_base_lot
    );
    assert!(pool.try_get_bad_debt_auction().is_err());
    assert_eq!(candidate.pool_bad_debt_commitment_count(&pool_id), 0);
    let residual_debt = debt_before_second - expected_second_bid;
    assert_eq!(
        pool.positions(&candidate_id).liabilities.get(1),
        Some(residual_debt)
    );
    assert_eq!(
        pool.backstop_loss_state(),
        v3_pool::BackstopLossState {
            committed_loss_entries: 0,
            liability_entries: 1,
            unresolved_bad_debt_entries: 0,
        }
    );
    assert!(!pool.backstop_withdrawal_allowed(&candidate_id));

    let continuation_id = BytesN::from_array(&env, &[24; 32]);
    let continuation = pool.continue_bad_debt_resolution(&continuation_id);
    assert!(continuation.auction_created);
    assert!(continuation.defaulted.is_empty());
    let continued_auction = pool.get_bad_debt_auction();
    assert_eq!(continued_auction.auction_id, continuation_id);
    assert_eq!(
        continued_auction.lot_quote.tier,
        v3_pool::BackstopTier::BlndUsdc
    );
    assert_eq!(
        continued_auction.bid.get(next_token_id),
        Some(residual_debt)
    );
    env.ledger()
        .set_sequence_number(continued_auction.block + 200);
    assert!(pool.fill_bad_debt_auction(&bad_debt_filler, &100).complete);
    assert!(pool.positions(&candidate_id).liabilities.is_empty());
    assert_eq!(
        pool.backstop_loss_state(),
        v3_pool::BackstopLossState {
            committed_loss_entries: 0,
            liability_entries: 0,
            unresolved_bad_debt_entries: 0,
        }
    );
    assert!(pool.backstop_withdrawal_allowed(&candidate_id));
}

#[test]
fn optimized_pool_hands_liquidation_residual_to_active_backstop_auction_state() {
    let fixture = Fixture::create();
    let env = &fixture.env;
    let operator = Address::generate(env);
    let pool_id = env.register(
        v3_pool::WASM,
        (
            &fixture.candidate.address,
            &operator,
            &direct_lending_config(&fixture.valuation_adapter),
            &soroban_sdk::vec![
                env,
                direct_lending_reserve_config(&fixture.legacy_token.address, 0),
                direct_lending_reserve_config(&fixture.next_token, 1),
            ],
        ),
    );
    fixture.pool_factory.set_pool(&pool_id);
    let pool = v3_pool::Client::new(env, &pool_id);
    let oracle = v3_test_valuation_adapter::Client::new(env, &fixture.valuation_adapter);

    let activation_provider = Address::generate(env);
    fixture
        .legacy_token
        .mint(&activation_provider, &(12_500 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&activation_provider, &pool_id, &(12_500 * SCALAR_7));
    assert_eq!(pool.set_pool_status(&0), 0);

    let supplier = Address::generate(env);
    let borrower = Address::generate(env);
    let filler = Address::generate(env);
    let next_admin = StellarAssetClient::new(env, &fixture.next_token);
    next_admin.mint(&supplier, &(1_000 * SCALAR_7));
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(200 * SCALAR_7));
    fixture.legacy_token.mint(&borrower, &(100 * SCALAR_7));
    fixture.legacy_token.mint(&filler, &(200 * SCALAR_7));
    pool.submit(
        &supplier,
        &supplier,
        &supplier,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: fixture.next_token.clone(),
                amount: 1_000 * SCALAR_7,
                request_type: 0,
            },
        ],
    );
    pool.submit(
        &fixture.candidate.address,
        &fixture.candidate.address,
        &fixture.candidate.address,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: fixture.legacy_token.address.clone(),
                amount: 200 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: fixture.next_token.clone(),
                amount: 10 * SCALAR_7,
                request_type: 4,
            },
        ],
    );
    pool.submit(
        &borrower,
        &borrower,
        &borrower,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: fixture.legacy_token.address.clone(),
                amount: 100 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: fixture.next_token.clone(),
                amount: 70 * SCALAR_7,
                request_type: 4,
            },
        ],
    );
    pool.submit(
        &filler,
        &filler,
        &filler,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: fixture.legacy_token.address.clone(),
                amount: 200 * SCALAR_7,
                request_type: 2,
            },
        ],
    );

    let bad_debt_auction = pool.new_bad_debt_auction(
        &BytesN::from_array(env, &[26; 32]),
        &soroban_sdk::vec![env, fixture.next_token.clone()],
    );
    assert_eq!(
        bad_debt_auction.bid.get(fixture.next_token.clone()),
        Some(10 * SCALAR_7)
    );

    oracle.set_price(
        &fixture.legacy_token.address,
        &v3_test_valuation_adapter::PriceQuote {
            decimals: 7,
            price: 5_000_000,
            valid_until: u64::MAX,
        },
    );
    let liquidation = pool.new_liquidation_auction(
        &borrower,
        &soroban_sdk::vec![env, fixture.next_token.clone()],
        &soroban_sdk::vec![env, fixture.legacy_token.address.clone()],
        &100,
    );
    let d_supply_before = pool.reserve(&fixture.next_token).data.d_supply;
    env.ledger()
        .set_sequence_number(liquidation.block.checked_add(300).unwrap());
    pool.submit(
        &filler,
        &filler,
        &filler,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: borrower.clone(),
                amount: 100,
                request_type: 6,
            },
        ],
    );

    assert!(pool.try_get_liquidation_auction(&borrower).is_err());
    assert!(pool.positions(&borrower).collateral.is_empty());
    assert!(pool.positions(&borrower).liabilities.is_empty());
    assert_eq!(
        pool.positions(&fixture.candidate.address)
            .liabilities
            .get(1),
        Some(45 * SCALAR_7)
    );
    assert_eq!(
        pool.reserve(&fixture.next_token).data.d_supply,
        d_supply_before
    );
    assert_eq!(pool.get_bad_debt_auction(), bad_debt_auction);
    assert_eq!(
        pool.backstop_loss_state(),
        v3_pool::BackstopLossState {
            committed_loss_entries: 1,
            liability_entries: 1,
            unresolved_bad_debt_entries: 0,
        }
    );
    assert!(!pool.backstop_withdrawal_allowed(&fixture.candidate.address));
}

#[test]
fn optimized_max_reserve_liquidation_handoff_fits_mainnet_invocation_limits() {
    let fixture = Fixture::create();
    let env = &fixture.env;
    let oracle = fixture.valuation_adapter.clone();
    let oracle_client = v3_test_valuation_adapter::Client::new(env, &oracle);
    let token_admin = Address::generate(env);
    let mut reserve_assets = soroban_sdk::Vec::<Address>::new(env);
    for _ in 0..30 {
        let asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        oracle_client.set_price(
            &asset,
            &v3_test_valuation_adapter::PriceQuote {
                decimals: 7,
                price: SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        reserve_assets.push_back(asset);
    }
    let mut reserves = soroban_sdk::Vec::<v3_pool::ReserveConfig>::new(env);
    for (index, asset) in reserve_assets.iter().enumerate() {
        reserves.push_back(direct_lending_reserve_config(&asset, index as u32));
    }
    let mut lending_config = direct_lending_config(&oracle);
    lending_config.max_positions = 60;
    let pool_id = env.register(
        v3_pool::WASM,
        (
            &fixture.candidate.address,
            &Address::generate(env),
            &lending_config,
            &reserves,
        ),
    );
    fixture.pool_factory.set_pool(&pool_id);
    let pool = v3_pool::Client::new(env, &pool_id);

    let activation_provider = Address::generate(env);
    fixture
        .legacy_token
        .mint(&activation_provider, &(12_500 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&activation_provider, &pool_id, &(12_500 * SCALAR_7));
    assert_eq!(pool.set_pool_status(&0), 0);

    // Setup intentionally spans multiple invocations. Only the maximum-size
    // optimized-WASM liquidation fill below is measured against mainnet
    // invocation limits.
    env.cost_estimate().budget().reset_unlimited();
    let supplier = Address::generate(env);
    for asset in reserve_assets.iter() {
        StellarAssetClient::new(env, &asset).mint(&supplier, &(100 * SCALAR_7));
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &soroban_sdk::vec![
                env,
                v3_pool::Request {
                    address: asset,
                    amount: 100 * SCALAR_7,
                    request_type: 0,
                },
            ],
        );
    }

    let collateral_asset = reserve_assets.get(0).unwrap();
    let borrower = Address::generate(env);
    let filler = Address::generate(env);
    StellarAssetClient::new(env, &collateral_asset).mint(&borrower, &(1_000 * SCALAR_7));
    StellarAssetClient::new(env, &collateral_asset).mint(&filler, &(5_000 * SCALAR_7));
    pool.submit(
        &borrower,
        &borrower,
        &borrower,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: collateral_asset.clone(),
                amount: 1_000 * SCALAR_7,
                request_type: 2,
            },
        ],
    );
    for asset in reserve_assets.iter() {
        pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &soroban_sdk::vec![
                env,
                v3_pool::Request {
                    address: asset,
                    amount: 10 * SCALAR_7,
                    request_type: 4,
                },
            ],
        );
    }
    pool.submit(
        &filler,
        &filler,
        &filler,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: collateral_asset.clone(),
                amount: 5_000 * SCALAR_7,
                request_type: 2,
            },
        ],
    );

    oracle_client.set_price(
        &collateral_asset,
        &v3_test_valuation_adapter::PriceQuote {
            decimals: 7,
            price: 3_000_000,
            valid_until: u64::MAX,
        },
    );
    assert!(!pool.position_health(&borrower).healthy);
    let liquidation = pool.new_liquidation_auction(
        &borrower,
        &reserve_assets,
        &soroban_sdk::vec![env, collateral_asset],
        &100,
    );
    let mut d_supplies_before = soroban_sdk::Vec::<i128>::new(env);
    for asset in reserve_assets.iter() {
        d_supplies_before.push_back(pool.reserve(&asset).data.d_supply);
    }
    env.ledger()
        .set_sequence_number(liquidation.block.checked_add(300).unwrap());
    pool.submit(
        &filler,
        &filler,
        &filler,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: borrower.clone(),
                amount: 100,
                request_type: 6,
            },
        ],
    );
    let resources = env.cost_estimate().resources();

    assert!(pool.positions(&borrower).liabilities.is_empty());
    let backstop_positions = pool.positions(&fixture.candidate.address);
    assert_eq!(backstop_positions.liabilities.len(), 30);
    for (index, asset) in reserve_assets.iter().enumerate() {
        assert_eq!(
            backstop_positions.liabilities.get(index as u32),
            Some(5 * SCALAR_7)
        );
        assert_eq!(
            pool.reserve(&asset).data.d_supply,
            d_supplies_before.get(index as u32).unwrap()
        );
    }
    assert_eq!(
        pool.backstop_loss_state(),
        v3_pool::BackstopLossState {
            committed_loss_entries: 0,
            liability_entries: 30,
            unresolved_bad_debt_entries: 0,
        }
    );
    assert!(resources.instructions <= 600_000_000);
    assert!(resources.mem_bytes <= 41_943_040);
    assert!(resources.disk_read_entries <= 100);
    assert!(resources.write_entries <= 50);
    assert!(
        resources.disk_read_entries + resources.memory_read_entries + resources.write_entries
            <= 100
    );
    assert!(resources.disk_read_bytes <= 200_000);
    assert!(resources.write_bytes <= 132_096);
    assert!(resources.contract_events_size_bytes <= 16_384);
}

#[test]
fn optimized_pool_defaults_residual_debt_only_after_backstop_exhaustion() {
    let fixture = Fixture::create();
    let env = &fixture.env;
    let operator = Address::generate(env);
    let pool_id = env.register(
        v3_pool::WASM,
        (
            &fixture.candidate.address,
            &operator,
            &direct_lending_config(&fixture.valuation_adapter),
            &soroban_sdk::vec![
                env,
                direct_lending_reserve_config(&fixture.legacy_token.address, 0),
                direct_lending_reserve_config(&fixture.next_token, 1),
            ],
        ),
    );
    fixture.pool_factory.set_pool(&pool_id);
    let pool = v3_pool::Client::new(env, &pool_id);
    let activation_provider = Address::generate(env);
    fixture
        .legacy_token
        .mint(&activation_provider, &(12_500 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&activation_provider, &pool_id, &(12_500 * SCALAR_7));
    assert_eq!(pool.set_pool_status(&0), 0);
    let queued = fixture.candidate.queue_blnd_usdc_withdrawal(
        &activation_provider,
        &pool_id,
        &(12_500 * SCALAR_7),
    );
    env.ledger().set_timestamp(queued.expires_at);
    fixture.candidate.withdraw_blnd_usdc(
        &activation_provider,
        &pool_id,
        &(12_500 * SCALAR_7),
        &activation_provider,
    );
    let supplier = Address::generate(env);
    let next_admin = StellarAssetClient::new(env, &fixture.next_token);
    next_admin.mint(&supplier, &(1_000 * SCALAR_7));
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(100 * SCALAR_7));
    pool.submit(
        &supplier,
        &supplier,
        &supplier,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: fixture.next_token.clone(),
                amount: 1_000 * SCALAR_7,
                request_type: 0,
            }
        ],
    );
    pool.submit(
        &fixture.candidate.address,
        &fixture.candidate.address,
        &fixture.candidate.address,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: fixture.legacy_token.address.clone(),
                amount: 100 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: fixture.next_token.clone(),
                amount: 50 * SCALAR_7,
                request_type: 4,
            },
        ],
    );
    assert_eq!(
        fixture
            .candidate
            .quote_bad_debt_lot(&pool_id, &(50 * SCALAR_7)),
        None
    );

    let continuation = pool.continue_bad_debt_resolution(&BytesN::from_array(env, &[25; 32]));
    assert!(!continuation.auction_created);
    assert_eq!(
        continuation.defaulted.get(fixture.next_token.clone()),
        Some(50 * SCALAR_7)
    );
    assert!(pool
        .positions(&fixture.candidate.address)
        .liabilities
        .is_empty());
    let reserve = pool.reserve(&fixture.next_token).data;
    assert_eq!(reserve.d_supply, 0);
    assert_eq!(reserve.b_supply, 1_000 * SCALAR_7);
    assert_eq!(reserve.b_rate, 950_000_000_000);
    assert!(pool.backstop_withdrawal_allowed(&fixture.candidate.address));
}

#[test]
fn optimized_max_reserve_supplier_default_fits_mainnet_invocation_limits() {
    let fixture = Fixture::create();
    let env = &fixture.env;
    let oracle = fixture.valuation_adapter.clone();
    let oracle_client = v3_test_valuation_adapter::Client::new(env, &oracle);
    let token_admin = Address::generate(env);
    let mut reserve_assets = soroban_sdk::Vec::<Address>::new(env);
    for _ in 0..30 {
        let asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        oracle_client.set_price(
            &asset,
            &v3_test_valuation_adapter::PriceQuote {
                decimals: 7,
                price: SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        reserve_assets.push_back(asset);
    }
    let mut reserves = soroban_sdk::Vec::<v3_pool::ReserveConfig>::new(env);
    for (index, asset) in reserve_assets.iter().enumerate() {
        reserves.push_back(direct_lending_reserve_config(&asset, index as u32));
    }
    let mut lending_config = direct_lending_config(&oracle);
    lending_config.max_positions = 60;
    let operator = Address::generate(env);
    let pool_id = env.register(
        v3_pool::WASM,
        (
            &fixture.candidate.address,
            &operator,
            &lending_config,
            &reserves,
        ),
    );
    fixture.pool_factory.set_pool(&pool_id);
    let pool = v3_pool::Client::new(env, &pool_id);

    let activation_provider = Address::generate(env);
    fixture
        .legacy_token
        .mint(&activation_provider, &(12_500 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&activation_provider, &pool_id, &(12_500 * SCALAR_7));
    assert_eq!(pool.set_pool_status(&0), 0);
    let queued = fixture.candidate.queue_blnd_usdc_withdrawal(
        &activation_provider,
        &pool_id,
        &(12_500 * SCALAR_7),
    );
    env.ledger().set_timestamp(queued.expires_at);
    fixture.candidate.withdraw_blnd_usdc(
        &activation_provider,
        &pool_id,
        &(12_500 * SCALAR_7),
        &activation_provider,
    );
    let dusty_tier_assets = 100 * SCALAR_7;
    for token in [
        fixture.legacy_token.address.clone(),
        fixture.next_token.clone(),
    ] {
        oracle_client.set_valuation(
            &token,
            &v3_test_valuation_adapter::AssetValuation {
                underlying_blnd: dusty_tier_assets,
                usdc_value: dusty_tier_assets,
                valid_until: u64::MAX,
            },
        );
    }
    let dust_provider = Address::generate(env);
    fixture
        .legacy_token
        .mint(&dust_provider, &dusty_tier_assets);
    StellarAssetClient::new(env, &fixture.next_token).mint(&dust_provider, &dusty_tier_assets);
    StellarAssetClient::new(env, &fixture.prefund_token).mint(&dust_provider, &dusty_tier_assets);
    fixture
        .candidate
        .deposit_blnd_usdc(&dust_provider, &pool_id, &dusty_tier_assets);
    fixture
        .candidate
        .deposit_blnd_xlm(&dust_provider, &pool_id, &dusty_tier_assets);
    fixture
        .candidate
        .deposit_usdc(&dust_provider, &pool_id, &dusty_tier_assets);
    // The setup intentionally builds a maximum-size position over many
    // invocations. Invocation-level mainnet limits remain enforced and are
    // asserted explicitly for the continuation below.
    env.cost_estimate().budget().reset_unlimited();

    let supplier = Address::generate(env);
    for asset in reserve_assets.iter() {
        StellarAssetClient::new(env, &asset).mint(&supplier, &(100 * SCALAR_7));
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &soroban_sdk::vec![
                env,
                v3_pool::Request {
                    address: asset,
                    amount: 100 * SCALAR_7,
                    request_type: 0,
                }
            ],
        );
    }

    let collateral_asset = reserve_assets.get(0).unwrap();
    StellarAssetClient::new(env, &collateral_asset)
        .mint(&fixture.candidate.address, &(100 * SCALAR_7));
    pool.submit(
        &fixture.candidate.address,
        &fixture.candidate.address,
        &fixture.candidate.address,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: collateral_asset,
                amount: 100 * SCALAR_7,
                request_type: 2,
            }
        ],
    );
    for asset in reserve_assets.iter() {
        pool.submit(
            &fixture.candidate.address,
            &fixture.candidate.address,
            &fixture.candidate.address,
            &soroban_sdk::vec![
                env,
                v3_pool::Request {
                    address: asset,
                    amount: SCALAR_7,
                    request_type: 4,
                }
            ],
        );
    }
    assert_eq!(
        pool.backstop_loss_state(),
        v3_pool::BackstopLossState {
            committed_loss_entries: 0,
            liability_entries: 30,
            unresolved_bad_debt_entries: 0,
        }
    );

    let continuation = pool.continue_bad_debt_resolution(&BytesN::from_array(env, &[26; 32]));
    let resources = env.cost_estimate().resources();
    assert!(!continuation.auction_created);
    assert_eq!(continuation.defaulted.len(), 30);
    assert!(resources.instructions <= 600_000_000);
    assert!(resources.mem_bytes <= 41_943_040);
    assert!(resources.disk_read_entries <= 100);
    assert!(resources.write_entries <= 50);
    // Preserve at least ten entries of headroom beneath the mainnet total
    // footprint limit for production-oracle implementation differences.
    assert!(
        resources.disk_read_entries + resources.memory_read_entries + resources.write_entries <= 90
    );
    assert!(resources.disk_read_bytes <= 200_000);
    assert!(resources.write_bytes <= 132_096);
    assert!(resources.contract_events_size_bytes <= 16_384);
    assert!(pool
        .positions(&fixture.candidate.address)
        .liabilities
        .is_empty());
    assert!(pool.backstop_withdrawal_allowed(&fixture.candidate.address));
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum InterestTierCase {
    BlndUsdc,
    BlndXlm,
    Usdc,
}

fn assert_optimized_interest_auction(case: InterestTierCase) {
    let fixture = Fixture::create();
    let env = &fixture.env;
    let oracle = v3_test_valuation_adapter::Client::new(env, &fixture.valuation_adapter);
    let operator = Address::generate(env);
    let extra_admin = Address::generate(env);
    let loan_asset = env
        .register_stellar_asset_contract_v2(extra_admin.clone())
        .address();
    let collateral_asset = env
        .register_stellar_asset_contract_v2(extra_admin)
        .address();
    let lending_config = v3_pool::LendingConfig {
        backstop_take_rate: 9_999_999,
        max_positions: 6,
        min_collateral: SCALAR_7,
        oracle: fixture.valuation_adapter.clone(),
        price_decimals: 7,
    };
    let reserves = soroban_sdk::vec![
        env,
        direct_lending_reserve_config(&loan_asset, 0),
        direct_lending_reserve_config(&collateral_asset, 1),
        direct_lending_reserve_config(&fixture.next_token, 2),
        direct_lending_reserve_config(&fixture.legacy_token.address, 3)
    ];
    let pool_id = env.register(
        v3_pool::WASM,
        (
            &fixture.candidate.address,
            &operator,
            &lending_config,
            &reserves,
        ),
    );
    fixture.pool_factory.set_pool(&pool_id);
    let pool = v3_pool::Client::new(env, &pool_id);

    for asset in [
        loan_asset.clone(),
        collateral_asset.clone(),
        fixture.next_token.clone(),
        fixture.legacy_token.address.clone(),
    ] {
        oracle.set_price(
            &asset,
            &v3_test_valuation_adapter::PriceQuote {
                decimals: 7,
                price: SCALAR_7,
                valid_until: u64::MAX,
            },
        );
    }

    let backstop_depositor = Address::generate(env);
    let backstop_amount = 12_500 * SCALAR_7;
    let (pool_tier, backstop_tier, bid_token) = match case {
        InterestTierCase::BlndUsdc => {
            fixture
                .legacy_token
                .mint(&backstop_depositor, &backstop_amount);
            fixture
                .candidate
                .deposit_blnd_usdc(&backstop_depositor, &pool_id, &backstop_amount);
            (
                v3_pool::BackstopTier::BlndUsdc,
                v3_backstop::BackstopTier::BlndUsdc,
                fixture.legacy_token.address.clone(),
            )
        }
        InterestTierCase::BlndXlm => {
            StellarAssetClient::new(env, &fixture.next_token)
                .mint(&backstop_depositor, &backstop_amount);
            fixture
                .candidate
                .deposit_blnd_xlm(&backstop_depositor, &pool_id, &backstop_amount);
            (
                v3_pool::BackstopTier::BlndXlm,
                v3_backstop::BackstopTier::BlndXlm,
                fixture.next_token.clone(),
            )
        }
        InterestTierCase::Usdc => {
            StellarAssetClient::new(env, &fixture.prefund_token)
                .mint(&backstop_depositor, &backstop_amount);
            fixture
                .candidate
                .deposit_usdc(&backstop_depositor, &pool_id, &backstop_amount);
            (
                v3_pool::BackstopTier::Usdc,
                v3_backstop::BackstopTier::Usdc,
                fixture.prefund_token.clone(),
            )
        }
    };
    assert_eq!(pool.set_pool_status(&0), 0);

    let lender = Address::generate(env);
    let borrower = Address::generate(env);
    let loan_token = StellarAssetClient::new(env, &loan_asset);
    loan_token.mint(&lender, &(30_000 * SCALAR_7));
    StellarAssetClient::new(env, &collateral_asset).mint(&borrower, &(20_000 * SCALAR_7));
    pool.submit(
        &lender,
        &lender,
        &lender,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: loan_asset.clone(),
                amount: 30_000 * SCALAR_7,
                request_type: 0,
            }
        ],
    );
    pool.submit(
        &borrower,
        &borrower,
        &borrower,
        &soroban_sdk::vec![
            env,
            v3_pool::Request {
                address: collateral_asset.clone(),
                amount: 20_000 * SCALAR_7,
                request_type: 2,
            },
            v3_pool::Request {
                address: loan_asset.clone(),
                amount: 10_000 * SCALAR_7,
                request_type: 4,
            }
        ],
    );

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 365 * 24 * 60 * 60);
    // LP-tier quoting exceeds the SDK test host's conservative default
    // budget. Run without that artificial cap and enforce Protocol 27's
    // invocation limits explicitly for both creation and fill.
    env.cost_estimate().budget().reset_unlimited();
    let auction = pool.new_interest_auction(
        &BytesN::from_array(env, &[71; 32]),
        &soroban_sdk::vec![
            env,
            loan_asset.clone(),
            collateral_asset,
            fixture.next_token.clone(),
            fixture.legacy_token.address.clone()
        ],
    );
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(auction.tier, pool_tier);
    let credit = auction.auction.lot.get(loan_asset.clone()).unwrap();
    assert!(credit >= 200 * SCALAR_7);
    assert_eq!(auction.lot_value, credit);
    assert_eq!(
        auction.auction.bid.get(bid_token.clone()).unwrap(),
        (credit * 6 + 4) / 5
    );

    let shares_before = fixture.candidate.pool_tier_shares(&backstop_tier, &pool_id);
    let blnd_usdc_assets_before = fixture
        .candidate
        .pool_tier_assets(&v3_backstop::BackstopTier::BlndUsdc, &pool_id);
    let blnd_xlm_assets_before = fixture
        .candidate
        .pool_tier_assets(&v3_backstop::BackstopTier::BlndXlm, &pool_id);
    let usdc_assets_before = fixture
        .candidate
        .pool_tier_assets(&v3_backstop::BackstopTier::Usdc, &pool_id);
    let filler = Address::generate(env);
    let bid = auction.auction.bid.get(bid_token.clone()).unwrap();
    let bid_token_client = StellarAssetClient::new(env, &bid_token);
    bid_token_client.mint(&filler, &bid);
    env.ledger()
        .set_sequence_number(auction.auction.block + 200);
    env.cost_estimate().budget().reset_unlimited();
    let fill = pool.fill_interest_auction(&filler, &100);
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert!(fill.complete);
    assert_eq!(fill.bid_amount, bid);
    assert_eq!(fill.lot.get(loan_asset.clone()), Some(credit));
    assert_eq!(loan_token.balance(&filler), credit);
    assert_eq!(TokenClient::new(env, &bid_token).balance(&filler), 0);
    assert_eq!(
        fixture
            .candidate
            .pool_tier_shares(&backstop_tier, &pool_id,),
        shares_before
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_assets(&v3_backstop::BackstopTier::BlndUsdc, &pool_id,),
        blnd_usdc_assets_before
            + if case == InterestTierCase::BlndUsdc {
                bid
            } else {
                0
            }
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_assets(&v3_backstop::BackstopTier::BlndXlm, &pool_id,),
        blnd_xlm_assets_before
            + if case == InterestTierCase::BlndXlm {
                bid
            } else {
                0
            }
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_assets(&v3_backstop::BackstopTier::Usdc, &pool_id,),
        usdc_assets_before
            + if case == InterestTierCase::Usdc {
                bid
            } else {
                0
            }
    );
    assert_eq!(pool.reserve(&loan_asset).data.backstop_credit, 0);
}

#[test]
fn optimized_v3_pool_realizes_blnd_usdc_interest_auction() {
    assert_optimized_interest_auction(InterestTierCase::BlndUsdc);
}

#[test]
fn optimized_v3_pool_realizes_blnd_xlm_interest_auction() {
    assert_optimized_interest_auction(InterestTierCase::BlndXlm);
}

#[test]
fn optimized_v3_pool_realizes_usdc_interest_auction() {
    assert_optimized_interest_auction(InterestTierCase::Usdc);
}

#[test]
fn v3_wasm_exposes_weighted_take_rate_quote() {
    let fixture = Fixture::create();
    let quote = fixture.candidate.quote_take_rate(
        &140,
        &v3_backstop::TakeRateValues {
            blnd_usdc: SCALAR_7,
            blnd_xlm: SCALAR_7,
            usdc: SCALAR_7,
        },
    );

    assert_eq!(quote.blnd_usdc, 50);
    assert_eq!(quote.blnd_xlm, 50);
    assert_eq!(quote.usdc, 40);
    assert_eq!(quote.remainder, 0);
}

#[test]
fn v3_wasm_exposes_blnd_weighted_emission_quotes() {
    let fixture = Fixture::create();
    assert_eq!(
        fixture.candidate.quote_ongoing_blnd_split(&11, &0),
        v3_backstop::OngoingBlndSplit {
            backstop: 7,
            carry: 1,
            pool: 3,
            total: 11,
        }
    );
    let pool_values = v3_backstop::BlndEmissionValues {
        blnd_usdc: 60 * SCALAR_7,
        blnd_xlm: 40 * SCALAR_7,
    };
    let pool_quote = fixture.candidate.quote_pool_blnd_emissions(
        &(1_000 * SCALAR_7),
        &pool_values,
        &(400 * SCALAR_7),
        &true,
    );
    assert_eq!(pool_quote.eligible_blnd, 100 * SCALAR_7);
    assert_eq!(pool_quote.allocation, 250 * SCALAR_7);

    let user_quote = fixture.candidate.quote_user_blnd_emissions(
        &(250 * SCALAR_7),
        &v3_backstop::BlndEmissionValues {
            blnd_usdc: 10 * SCALAR_7,
            blnd_xlm: 15 * SCALAR_7,
        },
        &(100 * SCALAR_7),
    );
    assert_eq!(user_quote.eligible_blnd, 25 * SCALAR_7);
    assert_eq!(user_quote.allocation, 625_000_000);
}

#[test]
fn v3_wasm_reads_current_comet_composition_for_emissions() {
    let env = Env::default();
    env.mock_all_auths();
    let token_admin = Address::generate(&env);
    let blnd = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let blnd_usdc = env.register(MockComet, (&blnd, &(1_000 * SCALAR_7), &(8_000 * SCALAR_7)));
    let blnd_xlm = env.register(MockComet, (&blnd, &(500 * SCALAR_7), &(1_500 * SCALAR_7)));
    let prefund = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let candidate_id = Address::generate(&env);
    let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
    let valuation_adapter =
        register_test_valuation_adapter(&env, &blnd, &prefund, &blnd_usdc, &blnd_xlm);
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &Address::generate(&env),
            &blnd,
            &pool_factory,
            &blnd_usdc,
            &blnd_xlm,
            &prefund,
            &valuation_adapter,
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);

    env.cost_estimate().budget().reset_unlimited();
    let values = candidate.spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7));
    assert_mainnet_invocation_limits!(env.cost_estimate().resources());
    assert_eq!(
        values,
        v3_backstop::BlndEmissionValues {
            blnd_usdc: 800 * SCALAR_7,
            blnd_xlm: 600 * SCALAR_7,
        }
    );

    MockCometClient::new(&env, &blnd_usdc)
        .set_composition(&(1_000 * SCALAR_7), &(4_000 * SCALAR_7));
    assert_eq!(
        candidate.spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7)),
        v3_backstop::BlndEmissionValues {
            blnd_usdc: 400 * SCALAR_7,
            blnd_xlm: 600 * SCALAR_7,
        }
    );
}

#[test]
fn direct_legacy_token_transfers_do_not_create_blnd_usdc_tier_shares() {
    let fixture = Fixture::create();
    let pool = Address::generate(&fixture.env);
    let user = Address::generate(&fixture.env);
    fixture
        .legacy_token
        .mint(&fixture.candidate.address, &(100 * SCALAR_7));

    assert_eq!(fixture.candidate.blnd_usdc_shares(&user, &pool), 0);
    assert_eq!(fixture.candidate.pool_blnd_usdc_shares(&pool), 0);
    assert_eq!(fixture.candidate.total_blnd_usdc_shares(), 0);
    assert_eq!(
        fixture
            .candidate
            .total_tier_assets(&v3_backstop::BackstopTier::BlndUsdc),
        0
    );
    assert_eq!(
        fixture.legacy_token.balance(&fixture.candidate.address),
        100 * SCALAR_7
    );
}

#[test]
fn blnd_usdc_q4w_is_bounded_and_uses_fifo_withdrawal_and_lifo_dequeue() {
    let fixture = Fixture::create();
    let pool_client = fixture.loss_aware_pool(true);
    let pool = pool_client.address.clone();
    let user = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&user, &(100 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&user, &pool, &(100 * SCALAR_7));

    let first = fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&user, &pool, &(30 * SCALAR_7));
    assert_eq!(
        first.expires_at,
        fixture.env.ledger().timestamp() + 17 * 24 * 60 * 60
    );
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp() + 1);
    fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&user, &pool, &(20 * SCALAR_7));
    assert_eq!(
        fixture.candidate.blnd_usdc_active_shares(&user, &pool),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.blnd_usdc_queued_shares(&user, &pool),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_queued_shares(&pool),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_blnd_usdc_queued_shares(),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_active_shares(&pool),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_blnd_usdc_active_shares(),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .tier_shares(&v3_backstop::BackstopTier::BlndUsdc, &user, &pool,),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .pool_tier_queued_shares(&v3_backstop::BackstopTier::BlndUsdc, &pool,),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .total_tier_active_shares(&v3_backstop::BackstopTier::BlndUsdc),
        50 * SCALAR_7
    );

    fixture
        .candidate
        .dequeue_blnd_usdc_withdrawal(&user, &pool, &(15 * SCALAR_7));
    let queue = fixture.candidate.blnd_usdc_withdrawal_queue(&user, &pool);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap().amount, 30 * SCALAR_7);
    assert_eq!(queue.get(1).unwrap().amount, 5 * SCALAR_7);
    let second_expiry = queue.get(1).unwrap().expires_at;
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_queued_shares(&pool),
        35 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_blnd_usdc_queued_shares(),
        35 * SCALAR_7
    );
    assert!(fixture
        .candidate
        .try_withdraw_blnd_usdc(&user, &pool, &(10 * SCALAR_7), &user)
        .is_err());

    fixture.env.ledger().set_timestamp(first.expires_at);
    pool_client.set_withdrawal_allowed(&false);
    assert!(fixture
        .candidate
        .try_withdraw_blnd_usdc(&user, &pool, &(10 * SCALAR_7), &user)
        .is_err());
    assert_eq!(
        fixture.candidate.blnd_usdc_queued_shares(&user, &pool),
        35 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.blnd_usdc_shares(&user, &pool),
        100 * SCALAR_7
    );
    assert_eq!(fixture.legacy_token.balance(&user), 0);
    pool_client.set_withdrawal_allowed(&true);
    assert!(fixture
        .candidate
        .try_withdraw_blnd_usdc(&user, &pool, &(31 * SCALAR_7), &user)
        .is_err());
    fixture
        .candidate
        .withdraw_blnd_usdc(&user, &pool, &(10 * SCALAR_7), &user);
    assert_eq!(
        fixture.candidate.blnd_usdc_shares(&user, &pool),
        90 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.blnd_usdc_queued_shares(&user, &pool),
        25 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_queued_shares(&pool),
        25 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_blnd_usdc_queued_shares(),
        25 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_blnd_usdc_active_shares(),
        65 * SCALAR_7
    );
    assert_eq!(fixture.legacy_token.balance(&user), 10 * SCALAR_7);

    fixture.env.ledger().set_timestamp(second_expiry);
    fixture
        .candidate
        .withdraw_blnd_usdc(&user, &pool, &(25 * SCALAR_7), &user);
    assert_eq!(
        fixture
            .candidate
            .blnd_usdc_withdrawal_queue(&user, &pool)
            .len(),
        0
    );
    assert_eq!(
        fixture.candidate.blnd_usdc_shares(&user, &pool),
        65 * SCALAR_7
    );
    assert_eq!(fixture.candidate.total_blnd_usdc_queued_shares(), 0);
    assert_eq!(
        fixture.candidate.total_blnd_usdc_active_shares(),
        65 * SCALAR_7
    );
    assert_eq!(fixture.legacy_token.balance(&user), 35 * SCALAR_7);
}

#[test]
fn blnd_usdc_deposit_rejects_unregistered_pool() {
    let fixture = Fixture::create();
    let pool = Address::generate(&fixture.env);
    let user = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&user, &SCALAR_7);

    assert!(fixture
        .candidate
        .try_deposit_blnd_usdc(&user, &pool, &SCALAR_7)
        .is_err());
    assert_eq!(fixture.candidate.blnd_usdc_shares(&user, &pool), 0);
    assert_eq!(fixture.legacy_token.balance(&user), SCALAR_7);
    assert_eq!(fixture.legacy_token.balance(&fixture.candidate.address), 0);
}

#[test]
fn blnd_usdc_deposit_rejects_registered_pool_without_loss_api() {
    let fixture = Fixture::create();
    let pool = Address::generate(&fixture.env);
    fixture.pool_factory.set_pool(&pool);
    let user = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&user, &SCALAR_7);

    assert!(fixture
        .candidate
        .try_deposit_blnd_usdc(&user, &pool, &SCALAR_7)
        .is_err());
    assert_eq!(fixture.candidate.blnd_usdc_shares(&user, &pool), 0);
    assert_eq!(fixture.legacy_token.balance(&user), SCALAR_7);
    assert_eq!(fixture.legacy_token.balance(&fixture.candidate.address), 0);
}

#[test]
fn blnd_usdc_deposit_accepts_registered_pool_while_withdrawal_is_blocked() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(false).address.clone();
    let user = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&user, &SCALAR_7);

    fixture.candidate.deposit_blnd_usdc(&user, &pool, &SCALAR_7);
    assert_eq!(fixture.candidate.blnd_usdc_shares(&user, &pool), SCALAR_7);
    assert_eq!(fixture.legacy_token.balance(&user), 0);

    let entry = fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&user, &pool, &SCALAR_7);
    fixture.env.ledger().set_timestamp(entry.expires_at);
    assert!(fixture
        .candidate
        .try_withdraw_blnd_usdc(&user, &pool, &SCALAR_7, &user)
        .is_err());
    assert_eq!(fixture.candidate.blnd_usdc_shares(&user, &pool), SCALAR_7);
    assert_eq!(fixture.legacy_token.balance(&user), 0);
}

#[test]
fn blnd_usdc_q4w_rejects_more_than_twenty_entries() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(true).address.clone();
    let user = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&user, &(21 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&user, &pool, &(21 * SCALAR_7));

    for _ in 0..20 {
        fixture
            .candidate
            .queue_blnd_usdc_withdrawal(&user, &pool, &SCALAR_7);
    }
    assert!(fixture
        .candidate
        .try_queue_blnd_usdc_withdrawal(&user, &pool, &SCALAR_7)
        .is_err());
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
    assert_eq!(fixture.candidate.usdc_token(), fixture.prefund_token);
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

    let pool = fixture.loss_aware_pool(true).address.clone();
    let depositor = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&depositor, &SCALAR_7);
    assert!(fixture
        .candidate
        .try_deposit_blnd_usdc(&depositor, &pool, &SCALAR_7)
        .is_err());
    assert_eq!(
        fixture
            .candidate
            .migration_position(&depositor, &pool)
            .amount,
        0
    );

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
    assert_eq!(fixture.candidate.backfill_end(), Some(original_unlock));
    assert_eq!(
        fixture.candidate.scheduled_backfill(),
        QUEUE_SECONDS as i128 * SCALAR_7
    );
    assert_eq!(
        fixture.blnd.balance(&fixture.candidate.address),
        10 * SCALAR_7
    );
}

#[test]
fn post_unlock_deposit_is_excluded_from_current_queue_backfill() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(true).address.clone();
    let early = Address::generate(&fixture.env);
    let late = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&early, &SCALAR_7);
    fixture.legacy_token.mint(&late, &SCALAR_7);
    fixture
        .candidate
        .deposit_blnd_usdc(&early, &pool, &SCALAR_7);
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    let unlock = fixture.candidate.prepare_migration();
    fixture.unlock();
    fixture.env.ledger().set_timestamp(unlock + 5);
    fixture.candidate.deposit_blnd_usdc(&late, &pool, &SCALAR_7);

    assert_eq!(fixture.candidate.finalize_migration(), unlock + 5);
    assert!(fixture.candidate.migration_weight(&early, &pool) > 0);
    assert_eq!(fixture.candidate.migration_weight(&late, &pool), 0);
    assert!(fixture.candidate.total_migration_weight() > 0);
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
fn unexpected_blnd_balance_delta_rolls_back_backfill_funding() {
    let fixture = Fixture::create_with_blnd_binding(false);
    let pool = fixture.loss_aware_pool(true).address.clone();
    let depositor = Address::generate(&fixture.env);
    fixture
        .legacy_token
        .mint(&fixture.incumbent, &(100 * SCALAR_7));
    fixture.legacy_token.mint(&depositor, &(100 * SCALAR_7 + 1));
    fixture
        .candidate
        .deposit_blnd_usdc(&depositor, &pool, &(100 * SCALAR_7 + 1));
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture.unlock();
    fixture.candidate.finalize_migration();

    let configured_blnd = fixture.candidate.blnd_token();
    let prefunded = fixture.candidate.scheduled_backfill();
    StellarAssetClient::new(&fixture.env, &configured_blnd)
        .mint(&fixture.candidate.address, &prefunded);
    assert!(fixture.candidate.try_fund_backfill().is_err());
    assert_eq!(fixture.candidate.funded_backfill(), None);
    assert!(!fixture.candidate.blnd_binding_verified());
    assert_eq!(fixture.candidate.total_backfill_claimed(), 0);
    assert_eq!(fixture.blnd.balance(&fixture.candidate.address), 0);
    assert_eq!(
        TokenClient::new(&fixture.env, &configured_blnd).balance(&fixture.candidate.address),
        prefunded
    );
}

#[test]
fn withdrawn_shares_forfeit_weight_to_time_weighted_survivors() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(true).address.clone();
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
    assert_eq!(
        fixture.candidate.blnd_usdc_shares(&alice, &pool),
        200 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_shares(&pool),
        500 * SCALAR_7
    );
    assert_eq!(fixture.candidate.total_blnd_usdc_shares(), 500 * SCALAR_7);
    fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&alice, &pool, &(50 * SCALAR_7));
    fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&carol, &pool, &(100 * SCALAR_7));

    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture
        .candidate
        .withdraw_migration(&alice, &pool, &(50 * SCALAR_7), &alice);
    fixture
        .candidate
        .withdraw_migration(&carol, &pool, &(50 * SCALAR_7), &carol);
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_shares(&pool),
        400 * SCALAR_7
    );
    assert_eq!(fixture.candidate.total_blnd_usdc_shares(), 400 * SCALAR_7);
    assert_eq!(
        fixture.candidate.blnd_usdc_queued_shares(&carol, &pool),
        50 * SCALAR_7
    );
    fixture.unlock();
    let migration_time = fixture.env.ledger().timestamp();
    fixture.candidate.finalize_migration();

    let alice_weight = (150 * SCALAR_7) * (migration_time - alice_start) as i128;
    let bob_weight = (200 * SCALAR_7) * (migration_time - bob_start) as i128;
    let carol_weight = (50 * SCALAR_7) * (migration_time - alice_start) as i128;
    assert_eq!(
        fixture.candidate.migration_weight(&alice, &pool),
        alice_weight
    );
    assert_eq!(fixture.candidate.migration_weight(&bob, &pool), bob_weight);
    assert_eq!(
        fixture.candidate.migration_weight(&carol, &pool),
        carol_weight
    );
    assert_eq!(
        fixture.candidate.total_migration_weight(),
        alice_weight + bob_weight + carol_weight
    );

    let dave = Address::generate(&fixture.env);
    fixture.legacy_token.mint(&dave, &(50 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_usdc(&dave, &pool, &(50 * SCALAR_7));
    assert_eq!(
        fixture.candidate.blnd_usdc_shares(&dave, &pool),
        50 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_shares(&pool),
        450 * SCALAR_7
    );
    assert_eq!(fixture.candidate.total_blnd_usdc_shares(), 450 * SCALAR_7);
    assert_eq!(fixture.candidate.migration_weight(&dave, &pool), 0);
    assert_eq!(
        fixture.candidate.total_migration_weight(),
        alice_weight + bob_weight + carol_weight
    );
    fixture
        .candidate
        .queue_blnd_usdc_withdrawal(&dave, &pool, &(20 * SCALAR_7));
    assert!(fixture
        .candidate
        .try_withdraw_blnd_usdc(&dave, &pool, &(20 * SCALAR_7), &dave)
        .is_err());
    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp() + 17 * 24 * 60 * 60);
    fixture
        .candidate
        .withdraw_blnd_usdc(&dave, &pool, &(20 * SCALAR_7), &dave);
    assert_eq!(
        fixture.candidate.blnd_usdc_shares(&dave, &pool),
        30 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_blnd_usdc_shares(&pool),
        430 * SCALAR_7
    );
    assert_eq!(fixture.candidate.total_blnd_usdc_shares(), 430 * SCALAR_7);
    assert_eq!(fixture.candidate.migration_weight(&dave, &pool), 0);

    let backfill = fixture.candidate.scheduled_backfill();
    assert_eq!(backfill, QUEUE_SECONDS as i128 * SCALAR_7);
    let eligible_weight = alice_weight + bob_weight + carol_weight;
    assert_eq!(
        fixture.candidate.quote_backfill(&alice, &pool),
        backfill * alice_weight / eligible_weight
    );
    assert_eq!(
        fixture.candidate.quote_backfill(&bob, &pool),
        backfill * bob_weight / eligible_weight
    );
    assert_eq!(
        fixture.candidate.quote_backfill(&carol, &pool),
        backfill * carol_weight / eligible_weight
    );

    assert_eq!(fixture.candidate.funded_backfill(), None);
    let candidate_blnd_before = fixture.blnd.balance(&fixture.candidate.address);
    assert_eq!(fixture.candidate.fund_backfill(), backfill);
    assert_eq!(fixture.candidate.funded_backfill(), Some(backfill));
    assert_eq!(fixture.candidate.total_backfill_claimed(), 0);
    assert_eq!(
        fixture.blnd.balance(&fixture.candidate.address),
        candidate_blnd_before + backfill
    );
    assert!(fixture.candidate.try_fund_backfill().is_err());

    let alice_recipient = Address::generate(&fixture.env);
    let alice_claim = fixture
        .candidate
        .claim_backfill(&alice, &pool, &alice_recipient);
    let bob_claim = fixture.candidate.claim_backfill(&bob, &pool, &bob);
    let carol_claim = fixture.candidate.claim_backfill(&carol, &pool, &carol);
    let total_claimed = alice_claim + bob_claim + carol_claim;

    assert_eq!(alice_claim, backfill * alice_weight / eligible_weight);
    assert_eq!(bob_claim, backfill * bob_weight / eligible_weight);
    assert_eq!(carol_claim, backfill * carol_weight / eligible_weight);
    assert_eq!(fixture.blnd.balance(&alice_recipient), alice_claim);
    assert_eq!(fixture.blnd.balance(&bob), bob_claim);
    assert_eq!(fixture.blnd.balance(&carol), carol_claim);
    assert_eq!(fixture.candidate.total_backfill_claimed(), total_claimed);
    assert_eq!(
        fixture.candidate.remaining_backfill(),
        backfill - total_claimed
    );
    assert_eq!(
        fixture.blnd.balance(&fixture.candidate.address),
        candidate_blnd_before + backfill - total_claimed
    );
    assert!(fixture
        .candidate
        .try_claim_backfill(&alice, &pool, &alice)
        .is_err());
    assert!(fixture
        .candidate
        .try_claim_backfill(&dave, &pool, &dave)
        .is_err());
    assert!(
        fixture
            .candidate
            .migration_position(&alice, &pool)
            .backfill_claimed
    );
}

#[test]
fn standard_tiers_are_pool_attributed_queued_and_conserved() {
    let fixture = Fixture::create();
    let pool_client = fixture.loss_aware_pool(false);
    let pool = pool_client.address.clone();
    let depositor = Address::generate(&fixture.env);
    let recipient = Address::generate(&fixture.env);
    let blnd_xlm = StellarAssetClient::new(&fixture.env, &fixture.next_token);
    let usdc = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);
    let blnd_xlm_tier = v3_backstop::BackstopTier::BlndXlm;
    let usdc_tier = v3_backstop::BackstopTier::Usdc;

    blnd_xlm.mint(&depositor, &(100 * SCALAR_7));
    usdc.mint(&depositor, &(80 * SCALAR_7));
    fixture
        .candidate
        .deposit_blnd_xlm(&depositor, &pool, &(100 * SCALAR_7));
    fixture
        .candidate
        .deposit_usdc(&depositor, &pool, &(80 * SCALAR_7));

    assert_eq!(
        fixture
            .candidate
            .tier_shares(&blnd_xlm_tier, &depositor, &pool),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.tier_shares(&usdc_tier, &depositor, &pool),
        80 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_tier_shares(&blnd_xlm_tier, &pool),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_tier_shares(&usdc_tier, &pool),
        80 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_shares(&blnd_xlm_tier),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_shares(&usdc_tier),
        80 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_assets(&blnd_xlm_tier),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_assets(&usdc_tier),
        80 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .migration_position(&depositor, &pool)
            .amount,
        0
    );
    assert!(fixture.candidate.try_begin_migration().is_err());

    // Unattributed direct transfers remain in custody but never mint shares.
    blnd_xlm.mint(&depositor, &(7 * SCALAR_7));
    usdc.mint(&depositor, &(7 * SCALAR_7));
    TokenClient::new(&fixture.env, &fixture.next_token).transfer(
        &depositor,
        &fixture.candidate.address,
        &(7 * SCALAR_7),
    );
    TokenClient::new(&fixture.env, &fixture.prefund_token).transfer(
        &depositor,
        &fixture.candidate.address,
        &(7 * SCALAR_7),
    );
    assert_eq!(
        fixture.candidate.total_tier_shares(&blnd_xlm_tier),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_shares(&usdc_tier),
        80 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_assets(&blnd_xlm_tier),
        100 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_assets(&usdc_tier),
        80 * SCALAR_7
    );

    let blnd_xlm_entry =
        fixture
            .candidate
            .queue_blnd_xlm_withdrawal(&depositor, &pool, &(30 * SCALAR_7));
    let usdc_entry = fixture
        .candidate
        .queue_usdc_withdrawal(&depositor, &pool, &(20 * SCALAR_7));
    assert_eq!(
        blnd_xlm_entry.expires_at,
        fixture.env.ledger().timestamp() + 17 * 24 * 60 * 60
    );
    assert_eq!(usdc_entry.expires_at, blnd_xlm_entry.expires_at);
    fixture
        .candidate
        .dequeue_blnd_xlm_withdrawal(&depositor, &pool, &(5 * SCALAR_7));
    assert_eq!(
        fixture
            .candidate
            .tier_queued_shares(&blnd_xlm_tier, &depositor, &pool),
        25 * SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .tier_active_shares(&blnd_xlm_tier, &depositor, &pool),
        75 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.pool_tier_queued_shares(&usdc_tier, &pool),
        20 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_active_shares(&usdc_tier),
        60 * SCALAR_7
    );
    assert!(fixture
        .candidate
        .try_withdraw_blnd_xlm(&depositor, &pool, &SCALAR_7, &recipient)
        .is_err());

    fixture
        .env
        .ledger()
        .set_timestamp(blnd_xlm_entry.expires_at.max(usdc_entry.expires_at));
    pool_client.set_withdrawal_allowed(&false);
    assert!(fixture
        .candidate
        .try_withdraw_usdc(&depositor, &pool, &(20 * SCALAR_7), &recipient)
        .is_err());
    pool_client.set_withdrawal_allowed(&true);
    fixture
        .candidate
        .withdraw_blnd_xlm(&depositor, &pool, &(25 * SCALAR_7), &recipient);
    fixture
        .candidate
        .withdraw_usdc(&depositor, &pool, &(20 * SCALAR_7), &recipient);

    assert_eq!(
        fixture.candidate.total_tier_shares(&blnd_xlm_tier),
        75 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_shares(&usdc_tier),
        60 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_assets(&blnd_xlm_tier),
        75 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_assets(&usdc_tier),
        60 * SCALAR_7
    );
    assert_eq!(
        fixture.candidate.total_tier_queued_shares(&blnd_xlm_tier),
        0
    );
    assert_eq!(fixture.candidate.total_tier_queued_shares(&usdc_tier), 0);
    assert_eq!(blnd_xlm.balance(&recipient), 25 * SCALAR_7);
    assert_eq!(usdc.balance(&recipient), 20 * SCALAR_7);
    assert_eq!(blnd_xlm.balance(&fixture.candidate.address), 82 * SCALAR_7);
    assert_eq!(usdc.balance(&fixture.candidate.address), 67 * SCALAR_7);
}

#[test]
fn standard_tier_deposits_require_a_compatible_registered_pool() {
    let fixture = Fixture::create();
    let unregistered_pool = Address::generate(&fixture.env);
    let incompatible_pool = Address::generate(&fixture.env);
    fixture.pool_factory.set_pool(&incompatible_pool);
    let depositor = Address::generate(&fixture.env);
    let blnd_xlm = StellarAssetClient::new(&fixture.env, &fixture.next_token);
    let usdc = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);
    blnd_xlm.mint(&depositor, &(2 * SCALAR_7));
    usdc.mint(&depositor, &(2 * SCALAR_7));

    assert!(fixture
        .candidate
        .try_deposit_blnd_xlm(&depositor, &unregistered_pool, &SCALAR_7)
        .is_err());
    assert!(fixture
        .candidate
        .try_deposit_usdc(&depositor, &unregistered_pool, &SCALAR_7)
        .is_err());
    assert!(fixture
        .candidate
        .try_deposit_blnd_xlm(&depositor, &incompatible_pool, &SCALAR_7)
        .is_err());
    assert!(fixture
        .candidate
        .try_deposit_usdc(&depositor, &incompatible_pool, &SCALAR_7)
        .is_err());
    assert_eq!(blnd_xlm.balance(&depositor), 2 * SCALAR_7);
    assert_eq!(usdc.balance(&depositor), 2 * SCALAR_7);
    assert_eq!(
        fixture
            .candidate
            .total_tier_shares(&v3_backstop::BackstopTier::BlndXlm),
        0
    );
    assert_eq!(
        fixture
            .candidate
            .total_tier_shares(&v3_backstop::BackstopTier::Usdc),
        0
    );
}

#[test]
fn standard_tier_deposits_remain_available_after_migration() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(true).address.clone();
    fixture.fund_qualifying_balances();
    fixture.queue();
    fixture.enter_preparation_window();
    fixture.candidate.prepare_migration();
    fixture.unlock();
    fixture.candidate.finalize_migration();

    let depositor = Address::generate(&fixture.env);
    let blnd_xlm = StellarAssetClient::new(&fixture.env, &fixture.next_token);
    let usdc = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);
    blnd_xlm.mint(&depositor, &SCALAR_7);
    usdc.mint(&depositor, &SCALAR_7);
    fixture
        .candidate
        .deposit_blnd_xlm(&depositor, &pool, &SCALAR_7);
    fixture.candidate.deposit_usdc(&depositor, &pool, &SCALAR_7);

    assert_eq!(
        fixture
            .candidate
            .tier_shares(&v3_backstop::BackstopTier::BlndXlm, &depositor, &pool,),
        SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .tier_shares(&v3_backstop::BackstopTier::Usdc, &depositor, &pool,),
        SCALAR_7
    );
    assert_eq!(
        fixture
            .candidate
            .migration_position(&depositor, &pool)
            .amount,
        0
    );
}

#[test]
fn standard_tier_q4w_rejects_more_than_twenty_entries() {
    let fixture = Fixture::create();
    let pool = fixture.loss_aware_pool(true).address.clone();
    let depositor = Address::generate(&fixture.env);
    let usdc = StellarAssetClient::new(&fixture.env, &fixture.prefund_token);
    usdc.mint(&depositor, &(21 * SCALAR_7));
    fixture
        .candidate
        .deposit_usdc(&depositor, &pool, &(21 * SCALAR_7));

    for _ in 0..20 {
        fixture
            .candidate
            .queue_usdc_withdrawal(&depositor, &pool, &SCALAR_7);
    }
    assert!(fixture
        .candidate
        .try_queue_usdc_withdrawal(&depositor, &pool, &SCALAR_7)
        .is_err());
}
