#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, token::TokenClient, Address, BytesN, Env, Map, Vec, I256,
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
const AUCTION_TTL_THRESHOLD: u32 = 45 * DAY_IN_LEDGERS;
const AUCTION_TTL_BUMP: u32 = 46 * DAY_IN_LEDGERS;
const BAD_DEBT_LOT_PREMIUM_NUMERATOR: i128 = 6;
const BAD_DEBT_LOT_PREMIUM_DENOMINATOR: i128 = 5;
const BAD_DEBT_TIER_MINIMUM_VALUE_USDC: i128 = 200 * SCALAR_7;
const INTEREST_LOT_PREMIUM_NUMERATOR: i128 = 6;
const INTEREST_LOT_PREMIUM_DENOMINATOR: i128 = 5;
const BACKSTOP_EMISSION_NUMERATOR: i128 = 7;
const POOL_EMISSION_NUMERATOR: i128 = 3;
const EMISSION_SPLIT_DENOMINATOR: i128 = 10;
const EMISSION_INDEX_SCALE: i128 = 100_000_000_000_000;
const MAX_REWARD_ZONE_SIZE: u32 = 30;
const POOL_EMISSION_GULP_INTERVAL_SECONDS: u64 = 24 * 60 * 60;
const MIN_DISTRIBUTION_INTERVAL_SECONDS: u64 = 5;
const WEIGHT_CHANGE_CHECKPOINT_MAX_AGE_SECONDS: u64 = MIN_DISTRIBUTION_INTERVAL_SECONDS;
const REWARD_ZONE_CHECKPOINT_MAX_AGE_SECONDS: u64 = 60 * 60;

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" The candidate's fixed three-tier backstop set in first-loss order."#
)]
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
    BlndBindingVerified,
    TotalBackfillClaimed,
    LastOngoingDistribution,
    OngoingTotalReceived,
    OngoingTotalClaimed,
    OngoingBackstopClaimed,
    OngoingBackstopAllocated,
    OngoingPoolAllocated,
    OngoingSplitCarry,
    OngoingBackstopCarry,
    OngoingPoolCarry,
    TotalBlndUsdcShares,
    TotalBlndUsdcQueuedShares,
    TotalTierShares(BackstopTier),
    TotalTierQueuedShares(BackstopTier),
    TotalTierAssets(BackstopTier),
}

#[derive(Clone)]
#[contracttype]
enum PersistentKey {
    RewardZone,
    RewardZoneCheckpoint,
    MigrationPosition(Address, Address),
    BlndUsdcQ4W(Address, Address),
    PoolBlndUsdcShares(Address),
    PoolBlndUsdcQueuedShares(Address),
    TierShares(BackstopTier, Address, Address),
    TierQ4W(BackstopTier, Address, Address),
    PoolTierShares(BackstopTier, Address),
    PoolTierQueuedShares(BackstopTier, Address),
    PoolTierAssets(BackstopTier, Address),
    PoolOngoingEmissions(Address),
    PoolEmissionReservation(Address),
    UserOngoingEmissions(BackstopTier, Address, Address),
}

#[derive(Clone)]
#[contracttype]
enum TemporaryKey {
    BadDebtCommitment(Address),
    InterestCommitment(Address),
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
    BadDebtCommitmentExists = 1239,
    BadDebtCommitmentNotFound = 1240,
    NoBadDebtLossCapacity = 1241,
    InterestCommitmentExists = 1242,
    InterestCommitmentNotFound = 1243,
    InvalidInterestLot = 1244,
    InterestTierLocked = 1245,
    InvalidRewardZoneEntry = 1246,
    RewardZoneFull = 1247,
    DistributionCheckpointRequired = 1248,
    DistributionTooSoon = 1249,
    InvalidOngoingBalance = 1250,
    NoOngoingEmissions = 1251,
    PoolEmissionGulpTooSoon = 1252,
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
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" One continuous BLND:USDC tier position plus temporary backfill metadata."#
)]
pub struct MigrationPosition {
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Tier shares owned by this position."#
    )]
    pub amount: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Actual BLND:USDC contributed before migration and still backfill"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" eligible. This remains separate from shares once the exchange rate"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" differs from one token per share."#
    )]
    pub backfill_amount: i128,
    pub backfill_claimed: bool,
    pub frozen_weight: Option<i128>,
    pub time_basis: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" One bounded tier withdrawal-queue entry, denominated in shares."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Q4WEntry {
    pub amount: i128,
    pub expires_at: u64,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Accounted assets and shares for one pool in one backstop tier."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PoolTierState {
    pub assets: i128,
    pub queued_shares: i128,
    pub shares: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Canonical pool-local lot for one bad-debt auction."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BadDebtLotQuote {
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" USDC value of the remaining selected assets after any partial fill."#
    )]
    pub committed_value: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Oracle-valued bad debt supplied at auction creation."#
    )]
    pub debt_value: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The single waterfall tier selected for this auction."#
    )]
    pub tier: BackstopTier,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Remaining base token units selected from the tier."#
    )]
    pub lot_amount: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Creation-time 120% target not filled by available backstop capital."#
    )]
    pub unfilled_target_value: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Creation-time inherited 120% target lot value."#
    )]
    pub target_value: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Creation-time earliest expiry of any LP valuation used by the quote."#
    )]
    pub valid_until: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
struct BadDebtCommitment {
    auction_id: BytesN<32>,
    quote: BadDebtLotQuote,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Canonical tier-token bid for one pool interest auction."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InterestLotQuote {
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Token donated to the selected pool-tier by the filler."#
    )]
    pub bid_token: Address,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Remaining base tier-token amount before the auction time modifier."#
    )]
    pub bid_amount: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Oracle-valued reserve-asset lot supplied by the registered pool."#
    )]
    pub lot_value: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Tier whose token is donated by a successful filler."#
    )]
    pub tier: BackstopTier,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Creation-time 120% target, denominated in 7-decimal USDC."#
    )]
    pub target_value: i128,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Earliest expiry of the tier-token valuation used by the quote."#
    )]
    pub valid_until: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
struct InterestCommitment {
    auction_id: BytesN<32>,
    quote: InterestLotQuote,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Verified, pool-specific asset values denominated in 7-decimal USDC units."#
)]
#[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Constructing these values from token balances and manipulation-resistant"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" prices is the responsibility of the canonical valuation layer."#
)]
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

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" One immutable adapter quote for an LP-token amount."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetValuation {
    pub underlying_blnd: i128,
    pub usdc_value: i128,
    pub valid_until: u64,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Immutable assets a valuation adapter is authorized to price."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AdapterBinding {
    pub blnd: Address,
    pub blnd_usdc: Address,
    pub blnd_xlm: Address,
    pub usdc: Address,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Canonical value of one pool's active and queued backstop positions."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PoolValuation {
    pub active_blnd: BlndEmissionValues,
    pub active_values: ActivationValues,
    pub queued_values: ActivationValues,
    pub valid_until: u64,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Arithmetic-only quote for one v2-compatible pool-status transition."#
)]
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

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Reward-eligible, pool-specific tier values denominated in 7-decimal USDC."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TakeRateValues {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub usdc: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Allocation of one take-rate asset across the three backstop tiers."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TakeRateQuote {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub remainder: i128,
    pub usdc: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Eligible underlying BLND held through the two BLND-bearing backstop tiers,"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" denominated in 7-decimal BLND base units."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BlndEmissionValues {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Arithmetic-only allocation quote for one BLND-emission scope."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BlndEmissionQuote {
    pub allocation: i128,
    pub eligible_blnd: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Immutable top-level split of ongoing BLND actually received from the"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" emitter. `carry` remains at this scope for the next split."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct OngoingBlndSplit {
    pub backstop: i128,
    pub carry: i128,
    pub pool: i128,
    pub total: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Most recent completed distribution checkpoint used to guard reward-zone"#
)]
#[cfg_attr(not(target_family = "wasm"), doc = r#" membership edits."#)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RewardZoneCheckpoint {
    pub timestamp: u64,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Pool-local ongoing-emission state."#
)]
#[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" The active amounts are candidate-accounted LP-token assets, not underlying"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" BLND. They are synchronized from canonical tier shares and assets whenever"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" either BLND-bearing tier changes, allowing a maximum-size reward-zone"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" distribution to read one persistent entry per pool."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PoolOngoingEmissions {
    pub accrued_backstop: i128,
    pub accrued_pool: i128,
    pub active_blnd_usdc: i128,
    pub active_blnd_usdc_shares: i128,
    pub active_blnd_xlm: i128,
    pub active_blnd_xlm_shares: i128,
    pub backstop_tier_carry: i128,
    pub blnd_usdc_index: i128,
    pub blnd_usdc_index_carry: i128,
    pub blnd_xlm_index: i128,
    pub blnd_xlm_index_carry: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Pool-owned reservation of the 30% supply/borrow tranche."#
)]
#[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" A permissionless pool gulp moves accrued pool emissions into this"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" reservation no more than once per day. The registered pool then consumes"#
)]
#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" the reservation atomically as its users claim reserve-token emissions."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PoolEmissionReservation {
    pub available: i128,
    pub last_gulp: Option<u64>,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" One user's bounded ongoing-BLND checkpoint in one BLND-bearing tier."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct UserOngoingEmissions {
    pub accrued: i128,
    pub carry: i128,
    pub index: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Auditable aggregate accounting for ongoing BLND held by the candidate."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct OngoingEmissionState {
    pub backstop_allocated: i128,
    pub backstop_carry: i128,
    pub backstop_claimed: i128,
    pub last_distribution: Option<u64>,
    pub pool_allocated: i128,
    pub pool_carry: i128,
    pub split_carry: i128,
    pub total_claimed: i128,
    pub total_received: i128,
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Result of one completed permissionless ongoing-emission distribution."#
)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct OngoingDistribution {
    pub backstop_allocated: i128,
    pub backstop_carry: i128,
    pub checkpoint: u64,
    pub eligible_blnd: i128,
    pub pool_allocated: i128,
    pub pool_carry: i128,
    pub received: i128,
    pub split_carry: i128,
}

#[contractclient(name = "EmitterClient")]
pub trait Emitter {
    fn distribute(env: Env) -> i128;
    fn drop(env: Env, list: Vec<(Address, i128)>);
    fn get_backstop(env: Env) -> Address;
    fn get_last_distro(env: Env, backstop_id: Address) -> u64;
    fn get_queued_swap(env: Env) -> Option<EmitterSwap>;
    fn queue_swap_backstop(env: Env, new_backstop: Address, new_backstop_token: Address);
    fn swap_backstop(env: Env);
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Minimal fail-closed loss-state interface required from an attributed pool."#
)]
#[contractclient(name = "PoolLossClient")]
pub trait PoolLoss {
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return true only when this backstop has no committed or unresolved loss"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" for the pool and its expired queued shares may leave custody."#
    )]
    fn backstop_withdrawal_allowed(env: Env, backstop: Address) -> bool;
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Immutable registry of pools accepted by this backstop."#
)]
#[contractclient(name = "PoolFactoryClient")]
pub trait PoolFactory {
    fn backstop(env: Env) -> Address;
    fn is_pool(env: Env, pool: Address) -> bool;
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Immutable boundary for manipulation-resistant LP valuation."#
)]
#[contractclient(name = "ValuationAdapterClient")]
pub trait ValuationAdapter {
    fn version(env: Env) -> u32;
    fn binding(env: Env) -> AdapterBinding;
    fn quote(env: Env, token: Address, amount: i128) -> AssetValuation;
}

#[cfg_attr(
    not(target_family = "wasm"),
    doc = r#" Minimal current-composition interface implemented by both approved Comet LP"#
)]
#[cfg_attr(not(target_family = "wasm"), doc = r#" contracts."#)]
#[contractclient(name = "CometClient")]
pub trait Comet {
    fn get_balance(env: Env, token: Address) -> i128;
    fn get_total_supply(env: Env) -> i128;
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
    pub shares: i128,
    pub to: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadDebtLotCommitted {
    pub auction_id: BytesN<32>,
    pub pool: Address,
    pub quote: BadDebtLotQuote,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadDebtLotReleased {
    pub auction_id: BytesN<32>,
    pub pool: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadDebtLotSettled {
    pub auction_id: BytesN<32>,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Base token amount removed from the auction commitment."#
    )]
    pub base_lot_amount: i128,
    pub complete: bool,
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Time-scaled token amount transferred to the filler."#
    )]
    pub lot_amount: i128,
    pub pool: Address,
    pub tier: BackstopTier,
    pub to: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestLotCommitted {
    pub auction_id: BytesN<32>,
    pub pool: Address,
    pub quote: InterestLotQuote,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestLotReleased {
    pub auction_id: BytesN<32>,
    pub pool: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestLotSettled {
    pub auction_id: BytesN<32>,
    pub base_bid_amount: i128,
    pub bid_amount: i128,
    pub complete: bool,
    pub from: Address,
    pub pool: Address,
    pub tier: BackstopTier,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardZoneAdded {
    pub pool: Address,
    pub removed: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardZoneRemoved {
    pub pool: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OngoingBlndDistributed {
    pub backstop_allocated: i128,
    pub checkpoint: u64,
    pub pool_allocated: i128,
    pub received: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OngoingBlndClaimed {
    pub amount: i128,
    pub pool: Address,
    pub recipient: Address,
    pub user: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolEmissionsGulped {
    pub amount: i128,
    pub pool: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolEmissionsClaimed {
    pub amount: i128,
    pub pool: Address,
    pub recipient: Address,
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
        let adapter = ValuationAdapterClient::new(&env, &valuation_adapter);
        let expected_binding = AdapterBinding {
            blnd: blnd_token.clone(),
            blnd_usdc: legacy_backstop_token.clone(),
            blnd_xlm: next_backstop_token.clone(),
            usdc: prefund_token.clone(),
        };
        if adapter.version() != VALUATION_ADAPTER_VERSION || adapter.binding() != expected_binding {
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the legacy token held directly by this contract for migration."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The name preserves the incumbent v2 read interface."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the immutable BLND token used to verify funding and pay claims."#
    )]
    pub fn blnd_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::BlndToken).unwrap()
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return whether an exact emitter mint delta has verified the BLND token."#
    )]
    pub fn blnd_binding_verified(env: Env) -> bool {
        extend_instance_ttl(&env);
        env.storage().instance().has(&DataKey::BlndBindingVerified)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the immutable registry used to validate attributed pools."#
    )]
    pub fn pool_factory(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::PoolFactory).unwrap()
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the token the emitter will use to qualify the next migration."#
    )]
    pub fn next_backstop_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::NextBackstopToken)
            .unwrap()
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the immutable plain-USDC backstop token."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The legacy name is retained in the constructor and this read interface"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" because this token began as the candidate's isolated prefunding lane."#
    )]
    pub fn prefund_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::PrefundToken)
            .unwrap()
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the immutable plain-USDC backstop token."#
    )]
    pub fn usdc_token(env: Env) -> Address {
        extend_instance_ttl(&env);
        prefund_token(&env)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the immutable LP valuation adapter."#
    )]
    pub fn valuation_adapter(env: Env) -> Address {
        extend_instance_ttl(&env);
        valuation_adapter(&env)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the bounded v2-compatible reward-zone membership."#
    )]
    pub fn reward_zone(env: Env) -> Vec<Address> {
        extend_instance_ttl(&env);
        get_reward_zone(&env)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the most recent completed distribution checkpoint, if any."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The ongoing distributor writes this checkpoint only after it has"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" accounted through the returned timestamp."#
    )]
    pub fn reward_zone_checkpoint(env: Env) -> Option<RewardZoneCheckpoint> {
        extend_instance_ttl(&env);
        get_reward_zone_checkpoint(&env).map(|timestamp| RewardZoneCheckpoint { timestamp })
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Add a threshold-qualified, positive-BLND registered pool to the reward zone."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The first member needs no earlier distribution. Every later edit"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" requires a completed distribution checkpoint within the preceding hour."#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" If all 30 slots are occupied, `to_remove` must name a member with"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" strictly less current eligible underlying BLND."#
    )]
    pub fn add_reward(env: Env, to_add: Address, to_remove: Option<Address>) {
        extend_instance_ttl(&env);
        let removed = add_to_reward_zone(&env, &to_add, to_remove.as_ref());
        RewardZoneAdded {
            pool: to_add,
            removed,
        }
        .publish(&env);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Remove a member below maintenance or with zero eligible BLND."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Zero-weight removal needs no distribution checkpoint because it cannot"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" reallocate unresolved emissions."#
    )]
    pub fn remove_reward(env: Env, to_remove: Address) {
        extend_instance_ttl(&env);
        remove_from_reward_zone(&env, &to_remove);
        RewardZoneRemoved { pool: to_remove }.publish(&env);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Reconcile BLND actually held after emitter distribution and account it"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" across the bounded reward zone."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The call is permissionless and available only after this candidate is"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" the emitter's active backstop. Exact-delta backfill funding may have"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" already bound the configured BLND token to the emitter's minted asset;"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" otherwise, the first positive ongoing distribution establishes that"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" binding atomically from its own emitter-call balance delta. Both the"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" immutable 70/30 split and each reward-zone allocation retain their own"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" integer carry. The public reward-zone checkpoint is written only after"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" every pool accrual and aggregate conservation check succeeds."#
    )]
    pub fn distribute(env: Env) -> OngoingDistribution {
        extend_instance_ttl(&env);
        let distribution = distribute_ongoing_blnd(&env);
        OngoingBlndDistributed {
            backstop_allocated: distribution.backstop_allocated,
            checkpoint: distribution.checkpoint,
            pool_allocated: distribution.pool_allocated,
            received: distribution.received,
        }
        .publish(&env);
        distribution
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return aggregate ongoing-emission receipts, allocations, and carries."#
    )]
    pub fn ongoing_emission_state(env: Env) -> OngoingEmissionState {
        extend_instance_ttl(&env);
        get_ongoing_emission_state(&env)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one registered pool's active LP amounts and accrued 70/30"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" tranches."#)]
    pub fn pool_ongoing_emissions(env: Env, pool: Address) -> PoolOngoingEmissions {
        extend_instance_ttl(&env);
        require_registered_pool(&env, &pool);
        get_pool_ongoing_emissions(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one registered pool's reserved, unclaimed 30% tranche."#
    )]
    pub fn pool_emission_reservation(env: Env, pool: Address) -> PoolEmissionReservation {
        extend_instance_ttl(&env);
        require_registered_pool(&env, &pool);
        get_pool_emission_reservation(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Move one pool's accrued 30% tranche into its claim reservation."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The registered pool contract authorizes the call. A positive gulp is"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" available at most once per 24 hours, matching v2's pool cadence."#
    )]
    pub fn gulp_pool_emissions(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        let amount = gulp_pool_ongoing_emissions(&env, &pool);
        PoolEmissionsGulped { amount, pool }.publish(&env);
        amount
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Pay an authorized reserve-token emission claim from a pool reservation."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Only the registered pool contract can consume its reservation. The"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" candidate remains the BLND custodian until this atomic payout so its"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" exact balance-reconciliation invariant remains intact."#
    )]
    pub fn claim_pool_emissions(env: Env, pool: Address, recipient: Address, amount: i128) {
        extend_instance_ttl(&env);
        claim_reserved_pool_emissions(&env, &pool, &recipient, amount);
        PoolEmissionsClaimed {
            amount,
            pool,
            recipient,
        }
        .publish(&env);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's current checkpointed and pending ongoing BLND for an"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" eligible backstop tier without changing storage."#
    )]
    pub fn user_ongoing_emissions(
        env: Env,
        user: Address,
        pool: Address,
        tier: BackstopTier,
    ) -> UserOngoingEmissions {
        extend_instance_ttl(&env);
        require_emission_tier(&env, tier);
        require_registered_pool(&env, &pool);
        preview_user_ongoing_emissions(&env, tier, &user, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Claim one pool's accrued backstop-depositor BLND to a chosen recipient."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The position owner authorizes the claim. Both eligible tiers are"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" checkpointed, but neither the 30% pool tranche nor migration backfill is"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" included."#)]
    pub fn claim_ongoing_blnd(env: Env, user: Address, pool: Address, recipient: Address) -> i128 {
        extend_instance_ttl(&env);
        let amount = claim_user_ongoing_blnd(&env, &user, &pool, &recipient);
        OngoingBlndClaimed {
            amount,
            pool,
            recipient,
            user,
        }
        .publish(&env);
        amount
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Deposit BLND:XLM LP into its pool-attributed second-loss tier."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Shares are minted at the pool-tier's current exchange rate. The"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" attributed pool must be factory registered and expose the permanent"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" fail-closed withdrawal callback."#
    )]
    pub fn deposit_blnd_xlm(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_standard_tier(&env, BackstopTier::BlndXlm, from, pool, amount);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Deposit plain USDC into its pool-attributed third-loss tier."#
    )]
    pub fn deposit_usdc(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_standard_tier(&env, BackstopTier::Usdc, from, pool, amount);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Queue active BLND:XLM shares for withdrawal after the 17-day lock."#
    )]
    pub fn queue_blnd_xlm_withdrawal(
        env: Env,
        from: Address,
        pool: Address,
        shares: i128,
    ) -> Q4WEntry {
        extend_instance_ttl(&env);
        queue_standard_tier_withdrawal(&env, BackstopTier::BlndXlm, from, pool, shares)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Queue active USDC shares for withdrawal after the 17-day lock."#
    )]
    pub fn queue_usdc_withdrawal(env: Env, from: Address, pool: Address, shares: i128) -> Q4WEntry {
        extend_instance_ttl(&env);
        queue_standard_tier_withdrawal(&env, BackstopTier::Usdc, from, pool, shares)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return newest queued BLND:XLM shares to active status first."#
    )]
    pub fn dequeue_blnd_xlm_withdrawal(env: Env, from: Address, pool: Address, shares: i128) {
        extend_instance_ttl(&env);
        dequeue_standard_tier_withdrawal(&env, BackstopTier::BlndXlm, from, pool, shares);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return newest queued USDC shares to active status first."#
    )]
    pub fn dequeue_usdc_withdrawal(env: Env, from: Address, pool: Address, shares: i128) {
        extend_instance_ttl(&env);
        dequeue_standard_tier_withdrawal(&env, BackstopTier::Usdc, from, pool, shares);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Withdraw expired BLND:XLM shares after the pool confirms loss safety."#
    )]
    pub fn withdraw_blnd_xlm(env: Env, from: Address, pool: Address, shares: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_standard_tier(&env, BackstopTier::BlndXlm, from, pool, shares, to);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Withdraw expired USDC shares after the pool confirms loss safety."#
    )]
    pub fn withdraw_usdc(env: Env, from: Address, pool: Address, shares: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_standard_tier(&env, BackstopTier::Usdc, from, pool, shares, to);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's total shares in a fixed backstop tier."#
    )]
    pub fn tier_shares(env: Env, tier: BackstopTier, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => get_position(&env, &user, &pool).amount,
            _ => get_tier_shares(&env, tier, &user, &pool),
        }
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's queued shares in a fixed backstop tier."#
    )]
    pub fn tier_queued_shares(env: Env, tier: BackstopTier, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => queued_shares(&env, &get_blnd_usdc_q4w(&env, &user, &pool)),
            _ => queued_shares(&env, &get_tier_q4w(&env, tier, &user, &pool)),
        }
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's unqueued share basis for activation and emissions."#
    )]
    pub fn tier_active_shares(env: Env, tier: BackstopTier, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            Self::tier_shares(env.clone(), tier, user.clone(), pool.clone()),
            Self::tier_queued_shares(env.clone(), tier, user, pool),
        )
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's bounded withdrawal queue for a fixed tier."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all shares allocated to one pool in a fixed tier."#
    )]
    pub fn pool_tier_shares(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        pool_shares(&env, tier, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return accounted token base units allocated to one pool and tier."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Direct token transfers are excluded. Assets may be lower than shares"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" after a tier absorbs loss."#)]
    pub fn pool_tier_assets(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_tier_assets(&env, tier, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one pool's complete share-to-asset state for a fixed tier."#
    )]
    pub fn pool_tier_state(env: Env, tier: BackstopTier, pool: Address) -> PoolTierState {
        extend_instance_ttl(&env);
        get_pool_tier_state(&env, tier, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Preview shares minted by a token deposit at the current pool-tier rate."#
    )]
    pub fn preview_tier_deposit(env: Env, tier: BackstopTier, pool: Address, assets: i128) -> i128 {
        extend_instance_ttl(&env);
        require_positive(&env, assets);
        shares_from_assets(&env, assets, &get_pool_tier_state(&env, tier, &pool))
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Preview token base units redeemed by burning pool-tier shares."#
    )]
    pub fn preview_tier_withdrawal(
        env: Env,
        tier: BackstopTier,
        pool: Address,
        shares: i128,
    ) -> i128 {
        extend_instance_ttl(&env);
        require_positive(&env, shares);
        assets_from_shares(&env, shares, &get_pool_tier_state(&env, tier, &pool))
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all queued shares for one pool in a fixed tier."#
    )]
    pub fn pool_tier_queued_shares(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        match tier {
            BackstopTier::BlndUsdc => get_pool_blnd_usdc_queued_shares(&env, &pool),
            _ => get_pool_tier_queued_shares(&env, tier, &pool),
        }
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one pool's unqueued share basis for activation and emissions."#
    )]
    pub fn pool_tier_active_shares(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            Self::pool_tier_shares(env.clone(), tier, pool.clone()),
            Self::pool_tier_queued_shares(env.clone(), tier, pool),
        )
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all accounted shares in a fixed tier."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all accounted token base units in a fixed tier."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Direct transfers are deliberately excluded."#
    )]
    pub fn total_tier_assets(env: Env, tier: BackstopTier) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalTierAssets(tier))
            .unwrap_or(0)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return token base units reserved by this pool's active bad-debt lots."#
    )]
    pub fn pool_tier_committed_assets(env: Env, tier: BackstopTier, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_tier_committed_assets(&env, tier, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the bounded number of active bad-debt commitments for one pool."#
    )]
    pub fn pool_bad_debt_commitment_count(env: Env, pool: Address) -> u32 {
        extend_instance_ttl(&env);
        get_pool_bad_debt_commitment_count(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one pool-authorized bad-debt commitment, if present."#
    )]
    pub fn bad_debt_commitment(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
    ) -> Option<BadDebtLotQuote> {
        extend_instance_ttl(&env);
        get_bad_debt_commitment(&env, &pool).and_then(|commitment| {
            if commitment.auction_id == auction_id {
                Some(commitment.quote)
            } else {
                None
            }
        })
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one pool-authorized interest commitment, if present."#
    )]
    pub fn interest_commitment(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
    ) -> Option<InterestLotQuote> {
        extend_instance_ttl(&env);
        get_interest_commitment(&env, &pool).and_then(|commitment| {
            if commitment.auction_id == auction_id {
                Some(commitment.quote)
            } else {
                None
            }
        })
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all queued shares in a fixed tier."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the global unqueued share basis for activation and emissions."#
    )]
    pub fn total_tier_active_shares(env: Env, tier: BackstopTier) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            Self::total_tier_shares(env.clone(), tier),
            Self::total_tier_queued_shares(env.clone(), tier),
        )
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's total continuous BLND:USDC tier shares."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Includes shares queued for withdrawal. Before migration these shares"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" also qualify for migration backfill, and while in custody they remain"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" eligible for take-rate accounting and proportionally exposed to losses."#
    )]
    pub fn blnd_usdc_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_position(&env, &user, &pool).amount
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's unqueued share basis for activation and emissions."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Final eligibility also depends on migration lifecycle, pool status, and"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" asset-policy gates."#)]
    pub fn blnd_usdc_active_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        let total = get_position(&env, &user, &pool).amount;
        checked_nonnegative_sub_i128(
            &env,
            total,
            queued_shares(&env, &get_blnd_usdc_q4w(&env, &user, &pool)),
        )
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's total queued BLND:USDC shares."#
    )]
    pub fn blnd_usdc_queued_shares(env: Env, user: Address, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        queued_shares(&env, &get_blnd_usdc_q4w(&env, &user, &pool))
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one user's bounded FIFO withdrawal queue."#
    )]
    pub fn blnd_usdc_withdrawal_queue(env: Env, user: Address, pool: Address) -> Vec<Q4WEntry> {
        extend_instance_ttl(&env);
        get_blnd_usdc_q4w(&env, &user, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the total accounted BLND:USDC tier shares allocated to one pool."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Includes queued shares and is the canonical share amount for take-rate"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" and within-tier loss accounting while those shares remain in custody."#
    )]
    pub fn pool_blnd_usdc_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_blnd_usdc_shares(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one pool's queued BLND:USDC shares."#
    )]
    pub fn pool_blnd_usdc_queued_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        get_pool_blnd_usdc_queued_shares(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return one pool's unqueued share basis for activation and emissions."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Final eligibility also depends on migration lifecycle, pool status, and"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" asset-policy gates."#)]
    pub fn pool_blnd_usdc_active_shares(env: Env, pool: Address) -> i128 {
        extend_instance_ttl(&env);
        checked_nonnegative_sub_i128(
            &env,
            get_pool_blnd_usdc_shares(&env, &pool),
            get_pool_blnd_usdc_queued_shares(&env, &pool),
        )
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all accounted BLND:USDC tier shares."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Includes queued shares and is the canonical share amount for take-rate"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" and within-tier loss accounting. Direct token transfers do not create"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" shares and are excluded."#)]
    pub fn total_blnd_usdc_shares(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalBlndUsdcShares)
            .unwrap_or(0)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return all queued BLND:USDC tier shares."#
    )]
    pub fn total_blnd_usdc_queued_shares(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalBlndUsdcQueuedShares)
            .unwrap_or(0)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the global unqueued share basis for activation and emissions."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Final eligibility also depends on migration lifecycle, pool status, and"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" asset-policy gates."#)]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the inherited withdrawal-queue lock duration."#
    )]
    pub fn q4w_lock_seconds(env: Env) -> u64 {
        extend_instance_ttl(&env);
        Q4W_LOCK_SECONDS
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the threshold an inactive pool must meet to activate."#
    )]
    pub fn activation_entry_threshold(env: Env) -> i128 {
        extend_instance_ttl(&env);
        ACTIVATION_ENTRY_THRESHOLD_USDC
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the threshold an active pool must maintain to remain active."#
    )]
    pub fn activation_maintenance_threshold(env: Env) -> i128 {
        extend_instance_ttl(&env);
        ACTIVATION_MAINTENANCE_THRESHOLD_USDC
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote verified pool-specific values under the canonical activation"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" policy's hysteresis rule."#)]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" This function defines policy arithmetic only. Its arguments MUST come"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" from the canonical valuation layer before its result is used for pool"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" status or reward eligibility."#)]
    pub fn quote_activation(
        env: Env,
        values: ActivationValues,
        currently_active: bool,
    ) -> ActivationQuote {
        extend_instance_ttl(&env);
        quote_activation_values(&env, &values, currently_active)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Derive one pool's canonical value from the redeemable assets represented"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" by its active and queued shares. LP quotes come only from the immutable"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" adapter; plain USDC is valued one-for-one."#
    )]
    pub fn pool_valuation(env: Env, pool: Address) -> PoolValuation {
        extend_instance_ttl(&env);
        build_pool_valuation(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote a single-tier 120%-of-debt lot from uncommitted pool capital."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The registered pool is responsible for deriving `debt_value` from its"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" immutable reserve oracle. The first tier in strict BLND:USDC,"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" BLND:XLM, then USDC order with at least $200 of verified value is"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" selected without changing state."#
    )]
    pub fn quote_bad_debt_lot(
        env: Env,
        pool: Address,
        debt_value: i128,
    ) -> Option<BadDebtLotQuote> {
        extend_instance_ttl(&env);
        require_registered_pool(&env, &pool);
        build_bad_debt_lot_quote(&env, &pool, debt_value)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Reserve a canonical bad-debt lot for a registered pool."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Only the pool contract itself can authorize this direct cross-contract"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" call. Commitment does not transfer assets or impair shares; settlement"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" remains a later atomic transition."#
    )]
    pub fn commit_bad_debt_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        debt_value: i128,
    ) -> BadDebtLotQuote {
        extend_instance_ttl(&env);
        pool.require_auth();
        require_registered_pool(&env, &pool);
        if get_bad_debt_commitment(&env, &pool).is_some() {
            panic_with_error!(&env, BackstopError::BadDebtCommitmentExists);
        }

        let quote = build_bad_debt_lot_quote(&env, &pool, debt_value)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::NoBadDebtLossCapacity));
        set_bad_debt_commitment(
            &env,
            &pool,
            &BadDebtCommitment {
                auction_id: auction_id.clone(),
                quote: quote.clone(),
            },
        );
        BadDebtLotCommitted {
            auction_id,
            pool,
            quote: quote.clone(),
        }
        .publish(&env);
        quote
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Release one unfilled or stale bad-debt lot back to available capital."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Only the pool that created the commitment can authorize release."#
    )]
    pub fn release_bad_debt_lot(env: Env, pool: Address, auction_id: BytesN<32>) {
        extend_instance_ttl(&env);
        pool.require_auth();
        require_registered_pool(&env, &pool);
        let commitment = get_bad_debt_commitment(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::BadDebtCommitmentNotFound));
        if commitment.auction_id != auction_id {
            panic_with_error!(&env, BackstopError::BadDebtCommitmentNotFound);
        }
        remove_bad_debt_commitment(&env, &pool);
        BadDebtLotReleased { auction_id, pool }.publish(&env);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Settle one partial or complete fill of a committed bad-debt lot."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" `base_lot_amount` is removed from the stored auction commitment."#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" `lot_amount` is the smaller, time-scaled amount actually transferred"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" and therefore the only amount that impairs the tier exchange rate."#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Only the registered pool can authorize this transition."#
    )]
    pub fn settle_bad_debt_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        base_lot_amount: i128,
        lot_amount: i128,
        to: Address,
    ) -> Option<BadDebtLotQuote> {
        extend_instance_ttl(&env);
        pool.require_auth();
        require_registered_pool(&env, &pool);
        if base_lot_amount < 0 || lot_amount < 0 || lot_amount > base_lot_amount {
            panic_with_error!(&env, BackstopError::InvalidAmount);
        }

        let mut commitment = get_bad_debt_commitment(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::BadDebtCommitmentNotFound));
        if commitment.auction_id != auction_id || base_lot_amount > commitment.quote.lot_amount {
            panic_with_error!(&env, BackstopError::BadDebtCommitmentNotFound);
        }

        let tier = commitment.quote.tier;
        let committed_lot_amount = commitment.quote.lot_amount;
        apply_pool_tier_loss(&env, tier, &pool, lot_amount);
        if lot_amount > 0 {
            TokenClient::new(&env, &tier_token(&env, tier)).transfer(
                &env.current_contract_address(),
                &to,
                &lot_amount,
            );
        }

        let remaining_lot_amount = checked_sub_i128(&env, committed_lot_amount, base_lot_amount);
        let remaining = if remaining_lot_amount == 0 {
            remove_bad_debt_commitment(&env, &pool);
            None
        } else {
            commitment.quote.committed_value = proportional_floor(
                &env,
                commitment.quote.committed_value,
                remaining_lot_amount,
                committed_lot_amount,
            );
            commitment.quote.lot_amount = remaining_lot_amount;
            set_bad_debt_commitment(&env, &pool, &commitment);
            Some(commitment.quote)
        };

        BadDebtLotSettled {
            auction_id,
            base_lot_amount,
            complete: remaining.is_none(),
            lot_amount,
            pool,
            tier,
            to,
        }
        .publish(&env);
        remaining
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Reserve the selected tier-token bid for one pool interest auction."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Only the registered pool may authorize this state transition. The"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" reserve-asset lot remains in pool custody until a later atomic fill."#
    )]
    pub fn commit_interest_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        tier: BackstopTier,
        lot_value: i128,
    ) -> InterestLotQuote {
        extend_instance_ttl(&env);
        pool.require_auth();
        require_registered_pool(&env, &pool);
        if get_interest_commitment(&env, &pool).is_some() {
            panic_with_error!(&env, BackstopError::InterestCommitmentExists);
        }
        if pool_shares(&env, tier, &pool) == 0 {
            panic_with_error!(&env, BackstopError::InvalidInterestLot);
        }

        let quote = build_interest_lot_quote(&env, tier, lot_value);
        set_interest_commitment(
            &env,
            &pool,
            &InterestCommitment {
                auction_id: auction_id.clone(),
                quote: quote.clone(),
            },
        );
        InterestLotCommitted {
            auction_id,
            pool,
            quote: quote.clone(),
        }
        .publish(&env);
        quote
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Release a stale interest-auction bid without changing tier assets."#
    )]
    pub fn release_interest_lot(env: Env, pool: Address, auction_id: BytesN<32>) {
        extend_instance_ttl(&env);
        pool.require_auth();
        require_registered_pool(&env, &pool);
        let commitment = get_interest_commitment(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::InterestCommitmentNotFound));
        if commitment.auction_id != auction_id {
            panic_with_error!(&env, BackstopError::InterestCommitmentNotFound);
        }
        remove_interest_commitment(&env, &pool);
        InterestLotReleased { auction_id, pool }.publish(&env);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Donate a time-scaled tier-token bid into one pool-tier without shares."#
    )]
    pub fn settle_interest_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        base_bid_amount: i128,
        bid_amount: i128,
        from: Address,
    ) -> Option<InterestLotQuote> {
        extend_instance_ttl(&env);
        pool.require_auth();
        require_registered_pool(&env, &pool);
        if base_bid_amount < 0
            || bid_amount < 0
            || bid_amount > base_bid_amount
            || from == pool
            || from == env.current_contract_address()
        {
            panic_with_error!(&env, BackstopError::InvalidInterestLot);
        }

        let mut commitment = get_interest_commitment(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::InterestCommitmentNotFound));
        if commitment.auction_id != auction_id || base_bid_amount > commitment.quote.bid_amount {
            panic_with_error!(&env, BackstopError::InterestCommitmentNotFound);
        }

        let tier = commitment.quote.tier;
        if bid_amount > 0 {
            let token = TokenClient::new(&env, &tier_token(&env, tier));
            let backstop = env.current_contract_address();
            let from_before = token.balance(&from);
            let backstop_before = token.balance(&backstop);
            token.transfer(&from, &backstop, &bid_amount);
            if token.balance(&from) != checked_sub_i128(&env, from_before, bid_amount)
                || token.balance(&backstop) != checked_add_i128(&env, backstop_before, bid_amount)
            {
                panic_with_error!(&env, BackstopError::InvalidInterestLot);
            }
            apply_pool_tier_gain(&env, tier, &pool, bid_amount);
        }

        let previous_bid_amount = commitment.quote.bid_amount;
        let remaining_bid_amount = checked_sub_i128(&env, previous_bid_amount, base_bid_amount);
        let remaining = if remaining_bid_amount == 0 {
            remove_interest_commitment(&env, &pool);
            None
        } else {
            commitment.quote.lot_value = proportional_floor(
                &env,
                commitment.quote.lot_value,
                remaining_bid_amount,
                previous_bid_amount,
            );
            commitment.quote.bid_amount = remaining_bid_amount;
            set_interest_commitment(&env, &pool, &commitment);
            Some(commitment.quote)
        };

        InterestLotSettled {
            auction_id,
            base_bid_amount,
            bid_amount,
            complete: remaining.is_none(),
            from,
            pool,
            tier,
        }
        .publish(&env);
        remaining
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote activation from canonical pool accounting and immutable pricing."#
    )]
    pub fn quote_pool_activation(
        env: Env,
        pool: Address,
        currently_active: bool,
    ) -> ActivationQuote {
        extend_instance_ttl(&env);
        let valuation = build_pool_valuation(&env, &pool);
        quote_activation_values(&env, &valuation.active_values, currently_active)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote a permissionless status refresh from canonical pool accounting."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote a pool-admin status request from canonical pool accounting."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Authorization and stored-status mutation remain the pool's"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" responsibility."#)]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote the permissionless v2-compatible status refresh."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Active and queued values MUST use the same canonical, manipulation-"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" resistant USDC valuation. This function defines arithmetic only; the"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" production pool remains responsible for storing and enforcing status."#
    )]
    pub fn quote_update_pool_status(
        env: Env,
        current_status: u32,
        active_values: ActivationValues,
        queued_values: ActivationValues,
    ) -> PoolStatusQuote {
        extend_instance_ttl(&env);
        quote_update_pool_status_values(&env, current_status, &active_values, &queued_values)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote a pool-local admin's v2-compatible status request."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" This function performs no authorization and defines arithmetic only."#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The production pool MUST require its current admin and MUST NOT grant"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" that admin any protocol-wide or withdrawal-safety authority."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote one take-rate asset across the three weighted tiers."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The result is arithmetic only. Values MUST come from the canonical"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" reward-eligibility layer before the quote is used for accrual."#
    )]
    pub fn quote_take_rate(env: Env, distribution: i128, values: TakeRateValues) -> TakeRateQuote {
        extend_instance_ttl(&env);
        quote_take_rate_values(&env, distribution, &values)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Allocate a bounded reserve-credit batch from canonical pool-tier value."#
    )]
    pub fn quote_pool_take_rate_batch(
        env: Env,
        pool: Address,
        distributions: Map<Address, i128>,
    ) -> Map<Address, TakeRateQuote> {
        extend_instance_ttl(&env);
        require_registered_pool(&env, &pool);
        if distributions.is_empty() || distributions.len() > 4 {
            panic_with_error!(&env, BackstopError::InvalidTakeRateValue);
        }
        let values = build_take_rate_values(&env, &pool);
        let mut quotes = Map::new(&env);
        for (asset, distribution) in distributions.iter() {
            quotes.set(asset, quote_take_rate_values(&env, distribution, &values));
        }
        quotes
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote a reward-zone pool's share of the backstop-depositor BLND tranche."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" `total_reward_zone_blnd` is the eligible underlying BLND across every"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" reward-zone pool. This is an arithmetic-only diagnostic boundary."#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Production accrual MUST obtain each tier's values from canonical"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" candidate shares and `spot_blnd_emission_values`, not untrusted caller"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" input."#)]
    pub fn quote_pool_blnd_emissions(
        env: Env,
        distribution: i128,
        values: BlndEmissionValues,
        total_reward_zone_blnd: i128,
        reward_zone_member: bool,
    ) -> BlndEmissionQuote {
        extend_instance_ttl(&env);
        quote_pool_blnd_emission_values(
            &env,
            distribution,
            &values,
            total_reward_zone_blnd,
            reward_zone_member,
        )
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote one user's share of a reward-zone pool's BLND-emission allocation."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" `pool_eligible_blnd` is the eligible underlying BLND held through both"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" BLND-bearing tiers for the pool. This is an arithmetic-only diagnostic"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" boundary. Production accrual MUST obtain each tier's values from"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" canonical candidate shares and `spot_blnd_emission_values`."#
    )]
    pub fn quote_user_blnd_emissions(
        env: Env,
        pool_distribution: i128,
        values: BlndEmissionValues,
        pool_eligible_blnd: i128,
    ) -> BlndEmissionQuote {
        extend_instance_ttl(&env);
        quote_user_blnd_emission_values(&env, pool_distribution, &values, pool_eligible_blnd)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Convert current BLND:USDC and BLND:XLM LP-token amounts to underlying"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" BLND using each Comet's same-invocation reserve composition."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" This is the canonical conversion boundary for ongoing emissions. The"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" caller-supplied amounts make this a quote only; production accrual MUST"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" supply candidate-accounted active amounts after checkpointing."#
    )]
    pub fn spot_blnd_emission_values(
        env: Env,
        blnd_usdc_lp: i128,
        blnd_xlm_lp: i128,
    ) -> BlndEmissionValues {
        extend_instance_ttl(&env);
        BlndEmissionValues {
            blnd_usdc: spot_underlying_blnd(&env, BackstopTier::BlndUsdc, blnd_usdc_lp),
            blnd_xlm: spot_underlying_blnd(&env, BackstopTier::BlndXlm, blnd_xlm_lp),
        }
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Derive one registered pool's canonical current emission weight from"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" accounted active assets in both BLND-bearing tiers."#
    )]
    pub fn pool_spot_blnd_emission_values(env: Env, pool: Address) -> BlndEmissionValues {
        extend_instance_ttl(&env);
        pool_spot_blnd_values(&env, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote the immutable 70% backstop / 30% pool split, including carry"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" retained from the preceding top-level split."#
    )]
    pub fn quote_ongoing_blnd_split(
        env: Env,
        distribution: i128,
        prior_carry: i128,
    ) -> OngoingBlndSplit {
        extend_instance_ttl(&env);
        quote_ongoing_blnd_split_values(&env, distribution, prior_carry)
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the registered original queue's immutable unlock anchor."#
    )]
    pub fn original_unlock(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::OriginalUnlock)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return when the bounded prefunding campaign began."#
    )]
    pub fn prefunding_start(env: Env) -> u64 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::PrefundingStart)
            .unwrap()
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the deployment-anchored maximum migration deadline."#
    )]
    pub fn absolute_migration_deadline(env: Env) -> u64 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::AbsoluteMigrationDeadline)
            .unwrap()
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return when the original legacy-emitter queue began."#
    )]
    pub fn migration_epoch_start(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::MigrationEpochStart)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the queue currently approved for finalization or synchronization."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the absolute end of the two-retry recovery horizon."#
    )]
    pub fn backfill_cap(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::OriginalUnlock)
            .map(|unlock| recovery_end(&env, unlock))
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the final time-weighted backfill endpoint after activation."#
    )]
    pub fn backfill_end(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::BackfillEnd)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Deposit BLND:USDC into its continuous pool-attributed backstop tier."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Before migration, these same shares also count toward legacy-emitter"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" qualification and earn amount-times-time backfill weight until the"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" currently verified queue unlocks. Deposits at or after that unlock are"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" ordinary tier shares and earn no backfill for that queue."#
    )]
    pub fn deposit_blnd_usdc(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_blnd_usdc_position(&env, from, pool, amount);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Compatibility alias for `deposit_blnd_usdc`."#
    )]
    pub fn deposit_migration(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        deposit_blnd_usdc_position(&env, from, pool, amount);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Queue active BLND:USDC shares for withdrawal after the 17-day lock."#
    )]
    pub fn queue_blnd_usdc_withdrawal(
        env: Env,
        from: Address,
        pool: Address,
        amount: i128,
    ) -> Q4WEntry {
        extend_instance_ttl(&env);
        queue_blnd_usdc_withdrawal(&env, from, pool, amount)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return queued BLND:USDC shares to active status, newest entries first."#
    )]
    pub fn dequeue_blnd_usdc_withdrawal(env: Env, from: Address, pool: Address, amount: i128) {
        extend_instance_ttl(&env);
        dequeue_blnd_usdc_withdrawal(&env, from, pool, amount);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Withdraw expired queued BLND:USDC tier shares, oldest entries first."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Before emitter migration succeeds, withdrawn shares forfeit their"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" proportional accumulated backfill weight. The attributed pool must"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" affirm that this backstop has no committed or unresolved loss; a failed,"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" missing, or false pool response blocks withdrawal."#
    )]
    pub fn withdraw_blnd_usdc(env: Env, from: Address, pool: Address, amount: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_blnd_usdc_position(&env, from, pool, amount, to);
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Compatibility alias for the queued `withdraw_blnd_usdc` operation."#
    )]
    pub fn withdraw_migration(env: Env, from: Address, pool: Address, amount: i128, to: Address) {
        extend_instance_ttl(&env);
        withdraw_blnd_usdc_position(&env, from, pool, amount, to);
    }

    pub fn migration_position(env: Env, user: Address, pool: Address) -> MigrationPosition {
        extend_instance_ttl(&env);
        get_position(&env, &user, &pool)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return a position's finalized amount-times-seconds allocation weight."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the deterministic 1-BLND-per-second backfill scheduled at activation."#
    )]
    pub fn scheduled_backfill(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::ScheduledBackfill)
            .unwrap_or_else(|| panic_with_error!(&env, BackstopError::MigrationNotActive))
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the exact scheduled amount received through the emitter's one-time drop."#
    )]
    pub fn funded_backfill(env: Env) -> Option<i128> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::BackfillFundedAmount)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the sum of successful position claims."#
    )]
    pub fn total_backfill_claimed(env: Env) -> i128 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::TotalBackfillClaimed)
            .unwrap_or(0)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return funded BLND that remains reserved for position claims and rounding dust."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Permissionlessly fund the exact scheduled backfill through the legacy emitter."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The candidate is the sole drop recipient. The call verifies the exact BLND"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" balance delta before recording the reserve and can succeed only once."#
    )]
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
        env.storage()
            .instance()
            .set(&DataKey::BlndBindingVerified, &true);

        BackfillFunded { amount: scheduled }.publish(&env);
        scheduled
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Claim one position's immutable, time-weighted BLND backfill allocation."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The position owner authorizes the destination. Each surviving position"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" can claim once; floor-rounding dust remains in the backfill reserve."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Quote a position's proportional share of scheduled BLND backpay."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" This reports the immutable entitlement and does not transfer BLND or"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" subtract an amount that the position has already claimed."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return when this contract first synchronized its active lifecycle state."#
    )]
    pub fn activated_at(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::ActivatedAt)
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Return the final timestamp at which a bypassed direct swap may synchronize."#
    )]
    pub fn sync_deadline(env: Env) -> Option<u64> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::VerifiedQueueUnlock)
            .map(|cutoff| checked_sync_deadline(&env, cutoff))
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Open the public accounting epoch from the original emitter queue."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" This does not verify the queue for finalization; that remains restricted"#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#" to the final seven days."#)]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Atomically create the canonical legacy-emitter queue and open its epoch."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Verify an emitter queue and commit its unlock as the active queue anchor."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" The first unlock and absolute recovery horizon are immutable. Up to two"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" replacement queues may be verified within that horizon."#
    )]
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

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Finalize the queued legacy-emitter swap and activate this candidate."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" This operation is permissionless. The emitter enforces its queue delay"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" and strictly-greater legacy-token balance requirement."#
    )]
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

        activate(&env, env.ledger().timestamp())
    }

    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Synchronize lifecycle state after the emitter was finalized directly."#
    )]
    #[cfg_attr(not(target_family = "wasm"), doc = r#""#)]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" Backfill remains bounded by the immutable recovery horizon recorded"#
    )]
    #[cfg_attr(
        not(target_family = "wasm"),
        doc = r#" during initial preparation, regardless of when synchronization occurs."#
    )]
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

        // The emitter may have started sending ongoing BLND immediately after
        // a direct swap. Cap recovery-path backfill at the earliest verified
        // swap time so the synchronization delay cannot earn under both
        // regimes.
        activate(&env, verified_unlock)
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

fn activate(env: &Env, requested_backfill_end: u64) -> u64 {
    let original_unlock = require_prepared(env);
    let activated_at = env.ledger().timestamp();
    if requested_backfill_end > activated_at {
        panic_with_error!(env, BackstopError::InvalidOngoingBalance);
    }
    let candidate = env.current_contract_address();
    let emitter = EmitterClient::new(env, &get_emitter(env));
    let last_distribution = emitter.get_last_distro(&candidate);
    if last_distribution > activated_at {
        panic_with_error!(env, BackstopError::InvalidOngoingBalance);
    }
    let backfill_cap = recovery_end(env, original_unlock);
    let backfill_end = requested_backfill_end.min(backfill_cap);
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
    set_ongoing_emission_state(
        env,
        &OngoingEmissionState {
            backstop_allocated: 0,
            backstop_carry: 0,
            backstop_claimed: 0,
            last_distribution: Some(last_distribution),
            pool_allocated: 0,
            pool_carry: 0,
            split_carry: 0,
            total_claimed: 0,
            total_received: 0,
        },
    );
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

fn backfill_deposit_eligible_for_current_queue(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, u64>(&DataKey::VerifiedQueueUnlock)
        .is_none_or(|unlock| env.ledger().timestamp() < unlock)
}

fn deposit_blnd_usdc_position(env: &Env, from: Address, pool: Address, amount: i128) {
    require_positive(env, amount);
    from.require_auth();
    require_compatible_pool(env, &pool);
    if interest_tier_locked(env, BackstopTier::BlndUsdc, &pool) {
        panic_with_error!(env, BackstopError::InterestTierLocked);
    }

    let state = get_pool_tier_state(env, BackstopTier::BlndUsdc, &pool);
    let shares = shares_from_assets(env, amount, &state);
    require_positive(env, shares);
    require_recent_ongoing_checkpoint_for_weight_change(env, BackstopTier::BlndUsdc, &pool);
    checkpoint_user_ongoing_for_weight_change(env, BackstopTier::BlndUsdc, &from, &pool);

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
        if backfill_deposit_eligible_for_current_queue(env) {
            let timestamp = eligibility_timestamp(env, env.ledger().timestamp());
            let time_basis_delta = checked_mul_i128(env, amount, timestamp as i128);
            position.backfill_amount = checked_add_i128(env, position.backfill_amount, amount);
            position.time_basis = checked_add_i128(env, position.time_basis, time_basis_delta);
            add_to_instance_total(env, DataKey::TotalMigrationAmount, amount);
            add_to_instance_total(env, DataKey::TotalMigrationTimeBasis, time_basis_delta);
        }
    }
    position.amount = checked_add_i128(env, position.amount, shares);
    set_position(env, &from, &pool, &position);

    set_pool_blnd_usdc_shares(env, &pool, checked_add_i128(env, state.shares, shares));
    set_pool_tier_assets(
        env,
        BackstopTier::BlndUsdc,
        &pool,
        checked_add_i128(env, state.assets, amount),
    );
    refresh_pool_ongoing_assets(env, &pool);
    add_to_instance_total(env, DataKey::TotalBlndUsdcShares, shares);
    add_to_instance_total(
        env,
        DataKey::TotalTierAssets(BackstopTier::BlndUsdc),
        amount,
    );

    TokenClient::new(env, &legacy_backstop_token(env)).transfer(
        &from,
        env.current_contract_address(),
        &amount,
    );
    TierDeposited {
        amount,
        depositor: from,
        pool,
        shares,
        tier: BackstopTier::BlndUsdc,
    }
    .publish(env);
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
    if interest_tier_locked(env, tier, &pool) {
        panic_with_error!(env, BackstopError::InterestTierLocked);
    }

    let state = get_pool_tier_state(env, tier, &pool);
    let shares = shares_from_assets(env, amount, &state);
    require_positive(env, shares);
    require_recent_ongoing_checkpoint_for_weight_change(env, tier, &pool);
    checkpoint_user_ongoing_for_weight_change(env, tier, &from, &pool);

    // Keep the companion queue live whenever its position is touched.
    get_tier_q4w(env, tier, &from, &pool);
    let user_shares = get_tier_shares(env, tier, &from, &pool);
    set_tier_shares(
        env,
        tier,
        &from,
        &pool,
        checked_add_i128(env, user_shares, shares),
    );
    set_pool_tier_shares(
        env,
        tier,
        &pool,
        checked_add_i128(env, state.shares, shares),
    );
    set_pool_tier_assets(
        env,
        tier,
        &pool,
        checked_add_i128(env, state.assets, amount),
    );
    refresh_pool_ongoing_assets_for_tier(env, tier, &pool);
    add_to_instance_total(env, DataKey::TotalTierShares(tier), shares);
    add_to_instance_total(env, DataKey::TotalTierAssets(tier), amount);

    TokenClient::new(env, &tier_token(env, tier)).transfer(
        &from,
        env.current_contract_address(),
        &amount,
    );
    TierDeposited {
        amount,
        depositor: from,
        pool,
        shares,
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
    require_recent_ongoing_checkpoint_for_weight_change(env, tier, &pool);
    checkpoint_user_ongoing_for_weight_change(env, tier, &from, &pool);

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
    refresh_pool_ongoing_assets_for_tier(env, tier, &pool);
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
    require_recent_ongoing_checkpoint_for_weight_change(env, tier, &pool);
    checkpoint_user_ongoing_for_weight_change(env, tier, &from, &pool);
    set_tier_q4w(env, tier, &from, &pool, &queue);
    add_pool_tier_queued_shares(env, tier, &pool, -shares);
    refresh_pool_ongoing_assets_for_tier(env, tier, &pool);
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
    if interest_tier_locked(env, tier, &pool)
        || get_pool_bad_debt_commitment_count(env, &pool) != 0
        || !PoolLossClient::new(env, &pool).backstop_withdrawal_allowed(&backstop)
    {
        panic_with_error!(env, BackstopError::PoolWithdrawalBlocked);
    }
    let state = get_pool_tier_state(env, tier, &pool);
    let assets = assets_from_shares(env, shares, &state);
    require_recent_ongoing_checkpoint_for_weight_change(env, tier, &pool);
    checkpoint_user_ongoing_for_weight_change(env, tier, &from, &pool);
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
    if shares > state.shares {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    set_pool_tier_shares(
        env,
        tier,
        &pool,
        checked_sub_i128(env, state.shares, shares),
    );
    set_pool_tier_assets(
        env,
        tier,
        &pool,
        checked_sub_i128(env, state.assets, assets),
    );
    add_pool_tier_queued_shares(env, tier, &pool, -shares);
    refresh_pool_ongoing_assets_for_tier(env, tier, &pool);
    add_to_instance_total(env, DataKey::TotalTierShares(tier), -shares);
    add_to_instance_total(env, DataKey::TotalTierQueuedShares(tier), -shares);
    add_to_instance_total(env, DataKey::TotalTierAssets(tier), -assets);

    if assets > 0 {
        TokenClient::new(env, &tier_token(env, tier)).transfer(
            &env.current_contract_address(),
            &to,
            &assets,
        );
    }
    TierWithdrawn {
        amount: assets,
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

fn require_emission_tier(env: &Env, tier: BackstopTier) {
    if tier == BackstopTier::Usdc {
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
    require_registered_pool(env, pool);

    // Probe interface availability before accepting custody. Either boolean is
    // valid here: an existing loss may block withdrawal without blocking new
    // capital. A missing or failed callback aborts the deposit atomically.
    let backstop = env.current_contract_address();
    let _ = PoolLossClient::new(env, pool).backstop_withdrawal_allowed(&backstop);
}

fn require_registered_pool(env: &Env, pool: &Address) {
    let factory: Address = env.storage().instance().get(&DataKey::PoolFactory).unwrap();
    if !PoolFactoryClient::new(env, &factory).is_pool(pool) {
        panic_with_error!(env, BackstopError::InvalidPool);
    }
}

fn get_reward_zone(env: &Env) -> Vec<Address> {
    let key = PersistentKey::RewardZone;
    let reward_zone = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    extend_persistent_ttl_if_present(env, &key);
    if reward_zone.len() > MAX_REWARD_ZONE_SIZE {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    reward_zone
}

fn set_reward_zone(env: &Env, reward_zone: &Vec<Address>) {
    if reward_zone.len() > MAX_REWARD_ZONE_SIZE {
        panic_with_error!(env, BackstopError::RewardZoneFull);
    }
    let key = PersistentKey::RewardZone;
    env.storage().persistent().set(&key, reward_zone);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_reward_zone_checkpoint(env: &Env) -> Option<u64> {
    let key = PersistentKey::RewardZoneCheckpoint;
    let checkpoint = env.storage().persistent().get(&key);
    extend_persistent_ttl_if_present(env, &key);
    checkpoint
}

fn require_recent_distribution_checkpoint(env: &Env) {
    let now = env.ledger().timestamp();
    let checkpoint = get_reward_zone_checkpoint(env)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::DistributionCheckpointRequired));
    if checkpoint > now
        || now
            .checked_sub(checkpoint)
            .is_none_or(|age| age > REWARD_ZONE_CHECKPOINT_MAX_AGE_SECONDS)
    {
        panic_with_error!(env, BackstopError::DistributionCheckpointRequired);
    }
}

fn add_to_reward_zone(env: &Env, to_add: &Address, to_remove: Option<&Address>) -> Option<Address> {
    require_no_unsynchronized_emitter_recipient(env);
    let mut reward_zone = get_reward_zone(env);
    if reward_zone.contains(to_add.clone()) {
        panic_with_error!(env, BackstopError::InvalidRewardZoneEntry);
    }

    let valuation = build_pool_valuation(env, to_add);
    if !quote_activation_values(env, &valuation.active_values, false).meets_threshold {
        panic_with_error!(env, BackstopError::InvalidRewardZoneEntry);
    }
    let entrant_weight = eligible_blnd(env, &pool_spot_blnd_values(env, to_add));
    if entrant_weight <= 0 {
        panic_with_error!(env, BackstopError::InvalidRewardZoneEntry);
    }
    if !reward_zone.is_empty() {
        require_recent_distribution_checkpoint(env);
    }

    let removed = if reward_zone.len() < MAX_REWARD_ZONE_SIZE {
        None
    } else {
        let to_remove =
            to_remove.unwrap_or_else(|| panic_with_error!(env, BackstopError::RewardZoneFull));
        let remove_index = reward_zone
            .first_index_of(to_remove.clone())
            .unwrap_or_else(|| panic_with_error!(env, BackstopError::InvalidRewardZoneEntry));
        let removed_weight = eligible_blnd(env, &pool_spot_blnd_values(env, to_remove));
        if entrant_weight <= removed_weight {
            panic_with_error!(env, BackstopError::InvalidRewardZoneEntry);
        }
        reward_zone.remove(remove_index);
        Some(to_remove.clone())
    };

    reward_zone.push_front(to_add.clone());
    refresh_pool_ongoing_assets(env, to_add);
    set_reward_zone(env, &reward_zone);
    removed
}

fn remove_from_reward_zone(env: &Env, to_remove: &Address) {
    require_no_unsynchronized_emitter_recipient(env);
    let mut reward_zone = get_reward_zone(env);
    let remove_index = reward_zone
        .first_index_of(to_remove.clone())
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::InvalidRewardZoneEntry));
    let removed_weight = eligible_blnd(env, &pool_spot_blnd_values(env, to_remove));
    if removed_weight > 0 {
        let valuation = build_pool_valuation(env, to_remove);
        if quote_activation_values(env, &valuation.active_values, true).meets_threshold {
            panic_with_error!(env, BackstopError::InvalidRewardZoneEntry);
        }
        require_recent_distribution_checkpoint(env);
    }
    reward_zone.remove(remove_index);
    set_reward_zone(env, &reward_zone);
}

fn pool_spot_blnd_values(env: &Env, pool: &Address) -> BlndEmissionValues {
    require_registered_pool(env, pool);
    BlndEmissionValues {
        blnd_usdc: spot_underlying_blnd(
            env,
            BackstopTier::BlndUsdc,
            pool_active_emission_assets(env, BackstopTier::BlndUsdc, pool),
        ),
        blnd_xlm: spot_underlying_blnd(
            env,
            BackstopTier::BlndXlm,
            pool_active_emission_assets(env, BackstopTier::BlndXlm, pool),
        ),
    }
}

fn pool_active_emission_assets(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    let state = get_pool_tier_state(env, tier, pool);
    let active_shares = checked_nonnegative_sub_i128(env, state.shares, state.queued_shares);
    assets_from_shares(env, active_shares, &state)
}

fn get_pool_ongoing_emissions(env: &Env, pool: &Address) -> PoolOngoingEmissions {
    let key = PersistentKey::PoolOngoingEmissions(pool.clone());
    let state = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(PoolOngoingEmissions {
            accrued_backstop: 0,
            accrued_pool: 0,
            active_blnd_usdc: 0,
            active_blnd_usdc_shares: 0,
            active_blnd_xlm: 0,
            active_blnd_xlm_shares: 0,
            backstop_tier_carry: 0,
            blnd_usdc_index: 0,
            blnd_usdc_index_carry: 0,
            blnd_xlm_index: 0,
            blnd_xlm_index_carry: 0,
        });
    extend_persistent_ttl_if_present(env, &key);
    if state.accrued_backstop < 0
        || state.accrued_pool < 0
        || state.active_blnd_usdc < 0
        || state.active_blnd_usdc_shares < 0
        || state.active_blnd_xlm < 0
        || state.active_blnd_xlm_shares < 0
        || state.backstop_tier_carry < 0
        || state.blnd_usdc_index < 0
        || state.blnd_usdc_index_carry < 0
        || state.blnd_xlm_index < 0
        || state.blnd_xlm_index_carry < 0
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    state
}

fn set_pool_ongoing_emissions(env: &Env, pool: &Address, state: &PoolOngoingEmissions) {
    if state.accrued_backstop < 0
        || state.accrued_pool < 0
        || state.active_blnd_usdc < 0
        || state.active_blnd_usdc_shares < 0
        || state.active_blnd_xlm < 0
        || state.active_blnd_xlm_shares < 0
        || state.backstop_tier_carry < 0
        || state.blnd_usdc_index < 0
        || state.blnd_usdc_index_carry < 0
        || state.blnd_xlm_index < 0
        || state.blnd_xlm_index_carry < 0
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let key = PersistentKey::PoolOngoingEmissions(pool.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_pool_emission_reservation(env: &Env, pool: &Address) -> PoolEmissionReservation {
    let key = PersistentKey::PoolEmissionReservation(pool.clone());
    let reservation = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(PoolEmissionReservation {
            available: 0,
            last_gulp: None,
        });
    extend_persistent_ttl_if_present(env, &key);
    if reservation.available < 0
        || reservation
            .last_gulp
            .is_some_and(|last_gulp| last_gulp > env.ledger().timestamp())
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    reservation
}

fn set_pool_emission_reservation(env: &Env, pool: &Address, reservation: &PoolEmissionReservation) {
    if reservation.available < 0
        || reservation
            .last_gulp
            .is_some_and(|last_gulp| last_gulp > env.ledger().timestamp())
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let key = PersistentKey::PoolEmissionReservation(pool.clone());
    env.storage().persistent().set(&key, reservation);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn gulp_pool_ongoing_emissions(env: &Env, pool: &Address) -> i128 {
    pool.require_auth();
    require_registered_pool(env, pool);

    let mut reservation = get_pool_emission_reservation(env, pool);
    let now = env.ledger().timestamp();
    if reservation.last_gulp.is_some_and(|last_gulp| {
        last_gulp
            .checked_add(POOL_EMISSION_GULP_INTERVAL_SECONDS)
            .is_none_or(|next_gulp| next_gulp > now)
    }) {
        panic_with_error!(env, BackstopError::PoolEmissionGulpTooSoon);
    }

    let mut pool_state = get_pool_ongoing_emissions(env, pool);
    let amount = pool_state.accrued_pool;
    if amount == 0 {
        return 0;
    }
    pool_state.accrued_pool = 0;
    reservation.available = checked_add_i128(env, reservation.available, amount);
    reservation.last_gulp = Some(now);
    set_pool_ongoing_emissions(env, pool, &pool_state);
    set_pool_emission_reservation(env, pool, &reservation);
    amount
}

fn claim_reserved_pool_emissions(env: &Env, pool: &Address, recipient: &Address, amount: i128) {
    require_positive(env, amount);
    pool.require_auth();
    require_registered_pool(env, pool);

    let mut reservation = get_pool_emission_reservation(env, pool);
    reservation.available = checked_nonnegative_sub_i128(env, reservation.available, amount);
    set_pool_emission_reservation(env, pool, &reservation);

    let mut ongoing = get_ongoing_emission_state(env);
    ongoing.total_claimed = checked_add_i128(env, ongoing.total_claimed, amount);
    set_ongoing_emission_state(env, &ongoing);

    let blnd: Address = env.storage().instance().get(&DataKey::BlndToken).unwrap();
    TokenClient::new(env, &blnd).transfer(&env.current_contract_address(), recipient, &amount);
}

fn refresh_pool_ongoing_assets(env: &Env, pool: &Address) {
    let mut state = get_pool_ongoing_emissions(env, pool);
    let blnd_usdc = get_pool_tier_state(env, BackstopTier::BlndUsdc, pool);
    state.active_blnd_usdc_shares =
        checked_nonnegative_sub_i128(env, blnd_usdc.shares, blnd_usdc.queued_shares);
    state.active_blnd_usdc = assets_from_shares(env, state.active_blnd_usdc_shares, &blnd_usdc);
    let blnd_xlm = get_pool_tier_state(env, BackstopTier::BlndXlm, pool);
    state.active_blnd_xlm_shares =
        checked_nonnegative_sub_i128(env, blnd_xlm.shares, blnd_xlm.queued_shares);
    state.active_blnd_xlm = assets_from_shares(env, state.active_blnd_xlm_shares, &blnd_xlm);
    set_pool_ongoing_emissions(env, pool, &state);
}

fn refresh_pool_ongoing_assets_for_tier(env: &Env, tier: BackstopTier, pool: &Address) {
    if tier != BackstopTier::Usdc {
        refresh_pool_ongoing_assets(env, pool);
    }
}

fn pool_ongoing_index(env: &Env, state: &PoolOngoingEmissions, tier: BackstopTier) -> i128 {
    match tier {
        BackstopTier::BlndUsdc => state.blnd_usdc_index,
        BackstopTier::BlndXlm => state.blnd_xlm_index,
        BackstopTier::Usdc => panic_with_error!(env, BackstopError::InvalidTier),
    }
}

fn get_user_ongoing_emissions(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
) -> UserOngoingEmissions {
    require_emission_tier(env, tier);
    let key = PersistentKey::UserOngoingEmissions(tier, user.clone(), pool.clone());
    let state = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(UserOngoingEmissions {
            accrued: 0,
            carry: 0,
            index: 0,
        });
    extend_persistent_ttl_if_present(env, &key);
    if state.accrued < 0
        || state.carry < 0
        || state.carry >= EMISSION_INDEX_SCALE
        || state.index < 0
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    state
}

fn set_user_ongoing_emissions(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
    state: &UserOngoingEmissions,
) {
    require_emission_tier(env, tier);
    if state.accrued < 0
        || state.carry < 0
        || state.carry >= EMISSION_INDEX_SCALE
        || state.index < 0
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let key = PersistentKey::UserOngoingEmissions(tier, user.clone(), pool.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn user_active_emission_shares(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
) -> i128 {
    match tier {
        BackstopTier::BlndUsdc => {
            let shares = get_position(env, user, pool).amount;
            let queued = queued_shares(env, &get_blnd_usdc_q4w(env, user, pool));
            checked_nonnegative_sub_i128(env, shares, queued)
        }
        BackstopTier::BlndXlm => {
            let shares = get_tier_shares(env, tier, user, pool);
            let queued = queued_shares(env, &get_tier_q4w(env, tier, user, pool));
            checked_nonnegative_sub_i128(env, shares, queued)
        }
        BackstopTier::Usdc => panic_with_error!(env, BackstopError::InvalidTier),
    }
}

fn accrue_user_ongoing_emissions(
    env: &Env,
    mut state: UserOngoingEmissions,
    active_shares: i128,
    current_index: i128,
) -> UserOngoingEmissions {
    if active_shares < 0 || current_index < state.index {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let index_delta = checked_sub_i128(env, current_index, state.index);
    let numerator = I256::from_i128(env, active_shares)
        .mul(&I256::from_i128(env, index_delta))
        .add(&I256::from_i128(env, state.carry));
    let scale = I256::from_i128(env, EMISSION_INDEX_SCALE);
    let accrued = numerator
        .div(&scale)
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    let carry = numerator
        .sub(&I256::from_i128(env, accrued).mul(&scale))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    state.accrued = checked_add_i128(env, state.accrued, accrued);
    state.carry = carry;
    state.index = current_index;
    state
}

fn preview_user_ongoing_emissions(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
) -> UserOngoingEmissions {
    let pool_state = get_pool_ongoing_emissions(env, pool);
    accrue_user_ongoing_emissions(
        env,
        get_user_ongoing_emissions(env, tier, user, pool),
        user_active_emission_shares(env, tier, user, pool),
        pool_ongoing_index(env, &pool_state, tier),
    )
}

fn checkpoint_user_ongoing_emissions(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
) -> UserOngoingEmissions {
    let state = preview_user_ongoing_emissions(env, tier, user, pool);
    set_user_ongoing_emissions(env, tier, user, pool, &state);
    state
}

fn checkpoint_user_ongoing_for_weight_change(
    env: &Env,
    tier: BackstopTier,
    user: &Address,
    pool: &Address,
) {
    if tier != BackstopTier::Usdc {
        checkpoint_user_ongoing_emissions(env, tier, user, pool);
    }
}

fn claim_user_ongoing_blnd(env: &Env, user: &Address, pool: &Address, recipient: &Address) -> i128 {
    user.require_auth();
    require_registered_pool(env, pool);

    let mut blnd_usdc = checkpoint_user_ongoing_emissions(env, BackstopTier::BlndUsdc, user, pool);
    let mut blnd_xlm = checkpoint_user_ongoing_emissions(env, BackstopTier::BlndXlm, user, pool);
    let amount = checked_add_i128(env, blnd_usdc.accrued, blnd_xlm.accrued);
    if amount <= 0 {
        panic_with_error!(env, BackstopError::NoOngoingEmissions);
    }
    blnd_usdc.accrued = 0;
    blnd_xlm.accrued = 0;
    set_user_ongoing_emissions(env, BackstopTier::BlndUsdc, user, pool, &blnd_usdc);
    set_user_ongoing_emissions(env, BackstopTier::BlndXlm, user, pool, &blnd_xlm);

    let mut pool_state = get_pool_ongoing_emissions(env, pool);
    pool_state.accrued_backstop =
        checked_nonnegative_sub_i128(env, pool_state.accrued_backstop, amount);
    set_pool_ongoing_emissions(env, pool, &pool_state);

    let mut ongoing = get_ongoing_emission_state(env);
    ongoing.backstop_claimed = checked_add_i128(env, ongoing.backstop_claimed, amount);
    ongoing.total_claimed = checked_add_i128(env, ongoing.total_claimed, amount);
    set_ongoing_emission_state(env, &ongoing);

    let blnd: Address = env.storage().instance().get(&DataKey::BlndToken).unwrap();
    TokenClient::new(env, &blnd).transfer(&env.current_contract_address(), recipient, &amount);
    amount
}

fn get_ongoing_emission_state(env: &Env) -> OngoingEmissionState {
    OngoingEmissionState {
        backstop_allocated: env
            .storage()
            .instance()
            .get(&DataKey::OngoingBackstopAllocated)
            .unwrap_or(0),
        backstop_carry: env
            .storage()
            .instance()
            .get(&DataKey::OngoingBackstopCarry)
            .unwrap_or(0),
        backstop_claimed: env
            .storage()
            .instance()
            .get(&DataKey::OngoingBackstopClaimed)
            .unwrap_or(0),
        last_distribution: env
            .storage()
            .instance()
            .get(&DataKey::LastOngoingDistribution),
        pool_allocated: env
            .storage()
            .instance()
            .get(&DataKey::OngoingPoolAllocated)
            .unwrap_or(0),
        pool_carry: env
            .storage()
            .instance()
            .get(&DataKey::OngoingPoolCarry)
            .unwrap_or(0),
        split_carry: env
            .storage()
            .instance()
            .get(&DataKey::OngoingSplitCarry)
            .unwrap_or(0),
        total_claimed: env
            .storage()
            .instance()
            .get(&DataKey::OngoingTotalClaimed)
            .unwrap_or(0),
        total_received: env
            .storage()
            .instance()
            .get(&DataKey::OngoingTotalReceived)
            .unwrap_or(0),
    }
}

fn set_ongoing_emission_state(env: &Env, state: &OngoingEmissionState) {
    if state.backstop_allocated < 0
        || state.backstop_carry < 0
        || state.backstop_claimed < 0
        || state.pool_allocated < 0
        || state.pool_carry < 0
        || state.split_carry < 0
        || state.total_claimed < 0
        || state.total_received < 0
        || state.backstop_claimed > state.backstop_allocated
        || state.backstop_claimed > state.total_claimed
        || state.total_claimed > state.total_received
    {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let pool_claimed =
        checked_nonnegative_sub_i128(env, state.total_claimed, state.backstop_claimed);
    if pool_claimed > state.pool_allocated {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let accounted = checked_add_i128(
        env,
        checked_add_i128(env, state.backstop_allocated, state.pool_allocated),
        checked_add_i128(
            env,
            state.split_carry,
            checked_add_i128(env, state.backstop_carry, state.pool_carry),
        ),
    );
    if accounted != state.total_received {
        panic_with_error!(env, BackstopError::InvalidOngoingBalance);
    }

    env.storage().instance().set(
        &DataKey::OngoingBackstopAllocated,
        &state.backstop_allocated,
    );
    env.storage()
        .instance()
        .set(&DataKey::OngoingBackstopCarry, &state.backstop_carry);
    env.storage()
        .instance()
        .set(&DataKey::OngoingBackstopClaimed, &state.backstop_claimed);
    env.storage()
        .instance()
        .set(&DataKey::OngoingPoolAllocated, &state.pool_allocated);
    env.storage()
        .instance()
        .set(&DataKey::OngoingPoolCarry, &state.pool_carry);
    env.storage()
        .instance()
        .set(&DataKey::OngoingSplitCarry, &state.split_carry);
    env.storage()
        .instance()
        .set(&DataKey::OngoingTotalClaimed, &state.total_claimed);
    env.storage()
        .instance()
        .set(&DataKey::OngoingTotalReceived, &state.total_received);
    if let Some(last_distribution) = state.last_distribution {
        env.storage()
            .instance()
            .set(&DataKey::LastOngoingDistribution, &last_distribution);
    }
}

fn remaining_backfill_or_zero(env: &Env) -> i128 {
    let funded: Option<i128> = env.storage().instance().get(&DataKey::BackfillFundedAmount);
    match funded {
        Some(funded) => {
            let claimed = env
                .storage()
                .instance()
                .get(&DataKey::TotalBackfillClaimed)
                .unwrap_or(0);
            checked_nonnegative_sub_i128(env, funded, claimed)
        }
        None => 0,
    }
}

fn comet_composition(env: &Env, tier: BackstopTier) -> (i128, i128) {
    let lp_token = match tier {
        BackstopTier::BlndUsdc | BackstopTier::BlndXlm => tier_token(env, tier),
        BackstopTier::Usdc => panic_with_error!(env, BackstopError::InvalidTier),
    };
    let comet = CometClient::new(env, &lp_token);
    let total_supply = comet.get_total_supply();
    let blnd: Address = env.storage().instance().get(&DataKey::BlndToken).unwrap();
    let blnd_reserve = comet.get_balance(&blnd);
    if total_supply <= 0 || blnd_reserve < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    (total_supply, blnd_reserve)
}

fn underlying_blnd_from_composition(
    env: &Env,
    lp_amount: i128,
    total_supply: i128,
    blnd_reserve: i128,
) -> i128 {
    if lp_amount < 0 || total_supply <= 0 || blnd_reserve < 0 || lp_amount > total_supply {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    proportional_floor(env, lp_amount, blnd_reserve, total_supply)
}

fn require_recent_ongoing_checkpoint_for_weight_change(
    env: &Env,
    tier: BackstopTier,
    pool: &Address,
) {
    if tier == BackstopTier::Usdc {
        return;
    }
    if !env.storage().instance().has(&DataKey::ActivatedAt) {
        require_no_unsynchronized_emitter_recipient(env);
        return;
    }
    if !get_reward_zone(env).contains(pool.clone()) {
        return;
    }
    let checkpoint = get_reward_zone_checkpoint(env)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::DistributionCheckpointRequired));
    let age = env
        .ledger()
        .timestamp()
        .checked_sub(checkpoint)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::DistributionCheckpointRequired));
    if age > WEIGHT_CHANGE_CHECKPOINT_MAX_AGE_SECONDS {
        panic_with_error!(env, BackstopError::DistributionCheckpointRequired);
    }
}

fn require_no_unsynchronized_emitter_recipient(env: &Env) {
    if env.storage().instance().has(&DataKey::ActivatedAt) {
        return;
    }
    let Some(verified_unlock) = env
        .storage()
        .instance()
        .get::<DataKey, u64>(&DataKey::VerifiedQueueUnlock)
    else {
        return;
    };
    if env.ledger().timestamp() < verified_unlock {
        return;
    }
    if EmitterClient::new(env, &get_emitter(env)).get_backstop() == env.current_contract_address() {
        panic_with_error!(env, BackstopError::MigrationNotActive);
    }
}

fn advance_ongoing_emission_index(
    env: &Env,
    allocation: i128,
    active_shares: i128,
    index: i128,
    carry: i128,
) -> (i128, i128) {
    if allocation < 0 || active_shares < 0 || index < 0 || carry < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    if active_shares == 0 {
        if allocation != 0 {
            panic_with_error!(env, BackstopError::InvalidEmissionValue);
        }
        return (index, carry);
    }

    let numerator = I256::from_i128(env, allocation)
        .mul(&I256::from_i128(env, EMISSION_INDEX_SCALE))
        .add(&I256::from_i128(env, carry));
    let denominator = I256::from_i128(env, active_shares);
    let index_increment = numerator
        .div(&denominator)
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    let next_carry = numerator
        .sub(&I256::from_i128(env, index_increment).mul(&denominator))
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    (checked_add_i128(env, index, index_increment), next_carry)
}

fn allocate_pool_backstop_emissions(
    env: &Env,
    state: &mut PoolOngoingEmissions,
    values: &BlndEmissionValues,
    allocation: i128,
) {
    if allocation < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let total_weight = eligible_blnd(env, values);
    let distribution = checked_add_i128(env, allocation, state.backstop_tier_carry);
    if total_weight <= 0 {
        if distribution != 0 {
            panic_with_error!(env, BackstopError::InvalidEmissionValue);
        }
        return;
    }

    let blnd_usdc = proportional_floor(env, distribution, values.blnd_usdc, total_weight);
    let blnd_xlm = proportional_floor(env, distribution, values.blnd_xlm, total_weight);
    state.backstop_tier_carry = checked_nonnegative_sub_i128(
        env,
        distribution,
        checked_add_i128(env, blnd_usdc, blnd_xlm),
    );
    (state.blnd_usdc_index, state.blnd_usdc_index_carry) = advance_ongoing_emission_index(
        env,
        blnd_usdc,
        state.active_blnd_usdc_shares,
        state.blnd_usdc_index,
        state.blnd_usdc_index_carry,
    );
    (state.blnd_xlm_index, state.blnd_xlm_index_carry) = advance_ongoing_emission_index(
        env,
        blnd_xlm,
        state.active_blnd_xlm_shares,
        state.blnd_xlm_index,
        state.blnd_xlm_index_carry,
    );
}

fn distribute_ongoing_blnd(env: &Env) -> OngoingDistribution {
    if !env.storage().instance().has(&DataKey::ActivatedAt) {
        panic_with_error!(env, BackstopError::MigrationNotActive);
    }
    let candidate = env.current_contract_address();
    let emitter = EmitterClient::new(env, &get_emitter(env));
    if emitter.get_backstop() != candidate {
        panic_with_error!(env, BackstopError::EmitterDidNotMigrate);
    }

    let reward_zone = get_reward_zone(env);
    if reward_zone.is_empty() {
        panic_with_error!(env, BackstopError::NoEligibleWeight);
    }
    let (blnd_usdc_supply, blnd_usdc_reserve) = comet_composition(env, BackstopTier::BlndUsdc);
    let (blnd_xlm_supply, blnd_xlm_reserve) = comet_composition(env, BackstopTier::BlndXlm);
    let mut weights = Vec::new(env);
    let mut total_eligible_blnd = 0_i128;
    let mut total_blnd_usdc = 0_i128;
    let mut total_blnd_xlm = 0_i128;
    for pool in reward_zone.iter() {
        let pool_state = get_pool_ongoing_emissions(env, &pool);
        total_blnd_usdc = checked_add_i128(env, total_blnd_usdc, pool_state.active_blnd_usdc);
        total_blnd_xlm = checked_add_i128(env, total_blnd_xlm, pool_state.active_blnd_xlm);
        let pool_blnd_usdc = underlying_blnd_from_composition(
            env,
            pool_state.active_blnd_usdc,
            blnd_usdc_supply,
            blnd_usdc_reserve,
        );
        let pool_blnd_xlm = underlying_blnd_from_composition(
            env,
            pool_state.active_blnd_xlm,
            blnd_xlm_supply,
            blnd_xlm_reserve,
        );
        let pool_weight = checked_add_i128(env, pool_blnd_usdc, pool_blnd_xlm);
        total_eligible_blnd = checked_add_i128(env, total_eligible_blnd, pool_weight);
        weights.push_back((
            pool,
            pool_weight,
            BlndEmissionValues {
                blnd_usdc: pool_blnd_usdc,
                blnd_xlm: pool_blnd_xlm,
            },
        ));
    }
    if total_blnd_usdc > blnd_usdc_supply
        || total_blnd_xlm > blnd_xlm_supply
        || total_eligible_blnd <= 0
    {
        panic_with_error!(env, BackstopError::NoEligibleWeight);
    }

    let mut state = get_ongoing_emission_state(env);
    let last_distribution = state
        .last_distribution
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::MigrationNotActive));
    let blnd: Address = env.storage().instance().get(&DataKey::BlndToken).unwrap();
    let token = TokenClient::new(env, &blnd);
    let binding_verified = env.storage().instance().has(&DataKey::BlndBindingVerified);
    let balance_before = if binding_verified {
        None
    } else {
        Some(token.balance(&candidate))
    };
    let emitted = emitter.distribute();
    if emitted < 0 {
        panic_with_error!(env, BackstopError::InvalidOngoingBalance);
    }
    let checkpoint = emitter.get_last_distro(&candidate);
    let elapsed = checkpoint
        .checked_sub(last_distribution)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::DistributionTooSoon));
    if elapsed < MIN_DISTRIBUTION_INTERVAL_SECONDS || checkpoint > env.ledger().timestamp() {
        panic_with_error!(env, BackstopError::DistributionTooSoon);
    }

    let outstanding_ongoing =
        checked_nonnegative_sub_i128(env, state.total_received, state.total_claimed);
    let protected_balance =
        checked_add_i128(env, remaining_backfill_or_zero(env), outstanding_ongoing);
    let balance = token.balance(&candidate);
    if let Some(balance_before) = balance_before {
        // The first positive ongoing mint is an independent binding path for a
        // migration with no eligible backfill weight. Compare only this call's
        // balance delta so a pre-funded wrong token cannot mask a mismatch.
        let binding_delta = checked_sub_i128(env, balance, balance_before);
        if emitted <= 0 || binding_delta != emitted {
            panic_with_error!(env, BackstopError::InvalidOngoingBalance);
        }
    }
    if balance < protected_balance {
        panic_with_error!(env, BackstopError::InvalidOngoingBalance);
    }
    let received = checked_sub_i128(env, balance, protected_balance);
    if received <= 0 || received < emitted {
        panic_with_error!(env, BackstopError::InvalidOngoingBalance);
    }

    let split = quote_ongoing_blnd_split_values(env, received, state.split_carry);
    let backstop_distribution = checked_add_i128(env, split.backstop, state.backstop_carry);
    let pool_distribution = checked_add_i128(env, split.pool, state.pool_carry);
    let mut backstop_allocated = 0_i128;
    let mut pool_allocated = 0_i128;
    for (pool, weight, values) in weights.iter() {
        let backstop_allocation =
            proportional_floor(env, backstop_distribution, weight, total_eligible_blnd);
        let pool_allocation =
            proportional_floor(env, pool_distribution, weight, total_eligible_blnd);
        let mut pool_state = get_pool_ongoing_emissions(env, &pool);
        allocate_pool_backstop_emissions(env, &mut pool_state, &values, backstop_allocation);
        pool_state.accrued_backstop =
            checked_add_i128(env, pool_state.accrued_backstop, backstop_allocation);
        pool_state.accrued_pool = checked_add_i128(env, pool_state.accrued_pool, pool_allocation);
        set_pool_ongoing_emissions(env, &pool, &pool_state);
        backstop_allocated = checked_add_i128(env, backstop_allocated, backstop_allocation);
        pool_allocated = checked_add_i128(env, pool_allocated, pool_allocation);
    }

    state.total_received = checked_add_i128(env, state.total_received, received);
    state.backstop_allocated = checked_add_i128(env, state.backstop_allocated, backstop_allocated);
    state.pool_allocated = checked_add_i128(env, state.pool_allocated, pool_allocated);
    state.split_carry = split.carry;
    state.backstop_carry =
        checked_nonnegative_sub_i128(env, backstop_distribution, backstop_allocated);
    state.pool_carry = checked_nonnegative_sub_i128(env, pool_distribution, pool_allocated);
    state.last_distribution = Some(checkpoint);
    set_ongoing_emission_state(env, &state);
    if !binding_verified {
        env.storage()
            .instance()
            .set(&DataKey::BlndBindingVerified, &true);
    }

    let checkpoint_key = PersistentKey::RewardZoneCheckpoint;
    env.storage().persistent().set(&checkpoint_key, &checkpoint);
    env.storage().persistent().extend_ttl(
        &checkpoint_key,
        POSITION_TTL_THRESHOLD,
        POSITION_TTL_BUMP,
    );

    OngoingDistribution {
        backstop_allocated,
        backstop_carry: state.backstop_carry,
        checkpoint,
        eligible_blnd: total_eligible_blnd,
        pool_allocated,
        pool_carry: state.pool_carry,
        received,
        split_carry: state.split_carry,
    }
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
    require_recent_ongoing_checkpoint_for_weight_change(env, BackstopTier::BlndUsdc, &pool);
    checkpoint_user_ongoing_for_weight_change(env, BackstopTier::BlndUsdc, &from, &pool);

    let expires_at = env
        .ledger()
        .timestamp()
        .checked_add(Q4W_LOCK_SECONDS)
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError));
    let entry = Q4WEntry { amount, expires_at };
    queue.push_back(entry.clone());
    set_blnd_usdc_q4w(env, &from, &pool, &queue);
    add_pool_blnd_usdc_queued_shares(env, &pool, amount);
    refresh_pool_ongoing_assets(env, &pool);
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
    require_recent_ongoing_checkpoint_for_weight_change(env, BackstopTier::BlndUsdc, &pool);
    checkpoint_user_ongoing_for_weight_change(env, BackstopTier::BlndUsdc, &from, &pool);
    set_blnd_usdc_q4w(env, &from, &pool, &queue);
    add_pool_blnd_usdc_queued_shares(env, &pool, -amount);
    refresh_pool_ongoing_assets(env, &pool);
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
    if interest_tier_locked(env, BackstopTier::BlndUsdc, &pool)
        || get_pool_bad_debt_commitment_count(env, &pool) != 0
        || !PoolLossClient::new(env, &pool).backstop_withdrawal_allowed(&backstop)
    {
        panic_with_error!(env, BackstopError::PoolWithdrawalBlocked);
    }
    let state = get_pool_tier_state(env, BackstopTier::BlndUsdc, &pool);
    let assets = assets_from_shares(env, amount, &state);
    require_recent_ongoing_checkpoint_for_weight_change(env, BackstopTier::BlndUsdc, &pool);
    checkpoint_user_ongoing_for_weight_change(env, BackstopTier::BlndUsdc, &from, &pool);
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
        let backfill_removed =
            proportional_floor(env, position.backfill_amount, amount, position.amount);
        let basis_removed = proportional_floor(env, position.time_basis, amount, position.amount);
        position.backfill_amount =
            checked_sub_i128(env, position.backfill_amount, backfill_removed);
        position.time_basis = checked_sub_i128(env, position.time_basis, basis_removed);
        add_to_instance_total(env, DataKey::TotalMigrationAmount, -backfill_removed);
        add_to_instance_total(env, DataKey::TotalMigrationTimeBasis, -basis_removed);
    }

    position.amount = checked_sub_i128(env, position.amount, amount);
    set_position(env, &from, &pool, &position);
    if amount > state.shares {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    set_pool_blnd_usdc_shares(env, &pool, checked_sub_i128(env, state.shares, amount));
    set_pool_tier_assets(
        env,
        BackstopTier::BlndUsdc,
        &pool,
        checked_sub_i128(env, state.assets, assets),
    );
    add_pool_blnd_usdc_queued_shares(env, &pool, -amount);
    refresh_pool_ongoing_assets(env, &pool);
    add_to_instance_total(env, DataKey::TotalBlndUsdcShares, -amount);
    add_to_instance_total(env, DataKey::TotalBlndUsdcQueuedShares, -amount);
    add_to_instance_total(
        env,
        DataKey::TotalTierAssets(BackstopTier::BlndUsdc),
        -assets,
    );

    if assets > 0 {
        TokenClient::new(env, &legacy_backstop_token(env)).transfer(
            &env.current_contract_address(),
            &to,
            &assets,
        );
    }
    BlndUsdcWithdrawn {
        amount: assets,
        depositor: from,
        pool,
        shares: amount,
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
            backfill_amount: 0,
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

fn pool_shares(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    match tier {
        BackstopTier::BlndUsdc => get_pool_blnd_usdc_shares(env, pool),
        _ => get_pool_tier_shares(env, tier, pool),
    }
}

fn pool_queued_shares(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    match tier {
        BackstopTier::BlndUsdc => get_pool_blnd_usdc_queued_shares(env, pool),
        _ => get_pool_tier_queued_shares(env, tier, pool),
    }
}

fn get_pool_tier_state(env: &Env, tier: BackstopTier, pool: &Address) -> PoolTierState {
    let state = PoolTierState {
        assets: get_pool_tier_assets(env, tier, pool),
        queued_shares: pool_queued_shares(env, tier, pool),
        shares: pool_shares(env, tier, pool),
    };
    if state.assets < 0
        || state.shares < 0
        || state.queued_shares < 0
        || state.queued_shares > state.shares
        || (state.shares == 0 && state.assets != 0)
    {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    state
}

fn get_pool_tier_assets(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    let key = PersistentKey::PoolTierAssets(tier, pool.clone());
    let shares_key = match tier {
        BackstopTier::BlndUsdc => PersistentKey::PoolBlndUsdcShares(pool.clone()),
        _ => PersistentKey::PoolTierShares(tier, pool.clone()),
    };
    let queued_key = match tier {
        BackstopTier::BlndUsdc => PersistentKey::PoolBlndUsdcQueuedShares(pool.clone()),
        _ => PersistentKey::PoolTierQueuedShares(tier, pool.clone()),
    };
    let assets = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &shares_key);
    extend_persistent_ttl_if_present(env, &queued_key);
    assets
}

fn set_pool_tier_assets(env: &Env, tier: BackstopTier, pool: &Address, assets: i128) {
    if assets < 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    let key = PersistentKey::PoolTierAssets(tier, pool.clone());
    env.storage().persistent().set(&key, &assets);
    env.storage()
        .persistent()
        .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_BUMP);
}

fn get_pool_tier_committed_assets(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    get_bad_debt_commitment(env, pool)
        .map(|commitment| {
            if commitment.quote.tier == tier {
                commitment.quote.lot_amount
            } else {
                0
            }
        })
        .unwrap_or(0)
}

fn get_pool_bad_debt_commitment_count(env: &Env, pool: &Address) -> u32 {
    u32::from(get_bad_debt_commitment(env, pool).is_some())
}

fn get_bad_debt_commitment(env: &Env, pool: &Address) -> Option<BadDebtCommitment> {
    env.storage()
        .temporary()
        .get(&TemporaryKey::BadDebtCommitment(pool.clone()))
}

fn set_bad_debt_commitment(env: &Env, pool: &Address, commitment: &BadDebtCommitment) {
    let key = TemporaryKey::BadDebtCommitment(pool.clone());
    env.storage().temporary().set(&key, commitment);
    env.storage()
        .temporary()
        .extend_ttl(&key, AUCTION_TTL_THRESHOLD, AUCTION_TTL_BUMP);
}

fn remove_bad_debt_commitment(env: &Env, pool: &Address) {
    env.storage()
        .temporary()
        .remove(&TemporaryKey::BadDebtCommitment(pool.clone()));
}

fn get_interest_commitment(env: &Env, pool: &Address) -> Option<InterestCommitment> {
    env.storage()
        .temporary()
        .get(&TemporaryKey::InterestCommitment(pool.clone()))
}

fn set_interest_commitment(env: &Env, pool: &Address, commitment: &InterestCommitment) {
    let key = TemporaryKey::InterestCommitment(pool.clone());
    env.storage().temporary().set(&key, commitment);
    env.storage()
        .temporary()
        .extend_ttl(&key, AUCTION_TTL_THRESHOLD, AUCTION_TTL_BUMP);
}

fn remove_interest_commitment(env: &Env, pool: &Address) {
    env.storage()
        .temporary()
        .remove(&TemporaryKey::InterestCommitment(pool.clone()));
}

fn interest_tier_locked(env: &Env, tier: BackstopTier, pool: &Address) -> bool {
    get_interest_commitment(env, pool).is_some_and(|commitment| commitment.quote.tier == tier)
}

fn shares_from_assets(env: &Env, assets: i128, state: &PoolTierState) -> i128 {
    if assets < 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    if assets == 0 {
        return 0;
    }
    if state.shares == 0 {
        if state.assets != 0 {
            panic_with_error!(env, BackstopError::ArithmeticError);
        }
        return assets;
    }
    if state.assets == 0 {
        return 0;
    }
    proportional_floor(env, assets, state.shares, state.assets)
}

fn assets_from_shares(env: &Env, shares: i128, state: &PoolTierState) -> i128 {
    if shares < 0 || shares > state.shares || (shares > 0 && state.shares == 0) {
        panic_with_error!(env, BackstopError::InsufficientBalance);
    }
    if shares == 0 {
        return 0;
    }
    if shares == state.shares {
        return state.assets;
    }
    proportional_floor(env, state.assets, shares, state.shares)
}

fn apply_pool_tier_loss(env: &Env, tier: BackstopTier, pool: &Address, assets: i128) {
    if assets < 0 {
        panic_with_error!(env, BackstopError::InvalidAmount);
    }
    if assets == 0 {
        return;
    }
    let state = get_pool_tier_state(env, tier, pool);
    if assets > state.assets {
        panic_with_error!(env, BackstopError::InsufficientBalance);
    }
    require_recent_ongoing_checkpoint_for_weight_change(env, tier, pool);
    set_pool_tier_assets(env, tier, pool, checked_sub_i128(env, state.assets, assets));
    refresh_pool_ongoing_assets_for_tier(env, tier, pool);
    add_to_instance_total(env, DataKey::TotalTierAssets(tier), -assets);
}

fn apply_pool_tier_gain(env: &Env, tier: BackstopTier, pool: &Address, assets: i128) {
    if assets <= 0 {
        panic_with_error!(env, BackstopError::InvalidAmount);
    }
    let state = get_pool_tier_state(env, tier, pool);
    if state.shares == 0 {
        panic_with_error!(env, BackstopError::InvalidInterestLot);
    }
    require_recent_ongoing_checkpoint_for_weight_change(env, tier, pool);
    set_pool_tier_assets(env, tier, pool, checked_add_i128(env, state.assets, assets));
    refresh_pool_ongoing_assets_for_tier(env, tier, pool);
    add_to_instance_total(env, DataKey::TotalTierAssets(tier), assets);
}

fn get_pool_blnd_usdc_shares(env: &Env, pool: &Address) -> i128 {
    let key = PersistentKey::PoolBlndUsdcShares(pool.clone());
    let queued_key = PersistentKey::PoolBlndUsdcQueuedShares(pool.clone());
    let assets_key = PersistentKey::PoolTierAssets(BackstopTier::BlndUsdc, pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &queued_key);
    extend_persistent_ttl_if_present(env, &assets_key);
    shares
}

fn get_pool_blnd_usdc_queued_shares(env: &Env, pool: &Address) -> i128 {
    let key = PersistentKey::PoolBlndUsdcQueuedShares(pool.clone());
    let shares_key = PersistentKey::PoolBlndUsdcShares(pool.clone());
    let assets_key = PersistentKey::PoolTierAssets(BackstopTier::BlndUsdc, pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &shares_key);
    extend_persistent_ttl_if_present(env, &assets_key);
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
    let assets_key = PersistentKey::PoolTierAssets(tier, pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &queued_key);
    extend_persistent_ttl_if_present(env, &assets_key);
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
    let assets_key = PersistentKey::PoolTierAssets(tier, pool.clone());
    let shares = env.storage().persistent().get(&key).unwrap_or(0);
    extend_persistent_ttl_if_present(env, &key);
    extend_persistent_ttl_if_present(env, &shares_key);
    extend_persistent_ttl_if_present(env, &assets_key);
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
        checked_mul_i128(env, position.backfill_amount, end as i128),
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

fn proportional_ceil(env: &Env, value: i128, numerator: i128, denominator: i128) -> i128 {
    if value < 0 || numerator < 0 || denominator <= 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    let denominator = I256::from_i128(env, denominator);
    I256::from_i128(env, value)
        .mul(&I256::from_i128(env, numerator))
        .add(&denominator)
        .sub(&I256::from_i32(env, 1))
        .div(&denominator)
        .to_i128()
        .unwrap_or_else(|| panic_with_error!(env, BackstopError::ArithmeticError))
}

fn build_bad_debt_lot_quote(
    env: &Env,
    pool: &Address,
    debt_value: i128,
) -> Option<BadDebtLotQuote> {
    require_positive(env, debt_value);
    let target_value = proportional_ceil(
        env,
        debt_value,
        BAD_DEBT_LOT_PREMIUM_NUMERATOR,
        BAD_DEBT_LOT_PREMIUM_DENOMINATOR,
    );
    let adapter_address = valuation_adapter(env);
    let adapter = ValuationAdapterClient::new(env, &adapter_address);
    if adapter.version() != VALUATION_ADAPTER_VERSION {
        panic_with_error!(env, BackstopError::InvalidValuationAdapter);
    }

    for tier in [
        BackstopTier::BlndUsdc,
        BackstopTier::BlndXlm,
        BackstopTier::Usdc,
    ] {
        let assets = available_pool_tier_assets(env, tier, pool);
        if assets == 0 {
            continue;
        }
        let valuation = if tier == BackstopTier::Usdc {
            AssetValuation {
                underlying_blnd: 0,
                usdc_value: assets,
                valid_until: u64::MAX,
            }
        } else {
            quote_lp_amount(env, &adapter, tier, assets)
        };
        if valuation.usdc_value < BAD_DEBT_TIER_MINIMUM_VALUE_USDC {
            continue;
        }
        let (lot_amount, committed_value, unfilled_target_value) =
            allocate_bad_debt_tier(env, assets, valuation.usdc_value, target_value);
        return Some(BadDebtLotQuote {
            committed_value,
            debt_value,
            tier,
            lot_amount,
            target_value,
            unfilled_target_value,
            valid_until: valuation.valid_until,
        });
    }

    None
}

fn build_interest_lot_quote(env: &Env, tier: BackstopTier, lot_value: i128) -> InterestLotQuote {
    require_positive(env, lot_value);
    let bid_token = tier_token(env, tier);
    if TokenClient::new(env, &bid_token).decimals() != 7 {
        panic_with_error!(env, BackstopError::InvalidInterestLot);
    }
    let (unit_value, valid_until) = if tier == BackstopTier::Usdc {
        (SCALAR_7, u64::MAX)
    } else {
        let adapter_address = valuation_adapter(env);
        let adapter = ValuationAdapterClient::new(env, &adapter_address);
        if adapter.version() != VALUATION_ADAPTER_VERSION {
            panic_with_error!(env, BackstopError::InvalidValuationAdapter);
        }
        let valuation = quote_lp_amount(env, &adapter, tier, SCALAR_7);
        if valuation.usdc_value <= 0 {
            panic_with_error!(env, BackstopError::InvalidValuation);
        }
        (valuation.usdc_value, valuation.valid_until)
    };
    InterestLotQuote {
        bid_token,
        bid_amount: calculate_interest_bid_amount(env, lot_value, unit_value),
        lot_value,
        tier,
        target_value: proportional_ceil(
            env,
            lot_value,
            INTEREST_LOT_PREMIUM_NUMERATOR,
            INTEREST_LOT_PREMIUM_DENOMINATOR,
        ),
        valid_until,
    }
}

fn calculate_interest_bid_amount(env: &Env, lot_value: i128, unit_value: i128) -> i128 {
    if lot_value <= 0 || unit_value <= 0 {
        panic_with_error!(env, BackstopError::InvalidInterestLot);
    }
    let target_value = proportional_ceil(
        env,
        lot_value,
        INTEREST_LOT_PREMIUM_NUMERATOR,
        INTEREST_LOT_PREMIUM_DENOMINATOR,
    );
    proportional_ceil(env, target_value, SCALAR_7, unit_value)
}

fn allocate_bad_debt_tier(
    env: &Env,
    available_assets: i128,
    available_value: i128,
    remaining_target: i128,
) -> (i128, i128, i128) {
    if available_assets <= 0 || available_value <= 0 || remaining_target <= 0 {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    if available_value <= remaining_target {
        return (
            available_assets,
            available_value,
            checked_sub_i128(env, remaining_target, available_value),
        );
    }
    let selected_assets =
        proportional_ceil(env, remaining_target, available_assets, available_value);
    if selected_assets <= 0 || selected_assets > available_assets {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    let selected_value =
        proportional_floor(env, available_value, selected_assets, available_assets);
    if selected_value < remaining_target {
        panic_with_error!(env, BackstopError::ArithmeticError);
    }
    (selected_assets, selected_value, 0)
}

fn available_pool_tier_assets(env: &Env, tier: BackstopTier, pool: &Address) -> i128 {
    checked_nonnegative_sub_i128(
        env,
        get_pool_tier_assets(env, tier, pool),
        get_pool_tier_committed_assets(env, tier, pool),
    )
}

fn build_pool_valuation(env: &Env, pool: &Address) -> PoolValuation {
    // Valuation is invoked by the pool itself during status transitions.
    // Factory registration is sufficient here and avoids a circular
    // pool -> backstop -> pool compatibility probe. Custody entry points keep
    // the stronger loss-interface check in `require_compatible_pool`.
    require_registered_pool(env, pool);
    let adapter_address = valuation_adapter(env);
    let adapter = ValuationAdapterClient::new(env, &adapter_address);
    if adapter.version() != VALUATION_ADAPTER_VERSION {
        panic_with_error!(env, BackstopError::InvalidValuationAdapter);
    }

    let (blnd_usdc_active, blnd_usdc_queued) =
        pool_tier_asset_partition(env, BackstopTier::BlndUsdc, pool);
    let (blnd_xlm_active, blnd_xlm_queued) =
        pool_tier_asset_partition(env, BackstopTier::BlndXlm, pool);
    let (usdc_active, usdc_queued) = pool_tier_asset_partition(env, BackstopTier::Usdc, pool);

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

fn build_take_rate_values(env: &Env, pool: &Address) -> TakeRateValues {
    require_registered_pool(env, pool);
    let adapter_address = valuation_adapter(env);
    let adapter = ValuationAdapterClient::new(env, &adapter_address);
    if adapter.version() != VALUATION_ADAPTER_VERSION {
        panic_with_error!(env, BackstopError::InvalidValuationAdapter);
    }

    let blnd_usdc = quote_lp_amount(
        env,
        &adapter,
        BackstopTier::BlndUsdc,
        get_pool_tier_assets(env, BackstopTier::BlndUsdc, pool),
    );
    let blnd_xlm = quote_lp_amount(
        env,
        &adapter,
        BackstopTier::BlndXlm,
        get_pool_tier_assets(env, BackstopTier::BlndXlm, pool),
    );
    TakeRateValues {
        blnd_usdc: blnd_usdc.usdc_value,
        blnd_xlm: blnd_xlm.usdc_value,
        usdc: get_pool_tier_assets(env, BackstopTier::Usdc, pool),
    }
}

fn pool_tier_asset_partition(env: &Env, tier: BackstopTier, pool: &Address) -> (i128, i128) {
    let mut state = get_pool_tier_state(env, tier, pool);
    state.assets = available_pool_tier_assets(env, tier, pool);
    let active_shares = checked_nonnegative_sub_i128(env, state.shares, state.queued_shares);
    let active_assets = assets_from_shares(env, active_shares, &state);
    let queued_assets = assets_from_shares(env, state.queued_shares, &state);
    (active_assets, queued_assets)
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

fn spot_underlying_blnd(env: &Env, tier: BackstopTier, lp_amount: i128) -> i128 {
    let (total_supply, blnd_reserve) = comet_composition(env, tier);
    underlying_blnd_from_composition(env, lp_amount, total_supply, blnd_reserve)
}

fn quote_ongoing_blnd_split_values(
    env: &Env,
    distribution: i128,
    prior_carry: i128,
) -> OngoingBlndSplit {
    if distribution < 0 || prior_carry < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let total = checked_add_i128(env, distribution, prior_carry);
    let backstop = proportional_floor(
        env,
        total,
        BACKSTOP_EMISSION_NUMERATOR,
        EMISSION_SPLIT_DENOMINATOR,
    );
    let pool = proportional_floor(
        env,
        total,
        POOL_EMISSION_NUMERATOR,
        EMISSION_SPLIT_DENOMINATOR,
    );
    let allocated = checked_add_i128(env, backstop, pool);
    OngoingBlndSplit {
        backstop,
        carry: checked_nonnegative_sub_i128(env, total, allocated),
        pool,
        total,
    }
}

fn quote_pool_blnd_emission_values(
    env: &Env,
    distribution: i128,
    values: &BlndEmissionValues,
    total_reward_zone_blnd: i128,
    reward_zone_member: bool,
) -> BlndEmissionQuote {
    if distribution < 0 || total_reward_zone_blnd < 0 {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let pool_blnd = eligible_blnd(env, values);
    if !reward_zone_member {
        return BlndEmissionQuote {
            allocation: 0,
            eligible_blnd: pool_blnd,
        };
    }
    if pool_blnd > total_reward_zone_blnd || (pool_blnd > 0 && total_reward_zone_blnd == 0) {
        panic_with_error!(env, BackstopError::InvalidEmissionValue);
    }
    let allocation = if pool_blnd == 0 {
        0
    } else {
        proportional_floor(env, distribution, pool_blnd, total_reward_zone_blnd)
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
        testutils::{
            storage::{Instance as _, Temporary as _},
            Address as _, Ledger, MockAuth, MockAuthInvoke,
        },
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

        pub fn commit_bad_debt_lot(
            env: Env,
            backstop: Address,
            auction_id: BytesN<32>,
            debt_value: i128,
        ) -> BadDebtLotQuote {
            BackstopContractClient::new(&env, &backstop).commit_bad_debt_lot(
                &env.current_contract_address(),
                &auction_id,
                &debt_value,
            )
        }

        pub fn release_bad_debt_lot(env: Env, backstop: Address, auction_id: BytesN<32>) {
            BackstopContractClient::new(&env, &backstop)
                .release_bad_debt_lot(&env.current_contract_address(), &auction_id);
        }

        pub fn settle_bad_debt_lot(
            env: Env,
            backstop: Address,
            auction_id: BytesN<32>,
            base_lot_amount: i128,
            lot_amount: i128,
            to: Address,
        ) -> Option<BadDebtLotQuote> {
            BackstopContractClient::new(&env, &backstop).settle_bad_debt_lot(
                &env.current_contract_address(),
                &auction_id,
                &base_lot_amount,
                &lot_amount,
                &to,
            )
        }

        pub fn commit_interest_lot(
            env: Env,
            backstop: Address,
            auction_id: BytesN<32>,
            tier: BackstopTier,
            lot_value: i128,
        ) -> InterestLotQuote {
            BackstopContractClient::new(&env, &backstop).commit_interest_lot(
                &env.current_contract_address(),
                &auction_id,
                &tier,
                &lot_value,
            )
        }

        pub fn release_interest_lot(env: Env, backstop: Address, auction_id: BytesN<32>) {
            BackstopContractClient::new(&env, &backstop)
                .release_interest_lot(&env.current_contract_address(), &auction_id);
        }

        pub fn settle_interest_lot(
            env: Env,
            backstop: Address,
            auction_id: BytesN<32>,
            base_bid_amount: i128,
            bid_amount: i128,
            from: Address,
        ) -> Option<InterestLotQuote> {
            BackstopContractClient::new(&env, &backstop).settle_interest_lot(
                &env.current_contract_address(),
                &auction_id,
                &base_bid_amount,
                &bid_amount,
                &from,
            )
        }
    }

    #[contract]
    struct MockValuationAdapter;

    #[derive(Clone)]
    #[contracttype]
    enum MockValuationAdapterKey {
        Binding,
        Invalid,
        ValidUntil,
        Version,
    }

    #[contractimpl]
    impl MockValuationAdapter {
        pub fn __constructor(env: Env, binding: AdapterBinding) {
            env.storage()
                .instance()
                .set(&MockValuationAdapterKey::Binding, &binding);
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

        pub fn binding(env: Env) -> AdapterBinding {
            env.storage()
                .instance()
                .get(&MockValuationAdapterKey::Binding)
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

    fn register_valuation_adapter(
        env: &Env,
        blnd: &Address,
        blnd_usdc: &Address,
        blnd_xlm: &Address,
        usdc: &Address,
    ) -> Address {
        env.register(
            MockValuationAdapter,
            (&AdapterBinding {
                blnd: blnd.clone(),
                blnd_usdc: blnd_usdc.clone(),
                blnd_xlm: blnd_xlm.clone(),
                usdc: usdc.clone(),
            },),
        )
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
            let balance = Self::balance(env.clone(), to.clone());
            env.storage().instance().set(
                &MockCometKey::Balance(to),
                &balance.checked_add(amount).unwrap(),
            );
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            let from_balance = Self::balance(env.clone(), from.clone());
            if amount < 0 || amount > from_balance {
                panic!("invalid transfer");
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
    struct MockEmitter;

    #[derive(Clone)]
    #[contracttype]
    enum MockEmitterKey {
        Backstop,
        BlndToken,
        Candidate,
        LastDistro(Address),
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
            env.storage().instance().set(
                &MockEmitterKey::LastDistro(backstop),
                &env.ledger().timestamp(),
            );
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

        pub fn get_last_distro(env: Env, backstop_id: Address) -> u64 {
            env.storage()
                .instance()
                .get(&MockEmitterKey::LastDistro(backstop_id))
                .unwrap_or(env.ledger().timestamp())
        }

        pub fn distribute(env: Env) -> i128 {
            let backstop: Address = env
                .storage()
                .instance()
                .get(&MockEmitterKey::Backstop)
                .unwrap();
            let last_distribution = Self::get_last_distro(env.clone(), backstop.clone());
            let elapsed = env
                .ledger()
                .timestamp()
                .checked_sub(last_distribution)
                .unwrap();
            let amount = (elapsed as i128).checked_mul(SCALAR_7).unwrap();
            env.storage().instance().set(
                &MockEmitterKey::LastDistro(backstop.clone()),
                &env.ledger().timestamp(),
            );
            if amount > 0 {
                let blnd: Address = env
                    .storage()
                    .instance()
                    .get(&MockEmitterKey::BlndToken)
                    .unwrap();
                StellarAssetClient::new(&env, &blnd).mint(&backstop, &amount);
            }
            amount
        }

        pub fn set_blnd_token(env: Env, blnd: Address) {
            env.storage()
                .instance()
                .set(&MockEmitterKey::BlndToken, &blnd);
        }

        pub fn set_backstop(env: Env, backstop: Address) {
            env.storage()
                .instance()
                .set(&MockEmitterKey::Backstop, &backstop);
            env.storage().instance().set(
                &MockEmitterKey::LastDistro(backstop),
                &env.ledger().timestamp(),
            );
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
            env.storage().instance().set(
                &MockEmitterKey::LastDistro(candidate),
                &env.ledger().timestamp(),
            );
        }
    }

    struct RewardZoneFixture {
        env: Env,
        candidate: Address,
        pool_factory: Address,
    }

    impl RewardZoneFixture {
        fn create() -> Self {
            let env = Env::default();
            env.ledger().set_timestamp(10_000);
            let incumbent = Address::generate(&env);
            let blnd = Address::generate(&env);
            let blnd_usdc = env.register(
                MockComet,
                (&blnd, &(1_000_000 * SCALAR_7), &(4_000_000 * SCALAR_7)),
            );
            let blnd_xlm = env.register(
                MockComet,
                (&blnd, &(1_000_000 * SCALAR_7), &(2_000_000 * SCALAR_7)),
            );
            let usdc = Address::generate(&env);
            let candidate = Address::generate(&env);
            let pool_factory = env.register(MockPoolFactory, (&candidate,));
            let emitter = env.register(MockEmitter, (&incumbent, &candidate, &blnd_xlm));
            let valuation_adapter =
                register_valuation_adapter(&env, &blnd, &blnd_usdc, &blnd_xlm, &usdc);
            env.register_at(
                &candidate,
                BackstopContract,
                (
                    &emitter,
                    &blnd,
                    &pool_factory,
                    &blnd_usdc,
                    &blnd_xlm,
                    &usdc,
                    &valuation_adapter,
                ),
            );
            Self {
                env,
                candidate,
                pool_factory,
            }
        }

        fn client(&self) -> BackstopContractClient<'_> {
            BackstopContractClient::new(&self.env, &self.candidate)
        }

        fn pool(&self) -> Address {
            let pool = Address::generate(&self.env);
            MockPoolFactoryClient::new(&self.env, &self.pool_factory).set_pool(&pool);
            pool
        }

        fn set_tier_state(
            &self,
            pool: &Address,
            tier: BackstopTier,
            assets: i128,
            shares: i128,
            queued_shares: i128,
        ) {
            self.env.as_contract(&self.candidate, || {
                match tier {
                    BackstopTier::BlndUsdc => {
                        set_pool_blnd_usdc_shares(&self.env, pool, shares);
                        if queued_shares != 0 {
                            add_pool_blnd_usdc_queued_shares(&self.env, pool, queued_shares);
                        }
                    }
                    _ => {
                        set_pool_tier_shares(&self.env, tier, pool, shares);
                        if queued_shares != 0 {
                            add_pool_tier_queued_shares(&self.env, tier, pool, queued_shares);
                        }
                    }
                }
                set_pool_tier_assets(&self.env, tier, pool, assets);
            });
        }

        fn add_queued_shares(&self, pool: &Address, tier: BackstopTier, delta: i128) {
            self.env.as_contract(&self.candidate, || match tier {
                BackstopTier::BlndUsdc => {
                    add_pool_blnd_usdc_queued_shares(&self.env, pool, delta);
                }
                _ => add_pool_tier_queued_shares(&self.env, tier, pool, delta),
            });
        }

        fn set_checkpoint(&self, timestamp: u64) {
            self.env.as_contract(&self.candidate, || {
                let key = PersistentKey::RewardZoneCheckpoint;
                self.env.storage().persistent().set(&key, &timestamp);
                self.env.storage().persistent().extend_ttl(
                    &key,
                    POSITION_TTL_THRESHOLD,
                    POSITION_TTL_BUMP,
                );
            });
        }

        fn set_reward_zone(&self, reward_zone: &Vec<Address>) {
            self.env.as_contract(&self.candidate, || {
                set_reward_zone(&self.env, reward_zone);
            });
        }

        fn commit_blnd_usdc(&self, pool: &Address, lot_amount: i128) {
            self.env.as_contract(&self.candidate, || {
                let commitment = BadDebtCommitment {
                    auction_id: BytesN::from_array(&self.env, &[29; 32]),
                    quote: BadDebtLotQuote {
                        committed_value: lot_amount,
                        debt_value: lot_amount,
                        tier: BackstopTier::BlndUsdc,
                        lot_amount,
                        unfilled_target_value: 0,
                        target_value: lot_amount,
                        valid_until: u64::MAX,
                    },
                };
                self.env
                    .storage()
                    .temporary()
                    .set(&TemporaryKey::BadDebtCommitment(pool.clone()), &commitment);
            });
        }
    }

    struct OngoingFixture {
        env: Env,
        candidate: Address,
        emitter: Address,
        blnd: Address,
        blnd_usdc: Address,
        blnd_xlm: Address,
        emitted_blnd: Address,
        pool_factory: Address,
    }

    impl OngoingFixture {
        fn create() -> Self {
            Self::create_with_blnd_binding(true)
        }

        fn create_with_blnd_binding(bind_emitter_blnd: bool) -> Self {
            Self::create_configured(bind_emitter_blnd, true)
        }

        fn create_without_auth_mocking() -> Self {
            Self::create_configured(true, false)
        }

        fn create_configured(bind_emitter_blnd: bool, mock_all_auths: bool) -> Self {
            let env = Env::default();
            if mock_all_auths {
                env.mock_all_auths();
            }
            env.ledger().set_timestamp(1_000);
            let incumbent = Address::generate(&env);
            let candidate = Address::generate(&env);
            let next_backstop_token = Address::generate(&env);
            let emitter = env.register(MockEmitter, (&incumbent, &candidate, &next_backstop_token));
            let emitted_blnd = env
                .register_stellar_asset_contract_v2(emitter.clone())
                .address();
            MockEmitterClient::new(&env, &emitter).set_blnd_token(&emitted_blnd);
            let candidate_blnd = if bind_emitter_blnd {
                emitted_blnd.clone()
            } else {
                env.register_stellar_asset_contract_v2(Address::generate(&env))
                    .address()
            };
            let blnd_usdc = env.register(
                MockComet,
                (
                    &candidate_blnd,
                    &(1_000_000 * SCALAR_7),
                    &(4_000_000 * SCALAR_7),
                ),
            );
            let blnd_xlm = env.register(
                MockComet,
                (
                    &candidate_blnd,
                    &(1_000_000 * SCALAR_7),
                    &(2_000_000 * SCALAR_7),
                ),
            );
            let usdc = Address::generate(&env);
            let pool_factory = env.register(MockPoolFactory, (&candidate,));
            let valuation_adapter =
                register_valuation_adapter(&env, &candidate_blnd, &blnd_usdc, &blnd_xlm, &usdc);
            env.register_at(
                &candidate,
                BackstopContract,
                (
                    &emitter,
                    &candidate_blnd,
                    &pool_factory,
                    &blnd_usdc,
                    &blnd_xlm,
                    &usdc,
                    &valuation_adapter,
                ),
            );
            MockEmitterClient::new(&env, &emitter).set_backstop(&candidate);
            env.as_contract(&candidate, || {
                env.storage()
                    .instance()
                    .set(&DataKey::ActivatedAt, &env.ledger().timestamp());
                env.storage()
                    .instance()
                    .set(&DataKey::BackfillEnd, &env.ledger().timestamp());
                set_ongoing_emission_state(
                    &env,
                    &OngoingEmissionState {
                        backstop_allocated: 0,
                        backstop_carry: 0,
                        backstop_claimed: 0,
                        last_distribution: Some(env.ledger().timestamp()),
                        pool_allocated: 0,
                        pool_carry: 0,
                        split_carry: 0,
                        total_claimed: 0,
                        total_received: 0,
                    },
                );
            });
            Self {
                env,
                candidate,
                emitter,
                blnd: candidate_blnd,
                blnd_usdc,
                blnd_xlm,
                emitted_blnd,
                pool_factory,
            }
        }

        fn client(&self) -> BackstopContractClient<'_> {
            BackstopContractClient::new(&self.env, &self.candidate)
        }

        fn pool(&self, blnd_usdc: i128, blnd_xlm: i128) -> Address {
            let pool = self.env.register(MockLossAwarePool, (&self.candidate,));
            MockPoolFactoryClient::new(&self.env, &self.pool_factory).set_pool(&pool);
            self.env.as_contract(&self.candidate, || {
                if blnd_usdc > 0 {
                    set_pool_blnd_usdc_shares(&self.env, &pool, blnd_usdc);
                    set_pool_tier_assets(&self.env, BackstopTier::BlndUsdc, &pool, blnd_usdc);
                }
                if blnd_xlm > 0 {
                    set_pool_tier_shares(&self.env, BackstopTier::BlndXlm, &pool, blnd_xlm);
                    set_pool_tier_assets(&self.env, BackstopTier::BlndXlm, &pool, blnd_xlm);
                }
                refresh_pool_ongoing_assets(&self.env, &pool);
            });
            pool
        }

        fn set_reward_zone(&self, pools: &Vec<Address>) {
            self.env.as_contract(&self.candidate, || {
                set_reward_zone(&self.env, pools);
            });
        }

        fn fund_backfill_reserve(&self, amount: i128) {
            StellarAssetClient::new(&self.env, &self.blnd).mint(&self.candidate, &amount);
            self.env.as_contract(&self.candidate, || {
                self.env
                    .storage()
                    .instance()
                    .set(&DataKey::BackfillFundedAmount, &amount);
                self.env
                    .storage()
                    .instance()
                    .set(&DataKey::TotalBackfillClaimed, &0_i128);
                self.env
                    .storage()
                    .instance()
                    .set(&DataKey::BlndBindingVerified, &true);
            });
        }
    }

    #[test]
    fn ongoing_distribution_reconciles_actual_blnd_and_excludes_backfill() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let first = fixture.pool(100 * SCALAR_7, 0);
        let second = fixture.pool(0, 100 * SCALAR_7);
        fixture.set_reward_zone(&soroban_sdk::vec![
            &fixture.env,
            first.clone(),
            second.clone()
        ]);
        assert!(!candidate.blnd_binding_verified());
        fixture.fund_backfill_reserve(5 * SCALAR_7);
        assert!(candidate.blnd_binding_verified());

        fixture.env.ledger().set_timestamp(1_010);
        assert_eq!(
            MockEmitterClient::new(&fixture.env, &fixture.emitter).distribute(),
            10 * SCALAR_7
        );
        let distribution = candidate.distribute();
        assert_eq!(
            distribution,
            OngoingDistribution {
                backstop_allocated: 7 * SCALAR_7 - 1,
                backstop_carry: 1,
                checkpoint: 1_010,
                eligible_blnd: 600 * SCALAR_7,
                pool_allocated: 3 * SCALAR_7,
                pool_carry: 0,
                received: 10 * SCALAR_7,
                split_carry: 0,
            }
        );
        assert_eq!(
            candidate.pool_ongoing_emissions(&first),
            PoolOngoingEmissions {
                accrued_backstop: 46_666_666,
                accrued_pool: 2 * SCALAR_7,
                active_blnd_usdc: 100 * SCALAR_7,
                active_blnd_usdc_shares: 100 * SCALAR_7,
                active_blnd_xlm: 0,
                active_blnd_xlm_shares: 0,
                backstop_tier_carry: 0,
                blnd_usdc_index: 4_666_666_600_000,
                blnd_usdc_index_carry: 0,
                blnd_xlm_index: 0,
                blnd_xlm_index_carry: 0,
            }
        );
        assert_eq!(
            candidate.pool_ongoing_emissions(&second),
            PoolOngoingEmissions {
                accrued_backstop: 23_333_333,
                accrued_pool: SCALAR_7,
                active_blnd_usdc: 0,
                active_blnd_usdc_shares: 0,
                active_blnd_xlm: 100 * SCALAR_7,
                active_blnd_xlm_shares: 100 * SCALAR_7,
                backstop_tier_carry: 0,
                blnd_usdc_index: 0,
                blnd_usdc_index_carry: 0,
                blnd_xlm_index: 2_333_333_300_000,
                blnd_xlm_index_carry: 0,
            }
        );
        assert_eq!(
            candidate.ongoing_emission_state(),
            OngoingEmissionState {
                backstop_allocated: 7 * SCALAR_7 - 1,
                backstop_carry: 1,
                backstop_claimed: 0,
                last_distribution: Some(1_010),
                pool_allocated: 3 * SCALAR_7,
                pool_carry: 0,
                split_carry: 0,
                total_claimed: 0,
                total_received: 10 * SCALAR_7,
            }
        );
        assert_eq!(
            candidate.reward_zone_checkpoint(),
            Some(RewardZoneCheckpoint { timestamp: 1_010 })
        );
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&fixture.candidate),
            15 * SCALAR_7
        );

        let state_before = candidate.ongoing_emission_state();
        assert!(candidate.try_distribute().is_err());
        assert_eq!(candidate.ongoing_emission_state(), state_before);

        fixture.env.ledger().set_timestamp(1_011);
        assert!(candidate.try_distribute().is_err());
        StellarAssetClient::new(&fixture.env, &fixture.blnd).mint(&fixture.candidate, &1);
        fixture.env.ledger().set_timestamp(1_015);
        let second_distribution = candidate.distribute();
        assert_eq!(second_distribution.received, 5 * SCALAR_7 + 1);
        assert_eq!(second_distribution.backstop_carry, 0);
        assert_eq!(
            candidate.ongoing_emission_state(),
            OngoingEmissionState {
                backstop_allocated: 105_000_000,
                backstop_carry: 0,
                backstop_claimed: 0,
                last_distribution: Some(1_015),
                pool_allocated: 45_000_000,
                pool_carry: 0,
                split_carry: 1,
                total_claimed: 0,
                total_received: 15 * SCALAR_7 + 1,
            }
        );
    }

    #[test]
    fn ongoing_users_claim_only_the_two_backstop_tier_allocations() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let pool = fixture.pool(0, 0);
        let blnd_usdc_user = Address::generate(&fixture.env);
        let blnd_xlm_user = Address::generate(&fixture.env);
        let blnd_usdc_recipient = Address::generate(&fixture.env);
        let blnd_xlm_recipient = Address::generate(&fixture.env);
        let deposit = 100 * SCALAR_7;
        MockCometClient::new(&fixture.env, &fixture.blnd_usdc).mint(&blnd_usdc_user, &deposit);
        MockCometClient::new(&fixture.env, &fixture.blnd_xlm).mint(&blnd_xlm_user, &deposit);
        candidate.deposit_blnd_usdc(&blnd_usdc_user, &pool, &deposit);
        candidate.deposit_blnd_xlm(&blnd_xlm_user, &pool, &deposit);
        fixture.set_reward_zone(&soroban_sdk::vec![&fixture.env, pool.clone()]);

        fixture.env.ledger().set_timestamp(1_010);
        assert_eq!(candidate.distribute().received, 10 * SCALAR_7);
        let blnd_usdc_pending =
            candidate.user_ongoing_emissions(&blnd_usdc_user, &pool, &BackstopTier::BlndUsdc);
        let blnd_xlm_pending =
            candidate.user_ongoing_emissions(&blnd_xlm_user, &pool, &BackstopTier::BlndXlm);
        assert_eq!(blnd_usdc_pending.accrued, 46_666_666);
        assert_eq!(blnd_xlm_pending.accrued, 23_333_333);
        assert!(candidate
            .try_user_ongoing_emissions(&blnd_usdc_user, &pool, &BackstopTier::Usdc)
            .is_err());

        // Later spot-composition changes cannot rewrite an allocation already
        // frozen into the two tier indexes.
        MockCometClient::new(&fixture.env, &fixture.blnd_usdc)
            .set_composition(&(1_000_000 * SCALAR_7), &(1_000_000 * SCALAR_7));
        MockCometClient::new(&fixture.env, &fixture.blnd_xlm)
            .set_composition(&(1_000_000 * SCALAR_7), &(10_000_000 * SCALAR_7));
        assert_eq!(
            candidate
                .user_ongoing_emissions(&blnd_usdc_user, &pool, &BackstopTier::BlndUsdc)
                .accrued,
            blnd_usdc_pending.accrued
        );
        assert_eq!(
            candidate
                .user_ongoing_emissions(&blnd_xlm_user, &pool, &BackstopTier::BlndXlm)
                .accrued,
            blnd_xlm_pending.accrued
        );

        assert_eq!(
            candidate.claim_ongoing_blnd(&blnd_usdc_user, &pool, &blnd_usdc_recipient),
            46_666_666
        );
        assert_eq!(
            candidate.claim_ongoing_blnd(&blnd_xlm_user, &pool, &blnd_xlm_recipient),
            23_333_333
        );
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&blnd_usdc_recipient),
            46_666_666
        );
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&blnd_xlm_recipient),
            23_333_333
        );
        let pool_state = candidate.pool_ongoing_emissions(&pool);
        assert_eq!(pool_state.accrued_backstop, 1);
        assert_eq!(pool_state.accrued_pool, 3 * SCALAR_7);
        let ongoing = candidate.ongoing_emission_state();
        assert_eq!(ongoing.backstop_claimed, 7 * SCALAR_7 - 1);
        assert_eq!(ongoing.total_claimed, 7 * SCALAR_7 - 1);
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&fixture.candidate),
            3 * SCALAR_7 + 1
        );
        assert!(candidate
            .try_claim_ongoing_blnd(&blnd_usdc_user, &pool, &blnd_usdc_recipient)
            .is_err());
    }

    #[test]
    fn pool_tranche_is_reserved_daily_and_paid_only_by_the_registered_pool() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let pool = fixture.pool(100 * SCALAR_7, 0);
        let recipient = Address::generate(&fixture.env);
        fixture.set_reward_zone(&soroban_sdk::vec![&fixture.env, pool.clone()]);

        fixture.env.ledger().set_timestamp(1_010);
        assert_eq!(candidate.distribute().received, 10 * SCALAR_7);
        assert_eq!(
            candidate.pool_ongoing_emissions(&pool).accrued_pool,
            3 * SCALAR_7
        );
        assert_eq!(candidate.gulp_pool_emissions(&pool), 3 * SCALAR_7);
        assert_eq!(
            candidate.pool_emission_reservation(&pool),
            PoolEmissionReservation {
                available: 3 * SCALAR_7,
                last_gulp: Some(1_010),
            }
        );
        assert_eq!(candidate.pool_ongoing_emissions(&pool).accrued_pool, 0);
        assert!(candidate.try_gulp_pool_emissions(&pool).is_err());

        candidate.claim_pool_emissions(&pool, &recipient, &SCALAR_7);
        assert_eq!(
            candidate.pool_emission_reservation(&pool).available,
            2 * SCALAR_7
        );
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&recipient),
            SCALAR_7
        );
        let ongoing = candidate.ongoing_emission_state();
        assert_eq!(ongoing.total_claimed, SCALAR_7);
        assert_eq!(ongoing.backstop_claimed, 0);
        assert!(candidate
            .try_claim_pool_emissions(&pool, &recipient, &(3 * SCALAR_7))
            .is_err());
        assert_eq!(
            candidate.pool_emission_reservation(&pool).available,
            2 * SCALAR_7
        );
    }

    #[test]
    fn ongoing_user_checkpoints_exclude_late_deposits_and_q4w_shares() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let pool = fixture.pool(0, 0);
        let incumbent = Address::generate(&fixture.env);
        let late = Address::generate(&fixture.env);
        let recipient = Address::generate(&fixture.env);
        let deposit = 100 * SCALAR_7;
        let token = MockCometClient::new(&fixture.env, &fixture.blnd_usdc);
        token.mint(&incumbent, &deposit);
        token.mint(&late, &deposit);
        candidate.deposit_blnd_usdc(&incumbent, &pool, &deposit);
        fixture.set_reward_zone(&soroban_sdk::vec![&fixture.env, pool.clone()]);

        fixture.env.ledger().set_timestamp(1_010);
        candidate.distribute();
        candidate.deposit_blnd_usdc(&late, &pool, &deposit);
        assert_eq!(
            candidate
                .user_ongoing_emissions(&late, &pool, &BackstopTier::BlndUsdc)
                .accrued,
            0
        );

        fixture.env.ledger().set_timestamp(1_015);
        candidate.distribute();
        assert_eq!(
            candidate.claim_ongoing_blnd(&incumbent, &pool, &recipient),
            87_500_000
        );
        assert_eq!(
            candidate.claim_ongoing_blnd(&late, &pool, &recipient),
            17_500_000
        );

        fixture.env.ledger().set_timestamp(1_020);
        candidate.distribute();
        candidate.queue_blnd_usdc_withdrawal(&late, &pool, &deposit);
        let late_before = candidate
            .user_ongoing_emissions(&late, &pool, &BackstopTier::BlndUsdc)
            .accrued;
        fixture.env.ledger().set_timestamp(1_025);
        candidate.distribute();
        assert_eq!(
            candidate
                .user_ongoing_emissions(&late, &pool, &BackstopTier::BlndUsdc)
                .accrued,
            late_before
        );
        assert!(
            candidate
                .user_ongoing_emissions(&incumbent, &pool, &BackstopTier::BlndUsdc)
                .accrued
                > late_before
        );
    }

    #[test]
    fn ongoing_tier_indexes_and_users_carry_fractional_dust_forward() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let pool = fixture.pool(3, 0);
        let first = Address::generate(&fixture.env);
        let second = Address::generate(&fixture.env);
        let recipient = Address::generate(&fixture.env);
        fixture.env.as_contract(&fixture.candidate, || {
            set_position(
                &fixture.env,
                &first,
                &pool,
                &MigrationPosition {
                    amount: 1,
                    backfill_amount: 0,
                    backfill_claimed: false,
                    frozen_weight: Some(0),
                    time_basis: 0,
                },
            );
            set_position(
                &fixture.env,
                &second,
                &pool,
                &MigrationPosition {
                    amount: 2,
                    backfill_amount: 0,
                    backfill_claimed: false,
                    frozen_weight: Some(0),
                    time_basis: 0,
                },
            );
        });
        fixture.set_reward_zone(&soroban_sdk::vec![&fixture.env, pool.clone()]);

        fixture.env.ledger().set_timestamp(1_005);
        candidate.distribute();
        let pool_after_first = candidate.pool_ongoing_emissions(&pool);
        assert_eq!(pool_after_first.blnd_usdc_index_carry, 2);
        let first_pending =
            candidate.user_ongoing_emissions(&first, &pool, &BackstopTier::BlndUsdc);
        let second_pending =
            candidate.user_ongoing_emissions(&second, &pool, &BackstopTier::BlndUsdc);
        assert_eq!(first_pending.accrued, 11_666_666);
        assert_eq!(second_pending.accrued, 23_333_333);
        assert!(first_pending.carry > 0);
        assert!(second_pending.carry > 0);
        assert_eq!(
            candidate.claim_ongoing_blnd(&first, &pool, &recipient)
                + candidate.claim_ongoing_blnd(&second, &pool, &recipient),
            35_000_000 - 1
        );
        assert_eq!(candidate.pool_ongoing_emissions(&pool).accrued_backstop, 1);

        fixture.env.ledger().set_timestamp(1_010);
        candidate.distribute();
        assert_eq!(
            candidate.claim_ongoing_blnd(&first, &pool, &recipient)
                + candidate.claim_ongoing_blnd(&second, &pool, &recipient),
            35_000_000
        );
        assert_eq!(candidate.pool_ongoing_emissions(&pool).accrued_backstop, 1);
    }

    #[test]
    fn ongoing_claim_requires_position_owner_authorization() {
        let fixture = OngoingFixture::create_without_auth_mocking();
        let candidate = fixture.client();
        let pool = fixture.pool(100 * SCALAR_7, 0);
        let user = Address::generate(&fixture.env);
        let recipient = Address::generate(&fixture.env);
        fixture.env.as_contract(&fixture.candidate, || {
            set_position(
                &fixture.env,
                &user,
                &pool,
                &MigrationPosition {
                    amount: 100 * SCALAR_7,
                    backfill_amount: 0,
                    backfill_claimed: false,
                    frozen_weight: Some(0),
                    time_basis: 0,
                },
            );
        });
        fixture.set_reward_zone(&soroban_sdk::vec![&fixture.env, pool.clone()]);
        fixture.env.ledger().set_timestamp(1_010);
        candidate.distribute();

        assert!(candidate
            .try_claim_ongoing_blnd(&user, &pool, &recipient)
            .is_err());
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&recipient),
            0
        );
        assert_eq!(
            candidate
                .mock_auths(&[MockAuth {
                    address: &user,
                    invoke: &MockAuthInvoke {
                        contract: &fixture.candidate,
                        fn_name: "claim_ongoing_blnd",
                        args: (&user, &pool, &recipient).into_val(&fixture.env),
                        sub_invokes: &[],
                    },
                }])
                .claim_ongoing_blnd(&user, &pool, &recipient),
            7 * SCALAR_7
        );
    }

    #[test]
    fn ongoing_distribution_rejects_prefunded_token_without_binding_proof() {
        let fixture = OngoingFixture::create_with_blnd_binding(false);
        let candidate = fixture.client();
        let pool = fixture.pool(100 * SCALAR_7, 0);
        fixture.set_reward_zone(&soroban_sdk::vec![&fixture.env, pool]);
        let prefunded = 10 * SCALAR_7;
        StellarAssetClient::new(&fixture.env, &fixture.blnd).mint(&fixture.candidate, &prefunded);
        fixture.env.ledger().set_timestamp(1_005);

        assert!(!candidate.blnd_binding_verified());
        assert!(candidate.try_distribute().is_err());
        assert!(!candidate.blnd_binding_verified());
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.blnd).balance(&fixture.candidate),
            prefunded
        );
        assert_eq!(
            TokenClient::new(&fixture.env, &fixture.emitted_blnd).balance(&fixture.candidate),
            0
        );
        assert_eq!(
            MockEmitterClient::new(&fixture.env, &fixture.emitter)
                .get_last_distro(&fixture.candidate),
            1_000
        );
        assert_eq!(
            candidate.ongoing_emission_state(),
            OngoingEmissionState {
                backstop_allocated: 0,
                backstop_carry: 0,
                backstop_claimed: 0,
                last_distribution: Some(1_000),
                pool_allocated: 0,
                pool_carry: 0,
                split_carry: 0,
                total_claimed: 0,
                total_received: 0,
            }
        );
        assert_eq!(candidate.reward_zone_checkpoint(), None);
    }

    #[test]
    fn ongoing_distribution_handles_maximum_reward_zone() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let mut reward_zone = Vec::new(&fixture.env);
        for _ in 0..MAX_REWARD_ZONE_SIZE {
            reward_zone.push_back(fixture.pool(10 * SCALAR_7, 10 * SCALAR_7));
        }
        fixture.set_reward_zone(&reward_zone);
        fixture.env.ledger().set_timestamp(1_005);

        let distribution = candidate.distribute();
        assert!(candidate.blnd_binding_verified());
        assert_eq!(distribution.received, 5 * SCALAR_7);
        assert_eq!(distribution.eligible_blnd, 1_800 * SCALAR_7);
        assert_eq!(candidate.reward_zone().len(), MAX_REWARD_ZONE_SIZE);
        for pool in reward_zone.iter() {
            let state = candidate.pool_ongoing_emissions(&pool);
            assert!(state.accrued_backstop > 0);
            assert!(state.accrued_pool > 0);
        }
    }

    #[test]
    fn ongoing_distribution_requires_recent_checkpoint_before_deposit_weight_changes() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let first = fixture.pool(100 * SCALAR_7, 0);
        let second = fixture.pool(100 * SCALAR_7, 0);
        fixture.set_reward_zone(&soroban_sdk::vec![
            &fixture.env,
            first.clone(),
            second.clone()
        ]);

        fixture.env.ledger().set_timestamp(1_010);
        candidate.distribute();

        let depositor = Address::generate(&fixture.env);
        MockCometClient::new(&fixture.env, &fixture.blnd_usdc).mint(&depositor, &(100 * SCALAR_7));
        fixture.env.ledger().set_timestamp(1_020);
        assert!(candidate
            .try_deposit_blnd_usdc(&depositor, &first, &(100 * SCALAR_7))
            .is_err());
        assert_eq!(
            candidate.pool_ongoing_emissions(&first).active_blnd_usdc,
            100 * SCALAR_7
        );

        candidate.distribute();
        fixture.env.ledger().set_timestamp(1_025);
        candidate.deposit_blnd_usdc(&depositor, &first, &(100 * SCALAR_7));

        let first_after_deposit = candidate.pool_ongoing_emissions(&first);
        let second_after_deposit = candidate.pool_ongoing_emissions(&second);
        assert_eq!(
            first_after_deposit.accrued_backstop,
            second_after_deposit.accrued_backstop
        );
        assert_eq!(
            first_after_deposit.accrued_pool,
            second_after_deposit.accrued_pool
        );
        assert_eq!(first_after_deposit.active_blnd_usdc, 200 * SCALAR_7);
        assert_eq!(second_after_deposit.active_blnd_usdc, 100 * SCALAR_7);
        assert_eq!(
            candidate.ongoing_emission_state().total_received,
            20 * SCALAR_7
        );

        fixture.env.ledger().set_timestamp(1_030);
        candidate.distribute();
        let first_after_next = candidate.pool_ongoing_emissions(&first);
        let second_after_next = candidate.pool_ongoing_emissions(&second);
        assert!(
            first_after_next.accrued_backstop - first_after_deposit.accrued_backstop
                > second_after_next.accrued_backstop - second_after_deposit.accrued_backstop
        );
    }

    #[test]
    fn ongoing_distribution_requires_recent_checkpoint_for_q4w_loss_and_gain_changes() {
        let fixture = OngoingFixture::create();
        let candidate = fixture.client();
        let first = fixture.pool(0, 0);
        let second = fixture.pool(0, 0);
        let depositor = Address::generate(&fixture.env);
        let token = MockCometClient::new(&fixture.env, &fixture.blnd_usdc);
        token.mint(&depositor, &(200 * SCALAR_7));
        candidate.deposit_blnd_usdc(&depositor, &first, &(100 * SCALAR_7));
        candidate.deposit_blnd_usdc(&depositor, &second, &(100 * SCALAR_7));
        fixture.set_reward_zone(&soroban_sdk::vec![
            &fixture.env,
            first.clone(),
            second.clone()
        ]);

        fixture.env.ledger().set_timestamp(1_010);
        candidate.distribute();
        fixture.env.ledger().set_timestamp(1_020);
        assert!(candidate
            .try_queue_blnd_usdc_withdrawal(&depositor, &first, &(50 * SCALAR_7))
            .is_err());
        assert_eq!(
            candidate.pool_ongoing_emissions(&first).active_blnd_usdc,
            100 * SCALAR_7
        );

        candidate.distribute();
        fixture.env.ledger().set_timestamp(1_025);
        candidate.queue_blnd_usdc_withdrawal(&depositor, &first, &(50 * SCALAR_7));
        let first_after_queue = candidate.pool_ongoing_emissions(&first);
        let second_after_queue = candidate.pool_ongoing_emissions(&second);
        assert_eq!(
            first_after_queue.accrued_backstop,
            second_after_queue.accrued_backstop
        );
        assert_eq!(first_after_queue.active_blnd_usdc, 50 * SCALAR_7);

        fixture.env.ledger().set_timestamp(1_030);
        candidate.distribute();
        let first_before_dequeue = candidate.pool_ongoing_emissions(&first);
        let second_before_dequeue = candidate.pool_ongoing_emissions(&second);
        assert!(
            first_before_dequeue.accrued_backstop - first_after_queue.accrued_backstop
                < second_before_dequeue.accrued_backstop - second_after_queue.accrued_backstop
        );

        fixture.env.ledger().set_timestamp(1_035);
        candidate.dequeue_blnd_usdc_withdrawal(&depositor, &first, &(50 * SCALAR_7));
        assert_eq!(
            candidate.pool_ongoing_emissions(&first).active_blnd_usdc,
            100 * SCALAR_7
        );

        fixture.env.ledger().set_timestamp(1_040);
        candidate.distribute();
        let first_before_loss = candidate.pool_ongoing_emissions(&first);
        let second_before_loss = candidate.pool_ongoing_emissions(&second);
        assert_eq!(
            first_before_loss.accrued_backstop - first_before_dequeue.accrued_backstop,
            second_before_loss.accrued_backstop - second_before_dequeue.accrued_backstop
        );

        fixture.env.ledger().set_timestamp(1_045);
        fixture.env.as_contract(&fixture.candidate, || {
            apply_pool_tier_loss(&fixture.env, BackstopTier::BlndUsdc, &first, 50 * SCALAR_7);
        });
        let first_after_loss = candidate.pool_ongoing_emissions(&first);
        let second_after_loss = candidate.pool_ongoing_emissions(&second);
        assert_eq!(
            first_after_loss.accrued_backstop,
            first_before_loss.accrued_backstop
        );
        assert_eq!(
            second_after_loss.accrued_backstop,
            second_before_loss.accrued_backstop
        );
        assert_eq!(first_after_loss.active_blnd_usdc, 50 * SCALAR_7);

        fixture.env.ledger().set_timestamp(1_050);
        candidate.distribute();
        let first_before_gain = candidate.pool_ongoing_emissions(&first);
        let second_before_gain = candidate.pool_ongoing_emissions(&second);
        assert!(
            first_before_gain.accrued_backstop - first_after_loss.accrued_backstop
                < second_before_gain.accrued_backstop - second_after_loss.accrued_backstop
        );

        fixture.env.ledger().set_timestamp(1_055);
        fixture.env.as_contract(&fixture.candidate, || {
            apply_pool_tier_gain(&fixture.env, BackstopTier::BlndUsdc, &first, 50 * SCALAR_7);
        });
        assert_eq!(
            candidate.pool_ongoing_emissions(&first).active_blnd_usdc,
            100 * SCALAR_7
        );
        assert_eq!(
            candidate.ongoing_emission_state().last_distribution,
            Some(1_050)
        );
    }

    #[test]
    fn reward_zone_is_permissionless_bounded_and_checkpoint_gated() {
        let fixture = RewardZoneFixture::create();
        let candidate = fixture.client();
        let blnd_amount = SCALAR_7;
        let qualifying_usdc = ACTIVATION_ENTRY_THRESHOLD_USDC - blnd_amount;

        let usdc_only = fixture.pool();
        fixture.set_tier_state(
            &usdc_only,
            BackstopTier::Usdc,
            ACTIVATION_ENTRY_THRESHOLD_USDC,
            ACTIVATION_ENTRY_THRESHOLD_USDC,
            0,
        );
        assert!(
            candidate.pool_valuation(&usdc_only).active_values.usdc
                >= ACTIVATION_ENTRY_THRESHOLD_USDC
        );
        assert!(candidate.try_add_reward(&usdc_only, &None).is_err());
        assert!(candidate.reward_zone().is_empty());

        let first = fixture.pool();
        fixture.set_tier_state(&first, BackstopTier::BlndUsdc, blnd_amount, blnd_amount, 0);
        fixture.set_tier_state(
            &first,
            BackstopTier::Usdc,
            qualifying_usdc - 1,
            qualifying_usdc - 1,
            0,
        );

        assert!(candidate.try_add_reward(&first, &None).is_err());
        fixture.set_tier_state(
            &first,
            BackstopTier::Usdc,
            qualifying_usdc,
            qualifying_usdc,
            0,
        );
        candidate.add_reward(&first, &None);
        assert_eq!(
            candidate.reward_zone(),
            soroban_sdk::vec![&fixture.env, first.clone()]
        );
        assert_eq!(candidate.reward_zone_checkpoint(), None);
        assert!(candidate.try_add_reward(&first, &None).is_err());
        assert!(candidate
            .try_add_reward(&Address::generate(&fixture.env), &None)
            .is_err());

        let second = fixture.pool();
        fixture.set_tier_state(&second, BackstopTier::BlndUsdc, blnd_amount, blnd_amount, 0);
        fixture.set_tier_state(
            &second,
            BackstopTier::Usdc,
            qualifying_usdc,
            qualifying_usdc,
            0,
        );
        assert!(candidate.try_add_reward(&second, &None).is_err());

        fixture.set_checkpoint(
            fixture.env.ledger().timestamp() - REWARD_ZONE_CHECKPOINT_MAX_AGE_SECONDS,
        );
        candidate.add_reward(&second, &None);
        fixture.env.ledger().set_timestamp(
            fixture.env.ledger().timestamp() + REWARD_ZONE_CHECKPOINT_MAX_AGE_SECONDS + 1,
        );
        let third = fixture.pool();
        fixture.set_tier_state(&third, BackstopTier::BlndUsdc, blnd_amount, blnd_amount, 0);
        fixture.set_tier_state(
            &third,
            BackstopTier::Usdc,
            qualifying_usdc,
            qualifying_usdc,
            0,
        );
        assert!(candidate.try_add_reward(&third, &None).is_err());

        fixture.set_checkpoint(fixture.env.ledger().timestamp());
        candidate.add_reward(&third, &None);
        while candidate.reward_zone().len() < MAX_REWARD_ZONE_SIZE {
            let pool = fixture.pool();
            fixture.set_tier_state(&pool, BackstopTier::BlndUsdc, blnd_amount, blnd_amount, 0);
            fixture.set_tier_state(
                &pool,
                BackstopTier::Usdc,
                qualifying_usdc,
                qualifying_usdc,
                0,
            );
            candidate.add_reward(&pool, &None);
        }
        assert_eq!(candidate.reward_zone().len(), MAX_REWARD_ZONE_SIZE);

        let overflow = fixture.pool();
        fixture.set_tier_state(
            &overflow,
            BackstopTier::BlndUsdc,
            blnd_amount,
            blnd_amount,
            0,
        );
        fixture.set_tier_state(
            &overflow,
            BackstopTier::Usdc,
            qualifying_usdc,
            qualifying_usdc,
            0,
        );
        assert!(candidate.try_add_reward(&overflow, &None).is_err());
        assert_eq!(candidate.reward_zone().len(), MAX_REWARD_ZONE_SIZE);
    }

    #[test]
    fn reward_zone_replacement_uses_spot_active_blnd_and_keeps_commitments_eligible() {
        let fixture = RewardZoneFixture::create();
        let candidate = fixture.client();
        let removed = fixture.pool();
        let entrant = fixture.pool();
        let mut reward_zone = Vec::new(&fixture.env);
        reward_zone.push_back(removed.clone());
        while reward_zone.len() < MAX_REWARD_ZONE_SIZE {
            reward_zone.push_back(fixture.pool());
        }
        fixture.set_reward_zone(&reward_zone);
        fixture.set_checkpoint(fixture.env.ledger().timestamp());

        fixture.set_tier_state(
            &removed,
            BackstopTier::BlndUsdc,
            100 * SCALAR_7,
            100 * SCALAR_7,
            0,
        );
        fixture.set_tier_state(
            &entrant,
            BackstopTier::BlndUsdc,
            101 * SCALAR_7,
            101 * SCALAR_7,
            SCALAR_7,
        );
        fixture.set_tier_state(
            &entrant,
            BackstopTier::Usdc,
            ACTIVATION_ENTRY_THRESHOLD_USDC,
            ACTIVATION_ENTRY_THRESHOLD_USDC,
            0,
        );
        fixture.commit_blnd_usdc(&entrant, 50 * SCALAR_7);

        assert_eq!(
            candidate.pool_spot_blnd_emission_values(&removed),
            BlndEmissionValues {
                blnd_usdc: 400 * SCALAR_7,
                blnd_xlm: 0,
            }
        );
        assert_eq!(
            candidate.pool_spot_blnd_emission_values(&entrant),
            BlndEmissionValues {
                blnd_usdc: 400 * SCALAR_7,
                blnd_xlm: 0,
            }
        );
        assert!(candidate
            .try_add_reward(&entrant, &Some(removed.clone()))
            .is_err());

        fixture.add_queued_shares(&entrant, BackstopTier::BlndUsdc, -SCALAR_7);
        assert_eq!(
            candidate.pool_spot_blnd_emission_values(&entrant),
            BlndEmissionValues {
                blnd_usdc: 404 * SCALAR_7,
                blnd_xlm: 0,
            }
        );
        candidate.add_reward(&entrant, &Some(removed.clone()));
        let updated = candidate.reward_zone();
        assert_eq!(updated.len(), MAX_REWARD_ZONE_SIZE);
        assert_eq!(updated.first(), Some(entrant));
        assert!(!updated.contains(removed));
    }

    #[test]
    fn reward_zone_removal_uses_maintenance_hysteresis_without_pool_status() {
        let fixture = RewardZoneFixture::create();
        let candidate = fixture.client();
        let pool = fixture.pool();
        let blnd_amount = SCALAR_7;
        fixture.set_tier_state(&pool, BackstopTier::BlndUsdc, blnd_amount, blnd_amount, 0);
        fixture.set_tier_state(
            &pool,
            BackstopTier::Usdc,
            ACTIVATION_ENTRY_THRESHOLD_USDC - blnd_amount,
            ACTIVATION_ENTRY_THRESHOLD_USDC - blnd_amount,
            0,
        );
        candidate.add_reward(&pool, &None);

        fixture.set_tier_state(
            &pool,
            BackstopTier::Usdc,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC - blnd_amount - 1,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC - blnd_amount - 1,
            0,
        );
        assert!(candidate.try_remove_reward(&pool).is_err());

        fixture.set_checkpoint(fixture.env.ledger().timestamp());
        fixture.set_tier_state(
            &pool,
            BackstopTier::Usdc,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC - blnd_amount,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC - blnd_amount,
            0,
        );
        assert!(candidate.try_remove_reward(&pool).is_err());

        fixture.set_tier_state(
            &pool,
            BackstopTier::Usdc,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC - blnd_amount - 1,
            ACTIVATION_MAINTENANCE_THRESHOLD_USDC - blnd_amount - 1,
            0,
        );
        candidate.remove_reward(&pool);
        assert!(candidate.reward_zone().is_empty());
    }

    #[test]
    fn zero_blnd_reward_zone_member_is_removable_without_checkpoint() {
        let fixture = RewardZoneFixture::create();
        let candidate = fixture.client();
        let pool = fixture.pool();
        fixture.set_tier_state(&pool, BackstopTier::BlndUsdc, SCALAR_7, SCALAR_7, 0);
        fixture.set_tier_state(
            &pool,
            BackstopTier::Usdc,
            ACTIVATION_ENTRY_THRESHOLD_USDC - SCALAR_7,
            ACTIVATION_ENTRY_THRESHOLD_USDC - SCALAR_7,
            0,
        );
        candidate.add_reward(&pool, &None);
        assert_eq!(candidate.reward_zone_checkpoint(), None);

        fixture.set_tier_state(&pool, BackstopTier::BlndUsdc, 0, 0, 0);
        fixture.set_tier_state(
            &pool,
            BackstopTier::Usdc,
            ACTIVATION_ENTRY_THRESHOLD_USDC,
            ACTIVATION_ENTRY_THRESHOLD_USDC,
            0,
        );
        candidate.remove_reward(&pool);
        assert!(candidate.reward_zone().is_empty());
    }

    #[test]
    fn pool_tier_exchange_rate_rounds_conservatively_and_sweeps_final_dust() {
        let env = Env::default();
        let impaired = PoolTierState {
            assets: 40,
            queued_shares: 40,
            shares: 100,
        };

        assert_eq!(shares_from_assets(&env, 10, &impaired), 25);
        assert_eq!(assets_from_shares(&env, 40, &impaired), 16);
        assert_eq!(assets_from_shares(&env, 60, &impaired), 24);

        let dusty = PoolTierState {
            assets: 10,
            queued_shares: 0,
            shares: 6,
        };
        assert_eq!(assets_from_shares(&env, 1, &dusty), 1);
        assert_eq!(assets_from_shares(&env, 6, &dusty), 10);

        let drained = PoolTierState {
            assets: 0,
            queued_shares: 0,
            shares: 100,
        };
        assert_eq!(shares_from_assets(&env, 10, &drained), 0);
        assert_eq!(assets_from_shares(&env, 100, &drained), 0);
    }

    #[test]
    fn bad_debt_lots_commit_first_qualifying_pool_local_tier() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let incumbent = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let usdc_admin = Address::generate(&env);
        let usdc_token = env
            .register_stellar_asset_contract_v2(usdc_admin.clone())
            .address();
        let candidate_id = Address::generate(&env);
        let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
        let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
        let pool = env.register(MockLossAwarePool, (&candidate_id,));
        let other_pool = env.register(MockLossAwarePool, (&candidate_id,));
        let below_minimum_pool = env.register(MockLossAwarePool, (&candidate_id,));
        pool_factory.set_pool(&pool);
        pool_factory.set_pool(&other_pool);
        pool_factory.set_pool(&below_minimum_pool);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter =
            register_valuation_adapter(&env, &blnd_token, &legacy_token, &next_token, &usdc_token);
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
            for (tier, primary_amount, other_amount) in [
                (BackstopTier::BlndUsdc, 50 * SCALAR_7, 200 * SCALAR_7),
                (BackstopTier::BlndXlm, 100 * SCALAR_7, 300 * SCALAR_7),
                (BackstopTier::Usdc, 500 * SCALAR_7, 500 * SCALAR_7),
            ] {
                if tier == BackstopTier::BlndUsdc {
                    set_pool_blnd_usdc_shares(&env, &pool, primary_amount);
                    set_pool_blnd_usdc_shares(&env, &other_pool, other_amount);
                    set_pool_blnd_usdc_shares(
                        &env,
                        &below_minimum_pool,
                        BAD_DEBT_TIER_MINIMUM_VALUE_USDC - 1,
                    );
                } else {
                    set_pool_tier_shares(&env, tier, &pool, primary_amount);
                    set_pool_tier_shares(&env, tier, &other_pool, other_amount);
                    set_pool_tier_shares(
                        &env,
                        tier,
                        &below_minimum_pool,
                        BAD_DEBT_TIER_MINIMUM_VALUE_USDC - 1,
                    );
                }
                set_pool_tier_assets(&env, tier, &pool, primary_amount);
                set_pool_tier_assets(&env, tier, &other_pool, other_amount);
                set_pool_tier_assets(
                    &env,
                    tier,
                    &below_minimum_pool,
                    BAD_DEBT_TIER_MINIMUM_VALUE_USDC - 1,
                );
                env.storage().instance().set(
                    &DataKey::TotalTierAssets(tier),
                    &(primary_amount + other_amount + BAD_DEBT_TIER_MINIMUM_VALUE_USDC - 1),
                );
            }
        });
        StellarAssetClient::new(&env, &usdc_token)
            .mock_all_auths()
            .mint(&candidate_id, &(179 * SCALAR_7));

        let auction_id = BytesN::from_array(&env, &[1; 32]);
        let adapter = MockValuationAdapterClient::new(&env, &valuation_adapter);
        adapter.set_valid_until(&999);
        assert!(candidate
            .try_quote_bad_debt_lot(&pool, &(200 * SCALAR_7))
            .is_err());
        adapter.set_valid_until(&u64::MAX);
        adapter.set_invalid(&true);
        assert!(candidate
            .try_quote_bad_debt_lot(&pool, &(200 * SCALAR_7))
            .is_err());
        adapter.set_invalid(&false);

        let expected = BadDebtLotQuote {
            committed_value: 240 * SCALAR_7,
            debt_value: 200 * SCALAR_7,
            tier: BackstopTier::Usdc,
            lot_amount: 240 * SCALAR_7,
            target_value: 240 * SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        };
        assert_eq!(
            candidate.quote_bad_debt_lot(&pool, &(200 * SCALAR_7)),
            Some(expected.clone())
        );
        assert!(candidate
            .try_commit_bad_debt_lot(&pool, &auction_id, &(200 * SCALAR_7))
            .is_err());

        let pool_client = MockLossAwarePoolClient::new(&env, &pool);
        assert_eq!(
            pool_client.commit_bad_debt_lot(&candidate_id, &auction_id, &(200 * SCALAR_7)),
            expected
        );
        assert_eq!(
            candidate.bad_debt_commitment(&pool, &auction_id),
            Some(expected.clone())
        );
        env.as_contract(&candidate_id, || {
            assert_eq!(
                env.storage()
                    .temporary()
                    .get_ttl(&TemporaryKey::BadDebtCommitment(pool.clone())),
                AUCTION_TTL_BUMP
            );
        });
        let commitment_ledger = env.ledger().sequence();
        env.ledger()
            .set_sequence_number(commitment_ledger.checked_add(10).unwrap());
        assert_eq!(
            candidate.bad_debt_commitment(&pool, &auction_id),
            Some(expected.clone())
        );
        env.as_contract(&candidate_id, || {
            assert_eq!(
                env.storage()
                    .temporary()
                    .get_ttl(&TemporaryKey::BadDebtCommitment(pool.clone())),
                AUCTION_TTL_BUMP - 10
            );
        });
        assert_eq!(candidate.pool_bad_debt_commitment_count(&pool), 1);
        assert_eq!(
            candidate.pool_tier_committed_assets(&BackstopTier::BlndUsdc, &pool),
            0
        );
        assert_eq!(
            candidate.pool_tier_committed_assets(&BackstopTier::BlndXlm, &pool),
            0
        );
        assert_eq!(
            candidate.pool_tier_committed_assets(&BackstopTier::Usdc, &pool),
            240 * SCALAR_7
        );
        assert_eq!(
            candidate.pool_valuation(&pool).active_values,
            ActivationValues {
                blnd_usdc: 50 * SCALAR_7,
                blnd_xlm: 100 * SCALAR_7,
                usdc: 260 * SCALAR_7,
            }
        );
        assert_eq!(
            candidate.quote_bad_debt_lot(&pool, &(100 * SCALAR_7)),
            Some(BadDebtLotQuote {
                committed_value: 120 * SCALAR_7,
                debt_value: 100 * SCALAR_7,
                tier: BackstopTier::Usdc,
                lot_amount: 120 * SCALAR_7,
                target_value: 120 * SCALAR_7,
                unfilled_target_value: 0,
                valid_until: u64::MAX,
            })
        );
        assert_eq!(
            candidate.quote_bad_debt_lot(&other_pool, &(250 * SCALAR_7)),
            Some(BadDebtLotQuote {
                committed_value: 200 * SCALAR_7,
                debt_value: 250 * SCALAR_7,
                tier: BackstopTier::BlndUsdc,
                lot_amount: 200 * SCALAR_7,
                target_value: 300 * SCALAR_7,
                unfilled_target_value: 100 * SCALAR_7,
                valid_until: u64::MAX,
            })
        );
        assert_eq!(
            candidate.quote_bad_debt_lot(&below_minimum_pool, &SCALAR_7),
            None
        );
        let below_minimum_pool_client = MockLossAwarePoolClient::new(&env, &below_minimum_pool);
        assert!(below_minimum_pool_client
            .try_commit_bad_debt_lot(
                &candidate_id,
                &BytesN::from_array(&env, &[2; 32]),
                &SCALAR_7
            )
            .is_err());
        assert!(pool_client
            .try_commit_bad_debt_lot(&candidate_id, &auction_id, &(200 * SCALAR_7))
            .is_err());
        assert!(candidate
            .try_release_bad_debt_lot(&pool, &auction_id)
            .is_err());
        assert!(candidate
            .try_settle_bad_debt_lot(
                &pool,
                &auction_id,
                &(120 * SCALAR_7),
                &(60 * SCALAR_7),
                &Address::generate(&env)
            )
            .is_err());
        assert!(pool_client
            .try_settle_bad_debt_lot(
                &candidate_id,
                &auction_id,
                &(120 * SCALAR_7),
                &(121 * SCALAR_7),
                &Address::generate(&env)
            )
            .is_err());
        env.mock_all_auths();
        assert!(candidate
            .try_withdraw_usdc(
                &Address::generate(&env),
                &pool,
                &1,
                &Address::generate(&env)
            )
            .is_err());

        let recipient = Address::generate(&env);
        let total_usdc_before = candidate.total_tier_assets(&BackstopTier::Usdc);
        assert_eq!(
            pool_client.settle_bad_debt_lot(
                &candidate_id,
                &auction_id,
                &(120 * SCALAR_7),
                &(60 * SCALAR_7),
                &recipient
            ),
            Some(BadDebtLotQuote {
                committed_value: 120 * SCALAR_7,
                debt_value: 200 * SCALAR_7,
                tier: BackstopTier::Usdc,
                lot_amount: 120 * SCALAR_7,
                target_value: 240 * SCALAR_7,
                unfilled_target_value: 0,
                valid_until: u64::MAX,
            })
        );
        assert_eq!(
            TokenClient::new(&env, &usdc_token).balance(&recipient),
            60 * SCALAR_7
        );
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &pool),
            PoolTierState {
                assets: 440 * SCALAR_7,
                queued_shares: 0,
                shares: 500 * SCALAR_7,
            }
        );
        assert_eq!(
            candidate.pool_tier_committed_assets(&BackstopTier::Usdc, &pool),
            120 * SCALAR_7
        );
        assert_eq!(
            candidate.pool_valuation(&pool).active_values,
            ActivationValues {
                blnd_usdc: 50 * SCALAR_7,
                blnd_xlm: 100 * SCALAR_7,
                usdc: 320 * SCALAR_7,
            }
        );
        assert_eq!(
            candidate.total_tier_assets(&BackstopTier::Usdc),
            total_usdc_before - 60 * SCALAR_7
        );

        assert!(pool_client
            .try_settle_bad_debt_lot(
                &candidate_id,
                &auction_id,
                &(120 * SCALAR_7),
                &(120 * SCALAR_7),
                &recipient
            )
            .is_err());
        assert_eq!(
            candidate.bad_debt_commitment(&pool, &auction_id),
            Some(BadDebtLotQuote {
                committed_value: 120 * SCALAR_7,
                debt_value: 200 * SCALAR_7,
                tier: BackstopTier::Usdc,
                lot_amount: 120 * SCALAR_7,
                target_value: 240 * SCALAR_7,
                unfilled_target_value: 0,
                valid_until: u64::MAX,
            })
        );
        assert_eq!(
            candidate.pool_tier_assets(&BackstopTier::Usdc, &pool),
            440 * SCALAR_7
        );
        assert_eq!(
            TokenClient::new(&env, &usdc_token).balance(&recipient),
            60 * SCALAR_7
        );
        StellarAssetClient::new(&env, &usdc_token)
            .mock_all_auths()
            .mint(&candidate_id, &SCALAR_7);

        assert_eq!(
            pool_client.settle_bad_debt_lot(
                &candidate_id,
                &auction_id,
                &(120 * SCALAR_7),
                &(120 * SCALAR_7),
                &recipient
            ),
            None
        );
        assert_eq!(candidate.bad_debt_commitment(&pool, &auction_id), None);
        assert_eq!(candidate.pool_bad_debt_commitment_count(&pool), 0);
        assert_eq!(
            TokenClient::new(&env, &usdc_token).balance(&recipient),
            180 * SCALAR_7
        );
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &pool),
            PoolTierState {
                assets: 320 * SCALAR_7,
                queued_shares: 0,
                shares: 500 * SCALAR_7,
            }
        );
        assert_eq!(
            candidate.pool_valuation(&pool).active_values,
            ActivationValues {
                blnd_usdc: 50 * SCALAR_7,
                blnd_xlm: 100 * SCALAR_7,
                usdc: 320 * SCALAR_7,
            }
        );
        assert_eq!(
            candidate.total_tier_assets(&BackstopTier::Usdc),
            total_usdc_before - 180 * SCALAR_7
        );

        let release_auction_id = BytesN::from_array(&env, &[2; 32]);
        assert_eq!(
            pool_client
                .commit_bad_debt_lot(&candidate_id, &release_auction_id, &(100 * SCALAR_7))
                .lot_amount,
            120 * SCALAR_7
        );
        pool_client.release_bad_debt_lot(&candidate_id, &release_auction_id);
        assert_eq!(
            candidate.bad_debt_commitment(&pool, &release_auction_id),
            None
        );
        assert_eq!(
            candidate.pool_tier_assets(&BackstopTier::Usdc, &pool),
            320 * SCALAR_7
        );
    }

    #[test]
    fn interest_commitment_donates_to_one_tier_without_minting_shares() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let incumbent = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let usdc_admin = Address::generate(&env);
        let usdc_token = env
            .register_stellar_asset_contract_v2(usdc_admin.clone())
            .address();
        let candidate_id = Address::generate(&env);
        let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
        let pool = env.register(MockLossAwarePool, (&candidate_id,));
        MockPoolFactoryClient::new(&env, &pool_factory_id).set_pool(&pool);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter =
            register_valuation_adapter(&env, &blnd_token, &legacy_token, &next_token, &usdc_token);
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
        let pool_client = MockLossAwarePoolClient::new(&env, &pool);
        let initial = 500 * SCALAR_7;
        env.as_contract(&candidate_id, || {
            set_pool_tier_shares(&env, BackstopTier::Usdc, &pool, initial);
            set_pool_tier_assets(&env, BackstopTier::Usdc, &pool, initial);
            env.storage()
                .instance()
                .set(&DataKey::TotalTierShares(BackstopTier::Usdc), &initial);
            env.storage()
                .instance()
                .set(&DataKey::TotalTierAssets(BackstopTier::Usdc), &initial);
        });
        StellarAssetClient::new(&env, &usdc_token)
            .mock_all_auths()
            .mint(&candidate_id, &initial);

        let distributions = soroban_sdk::map![
            &env,
            (Address::generate(&env), 140_i128),
            (Address::generate(&env), 141_i128)
        ];
        let weighted = candidate.quote_pool_take_rate_batch(&pool, &distributions);
        for (asset, amount) in distributions.iter() {
            let quote = weighted.get(asset).unwrap();
            assert_eq!(
                quote.blnd_usdc + quote.blnd_xlm + quote.usdc + quote.remainder,
                amount
            );
        }

        let auction_id = BytesN::from_array(&env, &[51; 32]);
        assert!(candidate
            .try_commit_interest_lot(&pool, &auction_id, &BackstopTier::Usdc, &(200 * SCALAR_7))
            .is_err());
        let quote = pool_client.commit_interest_lot(
            &candidate_id,
            &auction_id,
            &BackstopTier::Usdc,
            &(200 * SCALAR_7),
        );
        assert_eq!(quote.bid_token, usdc_token);
        assert_eq!(quote.bid_amount, 240 * SCALAR_7);
        assert_eq!(
            candidate.interest_commitment(&pool, &auction_id),
            Some(quote.clone())
        );

        let depositor = Address::generate(&env);
        let filler = Address::generate(&env);
        StellarAssetClient::new(&env, &usdc_token)
            .mock_all_auths()
            .mint(&depositor, &(10 * SCALAR_7));
        StellarAssetClient::new(&env, &usdc_token)
            .mock_all_auths()
            .mint(&filler, &(120 * SCALAR_7));
        env.mock_all_auths_allowing_non_root_auth();
        assert!(candidate
            .try_deposit_usdc(&depositor, &pool, &(10 * SCALAR_7))
            .is_err());

        assert_eq!(
            pool_client.settle_interest_lot(
                &candidate_id,
                &auction_id,
                &(240 * SCALAR_7),
                &(120 * SCALAR_7),
                &filler,
            ),
            None
        );
        assert_eq!(candidate.interest_commitment(&pool, &auction_id), None);
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &pool),
            PoolTierState {
                assets: 620 * SCALAR_7,
                queued_shares: 0,
                shares: initial,
            }
        );
        assert_eq!(
            candidate.total_tier_assets(&BackstopTier::Usdc),
            620 * SCALAR_7
        );
        assert_eq!(TokenClient::new(&env, &usdc_token).balance(&filler), 0);

        candidate.deposit_usdc(&depositor, &pool, &(10 * SCALAR_7));
        assert!(candidate.tier_shares(&BackstopTier::Usdc, &depositor, &pool) < 10 * SCALAR_7);
    }

    #[test]
    fn bad_debt_tier_selection_rounds_assets_up_without_underfunding() {
        let env = Env::default();
        assert_eq!(allocate_bad_debt_tier(&env, 3, 2, 1), (2, 1, 0));
        assert_eq!(allocate_bad_debt_tier(&env, 3, 2, 2), (3, 2, 0));
        assert_eq!(allocate_bad_debt_tier(&env, 3, 2, 3), (3, 2, 1));
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
        assert_eq!(
            candidate.ongoing_emission_state(),
            OngoingEmissionState {
                backstop_allocated: 0,
                backstop_carry: 0,
                backstop_claimed: 0,
                last_distribution: Some(unlock),
                pool_allocated: 0,
                pool_carry: 0,
                split_carry: 0,
                total_claimed: 0,
                total_received: 0,
            }
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
                    backfill_amount: 10,
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
        let valuation_adapter = register_valuation_adapter(
            &env,
            &blnd_token,
            &legacy_token,
            &next_token,
            &prefund_token,
        );
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
    #[should_panic(expected = "Error(Contract, #1236)")]
    fn constructor_rejects_mismatched_valuation_adapter_binding() {
        let env = Env::default();
        let emitter = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = Address::generate(&env);
        let next_token = Address::generate(&env);
        let prefund_token = Address::generate(&env);
        let candidate = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate,));
        let valuation_adapter = register_valuation_adapter(
            &env,
            &Address::generate(&env),
            &legacy_token,
            &next_token,
            &prefund_token,
        );

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
    fn pool_local_exchange_rates_impair_active_and_queued_shares_proportionally() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let incumbent = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let blnd_token = Address::generate(&env);
        let legacy_token = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let next_token = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let usdc_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let candidate_id = Address::generate(&env);
        let pool_factory_id = env.register(MockPoolFactory, (&candidate_id,));
        let pool_factory = MockPoolFactoryClient::new(&env, &pool_factory_id);
        let pool = env.register(MockLossAwarePool, (&candidate_id,));
        let other_pool = env.register(MockLossAwarePool, (&candidate_id,));
        pool_factory.set_pool(&pool);
        pool_factory.set_pool(&other_pool);
        let emitter_id = env.register(MockEmitter, (&incumbent, &candidate_id, &next_token));
        let valuation_adapter =
            register_valuation_adapter(&env, &blnd_token, &legacy_token, &next_token, &usdc_token);
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
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        let recipient = Address::generate(&env);
        let tiers = [
            (BackstopTier::BlndUsdc, legacy_token.clone()),
            (BackstopTier::BlndXlm, next_token.clone()),
            (BackstopTier::Usdc, usdc_token.clone()),
        ];
        for (_, token) in &tiers {
            StellarAssetClient::new(&env, token).mint(&first, &100);
            StellarAssetClient::new(&env, token).mint(&second, &11);
        }
        StellarAssetClient::new(&env, &usdc_token).mint(&first, &20);

        candidate.deposit_blnd_usdc(&first, &pool, &100);
        candidate.deposit_blnd_xlm(&first, &pool, &100);
        candidate.deposit_usdc(&first, &pool, &100);
        candidate.deposit_usdc(&first, &other_pool, &20);
        let blnd_usdc_entry = candidate.queue_blnd_usdc_withdrawal(&first, &pool, &40);
        let blnd_xlm_entry = candidate.queue_blnd_xlm_withdrawal(&first, &pool, &40);
        let usdc_entry = candidate.queue_usdc_withdrawal(&first, &pool, &40);
        assert_eq!(
            candidate.pool_ongoing_emissions(&pool),
            PoolOngoingEmissions {
                accrued_backstop: 0,
                accrued_pool: 0,
                active_blnd_usdc: 60,
                active_blnd_usdc_shares: 60,
                active_blnd_xlm: 60,
                active_blnd_xlm_shares: 60,
                backstop_tier_carry: 0,
                blnd_usdc_index: 0,
                blnd_usdc_index_carry: 0,
                blnd_xlm_index: 0,
                blnd_xlm_index_carry: 0,
            }
        );

        env.as_contract(&candidate_id, || {
            for (tier, _) in &tiers {
                apply_pool_tier_loss(&env, *tier, &pool, 60);
            }
        });
        assert_eq!(
            candidate.pool_ongoing_emissions(&pool),
            PoolOngoingEmissions {
                accrued_backstop: 0,
                accrued_pool: 0,
                active_blnd_usdc: 24,
                active_blnd_usdc_shares: 60,
                active_blnd_xlm: 24,
                active_blnd_xlm_shares: 60,
                backstop_tier_carry: 0,
                blnd_usdc_index: 0,
                blnd_usdc_index_carry: 0,
                blnd_xlm_index: 0,
                blnd_xlm_index_carry: 0,
            }
        );
        for (tier, _) in &tiers {
            assert_eq!(
                candidate.pool_tier_state(tier, &pool),
                PoolTierState {
                    assets: 40,
                    queued_shares: 40,
                    shares: 100,
                }
            );
            assert_eq!(candidate.preview_tier_deposit(tier, &pool, &10), 25);
            assert_eq!(candidate.preview_tier_withdrawal(tier, &pool, &40), 16);
        }
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &other_pool),
            PoolTierState {
                assets: 20,
                queued_shares: 0,
                shares: 20,
            }
        );
        assert_eq!(
            candidate.preview_tier_deposit(&BackstopTier::Usdc, &other_pool, &10),
            10
        );

        candidate.deposit_blnd_usdc(&second, &pool, &10);
        candidate.deposit_blnd_xlm(&second, &pool, &10);
        candidate.deposit_usdc(&second, &pool, &10);
        assert_eq!(
            candidate.pool_ongoing_emissions(&pool),
            PoolOngoingEmissions {
                accrued_backstop: 0,
                accrued_pool: 0,
                active_blnd_usdc: 34,
                active_blnd_usdc_shares: 85,
                active_blnd_xlm: 34,
                active_blnd_xlm_shares: 85,
                backstop_tier_carry: 0,
                blnd_usdc_index: 0,
                blnd_usdc_index_carry: 0,
                blnd_xlm_index: 0,
                blnd_xlm_index_carry: 0,
            }
        );
        assert_eq!(
            candidate.migration_position(&second, &pool),
            MigrationPosition {
                amount: 25,
                backfill_amount: 10,
                backfill_claimed: false,
                frozen_weight: None,
                time_basis: 10_000,
            }
        );
        assert_eq!(
            candidate.tier_shares(&BackstopTier::BlndXlm, &second, &pool),
            25
        );
        assert_eq!(
            candidate.tier_shares(&BackstopTier::Usdc, &second, &pool),
            25
        );

        let valuation = candidate.pool_valuation(&pool);
        assert_eq!(
            valuation.active_values,
            ActivationValues {
                blnd_usdc: 34,
                blnd_xlm: 34,
                usdc: 34,
            }
        );
        assert_eq!(
            valuation.queued_values,
            ActivationValues {
                blnd_usdc: 16,
                blnd_xlm: 16,
                usdc: 16,
            }
        );
        assert_eq!(
            valuation.active_blnd,
            BlndEmissionValues {
                blnd_usdc: 34,
                blnd_xlm: 34,
            }
        );

        env.ledger().set_timestamp(
            blnd_usdc_entry
                .expires_at
                .max(blnd_xlm_entry.expires_at)
                .max(usdc_entry.expires_at),
        );
        candidate.withdraw_blnd_usdc(&first, &pool, &40, &recipient);
        candidate.withdraw_blnd_xlm(&first, &pool, &40, &recipient);
        candidate.withdraw_usdc(&first, &pool, &40, &recipient);
        for (tier, token) in &tiers {
            assert_eq!(
                candidate.pool_tier_state(tier, &pool),
                PoolTierState {
                    assets: 34,
                    queued_shares: 0,
                    shares: 85,
                }
            );
            let expected_total_assets = match tier {
                BackstopTier::Usdc => 54,
                _ => 34,
            };
            assert_eq!(candidate.total_tier_assets(tier), expected_total_assets);
            assert_eq!(TokenClient::new(&env, token).balance(&recipient), 16);
        }
        assert_eq!(
            candidate.pool_ongoing_emissions(&pool),
            PoolOngoingEmissions {
                accrued_backstop: 0,
                accrued_pool: 0,
                active_blnd_usdc: 34,
                active_blnd_usdc_shares: 85,
                active_blnd_xlm: 34,
                active_blnd_xlm_shares: 85,
                backstop_tier_carry: 0,
                blnd_usdc_index: 0,
                blnd_usdc_index_carry: 0,
                blnd_xlm_index: 0,
                blnd_xlm_index_carry: 0,
            }
        );
        assert_eq!(
            candidate.migration_position(&first, &pool),
            MigrationPosition {
                amount: 60,
                backfill_amount: 60,
                backfill_claimed: false,
                frozen_weight: None,
                time_basis: 60_000,
            }
        );
        env.as_contract(&candidate_id, || {
            assert_eq!(
                env.storage()
                    .instance()
                    .get::<DataKey, i128>(&DataKey::TotalMigrationAmount),
                Some(70)
            );
        });

        env.as_contract(&candidate_id, || {
            apply_pool_tier_loss(&env, BackstopTier::Usdc, &pool, 34);
        });
        assert!(candidate.try_deposit_usdc(&second, &pool, &1).is_err());
        let drained_entry = candidate.queue_usdc_withdrawal(&second, &pool, &25);
        env.ledger().set_timestamp(drained_entry.expires_at);
        candidate.withdraw_usdc(&second, &pool, &25, &recipient);
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &pool),
            PoolTierState {
                assets: 0,
                queued_shares: 0,
                shares: 60,
            }
        );
        assert_eq!(TokenClient::new(&env, &usdc_token).balance(&recipient), 16);

        let final_entry = candidate.queue_usdc_withdrawal(&first, &pool, &60);
        env.ledger().set_timestamp(final_entry.expires_at);
        candidate.withdraw_usdc(&first, &pool, &60, &recipient);
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &pool),
            PoolTierState {
                assets: 0,
                queued_shares: 0,
                shares: 0,
            }
        );
        candidate.deposit_usdc(&second, &pool, &1);
        assert_eq!(
            candidate.pool_tier_state(&BackstopTier::Usdc, &pool),
            PoolTierState {
                assets: 1,
                queued_shares: 0,
                shares: 1,
            }
        );
    }

    #[test]
    fn canonical_pool_valuation_uses_active_and_queued_tier_assets() {
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
        let valuation_adapter =
            register_valuation_adapter(&env, &blnd_token, &legacy_token, &next_token, &usdc_token);
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
            set_pool_tier_assets(&env, BackstopTier::BlndUsdc, &pool_id, 5_000 * SCALAR_7);
            add_pool_blnd_usdc_queued_shares(&env, &pool_id, 1_000 * SCALAR_7);
            set_pool_tier_shares(&env, BackstopTier::BlndXlm, &pool_id, 5_000 * SCALAR_7);
            set_pool_tier_assets(&env, BackstopTier::BlndXlm, &pool_id, 5_000 * SCALAR_7);
            add_pool_tier_queued_shares(&env, BackstopTier::BlndXlm, &pool_id, 1_000 * SCALAR_7);
            set_pool_tier_shares(&env, BackstopTier::Usdc, &pool_id, 5_500 * SCALAR_7);
            set_pool_tier_assets(&env, BackstopTier::Usdc, &pool_id, 5_500 * SCALAR_7);
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
        let valuation_adapter =
            register_valuation_adapter(&env, &blnd_token, &legacy_token, &next_token, &usdc_token);
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
            set_pool_tier_assets(&env, BackstopTier::BlndUsdc, &pool_id, SCALAR_7);
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
    fn pool_status_policy_bounds_pool_admin_overrides() {
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
    fn interest_bid_amount_rounds_up_to_the_120_percent_target() {
        let env = Env::default();
        let lot_value = 200 * SCALAR_7;
        let unit_value = 2 * SCALAR_7;

        assert_eq!(
            calculate_interest_bid_amount(&env, lot_value, unit_value),
            120 * SCALAR_7
        );
        assert_eq!(
            calculate_interest_bid_amount(&env, lot_value + 1, unit_value),
            120 * SCALAR_7 + 1
        );
    }

    #[test]
    fn ongoing_blnd_split_is_immutable_and_conserves_top_level_carry() {
        let env = Env::default();
        let first = quote_ongoing_blnd_split_values(&env, 11, 0);
        assert_eq!(
            first,
            OngoingBlndSplit {
                backstop: 7,
                carry: 1,
                pool: 3,
                total: 11,
            }
        );

        let second = quote_ongoing_blnd_split_values(&env, 9, first.carry);
        assert_eq!(
            second,
            OngoingBlndSplit {
                backstop: 7,
                carry: 0,
                pool: 3,
                total: 10,
            }
        );
        assert_eq!(
            checked_add_i128(
                &env,
                checked_add_i128(&env, second.backstop, second.pool),
                second.carry,
            ),
            second.total
        );

        let negative = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_ongoing_blnd_split_values(&env, -1, 0)
        }));
        assert!(negative.is_err());
        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quote_ongoing_blnd_split_values(&env, i128::MAX, 1)
        }));
        assert!(overflow.is_err());
    }

    #[test]
    fn ongoing_blnd_spot_values_use_current_comet_composition() {
        let env = Env::default();
        let incumbent = Address::generate(&env);
        let blnd = Address::generate(&env);
        let blnd_usdc = env.register(MockComet, (&blnd, &(1_000 * SCALAR_7), &(8_000 * SCALAR_7)));
        let blnd_xlm = env.register(MockComet, (&blnd, &(500 * SCALAR_7), &(1_500 * SCALAR_7)));
        let usdc = Address::generate(&env);
        let candidate_id = Address::generate(&env);
        let pool_factory = env.register(MockPoolFactory, (&candidate_id,));
        let emitter = env.register(MockEmitter, (&incumbent, &candidate_id, &blnd_xlm));
        let valuation_adapter =
            register_valuation_adapter(&env, &blnd, &blnd_usdc, &blnd_xlm, &usdc);
        env.register_at(
            &candidate_id,
            BackstopContract,
            (
                &emitter,
                &blnd,
                &pool_factory,
                &blnd_usdc,
                &blnd_xlm,
                &usdc,
                &valuation_adapter,
            ),
        );
        let candidate = BackstopContractClient::new(&env, &candidate_id);

        assert_eq!(
            candidate.spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7),),
            BlndEmissionValues {
                blnd_usdc: 800 * SCALAR_7,
                blnd_xlm: 600 * SCALAR_7,
            }
        );

        MockCometClient::new(&env, &blnd_usdc)
            .set_composition(&(1_000 * SCALAR_7), &(4_000 * SCALAR_7));
        assert_eq!(
            candidate.spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7),),
            BlndEmissionValues {
                blnd_usdc: 400 * SCALAR_7,
                blnd_xlm: 600 * SCALAR_7,
            }
        );

        MockCometClient::new(&env, &blnd_xlm).set_composition(&0, &(1_500 * SCALAR_7));
        assert!(candidate
            .try_spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7))
            .is_err());
        MockCometClient::new(&env, &blnd_xlm)
            .set_composition(&(500 * SCALAR_7), &(1_500 * SCALAR_7));
        assert!(candidate
            .try_spot_blnd_emission_values(&(1_001 * SCALAR_7), &(200 * SCALAR_7))
            .is_err());
        MockCometClient::new(&env, &blnd_usdc).set_composition(&(1_000 * SCALAR_7), &-1);
        assert!(candidate
            .try_spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7))
            .is_err());
        MockCometClient::new(&env, &blnd_usdc).set_composition(&-1, &(4_000 * SCALAR_7));
        assert!(candidate
            .try_spot_blnd_emission_values(&(100 * SCALAR_7), &(200 * SCALAR_7))
            .is_err());
        assert!(candidate
            .try_spot_blnd_emission_values(&-1, &(200 * SCALAR_7))
            .is_err());
    }

    #[test]
    fn blnd_emissions_allocate_reward_zone_tranche_by_underlying_blnd() {
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
