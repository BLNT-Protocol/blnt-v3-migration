use soroban_sdk::{
    contractevent, contracttype, panic_with_error, token, Address, BytesN, Env, Map, Vec,
};

use crate::{
    clear_backstop_liabilities, get_backstop, get_backstop_liabilities, get_backstop_liability,
    lending::{
        checked_add, checked_sub_nonnegative, checkpoint_reserve, config, consume_backstop_credit,
        default_liability_to_suppliers, mul_div_ceil, mul_div_floor, position_data_with_prices,
        positions, require_position_limits, reserve_config, reserve_list, set_positions,
        value_reserve_amounts, Positions, PriceQuote, SCALAR_7,
    },
    set_backstop_liabilities, set_backstop_liability, BackstopClient, PoolError,
};

pub const ACTION_FILL_USER_LIQUIDATION: u32 = 6;
pub const ACTION_DELETE_LIQUIDATION: u32 = 9;

const AUCTION_TTL_THRESHOLD: u32 = 45 * 17_280;
const AUCTION_TTL_BUMP: u32 = 46 * 17_280;
const AUCTION_STALE_LEDGERS: u32 = 500;
const AUCTION_PRICE_LEDGERS: u32 = 400;
const AUCTION_PRICE_HALF_LEDGERS: u32 = 200;
const AUCTION_MODIFIER_PER_LEDGER: i128 = 50_000;
const MAX_POST_LIQUIDATION_HF: i128 = 11_500_000;
const MIN_POST_LIQUIDATION_HF: i128 = 10_300_000;
/// Preserve ledger-footprint headroom for stateful per-reserve oracle reads
/// while the same continuation validates and, if necessary, defaults the
/// complete bounded reserve set.
const MAX_BAD_DEBT_BID_ASSETS: u32 = 4;
const MAX_INTEREST_LOT_ASSETS: u32 = 4;
const INTEREST_TIER_MINIMUM_VALUE_USDC: i128 = 200 * SCALAR_7;
const INTEREST_STATE_TTL_THRESHOLD: u32 = 179 * 17_280;
const INTEREST_STATE_TTL_BUMP: u32 = 180 * 17_280;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AuctionData {
    /// dToken shares assumed by the filler, keyed by reserve asset.
    pub bid: Map<Address, i128>,
    /// bToken collateral shares received by the filler, keyed by reserve asset.
    pub lot: Map<Address, i128>,
    /// Ledger on which fills may begin.
    pub block: u32,
}

/// The candidate's fixed three-tier backstop set in first-loss order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum BackstopTier {
    BlndUsdc,
    BlndXlm,
    Usdc,
}

/// Canonical lot quote returned by the configured backstop.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BadDebtLotQuote {
    pub committed_value: i128,
    pub debt_value: i128,
    pub tier: BackstopTier,
    pub lot_amount: i128,
    pub unfilled_target_value: i128,
    pub target_value: i128,
    pub valid_until: u64,
}

/// Prepared or partially filled bad-debt auction data.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BadDebtAuctionData {
    pub auction_id: BytesN<32>,
    /// Remaining base dToken shares, keyed by reserve asset.
    pub bid: Map<Address, i128>,
    /// Ledger on which filling may begin.
    pub block: u32,
    pub lot_quote: BadDebtLotQuote,
}

/// Exact amounts processed by one bad-debt auction fill.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BadDebtAuctionFill {
    pub auction_id: BytesN<32>,
    /// Base lot removed from the commitment before the time modifier.
    pub base_lot_amount: i128,
    /// Time-scaled dToken shares assumed by the filler.
    pub bid: Map<Address, i128>,
    pub block: u32,
    /// True when no base bid or lot remains in the auction.
    pub complete: bool,
    /// Time-scaled tier-token amount transferred to the filler.
    pub lot_amount: i128,
    pub tier: BackstopTier,
}

/// Result of one bounded permissionless bad-debt continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BadDebtContinuation {
    /// True when the supplied ID now identifies the next active auction.
    pub auction_created: bool,
    /// dToken shares defaulted to suppliers, keyed by reserve asset.
    pub defaulted: Map<Address, i128>,
}

/// Canonical tier-token bid quote returned by the configured backstop.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InterestLotQuote {
    pub bid_token: Address,
    pub bid_amount: i128,
    pub lot_value: i128,
    pub tier: BackstopTier,
    pub target_value: i128,
    pub valid_until: u64,
}

/// Canonical `5:5:4` allocation of one reserve-credit amount.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TakeRateQuote {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub remainder: i128,
    pub usdc: i128,
}

/// Pending reserve credit apportioned to each tier for one reserve asset.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InterestReserveState {
    pub blnd_usdc: i128,
    pub blnd_xlm: i128,
    pub carry: i128,
    pub usdc: i128,
}

impl InterestReserveState {
    pub(crate) fn empty() -> Self {
        Self {
            blnd_usdc: 0,
            blnd_xlm: 0,
            carry: 0,
            usdc: 0,
        }
    }

    fn total(&self, env: &Env) -> i128 {
        checked_add(
            env,
            checked_add(env, self.blnd_usdc, self.blnd_xlm),
            checked_add(env, self.usdc, self.carry),
        )
    }

    fn tier_amount(&self, tier: BackstopTier) -> i128 {
        match tier {
            BackstopTier::BlndUsdc => self.blnd_usdc,
            BackstopTier::BlndXlm => self.blnd_xlm,
            BackstopTier::Usdc => self.usdc,
        }
    }

    fn set_tier_amount(&mut self, tier: BackstopTier, amount: i128) {
        match tier {
            BackstopTier::BlndUsdc => self.blnd_usdc = amount,
            BackstopTier::BlndXlm => self.blnd_xlm = amount,
            BackstopTier::Usdc => self.usdc = amount,
        }
    }
}

/// One active tier-specific interest auction.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InterestAuctionData {
    pub auction_id: BytesN<32>,
    pub auction: AuctionData,
    pub bid_valid_until: u64,
    /// Remaining commitment value scaled with the remaining base bid.
    pub lot_value: i128,
    /// Immutable creation-time 120% USDC target.
    pub target_value: i128,
    pub tier: BackstopTier,
    /// Earliest expiry across the reserve-lot and bid-token quotes.
    pub valid_until: u64,
}

/// Exact base and time-scaled amounts processed by one interest fill.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct InterestAuctionFill {
    pub auction_id: BytesN<32>,
    pub base_bid_amount: i128,
    pub base_lot: Map<Address, i128>,
    pub bid_amount: i128,
    pub block: u32,
    pub complete: bool,
    pub lot: Map<Address, i128>,
    pub returned_lot: Map<Address, i128>,
    pub tier: BackstopTier,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionCreated {
    pub auction: AuctionData,
    pub percent: u32,
    pub user: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionFilled {
    pub auction: AuctionData,
    pub filler: Address,
    pub percent: u32,
    pub user: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionDeleted {
    pub stale: bool,
    pub user: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserBadDebtTransferred {
    pub asset: Address,
    pub backstop: Address,
    pub d_tokens: i128,
    pub user: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadDebtAuctionCreated {
    pub auction: BadDebtAuctionData,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadDebtAuctionDeleted {
    pub auction_id: BytesN<32>,
    pub stale: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BadDebtAuctionFilled {
    pub fill: BadDebtAuctionFill,
    pub filler: Address,
    pub percent: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplierDebtDefaulted {
    pub asset: Address,
    pub b_rate: i128,
    pub d_tokens: i128,
    pub previous_b_rate: i128,
    pub underlying: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestAuctionCreated {
    pub auction: InterestAuctionData,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestAuctionFilled {
    pub fill: InterestAuctionFill,
    pub filler: Address,
    pub percent: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestAuctionDeleted {
    pub auction_id: BytesN<32>,
    pub stale: bool,
}

#[derive(Clone)]
#[contracttype]
enum AuctionDataKey {
    UserLiquidation(Address),
    BadDebt,
    Interest,
}

#[derive(Clone)]
#[contracttype]
enum InterestDataKey {
    Cursor,
    Reserve(Address),
}

pub(crate) fn create_bad_debt_auction(
    env: &Env,
    auction_id: &BytesN<32>,
    bid: &Vec<Address>,
) -> BadDebtAuctionData {
    if has_bad_debt_auction(env) {
        panic_with_error!(env, PoolError::AuctionInProgress);
    }
    if bid.is_empty() || bid.len() >= config(env).max_positions {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }
    require_unique_addresses(env, bid);

    let (bid_amounts, debt_value) = build_bad_debt_bid(env, bid);
    commit_bad_debt_auction(env, auction_id, &bid_amounts, debt_value, None)
}

/// Continue the strict single-tier bad-debt waterfall without caller
/// authorization.
///
/// A distinct call keeps each fill or continuation bounded and leaves a
/// permissionless recovery point if no keeper submits the next transaction.
pub(crate) fn continue_bad_debt_resolution(
    env: &Env,
    auction_id: &BytesN<32>,
) -> BadDebtContinuation {
    if has_bad_debt_auction(env) {
        panic_with_error!(env, PoolError::AuctionInProgress);
    }

    let bid = canonical_bad_debt_bid(env);
    let (bid_amounts, debt_value) = build_bad_debt_bid(env, &bid);
    let backstop = get_backstop(env);
    let pool = env.current_contract_address();
    let quoted = BackstopClient::new(env, &backstop).quote_bad_debt_lot(&pool, &debt_value);

    if let Some(quote) = quoted {
        commit_bad_debt_auction(env, auction_id, &bid_amounts, debt_value, Some(&quote));
        BadDebtContinuation {
            auction_created: true,
            defaulted: Map::new(env),
        }
    } else {
        BadDebtContinuation {
            auction_created: false,
            defaulted: default_all_backstop_liabilities(env),
        }
    }
}

fn build_bad_debt_bid(env: &Env, bid: &Vec<Address>) -> (Map<Address, i128>, i128) {
    let backstop = get_backstop(env);
    let backstop_positions = positions(env, &backstop);
    let mut included = Positions::empty(env);
    let mut bid_amounts = Map::new(env);
    for asset in bid {
        let reserve = reserve_config(env, &asset);
        let amount = backstop_positions
            .liabilities
            .get(reserve.index)
            .unwrap_or(0);
        if amount <= 0 {
            panic_with_error!(env, PoolError::InvalidAuctionBid);
        }
        if get_backstop_liability(env, &asset) != amount {
            panic_with_error!(env, PoolError::InvalidAuctionBid);
        }
        included.liabilities.set(reserve.index, amount);
        bid_amounts.set(asset, amount);
    }
    let mut prices = Map::new(env);
    let debt_data = position_data_with_prices(env, &included, &mut prices);
    let debt_value = mul_div_ceil(
        env,
        debt_data.raw_liability_base,
        SCALAR_7,
        debt_data.scalar,
    );
    if debt_value <= 0 {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }
    (bid_amounts, debt_value)
}

fn commit_bad_debt_auction(
    env: &Env,
    auction_id: &BytesN<32>,
    bid_amounts: &Map<Address, i128>,
    debt_value: i128,
    expected_quote: Option<&BadDebtLotQuote>,
) -> BadDebtAuctionData {
    let backstop = get_backstop(env);
    let pool = env.current_contract_address();
    let lot_quote =
        BackstopClient::new(env, &backstop).commit_bad_debt_lot(&pool, auction_id, &debt_value);
    if lot_quote.debt_value != debt_value
        || lot_quote.committed_value <= 0
        || lot_quote.lot_amount <= 0
        || lot_quote.valid_until < env.ledger().timestamp()
    {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }
    if expected_quote.is_some_and(|quote| quote != &lot_quote) {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }
    let block = env
        .ledger()
        .sequence()
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let auction = BadDebtAuctionData {
        auction_id: auction_id.clone(),
        bid: bid_amounts.clone(),
        block,
        lot_quote,
    };
    set_bad_debt_auction(env, &auction);
    BadDebtAuctionCreated {
        auction: auction.clone(),
    }
    .publish(env);
    auction
}

fn canonical_bad_debt_bid(env: &Env) -> Vec<Address> {
    let backstop = get_backstop(env);
    let backstop_positions = positions(env, &backstop);
    let recorded = get_backstop_liabilities(env);
    let position_bound = config(env)
        .max_positions
        .checked_sub(1)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let max_bid = core::cmp::min(position_bound, MAX_BAD_DEBT_BID_ASSETS);
    let mut bid = Vec::new(env);
    let mut liability_count = 0_u32;

    for asset in reserve_list(env) {
        let reserve = reserve_config(env, &asset);
        let amount = backstop_positions
            .liabilities
            .get(reserve.index)
            .unwrap_or(0);
        if amount < 0 || get_backstop_liability(env, &asset) != amount {
            panic_with_error!(env, PoolError::InvalidAuctionBid);
        }
        if amount > 0 {
            liability_count = liability_count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
            if bid.len() < max_bid {
                bid.push_back(asset);
            }
        }
    }
    if liability_count == 0
        || recorded.len() != liability_count
        || backstop_positions.liabilities.len() != liability_count
    {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }
    bid
}

fn default_all_backstop_liabilities(env: &Env) -> Map<Address, i128> {
    let backstop = get_backstop(env);
    let mut backstop_positions = positions(env, &backstop);
    let mut defaulted = Map::new(env);

    for asset in reserve_list(env) {
        let reserve = reserve_config(env, &asset);
        let amount = backstop_positions
            .liabilities
            .get(reserve.index)
            .unwrap_or(0);
        if amount == 0 {
            continue;
        }
        let (underlying, previous_b_rate, b_rate) =
            default_liability_to_suppliers(env, &mut backstop_positions, &asset, amount);
        defaulted.set(asset.clone(), amount);
        SupplierDebtDefaulted {
            asset,
            b_rate,
            d_tokens: amount,
            previous_b_rate,
            underlying,
        }
        .publish(env);
    }

    if !backstop_positions.liabilities.is_empty() || defaulted.is_empty() {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }
    set_positions(env, &backstop, &backstop_positions);
    clear_backstop_liabilities(env);
    defaulted
}

pub(crate) fn get_bad_debt_auction(env: &Env) -> BadDebtAuctionData {
    env.storage()
        .temporary()
        .get(&AuctionDataKey::BadDebt)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::AuctionNotFound))
}

pub(crate) fn fill_bad_debt_auction(
    env: &Env,
    filler: &Address,
    percent: u32,
) -> BadDebtAuctionFill {
    let pool = env.current_contract_address();
    let backstop = get_backstop(env);
    if filler == &pool || filler == &backstop || percent == 0 || percent > 100 {
        panic_with_error!(env, PoolError::InvalidLiquidation);
    }
    filler.require_auth();
    if has_user_liquidation(env, filler) {
        panic_with_error!(env, PoolError::AuctionInProgress);
    }

    let auction = get_bad_debt_auction(env);
    let (fill, remaining_bid, remaining_lot_amount) =
        scale_bad_debt_auction(env, &auction, percent);
    let mut backstop_positions = positions(env, &backstop);
    let mut filler_positions = positions(env, filler);
    let filler_previous_count = filler_positions.effective_count();
    for (asset, _) in auction.bid.iter() {
        let reserve = reserve_config(env, &asset);
        if get_backstop_liability(env, &asset)
            != backstop_positions
                .liabilities
                .get(reserve.index)
                .unwrap_or(0)
        {
            panic_with_error!(env, PoolError::InvalidAuctionBid);
        }
    }
    remove_positions(env, &mut backstop_positions, &Map::new(env), &fill.bid);
    add_positions(env, &mut filler_positions, &Map::new(env), &fill.bid);
    let mut prices = Map::new(env);
    require_position_limits(env, &filler_positions, filler_previous_count, &mut prices);

    let remaining_quote = BackstopClient::new(env, &backstop).settle_bad_debt_lot(
        &pool,
        &auction.auction_id,
        &fill.base_lot_amount,
        &fill.lot_amount,
        filler,
    );
    if fill.complete != remaining_quote.is_none() {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }
    if let Some(remaining_quote) = remaining_quote {
        let expected_committed_value = mul_div_floor(
            env,
            auction.lot_quote.committed_value,
            remaining_lot_amount,
            auction.lot_quote.lot_amount,
        );
        if remaining_quote.lot_amount != remaining_lot_amount
            || remaining_quote.committed_value != expected_committed_value
            || remaining_quote.debt_value != auction.lot_quote.debt_value
            || remaining_quote.tier != auction.lot_quote.tier
            || remaining_quote.target_value != auction.lot_quote.target_value
            || remaining_quote.unfilled_target_value != auction.lot_quote.unfilled_target_value
            || remaining_quote.valid_until != auction.lot_quote.valid_until
        {
            panic_with_error!(env, PoolError::InvalidAuctionLot);
        }
        set_bad_debt_auction(
            env,
            &BadDebtAuctionData {
                auction_id: auction.auction_id.clone(),
                bid: remaining_bid,
                block: auction.block,
                lot_quote: remaining_quote,
            },
        );
    } else {
        env.storage().temporary().remove(&AuctionDataKey::BadDebt);
    }

    set_positions(env, &backstop, &backstop_positions);
    set_positions(env, filler, &filler_positions);
    for (asset, _) in fill.bid.iter() {
        let reserve = reserve_config(env, &asset);
        set_backstop_liability(
            env,
            &asset,
            backstop_positions
                .liabilities
                .get(reserve.index)
                .unwrap_or(0),
        );
    }
    BadDebtAuctionFilled {
        fill: fill.clone(),
        filler: filler.clone(),
        percent,
    }
    .publish(env);
    fill
}

pub(crate) fn delete_stale_bad_debt_auction(env: &Env) {
    let auction = get_bad_debt_auction(env);
    let stale_at = auction
        .block
        .checked_add(AUCTION_STALE_LEDGERS)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    if env.ledger().sequence() < stale_at {
        panic_with_error!(env, PoolError::AuctionNotStale);
    }
    let backstop = get_backstop(env);
    BackstopClient::new(env, &backstop)
        .release_bad_debt_lot(&env.current_contract_address(), &auction.auction_id);
    env.storage().temporary().remove(&AuctionDataKey::BadDebt);
    BadDebtAuctionDeleted {
        auction_id: auction.auction_id,
        stale: true,
    }
    .publish(env);
}

pub(crate) fn has_bad_debt_auction(env: &Env) -> bool {
    env.storage().temporary().has(&AuctionDataKey::BadDebt)
}

fn set_bad_debt_auction(env: &Env, auction: &BadDebtAuctionData) {
    env.storage()
        .temporary()
        .set(&AuctionDataKey::BadDebt, auction);
    env.storage().temporary().extend_ttl(
        &AuctionDataKey::BadDebt,
        AUCTION_TTL_THRESHOLD,
        AUCTION_TTL_BUMP,
    );
}

pub(crate) fn create_interest_auction(
    env: &Env,
    auction_id: &BytesN<32>,
    lot_assets: &Vec<Address>,
) -> InterestAuctionData {
    if has_interest_auction(env) {
        panic_with_error!(env, PoolError::AuctionInProgress);
    }
    let max_lot_assets = core::cmp::min(
        config(env)
            .max_positions
            .checked_sub(1)
            .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError)),
        MAX_INTEREST_LOT_ASSETS,
    );
    if lot_assets.is_empty() || lot_assets.len() > max_lot_assets {
        panic_with_error!(env, PoolError::InvalidInterestAuction);
    }
    require_unique_addresses(env, lot_assets);

    let mut states = Map::<Address, InterestReserveState>::new(env);
    let mut distributions = Map::<Address, i128>::new(env);
    for asset in lot_assets {
        let reserve = checkpoint_reserve(env, &asset);
        let mut state = get_interest_reserve_state(env, &asset);
        let uncheckpointed =
            checked_sub_nonnegative(env, reserve.data.backstop_credit, state.total(env));
        let distribution = checked_add(env, uncheckpointed, state.carry);
        state.carry = 0;
        states.set(asset.clone(), state);
        distributions.set(asset, distribution);
    }

    let backstop = get_backstop(env);
    let pool = env.current_contract_address();
    let quotes =
        BackstopClient::new(env, &backstop).quote_pool_take_rate_batch(&pool, &distributions);
    if quotes.len() != distributions.len() {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }
    for (asset, distribution) in distributions.iter() {
        let quote = quotes
            .get(asset.clone())
            .unwrap_or_else(|| panic_with_error!(env, PoolError::InvalidAuctionLot));
        let allocated = checked_add(
            env,
            checked_add(env, quote.blnd_usdc, quote.blnd_xlm),
            checked_add(env, quote.usdc, quote.remainder),
        );
        if quote.blnd_usdc < 0
            || quote.blnd_xlm < 0
            || quote.usdc < 0
            || quote.remainder < 0
            || allocated != distribution
        {
            panic_with_error!(env, PoolError::InvalidAuctionLot);
        }
        let mut state = states.get(asset.clone()).unwrap();
        state.blnd_usdc = checked_add(env, state.blnd_usdc, quote.blnd_usdc);
        state.blnd_xlm = checked_add(env, state.blnd_xlm, quote.blnd_xlm);
        state.usdc = checked_add(env, state.usdc, quote.usdc);
        state.carry = quote.remainder;
        states.set(asset, state);
    }

    let mut tier_values = [0_i128; 3];
    let mut valid_until = u64::MAX;
    for asset in lot_assets {
        let state = states.get(asset.clone()).unwrap();
        let (blnd_usdc, blnd_xlm, usdc, asset_valid_until) =
            value_reserve_amounts(env, &asset, state.blnd_usdc, state.blnd_xlm, state.usdc);
        tier_values[0] = checked_add(env, tier_values[0], blnd_usdc);
        tier_values[1] = checked_add(env, tier_values[1], blnd_xlm);
        tier_values[2] = checked_add(env, tier_values[2], usdc);
        valid_until = valid_until.min(asset_valid_until);
    }

    let cursor = interest_tier_cursor(env);
    let mut selected = None;
    for offset in 0..3_u32 {
        let index = (cursor + offset) % 3;
        if tier_values[index as usize] >= INTEREST_TIER_MINIMUM_VALUE_USDC {
            selected = Some(tier_from_index(index));
            break;
        }
    }
    let tier =
        selected.unwrap_or_else(|| panic_with_error!(env, PoolError::NoInterestAuctionCapacity));
    let lot_value = tier_values[tier_index(tier) as usize];
    let mut lot = Map::new(env);
    for asset in lot_assets {
        let state = states.get(asset.clone()).unwrap();
        let amount = state.tier_amount(tier);
        if amount > 0 {
            lot.set(asset.clone(), amount);
        }
        set_interest_reserve_state(env, &asset, &state);
    }
    if lot.is_empty() {
        panic_with_error!(env, PoolError::NoInterestAuctionCapacity);
    }

    let quote = BackstopClient::new(env, &backstop)
        .commit_interest_lot(&pool, auction_id, &tier, &lot_value);
    let expected_target_value = mul_div_ceil(env, lot_value, 6, 5);
    if quote.tier != tier
        || quote.lot_value != lot_value
        || quote.bid_amount <= 0
        || quote.target_value != expected_target_value
        || quote.valid_until < env.ledger().timestamp()
    {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }
    valid_until = valid_until.min(quote.valid_until);
    let block = env
        .ledger()
        .sequence()
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let auction = InterestAuctionData {
        auction_id: auction_id.clone(),
        auction: AuctionData {
            bid: soroban_sdk::map![env, (quote.bid_token, quote.bid_amount)],
            lot,
            block,
        },
        bid_valid_until: quote.valid_until,
        lot_value,
        target_value: quote.target_value,
        tier,
        valid_until,
    };
    set_interest_auction(env, &auction);
    env.storage()
        .instance()
        .set(&InterestDataKey::Cursor, &((tier_index(tier) + 1) % 3));
    InterestAuctionCreated {
        auction: auction.clone(),
    }
    .publish(env);
    auction
}

pub(crate) fn get_interest_auction(env: &Env) -> InterestAuctionData {
    env.storage()
        .temporary()
        .get(&AuctionDataKey::Interest)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::AuctionNotFound))
}

pub(crate) fn fill_interest_auction(
    env: &Env,
    filler: &Address,
    percent: u32,
) -> InterestAuctionFill {
    let pool = env.current_contract_address();
    let backstop = get_backstop(env);
    if filler == &pool || filler == &backstop || percent == 0 || percent > 100 {
        panic_with_error!(env, PoolError::InvalidInterestAuction);
    }
    filler.require_auth();
    let auction = get_interest_auction(env);
    let fill = scale_interest_auction(env, &auction, percent);

    for (asset, amount) in fill.lot.iter() {
        let token = token::Client::new(env, &asset);
        let pool_before = token.balance(&pool);
        let filler_before = token.balance(filler);
        token.transfer(&pool, filler, &amount);
        if token.balance(&pool) != checked_sub_nonnegative(env, pool_before, amount)
            || token.balance(filler) != checked_add(env, filler_before, amount)
        {
            panic_with_error!(env, PoolError::InvalidTokenBalance);
        }
        consume_backstop_credit(env, &asset, amount);
    }
    consume_pending_interest_lot(env, auction.tier, &fill.lot);

    let remaining_quote = BackstopClient::new(env, &backstop).settle_interest_lot(
        &pool,
        &auction.auction_id,
        &fill.base_bid_amount,
        &fill.bid_amount,
        filler,
    );
    if fill.complete != remaining_quote.is_none() {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }
    if let Some(quote) = remaining_quote {
        let bid_token = auction.auction.bid.keys().get(0).unwrap();
        let previous_bid = auction.auction.bid.get(bid_token.clone()).unwrap();
        let remaining_bid = checked_sub_nonnegative(env, previous_bid, fill.base_bid_amount);
        let expected_lot_value = mul_div_floor(env, auction.lot_value, remaining_bid, previous_bid);
        if quote.bid_token != bid_token
            || quote.bid_amount != remaining_bid
            || quote.lot_value != expected_lot_value
            || quote.tier != auction.tier
            || quote.target_value != auction.target_value
            || quote.valid_until != auction.bid_valid_until
        {
            panic_with_error!(env, PoolError::InvalidAuctionLot);
        }
        let mut remaining_lot = Map::new(env);
        for (asset, amount) in auction.auction.lot.iter() {
            let remainder =
                checked_sub_nonnegative(env, amount, fill.base_lot.get(asset.clone()).unwrap_or(0));
            if remainder > 0 {
                remaining_lot.set(asset, remainder);
            }
        }
        set_interest_auction(
            env,
            &InterestAuctionData {
                auction_id: auction.auction_id.clone(),
                auction: AuctionData {
                    bid: soroban_sdk::map![env, (quote.bid_token, quote.bid_amount)],
                    lot: remaining_lot,
                    block: auction.auction.block,
                },
                bid_valid_until: auction.bid_valid_until,
                lot_value: quote.lot_value,
                target_value: auction.target_value,
                tier: auction.tier,
                valid_until: auction.valid_until,
            },
        );
    } else {
        env.storage().temporary().remove(&AuctionDataKey::Interest);
    }

    InterestAuctionFilled {
        fill: fill.clone(),
        filler: filler.clone(),
        percent,
    }
    .publish(env);
    fill
}

pub(crate) fn delete_stale_interest_auction(env: &Env) {
    let auction = get_interest_auction(env);
    let stale_at = auction
        .auction
        .block
        .checked_add(AUCTION_STALE_LEDGERS)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    if env.ledger().sequence() < stale_at {
        panic_with_error!(env, PoolError::AuctionNotStale);
    }
    let backstop = get_backstop(env);
    BackstopClient::new(env, &backstop)
        .release_interest_lot(&env.current_contract_address(), &auction.auction_id);
    env.storage().temporary().remove(&AuctionDataKey::Interest);
    InterestAuctionDeleted {
        auction_id: auction.auction_id,
        stale: true,
    }
    .publish(env);
}

pub(crate) fn interest_reserve_state(env: &Env, asset: &Address) -> InterestReserveState {
    get_interest_reserve_state(env, asset)
}

fn has_interest_auction(env: &Env) -> bool {
    env.storage().temporary().has(&AuctionDataKey::Interest)
}

fn set_interest_auction(env: &Env, auction: &InterestAuctionData) {
    env.storage()
        .temporary()
        .set(&AuctionDataKey::Interest, auction);
    env.storage().temporary().extend_ttl(
        &AuctionDataKey::Interest,
        AUCTION_TTL_THRESHOLD,
        AUCTION_TTL_BUMP,
    );
}

fn get_interest_reserve_state(env: &Env, asset: &Address) -> InterestReserveState {
    let key = InterestDataKey::Reserve(asset.clone());
    let state = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(InterestReserveState::empty);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            INTEREST_STATE_TTL_THRESHOLD,
            INTEREST_STATE_TTL_BUMP,
        );
    }
    state
}

fn set_interest_reserve_state(env: &Env, asset: &Address, state: &InterestReserveState) {
    if state.blnd_usdc < 0 || state.blnd_xlm < 0 || state.usdc < 0 || state.carry < 0 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    let key = InterestDataKey::Reserve(asset.clone());
    env.storage().persistent().set(&key, state);
    env.storage().persistent().extend_ttl(
        &key,
        INTEREST_STATE_TTL_THRESHOLD,
        INTEREST_STATE_TTL_BUMP,
    );
}

fn consume_pending_interest_lot(env: &Env, tier: BackstopTier, transferred: &Map<Address, i128>) {
    for (asset, amount) in transferred.iter() {
        let mut state = get_interest_reserve_state(env, &asset);
        state.set_tier_amount(
            tier,
            checked_sub_nonnegative(env, state.tier_amount(tier), amount),
        );
        set_interest_reserve_state(env, &asset, &state);
    }
}

fn interest_tier_cursor(env: &Env) -> u32 {
    let cursor = env
        .storage()
        .instance()
        .get(&InterestDataKey::Cursor)
        .unwrap_or(0);
    if cursor > 2 {
        panic_with_error!(env, PoolError::ArithmeticError);
    }
    cursor
}

fn tier_from_index(index: u32) -> BackstopTier {
    match index {
        0 => BackstopTier::BlndUsdc,
        1 => BackstopTier::BlndXlm,
        _ => BackstopTier::Usdc,
    }
}

fn tier_index(tier: BackstopTier) -> u32 {
    match tier {
        BackstopTier::BlndUsdc => 0,
        BackstopTier::BlndXlm => 1,
        BackstopTier::Usdc => 2,
    }
}

pub(crate) fn create_user_liquidation(
    env: &Env,
    user: &Address,
    bid: &Vec<Address>,
    lot: &Vec<Address>,
    percent: u32,
) -> AuctionData {
    if has_user_liquidation(env, user) {
        panic_with_error!(env, PoolError::AuctionInProgress);
    }
    if user == &env.current_contract_address()
        || user == &get_backstop(env)
        || percent == 0
        || percent > 100
    {
        panic_with_error!(env, PoolError::InvalidLiquidation);
    }
    require_unique_addresses(env, bid);
    require_unique_addresses(env, lot);
    let asset_count = bid
        .len()
        .checked_add(lot.len())
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    if asset_count > config(env).max_positions {
        panic_with_error!(env, PoolError::TooManyPositions);
    }

    let user_positions = positions(env, user);
    let mut prices = Map::new(env);
    let user_data = position_data_with_prices(env, &user_positions, &mut prices);
    if user_data.liability_base <= user_data.collateral_base {
        panic_with_error!(env, PoolError::InvalidLiquidation);
    }

    let mut included = Positions::empty(env);
    for asset in bid {
        let reserve = reserve_config(env, &asset);
        let amount = user_positions.liabilities.get(reserve.index).unwrap_or(0);
        if amount <= 0 {
            panic_with_error!(env, PoolError::InvalidAuctionBid);
        }
        included.liabilities.set(reserve.index, amount);
    }
    if included.liabilities.is_empty() {
        panic_with_error!(env, PoolError::InvalidAuctionBid);
    }
    for asset in lot {
        let reserve = reserve_config(env, &asset);
        let amount = user_positions.collateral.get(reserve.index).unwrap_or(0);
        if amount <= 0 {
            panic_with_error!(env, PoolError::InvalidAuctionLot);
        }
        included.collateral.set(reserve.index, amount);
    }
    if included.collateral.is_empty() {
        panic_with_error!(env, PoolError::InvalidAuctionLot);
    }

    let included_data = position_data_with_prices(env, &included, &mut prices);
    let all_collateral = included_data.raw_collateral_base == user_data.raw_collateral_base;
    let all_positions =
        all_collateral && included_data.raw_liability_base == user_data.raw_liability_base;
    let full_liquidation = all_positions && percent > 95;
    let checked_percent = if full_liquidation { 95 } else { percent };
    let percent_scaled = i128::from(checked_percent)
        .checked_mul(100_000)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));

    let average_collateral_factor = mul_div_floor(
        env,
        included_data.collateral_base,
        included_data.scalar,
        included_data.raw_collateral_base,
    );
    let average_inverse_liability_factor = mul_div_floor(
        env,
        included_data.liability_base,
        included_data.scalar,
        included_data.raw_liability_base,
    );
    let collateral_to_liability_factor = mul_div_ceil(
        env,
        average_collateral_factor,
        included_data.scalar,
        average_inverse_liability_factor,
    );
    let incentive_delta =
        checked_sub_nonnegative(env, included_data.scalar, collateral_to_liability_factor);
    let estimated_incentive = checked_add(
        env,
        included_data.scalar,
        mul_div_ceil(
            env,
            incentive_delta,
            included_data.scalar,
            checked_add(env, included_data.scalar, included_data.scalar),
        ),
    );
    let estimated_withdrawn_collateral = mul_div_floor(
        env,
        mul_div_floor(
            env,
            included_data.raw_liability_base,
            percent_scaled,
            SCALAR_7,
        ),
        estimated_incentive,
        included_data.scalar,
    );
    let mut collateral_percent = mul_div_ceil(
        env,
        estimated_withdrawn_collateral,
        included_data.scalar,
        included_data.raw_collateral_base,
    );
    if collateral_percent > included_data.scalar {
        collateral_percent = included_data.scalar;
        if !all_collateral {
            panic_with_error!(env, PoolError::InvalidLiquidation);
        }
    }

    let block = env
        .ledger()
        .sequence()
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let mut quote = AuctionData {
        bid: Map::new(env),
        lot: Map::new(env),
        block,
    };
    let mut full_quote = AuctionData {
        bid: Map::new(env),
        lot: Map::new(env),
        block,
    };
    let reserve_assets = crate::lending::reserve_list(env);
    for (index, amount) in included.collateral.iter() {
        let asset = reserve_assets
            .get(index)
            .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve));
        quote.lot.set(
            asset.clone(),
            mul_div_ceil(env, amount, collateral_percent, included_data.scalar),
        );
        full_quote.lot.set(asset, amount);
    }
    for (index, amount) in included.liabilities.iter() {
        let asset = reserve_assets
            .get(index)
            .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve));
        quote.bid.set(
            asset.clone(),
            mul_div_ceil(env, amount, percent_scaled, SCALAR_7),
        );
        full_quote.bid.set(asset, amount);
    }

    let mut simulated = user_positions.clone();
    remove_positions(env, &mut simulated, &quote.lot, &quote.bid);
    let post_liquidation = position_data_with_prices(env, &simulated, &mut prices);
    let auction = if full_liquidation {
        if estimated_withdrawn_collateral < user_data.raw_collateral_base
            && post_liquidation.is_hf_over(env, MAX_POST_LIQUIDATION_HF)
        {
            panic_with_error!(env, PoolError::InvalidLiquidationSize);
        }
        full_quote
    } else {
        if post_liquidation.is_hf_over(env, MAX_POST_LIQUIDATION_HF)
            || post_liquidation.is_hf_under(env, MIN_POST_LIQUIDATION_HF)
        {
            panic_with_error!(env, PoolError::InvalidLiquidationSize);
        }
        quote
    };

    set_user_liquidation(env, user, &auction);
    AuctionCreated {
        auction: auction.clone(),
        percent,
        user: user.clone(),
    }
    .publish(env);
    auction
}

pub(crate) fn get_user_liquidation(env: &Env, user: &Address) -> AuctionData {
    env.storage()
        .temporary()
        .get(&auction_key(user))
        .unwrap_or_else(|| panic_with_error!(env, PoolError::AuctionNotFound))
}

pub(crate) fn has_user_liquidation(env: &Env, user: &Address) -> bool {
    env.storage().temporary().has(&auction_key(user))
}

pub(crate) fn fill_user_liquidation(
    env: &Env,
    user: &Address,
    filler: &Address,
    filler_positions: &mut Positions,
    prices: &mut Map<Address, PriceQuote>,
    percent: i128,
) -> AuctionData {
    if user == filler || percent <= 0 || percent > 100 {
        panic_with_error!(env, PoolError::InvalidLiquidation);
    }
    let percent_u32 = u32::try_from(percent)
        .unwrap_or_else(|_| panic_with_error!(env, PoolError::InvalidLiquidation));
    let auction = get_user_liquidation(env, user);
    let (filled, remaining) = scale_auction(env, &auction, percent_u32);
    let mut liquidatee_positions = positions(env, user);
    let liquidatee_before = position_data_with_prices(env, &liquidatee_positions, prices);
    remove_positions(env, &mut liquidatee_positions, &filled.lot, &filled.bid);
    add_positions(env, filler_positions, &filled.lot, &filled.bid);

    if !liquidatee_positions.liabilities.is_empty() {
        if remaining.is_none() && liquidatee_positions.collateral.is_empty() {
            transfer_user_bad_debt(
                env,
                user,
                filler,
                &mut liquidatee_positions,
                filler_positions,
            );
        } else {
            let liquidatee_after = position_data_with_prices(env, &liquidatee_positions, prices);
            if liquidatee_positions.collateral.is_empty()
                || liquidatee_after.is_hf_lower_than(env, &liquidatee_before)
            {
                panic_with_error!(env, PoolError::BadDebtUnsupported);
            }
        }
    }

    set_positions(env, user, &liquidatee_positions);
    if let Some(remaining) = remaining {
        set_user_liquidation(env, user, &remaining);
    } else {
        remove_user_liquidation(env, user);
    }
    AuctionFilled {
        auction: filled.clone(),
        filler: filler.clone(),
        percent: percent_u32,
        user: user.clone(),
    }
    .publish(env);
    filled
}

/// Move every collateral-free residual dToken liability from a liquidated
/// user to the configured backstop without changing reserve dToken supply.
///
/// The completed liquidation and this handoff share one Soroban transaction.
/// Tier disposal remains a separate permissionless bad-debt continuation, so
/// a pre-existing bad-debt auction is neither replaced nor resized here.
fn transfer_user_bad_debt(
    env: &Env,
    user: &Address,
    filler: &Address,
    liquidatee_positions: &mut Positions,
    filler_positions: &mut Positions,
) {
    let backstop = get_backstop(env);
    let residual = liquidatee_positions.liabilities.clone();
    if residual.is_empty() || !liquidatee_positions.collateral.is_empty() {
        panic_with_error!(env, PoolError::BadDebtUnsupported);
    }

    let mut canonical_updates = Map::new(env);
    if filler == &backstop {
        for (index, amount) in residual.iter() {
            if amount <= 0 {
                panic_with_error!(env, PoolError::InvalidAuctionBid);
            }
            let asset = reserve_list(env)
                .get(index)
                .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve));
            add_position(env, &mut filler_positions.liabilities, index, amount);
            let next = filler_positions.liabilities.get(index).unwrap_or(0);
            canonical_updates.set(asset.clone(), next);
            UserBadDebtTransferred {
                asset,
                backstop: backstop.clone(),
                d_tokens: amount,
                user: user.clone(),
            }
            .publish(env);
        }
    } else {
        let mut backstop_positions = positions(env, &backstop);
        for (index, amount) in residual.iter() {
            if amount <= 0 {
                panic_with_error!(env, PoolError::InvalidAuctionBid);
            }
            let asset = reserve_list(env)
                .get(index)
                .unwrap_or_else(|| panic_with_error!(env, PoolError::UnknownReserve));
            add_position(env, &mut backstop_positions.liabilities, index, amount);
            let next = backstop_positions.liabilities.get(index).unwrap_or(0);
            canonical_updates.set(asset.clone(), next);
            UserBadDebtTransferred {
                asset,
                backstop: backstop.clone(),
                d_tokens: amount,
                user: user.clone(),
            }
            .publish(env);
        }
        set_positions(env, &backstop, &backstop_positions);
    }
    set_backstop_liabilities(env, &canonical_updates);

    liquidatee_positions.liabilities = Map::new(env);
}

pub(crate) fn cancel_user_liquidation(env: &Env, user: &Address) {
    if !has_user_liquidation(env, user) {
        panic_with_error!(env, PoolError::AuctionNotFound);
    }
    remove_user_liquidation(env, user);
    AuctionDeleted {
        stale: false,
        user: user.clone(),
    }
    .publish(env);
}

pub(crate) fn delete_stale_user_liquidation(env: &Env, user: &Address) {
    let auction = get_user_liquidation(env, user);
    let stale_at = auction
        .block
        .checked_add(AUCTION_STALE_LEDGERS)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    if env.ledger().sequence() < stale_at {
        panic_with_error!(env, PoolError::AuctionNotStale);
    }
    remove_user_liquidation(env, user);
    AuctionDeleted {
        stale: true,
        user: user.clone(),
    }
    .publish(env);
}

fn set_user_liquidation(env: &Env, user: &Address, auction: &AuctionData) {
    let key = auction_key(user);
    env.storage().temporary().set(&key, auction);
    env.storage()
        .temporary()
        .extend_ttl(&key, AUCTION_TTL_THRESHOLD, AUCTION_TTL_BUMP);
}

fn remove_user_liquidation(env: &Env, user: &Address) {
    env.storage().temporary().remove(&auction_key(user));
}

fn auction_key(user: &Address) -> AuctionDataKey {
    AuctionDataKey::UserLiquidation(user.clone())
}

fn scale_auction(
    env: &Env,
    auction: &AuctionData,
    percent: u32,
) -> (AuctionData, Option<AuctionData>) {
    let (bid_modifier, lot_modifier) = auction_modifiers(env, auction.block);
    let percent_scaled = i128::from(percent)
        .checked_mul(100_000)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let mut filled = AuctionData {
        bid: Map::new(env),
        lot: Map::new(env),
        block: auction.block,
    };
    let mut remaining = AuctionData {
        bid: Map::new(env),
        lot: Map::new(env),
        block: auction.block,
    };
    for (asset, amount) in auction.bid.iter() {
        let base = mul_div_ceil(env, amount, percent_scaled, SCALAR_7);
        let remainder = checked_sub_nonnegative(env, amount, base);
        if remainder > 0 {
            remaining.bid.set(asset.clone(), remainder);
        }
        let scaled = mul_div_ceil(env, base, bid_modifier, SCALAR_7);
        if scaled > 0 {
            filled.bid.set(asset, scaled);
        }
    }
    for (asset, amount) in auction.lot.iter() {
        let base = mul_div_floor(env, amount, percent_scaled, SCALAR_7);
        let remainder = checked_sub_nonnegative(env, amount, base);
        if remainder > 0 {
            remaining.lot.set(asset.clone(), remainder);
        }
        let scaled = mul_div_floor(env, base, lot_modifier, SCALAR_7);
        if scaled > 0 {
            filled.lot.set(asset, scaled);
        }
    }
    if remaining.bid.is_empty() && remaining.lot.is_empty() {
        (filled, None)
    } else {
        (filled, Some(remaining))
    }
}

fn scale_bad_debt_auction(
    env: &Env,
    auction: &BadDebtAuctionData,
    percent: u32,
) -> (BadDebtAuctionFill, Map<Address, i128>, i128) {
    if percent == 0 || percent > 100 {
        panic_with_error!(env, PoolError::InvalidLiquidation);
    }
    let (bid_modifier, lot_modifier) = auction_modifiers(env, auction.block);
    let percent_scaled = i128::from(percent)
        .checked_mul(100_000)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let mut filled_bid = Map::new(env);
    let mut remaining_bid = Map::new(env);
    for (asset, amount) in auction.bid.iter() {
        let base = mul_div_ceil(env, amount, percent_scaled, SCALAR_7);
        let remainder = checked_sub_nonnegative(env, amount, base);
        if remainder > 0 {
            remaining_bid.set(asset.clone(), remainder);
        }
        let scaled = mul_div_ceil(env, base, bid_modifier, SCALAR_7);
        if scaled > 0 {
            filled_bid.set(asset, scaled);
        }
    }
    let base_lot_amount =
        mul_div_floor(env, auction.lot_quote.lot_amount, percent_scaled, SCALAR_7);
    let remaining_lot_amount =
        checked_sub_nonnegative(env, auction.lot_quote.lot_amount, base_lot_amount);
    let lot_amount = mul_div_floor(env, base_lot_amount, lot_modifier, SCALAR_7);
    let complete = remaining_bid.is_empty() && remaining_lot_amount == 0;
    (
        BadDebtAuctionFill {
            auction_id: auction.auction_id.clone(),
            base_lot_amount,
            bid: filled_bid,
            block: auction.block,
            complete,
            lot_amount,
            tier: auction.lot_quote.tier,
        },
        remaining_bid,
        remaining_lot_amount,
    )
}

fn scale_interest_auction(
    env: &Env,
    auction: &InterestAuctionData,
    percent: u32,
) -> InterestAuctionFill {
    if percent == 0 || percent > 100 || auction.auction.bid.len() != 1 {
        panic_with_error!(env, PoolError::InvalidInterestAuction);
    }
    let (bid_modifier, lot_modifier) = auction_modifiers(env, auction.auction.block);
    let percent_scaled = i128::from(percent)
        .checked_mul(100_000)
        .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError));
    let (_, bid_amount) = auction.auction.bid.iter().next().unwrap();
    let base_bid_amount = mul_div_ceil(env, bid_amount, percent_scaled, SCALAR_7);
    let remaining_bid = checked_sub_nonnegative(env, bid_amount, base_bid_amount);
    let actual_bid_amount = mul_div_ceil(env, base_bid_amount, bid_modifier, SCALAR_7);
    let complete = remaining_bid == 0;
    let mut base_lot = Map::new(env);
    let mut lot = Map::new(env);
    let mut returned_lot = Map::new(env);
    for (asset, amount) in auction.auction.lot.iter() {
        let base = mul_div_floor(env, amount, percent_scaled, SCALAR_7);
        let actual = mul_div_floor(env, base, lot_modifier, SCALAR_7);
        let remainder = checked_sub_nonnegative(env, amount, base);
        let returned = checked_add(
            env,
            checked_sub_nonnegative(env, base, actual),
            if complete { remainder } else { 0 },
        );
        if base > 0 {
            base_lot.set(asset.clone(), base);
        }
        if actual > 0 {
            lot.set(asset.clone(), actual);
        }
        if returned > 0 {
            returned_lot.set(asset, returned);
        }
    }
    InterestAuctionFill {
        auction_id: auction.auction_id.clone(),
        base_bid_amount,
        base_lot,
        bid_amount: actual_bid_amount,
        block: auction.auction.block,
        complete,
        lot,
        returned_lot,
        tier: auction.tier,
    }
}

fn auction_modifiers(env: &Env, block: u32) -> (i128, i128) {
    let current_ledger = env.ledger().sequence();
    if current_ledger < block {
        panic_with_error!(env, PoolError::AuctionNotStarted);
    }
    let elapsed = current_ledger - block;
    if elapsed > AUCTION_PRICE_HALF_LEDGERS {
        let bid_modifier = if elapsed < AUCTION_PRICE_LEDGERS {
            checked_sub_nonnegative(
                env,
                SCALAR_7,
                i128::from(elapsed - AUCTION_PRICE_HALF_LEDGERS)
                    .checked_mul(AUCTION_MODIFIER_PER_LEDGER)
                    .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError)),
            )
        } else {
            0
        };
        (bid_modifier, SCALAR_7)
    } else {
        (
            SCALAR_7,
            i128::from(elapsed)
                .checked_mul(AUCTION_MODIFIER_PER_LEDGER)
                .unwrap_or_else(|| panic_with_error!(env, PoolError::ArithmeticError)),
        )
    }
}

fn remove_positions(
    env: &Env,
    positions: &mut Positions,
    collateral: &Map<Address, i128>,
    liabilities: &Map<Address, i128>,
) {
    for (asset, amount) in collateral.iter() {
        let reserve = reserve_config(env, &asset);
        remove_position(env, &mut positions.collateral, reserve.index, amount);
    }
    for (asset, amount) in liabilities.iter() {
        let reserve = reserve_config(env, &asset);
        remove_position(env, &mut positions.liabilities, reserve.index, amount);
    }
}

fn add_positions(
    env: &Env,
    positions: &mut Positions,
    collateral: &Map<Address, i128>,
    liabilities: &Map<Address, i128>,
) {
    for (asset, amount) in collateral.iter() {
        let reserve = reserve_config(env, &asset);
        add_position(env, &mut positions.collateral, reserve.index, amount);
    }
    for (asset, amount) in liabilities.iter() {
        let reserve = reserve_config(env, &asset);
        add_position(env, &mut positions.liabilities, reserve.index, amount);
    }
}

fn add_position(env: &Env, positions: &mut Map<u32, i128>, index: u32, amount: i128) {
    if amount <= 0 {
        return;
    }
    let previous = positions.get(index).unwrap_or(0);
    positions.set(index, checked_add(env, previous, amount));
}

fn remove_position(env: &Env, positions: &mut Map<u32, i128>, index: u32, amount: i128) {
    if amount <= 0 {
        return;
    }
    let previous = positions.get(index).unwrap_or(0);
    let next = checked_sub_nonnegative(env, previous, amount);
    if next == 0 {
        positions.remove(index);
    } else {
        positions.set(index, next);
    }
}

fn require_unique_addresses(env: &Env, addresses: &Vec<Address>) {
    let mut seen = Map::<Address, bool>::new(env);
    for address in addresses {
        if seen.contains_key(address.clone()) {
            panic_with_error!(env, PoolError::BadRequest);
        }
        seen.set(address, true);
    }
}

#[cfg(test)]
pub(crate) fn user_liquidation_ttl(env: &Env, user: &Address) -> u32 {
    use soroban_sdk::testutils::storage::Temporary as _;

    env.storage().temporary().get_ttl(&auction_key(user))
}

#[cfg(test)]
pub(crate) fn bad_debt_auction_ttl(env: &Env) -> u32 {
    use soroban_sdk::testutils::storage::Temporary as _;

    env.storage().temporary().get_ttl(&AuctionDataKey::BadDebt)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    #[test]
    fn auction_scaling_matches_v2_curve_and_partial_rounding() {
        let env = Env::default();
        let bid_asset = Address::generate(&env);
        let lot_asset = Address::generate(&env);
        let auction = AuctionData {
            bid: soroban_sdk::map![&env, (bid_asset.clone(), 1_000_i128)],
            lot: soroban_sdk::map![&env, (lot_asset.clone(), 1_000_i128)],
            block: 10,
        };

        for (ledger, expected_bid, expected_lot) in [
            (10, 1_000, 0),
            (110, 1_000, 500),
            (210, 1_000, 1_000),
            (310, 500, 1_000),
            (410, 0, 1_000),
        ] {
            env.ledger().set_sequence_number(ledger);
            let (filled, remaining) = scale_auction(&env, &auction, 100);
            assert_eq!(filled.bid.get(bid_asset.clone()).unwrap_or(0), expected_bid);
            assert_eq!(filled.lot.get(lot_asset.clone()).unwrap_or(0), expected_lot);
            assert!(remaining.is_none());
        }

        let odd_auction = AuctionData {
            bid: soroban_sdk::map![&env, (bid_asset.clone(), 101_i128)],
            lot: soroban_sdk::map![&env, (lot_asset.clone(), 101_i128)],
            block: 10,
        };
        env.ledger().set_sequence_number(210);
        let (filled, remaining) = scale_auction(&env, &odd_auction, 50);
        let remaining = remaining.unwrap();
        assert_eq!(filled.bid.get(bid_asset.clone()), Some(51));
        assert_eq!(filled.lot.get(lot_asset.clone()), Some(50));
        assert_eq!(remaining.bid.get(bid_asset), Some(50));
        assert_eq!(remaining.lot.get(lot_asset), Some(51));
    }

    #[test]
    fn interest_auction_scaling_preserves_base_lots_and_returns_curve_discount() {
        let env = Env::default();
        let bid_token = Address::generate(&env);
        let reserve_a = Address::generate(&env);
        let reserve_b = Address::generate(&env);
        let auction = InterestAuctionData {
            auction_id: BytesN::from_array(&env, &[7; 32]),
            auction: AuctionData {
                bid: soroban_sdk::map![&env, (bid_token, 1_001_i128)],
                lot: soroban_sdk::map![
                    &env,
                    (reserve_a.clone(), 1_001_i128),
                    (reserve_b.clone(), 501_i128)
                ],
                block: 10,
            },
            bid_valid_until: u64::MAX,
            lot_value: 300 * SCALAR_7,
            target_value: 360 * SCALAR_7,
            tier: BackstopTier::BlndXlm,
            valid_until: u64::MAX,
        };

        env.ledger().set_sequence_number(110);
        let scaled = scale_interest_auction(&env, &auction, 50);
        assert_eq!(scaled.base_bid_amount, 501);
        assert_eq!(scaled.bid_amount, 501);
        assert_eq!(scaled.base_lot.get(reserve_a.clone()), Some(500));
        assert_eq!(scaled.base_lot.get(reserve_b.clone()), Some(250));
        assert_eq!(scaled.lot.get(reserve_a.clone()), Some(250));
        assert_eq!(scaled.lot.get(reserve_b.clone()), Some(125));
        assert_eq!(scaled.returned_lot.get(reserve_a), Some(250));
        assert_eq!(scaled.returned_lot.get(reserve_b), Some(125));
        assert!(!scaled.complete);
    }
}
