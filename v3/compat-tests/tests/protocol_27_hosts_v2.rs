use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

mod v2_backstop {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/optimized/backstop.wasm"
    );
}

mod v3_probe {
    soroban_sdk::contractimport!(
        file = "../target/wasm32v1-none/release/blend_v3_compat_probe.wasm"
    );
}

#[test]
fn protocol_27_executes_sdk_22_wasm_through_sdk_27_wasm() {
    let env = Env::default();
    #[allow(deprecated)]
    let protocol_version = env.ledger().protocol_version();
    assert_eq!(protocol_version, 27);

    let backstop_token = Address::generate(&env);
    let emitter = Address::generate(&env);
    let blnd_token = Address::generate(&env);
    let usdc_token = Address::generate(&env);
    let pool_factory = Address::generate(&env);

    let v2_backstop_id = env.register(
        v2_backstop::WASM,
        (
            &backstop_token,
            &emitter,
            &blnd_token,
            &usdc_token,
            &pool_factory,
            Vec::<(Address, i128)>::new(&env),
        ),
    );
    let v3_probe_id = env.register(v3_probe::WASM, ());

    let v2_client = v2_backstop::Client::new(&env, &v2_backstop_id);
    assert_eq!(v2_client.backstop_token(), backstop_token);

    let v3_client = v3_probe::Client::new(&env, &v3_probe_id);
    assert_eq!(
        v3_client.read_backstop_token(&v2_backstop_id),
        backstop_token
    );
}
