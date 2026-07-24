#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Backstop,
    LossState,
}

/// Canonical aggregate state that can prevent backstop withdrawals.
///
/// Each field counts canonical nonzero records rather than combining amounts
/// from reserves with different units. The lending, auction, and
/// loss-waterfall engines must update these fields in the same transaction
/// that creates or resolves the corresponding record.
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
    pub fn __constructor(env: Env, backstop: Address) {
        extend_instance_ttl(&env);
        env.storage().instance().set(&DataKey::Backstop, &backstop);
        env.storage()
            .instance()
            .set(&DataKey::LossState, &empty_loss_state());
    }

    pub fn backstop(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Backstop).unwrap()
    }

    /// Return the canonical aggregate loss state used by the withdrawal gate.
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

fn empty_loss_state() -> BackstopLossState {
    BackstopLossState {
        committed_loss_entries: 0,
        liability_entries: 0,
        unresolved_bad_debt_entries: 0,
    }
}

fn get_loss_state(env: &Env) -> BackstopLossState {
    env.storage().instance().get(&DataKey::LossState).unwrap()
}

/// Internal pool-accounting boundary for canonical loss state.
///
/// This is intentionally not a contract entry point. Future lending, auction,
/// and waterfall modules must call it as part of their own atomic transitions;
/// no administrator or external caller can override withdrawal eligibility.
#[allow(dead_code)]
fn set_loss_state(env: &Env, state: &BackstopLossState) {
    env.storage().instance().set(&DataKey::LossState, state);
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
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn withdrawal_gate_uses_all_canonical_loss_dimensions() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let other = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop,));
        let pool = PoolContractClient::new(&env, &pool_id);

        assert!(pool.backstop_withdrawal_allowed(&backstop));
        assert!(!pool.backstop_withdrawal_allowed(&other));

        for blocked_state in [
            BackstopLossState {
                committed_loss_entries: 1,
                liability_entries: 0,
                unresolved_bad_debt_entries: 0,
            },
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 1,
                unresolved_bad_debt_entries: 0,
            },
            BackstopLossState {
                committed_loss_entries: 0,
                liability_entries: 0,
                unresolved_bad_debt_entries: 1,
            },
        ] {
            env.as_contract(&pool_id, || set_loss_state(&env, &blocked_state));
            assert_eq!(pool.backstop_loss_state(), blocked_state);
            assert!(!pool.backstop_withdrawal_allowed(&backstop));
        }

        env.as_contract(&pool_id, || set_loss_state(&env, &empty_loss_state()));
        assert!(pool.backstop_withdrawal_allowed(&backstop));
    }

    #[test]
    fn missing_loss_state_fails_closed() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop,));
        let pool = PoolContractClient::new(&env, &pool_id);

        env.as_contract(&pool_id, || {
            env.storage().instance().remove(&DataKey::LossState);
        });

        assert!(pool.try_backstop_withdrawal_allowed(&backstop).is_err());
    }
}
