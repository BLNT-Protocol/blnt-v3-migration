use soroban_sdk::{contractevent, contracttype, panic_with_error, Address, Env, Map, Vec, I256};

use crate::{
    get_admin, get_backstop,
    lending::{self, Positions, Reserve, SCALAR_7, SECONDS_PER_WEEK},
    BackstopClient, PoolError,
};

const MAX_RESERVE_TOKEN_IDS: u32 = 60;
const SHARED_TTL_THRESHOLD: u32 = 179 * 17_280;
const SHARED_TTL_BUMP: u32 = 180 * 17_280;
const USER_TTL_THRESHOLD: u32 = 359 * 17_280;
const USER_TTL_BUMP: u32 = 360 * 17_280;

#[derive(Clone)]
#[contracttype]
enum EmissionDataKey {
    PoolSupplyBorrowConfig,
    UnallocatedPoolEmissionCarry,
    CreatedReserveEmissionIds,
}

#[derive(Clone)]
#[contracttype]
enum EmissionPersistentKey {
    ReserveEmissionStates,
    UserEmissionCheckpoints(Address),
}

/// V2-compatible relative emission weight for one reserve token.
///
/// Even token identifiers are debt/dToken positions and odd identifiers are
/// supply/bToken positions: `reserve_index * 2 + reserve_type`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReserveEmissionMetadata {
    pub res_index: u32,
    pub res_type: u32,
    pub share: u64,
}

/// Seven-day emission stream and reserve-token index.
///
/// `carry` is whole BLND base-unit dust that could not yet be represented by
/// the scaled index. `remaining` is the unvested part of the current stream.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ReserveEmissionData {
    pub carry: i128,
    pub expiration: u64,
    pub index: i128,
    pub last_time: u64,
    pub remaining: i128,
}

/// One user's checkpoint for one reserve token.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct UserEmissionData {
    pub accrued: i128,
    pub carry: i128,
    pub index: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionsConfigured {
    pub admin: Address,
    pub count: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolEmissionsGulped {
    pub amount: i128,
    pub carry: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveEmissionUpdated {
    pub amount: i128,
    pub expiration: u64,
    pub reserve_token_id: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolEmissionsClaimed {
    pub amount: i128,
    pub recipient: Address,
    pub reserve_token_ids: Vec<u32>,
    pub user: Address,
}

pub(crate) fn set_config(env: &Env, metadata: Vec<ReserveEmissionMetadata>) {
    if metadata.is_empty() || metadata.len() > MAX_RESERVE_TOKEN_IDS {
        panic_with_error!(env, PoolError::InvalidEmissionConfig);
    }

    let mut next = Map::new(env);
    for entry in metadata {
        if entry.res_type > 1 || entry.share == 0 {
            panic_with_error!(env, PoolError::InvalidEmissionConfig);
        }
        let reserve_token_id = entry
            .res_index
            .checked_mul(2)
            .and_then(|value| value.checked_add(entry.res_type))
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
        require_reserve_token_id(env, reserve_token_id);
        if next.contains_key(reserve_token_id) {
            panic_with_error!(env, PoolError::InvalidEmissionConfig);
        }
        next.set(reserve_token_id, entry.share);
    }

    // Advancing every already-created stream freezes all prior accrual before
    // the mutable weights are replaced. New identifiers have no past stream.
    for reserve_token_id in reserve_token_ids(env) {
        checkpoint_reserve_by_id(env, reserve_token_id);
    }

    env.storage()
        .instance()
        .set(&EmissionDataKey::PoolSupplyBorrowConfig, &next);
    let admin = get_admin(env);
    EmissionsConfigured {
        admin,
        count: next.len(),
    }
    .publish(env);
}

pub(crate) fn config(env: &Env) -> Map<u32, u64> {
    env.storage()
        .instance()
        .get(&EmissionDataKey::PoolSupplyBorrowConfig)
        .unwrap_or_else(|| Map::new(env))
}

pub(crate) fn pool_carry(env: &Env) -> i128 {
    let carry = env
        .storage()
        .instance()
        .get(&EmissionDataKey::UnallocatedPoolEmissionCarry)
        .unwrap_or(0);
    if carry < 0 {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    carry
}

pub(crate) fn gulp(env: &Env) -> i128 {
    let configured = config(env);
    if configured.is_empty() {
        panic_with_error!(env, PoolError::InvalidEmissionConfig);
    }

    let mut enabled = Vec::new(env);
    let mut total_share = 0_i128;
    for (reserve_token_id, share) in configured.iter() {
        let reserve = reserve_for_token_id(env, reserve_token_id);
        if reserve.config.enabled {
            total_share = lending::checked_add(env, total_share, i128::from(share));
            enabled.push_back((reserve_token_id, share));
        }
    }
    if total_share <= 0 {
        panic_with_error!(env, PoolError::InvalidEmissionConfig);
    }

    let pool = env.current_contract_address();
    let amount = BackstopClient::new(env, &get_backstop(env)).gulp_pool_emissions(&pool);
    // Preserve v2's one-BLND lower bound. The failed cross-contract call rolls
    // the candidate reservation back, allowing the pool tranche to accumulate.
    if amount < SCALAR_7 {
        panic_with_error!(env, PoolError::InvalidEmissionAmount);
    }

    let prior_carry = pool_carry(env);
    let distribution = lending::checked_add(env, amount, prior_carry);
    let mut allocated = 0_i128;
    for (reserve_token_id, share) in enabled {
        let reserve_amount =
            lending::mul_div_floor(env, distribution, i128::from(share), total_share);
        add_reserve_stream(env, reserve_token_id, reserve_amount);
        allocated = lending::checked_add(env, allocated, reserve_amount);
    }
    let carry = lending::checked_sub_nonnegative(env, distribution, allocated);
    env.storage()
        .instance()
        .set(&EmissionDataKey::UnallocatedPoolEmissionCarry, &carry);
    PoolEmissionsGulped { amount, carry }.publish(env);
    amount
}

pub(crate) fn claim(
    env: &Env,
    user: &Address,
    reserve_token_ids: &Vec<u32>,
    recipient: &Address,
) -> i128 {
    user.require_auth();
    if reserve_token_ids.is_empty() || reserve_token_ids.len() > MAX_RESERVE_TOKEN_IDS {
        panic_with_error!(env, PoolError::InvalidEmissionConfig);
    }

    let positions = lending::positions(env, user);
    let mut seen = Vec::new(env);
    let mut amount = 0_i128;
    for reserve_token_id in reserve_token_ids.iter() {
        require_reserve_token_id(env, reserve_token_id);
        if seen.contains(reserve_token_id) {
            panic_with_error!(env, PoolError::InvalidEmissionConfig);
        }
        seen.push_back(reserve_token_id);
        let (mut user_data, index_scale) =
            checkpoint_user_by_id(env, user, &positions, reserve_token_id);
        amount = lending::checked_add(env, amount, user_data.accrued);
        user_data.accrued = 0;
        set_user_data(env, user, reserve_token_id, &user_data, index_scale);
    }

    if amount > 0 {
        BackstopClient::new(env, &get_backstop(env)).claim_pool_emissions(
            &env.current_contract_address(),
            recipient,
            &amount,
        );
    }
    PoolEmissionsClaimed {
        amount,
        recipient: recipient.clone(),
        reserve_token_ids: reserve_token_ids.clone(),
        user: user.clone(),
    }
    .publish(env);
    amount
}

pub(crate) fn reserve_emission(env: &Env, reserve_token_id: u32) -> Option<ReserveEmissionData> {
    require_reserve_token_id(env, reserve_token_id);
    let reserve = reserve_for_token_id(env, reserve_token_id);
    let index_scale = emission_index_scale(env, &reserve);
    get_reserve_data(env, reserve_token_id).map(|data| {
        advance_reserve_data(
            env,
            data,
            token_supply(&reserve, reserve_token_id),
            index_scale,
        )
    })
}

pub(crate) fn user_emission(env: &Env, user: &Address, reserve_token_id: u32) -> UserEmissionData {
    require_reserve_token_id(env, reserve_token_id);
    let positions = lending::positions(env, user);
    let reserve = reserve_for_token_id(env, reserve_token_id);
    let index_scale = emission_index_scale(env, &reserve);
    let reserve_data = get_reserve_data(env, reserve_token_id).map(|data| {
        advance_reserve_data(
            env,
            data,
            token_supply(&reserve, reserve_token_id),
            index_scale,
        )
    });
    let current_index = reserve_data.map(|data| data.index).unwrap_or(0);
    advance_user_data(
        env,
        get_user_data(env, user, reserve_token_id, index_scale),
        token_balance(env, &positions, reserve_token_id),
        current_index,
        index_scale,
    )
}

/// Checkpoint one user and reserve-token supply before a normal lending
/// request mutates either balance.
pub(crate) fn checkpoint_before_lending_change(
    env: &Env,
    user: &Address,
    positions: &Positions,
    reserve: &Reserve,
    reserve_token_id: u32,
) {
    // Gate the per-token persistent lookup behind the bounded instance list.
    // Pools with no configured stream therefore add one shared footprint
    // entry, not one absent persistent entry per reserve mutation.
    if !reserve_token_ids(env).contains(reserve_token_id) {
        return;
    }
    checkpoint_user_with_reserve(env, user, positions, reserve, reserve_token_id);
}

/// Checkpoint changed positions before a stored position map is replaced.
///
/// This covers liquidation transfers and bad-debt handoffs whose total token
/// supply does not change. Normal submit paths also call the narrower hook
/// above before changing reserve supply.
pub(crate) fn checkpoint_position_changes(
    env: &Env,
    user: &Address,
    previous: &Positions,
    next: &Positions,
) {
    for reserve_token_id in reserve_token_ids(env) {
        if token_balance(env, previous, reserve_token_id)
            != token_balance(env, next, reserve_token_id)
        {
            let reserve = reserve_for_token_id(env, reserve_token_id);
            checkpoint_user_with_reserve(env, user, previous, &reserve, reserve_token_id);
        }
    }
}

/// Checkpoint both reserve-token streams before mutable reserve metadata is
/// applied, freezing all accrual under the previous configuration.
pub(crate) fn checkpoint_reserve(env: &Env, reserve_index: u32) {
    let debt_id = reserve_index
        .checked_mul(2)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    checkpoint_reserve_by_id(env, debt_id);
    checkpoint_reserve_by_id(
        env,
        debt_id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError)),
    );
}

fn add_reserve_stream(env: &Env, reserve_token_id: u32, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    let reserve = reserve_for_token_id(env, reserve_token_id);
    let now = env.ledger().timestamp();
    let index_scale = emission_index_scale(env, &reserve);
    let mut data = get_reserve_data(env, reserve_token_id)
        .map(|data| {
            advance_reserve_data(
                env,
                data,
                token_supply(&reserve, reserve_token_id),
                index_scale,
            )
        })
        .unwrap_or(ReserveEmissionData {
            carry: 0,
            expiration: now,
            index: 0,
            last_time: now,
            remaining: 0,
        });
    if amount == 0 && data.remaining == 0 {
        return;
    }
    data.remaining = lending::checked_add(env, data.remaining, amount);
    data.last_time = now;
    data.expiration = now
        .checked_add(SECONDS_PER_WEEK)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    set_reserve_data(env, reserve_token_id, &data);
    add_reserve_token_id(env, reserve_token_id);
    ReserveEmissionUpdated {
        amount,
        expiration: data.expiration,
        reserve_token_id,
    }
    .publish(env);
}

fn checkpoint_reserve_by_id(env: &Env, reserve_token_id: u32) {
    let Some(data) = get_reserve_data(env, reserve_token_id) else {
        return;
    };
    let reserve = reserve_for_token_id(env, reserve_token_id);
    let data = advance_reserve_data(
        env,
        data,
        token_supply(&reserve, reserve_token_id),
        emission_index_scale(env, &reserve),
    );
    set_reserve_data(env, reserve_token_id, &data);
}

fn checkpoint_user_by_id(
    env: &Env,
    user: &Address,
    positions: &Positions,
    reserve_token_id: u32,
) -> (UserEmissionData, i128) {
    let reserve = reserve_for_token_id(env, reserve_token_id);
    let index_scale = emission_index_scale(env, &reserve);
    (
        checkpoint_user_with_reserve(env, user, positions, &reserve, reserve_token_id),
        index_scale,
    )
}

fn checkpoint_user_with_reserve(
    env: &Env,
    user: &Address,
    positions: &Positions,
    reserve: &Reserve,
    reserve_token_id: u32,
) -> UserEmissionData {
    let index_scale = emission_index_scale(env, reserve);
    let current_index = if let Some(data) = get_reserve_data(env, reserve_token_id) {
        let data = advance_reserve_data(
            env,
            data,
            token_supply(reserve, reserve_token_id),
            index_scale,
        );
        let index = data.index;
        set_reserve_data(env, reserve_token_id, &data);
        index
    } else {
        0
    };
    let user_data = advance_user_data(
        env,
        get_user_data(env, user, reserve_token_id, index_scale),
        token_balance(env, positions, reserve_token_id),
        current_index,
        index_scale,
    );
    set_user_data(env, user, reserve_token_id, &user_data, index_scale);
    user_data
}

fn advance_reserve_data(
    env: &Env,
    mut data: ReserveEmissionData,
    supply: i128,
    index_scale: i128,
) -> ReserveEmissionData {
    validate_reserve_data(env, &data);
    if supply < 0 || index_scale <= 0 {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    let checkpoint = core::cmp::min(env.ledger().timestamp(), data.expiration);
    if supply == 0
        || data.remaining == 0
        || checkpoint <= data.last_time
        || data.last_time >= data.expiration
    {
        return data;
    }

    let elapsed = checkpoint - data.last_time;
    let seconds_left = data.expiration - data.last_time;
    let vested = if checkpoint == data.expiration {
        data.remaining
    } else {
        lending::mul_div_floor(
            env,
            data.remaining,
            i128::from(elapsed),
            i128::from(seconds_left),
        )
    };
    data.remaining = lending::checked_sub_nonnegative(env, data.remaining, vested);
    let distributable = lending::checked_add(env, vested, data.carry);
    let index_delta = lending::mul_div_floor(env, distributable, index_scale, supply);
    let represented = lending::mul_div_floor(env, index_delta, supply, index_scale);
    data.carry = lending::checked_sub_nonnegative(env, distributable, represented);
    data.index = lending::checked_add(env, data.index, index_delta);
    data.last_time = checkpoint;
    validate_reserve_data(env, &data);
    data
}

fn advance_user_data(
    env: &Env,
    mut data: UserEmissionData,
    balance: i128,
    current_index: i128,
    index_scale: i128,
) -> UserEmissionData {
    validate_user_data(env, &data, index_scale);
    if balance < 0 || current_index < data.index {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    let delta = current_index - data.index;
    let numerator = I256::from_i128(env, balance)
        .mul(&I256::from_i128(env, delta))
        .add(&I256::from_i128(env, data.carry));
    let scale = I256::from_i128(env, index_scale);
    let accrued = numerator
        .div(&scale)
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let carry = numerator
        .sub(&I256::from_i128(env, accrued).mul(&scale))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    data.accrued = lending::checked_add(env, data.accrued, accrued);
    data.carry = carry;
    data.index = current_index;
    validate_user_data(env, &data, index_scale);
    data
}

fn emission_index_scale(env: &Env, reserve: &Reserve) -> i128 {
    lending::pow10(env, reserve.config.decimals)
        .checked_mul(SCALAR_7)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
}

fn reserve_for_token_id(env: &Env, reserve_token_id: u32) -> Reserve {
    let index = reserve_token_id / 2;
    let asset = lending::reserve_list(env)
        .get(index)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve));
    lending::reserve(env, &asset)
}

fn require_reserve_token_id(env: &Env, reserve_token_id: u32) {
    if reserve_token_id >= MAX_RESERVE_TOKEN_IDS {
        panic_with_error!(env, PoolError::InvalidEmissionConfig);
    }
    let _ = reserve_for_token_id(env, reserve_token_id);
}

fn token_supply(reserve: &Reserve, reserve_token_id: u32) -> i128 {
    if reserve_token_id.is_multiple_of(2) {
        reserve.data.d_supply
    } else {
        reserve.data.b_supply
    }
}

fn token_balance(env: &Env, positions: &Positions, reserve_token_id: u32) -> i128 {
    let index = reserve_token_id / 2;
    if reserve_token_id.is_multiple_of(2) {
        positions.liabilities.get(index).unwrap_or(0)
    } else {
        positions
            .supply
            .get(index)
            .unwrap_or(0)
            .checked_add(positions.collateral.get(index).unwrap_or(0))
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError))
    }
}

fn reserve_token_ids(env: &Env) -> Vec<u32> {
    let ids = env
        .storage()
        .instance()
        .get(&EmissionDataKey::CreatedReserveEmissionIds)
        .unwrap_or_else(|| Vec::new(env));
    if ids.len() > MAX_RESERVE_TOKEN_IDS {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    ids
}

fn add_reserve_token_id(env: &Env, reserve_token_id: u32) {
    let mut ids = reserve_token_ids(env);
    if !ids.contains(reserve_token_id) {
        if ids.len() >= MAX_RESERVE_TOKEN_IDS {
            panic_with_error!(env, PoolError::InvalidEmissionValue);
        }
        ids.push_back(reserve_token_id);
        env.storage()
            .instance()
            .set(&EmissionDataKey::CreatedReserveEmissionIds, &ids);
    }
}

fn get_reserve_data(env: &Env, reserve_token_id: u32) -> Option<ReserveEmissionData> {
    let key = EmissionPersistentKey::ReserveEmissionStates;
    let states: Option<Map<u32, ReserveEmissionData>> = env.storage().persistent().get(&key);
    if let Some(states) = states {
        env.storage()
            .persistent()
            .extend_ttl(&key, SHARED_TTL_THRESHOLD, SHARED_TTL_BUMP);
        if states.len() > MAX_RESERVE_TOKEN_IDS {
            panic_with_error!(env, PoolError::InvalidEmissionValue);
        }
        states.get(reserve_token_id)
    } else {
        None
    }
}

fn set_reserve_data(env: &Env, reserve_token_id: u32, data: &ReserveEmissionData) {
    validate_reserve_data(env, data);
    let key = EmissionPersistentKey::ReserveEmissionStates;
    let mut states: Map<u32, ReserveEmissionData> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Map::new(env));
    if !states.contains_key(reserve_token_id) && states.len() >= MAX_RESERVE_TOKEN_IDS {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    states.set(reserve_token_id, data.clone());
    env.storage().persistent().set(&key, &states);
    env.storage()
        .persistent()
        .extend_ttl(&key, SHARED_TTL_THRESHOLD, SHARED_TTL_BUMP);
}

fn get_user_data(
    env: &Env,
    user: &Address,
    reserve_token_id: u32,
    index_scale: i128,
) -> UserEmissionData {
    let key = EmissionPersistentKey::UserEmissionCheckpoints(user.clone());
    let checkpoints: Option<Map<u32, UserEmissionData>> = env.storage().persistent().get(&key);
    let data = checkpoints
        .as_ref()
        .and_then(|checkpoints| checkpoints.get(reserve_token_id))
        .unwrap_or(UserEmissionData {
            accrued: 0,
            carry: 0,
            index: 0,
        });
    if let Some(checkpoints) = checkpoints {
        if checkpoints.len() > MAX_RESERVE_TOKEN_IDS {
            panic_with_error!(env, PoolError::InvalidEmissionValue);
        }
        env.storage()
            .persistent()
            .extend_ttl(&key, USER_TTL_THRESHOLD, USER_TTL_BUMP);
    }
    validate_user_data(env, &data, index_scale);
    data
}

fn set_user_data(
    env: &Env,
    user: &Address,
    reserve_token_id: u32,
    data: &UserEmissionData,
    index_scale: i128,
) {
    validate_user_data(env, data, index_scale);
    let key = EmissionPersistentKey::UserEmissionCheckpoints(user.clone());
    let mut checkpoints: Map<u32, UserEmissionData> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Map::new(env));
    if !checkpoints.contains_key(reserve_token_id) && checkpoints.len() >= MAX_RESERVE_TOKEN_IDS {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
    checkpoints.set(reserve_token_id, data.clone());
    env.storage().persistent().set(&key, &checkpoints);
    env.storage()
        .persistent()
        .extend_ttl(&key, USER_TTL_THRESHOLD, USER_TTL_BUMP);
}

fn validate_reserve_data(env: &Env, data: &ReserveEmissionData) {
    if data.carry < 0 || data.index < 0 || data.remaining < 0 || data.last_time > data.expiration {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
}

fn validate_user_data(env: &Env, data: &UserEmissionData, index_scale: i128) {
    if index_scale <= 0
        || data.accrued < 0
        || data.carry < 0
        || data.carry >= index_scale
        || data.index < 0
    {
        panic_with_error!(env, PoolError::InvalidEmissionValue);
    }
}
