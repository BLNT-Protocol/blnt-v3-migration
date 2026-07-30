use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype,
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env,
};

mod deployed_emitter {
    soroban_sdk::contractimport!(file = "../../deployed-artifacts/mainnet/emitter.wasm");
}

#[allow(clippy::too_many_arguments)]
mod v3_backstop {
    soroban_sdk::contractimport!(
        file = "../../blend-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
    );
}

mod test_valuation {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/optimized/blend_v3_test_valuation_adapter.wasm"
    );
}

#[derive(Clone)]
#[contracttype]
struct QueuedSwap {
    new_backstop: Address,
    new_backstop_token: Address,
    unlock_time: u64,
}

#[allow(dead_code)]
#[contractclient(name = "DeployedEmitterClient")]
trait DeployedEmitter {
    fn initialize(env: Env, blnd_token: Address, backstop: Address, backstop_token: Address);
    fn get_backstop(env: Env) -> Address;
    fn get_queued_swap(env: Env) -> Option<QueuedSwap>;
}

#[derive(Clone)]
#[contracttype]
enum FactoryKey {
    Backstop,
}

#[contract]
struct MockPoolFactory;

#[contractimpl]
impl MockPoolFactory {
    pub fn __constructor(env: Env, backstop: Address) {
        env.storage()
            .instance()
            .set(&FactoryKey::Backstop, &backstop);
    }

    pub fn backstop(env: Env) -> Address {
        env.storage().instance().get(&FactoryKey::Backstop).unwrap()
    }

    pub fn is_pool(_env: Env, _pool: Address) -> bool {
        false
    }
}

const DAY: u64 = 24 * 60 * 60;
const SCALAR_7: i128 = 10_000_000;

fn create_asset(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

#[test]
fn optimized_candidate_migrates_through_the_deployed_emitter() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let token_admin = Address::generate(&env);
    let blnd = create_asset(&env, &token_admin);
    let blnd_usdc = create_asset(&env, &token_admin);
    let blnd_xlm = create_asset(&env, &token_admin);
    let usdc = create_asset(&env, &token_admin);

    let incumbent = Address::generate(&env);
    let emitter_id = env.register(deployed_emitter::WASM, ());
    StellarAssetClient::new(&env, &blnd).set_admin(&emitter_id);
    let emitter = DeployedEmitterClient::new(&env, &emitter_id);
    emitter.initialize(&blnd, &incumbent, &blnd_usdc);

    let candidate_id = Address::generate(&env);
    let factory = env.register(MockPoolFactory, (&candidate_id,));
    let valuation_admin = Address::generate(&env);
    let valuation = env.register(
        test_valuation::WASM,
        (&valuation_admin, &blnd, &usdc, &blnd_usdc, &blnd_xlm),
    );
    env.register_at(
        &candidate_id,
        v3_backstop::WASM,
        (
            &blnd_usdc,
            &blnd_xlm,
            &emitter_id,
            &blnd,
            &usdc,
            &factory,
            &valuation,
        ),
    );
    let candidate = v3_backstop::Client::new(&env, &candidate_id);

    let blnd_usdc_token = StellarAssetClient::new(&env, &blnd_usdc);
    blnd_usdc_token.mint(&incumbent, &(100 * SCALAR_7));
    blnd_usdc_token.mint(&candidate_id, &(100 * SCALAR_7 + 1));

    candidate.begin_migration();
    let queued = emitter.get_queued_swap().unwrap();
    assert_eq!(queued.new_backstop, candidate_id);
    assert_eq!(queued.new_backstop_token, blnd_xlm);

    env.ledger().set_timestamp(queued.unlock_time - 7 * DAY);
    candidate.prepare_migration();
    assert_eq!(
        candidate.migration_status(),
        v3_backstop::MigrationStatus::Prepared
    );

    env.ledger().set_timestamp(queued.unlock_time);
    assert_eq!(candidate.finalize_migration(), queued.unlock_time);

    assert_eq!(emitter.get_backstop(), candidate_id);
    assert!(emitter.get_queued_swap().is_none());
    assert_eq!(
        candidate.migration_status(),
        v3_backstop::MigrationStatus::Active
    );
    assert_eq!(candidate.activated_at(), Some(queued.unlock_time));
    assert_eq!(candidate.backstop_token(), blnd_usdc);
    assert_eq!(
        candidate.tier_token(&v3_backstop::BackstopTier::BlndXlm),
        blnd_xlm
    );
    assert_eq!(candidate.backstop_valuation(), valuation);
}
