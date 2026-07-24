#![no_std]

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 89 * DAY_IN_LEDGERS;
const INSTANCE_TTL_BUMP: u32 = 90 * DAY_IN_LEDGERS;
const POOL_TTL_THRESHOLD: u32 = 179 * DAY_IN_LEDGERS;
const POOL_TTL_BUMP: u32 = 180 * DAY_IN_LEDGERS;

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Backstop,
    PoolWasmHash,
}

#[derive(Clone)]
#[contracttype]
enum PersistentKey {
    Pool(Address),
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolDeployed {
    pub deployer: Address,
    pub pool: Address,
}

#[contract]
pub struct PoolFactoryContract;

#[contractimpl]
impl PoolFactoryContract {
    pub fn __constructor(env: Env, backstop: Address, pool_wasm_hash: BytesN<32>) {
        extend_instance_ttl(&env);
        env.storage().instance().set(&DataKey::Backstop, &backstop);
        env.storage()
            .instance()
            .set(&DataKey::PoolWasmHash, &pool_wasm_hash);
    }

    pub fn backstop(env: Env) -> Address {
        extend_instance_ttl(&env);
        env.storage().instance().get(&DataKey::Backstop).unwrap()
    }

    pub fn pool_wasm_hash(env: Env) -> BytesN<32> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::PoolWasmHash)
            .unwrap()
    }

    /// Deploy a pool whose immutable backstop is this factory's configured
    /// candidate. The deployer authorizes use of its deterministic salt domain.
    pub fn deploy(env: Env, deployer: Address, salt: BytesN<32>) -> Address {
        extend_instance_ttl(&env);
        deployer.require_auth();

        let backstop: Address = env.storage().instance().get(&DataKey::Backstop).unwrap();
        let pool_wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PoolWasmHash)
            .unwrap();
        let mut deployer_bytes = [0_u8; 56];
        deployer.to_string().copy_into_slice(&mut deployer_bytes);
        let mut salted_deployer: Bytes = salt.into_val(&env);
        salted_deployer.extend_from_array(&deployer_bytes);
        let effective_salt = env.crypto().keccak256(&salted_deployer);
        let pool = env
            .deployer()
            .with_current_contract(effective_salt)
            .deploy_v2(pool_wasm_hash, (&backstop,));
        let key = PersistentKey::Pool(pool.clone());
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, POOL_TTL_THRESHOLD, POOL_TTL_BUMP);

        PoolDeployed {
            deployer,
            pool: pool.clone(),
        }
        .publish(&env);
        pool
    }

    pub fn is_pool(env: Env, pool: Address) -> bool {
        extend_instance_ttl(&env);
        let key = PersistentKey::Pool(pool);
        let registered = env.storage().persistent().get(&key).unwrap_or(false);
        if registered {
            env.storage()
                .persistent()
                .extend_ttl(&key, POOL_TTL_THRESHOLD, POOL_TTL_BUMP);
        }
        registered
    }
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
    fn stores_immutable_configuration_and_rejects_unknown_contract() {
        let env = Env::default();
        let backstop = Address::generate(&env);
        let pool_hash = BytesN::from_array(&env, &[3; 32]);
        let factory_id = env.register(PoolFactoryContract, (&backstop, &pool_hash));
        let factory = PoolFactoryContractClient::new(&env, &factory_id);

        assert_eq!(factory.backstop(), backstop);
        assert_eq!(factory.pool_wasm_hash(), pool_hash);
        assert!(!factory.is_pool(&Address::generate(&env)));
    }
}
