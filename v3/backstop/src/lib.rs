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
// Exact integer representation of the relative weights 2.5:2.5:2.
const TAKE_RATE_WEIGHT_BLND_USDC: i128 = 5;
const TAKE_RATE_WEIGHT_BLND_XLM: i128 = 5;
const TAKE_RATE_WEIGHT_USDC: i128 = 4;
const VALUATION_ADAPTER_VERSION: u32 = 1;
const POSITION_TTL_THRESHOLD: u32 = 179 * DAY_IN_LEDGERS;
const POSITION_TTL_BUMP: u32 = 180 * DAY_IN_LEDGERS;

/// The candidate's fixed three-tier backstop set in first-loss order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum BackstopTier {
    BlndUsdc,
    BlndXlm,
    Usdc,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Emitter,
    BlndToken,
    PoolFactory,
    LegacyBackstopToken,
    NextBackstopToken,
    PrefundToken,
    ValuationAdapter,
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
    BackfillFundedAmount,
    TotalBackfillClaimed,
    TotalBlndUsdcShares,
    TotalBlndUsdcQueuedShares,
    TotalTierShares(BackstopTier),
    TotalTierQueuedShares(BackstopTier),
}

#[derive(Clone)]
#[contracttype]
enum PersistentKey {
    MigrationPosition(Address, Address),
    BlndUsdcQ4W(Address, Address),
    PoolBlndUsdcShares(Address),
    PoolBlndUsdcQueuedShares(Address),
    TierShares(BackstopTier, Address, Address),
    TierQ4W(BackstopTier, Address, Address),
    PoolTierShares(BackstopTier, Address),
    PoolTierQueuedShares(BackstopTier, Address),
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
    BackfillAlreadyFunded = 1231,
    BackfillNotFunded = 1232,
    BackfillAlreadyClaimed = 1233,
    InvalidBackfillFunding = 1234,
    InvalidTier = 1235,
    InvalidValuationAdapter = 1236,
    InvalidValuation = 1237,
    StaleValuation = 1238,
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
    pub backfill_claimed: bool,
    pub frozen_weight: Option<i128>,
    pub time_basis: i128,
}

/// One bounded tier withdrawal-queue entry, denominated in shares.
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
    pub usdc: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ActivationQuote {
    pub eligible_value: i128,
    pub meets_threshold: bool,
    pub required_value: i128,
}

/// One immutable adapter quote for an LP-token amount.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetValuation {
    pub underlying_blnd: i128,
    pub usdc_value: i128,
    pub valid_until: u64,
}

/// Canonical value of one pool's active and queued backstop positions.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PoolValuation {
    pub active_blnd: BlndEmissionValues,
    pub active_values: ActivationValues,
    pub queued_values: ActivationValues,
    pub valid_until: u64,
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
    pub usdc: i128,
}

/// Allocation of one take-rate asset across the three backstop tiers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TakeRateQuote {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
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
    fn drop(env: Env, list: Vec<(Address, i128)>);
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

/// Immutable boundary for manipulation-resistant LP valuation.
#[contractclient(name = "ValuationAdapterClient")]
pub trait ValuationAdapter {
    fn version(env: Env) -> u32;
    fn quote(env: Env, token: Address, amount: i128) -> AssetValuation;
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
pub struct BackfillFunded {
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackfillClaimed {
    pub amount: i128,
    pub pool: Address,
    pub recipient: Address,
    pub user: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierDeposited {
    pub amount: i128,
    pub depositor: Address,
    pub pool: Address,
    pub shares: i128,
    pub tier: BackstopTier,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierWithdrawalQueued {
    pub depositor: Address,
    pub expires_at: u64,
    pub pool: Address,
    pub shares: i128,
    pub tier: BackstopTier,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierWithdrawalDequeued {
    pub depositor: Address,
    pub pool: Address,
    pub shares: i128,
    pub tier: BackstopTier,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierWithdrawn {
    pub amount: i128,
    pub depositor: Address,
    pub pool: Address,
    pub shares: i128,
    pub tier: BackstopTier,
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
    #[allow(clippy::too_many_arguments)]
    pub fn __constructor(
        env: Env,
        emitter: Address,
        blnd_token: Address,
        pool_factory: Address,
        legacy_backstop_token: Address,
        next_backstop_token: Address,
        prefund_token: Address,
        valuation_adapter: Address,
    ) {
        if legacy_backstop_token == next_backstop_token
            || legacy_backstop_token == prefund_token
            || next_backstop_token == prefund_token
        {
            panic_with_error!(&env, BackstopError::AssetConfigurationCollision);
        }
        let candidate = env.current_contract_address();
        let pool_factory_client = PoolFactoryClient::new(&env, &pool_factory);
        if pool_factory_client.backstop() != candidate {
            panic_with_error!(&env, BackstopError::InvalidPoolFactoryBinding);
        }
        let _ = pool_factory_client.is_pool(&candidate);
        if ValuationAdapterClient::new(&env, &valuation_adapter).version()
            != VALUATION_ADAPTER_VERSION
        {
            panic_with_error!(&env, BackstopError::InvalidValuationAdapter);
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
            .set(&DataKey::BlndToken, &blnd_token);
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
        env.storage()
            .instance()
            .set(&DataKey::ValuationAdapter, &valuation_adapter);
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

    /// Return the immutable BLND token used to verify funding and pay claims.
    pub fn blnd_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::BlndToken).unwrap()
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

    /// Return the immutable plain-USDC backstop token.
    ///
    /// The legacy name is retained in the constructor and this read interface
    /// because this token began as the candidate's isolated prefunding lane.
    pub fn prefund_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::PrefundToken)
            .unwrap()
    }

    /// Return the immutable plain-USDC backstop token.
    pub fn usdc_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        prefund_token(&env)
    }

    /// Return the immutable LP valuation adapter.
    pub fn valuation_adapter(env: Env) -> Address {
        extend_instance_ttl(&env);
        valuation_adapter(&env)
    }

    /// Deposit BLND:XLM LP into its pool-attributed second-loss tier.
    ///
    /// Until loss execution is implemented, each token base unit mints exactly
    /// one share. The attributed pool must be factory registered and expose the
    /// permanent fail-closed withdrawal callback.
    pub fn deposit_blnd_xlm(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_standard_tier(&env, BackstopTier::BlndXlm, from, pool, amount);
    }

    /// Deposit plain USDC into its pool-attributed third-loss tier.
    pub fn deposit_usdc(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_standard_tier(&env, BackstopTier::Usdc, from, pool, amount);
    }

    /// Queue active BLND:XLM shares for withdrawal after the 17-day lock.
    pub fn queue_blnd_xlm_withdrawal(
        env: Env,
        from: Address,
        pool: Address,
        shares: i128,
    ) -> Q4WEntry {
        extend_instance_ttl(&env);
        queue_standard_tier_withdrawal(&env, BackstopTier::BlndXlm, from, pool, shares)
    }

    /// Queue active USDC shares for withdrawal after the 17-day lock.
    pub fn queue_usdc_withdrawal(env: Env, from: Address, pool: Address, shares: i128) -> Q4WEntry {
        extend_instance_ttl(&env);
        queue_standard_tier_withdrawal(&env, BackstopTier::Usdc, from, pool, shares)
    }

    /// Return newest queued BLND:XLM shares to active status first.
    pub fn dequeue_blnd_xlm_withdrawal(env: Env, from: Address, pool: Address, shares: i128) {
        extend_instance_ttl(&env);
        dequeue_standard_tier_withdrawal(&env, BackstopTier::BlndXlm, from, pool, shares);
    }

    /// Return newest queued USDC shares to active status first.
    pub fn dequeue_usdc_withdrawal(env: Env, from: Address, pool: Address, shares: i128) {
        extend_instance_ttl(&env);
        dequeue_standard_tier_withdrawal(&env, BackstopTier::Usdc, from, pool, shares);
    }

    /// Withdraw expired BLND:XLM shares after the pool confirms loss safety.
    pub fn withdraw_blnd_xlm(env: Env, from: Address, pool: Address, shares: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_standard_tier(&env, BackstopTier::BlndXlm, from, pool, shares, to);
    }

    /// Withdraw expired USDC shares after the pool confirms loss safety.
    pub fn withdraw_usdc(env: Env, from: Address, pool: Address, shares: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_standard_tier(&env, BackstopTier::Usdc, from, pool, shares, to);
    }

    /// Return one user's total shares in a fixed backstop tier.
    pub fn tier_shares(env: Env, tier: BackstopTier, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => get_position(&env, &user, &pool).amount,
            _ => get_tier_shares(&env, tier, &user, &pool),
        }
    }

    /// Return one user's queued shares in a fixed backstop tier.
    pub fn tier_queued_shares(env: Env, tier: BackstopTier, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => queued_shares(&env, &get_blnd_usdc_q4w(&env, &user, &pool)),
            _ => queued_shares(&env, &get_tier_q4w(&env, tier, &user, &pool)),
        }
    }

    /// Return one user's unqueued share basis for activation and emissions.
    pub fn tier_active_shares(env: Env, tier: BackstopTier, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            Self::tier_shares(env.clone(), tier, user.clone(), pool.clone()),
            Self::tier_queued_shares(env.clone(), tier, user, pool),
        )
    }

    /// Return one user's bounded withdrawal queue for a fixed tier.
    pub fn tier_withdrawal_queue(
        env: Env,
        tier: BackstopTier,
        user: Address,
        pool: Address,
    ) -> Vec<Q4WEntry> {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => get_blnd_usdc_q4w(&env, &user, &pool),
            _ => get_tier_q4w(&env, tier, &user, &pool),
        }
    }

    /// Return all shares allocated to one pool in a fixed tier.
    pub fn pool_tier_shares(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => get_pool_blnd_usdc_shares(&env, &pool),
            _ => get_pool_tier_shares(&env, tier, &pool),
        }
    }

    /// Return all queued shares for one pool in a fixed tier.
    pub fn pool_tier_queued_shares(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => get_pool_blnd_usdc_queued_shares(&env, &pool),
            _ => get_pool_tier_queued_shares(&env, tier, &pool),
        }
    }

    /// Return one pool's unqueued share basis for activation and emissions.
    pub fn pool_tier_active_shares(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            Self::pool_tier_shares(env.clone(), tier, pool.clone()),
            Self::pool_tier_queued_shares(env.clone(), tier, pool),
        )
    }

    /// Return all accounted shares in a fixed tier.
    pub fn total_tier_shares(env: Env, tier: BackstopTier) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => env
                .storage()
                .instance()
                .get(&DataKey::TotalBlndUsdcShares)
                .unwrap_or(0),
            _ => env
                .storage()
                .instance()
                .get(&DataKey::TotalTierShares(tier))
                .unwrap_or(0),
        }
    }

    /// Return all queued shares in a fixed tier.
    pub fn total_tier_queued_shares(env: Env, tier: BackstopTier) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => env
                .storage()
                .instance()
                .get(&DataKey::TotalBlndUsdcQueuedShares)
                .unwrap_or(0),
            _ => env
                .storage()
                .instance()
                .get(&DataKey::TotalTierQueuedShares(tier))
                .unwrap_or(0),
        }
    }

    /// Return the global unqueued share basis for activation and emissions.
    pub fn total_tier_active_shares(env: Env, tier: BackstopTier) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            Self::total_tier_shares(env.clone(), tier),
            Self::total_tier_queued_shares(env.clone(), tier),
        )
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

    /// Derive one pool's canonical value from accounted active and queued
    /// shares. While loss execution is absent, one share equals one token
    /// base unit. LP quotes come only from the immutable adapter; plain USDC
    /// is valued one-for-one.
    pub fn pool_valuation(env: Env, pool: Address) -> PoolValuation {
        extend_instance_ttl(&env);
        build_pool_valuation(&env, &pool)
    }

    /// Quote activation from canonical pool accounting and immutable pricing.
    pub fn quote_pool_activation(
        env: Env,
        pool: Address,
        currently_active: bool,
    ) -> ActivationQuote {
        extend_instance_ttl(&env);
        let valuation = build_pool_valuation(&env, &pool);
        quote_activation_values(&env, &valuation.active_values, currently_active)
    }

    /// Quote a permissionless status refresh from canonical pool accounting.
    pub fn quote_pool_status_update(
        env: Env,
        pool: Address,
        current_status: u32,
    ) -> PoolStatusQuote {
        extend_instance_ttl(&env);
        let valuation = build_pool_valuation(&env, &pool);
        quote_update_pool_status_values(
            &env,
            current_status,
            &valuation.active_values,
            &valuation.queued_values,
        )
    }

    /// Quote an operator status request from canonical pool accounting.
    ///
    /// Authorization and stored-status mutation remain the pool's
    /// responsibility.
    pub fn quote_pool_status_set(
        env: Env,
        pool: Address,
        current_status: u32,
        requested_status: u32,
    ) -> PoolStatusQuote {
        extend_instance_ttl(&env);
        let valuation = build_pool_valuation(&env, &pool);
        quote_set_pool_status_values(
            &env,
            current_status,
            requested_status,
            &valuation.active_values,
            &valuation.queued_values,
        )
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

    /// Quote one take-rate asset across the three weighted tiers.
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

    /// Return the exact scheduled amount received through the emitter's one-time drop.
    pub fn funded_backfill(env: Env) -> Option<i128> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::BackfillFundedAmount)
    }

    /// Return the sum of successful position claims.
    pub fn total_backfill_claimed(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalBackfillClaimed)
            .unwrap_or(0)
    }

    /// Return funded BLND that remains reserved for position claims and rounding dust.
    pub fn remaining_backfill(env: Env) -> i128 {
        extend_instance_ttl(&env);
        let funded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::BackfillFundedAmount)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::BackfillNotFunded));
        let claimed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBackfillClaimed)
            .unwrap_or(0);
        checked_nonnegative_sub_i128(&env, funded, claimed)
    }

    /// Permissionlessly fund the exact scheduled backfill through the legacy emitter.
    ///
    /// The candidate is the sole drop recipient. The call verifies the exact BLND
    /// balance delta before recording the reserve and can succeed only once.
    pub fn fund_backfill(env: Env) -> i128 {
        extend_instance_ttl(&env);
        if env.storage().instance().has(&DataKey::BackfillFundedAmount) {
            panic_with_error!(&env, BackstopError::BackfillAlreadyFunded);
        }
        let scheduled: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ScheduledBackfill)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        let total_weight: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMigrationWeight)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        if scheduled <= 0 || scheduled > MAX_BACKFILL_BLND {
            panic_with_error!(&env, BackstopError::InvalidBackfillFunding);
        }
        if total_weight <= 0 {
            panic_with_error!(&env, BackstopError::NoEligibleWeight);
        }

        // Record the one-time obligation before either external call. Any
        // failure below rolls the marker back with the transaction.
        env.storage()
            .instance()
            .set(&DataKey::BackfillFundedAmount, &scheduled);
        env.storage()
            .instance()
            .set(&DataKey::TotalBackfillClaimed, &0_i128);
        let candidate = env.current_contract_address();
        let token = TokenClient::new(&env, &Self::blnd_token(env.clone()));
        let balance_before = token.balance(&candidate);
        let mut recipients = Vec::new(&env);
        recipients.push_back((candidate.clone(), scheduled));
        EmitterClient::new(&env, &Self::emitter(env.clone())).drop(&recipients);
        let received = checked_sub_i128(&env, token.balance(&candidate), balance_before);
        if received != scheduled {
            panic_with_error!(&env, BackstopError::InvalidBackfillFunding);
        }

        BackfillFunded { amount: scheduled }.publish(&env);
        scheduled
    }

    /// Claim one position's immutable, time-weighted BLND backfill allocation.
    ///
    /// The position owner authorizes the destination. Each surviving position
    /// can claim once; floor-rounding dust remains in the backfill reserve.
    pub fn claim_backfill(env: Env, user: Address, pool: Address, recipient: Address) -> i128 {
        extend_instance_ttl(&env);
        user.require_auth();
        let funded: i128 = env
            .storage()
            .instance()
            .get(&DataKey::BackfillFundedAmount)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::BackfillNotFunded));
        let total_weight: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalMigrationWeight)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        if total_weight <= 0 {
            panic_with_error!(&env, BackstopError::NoEligibleWeight);
        }
        let backfill_end: u64 = env
            .storage()
            .instance()
            .get(&DataKey::BackfillEnd)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive));
        let mut position = get_position(&env, &user, &pool);
        if position.backfill_claimed {
            panic_with_error!(&env, BackstopError::BackfillAlreadyClaimed);
        }
        let weight = position
            .frozen_weight
            .unwrap_or_else(|| position_weight(&env, &position, backfill_end));
        if weight <= 0 {
            panic_with_error!(&env, BackstopError::NoEligibleWeight);
        }
        let amount = proportional_floor(&env, funded, weight, total_weight);
        if amount <= 0 {
            panic_with_error!(&env, BackstopError::NoEligibleWeight);
        }
        let claimed: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalBackfillClaimed)
            .unwrap_or(0);
        let next_claimed = checked_add_i128(&env, claimed, amount);
        if next_claimed > funded {
            panic_with_error!(&env, BackstopError::ArithmeticError);
        }

        position.frozen_weight = Some(weight);
        position.backfill_claimed = true;
        set_position(&env, &user, &pool, &position);
        env.storage()
            .instance()
            .set(&DataKey::TotalBackfillClaimed, &next_claimed);
        TokenClient::new(&env, &Self::blnd_token(env.clone())).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );
        BackfillClaimed {
            amount,
            pool,
            recipient,
            user,
        }
        .publish(&env);
        amount
    }

    /// Quote a position's proportional share of scheduled BLND backpay.
    ///
    /// This reports the immutable entitlement and does not transfer BLND or
    /// subtract an amount that the position has already claimed.
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

fn valuation_adapter(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::ValuationAdapter)
        .unwrap()
}

fn tier_token(env: &Env, tier: BackstopTier) -> Address {
    match tier {
        BackstopTier::BlndUsdc => legacy_backstop_token(env),
        BackstopTier::BlndXlm => env
            .storage()
            .instance()
            .get(&DataKey::NextBackstopToken)
            .unwrap(),
        BackstopTier::Usdc => prefund_token(env),
    }
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
        env.current_contract_address(),
        &amount,
    );
}

fn deposit_standard_tier(
    env: &Env,
    tier: BackstopTier,
    from: Address,
    pool: Address,
    amount: i128,
) {
    require_standard_tier(env, tier);
    require_positive(env, amount);
    from.require_auth();
    require_compatible_pool(env, &pool);

    // Keep the companion queue live whenever its position is touched.
    get_tier_q4w(env, tier, &from, &pool);
    let user_shares = get_tier_shares(env, tier, &from, &pool);
    set_tier_shares(
        env,
        tier,
        &from,
        &pool,
        checked_add_i128(env, user_shares, amount),
    );
    let pool_shares = get_pool_tier_shares(env, tier, &pool);
    set_pool_tier_shares(env, tier, &pool, checked_add_i128(env, pool_shares, amount));
    add_to_instance_total(env, DataKey::TotalTierShares(tier), amount);

    TokenClient::new(env, &tier_token(env, tier)).transfer(
        &from,
        env.current_contract_address(),
        &amount,
    );
    TierDeposited {
        amount,
        depositor: from,
        pool,
        shares: amount,
        tier,
    }
    .publish(env);
}

fn queue_standard_tier_withdrawal(
    env: &Env,
    tier: BackstopTier,
    from: Address,
    pool: Address,
    shares: i128,
) -> Q4WEntry {
    require_standard_tier(env, tier);
    require_positive(env, shares);
    from.require_auth();

    let total_shares = get_tier_shares(env, tier, &from, &pool);
    let mut queue = get_tier_q4w(env, tier, &from, &pool);
    let already_queued = queued_shares(env, &queue);
    let active_shares = checked_nonnegative_sub_i128(env, total_shares, already_queued);
    if shares > active_shares {
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
    let entry = Q4WEntry {
        amount: shares,
        expires_at,
    };
    queue.push_back(entry.clone());
    set_tier_q4w(env, tier, &from, &pool, &queue);
    add_pool_tier_queued_shares(env, tier, &pool, shares);
    add_to_instance_total(env, DataKey::TotalTierQueuedShares(tier), shares);
    TierWithdrawalQueued {
        depositor: from,
        expires_at,
        pool,
        shares,
        tier,
    }
    .publish(env);
    entry
}

fn dequeue_standard_tier_withdrawal(
    env: &Env,
    tier: BackstopTier,
    from: Address,
    pool: Address,
    shares: i128,
) {
    require_standard_tier(env, tier);
    require_positive(env, shares);
    from.require_auth();

    let mut queue = get_tier_q4w(env, tier, &from, &pool);
    consume_tier_queue_from_back(env, &mut queue, shares);
    set_tier_q4w(env, tier, &from, &pool, &queue);
    add_pool_tier_queued_shares(env, tier, &pool, -shares);
    add_to_instance_total(env, DataKey::TotalTierQueuedShares(tier), -shares);
    TierWithdrawalDequeued {
        depositor: from,
        pool,
        shares,
        tier,
    }
    .publish(env);
}

fn withdraw_standard_tier(
    env: &Env,
    tier: BackstopTier,
    from: Address,
    pool: Address,
    shares: i128,
    to: Address,
) {
    require_standard_tier(env, tier);
    require_positive(env, shares);
    from.require_auth();

    let backstop = env.current_contract_address();
    if !PoolLossClient::new(env, &pool).backstop_withdrawal_allowed(&backstop) {
        panic_with_error!(env, BackstopError::PoolWithdrawalBlocked);
    }
    consume_expired_tier_q4w(env, tier, &from, &pool, shares);
    let user_shares = get_tier_shares(env, tier, &from, &pool);
    if shares > user_shares {
        panic_with_error!(env, BackstopError::InsufficientBalance);
    }
    set_tier_shares(
        env,
        tier,
        &from,
        &pool,
        checked_sub_i128(env, user_shares, shares),
    );
    let pool_shares = get_pool_tier_shares(env, tier, &pool);
    if shares > pool_shares {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    set_pool_tier_shares(env, tier, &pool, checked_sub_i128(env, pool_shares, shares));
    add_pool_tier_queued_shares(env, tier, &pool, -shares);
    add_to_instance_total(env, DataKey::TotalTierShares(tier), -shares);
    add_to_instance_total(env, DataKey::TotalTierQueuedShares(tier), -shares);

    TokenClient::new(env, &tier_token(env, tier)).transfer(
        &env.current_contract_address(),
        &to,
        &shares,
    );
    TierWithdrawn {
        amount: shares,
        depositor: from,
        pool,
        shares,
        tier,
        to,
    }
    .publish(env);
}

fn require_standard_tier(env: &Env, tier: BackstopTier) {
    if tier == BackstopTier::BlndUsdc {
        panic_with_error!(env, BackstopError::InvalidTier);
    }
}

fn consume_tier_queue_from_back(env: &Env, queue: &mut Vec<Q4WEntry>, shares: i128) {
    let mut remaining = shares;
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
}

fn consume_expired_tier_q4w(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
    shares: i128,
) {
    let mut queue = get_tier_q4w(env, tier, user, pool);
    let mut remaining = shares;
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
    set_tier_q4w(env, tier, user, pool, &queue);
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
            backfill_claimed: false,
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

fn get_tier_shares(env: &Env, tier: BackstopTier, user: &Address, pool: &Address) -> i128 {
    let key = PersistentKey::TierShares(tier, user.clone(), pool.clone());
    let q4w_key = PersistentKey::TierQ4W(tier, user.clone(), pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &q4w_key);
    shares
}

fn set_tier_shares(env: &Env, tier: BackstopTier, user: &Address, pool: &Address, shares: i128) {
    let key = PersistentKey::TierShares(tier, user.clone(), pool.clone());
    env.storage().persistent().set(&key, &shares);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_tier_q4w(env: &Env, tier: BackstopTier, user: &Address, pool: &Address) -> Vec<Q4WEntry> {
    let key = PersistentKey::TierQ4W(tier, user.clone(), pool.clone());
    let shares_key = PersistentKey::TierShares(tier, user.clone(), pool.clone());
    let queue = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &shares_key);
    queue
}

fn set_tier_q4w(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
    queue: &Vec<Q4WEntry>,
) {
    let key = PersistentKey::TierQ4W(tier, user.clone(), pool.clone());
    env.storage().persistent().set(&key, queue);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_pool_tier_shares(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    let key = PersistentKey::PoolTierShares(tier, pool.clone());
    let queued_key = PersistentKey::PoolTierQueuedShares(tier, pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &queued_key);
    shares
}

fn set_pool_tier_shares(env: &Env, tier: BackstopTier, pool: &Address, shares: i128) {
    let key = PersistentKey::PoolTierShares(tier, pool.clone());
    env.storage().persistent().set(&key, &shares);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_pool_tier_queued_shares(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    let key = PersistentKey::PoolTierQueuedShares(tier, pool.clone());
    let shares_key = PersistentKey::PoolTierShares(tier, pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &shares_key);
    shares
}

fn add_pool_tier_queued_shares(env: &Env, tier: BackstopTier, pool: &Address, delta: i128) {
    let key = PersistentKey::PoolTierQueuedShares(tier, pool.clone());
    let current = get_pool_tier_queued_shares(env, tier, pool);
    let next = checked_add_i128(env, current, delta);
    if next < 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    env.storage().persistent().set(&key, &next);
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

fn build_pool_valuation(env: &Env, pool: &Address) -> PoolValuation {
    require_compatible_pool(env, pool);
    let adapter_address = valuation_adapter(env);
    let adapter = ValuationAdapterClient::new(env, &adapter_address);
    if adapter.version() != VALUATION_ADAPTER_VERSION {
        panic_with_error!(env, BackstopError::InvalidValuationAdapter);
    }

    let blnd_usdc_total = get_pool_blnd_usdc_shares(env, pool);
    let blnd_usdc_queued = get_pool_blnd_usdc_queued_shares(env, pool);
    let blnd_usdc_active = checked_nonnegative_sub_i128(env, blnd_usdc_total, blnd_usdc_queued);
    let blnd_xlm_total = get_pool_tier_shares(env, BackstopTier::BlndXlm, pool);
    let blnd_xlm_queued = get_pool_tier_queued_shares(env, BackstopTier::BlndXlm, pool);
    let blnd_xlm_active = checked_nonnegative_sub_i128(env, blnd_xlm_total, blnd_xlm_queued);
    let usdc_total = get_pool_tier_shares(env, BackstopTier::Usdc, pool);
    let usdc_queued = get_pool_tier_queued_shares(env, BackstopTier::Usdc, pool);
    let usdc_active = checked_nonnegative_sub_i128(env, usdc_total, usdc_queued);

    let blnd_usdc_active_quote =
        quote_lp_amount(env, &adapter, BackstopTier::BlndUsdc, blnd_usdc_active);
    let blnd_usdc_queued_quote =
        quote_lp_amount(env, &adapter, BackstopTier::BlndUsdc, blnd_usdc_queued);
    let blnd_xlm_active_quote =
        quote_lp_amount(env, &adapter, BackstopTier::BlndXlm, blnd_xlm_active);
    let blnd_xlm_queued_quote =
        quote_lp_amount(env, &adapter, BackstopTier::BlndXlm, blnd_xlm_queued);

    PoolValuation {
        active_blnd: BlndEmissionValues {
            blnd_usdc: blnd_usdc_active_quote.underlying_blnd,
            blnd_xlm: blnd_xlm_active_quote.underlying_blnd,
        },
        active_values: ActivationValues {
            blnd_usdc: blnd_usdc_active_quote.usdc_value,
            blnd_xlm: blnd_xlm_active_quote.usdc_value,
            usdc: usdc_active,
        },
        queued_values: ActivationValues {
            blnd_usdc: blnd_usdc_queued_quote.usdc_value,
            blnd_xlm: blnd_xlm_queued_quote.usdc_value,
            usdc: usdc_queued,
        },
        valid_until: blnd_usdc_active_quote
            .valid_until
            .min(blnd_usdc_queued_quote.valid_until)
            .min(blnd_xlm_active_quote.valid_until)
            .min(blnd_xlm_queued_quote.valid_until),
    }
}

fn quote_lp_amount(
    env: &Env,
    adapter: &ValuationAdapterClient,
    tier: BackstopTier,
    amount: i128,
) -> AssetValuation {
    if amount < 0 {
        panic_with_error!(env, BackstopError::InvalidValuation);
    }
    if amount == 0 {
        return AssetValuation {
            underlying_blnd: 0,
            usdc_value: 0,
            valid_until: u64::MAX,
        };
    }
    let quote = adapter.quote(&tier_token(env, tier), &amount);
    if quote.underlying_blnd < 0 || quote.usdc_value < 0 {
        panic_with_error!(env, BackstopError::InvalidValuation);
    }
    if quote.valid_until < env.ledger().timestamp() {
        panic_with_error!(env, BackstopError::StaleValuation);
    }
    quote
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
    if values.blnd_xlm < 0 || values.blnd_usdc < 0 || values.usdc < 0 {
        panic_with_error!(env, BackstopError::InvalidActivationValue);
    }
    checked_add_i128(
        env,
        checked_add_i128(env, values.blnd_xlm, values.blnd_usdc),
        values.usdc,
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
    if distribution < 0 || values.blnd_usdc < 0 || values.blnd_xlm < 0 || values.usdc < 0 {
        panic_with_error!(env, BackstopError::InvalidTakeRateValue);
    }

    let blnd_usdc_weighted = checked_mul_i128(env, values.blnd_usdc, TAKE_RATE_WEIGHT_BLND_USDC);
    let blnd_xlm_weighted = checked_mul_i128(env, values.blnd_xlm, TAKE_RATE_WEIGHT_BLND_XLM);
    let usdc_weighted = checked_mul_i128(env, values.usdc, TAKE_RATE_WEIGHT_USDC);
    let denominator = checked_add_i128(
        env,
        checked_add_i128(env, blnd_usdc_weighted, blnd_xlm_weighted),
        usdc_weighted,
    );

    if denominator == 0 {
        return TakeRateQuote {
            blnd_usdc: 0,
            blnd_xlm: 0,
            remainder: distribution,
            usdc: 0,
        };
    }

    let blnd_usdc = proportional_floor(env, distribution, blnd_usdc_weighted, denominator);
    let blnd_xlm = proportional_floor(env, distribution, blnd_xlm_weighted, denominator);
    let usdc = proportional_floor(env, distribution, usdc_weighted, denominator);
    let allocated = checked_add_i128(env, checked_add_i128(env, blnd_usdc, blnd_xlm), usdc);

    TakeRateQuote {
        blnd_usdc,
        blnd_xlm,
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
    struct MockLossAwarePool;

    #[derive(Clone)]
    #[contracttype]
    enum MockLossAwarePoolKey {
        Backstop,
    }

    #[contractimpl]
    impl MockLossAwarePool {
        pub fn __constructor(env: Env, backstop: Address) {
            env.storage()
                .instance()
                .set(&MockLossAwarePoolKey::Backstop, &backstop);
        }

        pub fn backstop_withdrawal_allowed(env: Env, backstop: Address) -> bool {
            env.storage()
                .instance()
                .get::<MockLossAwarePoolKey, Address>(&MockLossAwarePoolKey::Backstop)
                .map(|expected| expected == backstop)
                .unwrap_or(false)
        }
    }

    #[contract]
    struct MockValuationAdapter;

    #[derive(Clone)]
    #[contracttype]
    enum MockValuationAdapterKey {
        Invalid,
        ValidUntil,
        Version,
    }

    #[contractimpl]
    impl MockValuationAdapter {
        pub fn __constructor(env: Env) {
            env.storage()
                .instance()
                .set(&MockValuationAdapterKey::Invalid, &false);
            env.storage()
                .instance()
                .set(&MockValuationAdapterKey::ValidUntil, &u64::MAX);
            env.storage().instance().set(
                &MockValuationAdapterKey::Version,
                &VALUATION_ADAPTER_VERSION,
            );
        }

        pub fn version(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&MockValuationAdapterKey::Version)
                .unwrap()
        }

        pub fn quote(env: Env, _token: Address, amount: i128) -> AssetValuation {
            let invalid: bool = env
                .storage()
                .instance()
                .get(&MockValuationAdapterKey::Invalid)
                .unwrap();
            AssetValuation {
                underlying_blnd: if invalid { -1 } else { amount },
                usdc_value: amount,
                valid_until: env
                    .storage()
                    .instance()
                    .get(&MockValuationAdapterKey::ValidUntil)
                    .unwrap(),
            }
        }

        pub fn set_invalid(env: Env, invalid: bool) {
            env.storage()
                .instance()
                .set(&MockValuationAdapterKey::Invalid, &invalid);
        }

        pub fn set_valid_until(env: Env, valid_until: u64) {
            env.storage()
                .instance()
                .set(&MockValuationAdapterKey::ValidUntil, &valid_until);
        }

        pub fn set_version(env: Env, version: u32) {
            env.storage()
                .instance()
                .set(&MockValuationAdapterKey::Version, &version);
        }
    }

    fn register_valuation_adapter(env: &Env) -> Address {
        env.register(MockValuationAdapter, ())
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
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);

        assert_eq!(candidate.backstop_token(), legacy_token);
        assert_eq!(candidate.blnd_token(), blnd_token);
        assert_eq!(candidate.pool_factory(), pool_factory);
        assert_eq!(candidate.next_backstop_token(), next_token);
        assert_eq!(candidate.prefund_token(), prefund_token);
        assert_eq!(candidate.usdc_token(), prefund_token);
        assert_eq!(candidate.valuation_adapter(), valuation_adapter);
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
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
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
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
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
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
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
    fn backfill_claim_requires_position_owner_authorization() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let blnd_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);
        let user = Address::generate(&env);
        let pool = Address::generate(&env);
        let recipient = Address::generate(&env);
        let amount = 100_i128;
        StellarAssetClient::new(&env, &blnd_token)
            .mock_all_auths()
            .mint(&candidate_id, &amount);
        env.as_contract(&candidate_id, || {
            env.storage()
                .instance()
                .set(&DataKey::BackfillEnd, &100_u64);
            env.storage()
                .instance()
                .set(&DataKey::BackfillFundedAmount, &amount);
            env.storage()
                .instance()
                .set(&DataKey::TotalBackfillClaimed, &0_i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalMigrationWeight, &10_i128);
            set_position(
                &env,
                &user,
                &pool,
                &MigrationPosition {
                    amount: 10,
                    backfill_claimed: false,
                    frozen_weight: Some(10),
                    time_basis: 0,
                },
            );
        });

        assert!(candidate
            .try_claim_backfill(&user, &pool, &recipient)
            .is_err());
        assert!(!candidate.migration_position(&user, &pool).backfill_claimed);
        assert_eq!(TokenClient::new(&env, &blnd_token).balance(&recipient), 0);

        assert_eq!(
            candidate
                .mock_auths(&[MockAuth {
                    address: &user,
                    invoke: &MockAuthInvoke {
                        contract: &candidate_id,
                        fn_name: "claim_backfill",
                        args: (&user, &pool, &recipient).into_val(&env),
                        sub_invokes: &[],
                    },
                }])
                .claim_backfill(&user, &pool, &recipient),
            amount
        );
        assert_eq!(
            TokenClient::new(&env, &blnd_token).balance(&recipient),
            amount
        );
        assert!(candidate.migration_position(&user, &pool).backfill_claimed);
        assert!(candidate
            .try_claim_backfill(&user, &pool, &recipient)
            .is_err());
    }

    #[test]
    fn standard_tier_operations_require_owner_authorization() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let next_token = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let prefund_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);
        let user = Address::generate(&env);
        let recipient = Address::generate(&env);
        let pool = env.register(MockLossAwarePool, (&candidate_id,));
        MockPoolFactoryClient::new(&env, &pool_factory).set_pool(&pool);
        let amount = 100;
        StellarAssetClient::new(&env, &next_token)
            .mock_all_auths()
            .mint(&user, &amount);
        StellarAssetClient::new(&env, &prefund_token)
            .mock_all_auths()
            .mint(&user, &amount);

        // Every non-authorization precondition for each deposit is valid.
        assert!(candidate
            .try_deposit_blnd_xlm(&user, &pool, &amount)
            .is_err());
        assert!(candidate.try_deposit_usdc(&user, &pool, &amount).is_err());

        candidate
            .mock_auths(&[MockAuth {
                address: &user,
                invoke: &MockAuthInvoke {
                    contract: &candidate_id,
                    fn_name: "deposit_blnd_xlm",
                    args: (&user, &pool, amount).into_val(&env),
                    sub_invokes: &[MockAuthInvoke {
                        contract: &next_token,
                        fn_name: "transfer",
                        args: (&user, &candidate_id, amount).into_val(&env),
                        sub_invokes: &[],
                    }],
                },
            }])
            .deposit_blnd_xlm(&user, &pool, &amount);
        candidate
            .mock_auths(&[MockAuth {
                address: &user,
                invoke: &MockAuthInvoke {
                    contract: &candidate_id,
                    fn_name: "deposit_usdc",
                    args: (&user, &pool, amount).into_val(&env),
                    sub_invokes: &[MockAuthInvoke {
                        contract: &prefund_token,
                        fn_name: "transfer",
                        args: (&user, &candidate_id, amount).into_val(&env),
                        sub_invokes: &[],
                    }],
                },
            }])
            .deposit_usdc(&user, &pool, &amount);

        // Shares now exist, so authorization is the only queueing precondition
        // intentionally left unsatisfied.
        assert!(candidate
            .try_queue_blnd_xlm_withdrawal(&user, &pool, &amount)
            .is_err());
        assert!(candidate
            .try_queue_usdc_withdrawal(&user, &pool, &amount)
            .is_err());

        let blnd_xlm_entry = candidate
            .mock_auths(&[MockAuth {
                address: &user,
                invoke: &MockAuthInvoke {
                    contract: &candidate_id,
                    fn_name: "queue_blnd_xlm_withdrawal",
                    args: (&user, &pool, amount).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .queue_blnd_xlm_withdrawal(&user, &pool, &amount);
        let usdc_entry = candidate
            .mock_auths(&[MockAuth {
                address: &user,
                invoke: &MockAuthInvoke {
                    contract: &candidate_id,
                    fn_name: "queue_usdc_withdrawal",
                    args: (&user, &pool, amount).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .queue_usdc_withdrawal(&user, &pool, &amount);

        // Valid queued balances make authorization the only dequeue failure.
        assert!(candidate
            .try_dequeue_blnd_xlm_withdrawal(&user, &pool, &amount)
            .is_err());
        assert!(candidate
            .try_dequeue_usdc_withdrawal(&user, &pool, &amount)
            .is_err());

        env.ledger()
            .set_timestamp(blnd_xlm_entry.expires_at.max(usdc_entry.expires_at));
        // The queues are expired and the compatible pool permits withdrawal,
        // leaving owner authorization as the only unmet withdrawal condition.
        assert!(candidate
            .try_withdraw_blnd_xlm(&user, &pool, &amount, &recipient)
            .is_err());
        assert!(candidate
            .try_withdraw_usdc(&user, &pool, &amount, &recipient)
            .is_err());
        assert_eq!(candidate.total_tier_shares(&BackstopTier::BlndXlm), amount);
        assert_eq!(candidate.total_tier_shares(&BackstopTier::Usdc), amount);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1220)")]
    fn constructor_rejects_colliding_migration_and_prefund_assets() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate,));
        let valuation_adapter = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &blnd_token,
                &pool_factory,
                &token,
                &next_token,
                &token,
                &valuation_adapter,
            ),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1220)")]
    fn constructor_rejects_colliding_legacy_and_next_assets() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate,));
        let valuation_adapter = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &blnd_token,
                &pool_factory,
                &token,
                &token,
                &prefund_token,
                &valuation_adapter,
            ),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1220)")]
    fn constructor_rejects_colliding_next_and_usdc_assets() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate,));
        let valuation_adapter = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &token,
                &token,
                &valuation_adapter,
            ),
        );
    }

    #[test]
    #[should_panic]
    fn constructor_rejects_incompatible_pool_factory() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let pool_factory = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let valuation_adapter = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
            ),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1229)")]
    fn constructor_rejects_pool_factory_bound_to_another_backstop() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let other_backstop = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&other_backstop,));
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let valuation_adapter = Address::generate(&env);

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
            ),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1236)")]
    fn constructor_rejects_incompatible_valuation_adapter_version() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate,));
        let valuation_adapter = register_valuation_adapter(&env);
        MockValuationAdapterClient::new(&env, &valuation_adapter)
            .set_version(&(VALUATION_ADAPTER_VERSION + 1));

        env.register_at(
            &candidate,
            BackstopContract,
            (
                &emitter,
                &blnd_token,
                &pool_factory,
                &legacy_token,
                &next_token,
                &prefund_token,
                &valuation_adapter,
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
    fn activation_policy_counts_three_assets_equally() {
        let env = Env::default();
        let mixed = ActivationValues {
            blnd_xlm: 4_000 * SCALAR_7,
            blnd_usdc: 4_000 * SCALAR_7,
            usdc: 4_500 * SCALAR_7,
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
    fn canonical_pool_valuation_uses_active_and_queued_tier_shares() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let incumbent = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let usdc_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
        let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
        let pool_id = env.register(MockLossAwarePool, (&candidate_id,));
        pool_factory.set_pool(&pool_id);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory_id,
                &legacy_token,
                &next_token,
                &usdc_token,
                &valuation_adapter,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);

        env.as_contract(&candidate_id, || {
            set_pool_blnd_usdc_shares(&env, &pool_id, 5_000 * SCALAR_7);
            add_pool_blnd_usdc_queued_shares(&env, &pool_id, 1_000 * SCALAR_7);
            set_pool_tier_shares(&env, BackstopTier::BlndXlm, &pool_id, 5_000 * SCALAR_7);
            add_pool_tier_queued_shares(&env, BackstopTier::BlndXlm, &pool_id, 1_000 * SCALAR_7);
            set_pool_tier_shares(&env, BackstopTier::Usdc, &pool_id, 5_500 * SCALAR_7);
            add_pool_tier_queued_shares(&env, BackstopTier::Usdc, &pool_id, 1_000 * SCALAR_7);
        });

        let valuation = candidate.pool_valuation(&pool_id);
        assert_eq!(
            valuation,
            PoolValuation {
                active_blnd: BlndEmissionValues {
                    blnd_usdc: 4_000 * SCALAR_7,
                    blnd_xlm: 4_000 * SCALAR_7,
                },
                active_values: ActivationValues {
                    blnd_usdc: 4_000 * SCALAR_7,
                    blnd_xlm: 4_000 * SCALAR_7,
                    usdc: 4_500 * SCALAR_7,
                },
                queued_values: ActivationValues {
                    blnd_usdc: 1_000 * SCALAR_7,
                    blnd_xlm: 1_000 * SCALAR_7,
                    usdc: 1_000 * SCALAR_7,
                },
                valid_until: u64::MAX,
            }
        );
        assert!(
            candidate
                .quote_pool_activation(&pool_id, &false)
                .meets_threshold
        );
        let status = candidate.quote_pool_status_update(&pool_id, &1);
        assert_eq!(status.status, 1);
        assert_eq!(status.eligible_value, 12_500 * SCALAR_7);
        assert_eq!(status.q4w_percentage, 1_935_484);
    }

    #[test]
    fn canonical_pool_valuation_fails_closed() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let incumbent = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let usdc_token = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
        let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
        let pool_id = env.register(MockLossAwarePool, (&candidate_id,));
        pool_factory.set_pool(&pool_id);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter = register_valuation_adapter(&env);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter_id,
                &blnd_token,
                &pool_factory_id,
                &legacy_token,
                &next_token,
                &usdc_token,
                &valuation_adapter,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);
        let adapter = MockValuationAdapterClient::new(&env, &valuation_adapter);

        env.as_contract(&candidate_id, || {
            set_pool_blnd_usdc_shares(&env, &pool_id, SCALAR_7);
        });

        adapter.set_valid_until(&999);
        assert!(candidate.try_pool_valuation(&pool_id).is_err());
        adapter.set_valid_until(&u64::MAX);
        adapter.set_invalid(&true);
        assert!(candidate.try_pool_valuation(&pool_id).is_err());
        adapter.set_invalid(&false);
        adapter.set_version(&(VALUATION_ADAPTER_VERSION + 1));
        assert!(candidate.try_pool_valuation(&pool_id).is_err());
        assert!(candidate
            .try_pool_valuation(&Address::generate(&env))
            .is_err());
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

    #[test]
    fn pool_status_policy_applies_v2_q4w_thresholds() {
        let env = Env::default();
        let values = |amount| ActivationValues {
            blnd_xlm: amount,
            blnd_usdc: 0,
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
            usdc: 0,
        };
        let queued = ActivationValues {
            blnd_xlm: 0,
            blnd_usdc: 0,
            usdc: 1,
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
    fn take_rate_policy_applies_2_5_2_5_2_weights_and_conserves_assets() {
        let env = Env::default();
        let equal_values = TakeRateValues {
            blnd_usdc: SCALAR_7,
            blnd_xlm: SCALAR_7,
            usdc: SCALAR_7,
        };
        let quote = quote_take_rate_values(&env, 140, &equal_values);
        assert_eq!(quote.blnd_usdc, 50);
        assert_eq!(quote.blnd_xlm, 50);
        assert_eq!(quote.usdc, 40);
        assert_eq!(quote.remainder, 0);

        let rounding_quote = quote_take_rate_values(&env, 101, &equal_values);
        assert_eq!(
            rounding_quote.blnd_usdc
                + rounding_quote.blnd_xlm
                + rounding_quote.usdc
                + rounding_quote.remainder,
            101
        );
        assert_eq!(rounding_quote.remainder, 1);

        let unequal_values = TakeRateValues {
            blnd_usdc: 2 * SCALAR_7,
            blnd_xlm: SCALAR_7,
            usdc: SCALAR_7,
        };
        let unequal_quote = quote_take_rate_values(&env, 190, &unequal_values);
        assert_eq!(unequal_quote.blnd_usdc, 100);
        assert_eq!(unequal_quote.blnd_xlm, 50);
        assert_eq!(unequal_quote.usdc, 40);
        assert_eq!(unequal_quote.remainder, 0);
    }

    #[test]
    fn take_rate_policy_carries_empty_distribution() {
        let env = Env::default();
        let required_only = TakeRateValues {
            blnd_usdc: SCALAR_7,
            blnd_xlm: SCALAR_7,
            usdc: 0,
        };
        let quote = quote_take_rate_values(&env, 10, &required_only);
        assert_eq!(quote.blnd_usdc, 5);
        assert_eq!(quote.blnd_xlm, 5);
        assert_eq!(quote.remainder, 0);

        let empty = quote_take_rate_values(
            &env,
            10,
            &TakeRateValues {
                blnd_usdc: 0,
                blnd_xlm: 0,
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
                    usdc: -1,
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
