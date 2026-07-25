use soroban_sdk::{
    contractclient, contractevent, contracttype, panic_with_error, token, Address, Env, Map, Vec,
    I256,
};

use crate::{get_backstop, get_status, require_action_allowed, set_backstop_liability, PoolError};

pub const SCALAR_7: i128 = 10_000_000;
pub const SCALAR_12: i128 = 1_000_000_000_000;
const SECONDS_PER_YEAR: i128 = 31_536_000;
const MAX_RESERVES: u32 = 30;
const MAX_POSITIONS: u32 = MAX_RESERVES * 2;
const MAX_REQUESTS: u32 = MAX_POSITIONS;
const MIN_HEALTH_FACTOR: i128 = 10_000_100;
const SHARED_TTL_THRESHOLD: u32 = 179 * 17_280;
const SHARED_TTL_BUMP: u32 = 180 * 17_280;
const USER_TTL_THRESHOLD: u32 = 359 * 17_280;
const USER_TTL_BUMP: u32 = 360 * 17_280;

pub const ACTION_SUPPLY: u32 = 0;
pub const ACTION_WITHDRAW: u32 = 1;
pub const ACTION_SUPPLY_COLLATERAL: u32 = 2;
pub const ACTION_WITHDRAW_COLLATERAL: u32 = 3;
pub const ACTION_BORROW: u32 = 4;
pub const ACTION_REPAY: u32 = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct LendingConfig {
    pub backstop_take_rate: u32,
    pub max_positions: u32,
    pub min_collateral: i128,
    pub oracle: Address,
    pub price_decimals: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReserveConfig {
    pub asset: Address,
    pub c_factor: u32,
    pub decimals: u32,
    pub enabled: bool,
    pub index: u32,
    pub l_factor: u32,
    pub max_util: u32,
    pub r_base: u32,
    pub r_one: u32,
    pub r_three: u32,
    pub r_two: u32,
    pub reactivity: u32,
    pub supply_cap: i128,
    pub util: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReserveData {
    pub b_rate: i128,
    pub b_supply: i128,
    pub backstop_credit: i128,
    pub d_rate: i128,
    pub d_supply: i128,
    pub ir_mod: i128,
    pub last_time: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Reserve {
    pub config: ReserveConfig,
    pub data: ReserveData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Positions {
    pub collateral: Map<u32, i128>,
    pub liabilities: Map<u32, i128>,
    pub supply: Map<u32, i128>,
}

impl Positions {
    fn empty(env: &Env) -> Self {
        Self {
            collateral: Map::new(env),
            liabilities: Map::new(env),
            supply: Map::new(env),
        }
    }

    fn effective_count(&self) -> u32 {
        self.collateral.len() + self.liabilities.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Request {
    pub address: Address,
    pub amount: i128,
    pub request_type: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PriceQuote {
    pub decimals: u32,
    pub price: i128,
    pub valid_until: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PositionHealth {
    pub collateral_base: i128,
    pub healthy: bool,
    pub liability_base: i128,
    pub raw_collateral_base: i128,
}

#[contractclient(name = "PriceOracleClient")]
#[allow(dead_code)]
pub trait PriceOracle {
    fn price(env: Env, asset: Address) -> PriceQuote;
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LendingAction {
    pub action: u32,
    pub amount: i128,
    pub asset: Address,
    pub owner: Address,
    pub position_delta: i128,
}

#[derive(Clone)]
#[contracttype]
pub(crate) enum LendingDataKey {
    Config,
    ReserveConfig(Address),
    ReserveList,
}

#[derive(Clone)]
#[contracttype]
enum LendingPersistentKey {
    Positions(Address),
    ReserveData(Address),
}

struct Transfers {
    incoming: Map<Address, i128>,
    outgoing: Map<Address, i128>,
}

impl Transfers {
    fn new(env: &Env) -> Self {
        Self {
            incoming: Map::new(env),
            outgoing: Map::new(env),
        }
    }

    fn add_incoming(&mut self, env: &Env, asset: &Address, amount: i128) {
        let previous = self.incoming.get(asset.clone()).unwrap_or(0);
        self.incoming
            .set(asset.clone(), checked_add(env, previous, amount));
    }

    fn add_outgoing(&mut self, env: &Env, asset: &Address, amount: i128) {
        let previous = self.outgoing.get(asset.clone()).unwrap_or(0);
        self.outgoing
            .set(asset.clone(), checked_add(env, previous, amount));
    }
}

pub(crate) fn initialize(env: &Env, config: &LendingConfig, reserve_configs: &Vec<ReserveConfig>) {
    validate_lending_config(env, config);
    if reserve_configs.len() > MAX_RESERVES {
        panic_with_error!(env, PoolError::TooManyReserves);
    }

    let mut assets = Vec::new(env);
    for reserve in reserve_configs {
        validate_reserve_config(env, &reserve);
        if reserve.index != assets.len() || assets.contains(&reserve.asset) {
            panic_with_error!(env, PoolError::InvalidReserveConfig);
        }
        assets.push_back(reserve.asset.clone());
        env.storage().instance().set(
            &LendingDataKey::ReserveConfig(reserve.asset.clone()),
            &reserve,
        );
        set_reserve_data(
            env,
            &reserve.asset,
            &ReserveData {
                b_rate: SCALAR_12,
                b_supply: 0,
                backstop_credit: 0,
                d_rate: SCALAR_12,
                d_supply: 0,
                ir_mod: SCALAR_7,
                last_time: env.ledger().timestamp(),
            },
        );
    }
    env.storage()
        .instance()
        .set(&LendingDataKey::Config, config);
    env.storage()
        .instance()
        .set(&LendingDataKey::ReserveList, &assets);
}

pub(crate) fn config(env: &Env) -> LendingConfig {
    env.storage()
        .instance()
        .get(&LendingDataKey::Config)
        .unwrap()
}

pub(crate) fn reserve_list(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&LendingDataKey::ReserveList)
        .unwrap()
}

pub(crate) fn reserve(env: &Env, asset: &Address) -> Reserve {
    accrue_reserve(env, asset)
}

pub(crate) fn positions(env: &Env, user: &Address) -> Positions {
    let key = LendingPersistentKey::Positions(user.clone());
    let result = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Positions::empty(env));
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, USER_TTL_THRESHOLD, USER_TTL_BUMP);
    }
    result
}

pub(crate) fn health(env: &Env, user: &Address) -> PositionHealth {
    calculate_health(env, &positions(env, user))
}

pub(crate) fn submit(
    env: &Env,
    from: &Address,
    spender: &Address,
    to: &Address,
    requests: Vec<Request>,
) -> Positions {
    let pool = env.current_contract_address();
    if from == &pool
        || spender == &pool
        || to == &pool
        || requests.is_empty()
        || requests.len() > MAX_REQUESTS
    {
        panic_with_error!(env, PoolError::BadRequest);
    }
    spender.require_auth();
    if from != spender {
        from.require_auth();
    }

    let lending_config = config(env);
    let mut user = positions(env, from);
    let previous_position_count = user.effective_count();
    let mut transfers = Transfers::new(env);
    let mut health_check = false;
    let mut liability_checks: Map<Address, bool> = Map::new(env);
    let mut utilization_checks: Map<Address, bool> = Map::new(env);

    for request in requests {
        require_positive(env, request.amount);
        if request.request_type > ACTION_REPAY {
            panic_with_error!(env, PoolError::InvalidAction);
        }
        require_action_allowed(env, get_status(env), request.request_type);
        let mut reserve = accrue_reserve(env, &request.address);
        if !reserve.config.enabled
            && (request.request_type == ACTION_SUPPLY
                || request.request_type == ACTION_SUPPLY_COLLATERAL
                || request.request_type == ACTION_BORROW)
        {
            panic_with_error!(env, PoolError::ReserveDisabled);
        }

        let (event_amount, position_delta) = match request.request_type {
            ACTION_SUPPLY => {
                let minted = to_b_tokens_down(env, &reserve, request.amount);
                require_positive(env, minted);
                add_position(env, &mut user.supply, reserve.config.index, minted);
                reserve.data.b_supply = checked_add(env, reserve.data.b_supply, minted);
                require_supply_cap(env, &reserve);
                transfers.add_incoming(env, &reserve.config.asset, request.amount);
                (request.amount, minted)
            }
            ACTION_WITHDRAW => {
                let balance = user.supply.get(reserve.config.index).unwrap_or(0);
                let requested_burn = to_b_tokens_up(env, &reserve, request.amount);
                let burn = core::cmp::min(requested_burn, balance);
                require_positive(env, burn);
                let amount = if burn == requested_burn {
                    request.amount
                } else {
                    to_asset_from_b_tokens(env, &reserve, burn)
                };
                remove_position(env, &mut user.supply, reserve.config.index, burn);
                reserve.data.b_supply = checked_sub_nonnegative(env, reserve.data.b_supply, burn);
                require_utilization_below_one(env, &reserve);
                transfers.add_outgoing(env, &reserve.config.asset, amount);
                (amount, -burn)
            }
            ACTION_SUPPLY_COLLATERAL => {
                if reserve.config.c_factor == 0 {
                    panic_with_error!(env, PoolError::InvalidReserveConfig);
                }
                let minted = to_b_tokens_down(env, &reserve, request.amount);
                require_positive(env, minted);
                add_position(env, &mut user.collateral, reserve.config.index, minted);
                reserve.data.b_supply = checked_add(env, reserve.data.b_supply, minted);
                require_supply_cap(env, &reserve);
                transfers.add_incoming(env, &reserve.config.asset, request.amount);
                (request.amount, minted)
            }
            ACTION_WITHDRAW_COLLATERAL => {
                let balance = user.collateral.get(reserve.config.index).unwrap_or(0);
                let requested_burn = to_b_tokens_up(env, &reserve, request.amount);
                let burn = core::cmp::min(requested_burn, balance);
                require_positive(env, burn);
                let amount = if burn == requested_burn {
                    request.amount
                } else {
                    to_asset_from_b_tokens(env, &reserve, burn)
                };
                remove_position(env, &mut user.collateral, reserve.config.index, burn);
                reserve.data.b_supply = checked_sub_nonnegative(env, reserve.data.b_supply, burn);
                require_utilization_below_one(env, &reserve);
                transfers.add_outgoing(env, &reserve.config.asset, amount);
                health_check = true;
                (amount, -burn)
            }
            ACTION_BORROW => {
                if reserve.config.l_factor == 0 {
                    panic_with_error!(env, PoolError::ReserveDisabled);
                }
                let minted = to_d_tokens_up(env, &reserve, request.amount);
                require_positive(env, minted);
                add_position(env, &mut user.liabilities, reserve.config.index, minted);
                reserve.data.d_supply = checked_add(env, reserve.data.d_supply, minted);
                require_utilization_below_one(env, &reserve);
                transfers.add_outgoing(env, &reserve.config.asset, request.amount);
                liability_checks.set(reserve.config.asset.clone(), true);
                utilization_checks.set(reserve.config.asset.clone(), true);
                health_check = true;
                (request.amount, minted)
            }
            ACTION_REPAY => {
                let balance = user.liabilities.get(reserve.config.index).unwrap_or(0);
                require_positive(env, balance);
                let debt = to_asset_from_d_tokens(env, &reserve, balance);
                let (burn, repaid) = if request.amount >= debt {
                    transfers.add_outgoing(
                        env,
                        &reserve.config.asset,
                        checked_sub_nonnegative(env, request.amount, debt),
                    );
                    (balance, debt)
                } else {
                    (
                        to_d_tokens_down(env, &reserve, request.amount),
                        request.amount,
                    )
                };
                require_positive(env, burn);
                transfers.add_incoming(env, &reserve.config.asset, request.amount);
                remove_position(env, &mut user.liabilities, reserve.config.index, burn);
                reserve.data.d_supply = checked_sub_nonnegative(env, reserve.data.d_supply, burn);
                liability_checks.set(reserve.config.asset.clone(), true);
                (repaid, -burn)
            }
            _ => panic_with_error!(env, PoolError::InvalidAction),
        };

        set_reserve_data(env, &reserve.config.asset, &reserve.data);
        LendingAction {
            action: request.request_type,
            amount: event_amount,
            asset: request.address,
            owner: from.clone(),
            position_delta,
        }
        .publish(env);
    }

    if user.effective_count() > lending_config.max_positions
        && user.effective_count() > previous_position_count
    {
        panic_with_error!(env, PoolError::TooManyPositions);
    }
    for (asset, _) in utilization_checks {
        let reserve = accrue_reserve(env, &asset);
        if utilization(env, &reserve) > i128::from(reserve.config.max_util) {
            panic_with_error!(env, PoolError::InvalidUtilization);
        }
    }
    if health_check && !user.liabilities.is_empty() {
        let result = calculate_health(env, &user);
        if !result.healthy {
            panic_with_error!(env, PoolError::InvalidHealthFactor);
        }
        if result.collateral_base < lending_config.min_collateral {
            panic_with_error!(env, PoolError::MinimumCollateralNotMet);
        }
    }
    if from == &get_backstop(env) {
        for (asset, _) in liability_checks {
            let reserve = reserve_config(env, &asset);
            set_backstop_liability(
                env,
                &asset,
                user.liabilities.get(reserve.index).unwrap_or(0),
            );
        }
    }

    let mut balances_before: Map<Address, i128> = Map::new(env);
    for (asset, _) in transfers.incoming.iter() {
        balances_before.set(
            asset.clone(),
            token::Client::new(env, &asset).balance(&pool),
        );
    }
    for (asset, _) in transfers.outgoing.iter() {
        if !balances_before.contains_key(asset.clone()) {
            balances_before.set(
                asset.clone(),
                token::Client::new(env, &asset).balance(&pool),
            );
        }
    }
    for (asset, amount) in transfers.incoming.iter() {
        token::Client::new(env, &asset).transfer(spender, &pool, &amount);
    }
    for (asset, amount) in transfers.outgoing.iter() {
        if amount > 0 {
            token::Client::new(env, &asset).transfer(&pool, to, &amount);
        }
    }
    for (asset, balance_before) in balances_before {
        let expected = checked_sub_nonnegative(
            env,
            checked_add(
                env,
                balance_before,
                transfers.incoming.get(asset.clone()).unwrap_or(0),
            ),
            transfers.outgoing.get(asset.clone()).unwrap_or(0),
        );
        if token::Client::new(env, &asset).balance(&pool) != expected {
            panic_with_error!(env, PoolError::InvalidTokenBalance);
        }
    }
    set_positions(env, from, &user);
    user
}

fn validate_lending_config(env: &Env, config: &LendingConfig) {
    if config.backstop_take_rate >= SCALAR_7 as u32
        || config.max_positions < 2
        || config.max_positions > MAX_POSITIONS
        || config.min_collateral < 0
        || config.price_decimals > 18
    {
        panic_with_error!(env, PoolError::InvalidLendingConfig);
    }
}

fn validate_reserve_config(env: &Env, config: &ReserveConfig) {
    if config.decimals > 18
        || config.c_factor > SCALAR_7 as u32
        || config.l_factor > SCALAR_7 as u32
        || config.util == 0
        || config.util > 9_000_000
        || config.max_util > SCALAR_7 as u32
        || config.max_util <= config.util
        || config.r_base >= SCALAR_7 as u32
        || config.r_base < 1_000
        || config.r_one > config.r_two
        || config.r_two > config.r_three
        || config.reactivity > 1_000
        || config.supply_cap < 0
    {
        panic_with_error!(env, PoolError::InvalidReserveConfig);
    }
}

fn set_positions(env: &Env, user: &Address, positions: &Positions) {
    let key = LendingPersistentKey::Positions(user.clone());
    env.storage().persistent().set(&key, positions);
    env.storage()
        .persistent()
        .extend_ttl(&key, USER_TTL_THRESHOLD, USER_TTL_BUMP);
}

fn reserve_config(env: &Env, asset: &Address) -> ReserveConfig {
    env.storage()
        .instance()
        .get(&LendingDataKey::ReserveConfig(asset.clone()))
        .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve))
}

fn reserve_data(env: &Env, asset: &Address) -> ReserveData {
    let key = LendingPersistentKey::ReserveData(asset.clone());
    let data = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve));
    env.storage()
        .persistent()
        .extend_ttl(&key, SHARED_TTL_THRESHOLD, SHARED_TTL_BUMP);
    data
}

fn set_reserve_data(env: &Env, asset: &Address, data: &ReserveData) {
    let key = LendingPersistentKey::ReserveData(asset.clone());
    env.storage().persistent().set(&key, data);
    env.storage()
        .persistent()
        .extend_ttl(&key, SHARED_TTL_THRESHOLD, SHARED_TTL_BUMP);
}

fn accrue_reserve(env: &Env, asset: &Address) -> Reserve {
    let reserve_config = reserve_config(env, asset);
    let mut reserve = Reserve {
        data: reserve_data(env, asset),
        config: reserve_config,
    };
    let now = env.ledger().timestamp();
    if now <= reserve.data.last_time {
        return reserve;
    }
    if reserve.data.b_supply == 0 || reserve.data.d_supply == 0 {
        reserve.data.last_time = now;
        return reserve;
    }

    let current_utilization = utilization(env, &reserve);
    let (loan_accrual, new_ir_mod) = calculate_accrual(
        env,
        &reserve.config,
        current_utilization,
        reserve.data.ir_mod,
        reserve.data.last_time,
        now,
    );
    let previous_liabilities = total_liabilities(env, &reserve);
    reserve.data.d_rate = mul_div_ceil(env, loan_accrual, reserve.data.d_rate, SCALAR_12);
    reserve.data.ir_mod = new_ir_mod;
    let accrued =
        checked_sub_nonnegative(env, total_liabilities(env, &reserve), previous_liabilities);
    if accrued > 0 {
        let previous_supply = total_supply(env, &reserve);
        let take = mul_div_floor(
            env,
            accrued,
            i128::from(config(env).backstop_take_rate),
            SCALAR_7,
        );
        reserve.data.backstop_credit = checked_add(env, reserve.data.backstop_credit, take);
        let supplier_accrual = checked_sub_nonnegative(env, accrued, take);
        reserve.data.b_rate = mul_div_floor(
            env,
            checked_add(env, previous_supply, supplier_accrual),
            SCALAR_12,
            reserve.data.b_supply,
        );
    }
    reserve.data.last_time = now;
    reserve
}

fn calculate_accrual(
    env: &Env,
    config: &ReserveConfig,
    current_utilization: i128,
    ir_mod: i128,
    last_time: u64,
    now: u64,
) -> (i128, i128) {
    let target = i128::from(config.util);
    let current_rate = if current_utilization <= target {
        let base_rate = checked_add(
            env,
            i128::from(config.r_base),
            mul_div_ceil(env, current_utilization, i128::from(config.r_one), target),
        );
        mul_div_ceil(env, base_rate, ir_mod, SCALAR_7)
    } else if current_utilization <= 9_500_000 {
        let base_rate = checked_add(
            env,
            checked_add(env, i128::from(config.r_base), i128::from(config.r_one)),
            mul_div_ceil(
                env,
                current_utilization - target,
                i128::from(config.r_two),
                9_500_000 - target,
            ),
        );
        mul_div_ceil(env, base_rate, ir_mod, SCALAR_7)
    } else {
        let intersection = mul_div_ceil(
            env,
            checked_add(
                env,
                i128::from(config.r_base),
                checked_add(env, i128::from(config.r_one), i128::from(config.r_two)),
            ),
            ir_mod,
            SCALAR_7,
        );
        checked_add(
            env,
            intersection,
            mul_div_ceil(
                env,
                current_utilization - 9_500_000,
                i128::from(config.r_three),
                500_000,
            ),
        )
    };
    let elapsed = i128::from(now - last_time);
    let time_weight = mul_div_floor(env, elapsed, SCALAR_12, SECONDS_PER_YEAR);
    let accrual = checked_add(
        env,
        SCALAR_12,
        mul_div_ceil(env, time_weight, current_rate, SCALAR_7),
    );

    let utilization_difference = current_utilization - target;
    let modifier_delta = mul3_div_floor(
        env,
        elapsed,
        utilization_difference.unsigned_abs() as i128,
        i128::from(config.reactivity),
        SCALAR_7,
    );
    let next_modifier = if utilization_difference >= 0 {
        checked_add(env, ir_mod, modifier_delta)
    } else {
        ir_mod.checked_sub(modifier_delta).unwrap_or(0)
    };
    (accrual, next_modifier.clamp(SCALAR_7 / 10, SCALAR_7 * 10))
}

fn calculate_health(env: &Env, positions: &Positions) -> PositionHealth {
    let lending_config = config(env);
    let mut collateral_base = 0_i128;
    let mut raw_collateral_base = 0_i128;
    let mut liability_base = 0_i128;
    let mut reserve_index = 0_u32;
    for asset in reserve_list(env) {
        let collateral = positions.collateral.get(reserve_index).unwrap_or(0);
        let liabilities = positions.liabilities.get(reserve_index).unwrap_or(0);
        reserve_index = reserve_index
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
        if collateral == 0 && liabilities == 0 {
            continue;
        }
        let reserve = accrue_reserve(env, &asset);
        let quote = PriceOracleClient::new(env, &lending_config.oracle).price(&asset);
        if quote.price <= 0
            || quote.decimals != lending_config.price_decimals
            || quote.valid_until < env.ledger().timestamp()
        {
            panic_with_error!(env, PoolError::InvalidPrice);
        }
        let asset_scalar = pow10(env, reserve.config.decimals);
        if collateral > 0 {
            let assets = to_asset_from_b_tokens(env, &reserve, collateral);
            let raw_value = mul_div_floor(env, quote.price, assets, asset_scalar);
            raw_collateral_base = checked_add(env, raw_collateral_base, raw_value);
            let effective_assets =
                mul_div_floor(env, assets, i128::from(reserve.config.c_factor), SCALAR_7);
            collateral_base = checked_add(
                env,
                collateral_base,
                mul_div_floor(env, quote.price, effective_assets, asset_scalar),
            );
        }
        if liabilities > 0 {
            let assets = to_asset_from_d_tokens(env, &reserve, liabilities);
            let effective_assets =
                mul_div_ceil(env, assets, SCALAR_7, i128::from(reserve.config.l_factor));
            liability_base = checked_add(
                env,
                liability_base,
                mul_div_ceil(env, quote.price, effective_assets, asset_scalar),
            );
        }
    }
    let healthy = liability_base == 0
        || mul_div_floor(env, collateral_base, SCALAR_7, liability_base) >= MIN_HEALTH_FACTOR;
    PositionHealth {
        collateral_base,
        healthy,
        liability_base,
        raw_collateral_base,
    }
}

fn add_position(env: &Env, map: &mut Map<u32, i128>, index: u32, amount: i128) {
    let previous = map.get(index).unwrap_or(0);
    map.set(index, checked_add(env, previous, amount));
}

fn remove_position(env: &Env, map: &mut Map<u32, i128>, index: u32, amount: i128) {
    let previous = map.get(index).unwrap_or(0);
    let next = checked_sub_nonnegative(env, previous, amount);
    if next == 0 {
        map.remove(index);
    } else {
        map.set(index, next);
    }
}

fn require_supply_cap(env: &Env, reserve: &Reserve) {
    if total_supply(env, reserve) > reserve.config.supply_cap {
        panic_with_error!(env, PoolError::SupplyCapExceeded);
    }
}

fn require_utilization_below_one(env: &Env, reserve: &Reserve) {
    if utilization(env, reserve) >= SCALAR_7 {
        panic_with_error!(env, PoolError::InvalidUtilization);
    }
}

fn utilization(env: &Env, reserve: &Reserve) -> i128 {
    let liabilities = total_liabilities(env, reserve);
    let supply = total_supply(env, reserve);
    if liabilities == 0 {
        0
    } else if liabilities >= supply {
        SCALAR_7
    } else {
        mul_div_ceil(env, liabilities, SCALAR_7, supply)
    }
}

fn total_supply(env: &Env, reserve: &Reserve) -> i128 {
    to_asset_from_b_tokens(env, reserve, reserve.data.b_supply)
}

fn total_liabilities(env: &Env, reserve: &Reserve) -> i128 {
    to_asset_from_d_tokens(env, reserve, reserve.data.d_supply)
}

fn to_asset_from_b_tokens(env: &Env, reserve: &Reserve, amount: i128) -> i128 {
    mul_div_floor(env, amount, reserve.data.b_rate, SCALAR_12)
}

fn to_asset_from_d_tokens(env: &Env, reserve: &Reserve, amount: i128) -> i128 {
    mul_div_ceil(env, amount, reserve.data.d_rate, SCALAR_12)
}

fn to_b_tokens_down(env: &Env, reserve: &Reserve, amount: i128) -> i128 {
    mul_div_floor(env, amount, SCALAR_12, reserve.data.b_rate)
}

fn to_b_tokens_up(env: &Env, reserve: &Reserve, amount: i128) -> i128 {
    mul_div_ceil(env, amount, SCALAR_12, reserve.data.b_rate)
}

fn to_d_tokens_down(env: &Env, reserve: &Reserve, amount: i128) -> i128 {
    mul_div_floor(env, amount, SCALAR_12, reserve.data.d_rate)
}

fn to_d_tokens_up(env: &Env, reserve: &Reserve, amount: i128) -> i128 {
    mul_div_ceil(env, amount, SCALAR_12, reserve.data.d_rate)
}

fn checked_add(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_add(right)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

fn checked_sub_nonnegative(env: &Env, left: i128, right: i128) -> i128 {
    let result = left
        .checked_sub(right)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    if result < 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    result
}

fn require_positive(env: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, PoolError::BadRequest);
    }
}

fn pow10(env: &Env, decimals: u32) -> i128 {
    10_i128
        .checked_pow(decimals)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

fn mul_div_floor(env: &Env, left: i128, right: i128, denominator: i128) -> i128 {
    if left < 0 || right < 0 || denominator <= 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    I256::from_i128(env, left)
        .mul(&I256::from_i128(env, right))
        .div(&I256::from_i128(env, denominator))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

fn mul_div_ceil(env: &Env, left: i128, right: i128, denominator: i128) -> i128 {
    if left < 0 || right < 0 || denominator <= 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    let denominator_i256 = I256::from_i128(env, denominator);
    I256::from_i128(env, left)
        .mul(&I256::from_i128(env, right))
        .add(&denominator_i256)
        .sub(&I256::from_i32(env, 1))
        .div(&denominator_i256)
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

fn mul3_div_floor(env: &Env, first: i128, second: i128, third: i128, denominator: i128) -> i128 {
    if first < 0 || second < 0 || third < 0 || denominator <= 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    I256::from_i128(env, first)
        .mul(&I256::from_i128(env, second))
        .mul(&I256::from_i128(env, third))
        .div(&I256::from_i128(env, denominator))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::{storage::Persistent as _, Address as _, Ledger as _};

    fn interest_config(env: &Env) -> ReserveConfig {
        ReserveConfig {
            asset: Address::generate(env),
            c_factor: 7_500_000,
            decimals: 7,
            enabled: true,
            index: 0,
            l_factor: 7_500_000,
            max_util: 9_500_000,
            r_base: 100_000,
            r_one: 500_000,
            r_three: 15_000_000,
            r_two: 5_000_000,
            reactivity: 20,
            supply_cap: i128::MAX,
            util: 7_500_000,
        }
    }

    #[test]
    fn persistent_reserve_and_user_entries_refresh_their_documented_ttls() {
        let env = Env::default();
        let user = Address::generate(&env);
        let reserve = interest_config(&env);
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 2,
            min_collateral: 0,
            oracle: Address::generate(&env),
            price_decimals: 7,
        };
        let reserves = soroban_sdk::vec![&env, reserve.clone()];
        let contract = env.register(
            crate::PoolContract,
            (
                &Address::generate(&env),
                &Address::generate(&env),
                &lending_config,
                &reserves,
            ),
        );
        let reserve_key = LendingPersistentKey::ReserveData(reserve.asset.clone());
        let position_key = LendingPersistentKey::Positions(user.clone());

        env.as_contract(&contract, || {
            set_positions(&env, &user, &Positions::empty(&env));

            assert_eq!(
                env.storage().persistent().get_ttl(&reserve_key),
                SHARED_TTL_BUMP
            );
            assert_eq!(
                env.storage().persistent().get_ttl(&position_key),
                USER_TTL_BUMP
            );
        });

        let two_days = 2 * 17_280;
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + two_days);

        env.as_contract(&contract, || {
            assert_eq!(
                env.storage().persistent().get_ttl(&reserve_key),
                SHARED_TTL_BUMP - two_days
            );
            assert_eq!(
                env.storage().persistent().get_ttl(&position_key),
                USER_TTL_BUMP - two_days
            );

            reserve_data(&env, &reserve.asset);
            positions(&env, &user);

            assert_eq!(
                env.storage().persistent().get_ttl(&reserve_key),
                SHARED_TTL_BUMP
            );
            assert_eq!(
                env.storage().persistent().get_ttl(&position_key),
                USER_TTL_BUMP
            );
        });
    }

    #[test]
    fn interest_curve_matches_v2_reference_vectors() {
        let env = Env::default();
        let config = interest_config(&env);

        assert_eq!(
            calculate_accrual(&env, &config, 6_565_656, SCALAR_7, 0, 500),
            (1_000_000_852_536, 9_999_066)
        );
        assert_eq!(
            calculate_accrual(&env, &config, 7_979_797, SCALAR_7, 0, 500),
            (1_000_002_853_078, 10_000_479)
        );
        assert_eq!(
            calculate_accrual(&env, &config, 9_696_969, SCALAR_7, 0, 500),
            (1_000_018_247_510, 10_002_196)
        );
    }

    #[test]
    fn i256_math_handles_large_intermediates_and_rejects_large_results() {
        let env = Env::default();
        assert_eq!(
            mul_div_floor(&env, i128::MAX, i128::MAX, i128::MAX),
            i128::MAX
        );
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mul_div_floor(&env, i128::MAX, i128::MAX, 1);
        }))
        .is_err());
    }
}
