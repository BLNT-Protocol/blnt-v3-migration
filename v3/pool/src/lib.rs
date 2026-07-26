#![no_std]

mod auction;
mod lending;

pub use auction::{
    AuctionCreated, AuctionData, AuctionDeleted, AuctionFilled, BackstopTier,
    BadDebtAuctionCreated, BadDebtAuctionData, BadDebtAuctionDeleted, BadDebtAuctionFill,
    BadDebtAuctionFilled, BadDebtContinuation, BadDebtLotQuote, InterestAuctionCreated,
    InterestAuctionData, InterestAuctionDeleted, InterestAuctionFill, InterestAuctionFilled,
    InterestLotQuote, InterestReserveState, SupplierDebtDefaulted, TakeRateQuote,
    UserBadDebtTransferred, ACTION_DELETE_LIQUIDATION, ACTION_FILL_USER_LIQUIDATION,
};
pub use lending::{
    LendingAction, LendingConfig, PositionHealth, Positions, PriceQuote, Request, Reserve,
    ReserveConfig, ReserveConfigUpdate, ReserveData, ACTION_BORROW, ACTION_REPAY, ACTION_SUPPLY,
    ACTION_SUPPLY_COLLATERAL, ACTION_WITHDRAW, ACTION_WITHDRAW_COLLATERAL,
};

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, Address, BytesN, Env, Map, Vec,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const ADMIN_PROPOSAL_TTL: u32 = 10 * DAY_IN_LEDGERS;
const MAX_LOSS_RECORDS_PER_KIND: u32 = 30;
const STATUS_SETUP: u32 = 6;
const MAX_ACTION_TYPE: u32 = ACTION_DELETE_LIQUIDATION;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Backstop,
    LossRecords,
    ProposedAdmin,
    Status,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PoolError {
    InvalidLossAmount = 1300,
    TooManyLossRecords = 1301,
    StatusTransitionNotAllowed = 1302,
    InvalidPoolStatus = 1303,
    InvalidAction = 1304,
    InvalidLendingConfig = 1305,
    InvalidReserveConfig = 1306,
    TooManyReserves = 1307,
    UnknownReserve = 1308,
    ReserveDisabled = 1309,
    SupplyCapExceeded = 1310,
    InvalidUtilization = 1311,
    TooManyPositions = 1312,
    InvalidHealthFactor = 1313,
    MinimumCollateralNotMet = 1314,
    InvalidPrice = 1315,
    ArithmeticError = 1316,
    BadRequest = 1317,
    InvalidTokenBalance = 1318,
    InvalidLiquidation = 1319,
    AuctionInProgress = 1320,
    InvalidAuctionBid = 1321,
    InvalidAuctionLot = 1322,
    InvalidLiquidationSize = 1323,
    AuctionNotFound = 1324,
    AuctionNotStarted = 1325,
    AuctionNotStale = 1326,
    BadDebtUnsupported = 1327,
    InvalidInterestAuction = 1328,
    NoInterestAuctionCapacity = 1329,
}

/// Canonical status result returned by the configured backstop.
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

#[contractclient(name = "BackstopClient")]
pub trait Backstop {
    fn quote_pool_status_update(env: Env, pool: Address, current_status: u32) -> PoolStatusQuote;
    fn quote_pool_status_set(
        env: Env,
        pool: Address,
        current_status: u32,
        requested_status: u32,
    ) -> PoolStatusQuote;
    fn quote_bad_debt_lot(env: Env, pool: Address, debt_value: i128) -> Option<BadDebtLotQuote>;
    fn commit_bad_debt_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        debt_value: i128,
    ) -> BadDebtLotQuote;
    fn release_bad_debt_lot(env: Env, pool: Address, auction_id: BytesN<32>);
    fn settle_bad_debt_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        base_lot_amount: i128,
        lot_amount: i128,
        to: Address,
    ) -> Option<BadDebtLotQuote>;
    fn quote_pool_take_rate_batch(
        env: Env,
        pool: Address,
        distributions: Map<Address, i128>,
    ) -> Map<Address, TakeRateQuote>;
    fn commit_interest_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        tier: BackstopTier,
        lot_value: i128,
    ) -> InterestLotQuote;
    fn release_interest_lot(env: Env, pool: Address, auction_id: BytesN<32>);
    fn settle_interest_lot(
        env: Env,
        pool: Address,
        auction_id: BytesN<32>,
        base_bid_amount: i128,
        bid_amount: i128,
        from: Address,
    ) -> Option<InterestLotQuote>;
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStatusChanged {
    pub admin_action: bool,
    pub previous_status: u32,
    pub status: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolAdminChanged {
    pub previous_admin: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolConfigUpdated {
    pub admin: Address,
    pub backstop_take_rate: u32,
    pub max_positions: u32,
    pub min_collateral: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveConfigQueued {
    pub admin: Address,
    pub asset: Address,
    pub update: ReserveConfigUpdate,
    pub unlock_time: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveConfigCancelled {
    pub admin: Address,
    pub asset: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveConfigured {
    pub asset: Address,
    pub index: u32,
}

/// Canonical nonzero records that can prevent backstop withdrawals.
///
/// Values remain in each reserve, auction, or bad-debt record's own units.
/// They are never summed across maps.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
struct BackstopLossRecords {
    committed_losses: Map<BytesN<32>, bool>,
    liabilities: Map<Address, i128>,
    unresolved_bad_debt: Map<Address, i128>,
}

/// Canonical aggregate state that can prevent backstop withdrawals.
///
/// Each field is derived from canonical nonzero records rather than cached or
/// calculated by combining amounts from reserves with different units.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BackstopLossState {
    pub committed_loss_entries: u32,
    pub liability_entries: u32,
    pub unresolved_bad_debt_entries: u32,
}

impl BackstopLossState {
    fn is_clear(&self) -> bool {
        self.committed_loss_entries == 0
            && self.liability_entries == 0
            && self.unresolved_bad_debt_entries == 0
    }
}

#[contract]
pub struct PoolContract;

#[contractimpl]
impl PoolContract {
    pub fn __constructor(
        env: Env,
        backstop: Address,
        admin: Address,
        lending_config: LendingConfig,
        reserves: Vec<ReserveConfig>,
    ) {
        extend_instance_ttl(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Backstop, &backstop);
        env.storage()
            .instance()
            .set(&DataKey::Status, &STATUS_SETUP);
        env.storage()
            .instance()
            .set(&DataKey::LossRecords, &empty_loss_records(&env));
        lending::initialize(&env, &lending_config, &reserves);
    }

    pub fn backstop(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Backstop).unwrap()
    }

    /// Return this pool's current transferable v2-style administrator.
    pub fn admin(env: Env) -> Address {
        extend_instance_ttl(&env);
        get_admin(&env)
    }

    /// Propose a replacement administrator.
    ///
    /// The current administrator must authorize the proposal. The proposed
    /// administrator does not gain authority until it accepts.
    pub fn propose_admin(env: Env, new_admin: Address) {
        extend_instance_ttl(&env);
        get_admin(&env).require_auth();
        env.storage()
            .temporary()
            .set(&DataKey::ProposedAdmin, &new_admin);
        env.storage().temporary().extend_ttl(
            &DataKey::ProposedAdmin,
            ADMIN_PROPOSAL_TTL,
            ADMIN_PROPOSAL_TTL,
        );
    }

    /// Accept a pending administrator transfer.
    pub fn accept_admin(env: Env) {
        extend_instance_ttl(&env);
        let proposed_admin: Address = env
            .storage()
            .temporary()
            .get(&DataKey::ProposedAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, PoolError::BadRequest));
        proposed_admin.require_auth();
        let previous_admin = get_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::Admin, &proposed_admin);
        env.storage().temporary().remove(&DataKey::ProposedAdmin);
        PoolAdminChanged {
            previous_admin,
            admin: proposed_admin,
        }
        .publish(&env);
    }

    /// Return the stored v2-compatible pool status code.
    pub fn status(env: Env) -> u32 {
        extend_instance_ttl(&env);
        get_status(&env)
    }

    /// Refresh stored status permissionlessly from the backstop's canonical
    /// valuation and Q4W calculation.
    pub fn update_pool_status(env: Env) -> u32 {
        extend_instance_ttl(&env);
        let current_status = get_status(&env);
        let backstop = get_backstop(&env);
        let quote = BackstopClient::new(&env, &backstop)
            .quote_pool_status_update(&env.current_contract_address(), &current_status);
        apply_status_quote(&env, current_status, &quote, false)
    }

    /// Request one of the bounded v2-style pool-admin status overrides.
    pub fn set_pool_status(env: Env, requested_status: u32) -> u32 {
        extend_instance_ttl(&env);
        get_admin(&env).require_auth();
        let current_status = get_status(&env);
        let backstop = get_backstop(&env);
        let quote = BackstopClient::new(&env, &backstop).quote_pool_status_set(
            &env.current_contract_address(),
            &current_status,
            &requested_status,
        );
        apply_status_quote(&env, current_status, &quote, true)
    }

    /// Return whether the stored status permits one v2 request type.
    ///
    /// Every request submitted through `submit`, including user-liquidation
    /// fill and cancellation, uses this predicate. Direct interest- and
    /// bad-debt-auction operations follow their separate all-status rules.
    pub fn action_allowed(env: Env, action_type: u32) -> bool {
        extend_instance_ttl(&env);
        require_valid_action(&env, action_type);
        action_allowed_for_status(get_status(&env), action_type)
    }

    /// Return pool-level lending configuration.
    pub fn lending_config(env: Env) -> LendingConfig {
        extend_instance_ttl(&env);
        lending::config(&env)
    }

    /// Update the v2-compatible mutable pool parameters.
    pub fn update_pool(
        env: Env,
        backstop_take_rate: u32,
        max_positions: u32,
        min_collateral: i128,
    ) -> LendingConfig {
        extend_instance_ttl(&env);
        let admin = get_admin(&env);
        admin.require_auth();
        let config =
            lending::update_config(&env, backstop_take_rate, max_positions, min_collateral);
        PoolConfigUpdated {
            admin,
            backstop_take_rate,
            max_positions,
            min_collateral,
        }
        .publish(&env);
        config
    }

    /// Queue mutable metadata for a new or existing reserve.
    ///
    /// Setup pools may apply it immediately. Other statuses retain v2's
    /// seven-day timelock. A new reserve receives its immutable index only
    /// when this queue is applied.
    pub fn queue_set_reserve(env: Env, asset: Address, update: ReserveConfigUpdate) -> u64 {
        extend_instance_ttl(&env);
        let admin = get_admin(&env);
        admin.require_auth();
        let unlock_time = lending::queue_set_reserve(&env, &asset, &update);
        ReserveConfigQueued {
            admin,
            asset,
            update,
            unlock_time,
        }
        .publish(&env);
        unlock_time
    }

    /// Cancel one pending reserve configuration.
    pub fn cancel_set_reserve(env: Env, asset: Address) {
        extend_instance_ttl(&env);
        let admin = get_admin(&env);
        admin.require_auth();
        lending::cancel_set_reserve(&env, &asset);
        ReserveConfigCancelled { admin, asset }.publish(&env);
    }

    /// Permissionlessly apply an unlocked reserve configuration.
    pub fn set_reserve(env: Env, asset: Address) -> u32 {
        extend_instance_ttl(&env);
        let index = lending::set_reserve(&env, &asset);
        ReserveConfigured { asset, index }.publish(&env);
        index
    }

    /// Return the reserve list in reserve-index order.
    pub fn reserve_list(env: Env) -> Vec<Address> {
        extend_instance_ttl(&env);
        lending::reserve_list(&env)
    }

    /// Return one reserve with interest accrued through the current timestamp.
    pub fn reserve(env: Env, asset: Address) -> Reserve {
        extend_instance_ttl(&env);
        lending::reserve(&env, &asset)
    }

    /// Return one account's stored supply, collateral, and liability shares.
    pub fn positions(env: Env, user: Address) -> Positions {
        extend_instance_ttl(&env);
        lending::positions(&env, &user)
    }

    /// Return one account's current oracle-valued health result.
    pub fn position_health(env: Env, user: Address) -> PositionHealth {
        extend_instance_ttl(&env);
        lending::health(&env, &user)
    }

    /// Execute an atomic v2-compatible request set, including user-liquidation
    /// fill and cancellation actions.
    pub fn submit(
        env: Env,
        from: Address,
        spender: Address,
        to: Address,
        requests: Vec<Request>,
    ) -> Positions {
        extend_instance_ttl(&env);
        lending::submit(&env, &from, &spender, &to, requests)
    }

    /// Create a permissionless v2-compatible user-liquidation auction.
    pub fn new_liquidation_auction(
        env: Env,
        user: Address,
        bid: Vec<Address>,
        lot: Vec<Address>,
        percent: u32,
    ) -> AuctionData {
        extend_instance_ttl(&env);
        auction::create_user_liquidation(&env, &user, &bid, &lot, percent)
    }

    /// Return one active user-liquidation auction.
    pub fn get_liquidation_auction(env: Env, user: Address) -> AuctionData {
        extend_instance_ttl(&env);
        auction::get_user_liquidation(&env, &user)
    }

    /// Permissionlessly delete a user-liquidation auction after its stale
    /// boundary.
    pub fn delete_stale_liquidation_auction(env: Env, user: Address) {
        extend_instance_ttl(&env);
        auction::delete_stale_user_liquidation(&env, &user);
    }

    /// Prepare a v2-shaped bad-debt auction from liabilities already assigned
    /// to this pool's configured backstop.
    ///
    /// This reserves one strict single-tier lot for atomic filling.
    pub fn new_bad_debt_auction(
        env: Env,
        auction_id: BytesN<32>,
        bid: Vec<Address>,
    ) -> BadDebtAuctionData {
        extend_instance_ttl(&env);
        auction::create_bad_debt_auction(&env, &auction_id, &bid)
    }

    /// Return the single active bad-debt auction prepared for this pool.
    pub fn get_bad_debt_auction(env: Env) -> BadDebtAuctionData {
        extend_instance_ttl(&env);
        auction::get_bad_debt_auction(&env)
    }

    /// Fill part or all of the prepared bad-debt auction.
    ///
    /// The authenticated filler assumes the time-scaled dToken bid and
    /// receives the time-scaled committed tier token directly.
    pub fn fill_bad_debt_auction(env: Env, filler: Address, percent: u32) -> BadDebtAuctionFill {
        extend_instance_ttl(&env);
        auction::fill_bad_debt_auction(&env, &filler, percent)
    }

    /// Permissionlessly continue resolution after the current bad-debt auction
    /// has completed or been deleted.
    ///
    /// The first eligible tier in strict waterfall order is committed to a
    /// new auction. Only when the configured backstop verifies that every tier
    /// is below its minimum does the pool default all residual liabilities to
    /// suppliers through the affected reserves' bToken exchange rates.
    pub fn continue_bad_debt_resolution(env: Env, auction_id: BytesN<32>) -> BadDebtContinuation {
        extend_instance_ttl(&env);
        auction::continue_bad_debt_resolution(&env, &auction_id)
    }

    /// Release a prepared bad-debt lot after the inherited stale boundary.
    pub fn delete_stale_bad_debt_auction(env: Env) {
        extend_instance_ttl(&env);
        auction::delete_stale_bad_debt_auction(&env);
    }

    /// Checkpoint a bounded reserve-credit batch and create one tier-specific
    /// interest auction from the next qualifying cyclic tier.
    pub fn new_interest_auction(
        env: Env,
        auction_id: BytesN<32>,
        lot_assets: Vec<Address>,
    ) -> InterestAuctionData {
        extend_instance_ttl(&env);
        auction::create_interest_auction(&env, &auction_id, &lot_assets)
    }

    /// Return this pool's single active tier-specific interest auction.
    pub fn get_interest_auction(env: Env) -> InterestAuctionData {
        extend_instance_ttl(&env);
        auction::get_interest_auction(&env)
    }

    /// Fill part or all of the active tier-specific interest auction.
    pub fn fill_interest_auction(env: Env, filler: Address, percent: u32) -> InterestAuctionFill {
        extend_instance_ttl(&env);
        auction::fill_interest_auction(&env, &filler, percent)
    }

    /// Return one reserve's pending tier-specific interest-credit state.
    pub fn interest_reserve_state(env: Env, asset: Address) -> InterestReserveState {
        extend_instance_ttl(&env);
        auction::interest_reserve_state(&env, &asset)
    }

    /// Permissionlessly release an interest auction after 500 ledgers.
    pub fn delete_stale_interest_auction(env: Env) {
        extend_instance_ttl(&env);
        auction::delete_stale_interest_auction(&env);
    }

    /// Return counts derived from the canonical nonzero loss records.
    pub fn backstop_loss_state(env: Env) -> BackstopLossState {
        extend_instance_ttl(&env);
        get_loss_state(&env)
    }

    /// Return true only for this pool's configured backstop and only when no
    /// backstop liability, committed loss, or unresolved bad debt exists.
    pub fn backstop_withdrawal_allowed(env: Env, backstop: Address) -> bool {
        extend_instance_ttl(&env);
        let configured_backstop: Address =
            env.storage().instance().get(&DataKey::Backstop).unwrap();
        backstop == configured_backstop && get_loss_state(&env).is_clear()
    }
}

fn get_backstop(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Backstop).unwrap()
}

fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn get_status(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Status).unwrap()
}

fn apply_status_quote(
    env: &Env,
    previous_status: u32,
    quote: &PoolStatusQuote,
    admin_action: bool,
) -> u32 {
    if !quote.transition_allowed {
        panic_with_error!(env, PoolError::StatusTransitionNotAllowed);
    }
    require_valid_status(env, quote.status);
    env.storage()
        .instance()
        .set(&DataKey::Status, &quote.status);
    PoolStatusChanged {
        admin_action,
        previous_status,
        status: quote.status,
    }
    .publish(env);
    quote.status
}

fn require_valid_status(env: &Env, status: u32) {
    if status > STATUS_SETUP {
        panic_with_error!(env, PoolError::InvalidPoolStatus);
    }
}

fn require_valid_action(env: &Env, action_type: u32) {
    if action_type > MAX_ACTION_TYPE {
        panic_with_error!(env, PoolError::InvalidAction);
    }
}

fn action_allowed_for_status(status: u32, action_type: u32) -> bool {
    !((status > 1 && (action_type == ACTION_BORROW || action_type == ACTION_DELETE_LIQUIDATION))
        || (status > 3
            && (action_type == ACTION_SUPPLY || action_type == ACTION_SUPPLY_COLLATERAL)))
}

fn require_action_allowed(env: &Env, status: u32, action_type: u32) {
    require_valid_action(env, action_type);
    if !action_allowed_for_status(status, action_type) {
        panic_with_error!(env, PoolError::InvalidPoolStatus);
    }
}

fn empty_loss_records(env: &Env) -> BackstopLossRecords {
    BackstopLossRecords {
        committed_losses: Map::new(env),
        liabilities: Map::new(env),
        unresolved_bad_debt: Map::new(env),
    }
}

fn get_loss_records(env: &Env) -> BackstopLossRecords {
    env.storage().instance().get(&DataKey::LossRecords).unwrap()
}

fn get_loss_state(env: &Env) -> BackstopLossState {
    let records = get_loss_records(env);
    let committed_loss_entries = records
        .committed_losses
        .len()
        .checked_add(u32::from(auction::has_bad_debt_auction(env)))
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    BackstopLossState {
        committed_loss_entries,
        liability_entries: records.liabilities.len(),
        unresolved_bad_debt_entries: records.unresolved_bad_debt.len(),
    }
}

fn set_loss_records(env: &Env, records: &BackstopLossRecords) {
    env.storage().instance().set(&DataKey::LossRecords, records);
}

/// Internal pool-accounting boundaries for canonical loss records.
///
/// These are intentionally not contract entry points. Future lending, auction,
/// and waterfall modules must call them as part of their own atomic
/// transitions; no administrator or external caller can override withdrawal
/// eligibility.
#[allow(dead_code)]
fn set_backstop_liability(env: &Env, reserve: &Address, amount: i128) {
    let mut records = get_loss_records(env);
    set_nonzero_address_record(env, &mut records.liabilities, reserve, amount);
    set_loss_records(env, &records);
}

fn set_backstop_liabilities(env: &Env, liabilities: &Map<Address, i128>) {
    let mut records = get_loss_records(env);
    for (reserve, amount) in liabilities.iter() {
        set_nonzero_address_record(env, &mut records.liabilities, &reserve, amount);
    }
    set_loss_records(env, &records);
}

fn get_backstop_liability(env: &Env, reserve: &Address) -> i128 {
    get_loss_records(env)
        .liabilities
        .get(reserve.clone())
        .unwrap_or(0)
}

fn get_backstop_liabilities(env: &Env) -> Map<Address, i128> {
    get_loss_records(env).liabilities
}

fn clear_backstop_liabilities(env: &Env) {
    let mut records = get_loss_records(env);
    records.liabilities = Map::new(env);
    set_loss_records(env, &records);
}

#[allow(dead_code)]
fn set_unresolved_bad_debt(env: &Env, reserve: &Address, amount: i128) {
    let mut records = get_loss_records(env);
    set_nonzero_address_record(env, &mut records.unresolved_bad_debt, reserve, amount);
    set_loss_records(env, &records);
}

#[allow(dead_code)]
fn set_committed_loss(env: &Env, auction: &BytesN<32>, committed: bool) {
    let mut records = get_loss_records(env);
    if committed {
        require_record_capacity(
            env,
            records.committed_losses.len(),
            records.committed_losses.contains_key(auction.clone()),
        );
        records.committed_losses.set(auction.clone(), true);
    } else {
        records.committed_losses.remove(auction.clone());
    }
    set_loss_records(env, &records);
}

fn set_nonzero_address_record(
    env: &Env,
    records: &mut Map<Address, i128>,
    key: &Address,
    amount: i128,
) {
    require_valid_loss_amount(env, amount);
    if amount == 0 {
        records.remove(key.clone());
    } else {
        require_record_capacity(env, records.len(), records.contains_key(key.clone()));
        records.set(key.clone(), amount);
    }
}

fn require_valid_loss_amount(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, PoolError::InvalidLossAmount);
    }
}

fn require_record_capacity(env: &Env, len: u32, exists: bool) {
    if !exists && len >= MAX_LOSS_RECORDS_PER_KIND {
        panic_with_error!(env, PoolError::TooManyLossRecords);
    }
}

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

#[cfg(test)]
#[contractimpl]
impl PoolContract {
    pub fn test_loss_transition(env: Env, reserve: Address, auction: BytesN<32>, fail: bool) {
        set_backstop_liability(&env, &reserve, 100);
        set_committed_loss(&env, &auction, true);
        assert!(!fail, "forced loss-transition failure");
    }

    pub fn test_require_action_allowed(env: Env, action_type: u32) {
        require_action_allowed(&env, get_status(&env), action_type);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
        token::{StellarAssetClient, TokenClient},
        vec, Event, IntoVal,
    };

    #[contract]
    struct MockBackstop;

    #[contract]
    struct MockPriceOracle;

    #[contract]
    struct MockFeeToken;

    #[derive(Clone)]
    #[contracttype]
    enum MockBackstopKey {
        BadDebtQuote,
        CommittedAuction,
        CommittedDebt,
        InterestAuction,
        InterestBid,
        InterestBidToken,
        InterestLotValue,
        InterestReleased,
        InterestTarget,
        InterestTier,
        InterestWeighted,
        ReleasedAuction,
        SettledBaseLot,
        SettledCount,
        SettledLot,
        SettledTo,
        SetQuote,
        UpdateQuote,
    }

    #[derive(Clone)]
    #[contracttype]
    enum MockOracleKey {
        Calls(Address),
        Price(Address),
    }

    #[derive(Clone)]
    #[contracttype]
    enum MockFeeTokenKey {
        Balance(Address),
    }

    #[contractimpl]
    impl MockBackstop {
        pub fn set_update_quote(env: Env, quote: PoolStatusQuote) {
            env.storage()
                .instance()
                .set(&MockBackstopKey::UpdateQuote, &quote);
        }

        pub fn set_set_quote(env: Env, quote: PoolStatusQuote) {
            env.storage()
                .instance()
                .set(&MockBackstopKey::SetQuote, &quote);
        }

        pub fn quote_pool_status_update(
            env: Env,
            _pool: Address,
            _current_status: u32,
        ) -> PoolStatusQuote {
            env.storage()
                .instance()
                .get(&MockBackstopKey::UpdateQuote)
                .unwrap()
        }

        pub fn quote_pool_status_set(
            env: Env,
            _pool: Address,
            _current_status: u32,
            _requested_status: u32,
        ) -> PoolStatusQuote {
            env.storage()
                .instance()
                .get(&MockBackstopKey::SetQuote)
                .unwrap()
        }

        pub fn set_bad_debt_quote(env: Env, quote: BadDebtLotQuote) {
            env.storage()
                .instance()
                .set(&MockBackstopKey::BadDebtQuote, &quote);
        }

        pub fn clear_bad_debt_quote(env: Env) {
            env.storage()
                .instance()
                .remove(&MockBackstopKey::BadDebtQuote);
        }

        pub fn quote_bad_debt_lot(
            env: Env,
            _pool: Address,
            debt_value: i128,
        ) -> Option<BadDebtLotQuote> {
            env.storage()
                .instance()
                .get::<_, BadDebtLotQuote>(&MockBackstopKey::BadDebtQuote)
                .map(|mut quote| {
                    quote.debt_value = debt_value;
                    quote
                })
        }

        pub fn commit_bad_debt_lot(
            env: Env,
            pool: Address,
            auction_id: BytesN<32>,
            debt_value: i128,
        ) -> BadDebtLotQuote {
            pool.require_auth();
            env.storage()
                .instance()
                .set(&MockBackstopKey::CommittedAuction, &auction_id);
            env.storage()
                .instance()
                .set(&MockBackstopKey::CommittedDebt, &debt_value);
            let mut quote: BadDebtLotQuote = env
                .storage()
                .instance()
                .get(&MockBackstopKey::BadDebtQuote)
                .unwrap();
            quote.debt_value = debt_value;
            env.storage()
                .instance()
                .set(&MockBackstopKey::BadDebtQuote, &quote);
            quote
        }

        pub fn release_bad_debt_lot(env: Env, pool: Address, auction_id: BytesN<32>) {
            pool.require_auth();
            env.storage()
                .instance()
                .set(&MockBackstopKey::ReleasedAuction, &auction_id);
        }

        pub fn settle_bad_debt_lot(
            env: Env,
            pool: Address,
            auction_id: BytesN<32>,
            base_lot_amount: i128,
            lot_amount: i128,
            to: Address,
        ) -> Option<BadDebtLotQuote> {
            pool.require_auth();
            let committed_auction: BytesN<32> = env
                .storage()
                .instance()
                .get(&MockBackstopKey::CommittedAuction)
                .unwrap();
            assert_eq!(committed_auction, auction_id);
            let mut quote: BadDebtLotQuote = env
                .storage()
                .instance()
                .get(&MockBackstopKey::BadDebtQuote)
                .unwrap();
            assert!(base_lot_amount >= 0);
            assert!(lot_amount >= 0);
            assert!(lot_amount <= base_lot_amount);
            assert!(base_lot_amount <= quote.lot_amount);
            env.storage()
                .instance()
                .set(&MockBackstopKey::SettledBaseLot, &base_lot_amount);
            env.storage()
                .instance()
                .set(&MockBackstopKey::SettledLot, &lot_amount);
            env.storage()
                .instance()
                .set(&MockBackstopKey::SettledTo, &to);
            let count = env
                .storage()
                .instance()
                .get::<_, u32>(&MockBackstopKey::SettledCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&MockBackstopKey::SettledCount, &(count + 1));

            let previous_lot_amount = quote.lot_amount;
            quote.lot_amount -= base_lot_amount;
            if quote.lot_amount == 0 {
                env.storage()
                    .instance()
                    .remove(&MockBackstopKey::BadDebtQuote);
                None
            } else {
                quote.committed_value =
                    quote.committed_value * quote.lot_amount / previous_lot_amount;
                env.storage()
                    .instance()
                    .set(&MockBackstopKey::BadDebtQuote, &quote);
                Some(quote)
            }
        }

        pub fn committed_debt(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&MockBackstopKey::CommittedDebt)
                .unwrap_or(0)
        }

        pub fn committed_auction(env: Env) -> Option<BytesN<32>> {
            env.storage()
                .instance()
                .get(&MockBackstopKey::CommittedAuction)
        }

        pub fn released_auction(env: Env) -> Option<BytesN<32>> {
            env.storage()
                .instance()
                .get(&MockBackstopKey::ReleasedAuction)
        }

        pub fn settled_base_lot(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&MockBackstopKey::SettledBaseLot)
                .unwrap_or(0)
        }

        pub fn settled_count(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&MockBackstopKey::SettledCount)
                .unwrap_or(0)
        }

        pub fn settled_lot(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&MockBackstopKey::SettledLot)
                .unwrap_or(0)
        }

        pub fn settled_to(env: Env) -> Option<Address> {
            env.storage().instance().get(&MockBackstopKey::SettledTo)
        }

        pub fn set_interest_bid_token(env: Env, token: Address) {
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestBidToken, &token);
        }

        pub fn set_interest_weighted(env: Env, weighted: bool) {
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestWeighted, &weighted);
        }

        pub fn quote_pool_take_rate_batch(
            env: Env,
            _pool: Address,
            distributions: Map<Address, i128>,
        ) -> Map<Address, TakeRateQuote> {
            let weighted = env
                .storage()
                .instance()
                .get::<_, bool>(&MockBackstopKey::InterestWeighted)
                .unwrap_or(false);
            let mut quotes = Map::new(&env);
            for (asset, amount) in distributions.iter() {
                let (blnd_usdc, blnd_xlm, usdc) = if weighted {
                    (amount * 5 / 14, amount * 5 / 14, amount * 4 / 14)
                } else {
                    (amount, 0, 0)
                };
                quotes.set(
                    asset,
                    TakeRateQuote {
                        blnd_usdc,
                        blnd_xlm,
                        remainder: amount - blnd_usdc - blnd_xlm - usdc,
                        usdc,
                    },
                );
            }
            quotes
        }

        pub fn commit_interest_lot(
            env: Env,
            pool: Address,
            auction_id: BytesN<32>,
            tier: BackstopTier,
            lot_value: i128,
        ) -> InterestLotQuote {
            pool.require_auth();
            let bid_token: Address = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestBidToken)
                .unwrap();
            let bid_amount = lending::mul_div_ceil(&env, lot_value, 6, 5);
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestAuction, &auction_id);
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestBid, &bid_amount);
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestLotValue, &lot_value);
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestTarget, &bid_amount);
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestTier, &tier);
            InterestLotQuote {
                bid_token,
                bid_amount,
                lot_value,
                target_value: bid_amount,
                tier,
                valid_until: u64::MAX,
            }
        }

        pub fn release_interest_lot(env: Env, pool: Address, auction_id: BytesN<32>) {
            pool.require_auth();
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestReleased, &auction_id);
            env.storage()
                .instance()
                .remove(&MockBackstopKey::InterestAuction);
        }

        pub fn settle_interest_lot(
            env: Env,
            pool: Address,
            auction_id: BytesN<32>,
            base_bid_amount: i128,
            _bid_amount: i128,
            _from: Address,
        ) -> Option<InterestLotQuote> {
            pool.require_auth();
            let committed: BytesN<32> = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestAuction)
                .unwrap();
            assert_eq!(committed, auction_id);
            let previous_bid: i128 = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestBid)
                .unwrap();
            let remaining_bid = previous_bid - base_bid_amount;
            if remaining_bid == 0 {
                env.storage()
                    .instance()
                    .remove(&MockBackstopKey::InterestAuction);
                return None;
            }
            let previous_lot_value: i128 = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestLotValue)
                .unwrap();
            let lot_value = previous_lot_value * remaining_bid / previous_bid;
            let bid_token: Address = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestBidToken)
                .unwrap();
            let tier: BackstopTier = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestTier)
                .unwrap();
            let target_value: i128 = env
                .storage()
                .instance()
                .get(&MockBackstopKey::InterestTarget)
                .unwrap();
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestBid, &remaining_bid);
            env.storage()
                .instance()
                .set(&MockBackstopKey::InterestLotValue, &lot_value);
            Some(InterestLotQuote {
                bid_token,
                bid_amount: remaining_bid,
                lot_value,
                target_value,
                tier,
                valid_until: u64::MAX,
            })
        }

        pub fn released_interest_auction(env: Env) -> Option<BytesN<32>> {
            env.storage()
                .instance()
                .get(&MockBackstopKey::InterestReleased)
        }
    }

    #[contractimpl]
    impl MockPriceOracle {
        pub fn set_price(env: Env, asset: Address, quote: PriceQuote) {
            env.storage()
                .instance()
                .set(&MockOracleKey::Price(asset), &quote);
        }

        pub fn price(env: Env, asset: Address) -> PriceQuote {
            let calls_key = MockOracleKey::Calls(asset.clone());
            let calls: u32 = env.storage().instance().get(&calls_key).unwrap_or(0);
            env.storage().instance().set(&calls_key, &(calls + 1));
            env.storage()
                .instance()
                .get(&MockOracleKey::Price(asset))
                .unwrap()
        }

        pub fn calls(env: Env, asset: Address) -> u32 {
            env.storage()
                .instance()
                .get(&MockOracleKey::Calls(asset))
                .unwrap_or(0)
        }
    }

    #[contractimpl]
    impl MockFeeToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let key = MockFeeTokenKey::Balance(to);
            let balance: i128 = env.storage().instance().get(&key).unwrap_or(0);
            env.storage().instance().set(&key, &(balance + amount));
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage()
                .instance()
                .get(&MockFeeTokenKey::Balance(id))
                .unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            let from_key = MockFeeTokenKey::Balance(from);
            let to_key = MockFeeTokenKey::Balance(to);
            let from_balance: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
            assert!(amount > 1 && from_balance >= amount);
            let to_balance: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
            env.storage()
                .instance()
                .set(&from_key, &(from_balance - amount));
            env.storage()
                .instance()
                .set(&to_key, &(to_balance + amount - 1));
        }
    }

    fn status_quote(status: u32, transition_allowed: bool) -> PoolStatusQuote {
        PoolStatusQuote {
            eligible_value: 12_500 * 10_000_000,
            meets_activation_threshold: true,
            q4w_percentage: 0,
            required_value: 12_500 * 10_000_000,
            status,
            transition_allowed,
        }
    }

    fn register_pool(env: &Env, backstop: &Address, operator: &Address) -> Address {
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 4,
            min_collateral: 0,
            oracle: Address::generate(env),
            price_decimals: 7,
        };
        env.register(
            PoolContract,
            (
                backstop,
                operator,
                &lending_config,
                Vec::<ReserveConfig>::new(env),
            ),
        )
    }

    fn reserve_config(asset: &Address, index: u32) -> ReserveConfig {
        ReserveConfig {
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

    fn reserve_config_update(config: &ReserveConfig) -> ReserveConfigUpdate {
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

    fn register_lending_pool(
        env: &Env,
        backstop: &Address,
        operator: &Address,
        oracle: &Address,
        assets: &Vec<Address>,
    ) -> Address {
        let lending_config = LendingConfig {
            backstop_take_rate: 2_000_000,
            max_positions: 6,
            min_collateral: 10_000_000,
            oracle: oracle.clone(),
            price_decimals: 7,
        };
        let mut reserves = Vec::new(env);
        for (index, asset) in assets.iter().enumerate() {
            reserves.push_back(reserve_config(&asset, index as u32));
        }
        register_custom_lending_pool(env, backstop, operator, &lending_config, &reserves)
    }

    fn register_custom_lending_pool(
        env: &Env,
        backstop: &Address,
        operator: &Address,
        lending_config: &LendingConfig,
        reserves: &Vec<ReserveConfig>,
    ) -> Address {
        env.register(PoolContract, (backstop, operator, lending_config, reserves))
    }

    fn setup_liquidation_pool(env: &Env) -> (Address, Address, Address, Address, Address, Address) {
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);
        let token_admin = Address::generate(env);
        let collateral_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let borrowed_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let collateral_admin = StellarAssetClient::new(env, &collateral_asset);
        let borrowed_admin = StellarAssetClient::new(env, &borrowed_asset);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(env, &oracle_id);
        for asset in [collateral_asset.clone(), borrowed_asset.clone()] {
            oracle.set_price(
                &asset,
                &PriceQuote {
                    decimals: 7,
                    price: lending::SCALAR_7,
                    valid_until: u64::MAX,
                },
            );
        }

        let supplier = Address::generate(env);
        let borrower = Address::generate(env);
        let filler = Address::generate(env);
        let pool_id = register_lending_pool(
            env,
            &filler,
            &Address::generate(env),
            &oracle_id,
            &vec![env, collateral_asset.clone(), borrowed_asset.clone()],
        );
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let pool = PoolContractClient::new(env, &pool_id);
        borrowed_admin.mint(&supplier, &(1_000 * lending::SCALAR_7));
        collateral_admin.mint(&borrower, &(100 * lending::SCALAR_7));
        collateral_admin.mint(&filler, &(100 * lending::SCALAR_7));
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &vec![
                env,
                Request {
                    address: borrowed_asset.clone(),
                    amount: 1_000 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY,
                },
            ],
        );
        pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 100 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: borrowed_asset.clone(),
                    amount: 70 * lending::SCALAR_7,
                    request_type: ACTION_BORROW,
                },
            ],
        );
        pool.submit(
            &filler,
            &filler,
            &filler,
            &vec![
                env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 100 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY_COLLATERAL,
                },
            ],
        );
        (
            pool_id,
            oracle_id,
            collateral_asset,
            borrowed_asset,
            borrower,
            filler,
        )
    }

    fn setup_backstop_bad_debt_pool(env: &Env) -> (Address, Address, Address, Address, Address) {
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);
        let token_admin = Address::generate(env);
        let collateral_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let first_debt_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let second_debt_asset = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let collateral_admin = StellarAssetClient::new(env, &collateral_asset);
        let first_debt_admin = StellarAssetClient::new(env, &first_debt_asset);
        let second_debt_admin = StellarAssetClient::new(env, &second_debt_asset);
        let backstop_id = env.register(MockBackstop, ());
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(env, &oracle_id);
        for asset in [
            collateral_asset.clone(),
            first_debt_asset.clone(),
            second_debt_asset.clone(),
        ] {
            oracle.set_price(
                &asset,
                &PriceQuote {
                    decimals: 7,
                    price: lending::SCALAR_7,
                    valid_until: u64::MAX,
                },
            );
        }

        let pool_id = register_lending_pool(
            env,
            &backstop_id,
            &Address::generate(env),
            &oracle_id,
            &vec![
                env,
                collateral_asset.clone(),
                first_debt_asset.clone(),
                second_debt_asset.clone(),
            ],
        );
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let pool = PoolContractClient::new(env, &pool_id);
        let supplier = Address::generate(env);
        first_debt_admin.mint(&supplier, &(1_000 * lending::SCALAR_7));
        second_debt_admin.mint(&supplier, &(1_000 * lending::SCALAR_7));
        collateral_admin.mint(&backstop_id, &(200 * lending::SCALAR_7));
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &vec![
                env,
                Request {
                    address: first_debt_asset.clone(),
                    amount: 1_000 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY,
                },
                Request {
                    address: second_debt_asset.clone(),
                    amount: 1_000 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY,
                },
            ],
        );
        pool.submit(
            &backstop_id,
            &backstop_id,
            &backstop_id,
            &vec![
                env,
                Request {
                    address: collateral_asset,
                    amount: 200 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: first_debt_asset.clone(),
                    amount: 50 * lending::SCALAR_7,
                    request_type: ACTION_BORROW,
                },
                Request {
                    address: second_debt_asset.clone(),
                    amount: 75 * lending::SCALAR_7,
                    request_type: ACTION_BORROW,
                },
            ],
        );
        (
            pool_id,
            backstop_id,
            first_debt_asset,
            second_debt_asset,
            supplier,
        )
    }

    #[test]
    fn stores_admin_status_configuration_and_applies_canonical_quotes() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);

        assert_eq!(pool.backstop(), backstop_id);
        assert_eq!(pool.admin(), operator);
        assert_eq!(pool.status(), STATUS_SETUP);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        backstop.set_update_quote(&status_quote(3, true));
        assert_eq!(pool.update_pool_status(), 3);
        let events = env.events().all();
        assert_eq!(events.events().len(), 1);
        assert!(events.events()[0].contract_id.is_some());
        assert_eq!(pool.status(), 3);

        backstop.set_update_quote(&status_quote(1, false));
        assert!(pool.try_update_pool_status().is_err());
        assert_eq!(pool.status(), 3);

        backstop.set_update_quote(&status_quote(STATUS_SETUP + 1, true));
        assert!(pool.try_update_pool_status().is_err());
        assert_eq!(pool.status(), 3);
    }

    #[test]
    fn admin_status_request_requires_the_current_admin() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);
        backstop.set_set_quote(&status_quote(0, true));

        assert!(pool.try_set_pool_status(&0).is_err());
        assert_eq!(pool.status(), STATUS_SETUP);

        assert_eq!(
            pool.mock_auths(&[MockAuth {
                address: &operator,
                invoke: &MockAuthInvoke {
                    contract: &pool_id,
                    fn_name: "set_pool_status",
                    args: (0_u32,).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .set_pool_status(&0),
            0
        );
        assert_eq!(pool.status(), 0);
    }

    #[test]
    fn admin_transfer_requires_proposal_and_acceptance() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let current_admin = Address::generate(&env);
        let proposed_admin = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &current_admin);
        let pool = PoolContractClient::new(&env, &pool_id);

        assert!(pool.try_propose_admin(&proposed_admin).is_err());
        pool.mock_auths(&[MockAuth {
            address: &current_admin,
            invoke: &MockAuthInvoke {
                contract: &pool_id,
                fn_name: "propose_admin",
                args: (proposed_admin.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .propose_admin(&proposed_admin);
        assert_eq!(pool.admin(), current_admin);

        assert!(pool.try_accept_admin().is_err());
        pool.mock_auths(&[MockAuth {
            address: &proposed_admin,
            invoke: &MockAuthInvoke {
                contract: &pool_id,
                fn_name: "accept_admin",
                args: ().into_val(&env),
                sub_invokes: &[],
            },
        }])
        .accept_admin();
        assert_eq!(pool.admin(), proposed_admin);
        assert!(pool.try_accept_admin().is_err());

        assert!(pool
            .mock_auths(&[MockAuth {
                address: &current_admin,
                invoke: &MockAuthInvoke {
                    contract: &pool_id,
                    fn_name: "update_pool",
                    args: (1_000_000_u32, 4_u32, 0_i128).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .try_update_pool(&1_000_000, &4, &0)
            .is_err());
        assert!(pool
            .mock_auths(&[MockAuth {
                address: &proposed_admin,
                invoke: &MockAuthInvoke {
                    contract: &pool_id,
                    fn_name: "update_pool",
                    args: (1_000_000_u32, 4_u32, 0_i128).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .try_update_pool(&1_000_000, &4, &0)
            .is_ok());
    }

    #[test]
    fn admin_updates_pool_and_queues_v2_style_reserve_configuration() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let backstop_id = env.register(MockBackstop, ());
        let admin = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &admin);
        let pool = PoolContractClient::new(&env, &pool_id);

        let updated = pool.update_pool(&2_000_000, &6, &(5 * lending::SCALAR_7));
        assert_eq!(updated.backstop_take_rate, 2_000_000);
        assert_eq!(updated.max_positions, 6);
        assert_eq!(updated.min_collateral, 5 * lending::SCALAR_7);

        let asset = Address::generate(&env);
        let config = reserve_config(&asset, 0);
        let update = reserve_config_update(&config);
        assert_eq!(pool.queue_set_reserve(&asset, &update), 1_000);
        assert_eq!(
            env.events().all().events().last(),
            Some(
                &ReserveConfigQueued {
                    admin: admin.clone(),
                    asset: asset.clone(),
                    update: update.clone(),
                    unlock_time: 1_000,
                }
                .to_xdr(&env, &pool_id)
            )
        );
        assert_eq!(pool.set_reserve(&asset), 0);
        assert_eq!(pool.reserve_list(), vec![&env, asset.clone()]);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let mut updated_reserve = update;
        updated_reserve.supply_cap += 1;
        assert_eq!(
            pool.queue_set_reserve(&asset, &updated_reserve),
            1_000 + lending::SECONDS_PER_WEEK
        );
        assert!(pool.try_set_reserve(&asset).is_err());
        env.ledger()
            .set_timestamp(1_000 + lending::SECONDS_PER_WEEK);
        assert_eq!(pool.set_reserve(&asset), 0);
        assert_eq!(
            pool.reserve(&asset).config.supply_cap,
            updated_reserve.supply_cap
        );

        updated_reserve.supply_cap += 1;
        pool.queue_set_reserve(&asset, &updated_reserve);
        pool.cancel_set_reserve(&asset);
        assert!(pool.try_set_reserve(&asset).is_err());
    }

    #[test]
    fn concurrent_new_reserve_queues_assign_indexes_when_applied() {
        let env = Env::default();
        env.mock_all_auths();
        let backstop_id = env.register(MockBackstop, ());
        let admin = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &admin);
        let pool = PoolContractClient::new(&env, &pool_id);
        let asset_a = Address::generate(&env);
        let asset_b = Address::generate(&env);
        let config_a = reserve_config_update(&reserve_config(&asset_a, 0));
        let config_b = reserve_config_update(&reserve_config(&asset_b, 0));

        pool.queue_set_reserve(&asset_a, &config_a);
        pool.queue_set_reserve(&asset_b, &config_b);

        assert_eq!(pool.set_reserve(&asset_a), 0);
        assert_eq!(pool.set_reserve(&asset_b), 1);
        assert_eq!(
            pool.reserve_list(),
            vec![&env, asset_a.clone(), asset_b.clone()]
        );
        assert_eq!(pool.reserve(&asset_a).config.index, 0);
        assert_eq!(pool.reserve(&asset_b).config.index, 1);
    }

    #[test]
    fn reserve_queue_and_cancel_require_the_current_admin() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let current_admin = Address::generate(&env);
        let next_admin = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &current_admin);
        let pool = PoolContractClient::new(&env, &pool_id);
        let asset = Address::generate(&env);
        let config = reserve_config_update(&reserve_config(&asset, 0));

        assert!(pool.try_queue_set_reserve(&asset, &config).is_err());
        pool.mock_auths(&[MockAuth {
            address: &current_admin,
            invoke: &MockAuthInvoke {
                contract: &pool_id,
                fn_name: "queue_set_reserve",
                args: (asset.clone(), config.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .queue_set_reserve(&asset, &config);
        assert!(pool.try_cancel_set_reserve(&asset).is_err());

        pool.mock_auths(&[MockAuth {
            address: &current_admin,
            invoke: &MockAuthInvoke {
                contract: &pool_id,
                fn_name: "propose_admin",
                args: (next_admin.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .propose_admin(&next_admin);
        pool.mock_auths(&[MockAuth {
            address: &next_admin,
            invoke: &MockAuthInvoke {
                contract: &pool_id,
                fn_name: "accept_admin",
                args: ().into_val(&env),
                sub_invokes: &[],
            },
        }])
        .accept_admin();

        assert!(pool
            .mock_auths(&[MockAuth {
                address: &current_admin,
                invoke: &MockAuthInvoke {
                    contract: &pool_id,
                    fn_name: "cancel_set_reserve",
                    args: (asset.clone(),).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .try_cancel_set_reserve(&asset)
            .is_err());
        pool.mock_auths(&[MockAuth {
            address: &next_admin,
            invoke: &MockAuthInvoke {
                contract: &pool_id,
                fn_name: "cancel_set_reserve",
                args: (asset.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .cancel_set_reserve(&asset);
        assert!(pool.try_set_reserve(&asset).is_err());
    }

    #[test]
    fn reserve_queue_cancellation_is_v2_compatible_and_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let backstop_id = env.register(MockBackstop, ());
        let admin = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &admin);
        let pool = PoolContractClient::new(&env, &pool_id);
        let asset = Address::generate(&env);
        let config = reserve_config_update(&reserve_config(&asset, 0));

        pool.cancel_set_reserve(&asset);
        pool.queue_set_reserve(&asset, &config);
        pool.cancel_set_reserve(&asset);
        pool.cancel_set_reserve(&asset);

        assert!(pool.try_set_reserve(&asset).is_err());
    }

    #[test]
    fn action_policy_matches_the_v2_status_matrix() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop_id, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        for action in 0..=MAX_ACTION_TYPE {
            assert!(pool.action_allowed(&action));
        }

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &3_u32);
        });
        assert!(pool.action_allowed(&ACTION_SUPPLY));
        assert!(pool.action_allowed(&ACTION_SUPPLY_COLLATERAL));
        assert!(!pool.action_allowed(&ACTION_BORROW));
        assert!(!pool.action_allowed(&ACTION_DELETE_LIQUIDATION));

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &5_u32);
        });
        assert!(!pool.action_allowed(&ACTION_SUPPLY));
        assert!(!pool.action_allowed(&ACTION_SUPPLY_COLLATERAL));
        assert!(!pool.action_allowed(&ACTION_BORROW));
        assert!(!pool.action_allowed(&ACTION_DELETE_LIQUIDATION));
        assert!(pool
            .try_test_require_action_allowed(&ACTION_BORROW)
            .is_err());
        for action in [1_u32, 3, 5, 6, 7, 8] {
            assert!(pool.action_allowed(&action));
            pool.test_require_action_allowed(&action);
        }
        assert!(pool.try_action_allowed(&(MAX_ACTION_TYPE + 1)).is_err());
    }

    #[test]
    fn user_liquidation_auction_supports_partial_and_full_fills() {
        let env = Env::default();
        let (pool_id, oracle_id, collateral_asset, borrowed_asset, borrower, filler) =
            setup_liquidation_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 9_000_000,
                valid_until: u64::MAX,
            },
        );
        assert!(!pool.position_health(&borrower).healthy);
        let collateral_calls_before_creation = oracle.calls(&collateral_asset);
        let liability_calls_before_creation = oracle.calls(&borrowed_asset);

        let auction = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset.clone()],
            &vec![&env, collateral_asset.clone()],
            &50,
        );
        assert_eq!(
            oracle.calls(&collateral_asset),
            collateral_calls_before_creation + 1
        );
        assert_eq!(
            oracle.calls(&borrowed_asset),
            liability_calls_before_creation + 1
        );
        assert_eq!(auction.block, env.ledger().sequence() + 1);
        assert_eq!(pool.get_liquidation_auction(&borrower), auction);
        env.as_contract(&pool_id, || {
            let ledgers_since_creation =
                env.ledger().sequence() - auction.block.checked_sub(1).unwrap();
            assert_eq!(
                auction::user_liquidation_ttl(&env, &borrower),
                46 * DAY_IN_LEDGERS - ledgers_since_creation
            );
        });

        let borrower_before = pool.positions(&borrower);
        let collateral_supply_before = pool.reserve(&collateral_asset).data.b_supply;
        let liability_supply_before = pool.reserve(&borrowed_asset).data.d_supply;
        let borrowed_token = TokenClient::new(&env, &borrowed_asset);
        let borrower_balance = borrowed_token.balance(&borrower);
        assert!(pool
            .try_submit(
                &borrower,
                &borrower,
                &borrower,
                &vec![
                    &env,
                    Request {
                        address: borrowed_asset.clone(),
                        amount: lending::SCALAR_7,
                        request_type: ACTION_REPAY,
                    }
                ],
            )
            .is_err());
        assert_eq!(pool.positions(&borrower), borrower_before);
        assert_eq!(borrowed_token.balance(&borrower), borrower_balance);

        assert!(pool
            .try_submit(
                &filler,
                &filler,
                &filler,
                &vec![
                    &env,
                    Request {
                        address: borrower.clone(),
                        amount: 50,
                        request_type: ACTION_FILL_USER_LIQUIDATION,
                    }
                ],
            )
            .is_err());
        env.ledger().set_sequence_number(auction.block + 200);
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &5_u32);
        });
        let collateral_calls_before_fill = oracle.calls(&collateral_asset);
        let liability_calls_before_fill = oracle.calls(&borrowed_asset);

        let after_partial = pool.submit(
            &filler,
            &filler,
            &filler,
            &vec![
                &env,
                Request {
                    address: borrower.clone(),
                    amount: 50,
                    request_type: ACTION_FILL_USER_LIQUIDATION,
                },
            ],
        );
        assert_eq!(
            oracle.calls(&collateral_asset),
            collateral_calls_before_fill + 1
        );
        assert_eq!(
            oracle.calls(&borrowed_asset),
            liability_calls_before_fill + 1
        );
        let remaining = pool.get_liquidation_auction(&borrower);
        env.as_contract(&pool_id, || {
            let ledgers_since_creation =
                env.ledger().sequence() - auction.block.checked_sub(1).unwrap();
            assert_eq!(
                auction::user_liquidation_ttl(&env, &borrower),
                46 * DAY_IN_LEDGERS - ledgers_since_creation
            );
        });
        assert!(
            remaining.bid.get(borrowed_asset.clone()).unwrap()
                < auction.bid.get(borrowed_asset.clone()).unwrap()
        );
        assert!(after_partial.liabilities.get(1).unwrap() > 0);
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        assert!(pool
            .try_submit(
                &borrower,
                &borrower,
                &borrower,
                &vec![
                    &env,
                    Request {
                        address: borrowed_asset.clone(),
                        amount: lending::SCALAR_7,
                        request_type: ACTION_REPAY,
                    }
                ],
            )
            .is_err());

        let after_full = pool.submit(
            &filler,
            &filler,
            &filler,
            &vec![
                &env,
                Request {
                    address: borrower.clone(),
                    amount: 100,
                    request_type: ACTION_FILL_USER_LIQUIDATION,
                },
            ],
        );
        assert!(pool.try_get_liquidation_auction(&borrower).is_err());
        assert!(after_full.collateral.get(0).unwrap() > 100 * lending::SCALAR_7);
        assert!(after_full.liabilities.get(1).unwrap() > 0);
        assert!(pool.position_health(&filler).healthy);
        assert_eq!(pool.backstop_loss_state().liability_entries, 1);
        assert!(!pool.backstop_withdrawal_allowed(&filler));
        assert!(pool.positions(&borrower).collateral.get(0).unwrap() < 100 * lending::SCALAR_7);
        assert!(pool.positions(&borrower).liabilities.get(1).unwrap() < 70 * lending::SCALAR_7);
        assert_eq!(
            pool.reserve(&collateral_asset).data.b_supply,
            collateral_supply_before
        );
        assert_eq!(
            pool.reserve(&borrowed_asset).data.d_supply,
            liability_supply_before
        );
    }

    #[test]
    fn liquidation_creation_rejects_healthy_duplicate_and_missing_positions() {
        let env = Env::default();
        let (pool_id, oracle_id, collateral_asset, borrowed_asset, borrower, _) =
            setup_liquidation_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        assert!(pool
            .try_new_liquidation_auction(
                &borrower,
                &vec![&env, borrowed_asset.clone()],
                &vec![&env, collateral_asset.clone()],
                &50,
            )
            .is_err());

        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 9_000_000,
                valid_until: u64::MAX,
            },
        );
        assert!(pool
            .try_new_liquidation_auction(
                &borrower,
                &vec![&env, borrowed_asset.clone(), borrowed_asset.clone()],
                &vec![&env, collateral_asset.clone()],
                &50,
            )
            .is_err());
        assert!(pool
            .try_new_liquidation_auction(
                &borrower,
                &vec![&env, borrowed_asset.clone()],
                &Vec::new(&env),
                &50,
            )
            .is_err());
        assert!(pool.try_get_liquidation_auction(&borrower).is_err());

        pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset],
            &vec![&env, collateral_asset],
            &50,
        );
        assert!(pool
            .try_new_liquidation_auction(&borrower, &Vec::new(&env), &Vec::new(&env), &50,)
            .is_err());
    }

    #[test]
    fn liquidation_cancellation_obeys_status_and_stale_deletion_is_permissionless() {
        let env = Env::default();
        let (pool_id, oracle_id, collateral_asset, borrowed_asset, borrower, _) =
            setup_liquidation_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 9_000_000,
                valid_until: u64::MAX,
            },
        );
        let auction = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset.clone()],
            &vec![&env, collateral_asset.clone()],
            &50,
        );
        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &3_u32);
        });
        assert!(pool
            .try_submit(
                &borrower,
                &borrower,
                &borrower,
                &vec![
                    &env,
                    Request {
                        address: borrower.clone(),
                        amount: 0,
                        request_type: ACTION_DELETE_LIQUIDATION,
                    }
                ],
            )
            .is_err());
        assert_eq!(pool.get_liquidation_auction(&borrower), auction);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: borrower.clone(),
                    amount: 0,
                    request_type: ACTION_DELETE_LIQUIDATION,
                },
            ],
        );
        assert!(pool.try_get_liquidation_auction(&borrower).is_err());

        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 9_000_000,
                valid_until: u64::MAX,
            },
        );
        let replacement = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset],
            &vec![&env, collateral_asset],
            &50,
        );
        env.ledger().set_sequence_number(replacement.block + 499);
        assert!(pool
            .try_delete_stale_liquidation_auction(&borrower)
            .is_err());
        env.ledger().set_sequence_number(replacement.block + 500);
        pool.delete_stale_liquidation_auction(&borrower);
        assert!(pool.try_get_liquidation_auction(&borrower).is_err());
    }

    #[test]
    fn tier_specific_interest_auction_conserves_partial_fill_and_compounds_bid() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);
        env.ledger().set_sequence_number(10);
        let token_admin = Address::generate(&env);
        let reserve_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let reserve_admin = StellarAssetClient::new(&env, &reserve_asset);
        let bid_token = Address::generate(&env);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        backstop.set_interest_bid_token(&bid_token);
        let oracle_id = env.register(MockPriceOracle, ());
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &reserve_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![&env, reserve_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let credit = 400 * lending::SCALAR_7;
        reserve_admin.mint(&pool_id, &credit);
        env.as_contract(&pool_id, || {
            lending::set_backstop_credit(&env, &reserve_asset, credit);
        });

        let auction_id = BytesN::from_array(&env, &[41; 32]);
        let auction = pool.new_interest_auction(&auction_id, &vec![&env, reserve_asset.clone()]);
        assert_eq!(auction.tier, BackstopTier::BlndUsdc);
        assert_eq!(auction.auction.lot.get(reserve_asset.clone()), Some(credit));
        assert_eq!(auction.lot_value, credit);
        assert_eq!(
            pool.interest_reserve_state(&reserve_asset).blnd_usdc,
            credit
        );
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 0,
                unresolved_bad_debt_entries: 0,
            }
        );

        let filler = Address::generate(&env);
        env.ledger()
            .set_sequence_number(auction.auction.block + 100);
        let partial = pool.fill_interest_auction(&filler, &50);
        assert!(!partial.complete);
        assert_eq!(
            partial.base_lot.get(reserve_asset.clone()),
            Some(200 * lending::SCALAR_7)
        );
        assert_eq!(
            partial.lot.get(reserve_asset.clone()),
            Some(100 * lending::SCALAR_7)
        );
        assert_eq!(
            partial.returned_lot.get(reserve_asset.clone()),
            Some(100 * lending::SCALAR_7)
        );
        assert_eq!(
            TokenClient::new(&env, &reserve_asset).balance(&filler),
            100 * lending::SCALAR_7
        );
        assert_eq!(
            pool.reserve(&reserve_asset).data.backstop_credit,
            300 * lending::SCALAR_7
        );
        assert_eq!(
            pool.interest_reserve_state(&reserve_asset).blnd_usdc,
            300 * lending::SCALAR_7
        );

        env.ledger()
            .set_sequence_number(auction.auction.block + 200);
        let complete = pool.fill_interest_auction(&filler, &100);
        assert!(complete.complete);
        assert_eq!(
            TokenClient::new(&env, &reserve_asset).balance(&filler),
            300 * lending::SCALAR_7
        );
        assert_eq!(
            pool.reserve(&reserve_asset).data.backstop_credit,
            100 * lending::SCALAR_7
        );
        assert_eq!(
            pool.interest_reserve_state(&reserve_asset).blnd_usdc,
            100 * lending::SCALAR_7
        );
        assert!(pool.try_get_interest_auction().is_err());
    }

    #[test]
    fn stale_interest_auction_returns_the_reserved_lot_to_pending_credit() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);
        env.ledger().set_sequence_number(10);
        let token_admin = Address::generate(&env);
        let reserve_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let reserve_admin = StellarAssetClient::new(&env, &reserve_asset);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        backstop.set_interest_bid_token(&Address::generate(&env));
        let oracle_id = env.register(MockPriceOracle, ());
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &reserve_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![&env, reserve_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let credit = 200 * lending::SCALAR_7;
        reserve_admin.mint(&pool_id, &credit);
        env.as_contract(&pool_id, || {
            lending::set_backstop_credit(&env, &reserve_asset, credit);
        });
        let auction_id = BytesN::from_array(&env, &[42; 32]);
        let auction = pool.new_interest_auction(&auction_id, &vec![&env, reserve_asset.clone()]);

        env.ledger()
            .set_sequence_number(auction.auction.block + 499);
        assert!(pool.try_delete_stale_interest_auction().is_err());
        env.ledger()
            .set_sequence_number(auction.auction.block + 500);
        pool.delete_stale_interest_auction();
        assert_eq!(
            pool.interest_reserve_state(&reserve_asset).blnd_usdc,
            credit
        );
        assert_eq!(backstop.released_interest_auction(), Some(auction_id));
        assert_eq!(pool.reserve(&reserve_asset).data.backstop_credit, credit);
    }

    #[test]
    fn interest_auction_cursor_cycles_across_all_qualifying_weighted_tiers() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);
        env.ledger().set_sequence_number(10);
        let token_admin = Address::generate(&env);
        let reserve_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let reserve_admin = StellarAssetClient::new(&env, &reserve_asset);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        backstop.set_interest_bid_token(&Address::generate(&env));
        backstop.set_interest_weighted(&true);
        let oracle_id = env.register(MockPriceOracle, ());
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &reserve_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![&env, reserve_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let credit = 1_400 * lending::SCALAR_7;
        reserve_admin.mint(&pool_id, &credit);
        env.as_contract(&pool_id, || {
            lending::set_backstop_credit(&env, &reserve_asset, credit);
        });

        for (index, expected_tier, expected_lot) in [
            (0_u8, BackstopTier::BlndUsdc, 500 * lending::SCALAR_7),
            (1_u8, BackstopTier::BlndXlm, 500 * lending::SCALAR_7),
            (2_u8, BackstopTier::Usdc, 400 * lending::SCALAR_7),
        ] {
            let auction = pool.new_interest_auction(
                &BytesN::from_array(&env, &[60 + index; 32]),
                &vec![&env, reserve_asset.clone()],
            );
            assert_eq!(auction.tier, expected_tier);
            assert_eq!(
                auction.auction.lot.get(reserve_asset.clone()),
                Some(expected_lot)
            );
            env.ledger()
                .set_sequence_number(auction.auction.block + 500);
            pool.delete_stale_interest_auction();
        }
        assert_eq!(
            pool.interest_reserve_state(&reserve_asset),
            InterestReserveState {
                blnd_usdc: 500 * lending::SCALAR_7,
                blnd_xlm: 500 * lending::SCALAR_7,
                carry: 0,
                usdc: 400 * lending::SCALAR_7,
            }
        );
    }

    #[test]
    fn interest_auction_enforces_bounded_batch_and_exact_200_usdc_minimum() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);
        let token_admin = Address::generate(&env);
        let reserve_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let reserve_admin = StellarAssetClient::new(&env, &reserve_asset);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        backstop.set_interest_bid_token(&Address::generate(&env));
        let oracle_id = env.register(MockPriceOracle, ());
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &reserve_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![&env, reserve_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let auction_id = BytesN::from_array(&env, &[70; 32]);
        assert!(pool
            .try_new_interest_auction(&auction_id, &Vec::new(&env))
            .is_err());
        assert!(pool
            .try_new_interest_auction(
                &auction_id,
                &vec![&env, reserve_asset.clone(), reserve_asset.clone()]
            )
            .is_err());
        assert!(pool
            .try_new_interest_auction(
                &auction_id,
                &vec![
                    &env,
                    Address::generate(&env),
                    Address::generate(&env),
                    Address::generate(&env),
                    Address::generate(&env),
                    Address::generate(&env)
                ]
            )
            .is_err());

        let below = 200 * lending::SCALAR_7 - 1;
        reserve_admin.mint(&pool_id, &(below + 1));
        env.as_contract(&pool_id, || {
            lending::set_backstop_credit(&env, &reserve_asset, below);
        });
        assert!(pool
            .try_new_interest_auction(&auction_id, &vec![&env, reserve_asset.clone()])
            .is_err());
        env.as_contract(&pool_id, || {
            lending::set_backstop_credit(&env, &reserve_asset, below + 1);
        });
        let auction = pool.new_interest_auction(&auction_id, &vec![&env, reserve_asset.clone()]);
        assert_eq!(auction.lot_value, 200 * lending::SCALAR_7);
    }

    #[test]
    fn bad_debt_auction_preparation_commits_and_stale_delete_releases_lot() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        let debt_asset = Address::generate(&env);
        oracle.set_price(
            &debt_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![&env, debt_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let debt_shares = 50 * lending::SCALAR_7;
        env.as_contract(&pool_id, || {
            let mut backstop_positions = Positions::empty(&env);
            backstop_positions.liabilities.set(0, debt_shares);
            lending::set_positions(&env, &backstop_id, &backstop_positions);
            set_backstop_liability(&env, &debt_asset, debt_shares);
        });
        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 60 * lending::SCALAR_7,
            debt_value: 0,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 60 * lending::SCALAR_7,
            target_value: 60 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        });

        let auction_id = BytesN::from_array(&env, &[7; 32]);
        let auction = pool.new_bad_debt_auction(&auction_id, &vec![&env, debt_asset.clone()]);
        assert_eq!(auction.auction_id, auction_id);
        assert_eq!(auction.bid.get(debt_asset), Some(debt_shares));
        assert_eq!(auction.lot_quote.debt_value, 50 * lending::SCALAR_7);
        assert_eq!(backstop.committed_debt(), 50 * lending::SCALAR_7);
        assert_eq!(backstop.committed_auction(), Some(auction_id.clone()));
        assert_eq!(pool.get_bad_debt_auction(), auction);
        env.as_contract(&pool_id, || {
            assert_eq!(auction::bad_debt_auction_ttl(&env), 46 * DAY_IN_LEDGERS);
        });
        let creation_ledger = auction.block.checked_sub(1).unwrap();
        env.ledger()
            .set_sequence_number(creation_ledger.checked_add(100).unwrap());
        assert_eq!(pool.get_bad_debt_auction(), auction);
        env.as_contract(&pool_id, || {
            assert_eq!(
                auction::bad_debt_auction_ttl(&env),
                46 * DAY_IN_LEDGERS - 100
            );
        });
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 1,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&backstop_id));
        assert!(pool
            .try_new_bad_debt_auction(&BytesN::from_array(&env, &[8; 32]), &vec![&env])
            .is_err());

        env.ledger().set_sequence_number(auction.block + 499);
        assert!(pool.try_delete_stale_bad_debt_auction().is_err());
        env.ledger().set_sequence_number(auction.block + 500);
        pool.delete_stale_bad_debt_auction();
        assert!(pool.try_get_bad_debt_auction().is_err());
        assert_eq!(backstop.released_auction(), Some(auction_id));
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );
    }

    #[test]
    fn bad_debt_auction_partial_and_full_fills_are_atomic() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        let debt_asset = Address::generate(&env);
        let collateral_asset = Address::generate(&env);
        for asset in [debt_asset.clone(), collateral_asset.clone()] {
            oracle.set_price(
                &asset,
                &PriceQuote {
                    decimals: 7,
                    price: lending::SCALAR_7,
                    valid_until: u64::MAX,
                },
            );
        }
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![&env, debt_asset.clone(), collateral_asset],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let filler = Address::generate(&env);
        let unhealthy_filler = Address::generate(&env);
        let debt_shares = 50 * lending::SCALAR_7;
        env.as_contract(&pool_id, || {
            let mut backstop_positions = Positions::empty(&env);
            backstop_positions.liabilities.set(0, debt_shares);
            lending::set_positions(&env, &backstop_id, &backstop_positions);
            set_backstop_liability(&env, &debt_asset, debt_shares);

            let mut filler_positions = Positions::empty(&env);
            filler_positions.collateral.set(1, 200 * lending::SCALAR_7);
            lending::set_positions(&env, &filler, &filler_positions);
        });
        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 60 * lending::SCALAR_7,
            debt_value: 0,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 60 * lending::SCALAR_7,
            target_value: 60 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        });

        let auction_id = BytesN::from_array(&env, &[9; 32]);
        let auction = pool.new_bad_debt_auction(&auction_id, &vec![&env, debt_asset.clone()]);
        assert!(pool.try_fill_bad_debt_auction(&filler, &50).is_err());

        env.mock_all_auths();
        assert!(pool.try_fill_bad_debt_auction(&filler, &50).is_err());
        assert!(pool.try_fill_bad_debt_auction(&pool_id, &50).is_err());
        assert!(pool.try_fill_bad_debt_auction(&backstop_id, &50).is_err());
        assert_eq!(backstop.settled_count(), 0);

        env.ledger().set_sequence_number(auction.block + 100);
        let auction_before_unhealthy_fill = pool.get_bad_debt_auction();
        assert!(pool
            .try_fill_bad_debt_auction(&unhealthy_filler, &50)
            .is_err());
        assert_eq!(pool.get_bad_debt_auction(), auction_before_unhealthy_fill);
        assert_eq!(
            pool.positions(&backstop_id).liabilities.get(0),
            Some(debt_shares)
        );
        assert!(pool.positions(&unhealthy_filler).liabilities.is_empty());
        assert_eq!(backstop.settled_count(), 0);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &4_u32);
        });
        let first = pool.fill_bad_debt_auction(&filler, &50);
        assert_eq!(
            first,
            BadDebtAuctionFill {
                auction_id: auction_id.clone(),
                base_lot_amount: 30 * lending::SCALAR_7,
                bid: Map::from_array(&env, [(debt_asset.clone(), 25 * lending::SCALAR_7)]),
                block: auction.block,
                complete: false,
                lot_amount: 15 * lending::SCALAR_7,
                tier: BackstopTier::BlndUsdc,
            }
        );
        assert_eq!(backstop.settled_count(), 1);
        assert_eq!(backstop.settled_base_lot(), 30 * lending::SCALAR_7);
        assert_eq!(backstop.settled_lot(), 15 * lending::SCALAR_7);
        assert_eq!(backstop.settled_to(), Some(filler.clone()));
        assert_eq!(
            pool.get_bad_debt_auction(),
            BadDebtAuctionData {
                auction_id: auction_id.clone(),
                bid: Map::from_array(&env, [(debt_asset.clone(), 25 * lending::SCALAR_7)]),
                block: auction.block,
                lot_quote: BadDebtLotQuote {
                    committed_value: 30 * lending::SCALAR_7,
                    debt_value: 50 * lending::SCALAR_7,
                    tier: BackstopTier::BlndUsdc,
                    lot_amount: 30 * lending::SCALAR_7,
                    target_value: 60 * lending::SCALAR_7,
                    unfilled_target_value: 0,
                    valid_until: u64::MAX,
                },
            }
        );
        assert_eq!(
            pool.positions(&backstop_id).liabilities.get(0),
            Some(25 * lending::SCALAR_7)
        );
        assert_eq!(
            pool.positions(&filler).liabilities.get(0),
            Some(25 * lending::SCALAR_7)
        );
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 1,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );

        env.ledger().set_sequence_number(auction.block + 200);
        let second = pool.fill_bad_debt_auction(&filler, &100);
        assert_eq!(
            second,
            BadDebtAuctionFill {
                auction_id,
                base_lot_amount: 30 * lending::SCALAR_7,
                bid: Map::from_array(&env, [(debt_asset.clone(), 25 * lending::SCALAR_7)]),
                block: auction.block,
                complete: true,
                lot_amount: 30 * lending::SCALAR_7,
                tier: BackstopTier::BlndUsdc,
            }
        );
        assert_eq!(backstop.settled_count(), 2);
        assert_eq!(backstop.settled_base_lot(), 30 * lending::SCALAR_7);
        assert_eq!(backstop.settled_lot(), 30 * lending::SCALAR_7);
        assert!(pool.try_get_bad_debt_auction().is_err());
        assert!(pool.positions(&backstop_id).liabilities.is_empty());
        assert_eq!(
            pool.positions(&filler).liabilities.get(0),
            Some(50 * lending::SCALAR_7)
        );
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 0,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(pool.backstop_withdrawal_allowed(&backstop_id));

        let discounted_debt = 40 * lending::SCALAR_7;
        env.as_contract(&pool_id, || {
            let mut backstop_positions = Positions::empty(&env);
            backstop_positions.liabilities.set(0, discounted_debt);
            lending::set_positions(&env, &backstop_id, &backstop_positions);
            set_backstop_liability(&env, &debt_asset, discounted_debt);
        });
        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 48 * lending::SCALAR_7,
            debt_value: 0,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 48 * lending::SCALAR_7,
            target_value: 48 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        });
        let discounted_auction_id = BytesN::from_array(&env, &[10; 32]);
        let discounted_auction =
            pool.new_bad_debt_auction(&discounted_auction_id, &vec![&env, debt_asset]);
        env.ledger()
            .set_sequence_number(discounted_auction.block + 300);
        let discounted_fill = pool.fill_bad_debt_auction(&filler, &100);
        assert!(discounted_fill.complete);
        assert_eq!(
            discounted_fill.bid.values().get(0),
            Some(20 * lending::SCALAR_7)
        );
        assert_eq!(discounted_fill.lot_amount, 48 * lending::SCALAR_7);
        assert_eq!(
            pool.positions(&backstop_id).liabilities.get(0),
            Some(20 * lending::SCALAR_7)
        );
        assert_eq!(
            pool.positions(&filler).liabilities.get(0),
            Some(70 * lending::SCALAR_7)
        );
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&backstop_id));
    }

    #[test]
    fn bad_debt_continuation_prepares_the_next_single_tier_auction() {
        let env = Env::default();
        let (pool_id, backstop_id, first_debt_asset, second_debt_asset, _) =
            setup_backstop_bad_debt_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 150 * lending::SCALAR_7,
            debt_value: 125 * lending::SCALAR_7,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 150 * lending::SCALAR_7,
            target_value: 150 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        });

        let auction_id = BytesN::from_array(&env, &[31; 32]);
        let continuation = pool.continue_bad_debt_resolution(&auction_id);
        assert!(continuation.auction_created);
        assert!(continuation.defaulted.is_empty());
        assert_eq!(
            pool.get_bad_debt_auction().bid,
            Map::from_array(
                &env,
                [
                    (first_debt_asset, 50 * lending::SCALAR_7),
                    (second_debt_asset, 75 * lending::SCALAR_7),
                ],
            )
        );
        assert_eq!(backstop.committed_auction(), Some(auction_id));
        assert_eq!(backstop.committed_debt(), 125 * lending::SCALAR_7);
        assert_eq!(
            pool.reserve(&pool.reserve_list().get(1).unwrap())
                .data
                .b_rate,
            lending::SCALAR_12
        );
        assert!(pool
            .try_continue_bad_debt_resolution(&BytesN::from_array(&env, &[32; 32]))
            .is_err());
    }

    #[test]
    fn bad_debt_continuation_caps_stateful_oracle_pricing_at_four_assets() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        let mut assets = Vec::new(&env);
        for _ in 0..5 {
            let asset = Address::generate(&env);
            oracle.set_price(
                &asset,
                &PriceQuote {
                    decimals: 7,
                    price: lending::SCALAR_7,
                    valid_until: u64::MAX,
                },
            );
            assets.push_back(asset);
        }
        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &assets,
        );
        env.as_contract(&pool_id, || {
            let mut backstop_positions = Positions::empty(&env);
            for (index, asset) in assets.iter().enumerate() {
                backstop_positions
                    .liabilities
                    .set(index as u32, lending::SCALAR_7);
                set_backstop_liability(&env, &asset, lending::SCALAR_7);
            }
            lending::set_positions(&env, &backstop_id, &backstop_positions);
        });
        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 5 * lending::SCALAR_7,
            debt_value: 0,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 5 * lending::SCALAR_7,
            target_value: 5 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        });

        let pool = PoolContractClient::new(&env, &pool_id);
        let continuation = pool.continue_bad_debt_resolution(&BytesN::from_array(&env, &[34; 32]));
        assert!(continuation.auction_created);
        assert!(continuation.defaulted.is_empty());
        let bid = pool.get_bad_debt_auction().bid;
        assert_eq!(bid.len(), 4);
        for index in 0..4 {
            assert_eq!(bid.get(assets.get(index).unwrap()), Some(lending::SCALAR_7));
        }
        assert_eq!(bid.get(assets.get(4).unwrap()), None);
        assert_eq!(backstop.committed_debt(), 4 * lending::SCALAR_7);
        assert_eq!(pool.positions(&backstop_id).liabilities.len(), 5);
    }

    #[test]
    fn bad_debt_continuation_rejects_corrupt_canonical_liabilities_atomically() {
        for case in 0..4_u32 {
            let env = Env::default();
            let (pool_id, backstop_id, first_debt_asset, second_debt_asset, _) =
                setup_backstop_bad_debt_pool(&env);
            let pool = PoolContractClient::new(&env, &pool_id);
            let unknown_asset = Address::generate(&env);

            env.as_contract(&pool_id, || match case {
                // Missing canonical record for an existing position.
                0 => set_backstop_liability(&env, &first_debt_asset, 0),
                // Canonical amount disagrees with the position.
                1 => set_backstop_liability(&env, &first_debt_asset, 50 * lending::SCALAR_7 + 1),
                // Canonical record references an unknown reserve.
                2 => set_backstop_liability(&env, &unknown_asset, lending::SCALAR_7),
                // Negative state is injected directly to exercise the
                // continuation's fail-closed validation boundary.
                3 => {
                    let mut backstop_positions = lending::positions(&env, &backstop_id);
                    backstop_positions
                        .liabilities
                        .set(1, -(50 * lending::SCALAR_7));
                    lending::set_positions(&env, &backstop_id, &backstop_positions);
                    let mut records = get_loss_records(&env);
                    records
                        .liabilities
                        .set(first_debt_asset.clone(), -(50 * lending::SCALAR_7));
                    set_loss_records(&env, &records);
                }
                _ => unreachable!(),
            });

            let positions_before = pool.positions(&backstop_id);
            let records_before = env.as_contract(&pool_id, || get_loss_records(&env));
            let first_reserve_before = pool.reserve(&first_debt_asset).data;
            let second_reserve_before = pool.reserve(&second_debt_asset).data;
            let loss_state_before = pool.backstop_loss_state();
            assert!(!pool.backstop_withdrawal_allowed(&backstop_id));

            assert!(
                pool.try_continue_bad_debt_resolution(&BytesN::from_array(
                    &env,
                    &[35 + case as u8; 32],
                ))
                .is_err(),
                "corruption case {case} must fail"
            );

            assert_eq!(pool.positions(&backstop_id), positions_before);
            assert_eq!(
                env.as_contract(&pool_id, || get_loss_records(&env)),
                records_before
            );
            assert_eq!(pool.reserve(&first_debt_asset).data, first_reserve_before);
            assert_eq!(pool.reserve(&second_debt_asset).data, second_reserve_before);
            assert_eq!(pool.backstop_loss_state(), loss_state_before);
            assert!(!pool.backstop_withdrawal_allowed(&backstop_id));
            assert!(pool.try_get_bad_debt_auction().is_err());
        }
    }

    #[test]
    fn bad_debt_continuation_defaults_all_residual_debt_to_suppliers() {
        let env = Env::default();
        let (pool_id, backstop_id, first_debt_asset, second_debt_asset, supplier) =
            setup_backstop_bad_debt_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let first_supply_shares = pool.positions(&supplier).supply.get(1).unwrap();
        let second_supply_shares = pool.positions(&supplier).supply.get(2).unwrap();
        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 150 * lending::SCALAR_7,
            debt_value: 125 * lending::SCALAR_7,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 150 * lending::SCALAR_7,
            target_value: 150 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: env.ledger().timestamp() - 1,
        });
        assert!(pool
            .try_continue_bad_debt_resolution(&BytesN::from_array(&env, &[32; 32]))
            .is_err());
        assert_eq!(
            pool.positions(&backstop_id).liabilities.len(),
            2,
            "an invalid tier quote must not trigger supplier loss"
        );
        assert_eq!(
            pool.reserve(&first_debt_asset).data.b_rate,
            lending::SCALAR_12
        );
        assert!(pool.try_get_bad_debt_auction().is_err());

        backstop.clear_bad_debt_quote();
        let continuation = pool.continue_bad_debt_resolution(&BytesN::from_array(&env, &[33; 32]));
        assert!(!continuation.auction_created);
        assert_eq!(
            continuation.defaulted,
            Map::from_array(
                &env,
                [
                    (first_debt_asset.clone(), 50 * lending::SCALAR_7),
                    (second_debt_asset.clone(), 75 * lending::SCALAR_7),
                ],
            )
        );
        assert!(pool.try_get_bad_debt_auction().is_err());
        assert!(pool.positions(&backstop_id).liabilities.is_empty());
        assert_eq!(
            pool.positions(&supplier).supply.get(1),
            Some(first_supply_shares)
        );
        assert_eq!(
            pool.positions(&supplier).supply.get(2),
            Some(second_supply_shares)
        );

        let first_reserve = pool.reserve(&first_debt_asset).data;
        assert_eq!(first_reserve.d_supply, 0);
        assert_eq!(first_reserve.b_supply, 1_000 * lending::SCALAR_7);
        assert_eq!(first_reserve.b_rate, 950_000_000_000);
        let second_reserve = pool.reserve(&second_debt_asset).data;
        assert_eq!(second_reserve.d_supply, 0);
        assert_eq!(second_reserve.b_supply, 1_000 * lending::SCALAR_7);
        assert_eq!(second_reserve.b_rate, 925_000_000_000);
        assert_eq!(
            TokenClient::new(&env, &first_debt_asset).balance(&pool_id),
            950 * lending::SCALAR_7
        );
        assert_eq!(
            TokenClient::new(&env, &second_debt_asset).balance(&pool_id),
            925 * lending::SCALAR_7
        );
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 0,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(pool.backstop_withdrawal_allowed(&backstop_id));
    }

    #[test]
    fn completed_liquidation_hands_collateral_free_debt_to_backstop() {
        let env = Env::default();
        let (pool_id, oracle_id, collateral_asset, borrowed_asset, borrower, filler) =
            setup_liquidation_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 5_000_000,
                valid_until: u64::MAX,
            },
        );
        let auction = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset.clone()],
            &vec![&env, collateral_asset],
            &100,
        );
        env.ledger().set_sequence_number(auction.block + 300);
        let borrower_before = pool.positions(&borrower);
        let filler_before = pool.positions(&filler);

        // An incomplete fill cannot hand off debt, so the pre-handoff
        // fail-closed health boundary remains in force.
        assert!(pool
            .try_submit(
                &filler,
                &filler,
                &filler,
                &vec![
                    &env,
                    Request {
                        address: borrower.clone(),
                        amount: 50,
                        request_type: ACTION_FILL_USER_LIQUIDATION,
                    }
                ],
            )
            .is_err());
        assert_eq!(pool.positions(&borrower), borrower_before);
        assert_eq!(pool.positions(&filler), filler_before);
        assert_eq!(pool.get_liquidation_auction(&borrower), auction);

        let d_supply_before = pool.reserve(&borrowed_asset).data.d_supply;
        let filler_after = pool.submit(
            &filler,
            &filler,
            &filler,
            &vec![
                &env,
                Request {
                    address: borrower.clone(),
                    amount: 100,
                    request_type: ACTION_FILL_USER_LIQUIDATION,
                },
            ],
        );
        assert!(env
            .events()
            .all()
            .filter_by_contract(&pool_id)
            .events()
            .contains(
                &UserBadDebtTransferred {
                    asset: borrowed_asset.clone(),
                    backstop: filler.clone(),
                    d_tokens: 35 * lending::SCALAR_7,
                    user: borrower.clone(),
                }
                .to_xdr(&env, &pool_id)
            ));

        assert!(pool.try_get_liquidation_auction(&borrower).is_err());
        assert!(pool.positions(&borrower).collateral.is_empty());
        assert!(pool.positions(&borrower).liabilities.is_empty());
        assert_eq!(
            filler_after.liabilities.get(1),
            borrower_before.liabilities.get(1)
        );
        assert_eq!(pool.reserve(&borrowed_asset).data.d_supply, d_supply_before);
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&filler));
    }

    fn assert_failed_liquidation_handoff_rolls_back(
        env: &Env,
        pool_id: &Address,
        borrowed_asset: &Address,
        borrower: &Address,
        filler: &Address,
        auction: &AuctionData,
    ) {
        let pool = PoolContractClient::new(env, pool_id);
        let borrower_before = pool.positions(borrower);
        let filler_before = pool.positions(filler);
        let d_supply_before = pool.reserve(borrowed_asset).data.d_supply;
        let records_before = env.as_contract(pool_id, || get_loss_records(env));
        let events_before = env.events().all();

        assert!(pool
            .try_submit(
                filler,
                filler,
                filler,
                &vec![
                    env,
                    Request {
                        address: borrower.clone(),
                        amount: 100,
                        request_type: ACTION_FILL_USER_LIQUIDATION,
                    }
                ],
            )
            .is_err());

        assert_eq!(pool.positions(borrower), borrower_before);
        assert_eq!(pool.positions(filler), filler_before);
        assert_eq!(pool.reserve(borrowed_asset).data.d_supply, d_supply_before);
        assert_eq!(pool.get_liquidation_auction(borrower), *auction);
        assert_eq!(
            env.as_contract(pool_id, || get_loss_records(env)),
            records_before
        );
        assert_eq!(env.events().all().events(), events_before.events());
    }

    #[test]
    fn liquidation_handoff_capacity_failure_rolls_back_atomically() {
        let env = Env::default();
        let (pool_id, oracle_id, collateral_asset, borrowed_asset, borrower, filler) =
            setup_liquidation_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 5_000_000,
                valid_until: u64::MAX,
            },
        );
        let auction = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset.clone()],
            &vec![&env, collateral_asset],
            &100,
        );
        env.ledger()
            .set_sequence_number(auction.block.checked_add(300).unwrap());

        // Fill the bounded canonical map with test-only records so adding the
        // handoff reserve fails inside the same submit transaction.
        env.as_contract(&pool_id, || {
            for _ in 0..MAX_LOSS_RECORDS_PER_KIND {
                set_backstop_liability(&env, &Address::generate(&env), 1);
            }
        });

        assert_failed_liquidation_handoff_rolls_back(
            &env,
            &pool_id,
            &borrowed_asset,
            &borrower,
            &filler,
            &auction,
        );
    }

    #[test]
    fn liquidation_handoff_overflow_rolls_back_atomically() {
        let env = Env::default();
        let (pool_id, oracle_id, collateral_asset, borrowed_asset, borrower, filler) =
            setup_liquidation_pool(&env);
        let pool = PoolContractClient::new(&env, &pool_id);
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 5_000_000,
                valid_until: u64::MAX,
            },
        );
        let auction = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, borrowed_asset.clone()],
            &vec![&env, collateral_asset],
            &100,
        );
        env.ledger()
            .set_sequence_number(auction.block.checked_add(300).unwrap());

        // The late auction fill first transfers 35 dTokens to the configured
        // backstop, reaching i128::MAX, then the 35-dToken residual handoff
        // must fail checked addition. This deliberately corrupts only the
        // test pre-state; valid lending flows conserve dToken supply and
        // cannot originate this balance.
        let injected_liability = i128::MAX.checked_sub(35 * lending::SCALAR_7).unwrap();
        let mut filler_positions = pool.positions(&filler);
        filler_positions.liabilities.set(1, injected_liability);
        env.as_contract(&pool_id, || {
            lending::set_positions(&env, &filler, &filler_positions);
            set_backstop_liability(&env, &borrowed_asset, injected_liability);
        });

        assert_failed_liquidation_handoff_rolls_back(
            &env,
            &pool_id,
            &borrowed_asset,
            &borrower,
            &filler,
            &auction,
        );
    }

    #[test]
    fn liquidation_handoff_accumulates_debt_without_resizing_active_auction() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);

        let token_admin = Address::generate(&env);
        let collateral_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let first_debt_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let second_debt_asset = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let collateral_admin = StellarAssetClient::new(&env, &collateral_asset);
        let first_debt_admin = StellarAssetClient::new(&env, &first_debt_asset);
        let second_debt_admin = StellarAssetClient::new(&env, &second_debt_asset);
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        for asset in [
            collateral_asset.clone(),
            first_debt_asset.clone(),
            second_debt_asset.clone(),
        ] {
            oracle.set_price(
                &asset,
                &PriceQuote {
                    decimals: 7,
                    price: lending::SCALAR_7,
                    valid_until: u64::MAX,
                },
            );
        }

        let pool_id = register_lending_pool(
            &env,
            &backstop_id,
            &Address::generate(&env),
            &oracle_id,
            &vec![
                &env,
                collateral_asset.clone(),
                first_debt_asset.clone(),
                second_debt_asset.clone(),
            ],
        );
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let pool = PoolContractClient::new(&env, &pool_id);
        let supplier = Address::generate(&env);
        let borrower = Address::generate(&env);
        let filler = Address::generate(&env);

        first_debt_admin.mint(&supplier, &(1_000 * lending::SCALAR_7));
        second_debt_admin.mint(&supplier, &(1_000 * lending::SCALAR_7));
        collateral_admin.mint(&backstop_id, &(200 * lending::SCALAR_7));
        collateral_admin.mint(&borrower, &(100 * lending::SCALAR_7));
        collateral_admin.mint(&filler, &(200 * lending::SCALAR_7));
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &vec![
                &env,
                Request {
                    address: first_debt_asset.clone(),
                    amount: 1_000 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY,
                },
                Request {
                    address: second_debt_asset.clone(),
                    amount: 1_000 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY,
                },
            ],
        );
        pool.submit(
            &backstop_id,
            &backstop_id,
            &backstop_id,
            &vec![
                &env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 200 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: first_debt_asset.clone(),
                    amount: 10 * lending::SCALAR_7,
                    request_type: ACTION_BORROW,
                },
            ],
        );
        pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 100 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: first_debt_asset.clone(),
                    amount: 40 * lending::SCALAR_7,
                    request_type: ACTION_BORROW,
                },
                Request {
                    address: second_debt_asset.clone(),
                    amount: 30 * lending::SCALAR_7,
                    request_type: ACTION_BORROW,
                },
            ],
        );
        pool.submit(
            &filler,
            &filler,
            &filler,
            &vec![
                &env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 200 * lending::SCALAR_7,
                    request_type: ACTION_SUPPLY_COLLATERAL,
                },
            ],
        );

        backstop.set_bad_debt_quote(&BadDebtLotQuote {
            committed_value: 12 * lending::SCALAR_7,
            debt_value: 0,
            tier: BackstopTier::BlndUsdc,
            lot_amount: 12 * lending::SCALAR_7,
            target_value: 12 * lending::SCALAR_7,
            unfilled_target_value: 0,
            valid_until: u64::MAX,
        });
        let existing_auction = pool.new_bad_debt_auction(
            &BytesN::from_array(&env, &[41; 32]),
            &vec![&env, first_debt_asset.clone()],
        );

        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 5_000_000,
                valid_until: u64::MAX,
            },
        );
        let liquidation = pool.new_liquidation_auction(
            &borrower,
            &vec![&env, first_debt_asset.clone(), second_debt_asset.clone()],
            &vec![&env, collateral_asset],
            &100,
        );
        env.ledger()
            .set_sequence_number(liquidation.block.checked_add(300).unwrap());
        pool.submit(
            &filler,
            &filler,
            &filler,
            &vec![
                &env,
                Request {
                    address: borrower.clone(),
                    amount: 100,
                    request_type: ACTION_FILL_USER_LIQUIDATION,
                },
            ],
        );

        assert!(pool.positions(&borrower).liabilities.is_empty());
        assert_eq!(
            pool.positions(&backstop_id).liabilities,
            Map::from_array(
                &env,
                [(1, 30 * lending::SCALAR_7), (2, 15 * lending::SCALAR_7),],
            )
        );
        assert_eq!(pool.get_bad_debt_auction(), existing_auction);
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 1,
                liability_entries: 2,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&backstop_id));
    }

    #[test]
    fn lending_core_executes_atomic_supply_borrow_repay_and_withdraw() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().set_timestamp(1_000);

        let token_admin = Address::generate(&env);
        let collateral_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let borrowed_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let collateral_admin = StellarAssetClient::new(&env, &collateral_asset);
        let borrowed_admin = StellarAssetClient::new(&env, &borrowed_asset);
        let collateral_token = TokenClient::new(&env, &collateral_asset);
        let borrowed_token = TokenClient::new(&env, &borrowed_asset);

        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 20_000_000,
                valid_until: u64::MAX,
            },
        );
        oracle.set_price(
            &borrowed_asset,
            &PriceQuote {
                decimals: 7,
                price: 10_000_000,
                valid_until: u64::MAX,
            },
        );

        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_lending_pool(
            &env,
            &backstop,
            &operator,
            &oracle_id,
            &vec![&env, collateral_asset.clone(), borrowed_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });

        let liquidity_provider = Address::generate(&env);
        let borrower = backstop.clone();
        borrowed_admin.mint(&liquidity_provider, &1_000_000_000);
        collateral_admin.mint(&borrower, &1_000_000_000);
        borrowed_admin.mint(&borrower, &250_000_000);

        pool.submit(
            &liquidity_provider,
            &liquidity_provider,
            &liquidity_provider,
            &vec![
                &env,
                Request {
                    address: borrowed_asset.clone(),
                    amount: 1_000_000_000,
                    request_type: lending::ACTION_SUPPLY,
                },
            ],
        );
        let opened = pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 1_000_000_000,
                    request_type: lending::ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: borrowed_asset.clone(),
                    amount: 200_000_000,
                    request_type: lending::ACTION_BORROW,
                },
            ],
        );
        assert_eq!(opened.collateral.get(0), Some(1_000_000_000));
        assert_eq!(opened.liabilities.get(1), Some(200_000_000));
        assert_eq!(borrowed_token.balance(&borrower), 450_000_000);
        assert_eq!(
            pool.position_health(&borrower),
            PositionHealth {
                collateral_base: 1_600_000_000,
                healthy: true,
                liability_base: 222_222_223,
                raw_collateral_base: 2_000_000_000,
            }
        );
        assert_eq!(pool.backstop_loss_state().liability_entries, 1);
        assert!(!pool.backstop_withdrawal_allowed(&backstop));

        let repaid = pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: borrowed_asset.clone(),
                    amount: 250_000_000,
                    request_type: lending::ACTION_REPAY,
                },
            ],
        );
        assert!(repaid.liabilities.is_empty());
        assert_eq!(borrowed_token.balance(&borrower), 250_000_000);
        assert_eq!(pool.backstop_loss_state().liability_entries, 0);
        assert!(pool.backstop_withdrawal_allowed(&backstop));

        let closed = pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 1_000_000_000,
                    request_type: lending::ACTION_WITHDRAW_COLLATERAL,
                },
            ],
        );
        assert!(closed.collateral.is_empty());
        assert_eq!(collateral_token.balance(&borrower), 1_000_000_000);

        let remaining_supply = pool.submit(
            &liquidity_provider,
            &liquidity_provider,
            &liquidity_provider,
            &vec![
                &env,
                Request {
                    address: borrowed_asset,
                    amount: 500_000_000,
                    request_type: lending::ACTION_WITHDRAW,
                },
            ],
        );
        assert_eq!(remaining_supply.supply.get(1), Some(500_000_000));
        assert_eq!(borrowed_token.balance(&liquidity_provider), 500_000_000);
        assert_eq!(pool.reserve_list().len(), 2);
        assert_eq!(pool.lending_config().oracle, oracle_id);
    }

    #[test]
    fn lending_core_enforces_status_auth_health_prices_and_interest() {
        let env = Env::default();
        env.ledger().set_timestamp(1_000);
        let token_admin = Address::generate(&env);
        let collateral_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let borrowed_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let collateral_admin = StellarAssetClient::new(&env, &collateral_asset);
        let borrowed_admin = StellarAssetClient::new(&env, &borrowed_asset);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        env.mock_all_auths_allowing_non_root_auth();
        for asset in [collateral_asset.clone(), borrowed_asset.clone()] {
            oracle.set_price(
                &asset,
                &PriceQuote {
                    decimals: 7,
                    price: 10_000_000,
                    valid_until: u64::MAX,
                },
            );
        }
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_lending_pool(
            &env,
            &backstop,
            &operator,
            &oracle_id,
            &vec![&env, collateral_asset.clone(), borrowed_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        let supplier = Address::generate(&env);
        let borrower = Address::generate(&env);
        borrowed_admin.mint(&supplier, &1_000_000_000);
        collateral_admin.mint(&borrower, &1_000_000_000);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &vec![
                &env,
                Request {
                    address: borrowed_asset.clone(),
                    amount: 1_000_000_000,
                    request_type: lending::ACTION_SUPPLY,
                },
            ],
        );
        pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: collateral_asset.clone(),
                    amount: 1_000_000_000,
                    request_type: lending::ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: borrowed_asset.clone(),
                    amount: 100_000_000,
                    request_type: lending::ACTION_BORROW,
                },
            ],
        );

        env.ledger().set_timestamp(1_000 + 31_536_000);
        let accrued = pool.reserve(&borrowed_asset);
        assert!(accrued.data.d_rate > lending::SCALAR_12);
        assert!(accrued.data.b_rate > lending::SCALAR_12);
        assert!(accrued.data.backstop_credit > 0);

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &3_u32);
        });
        assert!(pool
            .try_submit(
                &borrower,
                &borrower,
                &borrower,
                &vec![
                    &env,
                    Request {
                        address: borrowed_asset.clone(),
                        amount: 1,
                        request_type: lending::ACTION_BORROW,
                    }
                ],
            )
            .is_err());

        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let before_failed_health = pool.positions(&borrower);
        for invalid_quote in [
            PriceQuote {
                decimals: 7,
                price: 10_000_000,
                valid_until: env.ledger().timestamp() - 1,
            },
            PriceQuote {
                decimals: 6,
                price: 10_000_000,
                valid_until: u64::MAX,
            },
            PriceQuote {
                decimals: 7,
                price: 0,
                valid_until: u64::MAX,
            },
        ] {
            oracle.set_price(&collateral_asset, &invalid_quote);
            assert!(pool
                .try_submit(
                    &borrower,
                    &borrower,
                    &borrower,
                    &vec![
                        &env,
                        Request {
                            address: collateral_asset.clone(),
                            amount: 1,
                            request_type: lending::ACTION_WITHDRAW_COLLATERAL,
                        }
                    ],
                )
                .is_err());
            assert_eq!(pool.positions(&borrower), before_failed_health);
        }
        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 1,
                valid_until: u64::MAX,
            },
        );
        assert!(pool
            .try_submit(
                &borrower,
                &borrower,
                &borrower,
                &vec![
                    &env,
                    Request {
                        address: collateral_asset.clone(),
                        amount: 1,
                        request_type: lending::ACTION_WITHDRAW_COLLATERAL,
                    }
                ],
            )
            .is_err());

        assert_eq!(pool.status(), 1);
    }

    #[test]
    fn lending_submit_requires_spender_and_position_owner_authorization() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let from = Address::generate(&env);
        let spender = Address::generate(&env);
        let to = Address::generate(&env);
        let requests = vec![
            &env,
            Request {
                address: Address::generate(&env),
                amount: 1,
                request_type: lending::ACTION_SUPPLY,
            },
        ];

        assert!(pool.try_submit(&from, &spender, &to, &requests).is_err());
        assert!(pool
            .mock_auths(&[MockAuth {
                address: &spender,
                invoke: &MockAuthInvoke {
                    contract: &pool_id,
                    fn_name: "submit",
                    args: (&from, &spender, &to, requests.clone()).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .try_submit(&from, &spender, &to, &requests)
            .is_err());
    }

    #[test]
    fn lending_failures_roll_back_caps_transfers_and_disabled_reserves() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        let token_admin = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let asset_admin = StellarAssetClient::new(&env, &asset);
        let asset_token = TokenClient::new(&env, &asset);
        let oracle_id = env.register(MockPriceOracle, ());
        MockPriceOracleClient::new(&env, &oracle_id).set_price(
            &asset,
            &PriceQuote {
                decimals: 7,
                price: 10_000_000,
                valid_until: u64::MAX,
            },
        );
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 2,
            min_collateral: 0,
            oracle: oracle_id,
            price_decimals: 7,
        };
        let mut capped_reserve = reserve_config(&asset, 0);
        capped_reserve.supply_cap = 50 * lending::SCALAR_7;
        let capped_pool_id = register_custom_lending_pool(
            &env,
            &backstop,
            &operator,
            &lending_config,
            &vec![&env, capped_reserve],
        );
        let capped_pool = PoolContractClient::new(&env, &capped_pool_id);
        env.as_contract(&capped_pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let user = Address::generate(&env);
        asset_admin.mint(&user, &(100 * lending::SCALAR_7));
        assert!(capped_pool
            .try_submit(
                &user,
                &user,
                &user,
                &vec![
                    &env,
                    Request {
                        address: asset.clone(),
                        amount: 60 * lending::SCALAR_7,
                        request_type: lending::ACTION_SUPPLY,
                    }
                ],
            )
            .is_err());
        assert_eq!(asset_token.balance(&user), 100 * lending::SCALAR_7);
        assert_eq!(asset_token.balance(&capped_pool_id), 0);
        assert!(capped_pool.positions(&user).supply.is_empty());
        assert_eq!(capped_pool.reserve(&asset).data.b_supply, 0);
        assert_eq!(env.events().all().events().len(), 0);

        let mut enabled_reserve = reserve_config(&asset, 0);
        enabled_reserve.supply_cap = 1_000 * lending::SCALAR_7;
        let transfer_pool_id = register_custom_lending_pool(
            &env,
            &backstop,
            &operator,
            &lending_config,
            &vec![&env, enabled_reserve],
        );
        let transfer_pool = PoolContractClient::new(&env, &transfer_pool_id);
        env.as_contract(&transfer_pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        assert!(transfer_pool
            .try_submit(
                &user,
                &user,
                &user,
                &vec![
                    &env,
                    Request {
                        address: asset.clone(),
                        amount: 101 * lending::SCALAR_7,
                        request_type: lending::ACTION_SUPPLY,
                    }
                ],
            )
            .is_err());
        assert!(transfer_pool.positions(&user).supply.is_empty());
        assert_eq!(transfer_pool.reserve(&asset).data.b_supply, 0);
        let mut too_many_requests = Vec::new(&env);
        for _ in 0..=60 {
            too_many_requests.push_back(Request {
                address: asset.clone(),
                amount: 1,
                request_type: lending::ACTION_SUPPLY,
            });
        }
        assert!(transfer_pool
            .try_submit(&user, &user, &user, &too_many_requests)
            .is_err());
        assert!(transfer_pool.positions(&user).supply.is_empty());

        let fee_token_id = env.register(MockFeeToken, ());
        let fee_token = MockFeeTokenClient::new(&env, &fee_token_id);
        let fee_pool_id = register_custom_lending_pool(
            &env,
            &backstop,
            &operator,
            &lending_config,
            &vec![&env, reserve_config(&fee_token_id, 0)],
        );
        let fee_pool = PoolContractClient::new(&env, &fee_pool_id);
        env.as_contract(&fee_pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        fee_token.mint(&user, &(10 * lending::SCALAR_7));
        assert!(fee_pool
            .try_submit(
                &user,
                &user,
                &user,
                &vec![
                    &env,
                    Request {
                        address: fee_token_id,
                        amount: 10 * lending::SCALAR_7,
                        request_type: lending::ACTION_SUPPLY,
                    }
                ],
            )
            .is_err());
        assert_eq!(fee_token.balance(&user), 10 * lending::SCALAR_7);
        assert_eq!(fee_token.balance(&fee_pool_id), 0);
        assert!(fee_pool.positions(&user).supply.is_empty());

        let mut disabled_reserve = reserve_config(&asset, 0);
        disabled_reserve.enabled = false;
        let disabled_pool_id = register_custom_lending_pool(
            &env,
            &backstop,
            &operator,
            &lending_config,
            &vec![&env, disabled_reserve],
        );
        let disabled_pool = PoolContractClient::new(&env, &disabled_pool_id);
        env.as_contract(&disabled_pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        assert!(disabled_pool
            .try_submit(
                &user,
                &user,
                &user,
                &vec![
                    &env,
                    Request {
                        address: asset,
                        amount: lending::SCALAR_7,
                        request_type: lending::ACTION_SUPPLY,
                    }
                ],
            )
            .is_err());
    }

    #[test]
    fn lending_enforces_maximum_utilization_boundary_atomically() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        let token_admin = Address::generate(&env);
        let collateral_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let borrowed_asset = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let collateral_admin = StellarAssetClient::new(&env, &collateral_asset);
        let borrowed_admin = StellarAssetClient::new(&env, &borrowed_asset);
        let oracle_id = env.register(MockPriceOracle, ());
        let oracle = MockPriceOracleClient::new(&env, &oracle_id);
        oracle.set_price(
            &collateral_asset,
            &PriceQuote {
                decimals: 7,
                price: 100 * lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        oracle.set_price(
            &borrowed_asset,
            &PriceQuote {
                decimals: 7,
                price: lending::SCALAR_7,
                valid_until: u64::MAX,
            },
        );
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_lending_pool(
            &env,
            &backstop,
            &operator,
            &oracle_id,
            &vec![&env, collateral_asset.clone(), borrowed_asset.clone()],
        );
        let pool = PoolContractClient::new(&env, &pool_id);
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let supplier = Address::generate(&env);
        let borrower = Address::generate(&env);
        borrowed_admin.mint(&supplier, &(100 * lending::SCALAR_7));
        collateral_admin.mint(&borrower, &(10 * lending::SCALAR_7));
        pool.submit(
            &supplier,
            &supplier,
            &supplier,
            &vec![
                &env,
                Request {
                    address: borrowed_asset.clone(),
                    amount: 100 * lending::SCALAR_7,
                    request_type: lending::ACTION_SUPPLY,
                },
            ],
        );
        pool.submit(
            &borrower,
            &borrower,
            &borrower,
            &vec![
                &env,
                Request {
                    address: collateral_asset,
                    amount: 10 * lending::SCALAR_7,
                    request_type: lending::ACTION_SUPPLY_COLLATERAL,
                },
                Request {
                    address: borrowed_asset.clone(),
                    amount: 95 * lending::SCALAR_7,
                    request_type: lending::ACTION_BORROW,
                },
            ],
        );
        let before = pool.positions(&borrower);
        assert!(pool
            .try_submit(
                &borrower,
                &borrower,
                &borrower,
                &vec![
                    &env,
                    Request {
                        address: borrowed_asset,
                        amount: 1,
                        request_type: lending::ACTION_BORROW,
                    }
                ],
            )
            .is_err());
        assert_eq!(pool.positions(&borrower), before);
    }

    #[test]
    fn lending_enforces_maximum_positions_atomically() {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        let token_admin = Address::generate(&env);
        let mut assets = Vec::new(&env);
        for _ in 0..3 {
            assets.push_back(
                env.register_stellar_asset_contract_v2(token_admin.clone())
                    .address(),
            );
        }
        let oracle = Address::generate(&env);
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 2,
            min_collateral: 0,
            oracle,
            price_decimals: 7,
        };
        let mut reserves = Vec::new(&env);
        for asset in assets.iter() {
            reserves.push_back(reserve_config(&asset, reserves.len()));
        }
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id =
            register_custom_lending_pool(&env, &backstop, &operator, &lending_config, &reserves);
        let pool = PoolContractClient::new(&env, &pool_id);
        env.as_contract(&pool_id, || {
            env.storage().instance().set(&DataKey::Status, &1_u32);
        });
        let user = Address::generate(&env);
        let mut requests = Vec::new(&env);
        for asset in assets {
            StellarAssetClient::new(&env, &asset).mint(&user, &lending::SCALAR_7);
            requests.push_back(Request {
                address: asset,
                amount: lending::SCALAR_7,
                request_type: lending::ACTION_SUPPLY_COLLATERAL,
            });
        }
        assert!(pool.try_submit(&user, &user, &user, &requests).is_err());
        assert!(pool.positions(&user).collateral.is_empty());
        for asset in pool.reserve_list() {
            assert_eq!(TokenClient::new(&env, &asset).balance(&pool_id), 0);
            assert_eq!(pool.reserve(&asset).data.b_supply, 0);
        }
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1305)")]
    fn lending_constructor_rejects_invalid_initial_configuration() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let lending_config = LendingConfig {
            backstop_take_rate: 0,
            max_positions: 1,
            min_collateral: 0,
            oracle: Address::generate(&env),
            price_decimals: 7,
        };
        register_custom_lending_pool(&env, &backstop, &operator, &lending_config, &Vec::new(&env));
    }

    #[test]
    fn withdrawal_gate_tracks_each_canonical_record_kind() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let other = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);

        assert!(pool.backstop_withdrawal_allowed(&backstop));
        assert!(!pool.backstop_withdrawal_allowed(&other));

        let reserve = Address::generate(&env);
        env.as_contract(&pool_id, || {
            set_backstop_liability(&env, &reserve, 100);
        });
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&backstop));

        env.as_contract(&pool_id, || {
            set_backstop_liability(&env, &reserve, 0);
            set_unresolved_bad_debt(&env, &reserve, 50);
        });
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 0,
                unresolved_bad_debt_entries: 1,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&backstop));

        let auction = BytesN::from_array(&env, &[7; 32]);
        env.as_contract(&pool_id, || {
            set_unresolved_bad_debt(&env, &reserve, 0);
            set_committed_loss(&env, &auction, true);
        });
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 1,
                liability_entries: 0,
                unresolved_bad_debt_entries: 0,
            }
        );
        assert!(!pool.backstop_withdrawal_allowed(&backstop));

        env.as_contract(&pool_id, || {
            set_committed_loss(&env, &auction, false);
        });
        assert!(pool.backstop_withdrawal_allowed(&backstop));
    }

    #[test]
    fn missing_loss_records_fail_closed() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);

        env.as_contract(&pool_id, || {
            env.storage().instance().remove(&DataKey::LossRecords);
        });

        assert!(pool.try_backstop_withdrawal_allowed(&backstop).is_err());
    }

    #[test]
    fn updating_existing_records_does_not_change_counts() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);
        let reserve = Address::generate(&env);

        env.as_contract(&pool_id, || {
            set_backstop_liability(&env, &reserve, 100);
            set_backstop_liability(&env, &reserve, 40);
        });

        assert_eq!(pool.backstop_loss_state().liability_entries, 1);
    }

    #[test]
    fn failed_loss_transition_rolls_back_every_record() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);
        let pool = PoolContractClient::new(&env, &pool_id);
        let reserve = Address::generate(&env);
        let auction = BytesN::from_array(&env, &[8; 32]);

        assert!(pool
            .try_test_loss_transition(&reserve, &auction, &true)
            .is_err());
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 0,
                unresolved_bad_debt_entries: 0,
            }
        );

        pool.test_loss_transition(&reserve, &auction, &false);
        assert_eq!(
            pool.backstop_loss_state(),
            BackstopLossState {
                committed_loss_entries: 1,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            }
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1300)")]
    fn canonical_loss_records_reject_negative_amounts() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);
        let reserve = Address::generate(&env);

        env.as_contract(&pool_id, || {
            set_unresolved_bad_debt(&env, &reserve, -1);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1301)")]
    fn canonical_loss_records_are_bounded() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let pool_id = register_pool(&env, &backstop, &operator);

        env.as_contract(&pool_id, || {
            for _ in 0..MAX_LOSS_RECORDS_PER_KIND {
                set_backstop_liability(&env, &Address::generate(&env), 1);
            }
            set_backstop_liability(&env, &Address::generate(&env), 1);
        });
    }
}
