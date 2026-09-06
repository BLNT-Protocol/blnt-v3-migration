// Copyright (C) 2026 BLNT Protocol contributors
// SPDX-License-Identifier: AGPL-3.0-only
#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};

const DAY_IN_LEDGERS: u32 = 17_280;
const ALL_PERMISSIONS: u32 = 1 | 2 | 4;

/// Testnet demonstration fixture, not a production access-management system.
///
/// The constructor's setup wallet has supply, borrow and backstop-deposit
/// permission for any pool using this controller. Every other user has none.
/// There are deliberately no setters, administrators or upgrade entrypoints.
#[contract]
pub struct TestAccessController;

#[contractimpl]
impl TestAccessController {
    pub fn __constructor(env: Env, allowed_user: Address) {
        env.storage()
            .instance()
            .set(&symbol_short!("user"), &allowed_user);
        extend_ttl(&env);
    }

    pub fn permissions(env: Env, pool: Address, user: Address) -> u32 {
        let _ = pool;
        extend_ttl(&env);
        let allowed_user: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("user"))
            .unwrap();
        if user == allowed_user {
            ALL_PERMISSIONS
        } else {
            0
        }
    }
}

fn extend_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(89 * DAY_IN_LEDGERS, 90 * DAY_IN_LEDGERS);
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn only_setup_wallet_has_permissions_for_consuming_pools() {
        let env = Env::default();
        let setup = Address::generate(&env);
        let controller = env.register(TestAccessController, (setup.clone(),));
        let client = TestAccessControllerClient::new(&env, &controller);
        for _ in 0..3 {
            let pool = Address::generate(&env);
            assert_eq!(client.permissions(&pool, &setup), 7);
            assert_eq!(client.permissions(&pool, &Address::generate(&env)), 0);
        }
    }

    #[test]
    fn permission_queries_extend_instance_ttl() {
        let env = Env::default();
        let setup = Address::generate(&env);
        let pool = Address::generate(&env);
        let controller = env.register(TestAccessController, (setup.clone(),));
        let client = TestAccessControllerClient::new(&env, &controller);
        env.ledger()
            .with_mut(|ledger| ledger.sequence_number += 2 * DAY_IN_LEDGERS);
        assert_eq!(client.permissions(&pool, &setup), 7);
        env.ledger()
            .with_mut(|ledger| ledger.sequence_number += 89 * DAY_IN_LEDGERS);
        assert_eq!(client.permissions(&pool, &setup), 7);
        assert_eq!(client.permissions(&pool, &Address::generate(&env)), 0);
    }
}
