# V2 Baseline

## Provenance

- Upstream repository: `https://github.com/blend-capital/blend-contracts-v2`
- Upstream commit: `ba22b487b2c5057a4ecc28b05b5193c28e4bd117`
- Rust toolchain: `1.81`
- Soroban SDK: `22.0.7`

The upstream source was copied without its `.git` directory, build output, or
local editor files. The original repository is retained as the `upstream`
remote so later upstream changes can be inspected explicitly.

## Local source repositories

- `../blend-contracts` contains the legacy Blend repository, including the
  emitter source and the `v1.0.0` tag used by the migration mechanism.
- `../xycloans` is checked out at
  `d9a7ae163e6c7120000532bf4b15b6a96b3a82bc`.
- The `moderc3156` interface from that commit is vendored at
  `vendor/moderc3156` and used as a workspace path dependency. This makes a
  fresh clone and its CI build independent of the sibling checkout and avoids
  a GitHub dependency fetch.

Crates published through crates.io remain managed by Cargo's local registry
cache; they are not separate Git checkouts.

## Baseline policy

Before changing protocol behavior:

1. Run the inherited v2 unit and integration tests.
2. Run `test_v2_to_v3_candidate` to verify the legacy emitter migration
   harness.
3. Record intentional compatibility changes in this document or a dedicated
   design document.
4. Add a regression test before changing an existing invariant.

Do not silently upgrade the inherited v2 toolchain while establishing the
baseline. The v3 toolchain and WASM target can be upgraded separately, with
explicit compatibility tests against the deployed legacy emitter.

## Initial contract roles

- `emitter`: legacy migration and BLND distribution interface; source is
  provided by `blend-contract-sdk`.
- `backstop`: incumbent v2 implementation and starting point for the v3
  backstop.
- `pool`: v2 lending-pool implementation.
- `pool-factory`: deploys pools tied to a specific backstop.
- `test-suites`: synthetic fixtures, WASM integration tests, ledger snapshot
  tests, and migration tests.

The first v3 implementation milestone should replace the candidate backstop in
the migration baseline without changing the incumbent v2 contracts.
