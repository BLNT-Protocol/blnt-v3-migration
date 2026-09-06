// Copyright (C) 2026 BLNT Protocol contributors
// SPDX-License-Identifier: AGPL-3.0-only
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    symbol_short, token::TokenInterface, Address, Env, MuxedAddress, String,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const BALANCE_TTL_THRESHOLD: u32 = 179 * DAY_IN_LEDGERS;
const BALANCE_TTL_BUMP: u32 = 180 * DAY_IN_LEDGERS;

#[derive(Clone)]
#[contracttype]
struct TokenMetadata {
    decimal: u32,
    name: String,
    symbol: String,
}

#[derive(Clone)]
#[contracttype]
struct AllowanceKey {
    from: Address,
    spender: Address,
}

#[derive(Clone)]
#[contracttype]
struct Allowance {
    amount: i128,
    live_until_ledger: u32,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Admin,
    Allowance(AllowanceKey),
    Balance(Address),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum TokenError {
    InvalidAmount = 1,
    InsufficientBalance = 2,
    InsufficientAllowance = 3,
    InvalidAllowanceExpiration = 4,
    ArithmeticOverflow = 5,
    InvalidDecimals = 6,
}

#[contractevent(topics = ["approve"], data_format = "vec")]
struct ApproveEvent {
    #[topic]
    from: Address,
    #[topic]
    spender: Address,
    amount: i128,
    live_until_ledger: u32,
}

#[contractevent(topics = ["transfer"], data_format = "map")]
struct TransferEvent {
    #[topic]
    from: Address,
    #[topic]
    to: Address,
    to_muxed_id: Option<u64>,
    amount: i128,
}

#[contractevent(topics = ["burn"], data_format = "single-value")]
struct BurnEvent {
    #[topic]
    from: Address,
    amount: i128,
}

#[contractevent(topics = ["mint"], data_format = "map")]
struct MintEvent {
    #[topic]
    to: Address,
    to_muxed_id: Option<u64>,
    amount: i128,
}

/// Standard, admin-mintable SEP-41 fixture used only by the public-testnet
/// deployment harness to exercise non-SAC reserve support.
#[contract]
pub struct TestSep41Token;

#[contractimpl]
impl TestSep41Token {
    pub fn __constructor(env: Env, admin: Address, decimal: u32, name: String, symbol: String) {
        if decimal > 18 {
            panic_with_error!(&env, TokenError::InvalidDecimals);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        // Match the standard token instance-storage layout consumed by the
        // Stellar SDK and BLNT UI metadata loader.
        env.storage().instance().set(
            &symbol_short!("METADATA"),
            &TokenMetadata {
                decimal,
                name,
                symbol,
            },
        );
        extend_instance_ttl(&env);
    }

    pub fn admin(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        require_nonnegative(&env, amount);
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        extend_instance_ttl(&env);
        receive_balance(&env, &to, amount);
        MintEvent {
            to,
            to_muxed_id: None,
            amount,
        }
        .publish(&env);
    }
}

#[contractimpl]
impl TokenInterface for TestSep41Token {
    fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        extend_instance_ttl(&env);
        get_allowance(&env, &from, &spender).amount
    }

    fn approve(env: Env, from: Address, spender: Address, amount: i128, live_until_ledger: u32) {
        from.require_auth();
        require_nonnegative(&env, amount);
        if amount > 0 && live_until_ledger < env.ledger().sequence() {
            panic_with_error!(&env, TokenError::InvalidAllowanceExpiration);
        }
        extend_instance_ttl(&env);
        set_allowance(&env, &from, &spender, amount, live_until_ledger);
        ApproveEvent {
            from,
            spender,
            amount,
            live_until_ledger,
        }
        .publish(&env);
    }

    fn balance(env: Env, id: Address) -> i128 {
        extend_instance_ttl(&env);
        get_balance(&env, &id)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        require_nonnegative(&env, amount);
        extend_instance_ttl(&env);
        let recipient = to.address();
        spend_balance(&env, &from, amount);
        receive_balance(&env, &recipient, amount);
        TransferEvent {
            from,
            to: recipient,
            to_muxed_id: to.id(),
            amount,
        }
        .publish(&env);
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        require_nonnegative(&env, amount);
        extend_instance_ttl(&env);
        spend_allowance(&env, &from, &spender, amount);
        spend_balance(&env, &from, amount);
        receive_balance(&env, &to, amount);
        TransferEvent {
            from,
            to,
            to_muxed_id: None,
            amount,
        }
        .publish(&env);
    }

    fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        require_nonnegative(&env, amount);
        extend_instance_ttl(&env);
        spend_balance(&env, &from, amount);
        BurnEvent { from, amount }.publish(&env);
    }

    fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        require_nonnegative(&env, amount);
        extend_instance_ttl(&env);
        spend_allowance(&env, &from, &spender, amount);
        spend_balance(&env, &from, amount);
        BurnEvent { from, amount }.publish(&env);
    }

    fn decimals(env: Env) -> u32 {
        extend_instance_ttl(&env);
        metadata(&env).decimal
    }

    fn name(env: Env) -> String {
        extend_instance_ttl(&env);
        metadata(&env).name
    }

    fn symbol(env: Env) -> String {
        extend_instance_ttl(&env);
        metadata(&env).symbol
    }
}

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

fn metadata(env: &Env) -> TokenMetadata {
    env.storage()
        .instance()
        .get(&symbol_short!("METADATA"))
        .unwrap()
}

fn require_nonnegative(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, TokenError::InvalidAmount);
    }
}

fn get_balance(env: &Env, address: &Address) -> i128 {
    let key = DataKey::Balance(address.clone());
    let balance = env.storage().persistent().get(&key).unwrap_or(0);
    if balance != 0 {
        env.storage()
            .persistent()
            .extend_ttl(&key, BALANCE_TTL_THRESHOLD, BALANCE_TTL_BUMP);
    }
    balance
}

fn set_balance(env: &Env, address: &Address, amount: i128) {
    let key = DataKey::Balance(address.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        env.storage()
            .persistent()
            .extend_ttl(&key, BALANCE_TTL_THRESHOLD, BALANCE_TTL_BUMP);
    }
}

fn receive_balance(env: &Env, address: &Address, amount: i128) {
    let next = get_balance(env, address)
        .checked_add(amount)
        .unwrap_or_else(|| panic_with_error!(env, TokenError::ArithmeticOverflow));
    set_balance(env, address, next);
}

fn spend_balance(env: &Env, address: &Address, amount: i128) {
    let balance = get_balance(env, address);
    if balance < amount {
        panic_with_error!(env, TokenError::InsufficientBalance);
    }
    set_balance(env, address, balance - amount);
}

fn allowance_key(from: &Address, spender: &Address) -> DataKey {
    DataKey::Allowance(AllowanceKey {
        from: from.clone(),
        spender: spender.clone(),
    })
}

fn get_allowance(env: &Env, from: &Address, spender: &Address) -> Allowance {
    let allowance = env
        .storage()
        .temporary()
        .get(&allowance_key(from, spender))
        .unwrap_or(Allowance {
            amount: 0,
            live_until_ledger: 0,
        });
    if env.ledger().sequence() > allowance.live_until_ledger {
        Allowance {
            amount: 0,
            live_until_ledger: 0,
        }
    } else {
        allowance
    }
}

fn set_allowance(
    env: &Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    live_until_ledger: u32,
) {
    let key = allowance_key(from, spender);
    if amount == 0 {
        env.storage().temporary().remove(&key);
        return;
    }
    env.storage().temporary().set(
        &key,
        &Allowance {
            amount,
            live_until_ledger,
        },
    );
    let ttl = live_until_ledger
        .checked_sub(env.ledger().sequence())
        .unwrap_or_else(|| panic_with_error!(env, TokenError::InvalidAllowanceExpiration));
    if ttl > 0 {
        env.storage().temporary().extend_ttl(&key, ttl, ttl);
    }
}

fn spend_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) {
    let allowance = get_allowance(env, from, spender);
    if allowance.amount < amount || env.ledger().sequence() > allowance.live_until_ledger {
        panic_with_error!(env, TokenError::InsufficientAllowance);
    }
    if amount > 0 {
        set_allowance(
            env,
            from,
            spender,
            allowance.amount - amount,
            allowance.live_until_ledger,
        );
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token::TokenClient,
    };

    #[test]
    fn implements_sep41_transfers_and_allowances() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let spender = Address::generate(&env);
        let token = env.register(
            TestSep41Token,
            (
                admin.clone(),
                7_u32,
                String::from_str(&env, "Wrapped Bitcoin"),
                String::from_str(&env, "wBTC"),
            ),
        );
        let admin_client = TestSep41TokenClient::new(&env, &token);
        let token_client = TokenClient::new(&env, &token);

        admin_client.mint(&alice, &1_000);
        token_client.transfer(&alice, &bob, &250);
        token_client.approve(&alice, &spender, &300, &100);
        token_client.transfer_from(&spender, &alice, &bob, &125);

        assert_eq!(token_client.balance(&alice), 625);
        assert_eq!(token_client.balance(&bob), 375);
        assert_eq!(token_client.allowance(&alice, &spender), 175);
        assert_eq!(token_client.decimals(), 7);
        assert_eq!(token_client.symbol(), String::from_str(&env, "wBTC"));
    }

    #[test]
    fn shortened_allowance_reports_zero_after_logical_expiration() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_sequence_number(10);
        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let spender = Address::generate(&env);
        let token = env.register(
            TestSep41Token,
            (
                admin.clone(),
                7_u32,
                String::from_str(&env, "Wrapped Bitcoin"),
                String::from_str(&env, "wBTC"),
            ),
        );
        let admin_client = TestSep41TokenClient::new(&env, &token);
        let token_client = TokenClient::new(&env, &token);

        admin_client.mint(&alice, &1_000);
        token_client.approve(&alice, &spender, &300, &100);
        token_client.approve(&alice, &spender, &200, &20);
        env.ledger().set_sequence_number(21);

        assert_eq!(token_client.allowance(&alice, &spender), 0);
        assert!(token_client
            .try_transfer_from(&spender, &alice, &bob, &1)
            .is_err());
        assert_eq!(token_client.balance(&alice), 1_000);
        assert_eq!(token_client.balance(&bob), 0);
    }
}
