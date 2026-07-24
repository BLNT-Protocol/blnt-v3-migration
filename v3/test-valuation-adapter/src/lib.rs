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

#[derive(Clone)]
#[contracttype]
enum DataKey {
    BlndUsdc,
    BlndXlm,
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
    pub fn __constructor(env: Env, blnd_usdc: Address, blnd_xlm: Address) {
        env.storage().instance().set(&DataKey::BlndUsdc, &blnd_usdc);
        env.storage().instance().set(&DataKey::BlndXlm, &blnd_xlm);
    }

    pub fn version(_env: Env) -> u32 {
        ADAPTER_VERSION
    }

    pub fn quote(env: Env, token: Address, amount: i128) -> AssetValuation {
        if amount <= 0 {
            panic_with_error!(&env, TestAdapterError::InvalidAmount);
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
}
