use soroban_sdk::{contractevent, contracttype, panic_with_error, Address, Env, Map, Vec};

use crate::{
    get_backstop,
    lending::{
        checked_add, checked_sub_nonnegative, config, mul_div_ceil, mul_div_floor,
        position_data_with_prices, positions, reserve_config, set_positions, Positions, PriceQuote,
        SCALAR_7,
    },
    PoolError,
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

#[derive(Clone)]
#[contracttype]
enum AuctionDataKey {
    UserLiquidation(Address),
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
        let liquidatee_after = position_data_with_prices(env, &liquidatee_positions, prices);
        if liquidatee_positions.collateral.is_empty()
            || liquidatee_after.is_hf_lower_than(env, &liquidatee_before)
        {
            panic_with_error!(env, PoolError::BadDebtUnsupported);
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
    let current_ledger = env.ledger().sequence();
    if current_ledger < auction.block {
        panic_with_error!(env, PoolError::AuctionNotStarted);
    }
    let elapsed = current_ledger - auction.block;
    let (bid_modifier, lot_modifier) = if elapsed > AUCTION_PRICE_HALF_LEDGERS {
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
    };
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
}
