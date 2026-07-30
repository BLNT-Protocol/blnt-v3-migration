#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

#[contract]
pub struct CompatibilityProbe;

#[contractimpl]
impl CompatibilityProbe {
    /// Read the designated backstop token through the deployed v2 interface.
    pub fn read_backstop_token(env: Env, backstop: Address) -> Address {
        env.invoke_contract(
            &backstop,
            &Symbol::new(&env, "backstop_token"),
            Vec::new(&env),
        )
    }
}
