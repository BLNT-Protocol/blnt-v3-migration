#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
};

const ADAPTER_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AssetValuation {
    pub underlying_blnd: i128,
    pub usdc_value: i128,
    pub valid_until: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AdapterBinding {
    pub blnd: Address,
    pub blnd_usdc: Address,
    pub blnd_xlm: Address,
    pub usdc: Address,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PriceQuote {
    pub decimals: u32,
    pub price: i128,
    pub valid_until: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Blnd,
    BlndUsdc,
    BlndXlm,
    Price(Address),
    Usdc,
    Valuation(Address),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum TestAdapterError {
    InvalidAmount = 1500,
    UnsupportedAsset = 1501,
}

/// Localnet and compatibility-test fixture only.
///
/// It deliberately values each configured LP token one-for-one in both USDC
/// and underlying BLND. It is not a production oracle or pricing mechanism.
#[contract]
pub struct TestValuationAdapter;

#[contractimpl]
impl TestValuationAdapter {
    pub fn __constructor(
        env: Env,
        blnd: Address,
        usdc: Address,
        blnd_usdc: Address,
        blnd_xlm: Address,
    ) {
        env.storage().instance().set(&DataKey::Blnd, &blnd);
        env.storage().instance().set(&DataKey::Usdc, &usdc);
        env.storage().instance().set(&DataKey::BlndUsdc, &blnd_usdc);
        env.storage().instance().set(&DataKey::BlndXlm, &blnd_xlm);
    }

    pub fn version(_env: Env) -> u32 {
        ADAPTER_VERSION
    }

    pub fn binding(env: Env) -> AdapterBinding {
        AdapterBinding {
            blnd: env.storage().instance().get(&DataKey::Blnd).unwrap(),
            blnd_usdc: env.storage().instance().get(&DataKey::BlndUsdc).unwrap(),
            blnd_xlm: env.storage().instance().get(&DataKey::BlndXlm).unwrap(),
            usdc: env.storage().instance().get(&DataKey::Usdc).unwrap(),
        }
    }

    /// Store a fixture price in its own persistent ledger entry.
    ///
    /// Compatibility resource tests use this path to model the per-asset
    /// footprint of an optimized stateful oracle.
    pub fn set_price(env: Env, asset: Address, quote: PriceQuote) {
        env.storage()
            .persistent()
            .set(&DataKey::Price(asset), &quote);
    }

    /// Store an exact fixture valuation in its own persistent ledger entry.
    ///
    /// Resource tests use this to include stateful LP-valuation reads in the
    /// measured bad-debt exhaustion path.
    pub fn set_valuation(env: Env, token: Address, valuation: AssetValuation) {
        env.storage()
            .persistent()
            .set(&DataKey::Valuation(token), &valuation);
    }

    pub fn quote(env: Env, token: Address, amount: i128) -> AssetValuation {
        if amount <= 0 {
            panic_with_error!(&env, TestAdapterError::InvalidAmount);
        }
        if let Some(valuation) = env
            .storage()
            .persistent()
            .get(&DataKey::Valuation(token.clone()))
        {
            return valuation;
        }
        let blnd_usdc: Address = env.storage().instance().get(&DataKey::BlndUsdc).unwrap();
        let blnd_xlm: Address = env.storage().instance().get(&DataKey::BlndXlm).unwrap();
        if token != blnd_usdc && token != blnd_xlm {
            panic_with_error!(&env, TestAdapterError::UnsupportedAsset);
        }
        AssetValuation {
            underlying_blnd: amount,
            usdc_value: amount,
            valid_until: u64::MAX,
        }
    }

    /// Return an explicitly stored fixture price, or a deterministic one-USDC
    /// price for either configured LP token. This test-only interface is not a
    /// production oracle.
    pub fn price(env: Env, asset: Address) -> PriceQuote {
        if let Some(quote) = env
            .storage()
            .persistent()
            .get(&DataKey::Price(asset.clone()))
        {
            return quote;
        }
        let blnd_usdc: Address = env.storage().instance().get(&DataKey::BlndUsdc).unwrap();
        let blnd_xlm: Address = env.storage().instance().get(&DataKey::BlndXlm).unwrap();
        if asset != blnd_usdc && asset != blnd_xlm {
            panic_with_error!(&env, TestAdapterError::UnsupportedAsset);
        }
        PriceQuote {
            decimals: 7,
            price: 10_000_000,
            valid_until: u64::MAX,
        }
    }
}
