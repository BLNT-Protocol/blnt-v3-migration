#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, token::TokenClient, Address, Env, I256,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const DAY_IN_SECONDS: u64 = 24 * 60 * 60;
const QUEUE_SECONDS: u64 = 31 * DAY_IN_SECONDS;
const PREPARATION_WINDOW_SECONDS: u64 = 7 * DAY_IN_SECONDS;
const SYNC_GRACE_SECONDS: u64 = 7 * DAY_IN_SECONDS;
const RETRY_PERIOD_SECONDS: u64 = QUEUE_SECONDS + SYNC_GRACE_SECONDS;
const MAX_RETRY_QUEUES: u32 = 2;
const RECOVERY_HORIZON_SECONDS: u64 = MAX_RETRY_QUEUES as u64 * RETRY_PERIOD_SECONDS;
const MAX_MIGRATION_LIFETIME_SECONDS: u64 = 2 * QUEUE_SECONDS + RECOVERY_HORIZON_SECONDS;
const SCALAR_7: i128 = 10_000_000;
const BLND_SCALAR: i128 = SCALAR_7;
const MAX_BACKFILL_BLND: i128 = 10_000_000 * BLND_SCALAR;
const ACTIVATION_ENTRY_THRESHOLD_USDC: i128 = 12_500 * SCALAR_7;
const ACTIVATION_MAINTENANCE_THRESHOLD_USDC: i128 = 10_000 * SCALAR_7;
const POSITION_TTL_THRESHOLD: u32 = 179 * DAY_IN_LEDGERS;
const POSITION_TTL_BUMP: u32 = 180 * DAY_IN_LEDGERS;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Emitter,
    LegacyBackstopToken,
    NextBackstopToken,
    PrefundToken,
    PrefundingStart,
    AbsoluteMigrationDeadline,
    MigrationEpochStart,
    OriginalUnlock,
    VerifiedQueueUnlock,
    RetryCount,
    ActivatedAt,
    BackfillEnd,
    TotalMigrationAmount,
    TotalMigrationTimeBasis,
    TotalMigrationWeight,
    ScheduledBackfill,
    TotalPrefundShares,
}

#[derive(Clone)]
#[contracttype]
enum PersistentKey {
    MigrationPosition(Address, Address),
    PrefundShares(Address, Address),
    PoolPrefundShares(Address),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MigrationStatus {
    Pending,
    Open,
    Prepared,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum BackstopError {
    AlreadyFinalized = 1200,
    EmitterDidNotMigrate = 1201,
    SwapNotQueued = 1202,
    InvalidQueuedSwap = 1203,
    EmitterAlreadyMigrated = 1204,
    MigrationNotPrepared = 1205,
    PreparationOutsideWindow = 1206,
    SyncWindowExpired = 1207,
    ArithmeticError = 1208,
    RetryLimitExceeded = 1209,
    RecoveryHorizonExceeded = 1210,
    QueueNotVerified = 1211,
    InvalidAmount = 1212,
    InsufficientBalance = 1213,
    MigrationAlreadyActive = 1214,
    MigrationNotActive = 1215,
    NoEligibleWeight = 1216,
    PrefundingWindowExceeded = 1217,
    MigrationEpochNotOpen = 1218,
    MigrationEpochAlreadyOpen = 1219,
    AssetConfigurationCollision = 1220,
    InvalidActivationValue = 1221,
}

#[derive(Clone)]
#[contracttype]
pub struct EmitterSwap {
    pub new_backstop: Address,
    pub new_backstop_token: Address,
    pub unlock_time: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MigrationPosition {
    pub amount: i128,
    pub frozen_weight: Option<i128>,
    pub time_basis: i128,
}

/// Verified, pool-specific asset values denominated in 7-decimal USDC units.
///
/// Constructing these values from token balances and manipulation-resistant
/// prices is the responsibility of the canonical valuation layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ActivationValues {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub usdc: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ActivationQuote {
    pub eligible_value: i128,
    pub meets_threshold: bool,
    pub required_value: i128,
}

#[contractclient(name = "EmitterClient")]
pub trait Emitter {
    fn get_backstop(env: Env) -> Address;
    fn get_queued_swap(env: Env) -> Option<EmitterSwap>;
    fn queue_swap_backstop(env: Env, new_backstop: Address, new_backstop_token: Address);
    fn swap_backstop(env: Env);
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPrepared {
    pub original_unlock: u64,
    pub retry_count: u32,
    pub verified_queue_unlock: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationActivated {
    pub activated_at: u64,
    pub backfill_end: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrefundDeposited {
    pub depositor: Address,
    pub pool: Address,
    pub shares: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrefundWithdrawn {
    pub depositor: Address,
    pub pool: Address,
    pub shares: i128,
    pub to: Address,
}

#[contract]
pub struct BackstopContract;

#[contractimpl]
impl BackstopContract {
    pub fn __constructor(
        env: Env,
        emitter: Address,
        legacy_backstop_token: Address,
        next_backstop_token: Address,
        prefund_token: Address,
    ) {
        if legacy_backstop_token == prefund_token {
            panic_with_error!(&env, BackstopError::AssetConfigurationCollision);
        }
        extend_instance_ttl(&env);
        let prefunding_start = env.ledger().timestamp();
        let absolute_migration_deadline =
            checked_add_seconds(&env, prefunding_start, MAX_MIGRATION_LIFETIME_SECONDS);
        env.storage()
            .instance()
            .set(&DataKey::PrefundingStart, &prefunding_start);
        env.storage().instance().set(
            &DataKey::AbsoluteMigrationDeadline,
            &absolute_migration_deadline,
        );
        env.storage().instance().set(&DataKey::Emitter, &emitter);
        env.storage()
            .instance()
            .set(&DataKey::LegacyBackstopToken, &legacy_backstop_token);
        env.storage()
            .instance()
            .set(&DataKey::NextBackstopToken, &next_backstop_token);
        env.storage()
            .instance()
            .set(&DataKey::PrefundToken, &prefund_token);
    }

    /// Return the legacy token held directly by this contract for migration.
    ///
    /// The name preserves the incumbent v2 read interface.
    pub fn backstop_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::LegacyBackstopToken)
            .unwrap()
    }

    pub fn emitter(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Emitter).unwrap()
    }

    /// Return the token the emitter will use to qualify the next migration.
    pub fn next_backstop_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::NextBackstopToken)
            .unwrap()
    }

    /// Return the independently configured post-migration prefunding asset.
    pub fn prefund_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::PrefundToken)
            .unwrap()
    }

    /// Deposit the configured post-migration asset as one-to-one prefund shares.
    ///
    /// Prefund shares do not count toward legacy emitter qualification,
    /// migration backfill, activation, losses, or ongoing emissions.
    pub fn deposit_prefund(env: Env, from: Address, pool: Address, shares: i128) {
        extend_instance_ttl(&env);
        require_positive(&env, shares);
        from.require_auth();

        let user_shares = get_prefund_shares(&env, &from, &pool);
        set_prefund_shares(
            &env,
            &from,
            &pool,
            checked_add_i128(&env, user_shares, shares),
        );
        let pool_shares = get_pool_prefund_shares(&env, &pool);
        set_pool_prefund_shares(&env, &pool, checked_add_i128(&env, pool_shares, shares));
        add_to_instance_total(&env, DataKey::TotalPrefundShares, shares);

        TokenClient::new(&env, &prefund_token(&env)).transfer(
            &from,
            &env.current_contract_address(),
            &shares,
        );
        PrefundDeposited {
            depositor: from,
            pool,
            shares,
        }
        .publish(&env);
    }

    /// Withdraw one-to-one prefund shares.
    ///
    /// Immediate withdrawal is permitted while prefunding remains ineligible
    /// for activation and is not committed to a loss process.
    pub fn withdraw_prefund(env: Env, from: Address, pool: Address, shares: i128, to: Address) {
        extend_instance_ttl(&env);
        require_positive(&env, shares);
        from.require_auth();

        let user_shares = get_prefund_shares(&env, &from, &pool);
        if shares > user_shares {
            panic_with_error!(&env, BackstopError::InsufficientBalance);
        }
        set_prefund_shares(
            &env,
            &from,
            &pool,
            checked_sub_i128(&env, user_shares, shares),
        );
        let pool_shares = get_pool_prefund_shares(&env, &pool);
        set_pool_prefund_shares(&env, &pool, checked_sub_i128(&env, pool_shares, shares));
        add_to_instance_total(&env, DataKey::TotalPrefundShares, -shares);

        TokenClient::new(&env, &prefund_token(&env)).transfer(
            &env.current_contract_address(),
            &to,
            &shares,
        );
        PrefundWithdrawn {
            depositor: from,
            pool,
            shares,
            to,
        }
        .publish(&env);
    }

    pub fn prefund_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_prefund_shares(&env, &user, &pool)
    }

    pub fn pool_prefund_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_prefund_shares(&env, &pool)
    }

    pub fn total_prefund_shares(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalPrefundShares)
            .unwrap_or(0)
    }

    /// Return the threshold an inactive pool must meet to activate.
    pub fn activation_entry_threshold(env: Env) -> i128 {
        extend_instance_ttl(&env);
        ACTIVATION_ENTRY_THRESHOLD_USDC
    }

    /// Return the threshold an active pool must maintain to remain active.
    pub fn activation_maintenance_threshold(env: Env) -> i128 {
        extend_instance_ttl(&env);
        ACTIVATION_MAINTENANCE_THRESHOLD_USDC
    }

    /// Quote verified pool-specific values under the canonical activation
    /// policy's hysteresis rule.
    ///
    /// This function defines policy arithmetic only. Its arguments MUST come
    /// from the canonical valuation layer before its result is used for pool
    /// status or reward eligibility.
    pub fn quote_activation(
        env: Env,
        values: ActivationValues,
        currently_active: bool,
    ) -> ActivationQuote {
        extend_instance_ttl(&env);
        quote_activation_values(&env, &values, currently_active)
    }

    pub fn migration_status(env: Env) -> MigrationStatus {
        extend_instance_ttl(&env);
        if env.storage().instance().has(&DataKey::ActivatedAt) {
            MigrationStatus::Active
        } else if env.storage().instance().has(&DataKey::VerifiedQueueUnlock) {
            MigrationStatus::Prepared
        } else if env.storage().instance().has(&DataKey::OriginalUnlock) {
            MigrationStatus::Open
        } else {
            MigrationStatus::Pending
        }
    }

    /// Return the registered original queue's immutable unlock anchor.
    pub fn original_unlock(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::OriginalUnlock)
    }

    /// Return when the bounded prefunding campaign began.
    pub fn prefunding_start(env: Env) -> u64 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::PrefundingStart)
            .unwrap()
    }

    /// Return the deployment-anchored maximum migration deadline.
    pub fn absolute_migration_deadline(env: Env) -> u64 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::AbsoluteMigrationDeadline)
            .unwrap()
    }

    /// Return when the original legacy-emitter queue began.
    pub fn migration_epoch_start(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::MigrationEpochStart)
    }

    /// Return the queue currently approved for finalization or synchronization.
    pub fn verified_queue_unlock(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::VerifiedQueueUnlock)
    }

    pub fn retry_count(env: Env) -> u32 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::RetryCount)
            .unwrap_or(0)
    }

    /// Return the absolute end of the two-retry recovery horizon.
    pub fn backfill_cap(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::OriginalUnlock)
            .map(|unlock| recovery_end(&env, unlock))
    }

    /// Return the final time-weighted backfill endpoint after activation.
    pub fn backfill_end(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::BackfillEnd)
    }

    /// Deposit legacy backstop tokens into a pool-attributed migration position.
    ///
    /// Only shares still deposited when migration activates retain their
    /// accumulated amount-times-time weight.
    pub fn deposit_migration(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        require_not_active_for_deposit(&env);
        require_positive(&env, amount);
        from.require_auth();

        try_open_epoch_from_current_queue(&env);
        let mut position = get_position(&env, &from, &pool);
        let timestamp = eligibility_timestamp(&env, env.ledger().timestamp());
        position.amount = checked_add_i128(&env, position.amount, amount);
        position.time_basis = checked_add_i128(
            &env,
            position.time_basis,
            checked_mul_i128(&env, amount, timestamp as i128),
        );
        set_position(&env, &from, &pool, &position);

        add_to_instance_total(&env, DataKey::TotalMigrationAmount, amount);
        add_to_instance_total(
            &env,
            DataKey::TotalMigrationTimeBasis,
            checked_mul_i128(&env, amount, timestamp as i128),
        );

        TokenClient::new(&env, &legacy_backstop_token(&env)).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );
    }

    /// Withdraw legacy tokens. Before activation, the withdrawn shares forfeit
    /// their proportional accumulated backfill weight.
    pub fn withdraw_migration(env: Env, from: Address, pool: Address, amount: i128, to: Address) {
        extend_instance_ttl(&env);
        require_positive(&env, amount);
        from.require_auth();

        let mut position = get_position(&env, &from, &pool);
        if amount > position.amount {
            panic_with_error!(&env, BackstopError::InsufficientBalance);
        }

        if let Some(backfill_end) = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::BackfillEnd)
        {
            if position.frozen_weight.is_none() {
                position.frozen_weight = Some(position_weight(&env, &position, backfill_end));
            }
        } else {
            let basis_removed =
                proportional_floor(&env, position.time_basis, amount, position.amount);
            position.time_basis = checked_sub_i128(&env, position.time_basis, basis_removed);
            add_to_instance_total(&env, DataKey::TotalMigrationAmount, -amount);
            add_to_instance_total(&env, DataKey::TotalMigrationTimeBasis, -basis_removed);
        }

        position.amount = checked_sub_i128(&env, position.amount, amount);
        set_position(&env, &from, &pool, &position);
        TokenClient::new(&env, &legacy_backstop_token(&env)).transfer(
            &env.current_contract_address(),
            &to,
            &amount,
        );
    }

    pub fn migration_position(env: Env, user: Address, pool: Address) -> MigrationPosition {
        extend_instance_ttl(&env);
        get_position(&env, &user, &pool)
    }

    /// Return a position's finalized amount-times-seconds allocation weight.
    pub fn migration_weight(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        let backfill_end: u64 = env
            .storage()
            .instance()
            .get(&DataKey::BackfillEnd)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        let position = get_position(&env, &user, &pool);
        position
            .frozen_weight
            .unwrap_or_else(|| position_weight(&env, &position, backfill_end))
    }

    pub fn total_migration_weight(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalMigrationWeight)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive))
    }

    /// Return the deterministic 1-BLND-per-second backfill scheduled at activation.
    pub fn scheduled_backfill(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::ScheduledBackfill)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive))
    }

    /// Quote a position's proportional share of scheduled BLND backpay.
    ///
    /// This does not transfer BLND; the funding and claim mechanism remains a
    /// separate milestone and must use the same finalized weights.
    pub fn quote_backfill(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        let total_weight: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMigrationWeight)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        if total_weight <= 0 {
            panic_with_error!(&env, BackstopError::NoEligibleWeight);
        }
        let total_backfill: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ScheduledBackfill)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        let position_weight = Self::migration_weight(env.clone(), user, pool);
        proportional_floor(&env, total_backfill, position_weight, total_weight)
    }

    /// Return when this contract first synchronized its active lifecycle state.
    pub fn activated_at(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::ActivatedAt)
    }

    /// Return the final timestamp at which a bypassed direct swap may synchronize.
    pub fn sync_deadline(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::VerifiedQueueUnlock)
            .map(|cutoff| checked_sync_deadline(&env, cutoff))
    }

    /// Open the public accounting epoch from the original emitter queue.
    ///
    /// This does not verify the queue for finalization; that remains restricted
    /// to the final seven days.
    pub fn open_migration_epoch(env: Env) -> u64 {
        extend_instance_ttl(&env);
        require_not_active(&env);
        if let Some(epoch_start) = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::MigrationEpochStart)
        {
            return epoch_start;
        }
        let emitter = get_emitter(&env);
        let queued_swap = EmitterClient::new(&env, &emitter)
            .get_queued_swap()
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::SwapNotQueued));
        require_valid_queued_swap(&env, &queued_swap, &env.current_contract_address());
        open_epoch(&env, queued_swap.unlock_time)
    }

    /// Atomically create the canonical legacy-emitter queue and open its epoch.
    pub fn begin_migration(env: Env) -> u64 {
        extend_instance_ttl(&env);
        require_not_active(&env);
        if env.storage().instance().has(&DataKey::OriginalUnlock) {
            panic_with_error!(&env, BackstopError::MigrationEpochAlreadyOpen);
        }
        let emitter = get_emitter(&env);
        let emitter_client = EmitterClient::new(&env, &emitter);
        let candidate = env.current_contract_address();
        let next_backstop_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::NextBackstopToken)
            .unwrap();
        emitter_client.queue_swap_backstop(&candidate, &next_backstop_token);
        let queued_swap = emitter_client
            .get_queued_swap()
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::SwapNotQueued));
        require_valid_queued_swap(&env, &queued_swap, &candidate);
        open_epoch(&env, queued_swap.unlock_time)
    }

    /// Verify an emitter queue and commit its unlock as the active queue anchor.
    ///
    /// The first unlock and absolute recovery horizon are immutable. Up to two
    /// replacement queues may be verified within that horizon.
    pub fn prepare_migration(env: Env) -> u64 {
        extend_instance_ttl(&env);
        require_not_active(&env);
        let emitter = get_emitter(&env);
        let emitter_client = EmitterClient::new(&env, &emitter);
        let candidate = env.current_contract_address();
        let queued_swap = emitter_client
            .get_queued_swap()
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::SwapNotQueued));
        require_valid_queued_swap(&env, &queued_swap, &candidate);
        let timestamp = env.ledger().timestamp();
        let preparation_start = queued_swap
            .unlock_time
            .saturating_sub(PREPARATION_WINDOW_SECONDS);
        if timestamp < preparation_start || timestamp > queued_swap.unlock_time {
            panic_with_error!(&env, BackstopError::PreparationOutsideWindow);
        }

        if !env.storage().instance().has(&DataKey::OriginalUnlock) {
            panic_with_error!(&env, BackstopError::MigrationEpochNotOpen);
        }
        let original_unlock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::OriginalUnlock)
            .unwrap();
        let verified_unlock = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::VerifiedQueueUnlock);
        if let Some(verified_unlock) = verified_unlock {
            if queued_swap.unlock_time == verified_unlock {
                return original_unlock;
            }

            let retry_count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::RetryCount)
                .unwrap_or(0);
            if retry_count >= MAX_RETRY_QUEUES {
                panic_with_error!(&env, BackstopError::RetryLimitExceeded);
            }
            let recovery_end = recovery_end(&env, original_unlock);
            if checked_sync_deadline(&env, queued_swap.unlock_time) > recovery_end {
                panic_with_error!(&env, BackstopError::RecoveryHorizonExceeded);
            }
            let next_retry_count = retry_count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(&env, BackstopError::ArithmeticError));
            env.storage()
                .instance()
                .set(&DataKey::VerifiedQueueUnlock, &queued_swap.unlock_time);
            env.storage()
                .instance()
                .set(&DataKey::RetryCount, &next_retry_count);
            MigrationPrepared {
                original_unlock,
                retry_count: next_retry_count,
                verified_queue_unlock: queued_swap.unlock_time,
            }
            .publish(&env);
            return original_unlock;
        }

        let retry_count = if queued_swap.unlock_time == original_unlock {
            0
        } else {
            let recovery_end = recovery_end(&env, original_unlock);
            if checked_sync_deadline(&env, queued_swap.unlock_time) > recovery_end {
                panic_with_error!(&env, BackstopError::RecoveryHorizonExceeded);
            }
            1
        };
        env.storage()
            .instance()
            .set(&DataKey::VerifiedQueueUnlock, &queued_swap.unlock_time);
        env.storage()
            .instance()
            .set(&DataKey::RetryCount, &retry_count);
        MigrationPrepared {
            original_unlock,
            retry_count,
            verified_queue_unlock: queued_swap.unlock_time,
        }
        .publish(&env);
        original_unlock
    }

    /// Finalize the queued legacy-emitter swap and activate this candidate.
    ///
    /// This operation is permissionless. The emitter enforces its queue delay
    /// and strictly-greater legacy-token balance requirement.
    pub fn finalize_migration(env: Env) -> u64 {
        extend_instance_ttl(&env);
        require_not_active(&env);
        let original_unlock = require_prepared(&env);
        if env.ledger().timestamp() > recovery_end(&env, original_unlock) {
            panic_with_error!(&env, BackstopError::RecoveryHorizonExceeded);
        }
        let emitter = get_emitter(&env);
        let emitter_client = EmitterClient::new(&env, &emitter);
        let candidate = env.current_contract_address();

        if emitter_client.get_backstop() == candidate {
            panic_with_error!(&env, BackstopError::EmitterAlreadyMigrated);
        }

        let queued_swap = emitter_client
            .get_queued_swap()
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::SwapNotQueued));
        require_valid_queued_swap(&env, &queued_swap, &candidate);
        require_verified_queue(&env, &queued_swap);
        emitter_client.swap_backstop();
        if emitter_client.get_backstop() != candidate {
            panic_with_error!(&env, BackstopError::EmitterDidNotMigrate);
        }

        activate(&env)
    }

    /// Synchronize lifecycle state after the emitter was finalized directly.
    ///
    /// Backfill remains bounded by the immutable recovery horizon recorded
    /// during initial preparation, regardless of when synchronization occurs.
    pub fn sync_migration(env: Env) -> u64 {
        extend_instance_ttl(&env);
        require_not_active(&env);
        require_prepared(&env);
        let verified_unlock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::VerifiedQueueUnlock)
            .unwrap();
        if env.ledger().timestamp() > checked_sync_deadline(&env, verified_unlock) {
            panic_with_error!(&env, BackstopError::SyncWindowExpired);
        }
        let emitter = get_emitter(&env);
        let emitter_client = EmitterClient::new(&env, &emitter);
        if emitter_client.get_backstop() != env.current_contract_address() {
            panic_with_error!(&env, BackstopError::EmitterDidNotMigrate);
        }

        activate(&env)
    }
}

fn get_emitter(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Emitter).unwrap()
}

fn legacy_backstop_token(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::LegacyBackstopToken)
        .unwrap()
}

fn prefund_token(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::PrefundToken)
        .unwrap()
}

fn require_not_active(env: &Env) {
    if env.storage().instance().has(&DataKey::ActivatedAt) {
        panic_with_error!(env, BackstopError::AlreadyFinalized);
    }
}

fn require_not_active_for_deposit(env: &Env) {
    if env.storage().instance().has(&DataKey::ActivatedAt) {
        panic_with_error!(env, BackstopError::MigrationAlreadyActive);
    }
}

fn require_positive(env: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, BackstopError::InvalidAmount);
    }
}

fn require_prepared(env: &Env) -> u64 {
    if !env.storage().instance().has(&DataKey::VerifiedQueueUnlock) {
        panic_with_error!(env, BackstopError::MigrationNotPrepared);
    }
    env.storage()
        .instance()
        .get(&DataKey::OriginalUnlock)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::MigrationNotPrepared))
}

fn try_open_epoch_from_current_queue(env: &Env) {
    if env.storage().instance().has(&DataKey::MigrationEpochStart) {
        return;
    }
    let emitter = get_emitter(env);
    if let Some(queued_swap) = EmitterClient::new(env, &emitter).get_queued_swap() {
        let candidate = env.current_contract_address();
        let expected_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::NextBackstopToken)
            .unwrap();
        if queued_swap.new_backstop == candidate
            && queued_swap.new_backstop_token == expected_token
            && env.ledger().timestamp() <= queued_swap.unlock_time
        {
            open_epoch(env, queued_swap.unlock_time);
        }
    }
}

fn open_epoch(env: &Env, original_unlock: u64) -> u64 {
    let epoch_start = original_unlock
        .checked_sub(QUEUE_SECONDS)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    let prefunding_start: u64 = env
        .storage()
        .instance()
        .get(&DataKey::PrefundingStart)
        .unwrap();
    let prefunding_duration = epoch_start
        .checked_sub(prefunding_start)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    if prefunding_duration > QUEUE_SECONDS {
        panic_with_error!(env, BackstopError::PrefundingWindowExceeded);
    }
    if env.ledger().timestamp() < epoch_start || env.ledger().timestamp() > original_unlock {
        panic_with_error!(env, BackstopError::PreparationOutsideWindow);
    }
    recovery_end(env, original_unlock);
    env.storage()
        .instance()
        .set(&DataKey::MigrationEpochStart, &epoch_start);
    env.storage()
        .instance()
        .set(&DataKey::OriginalUnlock, &original_unlock);
    env.storage().instance().set(&DataKey::RetryCount, &0u32);
    epoch_start
}

fn require_valid_queued_swap(env: &Env, queued_swap: &EmitterSwap, candidate: &Address) {
    let next_backstop_token: Address = env
        .storage()
        .instance()
        .get(&DataKey::NextBackstopToken)
        .unwrap();
    if queued_swap.new_backstop != *candidate
        || queued_swap.new_backstop_token != next_backstop_token
    {
        panic_with_error!(env, BackstopError::InvalidQueuedSwap);
    }
}

fn require_verified_queue(env: &Env, queued_swap: &EmitterSwap) {
    let verified_unlock: u64 = env
        .storage()
        .instance()
        .get(&DataKey::VerifiedQueueUnlock)
        .unwrap();
    if queued_swap.unlock_time != verified_unlock {
        panic_with_error!(env, BackstopError::QueueNotVerified);
    }
}

fn checked_sync_deadline(env: &Env, verified_unlock: u64) -> u64 {
    checked_add_seconds(env, verified_unlock, SYNC_GRACE_SECONDS)
}

fn checked_add_seconds(env: &Env, timestamp: u64, seconds: u64) -> u64 {
    timestamp
        .checked_add(seconds)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn recovery_end(env: &Env, original_unlock: u64) -> u64 {
    let queue_anchored_end = checked_add_seconds(env, original_unlock, RECOVERY_HORIZON_SECONDS);
    let absolute_deadline: u64 = env
        .storage()
        .instance()
        .get(&DataKey::AbsoluteMigrationDeadline)
        .unwrap();
    if queue_anchored_end > absolute_deadline {
        panic_with_error!(env, BackstopError::RecoveryHorizonExceeded);
    }
    queue_anchored_end
}

fn activate(env: &Env) -> u64 {
    let original_unlock = require_prepared(env);
    let activated_at = env.ledger().timestamp();
    let backfill_cap = recovery_end(env, original_unlock);
    let backfill_end = activated_at.min(backfill_cap);
    let migration_epoch_start: u64 = env
        .storage()
        .instance()
        .get(&DataKey::MigrationEpochStart)
        .unwrap();
    let scheduled_backfill = calculate_scheduled_backfill(env, migration_epoch_start, backfill_end);
    env.storage()
        .instance()
        .set(&DataKey::ActivatedAt, &activated_at);
    env.storage()
        .instance()
        .set(&DataKey::BackfillEnd, &backfill_end);
    env.storage()
        .instance()
        .set(&DataKey::ScheduledBackfill, &scheduled_backfill);
    let total_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalMigrationAmount)
        .unwrap_or(0);
    let total_time_basis: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalMigrationTimeBasis)
        .unwrap_or(0);
    let total_weight = checked_sub_i128(
        env,
        checked_mul_i128(env, total_amount, backfill_end as i128),
        total_time_basis,
    );
    env.storage()
        .instance()
        .set(&DataKey::TotalMigrationWeight, &total_weight);
    MigrationActivated {
        activated_at,
        backfill_end,
    }
    .publish(env);
    activated_at
}

fn eligibility_timestamp(env: &Env, timestamp: u64) -> u64 {
    if let Some(original_unlock) = env
        .storage()
        .instance()
        .get::<DataKey, u64>(&DataKey::OriginalUnlock)
    {
        timestamp.min(recovery_end(env, original_unlock))
    } else {
        timestamp
    }
}

fn get_position(env: &Env, user: &Address, pool: &Address) -> MigrationPosition {
    let key = PersistentKey::MigrationPosition(user.clone(), pool.clone());
    let position = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(MigrationPosition {
            amount: 0,
            frozen_weight: None,
            time_basis: 0,
        });
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
    }
    position
}

fn get_prefund_shares(env: &Env, user: &Address, pool: &Address) -> i128 {
    let key = PersistentKey::PrefundShares(user.clone(), pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
    }
    shares
}

fn set_prefund_shares(env: &Env, user: &Address, pool: &Address, shares: i128) {
    let key = PersistentKey::PrefundShares(user.clone(), pool.clone());
    env.storage().persistent().set(&key, &shares);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_pool_prefund_shares(env: &Env, pool: &Address) -> i128 {
    let key = PersistentKey::PoolPrefundShares(pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
    }
    shares
}

fn set_pool_prefund_shares(env: &Env, pool: &Address, shares: i128) {
    let key = PersistentKey::PoolPrefundShares(pool.clone());
    env.storage().persistent().set(&key, &shares);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn set_position(env: &Env, user: &Address, pool: &Address, position: &MigrationPosition) {
    let key = PersistentKey::MigrationPosition(user.clone(), pool.clone());
    env.storage().persistent().set(&key, position);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn add_to_instance_total(env: &Env, key: DataKey, delta: i128) {
    let current: i128 = env.storage().instance().get(&key).unwrap_or(0);
    env.storage()
        .instance()
        .set(&key, &checked_add_i128(env, current, delta));
}

fn position_weight(env: &Env, position: &MigrationPosition, end: u64) -> i128 {
    checked_sub_i128(
        env,
        checked_mul_i128(env, position.amount, end as i128),
        position.time_basis,
    )
}

fn calculate_scheduled_backfill(env: &Env, epoch_start: u64, backfill_end: u64) -> i128 {
    let backfill_seconds = backfill_end
        .checked_sub(epoch_start)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    checked_mul_i128(env, backfill_seconds as i128, BLND_SCALAR).min(MAX_BACKFILL_BLND)
}

fn proportional_floor(env: &Env, value: i128, numerator: i128, denominator: i128) -> i128 {
    if value < 0 || numerator < 0 || denominator <= 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    I256::from_i128(env, value)
        .mul(&I256::from_i128(env, numerator))
        .div(&I256::from_i128(env, denominator))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn quote_activation_values(
    env: &Env,
    values: &ActivationValues,
    currently_active: bool,
) -> ActivationQuote {
    if values.blnd_xlm < 0 || values.blnd_usdc < 0 || values.usdc < 0 {
        panic_with_error!(env, BackstopError::InvalidActivationValue);
    }
    let eligible_value = checked_add_i128(
        env,
        checked_add_i128(env, values.blnd_xlm, values.blnd_usdc),
        values.usdc,
    );
    let required_value = if currently_active {
        ACTIVATION_MAINTENANCE_THRESHOLD_USDC
    } else {
        ACTIVATION_ENTRY_THRESHOLD_USDC
    };
    ActivationQuote {
        eligible_value,
        meets_threshold: eligible_value >= required_value,
        required_value,
    }
}

fn checked_add_i128(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_add(right)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn checked_sub_i128(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_sub(right)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn checked_mul_i128(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_mul(right)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        testutils::{storage::Instance, Address as _, Ledger, MockAuth, MockAuthInvoke},
        token::StellarAssetClient,
        Address, IntoVal,
    };

    #[contract]
    struct MockEmitter;

    #[derive(Clone)]
    #[contracttype]
    enum MockEmitterKey {
        Backstop,
        Candidate,
        NextBackstopToken,
        QueuedUnlock,
    }

    #[contractimpl]
    impl MockEmitter {
        pub fn __constructor(
            env: Env,
            backstop: Address,
            candidate: Address,
            next_backstop_token: Address,
        ) {
            env.storage()
                .instance()
                .set(&MockEmitterKey::Backstop, &backstop);
            env.storage()
                .instance()
                .set(&MockEmitterKey::Candidate, &candidate);
            env.storage()
                .instance()
                .set(&MockEmitterKey::NextBackstopToken, &next_backstop_token);
        }

        pub fn get_backstop(env: Env) -> Address {
            env.storage()
                .instance()
                .get(&MockEmitterKey::Backstop)
                .unwrap()
        }

        pub fn get_queued_swap(env: Env) -> Option<EmitterSwap> {
            let candidate = env
                .storage()
                .instance()
                .get(&MockEmitterKey::Candidate)
                .unwrap();
            let next_backstop_token = env
                .storage()
                .instance()
                .get(&MockEmitterKey::NextBackstopToken)
                .unwrap();
            let unlock_time = env
                .storage()
                .instance()
                .get(&MockEmitterKey::QueuedUnlock)?;
            Some(EmitterSwap {
                new_backstop: candidate,
                new_backstop_token: next_backstop_token,
                unlock_time,
            })
        }

        pub fn queue_swap_backstop(env: Env, _new_backstop: Address, _new_backstop_token: Address) {
            let unlock_time = env.ledger().timestamp().checked_add(QUEUE_SECONDS).unwrap();
            env.storage()
                .instance()
                .set(&MockEmitterKey::QueuedUnlock, &unlock_time);
        }

        pub fn swap_backstop(env: Env) {
            let candidate: Address = env
                .storage()
                .instance()
                .get(&MockEmitterKey::Candidate)
                .unwrap();
            env.storage()
                .instance()
                .set(&MockEmitterKey::Backstop, &candidate);
        }
    }

    #[test]
    fn prepares_and_finalizes_once() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (&emitter_id, &legacy_token, &next_token, &prefund_token),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);

        assert_eq!(candidate.backstop_token(), legacy_token);
        assert_eq!(candidate.next_backstop_token(), next_token);
        assert_eq!(candidate.prefund_token(), prefund_token);
        assert_eq!(candidate.migration_status(), MigrationStatus::Pending);
        assert_eq!(candidate.original_unlock(), None);
        assert_eq!(candidate.backfill_end(), None);
        assert_eq!(candidate.activated_at(), None);
        assert_eq!(
            candidate.absolute_migration_deadline(),
            MAX_MIGRATION_LIFETIME_SECONDS
        );

        env.ledger().set_timestamp(1234);
        assert_eq!(candidate.begin_migration(), 1234);
        let unlock = QUEUE_SECONDS + 1234;
        env.ledger()
            .set_timestamp(unlock - PREPARATION_WINDOW_SECONDS);
        assert_eq!(candidate.prepare_migration(), unlock);
        assert_eq!(candidate.prepare_migration(), unlock);
        assert_eq!(candidate.migration_status(), MigrationStatus::Prepared);
        assert_eq!(candidate.migration_epoch_start(), Some(1234));
        assert_eq!(candidate.original_unlock(), Some(unlock));
        assert_eq!(candidate.verified_queue_unlock(), Some(unlock));
        env.ledger().set_timestamp(unlock);
        assert_eq!(candidate.finalize_migration(), unlock);
        assert_eq!(candidate.migration_status(), MigrationStatus::Active);
        assert_eq!(candidate.original_unlock(), Some(unlock));
        assert_eq!(candidate.backfill_end(), Some(unlock));
        assert_eq!(candidate.activated_at(), Some(unlock));
        assert_eq!(
            candidate.scheduled_backfill(),
            QUEUE_SECONDS as i128 * BLND_SCALAR
        );
        assert!(candidate.try_finalize_migration().is_err());
        assert!(candidate.try_sync_migration().is_err());
    }

    #[test]
    fn instance_ttl_covers_and_refreshes_after_two_retry_horizon() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (&emitter_id, &legacy_token, &next_token, &prefund_token),
        );

        let candidate = BackstopContractClient::new(&env, &candidate_id);

        advance_ledger_days(&env, 31);
        candidate.begin_migration();
        assert_instance_ttl(&env, &candidate_id, INSTANCE_TTL_BUMP);

        advance_ledger_days(&env, 24);
        candidate.prepare_migration();
        assert_instance_ttl(&env, &candidate_id, INSTANCE_TTL_BUMP);

        advance_ledger_days(&env, 83);
        candidate.migration_status();
        assert_instance_ttl(&env, &candidate_id, INSTANCE_TTL_BUMP);
    }

    #[test]
    fn migration_deposit_requires_user_authorization() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (&emitter_id, &legacy_token, &next_token, &prefund_token),
        );
        let user = Address::generate(&env);
        let pool = Address::generate(&env);

        assert!(BackstopContractClient::new(&env, &candidate_id)
            .try_deposit_migration(&user, &pool, &1)
            .is_err());
    }

    #[test]
    fn prefund_deposit_and_withdraw_require_owner_authorization() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let prefund_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let candidate_id = Address::generate(&env);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (&emitter_id, &legacy_token, &next_token, &prefund_token),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);
        let token = StellarAssetClient::new(&env, &prefund_token);
        let user = Address::generate(&env);
        let recipient = Address::generate(&env);
        let pool = Address::generate(&env);
        let amount = 100;
        token.mock_all_auths().mint(&user, &amount);

        assert!(candidate
            .try_deposit_prefund(&user, &pool, &amount)
            .is_err());
        assert_eq!(candidate.prefund_shares(&user, &pool), 0);
        assert_eq!(candidate.total_prefund_shares(), 0);
        assert_eq!(token.balance(&user), amount);

        candidate
            .mock_auths(&[MockAuth {
                address: &user,
                invoke: &MockAuthInvoke {
                    contract: &candidate_id,
                    fn_name: "deposit_prefund",
                    args: (&user, &pool, amount).into_val(&env),
                    sub_invokes: &[MockAuthInvoke {
                        contract: &prefund_token,
                        fn_name: "transfer",
                        args: (&user, &candidate_id, amount).into_val(&env),
                        sub_invokes: &[],
                    }],
                },
            }])
            .deposit_prefund(&user, &pool, &amount);
        assert_eq!(candidate.prefund_shares(&user, &pool), amount);
        assert_eq!(candidate.total_prefund_shares(), amount);
        assert_eq!(token.balance(&candidate_id), amount);

        assert!(candidate
            .try_withdraw_prefund(&user, &pool, &amount, &recipient)
            .is_err());
        assert_eq!(candidate.prefund_shares(&user, &pool), amount);
        assert_eq!(token.balance(&candidate_id), amount);

        candidate
            .mock_auths(&[MockAuth {
                address: &user,
                invoke: &MockAuthInvoke {
                    contract: &candidate_id,
                    fn_name: "withdraw_prefund",
                    args: (&user, &pool, amount, &recipient).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .withdraw_prefund(&user, &pool, &amount, &recipient);
        assert_eq!(candidate.prefund_shares(&user, &pool), 0);
        assert_eq!(candidate.total_prefund_shares(), 0);
        assert_eq!(token.balance(&candidate_id), 0);
        assert_eq!(token.balance(&recipient), amount);
    }

    #[test]
    #[should_panic]
    fn constructor_rejects_colliding_migration_and_prefund_assets() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let candidate = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (&emitter, &token, &next_token, &token),
        );
    }

    #[test]
    fn full_two_retry_horizon_schedules_9_244_800_blnd() {
        let env = Env::default();
        let epoch_start = 1_000;
        let original_unlock = epoch_start + QUEUE_SECONDS;
        let backfill_end = original_unlock + RECOVERY_HORIZON_SECONDS;

        assert_eq!(
            calculate_scheduled_backfill(&env, epoch_start, backfill_end),
            9_244_800 * BLND_SCALAR
        );
    }

    #[test]
    fn activation_policy_counts_three_assets_equally() {
        let env = Env::default();
        let mixed = ActivationValues {
            blnd_xlm: 5_000 * SCALAR_7,
            blnd_usdc: 4_000 * SCALAR_7,
            usdc: 3_500 * SCALAR_7,
        };
        let entry = quote_activation_values(&env, &mixed, false);
        assert_eq!(entry.eligible_value, ACTIVATION_ENTRY_THRESHOLD_USDC);
        assert_eq!(entry.required_value, ACTIVATION_ENTRY_THRESHOLD_USDC);
        assert!(entry.meets_threshold);

        let usdc_only = quote_activation_values(
            &env,
            &ActivationValues {
                blnd_xlm: 0,
                blnd_usdc: 0,
                usdc: ACTIVATION_ENTRY_THRESHOLD_USDC,
            },
            false,
        );
        assert!(usdc_only.meets_threshold);

        let blnd_xlm_only = quote_activation_values(
            &env,
            &ActivationValues {
                blnd_xlm: ACTIVATION_ENTRY_THRESHOLD_USDC,
                blnd_usdc: 0,
                usdc: 0,
            },
            false,
        );
        assert!(blnd_xlm_only.meets_threshold);

        let blnd_usdc_only = quote_activation_values(
            &env,
            &ActivationValues {
                blnd_xlm: 0,
                blnd_usdc: ACTIVATION_ENTRY_THRESHOLD_USDC,
                usdc: 0,
            },
            false,
        );
        assert!(blnd_usdc_only.meets_threshold);
    }

    #[test]
    fn activation_policy_applies_entry_and_maintenance_hysteresis() {
        let env = Env::default();
        let value = |amount| ActivationValues {
            blnd_xlm: amount,
            blnd_usdc: 0,
            usdc: 0,
        };

        let below_entry =
            quote_activation_values(&env, &value(ACTIVATION_ENTRY_THRESHOLD_USDC - 1), false);
        assert!(!below_entry.meets_threshold);

        let at_entry =
            quote_activation_values(&env, &value(ACTIVATION_ENTRY_THRESHOLD_USDC), false);
        assert!(at_entry.meets_threshold);

        let above_entry =
            quote_activation_values(&env, &value(ACTIVATION_ENTRY_THRESHOLD_USDC + 1), false);
        assert!(above_entry.meets_threshold);

        let hysteresis_band =
            quote_activation_values(&env, &value(ACTIVATION_MAINTENANCE_THRESHOLD_USDC), true);
        assert_eq!(
            hysteresis_band.required_value,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC
        );
        assert!(hysteresis_band.meets_threshold);

        let below_maintenance = quote_activation_values(
            &env,
            &value(ACTIVATION_MAINTENANCE_THRESHOLD_USDC - 1),
            true,
        );
        assert!(!below_maintenance.meets_threshold);

        let inactive_in_band =
            quote_activation_values(&env, &value(ACTIVATION_MAINTENANCE_THRESHOLD_USDC), false);
        assert!(!inactive_in_band.meets_threshold);
    }

    #[test]
    fn activation_policy_rejects_negative_values_and_overflow() {
        let env = Env::default();
        let negative = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_activation_values(
                &env,
                &ActivationValues {
                    blnd_xlm: -1,
                    blnd_usdc: 0,
                    usdc: 0,
                },
                false,
            )
        }));
        assert!(negative.is_err());

        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_activation_values(
                &env,
                &ActivationValues {
                    blnd_xlm: i128::MAX,
                    blnd_usdc: 1,
                    usdc: 0,
                },
                false,
            )
        }));
        assert!(overflow.is_err());
    }

    fn advance_ledger_days(env: &Env, days: u32) {
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + days * DAY_IN_LEDGERS);
        env.ledger().set_timestamp(
            env.ledger()
                .timestamp()
                .checked_add(days as u64 * DAY_IN_SECONDS)
                .unwrap(),
        );
    }

    fn assert_instance_ttl(env: &Env, contract: &Address, expected: u32) {
        env.as_contract(contract, || {
            assert_eq!(env.storage().instance().get_ttl(), expected);
        });
    }
}
