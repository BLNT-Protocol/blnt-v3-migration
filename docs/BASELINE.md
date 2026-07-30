# V2 Baseline

## Provenance

- Source repository:
  `git@github.com:blend-capital/blend-contracts-v2.git`
- Pinned source commit:
  `ba22b487b2c5057a4ecc28b05b5193c28e4bd117`
- Published v2.0.0 commit:
  `c19abee5b9be4f49e0cda9057e87d343e5dcc095`
- Rust toolchain: `1.81`
- Soroban SDK: `22.0.7`

The deployed v2 source is retained as the pinned
`blend-contracts-v2` Git submodule. Its upstream dependency graph and
lockfile are preserved without modification. Initialize it after cloning:

```bash
git submodule update --init
```

Exact deployed mainnet emitter, v2 backstop, pool-factory, and pool WASMs are
retained under `deployed-artifacts/mainnet`. Their mainnet contract IDs, source
provenance, and SHA-256 digests are recorded in
`deployed-artifacts/mainnet/manifest.json`. Run `make verify-deployed-artifacts` before using them.

These deployed bytes are authoritative for compatibility tests and isolated
migration deployments. Locally rebuilt SDK-22 artifacts remain valuable for
source verification, but build and optimizer differences can change their
hashes; they MUST NOT silently replace the exact deployed fixtures.

## Baseline policy

The frozen executable behavior and candidate-only changes are documented by
the contract submodule's
[composed specification](../blend-contracts-v3/docs/SYSTEM_SPEC.md).
Migration requirements compose separately under
[SYSTEM_SPEC.md](SYSTEM_SPEC.md). Migration fixtures MUST NOT silently restate
or redefine contract behavior.

Before changing protocol behavior:

1. Verify the pinned source and deployed fixtures with `make verify-deployed-artifacts`.
2. Run the inherited v2 unit and integration tests with `make test`.
3. Run the Protocol-27 compatibility harness with
   `make test-v2-migration`.
4. Record every intentional compatibility change in
   `blend-contracts-v3/docs/V3_SYSTEM_SPEC.md`.
5. Add a regression test before changing an existing invariant.

Do not upgrade or edit the inherited v2 source submodule. It remains frozen on
Rust 1.81, Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.

New v3 implementation work belongs in the `blend-contracts-v3` submodule,
which targets Protocol 27 using Rust 1.91.1, Soroban SDK 27.0.3, and
`wasm32v1-none`. There is intentionally no SDK 26 compatibility lane.
Compatibility is defined by the exact deployed v2 and emitter bytecode running
on the current host and interacting with the pinned v3 contracts.

Updating the v3 SDK or protocol target is an explicit maintenance change and
must not modify the pinned v2 source, its dependency graph, or its lockfile.

## Contract roles

- `emitter.wasm`: deployed migration and BLND distribution boundary.
- `v2-backstop.wasm`: deployed single-tier incumbent backstop.
- `v2-pool.wasm`: deployed lending-pool implementation.
- `v2-pool-factory.wasm`: deployed pool factory.
- `blend-contracts-v2`: source tests, audit reference, and rebuild lane.
- `migration/compat-tests`: current-host compatibility and migration tests.
- `blend-contracts-v3`: pinned production v3 contract source and tests.

Emitter replacement changes only the recipient of future BLND distributions.
It does not migrate or mutate incumbent v2 supplier, borrower, or backstop
positions; those remain governed by the deployed v2 contracts and their
existing claim and withdrawal rules.
