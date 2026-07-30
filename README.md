# Blend V3 Candidate

Experimental Blend Protocol v3 candidate implementation for Stellar.

This repository starts from Blend Protocol v2 and is intended to develop and
test backwards-compatible protocol migration, multi-asset backstops, and
candidate v3 pool behavior. It is not an official or production-ready Blend
release.

## Baseline

Initialize both pinned contract-source submodules after cloning:

```bash
git submodule update --init --recursive
```

The deployed v2 source baseline is Blend Protocol v2 commit
`ba22b487b2c5057a4ecc28b05b5193c28e4bd117`. The source submodules use the
GitHub SSH URLs `git@github.com:blend-capital/blend-contracts-v2.git` and
`git@github.com:levinson/blend-contracts-v3.git`.

Exact mainnet emitter, v2 backstop, pool-factory, and pool WASMs are checked in
under `deployed-artifacts/mainnet/` with their contract IDs and SHA-256 digests. These
deployed bytes, rather than a local rebuild, are the authoritative
compatibility and isolated-migration fixtures. Run `make verify-deployed-artifacts` to
verify both inputs.

GitHub Actions requires a fine-grained `SUBMODULES_TOKEN` repository secret
with read access to the private `blend-contracts-v3` repository.

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
[docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) records
non-normative candidate progress. See
[docs/BASELINE.md](docs/BASELINE.md) for source and artifact policy, and
[docs/MIGRATION_TEST_PLAN.md](docs/MIGRATION_TEST_PLAN.md) for the migration
test requirements. The contract submodule owns the normative production
LP-pricing specification, while the fail-closed provider qualification and
deployment process remains in
[docs/ORACLE_DEPLOYMENT.md](docs/ORACLE_DEPLOYMENT.md). Operational instructions
live in
[docs/LOCALNET_MIGRATION.md](docs/LOCALNET_MIGRATION.md) and
[docs/TESTNET_MIGRATION.md](docs/TESTNET_MIGRATION.md). The isolated
production-adapter testnet lane is documented in
[docs/TESTNET_PRODUCTION_MIGRATION.md](docs/TESTNET_PRODUCTION_MIGRATION.md).

## Toolchain lanes

This repository deliberately maintains three independent source and test
lanes:

- `blend-contracts-v2/` is the deployed v2 source baseline, pinned to
  Rust 1.81, Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.
- `blend-contracts-v3/` is the production candidate submodule, pinned to Rust
  1.91.1, Soroban SDK 27.0.3, Protocol 27 tooling, and `wasm32v1-none`.
- `migration/` contains only Protocol-27 cross-version tests and test-only
  support contracts.

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

This verifies the exact deployed fixtures and builds the SDK-27
compatibility probe, candidate contracts, production valuation adapter, and
deterministic test adapter. The compatibility
harness verifies both a cross-contract call into the SDK-22 backstop and a
complete deployed-emitter migration into the v3 candidate on a Protocol-27
host. The production adapter uses immutable SEP-40 BLND/USDC and XLM/USDC
TWAPs plus conservative 80:20 Comet reserve valuation. The deterministic
adapter remains a fixture only. The candidate pool persists v2-compatible
status transitions using that canonical boundary and a transferable v2-style
pool admin. Its SDK-27 lending core executes the six non-auction v2 request
types with v2-compatible mutable pool parameters and timelocked reserve
configuration, checked interest, share, health, authorization, and
token-transfer accounting. The pool also supports
permissionless v2-compatible user-liquidation auctions. Completion of an
auction that exhausts the liquidatee's collateral atomically transfers all
residual dToken liabilities to the configured backstop and its canonical loss
records, without changing reserve dToken supply. It can
prepare and atomically fill one pool-authorized, single-tier, 120%-of-debt
backstop auction by selecting the first tier with at least $200 of verified
value in strict BLND:USDC LP, BLND:XLM LP, then plain-USDC order. An
authenticated healthy filler assumes the time-scaled dToken bid and receives
the time-scaled tier token directly. The three-tier backstop impairs its
pool-local asset/share exchange rate only by the amount actually transferred,
so active and queued shares absorb the loss proportionally. After an auction
ends, any caller can continue the waterfall in a separate bounded transaction.
The pool prepares the next strict-priority tier auction while one qualifies;
only a verified no-eligible-tier quote defaults all residual backstop
liabilities to suppliers using Blend v2's bToken exchange-rate reduction.
An existing bad-debt auction remains unchanged when a later user liquidation
adds liabilities; permissionless continuation handles those residual
liabilities after the auction clears.

The candidate also fixes ongoing BLND at the inherited 70% backstop-depositor
and 30% pool supply/borrow split, with deterministic top-level carry. Its
emission quote boundary derives underlying BLND directly from the current
BLND reserve and total supply of the BLND:USDC and BLND:XLM Comet contracts.
That same-invocation composition is an intentional spot policy and therefore
accepts reserve-ratio manipulation risk at a checkpoint. Permissionless,
maximum-30 reward-zone membership, threshold entry/removal, and spot-BLND
replacement are implemented. USDC alone can activate a pool, but reward-zone
entry requires positive eligible underlying BLND; a zero-BLND member can be
removed without a checkpoint so it cannot block distribution. After migration,
the permissionless distributor verifies the configured BLND token through
either exact-delta backfill funding or an exact delta around the first positive
ongoing emitter call. The latter keeps ongoing distribution available when a
direct-transfer-only migration has zero eligible backfill weight. The
distributor then reconciles the candidate's actual unaccounted BLND balance,
excludes the segregated backfill reserve, allocates both tranches across the
bounded reward zone by current eligible underlying BLND, carries each floor
remainder at its own scope, and writes the production distribution checkpoint
only after the complete allocation succeeds. Active BLND-tier deposits, Q4W
changes, withdrawals, losses, and gains require that completed checkpoint to be
no more than five seconds old before changing pool weight, bounding the
unresolved allocation window to one ledger. The 70% pool allocation is frozen
into scaled BLND:USDC and BLND:XLM per-share indexes in that same distribution;
user checkpoints exclude late deposits and queued shares, preserve scoped
rounding carry, and support owner-authorized BLND claims to a selected
recipient. For the 30% tranche, the pool admin configures v2-compatible
supply/debt-token weights. A pool-authorized once-daily gulp reserves the
allocation without moving BLND out of candidate custody, then seven-day
reserve-token streams checkpoint every balance mutation and carry integer dust
at pool, reserve, and user scope. Owner-authorized claims over selected
reserve-token identifiers are paid directly by the candidate to the chosen
recipient.

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
