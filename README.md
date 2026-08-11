# Blend V3 Migration

Experimental migration, compatibility, and deployment harness for the
transition from Blend Protocol v2 to the v3 candidate on Stellar.

This repository pins the frozen v2 baseline and the v3 candidate contracts to
test emitter migration, cross-version behavior, and network deployments.
Production v3 contract behavior belongs to the `blend-contracts-v3` submodule.
This is not an official or production-ready Blend release.

## Baseline

Initialize all pinned source submodules after cloning:

```bash
git submodule update --init --recursive
```

The deployed v2 source baseline is Blend Protocol v2 commit
`ba22b487b2c5057a4ecc28b05b5193c28e4bd117`. The contract submodules are
`blend-contracts-v2/` and `blend-contracts-v3/`. The client-integration
submodules are `blend-sdk-js/` and `blend-ui/`.

Exact mainnet emitter, v2 backstop, pool-factory, and pool WASMs are checked in
under `deployed-artifacts/mainnet/` with their contract IDs and SHA-256 digests. These
deployed bytes, rather than a local rebuild, are the authoritative
compatibility and isolated-migration fixtures. Run `make verify-deployed-artifacts` to
verify both inputs.

GitHub Actions requires a `SUBMODULES_TOKEN` repository secret with read
access to the private `blend-contracts-v3`, `blend-sdk-js`, and `blend-ui`
repositories.

The emitter migration does not upgrade the deployed v2 contracts or move
their supplier, borrower, or backstop positions. Immediately before switching
recipients, the deployed emitter mints its final time-accrued BLND distribution
to the incumbent backstop. Subsequent emitter distributions go to v3, while
existing v2 positions and previously accrued v2 claims remain governed by the
v2 contracts. This routing lasts while v3 remains the emitter's registered
backstop.

See [docs/SYSTEM_SPEC.md](docs/SYSTEM_SPEC.md) for the migration repository
boundary and [docs/MIGRATION_PROTOCOL_SPEC.md](docs/MIGRATION_PROTOCOL_SPEC.md)
for the migration protocol. Contract behavior is composed by
[blend-contracts-v3/docs/SYSTEM_SPEC.md](blend-contracts-v3/docs/SYSTEM_SPEC.md).
[docs/BASELINE.md](docs/BASELINE.md) defines source and artifact policy, and
[docs/MIGRATION_ACCEPTANCE.md](docs/MIGRATION_ACCEPTANCE.md) defines
cross-version migration acceptance. The contract submodule's v3 specification
defines the integrated Comet-based backstop valuation.
Operational instructions live in
[docs/LOCALNET_MIGRATION.md](docs/LOCALNET_MIGRATION.md). The isolated
native-XLM public-testnet lane is documented in
[docs/TESTNET_PRODUCTION_MIGRATION.md](docs/TESTNET_PRODUCTION_MIGRATION.md).

## Toolchain lanes

This repository deliberately maintains three independent source and test
lanes:

- `blend-contracts-v2/` is the deployed v2 source baseline, pinned to
  Rust 1.81, Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.
- `blend-contracts-v3/` is the production candidate submodule, pinned to Rust
  1.91.1, Soroban SDK 27.0.3, Protocol 27 tooling, and `wasm32v1-none`.
- `migration-harness/` contains only Protocol-27 cross-version tests and
  test-only support contracts.

There is no SDK 26 compatibility lane. Compatibility tests span the historical
SDK-22 implementation and the current v3 candidate.

## Build and test

Build the deployed v2 contracts:

```bash
make
```

Run the deployed v2 test suite:

```bash
make test
```

Run the candidate migration baseline:

```bash
make test-v2-migration
```

Run the accelerated Protocol-27 test that proves live SDK-22 v2 lending and
backstop positions survive the emitter migration unchanged:

```bash
make test-v2-positions-survive-migration
```

Check the v3 workspace with its pinned current toolchain:

```bash
make check-v3
```

This verifies the exact deployed fixtures and builds the SDK-27 compatibility
probe, candidate contracts, and test-only lending oracle. It then runs the
contract suites and the deployed-WASM cross-version migration harness. Exact behavioral
requirements remain in the contract and migration specifications rather than
being restated here.

Run both lanes:

```bash
make test-all
```

## Releases

Automated tag-triggered releases are intentionally disabled while the v3
candidate remains experimental and incomplete. Add a v3-specific release
workflow only after deployable v3 contract packages and their artifact checks
exist. The repository must not publish the historical SDK-22 contracts as v3
artifacts.

## Development principles

- Preserve a passing v2 baseline before introducing behavioral changes.
- Keep the deployed emitter interface compatible.
- Test both native Rust contracts and deployed WASM.
- Treat activation, loss absorption, emissions, and emitter migration as
  separate protocol mechanisms.
- Keep `overflow-checks = true` for release builds.
- Do not treat this repository as audited until an independent audit is
  completed.

## License

This repository is derived from Blend Protocol v2 and remains licensed under
the GNU Affero General Public License v3.0. See [LICENSE](LICENSE).
