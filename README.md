# Blend V3 Migration

Experimental migration, compatibility, and deployment harness for the
transition from Blend Protocol v2 to the v3 candidate on Stellar.

This repository pins the frozen v2 baseline and the v3 candidate contracts to
test independent legacy/v3 emitters, cross-version behavior, and network deployments.
Production v3 protocol behavior belongs to `blnt-contracts-v3`; initial
allocation and conversion behavior belongs to `blnt-backfill-contract`.
This is not an official or production-ready Blend release.

## Baseline

Initialize all pinned source submodules after cloning:

```bash
git submodule update --init --recursive
```

The deployed v2 source baseline is Blend Protocol v2 commit
`ba22b487b2c5057a4ecc28b05b5193c28e4bd117`. The Blend contract submodules are
`blend-contracts-v2/` and `blnt-contracts-v3/`. The production transition
contract is pinned in `blnt-backfill-contract/`. Protocol-27 Comet v2
weighted-pool source is pinned in the historically named
`comet-contracts-v1/`, with its Protocol-27 dependency sources pinned in
`sep-41-token/` and `soroban-fixed-point-math/`. The client-integration
submodules are `blnt-sdk-js/` and `blnt-ui/`.

Blend v1/v2 LPs use Comet v1. The fresh BLNT LPs deployed for Blend v3 use
Comet v2; the `comet-contracts-v1/` directory name is historical.

Exact mainnet emitter, v2 backstop, pool-factory, and pool WASMs are checked in
under `deployed-artifacts/mainnet/` with their contract IDs and SHA-256 digests. These
deployed bytes, rather than a local rebuild, are the authoritative
compatibility and isolated-migration fixtures. Run `make verify-deployed-artifacts` to
verify both inputs.

GitHub Actions requires a `SUBMODULES_TOKEN` repository secret with read
access to the private `blnt-contracts-v3`, `blnt-sdk-js`, and `blnt-ui`
repositories, plus `blnt-backfill-contract`.

V3 no longer replaces or depends on the deployed BLND emitter. The legacy
emitter remains with v1/v2, while v3 deploys a separate emitter that creates
BLNT for the v3 backstop from launch. The v3 emitter sends its complete
150-million-BLNT initial drop to the immutable backfill contract. That contract
owns same-address claims and a 270-day refundable BLND-to-BLNT conversion with
net outstanding conversions capped at 120 million BLNT. At expiry it
permissionlessly burns unrefunded BLND escrow and the unused BLNT conversion
reserve. The production claim manifest allocates 20 million BLNT across
the 434 positive pre-incident `flattened_total_cpal_raw` ownership rows (423
accounts and 11 contracts), vesting linearly for 180 days from construction.
An independent constructor-supplied grant list may allocate the remaining 10
million BLNT to at most 100 grantees with 720-day linear vesting. The emitter
retains the established 31-day backstop-swap API for
a future BLNT-based protocol upgrade. Existing v2 positions and claims remain
entirely governed by the legacy contracts.

See [docs/SYSTEM_SPEC.md](docs/SYSTEM_SPEC.md) for the migration repository
boundary and [docs/MIGRATION_PROTOCOL_SPEC.md](docs/MIGRATION_PROTOCOL_SPEC.md)
for the migration protocol. Contract behavior is composed by
[blnt-contracts-v3/docs/SYSTEM_SPEC.md](blnt-contracts-v3/docs/SYSTEM_SPEC.md).
[docs/BASELINE.md](docs/BASELINE.md) defines source and artifact policy, and
[docs/MIGRATION_ACCEPTANCE.md](docs/MIGRATION_ACCEPTANCE.md) defines
cross-version migration acceptance. The contract submodule's v3 specification
defines the integrated Comet v2-based backstop valuation.
Operational instructions live in
[docs/DUAL_EMITTER_DEPLOYMENT.md](docs/DUAL_EMITTER_DEPLOYMENT.md). The older
queued-migration localnet and testnet runbooks are retained only for regression
coverage of the legacy replacement path.
The integrated pool/SAC clawback and reserve-loss reconciliation fixture is
documented in
[docs/RESERVE_LOSS_TESTING.md](docs/RESERVE_LOSS_TESTING.md).

## Toolchain lanes

This repository deliberately maintains four independent source and test lanes,
with supporting Protocol-27 dependency sources pinned separately:

- `blend-contracts-v2/` is the deployed v2 source baseline, pinned to
  Rust 1.81, Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.
- `blnt-contracts-v3/` is the production candidate submodule, pinned to Rust
  1.91.1, Soroban SDK 27.0.3, Protocol 27 tooling, and `wasm32v1-none`.
- `blnt-backfill-contract/` is the production transition-contract submodule,
  pinned to the same Protocol-27 toolchain.
- `comet-contracts-v1/` is the Protocol-27 Comet v2 weighted-pool source
  submodule, pinned to Rust 1.91.1, Soroban SDK 27.0.3, and `wasm32v1-none`.
- `sep-41-token/` and `soroban-fixed-point-math/` are the corresponding
  Protocol-27 dependency source submodules, pinned to the same toolchain.
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

Run the cross-version emitter and position-isolation baseline:

```bash
make test-v2-migration
```

Run the accelerated Protocol-27 test that proves live SDK-22 v2 lending and
backstop positions remain unchanged beside the independent v3 lane:

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

## Testnet UI

Build the local v3-aware SDK once, then start the UI with the committed
testnet configuration:

```bash
cd blnt-sdk-js
npm install
npm run build
cd ../blnt-ui
npm install
npm run dev:testnet
```

Open <http://localhost:3000>. The testnet UI separates Blend's team deployment,
this repository's v2 deployment, and the v3 candidate into V1, V2, and V3
market lanes. The configured Fixed and Custom v2/v3 pools are also seeded in
the dashboard and backstop pool menus. Blend's current team testnet pools use
v2 pool bytecode; the V1 lane name identifies the requested team-deployment
lane rather than claiming those contracts are historical v1 bytecode.

## Releases

Automated tag-triggered releases are intentionally disabled while the v3
candidate remains experimental and incomplete. Add a v3-specific release
workflow only after deployable v3 contract packages and their artifact checks
exist. The repository must not publish the historical SDK-22 contracts as v3
artifacts.

## Development principles

- Preserve a passing v2 baseline before introducing behavioral changes.
- Keep the established emitter replacement interface compatible while using a
  separate v3 BLNT emitter.
- Test both native Rust contracts and deployed WASM.
- Treat activation, loss absorption, emissions, and emitter migration as
  separate protocol mechanisms.
- Keep `overflow-checks = true` for release builds.
- Do not treat this repository as audited until an independent audit is
  completed.

## License

This repository is derived from Blend Protocol v2 and remains licensed under
the GNU Affero General Public License v3.0. See [LICENSE](LICENSE).
