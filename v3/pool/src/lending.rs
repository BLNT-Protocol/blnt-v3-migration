use soroban_sdk::{
    contractclient, contractevent, contracttype, panic_with_error, token, Address, Env, Map, Vec,
    I256,
};

use crate::{
    auction::{self, ACTION_DELETE_LIQUIDATION, ACTION_FILL_USER_LIQUIDATION},
    emissions, get_backstop, get_status, require_action_allowed, set_backstop_liability, PoolError,
};

pub const SCALAR_7: i128 = 10_000_000;
pub const SCALAR_12: i128 = 1_000_000_000_000;
const SECONDS_PER_YEAR: i128 = 31_536_000;
pub(crate) const SECONDS_PER_WEEK: u64 = 604_800;
const MAX_RESERVES: u32 = 30;
const MAX_POSITIONS: u32 = MAX_RESERVES * 2;
pub(crate) const MAX_REQUESTS: u32 = MAX_POSITIONS;
const MIN_HEALTH_FACTOR: i128 = 10_000_100;
const RESERVE_QUEUE_TTL_THRESHOLD: u32 = 100 * 17_280;
const RESERVE_QUEUE_TTL_BUMP: u32 = 120 * 17_280;
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

/// Mutable reserve metadata accepted by the timelocked configuration queue.
///
/// The asset is supplied separately and the immutable reserve index is
/// assigned when a new reserve is applied or preserved when one is updated.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReserveConfigUpdate {
    pub c_factor: u32,
    pub decimals: u32,
    pub enabled: bool,
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
    pub(crate) fn empty(env: &Env) -> Self {
        Self {
            collateral: Map::new(env),
            liabilities: Map::new(env),
            supply: Map::new(env),
        }
    }

    pub(crate) fn effective_count(&self) -> u32 {
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
pub struct FlashLoan {
    pub contract: Address,
    pub asset: Address,
    pub amount: i128,
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

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
struct QueuedReserveConfig {
    update: ReserveConfigUpdate,
    unlock_time: u64,
}

pub(crate) struct PositionData {
    pub collateral_base: i128,
    pub liability_base: i128,
    pub raw_collateral_base: i128,
    pub raw_liability_base: i128,
    pub scalar: i128,
}

impl PositionData {
    pub(crate) fn health_factor(&self, env: &Env) -> i128 {
        if self.liability_base == 0 {
            return i128::MAX;
        }
        mul_div_floor(env, self.collateral_base, self.scalar, self.liability_base)
    }

    pub(crate) fn is_hf_lower_than(&self, env: &Env, other: &Self) -> bool {
        if self.liability_base == 0 {
            return false;
        }
        if other.liability_base == 0 {
            return true;
        }
        I256::from_i128(env, self.collateral_base).mul(&I256::from_i128(env, other.liability_base))
            < I256::from_i128(env, other.collateral_base)
                .mul(&I256::from_i128(env, self.liability_base))
    }

    pub(crate) fn is_hf_over(&self, env: &Env, maximum: i128) -> bool {
        if self.liability_base == 0 {
            return true;
        }
        let threshold = mul_div_ceil(env, self.scalar, maximum, SCALAR_7);
        self.health_factor(env) > threshold
    }

    pub(crate) fn is_hf_under(&self, env: &Env, minimum: i128) -> bool {
        if self.liability_base == 0 {
            return false;
        }
        let threshold = mul_div_floor(env, self.scalar, minimum, SCALAR_7);
        self.health_factor(env) < threshold
    }
}

#[contractclient(name = "PriceOracleClient")]
#[allow(dead_code)]
pub trait PriceOracle {
    fn price(env: Env, asset: Address) -> PriceQuote;
}

#[contractclient(name = "FlashLoanReceiverClient")]
#[allow(dead_code)]
pub trait FlashLoanReceiver {
    fn exec_op(env: Env, caller: Address, token: Address, amount: i128, fee: i128);
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

#[contractevent(topics = ["flash_loan"], data_format = "vec")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashLoanExecuted {
    #[topic]
    pub asset: Address,
    #[topic]
    pub owner: Address,
    #[topic]
    pub receiver: Address,
    pub amount: i128,
    pub position_delta: i128,
}

#[derive(Clone)]
#[contracttype]
pub(crate) enum LendingDataKey {
    Config,
    QueuedReserve(Address),
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

#[derive(Copy, Clone)]
enum TransferMode {
    Allowance,
    Direct,
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

pub(crate) fn update_config(
    env: &Env,
    backstop_take_rate: u32,
    max_positions: u32,
    min_collateral: i128,
) -> LendingConfig {
    let mut config = config(env);
    if config.backstop_take_rate != backstop_take_rate {
        for asset in reserve_list(env) {
            let reserve = accrue_reserve(env, &asset);
            set_reserve_data(env, &asset, &reserve.data);
        }
    }
    config.backstop_take_rate = backstop_take_rate;
    config.max_positions = max_positions;
    config.min_collateral = min_collateral;
    validate_lending_config(env, &config);
    env.storage()
        .instance()
        .set(&LendingDataKey::Config, &config);
    config
}

pub(crate) fn reserve_list(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&LendingDataKey::ReserveList)
        .unwrap()
}

pub(crate) fn queue_set_reserve(env: &Env, asset: &Address, update: &ReserveConfigUpdate) -> u64 {
    let key = LendingDataKey::QueuedReserve(asset.clone());
    if env.storage().temporary().has(&key) {
        panic_with_error!(env, PoolError::BadRequest);
    }
    validate_reserve_config_update(env, update);
    let assets = reserve_list(env);
    if assets.contains(asset.clone()) {
        validate_reserve_config_change(env, &reserve_config(env, asset), update);
    } else if assets.len() >= MAX_RESERVES {
        panic_with_error!(env, PoolError::InvalidReserveConfig);
    }

    let unlock_time = if crate::get_status(env) == crate::STATUS_SETUP {
        env.ledger().timestamp()
    } else {
        env.ledger()
            .timestamp()
            .checked_add(SECONDS_PER_WEEK)
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
    };
    env.storage().temporary().set(
        &key,
        &QueuedReserveConfig {
            update: update.clone(),
            unlock_time,
        },
    );
    env.storage()
        .temporary()
        .extend_ttl(&key, RESERVE_QUEUE_TTL_THRESHOLD, RESERVE_QUEUE_TTL_BUMP);
    unlock_time
}

pub(crate) fn cancel_set_reserve(env: &Env, asset: &Address) {
    let key = LendingDataKey::QueuedReserve(asset.clone());
    env.storage().temporary().remove(&key);
}

pub(crate) fn set_reserve(env: &Env, asset: &Address) -> u32 {
    let key = LendingDataKey::QueuedReserve(asset.clone());
    let queued: QueuedReserveConfig = env
        .storage()
        .temporary()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::BadRequest));
    if queued.unlock_time > env.ledger().timestamp() {
        panic_with_error!(env, PoolError::BadRequest);
    }

    let mut assets = reserve_list(env);
    let index = if assets.contains(asset.clone()) {
        let current = reserve_config(env, asset);
        validate_reserve_config_change(env, &current, &queued.update);
        emissions::checkpoint_reserve(env, current.index);
        let mut reserve = accrue_reserve(env, asset);
        if current.r_base != queued.update.r_base
            || current.r_one != queued.update.r_one
            || current.r_two != queued.update.r_two
            || current.r_three != queued.update.r_three
            || current.util != queued.update.util
        {
            reserve.data.ir_mod = SCALAR_7;
        }
        set_reserve_data(env, asset, &reserve.data);
        current.index
    } else {
        if assets.len() >= MAX_RESERVES {
            panic_with_error!(env, PoolError::InvalidReserveConfig);
        }
        // Matching v2, a new reserve receives the next index when its queued
        // configuration is applied. This permits multiple new reserves to be
        // queued concurrently without making later queues stale.
        let index = assets.len();
        assets.push_back(asset.clone());
        env.storage()
            .instance()
            .set(&LendingDataKey::ReserveList, &assets);
        set_reserve_data(
            env,
            asset,
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
        index
    };

    let config = reserve_config_from_update(asset, index, &queued.update);
    env.storage()
        .instance()
        .set(&LendingDataKey::ReserveConfig(asset.clone()), &config);
    env.storage().temporary().remove(&key);
    index
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
    execute_submit(env, from, spender, to, requests, TransferMode::Direct, None)
}

pub(crate) fn submit_with_allowance(
    env: &Env,
    from: &Address,
    spender: &Address,
    to: &Address,
    requests: Vec<Request>,
) -> Positions {
    execute_submit(
        env,
        from,
        spender,
        to,
        requests,
        TransferMode::Allowance,
        None,
    )
}

pub(crate) fn flash_loan(
    env: &Env,
    from: &Address,
    flash_loan: FlashLoan,
    requests: Vec<Request>,
) -> Positions {
    execute_submit(
        env,
        from,
        from,
        from,
        requests,
        TransferMode::Allowance,
        Some(flash_loan),
    )
}

fn execute_submit(
    env: &Env,
    from: &Address,
    spender: &Address,
    to: &Address,
    requests: Vec<Request>,
    transfer_mode: TransferMode,
    flash_loan: Option<FlashLoan>,
) -> Positions {
    let pool = env.current_contract_address();
    if from == &pool || spender == &pool || to == &pool || requests.len() > MAX_REQUESTS {
        panic_with_error!(env, PoolError::BadRequest);
    }
    spender.require_auth();
    if from != spender {
        from.require_auth();
    }

    let mut user = positions(env, from);
    let previous_position_count = user.effective_count();
    let mut transfers = Transfers::new(env);
    let mut health_check = false;
    let mut liability_checks: Map<Address, bool> = Map::new(env);
    let mut prices: Map<Address, PriceQuote> = Map::new(env);
    let mut utilization_checks: Map<Address, bool> = Map::new(env);

    // Match v2 by creating the flash-borrow liability before executing the
    // accompanying request set. The request set may repay it immediately or
    // leave a healthy collateralized liability.
    if let Some(flash) = flash_loan.as_ref() {
        if flash.amount <= 0 {
            panic_with_error!(env, PoolError::BadRequest);
        }
        require_action_allowed(env, get_status(env), ACTION_BORROW);
        let mut reserve = accrue_reserve(env, &flash.asset);
        if !reserve.config.enabled || reserve.config.l_factor == 0 {
            panic_with_error!(env, PoolError::ReserveDisabled);
        }
        emissions::checkpoint_before_lending_change(
            env,
            from,
            &user,
            &reserve,
            reserve
                .config
                .index
                .checked_mul(2)
                .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError)),
        );
        let minted = to_d_tokens_up(env, &reserve, flash.amount);
        require_positive(env, minted);
        add_position(env, &mut user.liabilities, reserve.config.index, minted);
        reserve.data.d_supply = checked_add(env, reserve.data.d_supply, minted);
        require_utilization_below_one(env, &reserve);
        set_reserve_data(env, &reserve.config.asset, &reserve.data);
        liability_checks.set(reserve.config.asset.clone(), true);
        utilization_checks.set(reserve.config.asset.clone(), true);
        health_check = true;
        FlashLoanExecuted {
            asset: flash.asset.clone(),
            owner: from.clone(),
            receiver: flash.contract.clone(),
            amount: flash.amount,
            position_delta: minted,
        }
        .publish(env);
    }

    for request in requests {
        if request.amount < 0
            || (request.amount == 0 && request.request_type != ACTION_DELETE_LIQUIDATION)
        {
            panic_with_error!(env, PoolError::BadRequest);
        }
        require_action_allowed(env, get_status(env), request.request_type);
        if request.request_type == ACTION_FILL_USER_LIQUIDATION {
            let filled = auction::fill_user_liquidation(
                env,
                &request.address,
                from,
                &mut user,
                &mut prices,
                request.amount,
            );
            for (asset, _) in filled.bid {
                liability_checks.set(asset, true);
            }
            health_check = true;
            LendingAction {
                action: request.request_type,
                amount: request.amount,
                asset: request.address,
                owner: from.clone(),
                position_delta: 0,
            }
            .publish(env);
            continue;
        }
        if request.request_type == ACTION_DELETE_LIQUIDATION {
            auction::cancel_user_liquidation(env, from);
            health_check = true;
            LendingAction {
                action: request.request_type,
                amount: 0,
                asset: request.address,
                owner: from.clone(),
                position_delta: 0,
            }
            .publish(env);
            continue;
        }
        if request.request_type > ACTION_REPAY {
            panic_with_error!(env, PoolError::InvalidAction);
        }
        let mut reserve = accrue_reserve(env, &request.address);
        if !reserve.config.enabled
            && (request.request_type == ACTION_SUPPLY
                || request.request_type == ACTION_SUPPLY_COLLATERAL
                || request.request_type == ACTION_BORROW)
        {
            panic_with_error!(env, PoolError::ReserveDisabled);
        }
        let reserve_token_id = reserve
            .config
            .index
            .checked_mul(2)
            .and_then(|value| {
                value.checked_add(
                    if request.request_type == ACTION_BORROW || request.request_type == ACTION_REPAY
                    {
                        0
                    } else {
                        1
                    },
                )
            })
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
        emissions::checkpoint_before_lending_change(env, from, &user, &reserve, reserve_token_id);

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
                let requested_burn = to_d_tokens_down(env, &reserve, request.amount);
                let (burn, repaid) = if requested_burn > balance {
                    let debt = to_asset_from_d_tokens(env, &reserve, balance);
                    transfers.add_outgoing(
                        env,
                        &reserve.config.asset,
                        checked_sub_nonnegative(env, request.amount, debt),
                    );
                    (balance, debt)
                } else {
                    (requested_burn, request.amount)
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

    if auction::has_user_liquidation(env, from) {
        panic_with_error!(env, PoolError::AuctionInProgress);
    }
    require_max_positions(env, &user, previous_position_count);
    for (asset, _) in utilization_checks {
        let reserve = accrue_reserve(env, &asset);
        if utilization(env, &reserve) > i128::from(reserve.config.max_util) {
            panic_with_error!(env, PoolError::InvalidUtilization);
        }
    }
    require_health_if(env, &user, &mut prices, health_check);
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
    if let Some(flash) = flash_loan.as_ref() {
        if !balances_before.contains_key(flash.asset.clone()) {
            balances_before.set(
                flash.asset.clone(),
                token::Client::new(env, &flash.asset).balance(&pool),
            );
        }
        token::Client::new(env, &flash.asset).transfer(&pool, &flash.contract, &flash.amount);
        FlashLoanReceiverClient::new(env, &flash.contract).exec_op(
            from,
            &flash.asset,
            &flash.amount,
            &0,
        );
    }
    match transfer_mode {
        TransferMode::Direct => {
            for (asset, amount) in transfers.incoming.iter() {
                token::Client::new(env, &asset).transfer(spender, &pool, &amount);
            }
            for (asset, amount) in transfers.outgoing.iter() {
                if amount > 0 {
                    token::Client::new(env, &asset).transfer(&pool, to, &amount);
                }
            }
        }
        TransferMode::Allowance => {
            for (asset, incoming) in transfers.incoming.iter() {
                let outgoing = transfers.outgoing.get(asset.clone()).unwrap_or(0);
                if incoming > outgoing {
                    token::Client::new(env, &asset).transfer_from(
                        &pool,
                        spender,
                        &pool,
                        &checked_sub_nonnegative(env, incoming, outgoing),
                    );
                }
            }
            for (asset, outgoing) in transfers.outgoing.iter() {
                let incoming = transfers.incoming.get(asset.clone()).unwrap_or(0);
                if outgoing > incoming {
                    token::Client::new(env, &asset).transfer(
                        &pool,
                        to,
                        &checked_sub_nonnegative(env, outgoing, incoming),
                    );
                }
            }
        }
    }
    for (asset, balance_before) in balances_before {
        let after_requests = checked_sub_nonnegative(
            env,
            checked_add(
                env,
                balance_before,
                transfers.incoming.get(asset.clone()).unwrap_or(0),
            ),
            transfers.outgoing.get(asset.clone()).unwrap_or(0),
        );
        let flash_amount = flash_loan
            .as_ref()
            .filter(|flash| flash.asset == asset)
            .map(|flash| flash.amount)
            .unwrap_or(0);
        let expected = checked_sub_nonnegative(env, after_requests, flash_amount);
        if token::Client::new(env, &asset).balance(&pool) != expected {
            panic_with_error!(env, PoolError::InvalidTokenBalance);
        }
    }
    set_positions_after_emission_checkpoint(env, from, &user);
    user
}

pub(crate) fn require_position_limits(
    env: &Env,
    positions: &Positions,
    previous_position_count: u32,
    prices: &mut Map<Address, PriceQuote>,
) {
    require_max_positions(env, positions, previous_position_count);
    require_health_if(env, positions, prices, true);
}

fn require_max_positions(env: &Env, positions: &Positions, previous_position_count: u32) {
    let lending_config = config(env);
    if positions.effective_count() > lending_config.max_positions
        && positions.effective_count() > previous_position_count
    {
        panic_with_error!(env, PoolError::TooManyPositions);
    }
}

fn require_health_if(
    env: &Env,
    positions: &Positions,
    prices: &mut Map<Address, PriceQuote>,
    health_check: bool,
) {
    if health_check && !positions.liabilities.is_empty() {
        let lending_config = config(env);
        let result = calculate_health_with_prices(env, positions, prices);
        if !result.healthy {
            panic_with_error!(env, PoolError::InvalidHealthFactor);
        }
        if result.collateral_base < lending_config.min_collateral {
            panic_with_error!(env, PoolError::MinimumCollateralNotMet);
        }
    }
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
    validate_reserve_config_update(env, &reserve_config_to_update(config));
}

fn validate_reserve_config_update(env: &Env, update: &ReserveConfigUpdate) {
    if update.decimals > 18
        || update.c_factor > SCALAR_7 as u32
        || update.l_factor > SCALAR_7 as u32
        || update.util == 0
        || update.util > 9_000_000
        || update.max_util > SCALAR_7 as u32
        || update.max_util <= update.util
        || update.r_base >= SCALAR_7 as u32
        || update.r_base < 1_000
        || update.r_one > update.r_two
        || update.r_two > update.r_three
        || update.reactivity > 1_000
        || update.supply_cap < 0
    {
        panic_with_error!(env, PoolError::InvalidReserveConfig);
    }
}

fn validate_reserve_config_change(
    env: &Env,
    current: &ReserveConfig,
    update: &ReserveConfigUpdate,
) {
    if current.decimals != update.decimals || (current.l_factor != 0 && update.l_factor == 0) {
        panic_with_error!(env, PoolError::InvalidReserveConfig);
    }
}

fn reserve_config_to_update(config: &ReserveConfig) -> ReserveConfigUpdate {
    ReserveConfigUpdate {
        c_factor: config.c_factor,
        decimals: config.decimals,
        enabled: config.enabled,
        l_factor: config.l_factor,
        max_util: config.max_util,
        r_base: config.r_base,
        r_one: config.r_one,
        r_three: config.r_three,
        r_two: config.r_two,
        reactivity: config.reactivity,
        supply_cap: config.supply_cap,
        util: config.util,
    }
}

fn reserve_config_from_update(
    asset: &Address,
    index: u32,
    update: &ReserveConfigUpdate,
) -> ReserveConfig {
    ReserveConfig {
        asset: asset.clone(),
        c_factor: update.c_factor,
        decimals: update.decimals,
        enabled: update.enabled,
        index,
        l_factor: update.l_factor,
        max_util: update.max_util,
        r_base: update.r_base,
        r_one: update.r_one,
        r_three: update.r_three,
        r_two: update.r_two,
        reactivity: update.reactivity,
        supply_cap: update.supply_cap,
        util: update.util,
    }
}

pub(crate) fn set_positions(env: &Env, user: &Address, positions: &Positions) {
    let previous = self::positions(env, user);
    emissions::checkpoint_position_changes(env, user, &previous, positions);
    set_positions_after_emission_checkpoint(env, user, positions);
}

pub(crate) fn set_positions_after_emission_checkpoint(
    env: &Env,
    user: &Address,
    positions: &Positions,
) {
    let key = LendingPersistentKey::Positions(user.clone());
    env.storage().persistent().set(&key, positions);
    env.storage()
        .persistent()
        .extend_ttl(&key, USER_TTL_THRESHOLD, USER_TTL_BUMP);
}

/// Remove one account's entire liability in a reserve and pass the equivalent
/// asset loss to that reserve's suppliers through its bToken exchange rate.
///
/// The rounding and zero-rate saturation intentionally match Blend v2's
/// `default_liabilities` behavior. The caller is responsible for persisting
/// `positions` after all atomic defaults have been applied.
pub(crate) fn default_liability_to_suppliers(
    env: &Env,
    owner: &Address,
    positions: &mut Positions,
    asset: &Address,
    amount: i128,
) -> (i128, i128, i128) {
    if amount <= 0 {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }

    let mut reserve = accrue_reserve(env, asset);
    if positions.liabilities.get(reserve.config.index).unwrap_or(0) != amount
        || reserve.data.b_supply <= 0
    {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }
    let reserve_token_id = reserve
        .config
        .index
        .checked_mul(2)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    emissions::checkpoint_before_lending_change(env, owner, positions, &reserve, reserve_token_id);

    let default_amount = to_asset_from_d_tokens(env, &reserve, amount);
    let previous_b_rate = reserve.data.b_rate;
    let b_rate_loss = mul_div_ceil(env, default_amount, SCALAR_12, reserve.data.b_supply);

    remove_position(
        env,
        &mut positions.liabilities,
        reserve.config.index,
        amount,
    );
    reserve.data.d_supply = checked_sub_nonnegative(env, reserve.data.d_supply, amount);
    reserve.data.b_rate = if b_rate_loss >= previous_b_rate {
        0
    } else {
        checked_sub_nonnegative(env, previous_b_rate, b_rate_loss)
    };
    set_reserve_data(env, asset, &reserve.data);

    (default_amount, previous_b_rate, reserve.data.b_rate)
}

pub(crate) fn reserve_config(env: &Env, asset: &Address) -> ReserveConfig {
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

#[cfg(test)]
pub(crate) fn set_backstop_credit(env: &Env, asset: &Address, amount: i128) {
    let mut data = reserve_data(env, asset);
    data.backstop_credit = amount;
    set_reserve_data(env, asset, &data);
}

pub(crate) fn checkpoint_reserve(env: &Env, asset: &Address) -> Reserve {
    let reserve = accrue_reserve(env, asset);
    set_reserve_data(env, asset, &reserve.data);
    reserve
}

pub(crate) fn consume_backstop_credit(env: &Env, asset: &Address, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    let mut reserve = accrue_reserve(env, asset);
    reserve.data.backstop_credit =
        checked_sub_nonnegative(env, reserve.data.backstop_credit, amount);
    set_reserve_data(env, asset, &reserve.data);
}

/// Value three amounts of one reserve in 7-decimal USDC with one oracle read.
pub(crate) fn value_reserve_amounts(
    env: &Env,
    asset: &Address,
    first: i128,
    second: i128,
    third: i128,
) -> (i128, i128, i128, u64) {
    if first < 0 || second < 0 || third < 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    let reserve = reserve_config(env, asset);
    let lending_config = config(env);
    let quote = PriceOracleClient::new(env, &lending_config.oracle).price(asset);
    if quote.price <= 0
        || quote.decimals != lending_config.price_decimals
        || quote.valid_until < env.ledger().timestamp()
    {
        panic_with_error!(env, PoolError::InvalidPrice);
    }
    let asset_scalar = pow10(env, reserve.decimals);
    let oracle_scalar = pow10(env, lending_config.price_decimals);
    let value = |amount| {
        let quoted = mul_div_floor(env, quote.price, amount, asset_scalar);
        mul_div_floor(env, quoted, SCALAR_7, oracle_scalar)
    };
    (value(first), value(second), value(third), quote.valid_until)
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
    let mut prices = Map::new(env);
    calculate_health_with_prices(env, positions, &mut prices)
}

fn calculate_health_with_prices(
    env: &Env,
    positions: &Positions,
    prices: &mut Map<Address, PriceQuote>,
) -> PositionHealth {
    let data = position_data_with_prices(env, positions, prices);
    PositionHealth {
        collateral_base: data.collateral_base,
        healthy: !data.is_hf_under(env, MIN_HEALTH_FACTOR),
        liability_base: data.liability_base,
        raw_collateral_base: data.raw_collateral_base,
    }
}

pub(crate) fn position_data_with_prices(
    env: &Env,
    positions: &Positions,
    prices: &mut Map<Address, PriceQuote>,
) -> PositionData {
    let lending_config = config(env);
    let mut collateral_base = 0_i128;
    let mut raw_collateral_base = 0_i128;
    let mut liability_base = 0_i128;
    let mut raw_liability_base = 0_i128;
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
        let quote = if let Some(quote) = prices.get(asset.clone()) {
            quote
        } else {
            let quote = PriceOracleClient::new(env, &lending_config.oracle).price(&asset);
            if quote.price <= 0
                || quote.decimals != lending_config.price_decimals
                || quote.valid_until < env.ledger().timestamp()
            {
                panic_with_error!(env, PoolError::InvalidPrice);
            }
            prices.set(asset.clone(), quote.clone());
            quote
        };
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
            raw_liability_base = checked_add(
                env,
                raw_liability_base,
                mul_div_ceil(env, quote.price, assets, asset_scalar),
            );
            let effective_assets =
                mul_div_ceil(env, assets, SCALAR_7, i128::from(reserve.config.l_factor));
            liability_base = checked_add(
                env,
                liability_base,
                mul_div_ceil(env, quote.price, effective_assets, asset_scalar),
            );
        }
    }
    PositionData {
        collateral_base,
        liability_base,
        raw_collateral_base,
        raw_liability_base,
        scalar: pow10(env, lending_config.price_decimals),
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

pub(crate) fn checked_add(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_add(right)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

pub(crate) fn checked_sub_nonnegative(env: &Env, left: i128, right: i128) -> i128 {
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

pub(crate) fn pow10(env: &Env, decimals: u32) -> i128 {
    10_i128
        .checked_pow(decimals)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

pub(crate) fn mul_div_floor(env: &Env, left: i128, right: i128, denominator: i128) -> i128 {
    if left < 0 || right < 0 || denominator <= 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    I256::from_i128(env, left)
        .mul(&I256::from_i128(env, right))
        .div(&I256::from_i128(env, denominator))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

pub(crate) fn mul_div_ceil(env: &Env, left: i128, right: i128, denominator: i128) -> i128 {
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
    #[should_panic(expected = "Error(Contract, #1306)")]
    fn reserve_config_update_rejects_invalid_metadata() {
        let env = Env::default();
        let mut update = reserve_config_to_update(&interest_config(&env));
        update.supply_cap = -1;
        validate_reserve_config_update(&env, &update);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1306)")]
    fn reserve_config_update_rejects_decimal_change() {
        let env = Env::default();
        let current = interest_config(&env);
        let mut update = reserve_config_to_update(&current);
        update.decimals += 1;
        validate_reserve_config_change(&env, &current, &update);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1306)")]
    fn reserve_config_update_rejects_liability_factor_removal() {
        let env = Env::default();
        let current = interest_config(&env);
        let mut update = reserve_config_to_update(&current);
        update.l_factor = 0;
        validate_reserve_config_change(&env, &current, &update);
    }

    #[test]
    fn reserve_config_update_preserves_index_and_resets_changed_curve() {
        let env = Env::default();
        let initial_config = interest_config(&env);
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 2,
            min_collateral: 0,
            oracle: Address::generate(&env),
            price_decimals: 7,
        };
        let contract = env.register(
            crate::PoolContract,
            (
                &Address::generate(&env),
                &Address::generate(&env),
                &lending_config,
                &soroban_sdk::vec![&env, initial_config.clone()],
            ),
        );
        env.as_contract(&contract, || {
            let modified_ir_mod = SCALAR_7 + 123;
            let mut data = reserve_data(&env, &initial_config.asset);
            data.ir_mod = modified_ir_mod;
            set_reserve_data(&env, &initial_config.asset, &data);

            let mut non_curve_update = reserve_config_to_update(&initial_config);
            non_curve_update.supply_cap -= 1;
            queue_set_reserve(&env, &initial_config.asset, &non_curve_update);
            assert_eq!(
                set_reserve(&env, &initial_config.asset),
                initial_config.index
            );
            let after_non_curve = reserve(&env, &initial_config.asset);
            assert_eq!(after_non_curve.config.index, initial_config.index);
            assert_eq!(after_non_curve.data.ir_mod, modified_ir_mod);

            let mut curve_update = reserve_config_to_update(&after_non_curve.config);
            curve_update.r_one += 1;
            queue_set_reserve(&env, &initial_config.asset, &curve_update);
            assert_eq!(
                set_reserve(&env, &initial_config.asset),
                initial_config.index
            );
            let after_curve = reserve(&env, &initial_config.asset);
            assert_eq!(after_curve.config.index, initial_config.index);
            assert_eq!(after_curve.config.r_one, curve_update.r_one);
            assert_eq!(after_curve.data.ir_mod, SCALAR_7);
        });
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
    fn supplier_default_matches_v2_ceiling_rounding_and_zero_saturation() {
        let env = Env::default();
        let reserve = interest_config(&env);
        let owner = Address::generate(&env);
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 2,
            min_collateral: 0,
            oracle: Address::generate(&env),
            price_decimals: 7,
        };
        let contract = env.register(
            crate::PoolContract,
            (
                &Address::generate(&env),
                &Address::generate(&env),
                &lending_config,
                &soroban_sdk::vec![&env, reserve.clone()],
            ),
        );

        env.as_contract(&contract, || {
            set_reserve_data(
                &env,
                &reserve.asset,
                &ReserveData {
                    b_rate: 2 * SCALAR_12,
                    b_supply: 3,
                    backstop_credit: 0,
                    d_rate: 1_500_000_000_001,
                    d_supply: 2,
                    ir_mod: SCALAR_7,
                    last_time: env.ledger().timestamp(),
                },
            );
            let mut positions = Positions::empty(&env);
            positions.liabilities.set(0, 2);

            let result =
                default_liability_to_suppliers(&env, &owner, &mut positions, &reserve.asset, 2);

            assert_eq!(result, (4, 2 * SCALAR_12, 666_666_666_666));
            assert!(positions.liabilities.is_empty());
            let data = reserve_data(&env, &reserve.asset);
            assert_eq!(data.d_supply, 0);
            assert_eq!(data.b_supply, 3);
            assert_eq!(data.b_rate, 666_666_666_666);

            set_reserve_data(
                &env,
                &reserve.asset,
                &ReserveData {
                    b_rate: SCALAR_12,
                    b_supply: 3,
                    backstop_credit: 0,
                    d_rate: 2 * SCALAR_12,
                    d_supply: 4,
                    ir_mod: SCALAR_7,
                    last_time: env.ledger().timestamp(),
                },
            );
            positions.liabilities.set(0, 4);

            let saturated =
                default_liability_to_suppliers(&env, &owner, &mut positions, &reserve.asset, 4);

            assert_eq!(saturated, (8, SCALAR_12, 0));
            assert!(positions.liabilities.is_empty());
            let saturated_data = reserve_data(&env, &reserve.asset);
            assert_eq!(saturated_data.d_supply, 0);
            assert_eq!(saturated_data.b_supply, 3);
            assert_eq!(saturated_data.b_rate, 0);
        });
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

    #[test]
    fn exact_health_comparison_does_not_hide_low_precision_deterioration() {
        let env = Env::default();
        let before = PositionData {
            collateral_base: 150,
            liability_base: 100,
            raw_collateral_base: 150,
            raw_liability_base: 100,
            scalar: 1,
        };
        let after = PositionData {
            collateral_base: 140,
            liability_base: 100,
            raw_collateral_base: 140,
            raw_liability_base: 100,
            scalar: 1,
        };

        assert_eq!(before.health_factor(&env), after.health_factor(&env));
        assert!(after.is_hf_lower_than(&env, &before));
        assert!(!before.is_hf_lower_than(&env, &after));
    }
}
