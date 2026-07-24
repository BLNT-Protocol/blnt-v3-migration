#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, token::TokenClient, Address, Env, Vec, I256,
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
const Q4W_LOCK_SECONDS: u64 = 17 * DAY_IN_SECONDS;
const MAX_Q4W_ENTRIES: u32 = 20;
const SCALAR_7: i128 = 10_000_000;
const BLND_SCALAR: i128 = SCALAR_7;
const MAX_BACKFILL_BLND: i128 = 10_000_000 * BLND_SCALAR;
const ACTIVATION_ENTRY_THRESHOLD_USDC: i128 = 12_500 * SCALAR_7;
const ACTIVATION_MAINTENANCE_THRESHOLD_USDC: i128 = 10_000 * SCALAR_7;
const STATUS_ADMIN_ACTIVE: u32 = 0;
const STATUS_ACTIVE: u32 = 1;
const STATUS_ADMIN_ON_ICE: u32 = 2;
const STATUS_ON_ICE: u32 = 3;
const STATUS_ADMIN_FROZEN: u32 = 4;
const STATUS_FROZEN: u32 = 5;
const STATUS_SETUP: u32 = 6;
const Q4W_ON_ICE_THRESHOLD: i128 = 3_000_000;
const Q4W_ADMIN_ACTIVE_LIMIT: i128 = 5_000_000;
const Q4W_FROZEN_THRESHOLD: i128 = 6_000_000;
const Q4W_ADMIN_ON_ICE_LIMIT: i128 = 7_500_000;
// Exact integer representation of the relative weights 2.5:2.5:2:2:1.
const TAKE_RATE_WEIGHT_BLND_USDC: i128 = 5;
const TAKE_RATE_WEIGHT_BLND_XLM: i128 = 5;
const TAKE_RATE_WEIGHT_USDC: i128 = 4;
const TAKE_RATE_WEIGHT_CUSTOM_A: i128 = 4;
const TAKE_RATE_WEIGHT_CUSTOM_B: i128 = 2;
const POSITION_TTL_THRESHOLD: u32 = 179 * DAY_IN_LEDGERS;
const POSITION_TTL_BUMP: u32 = 180 * DAY_IN_LEDGERS;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Emitter,
    PoolFactory,
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
    TotalBlndUsdcShares,
    TotalBlndUsdcQueuedShares,
    TotalPrefundShares,
}

#[derive(Clone)]
#[contracttype]
enum PersistentKey {
    MigrationPosition(Address, Address),
    BlndUsdcQ4W(Address, Address),
    PoolBlndUsdcShares(Address),
    PoolBlndUsdcQueuedShares(Address),
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
    InvalidTakeRateValue = 1222,
    InvalidEmissionValue = 1223,
    TooManyQ4WEntries = 1224,
    InsufficientQueuedShares = 1225,
    WithdrawalNotExpired = 1226,
    PoolWithdrawalBlocked = 1227,
    InvalidPool = 1228,
    InvalidPoolFactoryBinding = 1229,
    InvalidPoolStatus = 1230,
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
/// One continuous BLND:USDC tier position plus temporary backfill metadata.
pub struct MigrationPosition {
    pub amount: i128,
    pub frozen_weight: Option<i128>,
    pub time_basis: i128,
}

/// One bounded BLND:USDC withdrawal-queue entry.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Q4WEntry {
    pub amount: i128,
    pub expires_at: u64,
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
    pub custom_a: i128,
    pub custom_b: i128,
    pub usdc: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ActivationQuote {
    pub eligible_value: i128,
    pub meets_threshold: bool,
    pub required_value: i128,
}

/// Arithmetic-only quote for one v2-compatible pool-status transition.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PoolStatusQuote {
    pub eligible_value: i128,
    pub meets_activation_threshold: bool,
    pub q4w_percentage: i128,
    pub required_value: i128,
    pub status: u32,
    pub transition_allowed: bool,
}

/// Reward-eligible, pool-specific tier values denominated in 7-decimal USDC.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TakeRateValues {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub custom_a: i128,
    pub custom_b: i128,
    pub usdc: i128,
}

/// Allocation of one take-rate asset across the five backstop tiers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TakeRateQuote {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub custom_a: i128,
    pub custom_b: i128,
    pub remainder: i128,
    pub usdc: i128,
}

/// Eligible underlying BLND held through the two BLND-bearing backstop tiers,
/// denominated in 7-decimal BLND base units.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BlndEmissionValues {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
}

/// Arithmetic-only allocation quote for one BLND-emission scope.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BlndEmissionQuote {
    pub allocation: i128,
    pub eligible_blnd: i128,
}

#[contractclient(name = "EmitterClient")]
pub trait Emitter {
    fn get_backstop(env: Env) -> Address;
    fn get_queued_swap(env: Env) -> Option<EmitterSwap>;
    fn queue_swap_backstop(env: Env, new_backstop: Address, new_backstop_token: Address);
    fn swap_backstop(env: Env);
}

/// Minimal fail-closed loss-state interface required from an attributed pool.
#[contractclient(name = "PoolLossClient")]
pub trait PoolLoss {
    /// Return true only when this backstop has no committed or unresolved loss
    /// for the pool and its expired queued shares may leave custody.
    fn backstop_withdrawal_allowed(env: Env, backstop: Address) -> bool;
}

/// Immutable registry of pools accepted by this backstop.
#[contractclient(name = "PoolFactoryClient")]
pub trait PoolFactory {
    fn backstop(env: Env) -> Address;
    fn is_pool(env: Env, pool: Address) -> bool;
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

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlndUsdcWithdrawalQueued {
    pub amount: i128,
    pub depositor: Address,
    pub expires_at: u64,
    pub pool: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlndUsdcWithdrawalDequeued {
    pub amount: i128,
    pub depositor: Address,
    pub pool: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlndUsdcWithdrawn {
    pub amount: i128,
    pub depositor: Address,
    pub pool: Address,
    pub to: Address,
}

#[contract]
pub struct BackstopContract;

#[contractimpl]
impl BackstopContract {
    pub fn __constructor(
        env: Env,
        emitter: Address,
        pool_factory: Address,
        legacy_backstop_token: Address,
        next_backstop_token: Address,
        prefund_token: Address,
    ) {
        if legacy_backstop_token == prefund_token {
            panic_with_error!(&env, BackstopError::AssetConfigurationCollision);
        }
        let candidate = env.current_contract_address();
        let pool_factory_client = PoolFactoryClient::new(&env, &pool_factory);
        if pool_factory_client.backstop() != candidate {
            panic_with_error!(&env, BackstopError::InvalidPoolFactoryBinding);
        }
        let _ = pool_factory_client.is_pool(&candidate);
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
            .set(&DataKey::PoolFactory, &pool_factory);
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

    /// Return the immutable registry used to validate attributed pools.
    pub fn pool_factory(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::PoolFactory).unwrap()
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
        if shares > pool_shares {
            panic_with_error!(&env, BackstopError::ArithmeticError);
        }
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

    /// Return one user's total continuous BLND:USDC tier shares.
    ///
    /// Includes shares queued for withdrawal. Before migration these shares
    /// also qualify for migration backfill, and while in custody they remain
    /// eligible for take-rate accounting and proportionally exposed to losses.
    pub fn blnd_usdc_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_position(&env, &user, &pool).amount
    }

    /// Return one user's unqueued share basis for activation and emissions.
    ///
    /// Final eligibility also depends on migration lifecycle, pool status, and
    /// asset-policy gates.
    pub fn blnd_usdc_active_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        let total = get_position(&env, &user, &pool).amount;
        checked_nonnegative_sub_i128(
            &env,
            total,
            queued_shares(&env, &get_blnd_usdc_q4w(&env, &user, &pool)),
        )
    }

    /// Return one user's total queued BLND:USDC shares.
    pub fn blnd_usdc_queued_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        queued_shares(&env, &get_blnd_usdc_q4w(&env, &user, &pool))
    }

    /// Return one user's bounded FIFO withdrawal queue.
    pub fn blnd_usdc_withdrawal_queue(env: Env, user: Address, pool: Address) -> Vec<Q4WEntry> {
        extend_instance_ttl(&env);
        get_blnd_usdc_q4w(&env, &user, &pool)
    }

    /// Return the total accounted BLND:USDC tier shares allocated to one pool.
    ///
    /// Includes queued shares and is the canonical share amount for take-rate
    /// and within-tier loss accounting while those shares remain in custody.
    pub fn pool_blnd_usdc_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_blnd_usdc_shares(&env, &pool)
    }

    /// Return one pool's queued BLND:USDC shares.
    pub fn pool_blnd_usdc_queued_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_blnd_usdc_queued_shares(&env, &pool)
    }

    /// Return one pool's unqueued share basis for activation and emissions.
    ///
    /// Final eligibility also depends on migration lifecycle, pool status, and
    /// asset-policy gates.
    pub fn pool_blnd_usdc_active_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            get_pool_blnd_usdc_shares(&env, &pool),
            get_pool_blnd_usdc_queued_shares(&env, &pool),
        )
    }

    /// Return all accounted BLND:USDC tier shares.
    ///
    /// Includes queued shares and is the canonical share amount for take-rate
    /// and within-tier loss accounting. Direct token transfers do not create
    /// shares and are excluded.
    pub fn total_blnd_usdc_shares(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalBlndUsdcShares)
            .unwrap_or(0)
    }

    /// Return all queued BLND:USDC tier shares.
    pub fn total_blnd_usdc_queued_shares(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalBlndUsdcQueuedShares)
            .unwrap_or(0)
    }

    /// Return the global unqueued share basis for activation and emissions.
    ///
    /// Final eligibility also depends on migration lifecycle, pool status, and
    /// asset-policy gates.
    pub fn total_blnd_usdc_active_shares(env: Env) -> i128 {
        extend_instance_ttl(&env);
        let total: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBlndUsdcShares)
            .unwrap_or(0);
        let queued: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBlndUsdcQueuedShares)
            .unwrap_or(0);
        checked_nonnegative_sub_i128(&env, total, queued)
    }

    /// Return the inherited withdrawal-queue lock duration.
    pub fn q4w_lock_seconds(env: Env) -> u64 {
        extend_instance_ttl(&env);
        Q4W_LOCK_SECONDS
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

    /// Quote the permissionless v2-compatible status refresh.
    ///
    /// Active and queued values MUST use the same canonical, manipulation-
    /// resistant USDC valuation. This function defines arithmetic only; the
    /// production pool remains responsible for storing and enforcing status.
    pub fn quote_update_pool_status(
        env: Env,
        current_status: u32,
        active_values: ActivationValues,
        queued_values: ActivationValues,
    ) -> PoolStatusQuote {
        extend_instance_ttl(&env);
        quote_update_pool_status_values(&env, current_status, &active_values, &queued_values)
    }

    /// Quote a pool-local operator's v2-compatible status request.
    ///
    /// This function performs no authorization and defines arithmetic only.
    /// The production pool MUST require its immutable operator and MUST NOT
    /// grant that operator any protocol-wide or withdrawal-safety authority.
    pub fn quote_set_pool_status(
        env: Env,
        current_status: u32,
        requested_status: u32,
        active_values: ActivationValues,
        queued_values: ActivationValues,
    ) -> PoolStatusQuote {
        extend_instance_ttl(&env);
        quote_set_pool_status_values(
            &env,
            current_status,
            requested_status,
            &active_values,
            &queued_values,
        )
    }

    /// Quote one take-rate asset across the five weighted tiers.
    ///
    /// The result is arithmetic only. Values MUST come from the canonical
    /// reward-eligibility layer before the quote is used for accrual.
    pub fn quote_take_rate(env: Env, distribution: i128, values: TakeRateValues) -> TakeRateQuote {
        extend_instance_ttl(&env);
        quote_take_rate_values(&env, distribution, &values)
    }

    /// Quote an active pool's share of the backstop-depositor BLND tranche.
    ///
    /// `total_active_blnd` is the eligible underlying BLND across every active
    /// pool. Values and active status MUST come from canonical checkpointed
    /// accounting before this arithmetic is used for accrual.
    pub fn quote_pool_blnd_emissions(
        env: Env,
        distribution: i128,
        values: BlndEmissionValues,
        total_active_blnd: i128,
        active: bool,
    ) -> BlndEmissionQuote {
        extend_instance_ttl(&env);
        quote_pool_blnd_emission_values(&env, distribution, &values, total_active_blnd, active)
    }

    /// Quote one user's share of an active pool's BLND-emission allocation.
    ///
    /// `pool_eligible_blnd` is the eligible underlying BLND held through both
    /// BLND-bearing tiers for the pool. Values MUST come from canonical
    /// checkpointed accounting before this arithmetic is used for accrual.
    pub fn quote_user_blnd_emissions(
        env: Env,
        pool_distribution: i128,
        values: BlndEmissionValues,
        pool_eligible_blnd: i128,
    ) -> BlndEmissionQuote {
        extend_instance_ttl(&env);
        quote_user_blnd_emission_values(&env, pool_distribution, &values, pool_eligible_blnd)
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

    /// Deposit BLND:USDC into its continuous pool-attributed backstop tier.
    ///
    /// Before migration, these same shares also count toward legacy-emitter
    /// qualification and earn amount-times-time backfill weight. Deposits after
    /// migration are ordinary tier shares and earn no retroactive backfill.
    pub fn deposit_blnd_usdc(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_blnd_usdc_position(&env, from, pool, amount);
    }

    /// Compatibility alias for `deposit_blnd_usdc`.
    pub fn deposit_migration(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_blnd_usdc_position(&env, from, pool, amount);
    }

    /// Queue active BLND:USDC shares for withdrawal after the 17-day lock.
    pub fn queue_blnd_usdc_withdrawal(
        env: Env,
        from: Address,
        pool: Address,
        amount: i128,
    ) -> Q4WEntry {
        extend_instance_ttl(&env);
        queue_blnd_usdc_withdrawal(&env, from, pool, amount)
    }

    /// Return queued BLND:USDC shares to active status, newest entries first.
    pub fn dequeue_blnd_usdc_withdrawal(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        dequeue_blnd_usdc_withdrawal(&env, from, pool, amount);
    }

    /// Withdraw expired queued BLND:USDC tier shares, oldest entries first.
    ///
    /// Before emitter migration succeeds, withdrawn shares forfeit their
    /// proportional accumulated backfill weight. The attributed pool must
    /// affirm that this backstop has no committed or unresolved loss; a failed,
    /// missing, or false pool response blocks withdrawal.
    pub fn withdraw_blnd_usdc(env: Env, from: Address, pool: Address, amount: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_blnd_usdc_position(&env, from, pool, amount, to);
    }

    /// Compatibility alias for the queued `withdraw_blnd_usdc` operation.
    pub fn withdraw_migration(env: Env, from: Address, pool: Address, amount: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_blnd_usdc_position(&env, from, pool, amount, to);
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

fn deposit_blnd_usdc_position(env: &Env, from: Address, pool: Address, amount: i128) {
    require_positive(env, amount);
    from.require_auth();
    require_compatible_pool(env, &pool);

    // Keep companion withdrawal state live whenever the position is touched.
    get_blnd_usdc_q4w(env, &from, &pool);
    let backfill_end = env
        .storage()
        .instance()
        .get::<DataKey, u64>(&DataKey::BackfillEnd);
    let mut position = get_position(env, &from, &pool);
    if let Some(backfill_end) = backfill_end {
        if position.frozen_weight.is_none() {
            position.frozen_weight = Some(position_weight(env, &position, backfill_end));
        }
    } else {
        try_open_epoch_from_current_queue(env);
        let timestamp = eligibility_timestamp(env, env.ledger().timestamp());
        let time_basis_delta = checked_mul_i128(env, amount, timestamp as i128);
        position.time_basis = checked_add_i128(env, position.time_basis, time_basis_delta);
        add_to_instance_total(env, DataKey::TotalMigrationAmount, amount);
        add_to_instance_total(env, DataKey::TotalMigrationTimeBasis, time_basis_delta);
    }
    position.amount = checked_add_i128(env, position.amount, amount);
    set_position(env, &from, &pool, &position);

    let pool_shares = get_pool_blnd_usdc_shares(env, &pool);
    set_pool_blnd_usdc_shares(env, &pool, checked_add_i128(env, pool_shares, amount));
    add_to_instance_total(env, DataKey::TotalBlndUsdcShares, amount);

    TokenClient::new(env, &legacy_backstop_token(env)).transfer(
        &from,
        &env.current_contract_address(),
        &amount,
    );
}

fn require_compatible_pool(env: &Env, pool: &Address) {
    let factory: Address = env.storage().instance().get(&DataKey::PoolFactory).unwrap();
    if !PoolFactoryClient::new(env, &factory).is_pool(pool) {
        panic_with_error!(env, BackstopError::InvalidPool);
    }

    // Probe interface availability before accepting custody. Either boolean is
    // valid here: an existing loss may block withdrawal without blocking new
    // capital. A missing or failed callback aborts the deposit atomically.
    let backstop = env.current_contract_address();
    let _ = PoolLossClient::new(env, pool).backstop_withdrawal_allowed(&backstop);
}

fn queue_blnd_usdc_withdrawal(env: &Env, from: Address, pool: Address, amount: i128) -> Q4WEntry {
    require_positive(env, amount);
    from.require_auth();

    let position = get_position(env, &from, &pool);
    let mut queue = get_blnd_usdc_q4w(env, &from, &pool);
    let already_queued = queued_shares(env, &queue);
    let active_shares = checked_nonnegative_sub_i128(env, position.amount, already_queued);
    if amount > active_shares {
        panic_with_error!(env, BackstopError::InsufficientBalance);
    }
    if queue.len() >= MAX_Q4W_ENTRIES {
        panic_with_error!(env, BackstopError::TooManyQ4WEntries);
    }

    let expires_at = env
        .ledger()
        .timestamp()
        .checked_add(Q4W_LOCK_SECONDS)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    let entry = Q4WEntry { amount, expires_at };
    queue.push_back(entry.clone());
    set_blnd_usdc_q4w(env, &from, &pool, &queue);
    add_pool_blnd_usdc_queued_shares(env, &pool, amount);
    add_to_instance_total(env, DataKey::TotalBlndUsdcQueuedShares, amount);

    BlndUsdcWithdrawalQueued {
        amount,
        depositor: from,
        expires_at,
        pool,
    }
    .publish(env);
    entry
}

fn dequeue_blnd_usdc_withdrawal(env: &Env, from: Address, pool: Address, amount: i128) {
    require_positive(env, amount);
    from.require_auth();

    let mut queue = get_blnd_usdc_q4w(env, &from, &pool);
    let mut remaining = amount;
    while remaining > 0 {
        let mut entry = queue
            .pop_back()
            .unwrap_or_else(|| panic_with_error!(env, BackstopError::InsufficientQueuedShares));
        if entry.amount > remaining {
            entry.amount = checked_sub_i128(env, entry.amount, remaining);
            queue.push_back(entry);
            remaining = 0;
        } else {
            remaining = checked_sub_i128(env, remaining, entry.amount);
        }
    }
    set_blnd_usdc_q4w(env, &from, &pool, &queue);
    add_pool_blnd_usdc_queued_shares(env, &pool, -amount);
    add_to_instance_total(env, DataKey::TotalBlndUsdcQueuedShares, -amount);

    BlndUsdcWithdrawalDequeued {
        amount,
        depositor: from,
        pool,
    }
    .publish(env);
}

fn withdraw_blnd_usdc_position(env: &Env, from: Address, pool: Address, amount: i128, to: Address) {
    require_positive(env, amount);
    from.require_auth();

    let backstop = env.current_contract_address();
    if !PoolLossClient::new(env, &pool).backstop_withdrawal_allowed(&backstop) {
        panic_with_error!(env, BackstopError::PoolWithdrawalBlocked);
    }
    consume_expired_blnd_usdc_q4w(env, &from, &pool, amount);
    let mut position = get_position(env, &from, &pool);
    if amount > position.amount {
        panic_with_error!(env, BackstopError::InsufficientBalance);
    }

    if let Some(backfill_end) = env
        .storage()
        .instance()
        .get::<DataKey, u64>(&DataKey::BackfillEnd)
    {
        if position.frozen_weight.is_none() {
            position.frozen_weight = Some(position_weight(env, &position, backfill_end));
        }
    } else {
        let basis_removed = proportional_floor(env, position.time_basis, amount, position.amount);
        position.time_basis = checked_sub_i128(env, position.time_basis, basis_removed);
        add_to_instance_total(env, DataKey::TotalMigrationAmount, -amount);
        add_to_instance_total(env, DataKey::TotalMigrationTimeBasis, -basis_removed);
    }

    position.amount = checked_sub_i128(env, position.amount, amount);
    set_position(env, &from, &pool, &position);
    let pool_shares = get_pool_blnd_usdc_shares(env, &pool);
    if amount > pool_shares {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    set_pool_blnd_usdc_shares(env, &pool, checked_sub_i128(env, pool_shares, amount));
    add_pool_blnd_usdc_queued_shares(env, &pool, -amount);
    add_to_instance_total(env, DataKey::TotalBlndUsdcShares, -amount);
    add_to_instance_total(env, DataKey::TotalBlndUsdcQueuedShares, -amount);

    TokenClient::new(env, &legacy_backstop_token(env)).transfer(
        &env.current_contract_address(),
        &to,
        &amount,
    );
    BlndUsdcWithdrawn {
        amount,
        depositor: from,
        pool,
        to,
    }
    .publish(env);
}

fn consume_expired_blnd_usdc_q4w(env: &Env, user: &Address, pool: &Address, amount: i128) {
    let mut queue = get_blnd_usdc_q4w(env, user, pool);
    let mut remaining = amount;
    let timestamp = env.ledger().timestamp();
    while remaining > 0 {
        let mut entry = queue
            .pop_front()
            .unwrap_or_else(|| panic_with_error!(env, BackstopError::InsufficientQueuedShares));
        if entry.expires_at > timestamp {
            panic_with_error!(env, BackstopError::WithdrawalNotExpired);
        }
        if entry.amount > remaining {
            entry.amount = checked_sub_i128(env, entry.amount, remaining);
            queue.push_front(entry);
            remaining = 0;
        } else {
            remaining = checked_sub_i128(env, remaining, entry.amount);
        }
    }
    set_blnd_usdc_q4w(env, user, pool, &queue);
}

fn get_position(env: &Env, user: &Address, pool: &Address) -> MigrationPosition {
    let key = PersistentKey::MigrationPosition(user.clone(), pool.clone());
    let q4w_key = PersistentKey::BlndUsdcQ4W(user.clone(), pool.clone());
    let position = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(MigrationPosition {
            amount: 0,
            frozen_weight: None,
            time_basis: 0,
        });
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &q4w_key);
    position
}

fn get_blnd_usdc_q4w(env: &Env, user: &Address, pool: &Address) -> Vec<Q4WEntry> {
    let key = PersistentKey::BlndUsdcQ4W(user.clone(), pool.clone());
    let position_key = PersistentKey::MigrationPosition(user.clone(), pool.clone());
    let queue = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &position_key);
    queue
}

fn set_blnd_usdc_q4w(env: &Env, user: &Address, pool: &Address, queue: &Vec<Q4WEntry>) {
    let key = PersistentKey::BlndUsdcQ4W(user.clone(), pool.clone());
    env.storage().persistent().set(&key, queue);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn queued_shares(env: &Env, queue: &Vec<Q4WEntry>) -> i128 {
    let mut total = 0;
    for entry in queue.iter() {
        total = checked_add_i128(env, total, entry.amount);
    }
    total
}

fn get_pool_blnd_usdc_shares(env: &Env, pool: &Address) -> i128 {
    let key = PersistentKey::PoolBlndUsdcShares(pool.clone());
    let queued_key = PersistentKey::PoolBlndUsdcQueuedShares(pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &queued_key);
    shares
}

fn get_pool_blnd_usdc_queued_shares(env: &Env, pool: &Address) -> i128 {
    let key = PersistentKey::PoolBlndUsdcQueuedShares(pool.clone());
    let shares_key = PersistentKey::PoolBlndUsdcShares(pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &shares_key);
    shares
}

fn add_pool_blnd_usdc_queued_shares(env: &Env, pool: &Address, delta: i128) {
    let key = PersistentKey::PoolBlndUsdcQueuedShares(pool.clone());
    let current = get_pool_blnd_usdc_queued_shares(env, pool);
    let next = checked_add_i128(env, current, delta);
    if next < 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    env.storage().persistent().set(&key, &next);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn set_pool_blnd_usdc_shares(env: &Env, pool: &Address, shares: i128) {
    let key = PersistentKey::PoolBlndUsdcShares(pool.clone());
    env.storage().persistent().set(&key, &shares);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
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
    let next = checked_add_i128(env, current, delta);
    if next < 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    env.storage().instance().set(&key, &next);
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
    let eligible_value = sum_activation_values(env, values);
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

fn sum_activation_values(env: &Env, values: &ActivationValues) -> i128 {
    if values.blnd_xlm < 0
        || values.blnd_usdc < 0
        || values.usdc < 0
        || values.custom_a < 0
        || values.custom_b < 0
    {
        panic_with_error!(env, BackstopError::InvalidActivationValue);
    }
    checked_add_i128(
        env,
        checked_add_i128(
            env,
            checked_add_i128(
                env,
                checked_add_i128(env, values.blnd_xlm, values.blnd_usdc),
                values.usdc,
            ),
            values.custom_a,
        ),
        values.custom_b,
    )
}

fn quote_update_pool_status_values(
    env: &Env,
    current_status: u32,
    active_values: &ActivationValues,
    queued_values: &ActivationValues,
) -> PoolStatusQuote {
    require_valid_pool_status(env, current_status);
    let currently_active = current_status == STATUS_ADMIN_ACTIVE || current_status == STATUS_ACTIVE;
    let activation = quote_activation_values(env, active_values, currently_active);
    let q4w_percentage = calculate_q4w_percentage(env, active_values, queued_values);

    let (status, transition_allowed) = match current_status {
        STATUS_SETUP | STATUS_ADMIN_FROZEN => (current_status, false),
        STATUS_ADMIN_ON_ICE => {
            if q4w_percentage >= Q4W_ADMIN_ON_ICE_LIMIT {
                (STATUS_FROZEN, true)
            } else {
                (STATUS_ADMIN_ON_ICE, true)
            }
        }
        STATUS_ADMIN_ACTIVE => {
            if !activation.meets_threshold || q4w_percentage >= Q4W_ADMIN_ACTIVE_LIMIT {
                (STATUS_ON_ICE, true)
            } else {
                (STATUS_ADMIN_ACTIVE, true)
            }
        }
        STATUS_ACTIVE | STATUS_ON_ICE | STATUS_FROZEN => {
            if q4w_percentage >= Q4W_FROZEN_THRESHOLD {
                (STATUS_FROZEN, true)
            } else if !activation.meets_threshold || q4w_percentage >= Q4W_ON_ICE_THRESHOLD {
                (STATUS_ON_ICE, true)
            } else {
                (STATUS_ACTIVE, true)
            }
        }
        _ => panic_with_error!(env, BackstopError::InvalidPoolStatus),
    };

    PoolStatusQuote {
        eligible_value: activation.eligible_value,
        meets_activation_threshold: activation.meets_threshold,
        q4w_percentage,
        required_value: activation.required_value,
        status,
        transition_allowed,
    }
}

fn quote_set_pool_status_values(
    env: &Env,
    current_status: u32,
    requested_status: u32,
    active_values: &ActivationValues,
    queued_values: &ActivationValues,
) -> PoolStatusQuote {
    require_valid_pool_status(env, current_status);
    let currently_active = current_status == STATUS_ADMIN_ACTIVE || current_status == STATUS_ACTIVE;
    let activation = quote_activation_values(env, active_values, currently_active);
    let q4w_percentage = calculate_q4w_percentage(env, active_values, queued_values);
    let transition_allowed = match requested_status {
        STATUS_ADMIN_ACTIVE => {
            activation.meets_threshold && q4w_percentage < Q4W_ADMIN_ACTIVE_LIMIT
        }
        STATUS_ADMIN_ON_ICE | STATUS_ON_ICE => q4w_percentage < Q4W_ADMIN_ON_ICE_LIMIT,
        STATUS_ADMIN_FROZEN => true,
        _ => panic_with_error!(env, BackstopError::InvalidPoolStatus),
    };

    PoolStatusQuote {
        eligible_value: activation.eligible_value,
        meets_activation_threshold: activation.meets_threshold,
        q4w_percentage,
        required_value: activation.required_value,
        status: if transition_allowed {
            requested_status
        } else {
            current_status
        },
        transition_allowed,
    }
}

fn calculate_q4w_percentage(
    env: &Env,
    active_values: &ActivationValues,
    queued_values: &ActivationValues,
) -> i128 {
    let active_value = sum_activation_values(env, active_values);
    let queued_value = sum_activation_values(env, queued_values);
    let total_value = checked_add_i128(env, active_value, queued_value);
    if total_value == 0 {
        return 0;
    }

    let numerator = I256::from_i128(env, queued_value).mul(&I256::from_i128(env, SCALAR_7));
    let denominator = I256::from_i128(env, total_value);
    numerator
        .add(&denominator)
        .sub(&I256::from_i32(env, 1))
        .div(&denominator)
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn require_valid_pool_status(env: &Env, status: u32) {
    if status > STATUS_SETUP {
        panic_with_error!(env, BackstopError::InvalidPoolStatus);
    }
}

fn quote_take_rate_values(env: &Env, distribution: i128, values: &TakeRateValues) -> TakeRateQuote {
    if distribution < 0
        || values.blnd_usdc < 0
        || values.blnd_xlm < 0
        || values.usdc < 0
        || values.custom_a < 0
        || values.custom_b < 0
    {
        panic_with_error!(env, BackstopError::InvalidTakeRateValue);
    }

    let blnd_usdc_weighted = checked_mul_i128(env, values.blnd_usdc, TAKE_RATE_WEIGHT_BLND_USDC);
    let blnd_xlm_weighted = checked_mul_i128(env, values.blnd_xlm, TAKE_RATE_WEIGHT_BLND_XLM);
    let usdc_weighted = checked_mul_i128(env, values.usdc, TAKE_RATE_WEIGHT_USDC);
    let custom_a_weighted = checked_mul_i128(env, values.custom_a, TAKE_RATE_WEIGHT_CUSTOM_A);
    let custom_b_weighted = checked_mul_i128(env, values.custom_b, TAKE_RATE_WEIGHT_CUSTOM_B);
    let denominator = checked_add_i128(
        env,
        checked_add_i128(
            env,
            checked_add_i128(
                env,
                checked_add_i128(env, blnd_usdc_weighted, blnd_xlm_weighted),
                usdc_weighted,
            ),
            custom_a_weighted,
        ),
        custom_b_weighted,
    );

    if denominator == 0 {
        return TakeRateQuote {
            blnd_usdc: 0,
            blnd_xlm: 0,
            custom_a: 0,
            custom_b: 0,
            remainder: distribution,
            usdc: 0,
        };
    }

    let blnd_usdc = proportional_floor(env, distribution, blnd_usdc_weighted, denominator);
    let blnd_xlm = proportional_floor(env, distribution, blnd_xlm_weighted, denominator);
    let usdc = proportional_floor(env, distribution, usdc_weighted, denominator);
    let custom_a = proportional_floor(env, distribution, custom_a_weighted, denominator);
    let custom_b = proportional_floor(env, distribution, custom_b_weighted, denominator);
    let allocated = checked_add_i128(
        env,
        checked_add_i128(
            env,
            checked_add_i128(env, checked_add_i128(env, blnd_usdc, blnd_xlm), usdc),
            custom_a,
        ),
        custom_b,
    );

    TakeRateQuote {
        blnd_usdc,
        blnd_xlm,
        custom_a,
        custom_b,
        remainder: checked_sub_i128(env, distribution, allocated),
        usdc,
    }
}

fn eligible_blnd(env: &Env, values: &BlndEmissionValues) -> i128 {
    if values.blnd_usdc < 0 || values.blnd_xlm < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    checked_add_i128(env, values.blnd_usdc, values.blnd_xlm)
}

fn quote_pool_blnd_emission_values(
    env: &Env,
    distribution: i128,
    values: &BlndEmissionValues,
    total_active_blnd: i128,
    active: bool,
) -> BlndEmissionQuote {
    if distribution < 0 || total_active_blnd < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let pool_blnd = eligible_blnd(env, values);
    if !active {
        return BlndEmissionQuote {
            allocation: 0,
            eligible_blnd: pool_blnd,
        };
    }
    if pool_blnd > total_active_blnd || (pool_blnd > 0 && total_active_blnd == 0) {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let allocation = if pool_blnd == 0 {
        0
    } else {
        proportional_floor(env, distribution, pool_blnd, total_active_blnd)
    };
    BlndEmissionQuote {
        allocation,
        eligible_blnd: pool_blnd,
    }
}

fn quote_user_blnd_emission_values(
    env: &Env,
    pool_distribution: i128,
    values: &BlndEmissionValues,
    pool_eligible_blnd: i128,
) -> BlndEmissionQuote {
    if pool_distribution < 0 || pool_eligible_blnd < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let user_blnd = eligible_blnd(env, values);
    if user_blnd > pool_eligible_blnd || (user_blnd > 0 && pool_eligible_blnd == 0) {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let allocation = if user_blnd == 0 {
        0
    } else {
        proportional_floor(env, pool_distribution, user_blnd, pool_eligible_blnd)
    };
    BlndEmissionQuote {
        allocation,
        eligible_blnd: user_blnd,
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

fn checked_nonnegative_sub_i128(env: &Env, left: i128, right: i128) -> i128 {
    let difference = checked_sub_i128(env, left, right);
    if difference < 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    difference
}

fn checked_mul_i128(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_mul(right)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn extend_persistent_ttl_if_present(env: &Env, key: &PersistentKey) {
    if env.storage().persistent().has(key) {
        env.storage()
            .persistent()
            .extend_ttl(key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
    }
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
    struct MockPoolFactory;

    #[derive(Clone)]
    #[contracttype]
    enum MockPoolFactoryKey {
        Backstop,
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

        pub fn is_pool(_env: Env, _pool: Address) -> bool {
            false
        }
    }

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
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);

        assert_eq!(candidate.backstop_token(), legacy_token);
        assert_eq!(candidate.pool_factory(), pool_factory);
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
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
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
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
        );
        let user = Address::generate(&env);
        let pool = Address::generate(&env);

        assert!(BackstopContractClient::new(&env, &candidate_id)
            .try_deposit_migration(&user, &pool, &1)
            .is_err());
    }

    #[test]
    fn blnd_usdc_q4w_operations_require_user_authorization() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);
        let user = Address::generate(&env);
        let recipient = Address::generate(&env);
        let pool = Address::generate(&env);

        assert!(candidate
            .try_queue_blnd_usdc_withdrawal(&user, &pool, &1)
            .is_err());
        assert!(candidate
            .try_dequeue_blnd_usdc_withdrawal(&user, &pool, &1)
            .is_err());
        assert!(candidate
            .try_withdraw_blnd_usdc(&user, &pool, &1, &recipient)
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
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
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
        let pool_factory = Address::generate(&env);
        let candidate = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (&emitter, &pool_factory, &token, &next_token, &token),
        );
    }

    #[test]
    #[should_panic]
    fn constructor_rejects_incompatible_pool_factory() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let pool_factory = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1229)")]
    fn constructor_rejects_pool_factory_bound_to_another_backstop() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let candidate = Address::generate(&env);
        let other_backstop = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&other_backstop,));
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
            ),
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
    fn activation_policy_counts_five_assets_equally() {
        let env = Env::default();
        let mixed = ActivationValues {
            blnd_xlm: 3_000 * SCALAR_7,
            blnd_usdc: 3_000 * SCALAR_7,
            usdc: 2_500 * SCALAR_7,
            custom_a: 2_000 * SCALAR_7,
            custom_b: 2_000 * SCALAR_7,
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
                custom_a: 0,
                custom_b: 0,
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
                custom_a: 0,
                custom_b: 0,
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
                custom_a: 0,
                custom_b: 0,
                usdc: 0,
            },
            false,
        );
        assert!(blnd_usdc_only.meets_threshold);

        let custom_a_only = quote_activation_values(
            &env,
            &ActivationValues {
                blnd_xlm: 0,
                blnd_usdc: 0,
                custom_a: ACTIVATION_ENTRY_THRESHOLD_USDC,
                custom_b: 0,
                usdc: 0,
            },
            false,
        );
        assert!(custom_a_only.meets_threshold);

        let custom_b_only = quote_activation_values(
            &env,
            &ActivationValues {
                blnd_xlm: 0,
                blnd_usdc: 0,
                custom_a: 0,
                custom_b: ACTIVATION_ENTRY_THRESHOLD_USDC,
                usdc: 0,
            },
            false,
        );
        assert!(custom_b_only.meets_threshold);
    }

    #[test]
    fn activation_policy_applies_entry_and_maintenance_hysteresis() {
        let env = Env::default();
        let value = |amount| ActivationValues {
            blnd_xlm: amount,
            blnd_usdc: 0,
            custom_a: 0,
            custom_b: 0,
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
                    custom_a: 0,
                    custom_b: 0,
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
                    custom_a: 0,
                    custom_b: 0,
                    usdc: 0,
                },
                false,
            )
        }));
        assert!(overflow.is_err());
    }

    #[test]
    fn pool_status_policy_applies_v2_q4w_thresholds() {
        let env = Env::default();
        let values = |amount| ActivationValues {
            blnd_xlm: amount,
            blnd_usdc: 0,
            custom_a: 0,
            custom_b: 0,
            usdc: 0,
        };
        let total = 20_000 * SCALAR_7;
        let quote = |status, q4w_percentage| {
            let queued = total * q4w_percentage / SCALAR_7;
            let result = quote_update_pool_status_values(
                &env,
                status,
                &values(total - queued),
                &values(queued),
            );
            assert_eq!(result.q4w_percentage, q4w_percentage);
            result
        };

        assert_eq!(
            quote(STATUS_ACTIVE, Q4W_ON_ICE_THRESHOLD - 1).status,
            STATUS_ACTIVE
        );
        assert_eq!(
            quote(STATUS_ACTIVE, Q4W_ON_ICE_THRESHOLD).status,
            STATUS_ON_ICE
        );
        assert_eq!(
            quote(STATUS_ACTIVE, Q4W_FROZEN_THRESHOLD - 1).status,
            STATUS_ON_ICE
        );
        assert_eq!(
            quote(STATUS_ACTIVE, Q4W_FROZEN_THRESHOLD).status,
            STATUS_FROZEN
        );

        assert_eq!(
            quote(STATUS_ADMIN_ACTIVE, Q4W_ADMIN_ACTIVE_LIMIT - 1).status,
            STATUS_ADMIN_ACTIVE
        );
        assert_eq!(
            quote(STATUS_ADMIN_ACTIVE, Q4W_ADMIN_ACTIVE_LIMIT).status,
            STATUS_ON_ICE
        );
        assert_eq!(
            quote(STATUS_ADMIN_ON_ICE, Q4W_ADMIN_ON_ICE_LIMIT - 1).status,
            STATUS_ADMIN_ON_ICE
        );
        assert_eq!(
            quote(STATUS_ADMIN_ON_ICE, Q4W_ADMIN_ON_ICE_LIMIT).status,
            STATUS_FROZEN
        );
    }

    #[test]
    fn pool_status_policy_preserves_activation_hysteresis() {
        let env = Env::default();
        let values = |amount| ActivationValues {
            blnd_xlm: 0,
            blnd_usdc: amount,
            custom_a: 0,
            custom_b: 0,
            usdc: 0,
        };
        let zero = values(0);

        let active_at_maintenance = quote_update_pool_status_values(
            &env,
            STATUS_ACTIVE,
            &values(ACTIVATION_MAINTENANCE_THRESHOLD_USDC),
            &zero,
        );
        assert_eq!(active_at_maintenance.status, STATUS_ACTIVE);
        assert_eq!(
            active_at_maintenance.required_value,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC
        );

        let active_below_maintenance = quote_update_pool_status_values(
            &env,
            STATUS_ACTIVE,
            &values(ACTIVATION_MAINTENANCE_THRESHOLD_USDC - 1),
            &zero,
        );
        assert_eq!(active_below_maintenance.status, STATUS_ON_ICE);

        let inactive_in_band = quote_update_pool_status_values(
            &env,
            STATUS_ON_ICE,
            &values(ACTIVATION_MAINTENANCE_THRESHOLD_USDC),
            &zero,
        );
        assert_eq!(inactive_in_band.status, STATUS_ON_ICE);
        assert_eq!(
            inactive_in_band.required_value,
            ACTIVATION_ENTRY_THRESHOLD_USDC
        );

        let inactive_at_entry = quote_update_pool_status_values(
            &env,
            STATUS_ON_ICE,
            &values(ACTIVATION_ENTRY_THRESHOLD_USDC),
            &zero,
        );
        assert_eq!(inactive_at_entry.status, STATUS_ACTIVE);
    }

    #[test]
    fn pool_status_policy_bounds_pool_operator_overrides() {
        let env = Env::default();
        let values = |amount| ActivationValues {
            blnd_xlm: 0,
            blnd_usdc: 0,
            custom_a: 0,
            custom_b: 0,
            usdc: amount,
        };
        let total = 20_000 * SCALAR_7;
        let quote = |current, requested, queued_pct| {
            let queued = total * queued_pct / 100;
            quote_set_pool_status_values(
                &env,
                current,
                requested,
                &values(total - queued),
                &values(queued),
            )
        };

        let ordinary_at_37 = quote_update_pool_status_values(
            &env,
            STATUS_ON_ICE,
            &values(63 * total / 100),
            &values(37 * total / 100),
        );
        assert_eq!(ordinary_at_37.status, STATUS_ON_ICE);

        let active_allowed = quote(STATUS_ON_ICE, STATUS_ADMIN_ACTIVE, 37);
        assert!(active_allowed.transition_allowed);
        assert_eq!(active_allowed.status, STATUS_ADMIN_ACTIVE);

        let active_blocked = quote(STATUS_ON_ICE, STATUS_ADMIN_ACTIVE, 50);
        assert!(!active_blocked.transition_allowed);
        assert_eq!(active_blocked.status, STATUS_ON_ICE);

        let on_ice_allowed = quote(STATUS_ACTIVE, STATUS_ADMIN_ON_ICE, 74);
        assert!(on_ice_allowed.transition_allowed);
        assert_eq!(on_ice_allowed.status, STATUS_ADMIN_ON_ICE);

        let on_ice_blocked = quote(STATUS_ACTIVE, STATUS_ADMIN_ON_ICE, 75);
        assert!(!on_ice_blocked.transition_allowed);
        assert_eq!(on_ice_blocked.status, STATUS_ACTIVE);

        let relinquished_override = quote(STATUS_ADMIN_ON_ICE, STATUS_ON_ICE, 74);
        assert!(relinquished_override.transition_allowed);
        assert_eq!(relinquished_override.status, STATUS_ON_ICE);

        let frozen = quote(STATUS_ACTIVE, STATUS_ADMIN_FROZEN, 100);
        assert!(frozen.transition_allowed);
        assert_eq!(frozen.status, STATUS_ADMIN_FROZEN);
    }

    #[test]
    fn pool_status_policy_uses_usdc_value_and_rounds_q4w_up() {
        let env = Env::default();
        let active = ActivationValues {
            blnd_xlm: 1,
            blnd_usdc: 1,
            custom_a: 0,
            custom_b: 0,
            usdc: 0,
        };
        let queued = ActivationValues {
            blnd_xlm: 0,
            blnd_usdc: 0,
            custom_a: 0,
            custom_b: 1,
            usdc: 0,
        };
        let quote = quote_update_pool_status_values(&env, STATUS_ACTIVE, &active, &queued);
        assert_eq!(quote.q4w_percentage, 3_333_334);

        let blocked = quote_update_pool_status_values(&env, STATUS_ADMIN_FROZEN, &active, &queued);
        assert!(!blocked.transition_allowed);
        assert_eq!(blocked.status, STATUS_ADMIN_FROZEN);

        let setup = quote_update_pool_status_values(&env, STATUS_SETUP, &active, &queued);
        assert!(!setup.transition_allowed);
        assert_eq!(setup.status, STATUS_SETUP);
    }

    #[test]
    fn take_rate_policy_applies_2_5_2_5_2_2_1_weights_and_conserves_assets() {
        let env = Env::default();
        let equal_values = TakeRateValues {
            blnd_usdc: SCALAR_7,
            blnd_xlm: SCALAR_7,
            custom_a: SCALAR_7,
            custom_b: SCALAR_7,
            usdc: SCALAR_7,
        };
        let quote = quote_take_rate_values(&env, 200, &equal_values);
        assert_eq!(quote.blnd_usdc, 50);
        assert_eq!(quote.blnd_xlm, 50);
        assert_eq!(quote.usdc, 40);
        assert_eq!(quote.custom_a, 40);
        assert_eq!(quote.custom_b, 20);
        assert_eq!(quote.remainder, 0);

        let rounding_quote = quote_take_rate_values(&env, 101, &equal_values);
        assert_eq!(
            rounding_quote.blnd_usdc
                + rounding_quote.blnd_xlm
                + rounding_quote.usdc
                + rounding_quote.custom_a
                + rounding_quote.custom_b
                + rounding_quote.remainder,
            101
        );
        assert_eq!(rounding_quote.remainder, 1);

        let unequal_values = TakeRateValues {
            blnd_usdc: 2 * SCALAR_7,
            blnd_xlm: SCALAR_7,
            custom_a: SCALAR_7,
            custom_b: SCALAR_7,
            usdc: SCALAR_7,
        };
        let unequal_quote = quote_take_rate_values(&env, 250, &unequal_values);
        assert_eq!(unequal_quote.blnd_usdc, 100);
        assert_eq!(unequal_quote.blnd_xlm, 50);
        assert_eq!(unequal_quote.usdc, 40);
        assert_eq!(unequal_quote.custom_a, 40);
        assert_eq!(unequal_quote.custom_b, 20);
        assert_eq!(unequal_quote.remainder, 0);
    }

    #[test]
    fn take_rate_policy_omits_empty_optional_tiers_and_carries_empty_distribution() {
        let env = Env::default();
        let required_only = TakeRateValues {
            blnd_usdc: SCALAR_7,
            blnd_xlm: SCALAR_7,
            custom_a: 0,
            custom_b: 0,
            usdc: 0,
        };
        let quote = quote_take_rate_values(&env, 10, &required_only);
        assert_eq!(quote.blnd_usdc, 5);
        assert_eq!(quote.blnd_xlm, 5);
        assert_eq!(quote.custom_a, 0);
        assert_eq!(quote.custom_b, 0);
        assert_eq!(quote.remainder, 0);

        let empty = quote_take_rate_values(
            &env,
            10,
            &TakeRateValues {
                blnd_usdc: 0,
                blnd_xlm: 0,
                custom_a: 0,
                custom_b: 0,
                usdc: 0,
            },
        );
        assert_eq!(empty.remainder, 10);
    }

    #[test]
    fn take_rate_policy_rejects_negative_values_and_overflow() {
        let env = Env::default();
        let negative = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_take_rate_values(
                &env,
                1,
                &TakeRateValues {
                    blnd_usdc: 0,
                    blnd_xlm: 0,
                    custom_a: 0,
                    custom_b: -1,
                    usdc: 0,
                },
            )
        }));
        assert!(negative.is_err());

        let negative_distribution = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_take_rate_values(
                &env,
                -1,
                &TakeRateValues {
                    blnd_usdc: 0,
                    blnd_xlm: 0,
                    custom_a: 0,
                    custom_b: 0,
                    usdc: 0,
                },
            )
        }));
        assert!(negative_distribution.is_err());

        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_take_rate_values(
                &env,
                1,
                &TakeRateValues {
                    blnd_usdc: i128::MAX,
                    blnd_xlm: 0,
                    custom_a: 0,
                    custom_b: 0,
                    usdc: 0,
                },
            )
        }));
        assert!(overflow.is_err());
    }

    #[test]
    fn blnd_emissions_allocate_active_pool_tranche_by_underlying_blnd() {
        let env = Env::default();
        let pool_values = BlndEmissionValues {
            blnd_usdc: 60 * SCALAR_7,
            blnd_xlm: 40 * SCALAR_7,
        };
        let quote = quote_pool_blnd_emission_values(
            &env,
            1_000 * SCALAR_7,
            &pool_values,
            400 * SCALAR_7,
            true,
        );
        assert_eq!(quote.eligible_blnd, 100 * SCALAR_7);
        assert_eq!(quote.allocation, 250 * SCALAR_7);

        let inactive = quote_pool_blnd_emission_values(
            &env,
            1_000 * SCALAR_7,
            &pool_values,
            300 * SCALAR_7,
            false,
        );
        assert_eq!(inactive.eligible_blnd, 100 * SCALAR_7);
        assert_eq!(inactive.allocation, 0);

        let empty = quote_pool_blnd_emission_values(
            &env,
            1_000 * SCALAR_7,
            &BlndEmissionValues {
                blnd_usdc: 0,
                blnd_xlm: 0,
            },
            300 * SCALAR_7,
            true,
        );
        assert_eq!(empty.eligible_blnd, 0);
        assert_eq!(empty.allocation, 0);
    }

    #[test]
    fn blnd_emissions_allocate_users_across_both_eligible_tiers() {
        let env = Env::default();
        let user_values = BlndEmissionValues {
            blnd_usdc: 15 * SCALAR_7,
            blnd_xlm: 10 * SCALAR_7,
        };
        let quote =
            quote_user_blnd_emission_values(&env, 200 * SCALAR_7, &user_values, 100 * SCALAR_7);
        assert_eq!(quote.eligible_blnd, 25 * SCALAR_7);
        assert_eq!(quote.allocation, 50 * SCALAR_7);

        let empty = quote_user_blnd_emission_values(
            &env,
            200 * SCALAR_7,
            &BlndEmissionValues {
                blnd_usdc: 0,
                blnd_xlm: 0,
            },
            100 * SCALAR_7,
        );
        assert_eq!(empty.eligible_blnd, 0);
        assert_eq!(empty.allocation, 0);
    }

    #[test]
    fn blnd_emissions_reject_invalid_denominators_values_and_overflow() {
        let env = Env::default();
        let invalid_pool_total = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_pool_blnd_emission_values(
                &env,
                SCALAR_7,
                &BlndEmissionValues {
                    blnd_usdc: 2,
                    blnd_xlm: 0,
                },
                1,
                true,
            )
        }));
        assert!(invalid_pool_total.is_err());

        let invalid_user_total = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_user_blnd_emission_values(
                &env,
                SCALAR_7,
                &BlndEmissionValues {
                    blnd_usdc: 0,
                    blnd_xlm: 1,
                },
                0,
            )
        }));
        assert!(invalid_user_total.is_err());

        let negative = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_user_blnd_emission_values(
                &env,
                SCALAR_7,
                &BlndEmissionValues {
                    blnd_usdc: -1,
                    blnd_xlm: 0,
                },
                SCALAR_7,
            )
        }));
        assert!(negative.is_err());

        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_pool_blnd_emission_values(
                &env,
                1,
                &BlndEmissionValues {
                    blnd_usdc: i128::MAX,
                    blnd_xlm: 1,
                },
                i128::MAX,
                true,
            )
        }));
        assert!(overflow.is_err());

        let wide_multiplication = quote_pool_blnd_emission_values(
            &env,
            i128::MAX,
            &BlndEmissionValues {
                blnd_usdc: i128::MAX,
                blnd_xlm: 0,
            },
            i128::MAX,
            true,
        );
        assert_eq!(wide_multiplication.allocation, i128::MAX);
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
