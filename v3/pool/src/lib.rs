#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, Address, BytesN, Env, Map,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const MAX_LOSS_RECORDS_PER_KIND: u32 = 30;
const STATUS_SETUP: u32 = 6;
const ACTION_SUPPLY: u32 = 0;
const ACTION_SUPPLY_COLLATERAL: u32 = 2;
const ACTION_BORROW: u32 = 4;
const ACTION_DELETE_LIQUIDATION: u32 = 9;
const MAX_ACTION_TYPE: u32 = ACTION_DELETE_LIQUIDATION;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Backstop,
    LossRecords,
    Operator,
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
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStatusChanged {
    pub operator_action: bool,
    pub previous_status: u32,
    pub status: u32,
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
    pub fn __constructor(env: Env, backstop: Address, operator: Address) {
        extend_instance_ttl(&env);
        env.storage().instance().set(&DataKey::Backstop, &backstop);
        env.storage().instance().set(&DataKey::Operator, &operator);
        env.storage()
            .instance()
            .set(&DataKey::Status, &STATUS_SETUP);
        env.storage()
            .instance()
            .set(&DataKey::LossRecords, &empty_loss_records(&env));
    }

    pub fn backstop(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Backstop).unwrap()
    }

    /// Return this pool's immutable local operator.
    pub fn operator(env: Env) -> Address {
        extend_instance_ttl(&env);
        get_operator(&env)
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

    /// Request one of the bounded pool-local operator status overrides.
    pub fn set_pool_status(env: Env, requested_status: u32) -> u32 {
        extend_instance_ttl(&env);
        get_operator(&env).require_auth();
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
    /// Future lending and auction entry points must call the same internal
    /// predicate before performing their accounting transition.
    pub fn action_allowed(env: Env, action_type: u32) -> bool {
        extend_instance_ttl(&env);
        require_valid_action(&env, action_type);
        action_allowed_for_status(get_status(&env), action_type)
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

fn get_operator(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Operator).unwrap()
}

fn get_status(env: &Env) -> u32 {
    env.storage().instance().get(&DataKey::Status).unwrap()
}

fn apply_status_quote(
    env: &Env,
    previous_status: u32,
    quote: &PoolStatusQuote,
    operator_action: bool,
) -> u32 {
    if !quote.transition_allowed {
        panic_with_error!(env, PoolError::StatusTransitionNotAllowed);
    }
    require_valid_status(env, quote.status);
    env.storage()
        .instance()
        .set(&DataKey::Status, &quote.status);
    PoolStatusChanged {
        operator_action,
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

#[allow(dead_code)]
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
        testutils::{Address as _, Events, MockAuth, MockAuthInvoke},
        IntoVal,
    };

    #[contract]
    struct MockBackstop;

    #[derive(Clone)]
    #[contracttype]
    enum MockBackstopKey {
        SetQuote,
        UpdateQuote,
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

    #[test]
    fn stores_immutable_status_configuration_and_applies_canonical_quotes() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let operator = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop_id, &operator));
        let pool = PoolContractClient::new(&env, &pool_id);

        assert_eq!(pool.backstop(), backstop_id);
        assert_eq!(pool.operator(), operator);
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
    fn operator_status_request_requires_the_immutable_operator() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let backstop = MockBackstopClient::new(&env, &backstop_id);
        let operator = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop_id, &operator));
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
    fn action_policy_matches_the_v2_status_matrix() {
        let env = Env::default();
        let backstop_id = env.register(MockBackstop, ());
        let operator = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop_id, &operator));
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
    fn withdrawal_gate_tracks_each_canonical_record_kind() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let operator = Address::generate(&env);
        let other = Address::generate(&env);
        let pool_id = env.register(PoolContract, (&backstop, &operator));
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
        let pool_id = env.register(PoolContract, (&backstop, &operator));
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
        let pool_id = env.register(PoolContract, (&backstop, &operator));
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
        let pool_id = env.register(PoolContract, (&backstop, &operator));
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
        let pool_id = env.register(PoolContract, (&backstop, &operator));
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
        let pool_id = env.register(PoolContract, (&backstop, &operator));

        env.as_contract(&pool_id, || {
            for _ in 0..MAX_LOSS_RECORDS_PER_KIND {
                set_backstop_liability(&env, &Address::generate(&env), 1);
            }
            set_backstop_liability(&env, &Address::generate(&env), 1);
        });
    }
}
