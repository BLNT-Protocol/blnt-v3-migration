#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, BytesN, Env,
    Map,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const MAX_LOSS_RECORDS_PER_KIND: u32 = 30;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Backstop,
    LossRecords,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PoolError {
    InvalidLossAmount = 1300,
    TooManyLossRecords = 1301,
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
    pub fn __constructor(env: Env, backstop: Address) {
        extend_instance_ttl(&env);
        env.storage().instance().set(&DataKey::Backstop, &backstop);
        env.storage()
            .instance()
            .set(&DataKey::LossRecords, &empty_loss_records(&env));
    }

    pub fn backstop(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Backstop).unwrap()
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
    BackstopLossState {
        committed_loss_entries: records.committed_losses.len(),
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
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn withdrawal_gate_tracks_each_canonical_record_kind() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let other = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop,));
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
        let pool_id = env.register(PoolContract, (&backstop,));
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
        let pool_id = env.register(PoolContract, (&backstop,));
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
        let pool_id = env.register(PoolContract, (&backstop,));
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
        let pool_id = env.register(PoolContract, (&backstop,));
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
        let pool_id = env.register(PoolContract, (&backstop,));

        env.as_contract(&pool_id, || {
            for _ in 0..MAX_LOSS_RECORDS_PER_KIND {
                set_backstop_liability(&env, &Address::generate(&env), 1);
            }
            set_backstop_liability(&env, &Address::generate(&env), 1);
        });
    }
}
