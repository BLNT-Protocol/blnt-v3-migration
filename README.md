# BLNT V3 Migration and Deployment

Migration, compatibility, and deployment harness for BLNT V3, an immutable
lending protocol derived from Blend and designed to support real-world assets.

This repository pins the frozen Blend v2 baseline and the BLNT V3 contracts to
test independent legacy/V3 emitters, cross-version behavior, and network deployments.
Production v3 protocol behavior belongs to `blnt-contracts-v3`; initial
allocation and conversion behavior belongs to `blnt-backfill-contract`.
BLNT V3 is independently developed and is not an official Blend release.

## Baseline

Initialize all pinned source submodules after cloning:

```bash
git submodule update --init --recursive
```

The deployed v2 source baseline is Blend Protocol v2 commit
`ba22b487b2c5057a4ecc28b05b5193c28e4bd117`. The Blend contract submodules are
`blend-contracts-v2/` and `blnt-contracts-v3/`. The production transition
contract is pinned in `blnt-backfill-contract/`. Upstream Comet V1.1 source is
pinned at tag `v1.1.0` in `comet-contracts-v1/`; V3 uses the official
`v1.1.0_contracts_pkg1.0.0_cli25.1.0` release WASM embedded in
`blnt-contracts-v3/comet.wasm`. V3's
SEP-41 compatibility wrapper is maintained within
`blnt-contracts-v3/compat/sep-41-token/`. The client-integration submodules
are `blnt-sdk-js/` and `blnt-ui/`. The independently deployed, status-only
pool administrator is pinned in `pool-status-admin/`; its mainnet deployment
is recorded in `deployments/mainnet-v3-pool-status-admin.json`. The
YieldBlent backstop-management contract is pinned in `backstop-manager/`;
its mainnet deployment workflow is documented in
`docs/YIELDBLENT_BACKSTOP_MANAGER_DEPLOYMENT.md`. The separate contributor-
funded remediation contract for pre-incident Blend lockup positions is pinned
in `blnt-lockup-backfill-contract/`. The separate owner-authorized helper for
atomic two-tier YieldBlent deposits and emissions claims is
pinned in `blnt-backstop-manager-helper/`. The independently versioned
off-chain auction automation is pinned in `auctioneer-bot/`; its example
mainnet configuration monitors Fixed Pool V3 and YieldBlent Pool by default.

Blend v1/v2 LPs use Comet v1. The canonical BLNT LPs deployed for BLNT V3 use
Comet V1.1.

Exact mainnet emitter, v2 backstop, pool-factory, and pool WASMs are checked in
under `deployed-artifacts/mainnet/` with their contract IDs and SHA-256 digests. These
deployed bytes, rather than a local rebuild, are the authoritative
compatibility and isolated-migration fixtures. Run `make verify-deployed-artifacts` to
verify both inputs.

GitHub Actions requires a `SUBMODULES_TOKEN` repository secret with read
access to the private `blnt-contracts-v3`, `blnt-sdk-js`, and `blnt-ui`
repositories, plus `blnt-backfill-contract` and
`blnt-lockup-backfill-contract` and `blnt-backstop-manager-helper`.

V3 no longer replaces or depends on the deployed BLND emitter. The legacy
emitter remains with v1/v2, while v3 deploys a separate emitter that creates
BLNT for the v3 backstop from launch. The v3 emitter sends its complete
150-million-BLNT initial drop to the immutable backfill contract. That contract
owns same-address claims and a 270-day, 1:1 BLND-to-BLNT conversion capped at
51 million BLNT output and 51 million BLND input. Converted BLND is burned immediately; at expiry it
permissionlessly burns the unused BLNT conversion reserve. The production
claim manifest allocates 74 million BLNT across
the 434 positive pre-incident `flattened_total_cpal_raw` ownership rows (423
accounts and 11 contracts), vesting linearly over 180 days from construction.
An independent constructor-supplied grant list allocates the remaining 25
million BLNT to at most 100 grantees with 360-day linear vesting. The emitter
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
defines the integrated Comet V1.1-based backstop valuation.
Operational instructions live in
[docs/DUAL_EMITTER_DEPLOYMENT.md](docs/DUAL_EMITTER_DEPLOYMENT.md). The
mainnet core and Fixed-pool deployment procedures are documented separately in
[docs/MAINNET_V3_DEPLOYMENT.md](docs/MAINNET_V3_DEPLOYMENT.md). The older
queued-migration localnet and testnet runbooks are retained only for regression
coverage of the legacy replacement path.
Every production contract deployment is subject to the shared source,
release-attestation, constructor-binding, and evidence gate in
[docs/CONTRACT_DEPLOYMENT_READINESS.md](docs/CONTRACT_DEPLOYMENT_READINESS.md).
The response process for a critical finding in the immutable mainnet contracts
is documented in
[docs/IMMUTABLE_SECURITY_RESPONSE.md](docs/IMMUTABLE_SECURITY_RESPONSE.md).
The integrated pool/SAC clawback and reserve-loss reconciliation fixture is
documented in
[docs/RESERVE_LOSS_TESTING.md](docs/RESERVE_LOSS_TESTING.md).

## Toolchain lanes

This repository deliberately maintains five independent source and test lanes,
with supporting Protocol-27 dependency sources pinned separately:

- `blend-contracts-v2/` is the deployed v2 source baseline, pinned to
  Rust 1.81, Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.
- `blnt-contracts-v3/` is the production BLNT V3 submodule, pinned to Rust
  1.91.1, Soroban SDK 27.0.3, Protocol 27 tooling, and `wasm32v1-none`.
- `blnt-backfill-contract/` is the production transition-contract submodule,
  pinned to the same Protocol-27 toolchain.
- `pool-status-admin/` is the status-only pool-administration controller source;
  it remains a separate Soroban SDK 25 build lane.
- `backstop-manager/` is the YieldBlent backstop-management controller source,
  pinned to Rust 1.91.1, Soroban SDK 27.0.3, and `wasm32v1-none`.
- `blnt-lockup-backfill-contract/` is the separately deployed lockup-position
  remediation contract, pinned to its reviewed production source commit.
- `blnt-backstop-manager-helper/` is the separately deployed two-tier
  backstop-manager orchestration helper, pinned to Rust 1.91.1, Soroban SDK
  27.0.3, and `wasm32v1-none`.
- `auctioneer-bot/` is the Node 22 off-chain auction automation for BLNT V3. It
  is isolated from the immutable contract build lanes.
- `comet-contracts-v1/` is the upstream Comet V1.1 weighted-pool source at
  tag `v1.1.0` commit `b3a2d827ed6c158341f8e67ea91eb3b0395d3871`.
- `blnt-contracts-v3/comet.wasm` is the official Comet V1.1 release artifact
  with SHA-256 `d735c3395f59510172cf5cf838823a1389b97cfec6bf24e580e9bd77d2b3e687`.
- `migration-harness/` contains only Protocol-27 cross-version tests and
  test-only support contracts.

There is no SDK 26 compatibility lane. Compatibility tests span the historical
SDK-22 implementation and BLNT V3.

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
probe, BLNT V3 contracts, and test-only lending oracle. It then runs the
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
this repository's v2 deployment, and BLNT V3 into V1, V2, and V3
market lanes. The configured Fixed and Custom v2/v3 pools are also seeded in
the dashboard and backstop pool menus. Blend's current team testnet pools use
v2 pool bytecode; the V1 lane name identifies the requested team-deployment
lane rather than claiming those contracts are historical v1 bytecode.

## Releases

Automated tag-triggered releases are intentionally disabled. Release evidence
for the deployed immutable BLNT V3 contracts is recorded by the repository's
deployment manifests and artifact checks. The repository must not publish the
historical SDK-22 contracts as V3 artifacts.

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
