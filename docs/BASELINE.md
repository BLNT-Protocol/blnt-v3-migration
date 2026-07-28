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

The frozen executable behavior is documented normatively in
[V2_SYSTEM_SPEC.md](V2_SYSTEM_SPEC.md). Candidate-only changes are documented
in [V3_SYSTEM_SPEC.md](V3_SYSTEM_SPEC.md) and compose under
[SYSTEM_SPEC.md](SYSTEM_SPEC.md). `V3_SYSTEM_SPEC.md` MUST NOT silently restate
or redefine an inherited v2 rule.

Before changing protocol behavior:

1. Run the inherited v2 unit and integration tests.
2. Run `test_v2_to_v3_candidate` to verify the legacy emitter migration
   harness.
3. Record every intentional compatibility change in `V3_SYSTEM_SPEC.md` and
   cross-reference it in `V2_PARITY.md`.
4. Add a regression test before changing an existing invariant.

Do not upgrade the inherited v2 toolchain. The repository root remains frozen
on Rust 1.81, Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.

New v3 implementation work belongs in the independent `v3/` workspace, which
targets Protocol 27 using Rust 1.91.0, Soroban SDK 27.0.2, and
`wasm32v1-none`. There is intentionally no SDK 26 compatibility lane.
Compatibility is defined by migration behavior between the historical v2
contracts, the deployed legacy emitter boundary, and the current v3 candidate.

Updating the v3 SDK or protocol target is an explicit maintenance change and
must not modify the root v2 dependency graph or `Cargo.lock`.

## Initial contract roles

- `emitter`: legacy migration and BLND distribution interface; source is
  provided by `blend-contract-sdk`.
- `backstop`: incumbent v2 implementation and starting point for the v3
  backstop.
- `pool`: v2 lending-pool implementation.
- `pool-factory`: deploys pools tied to a specific backstop.
- `test-suites`: synthetic fixtures, WASM integration tests, ledger snapshot
  tests, and migration tests.

The Protocol-27 migration harness now uses the isolated v3 backstop as the
candidate without changing the incumbent v2 contracts. The root SDK-22 test
retains a second v2 backstop only as a frozen historical-emitter baseline.
Emitter replacement changes only the recipient of future BLND distributions.
It does not migrate or mutate incumbent v2 supplier, borrower, or backstop
positions; those remain governed by the deployed v2 contracts and their
existing claim and withdrawal rules.
