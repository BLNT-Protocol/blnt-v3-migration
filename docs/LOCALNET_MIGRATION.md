# Local-Network Migration Exercise

The local-network runner deploys a frozen SDK-22 incumbent and an SDK-27
candidate against a Protocol-27 Stellar Quickstart network configured with
testnet Soroban resource limits. It records contract IDs, transaction results,
CLI `--cost` output, balances, and final invariants below the ignored
`.localnet/` directory.

The same transaction assembly is reused by the native-XLM public-testnet
wrapper documented in
[TESTNET_PRODUCTION_MIGRATION.md](TESTNET_PRODUCTION_MIGRATION.md).
Network-specific state and keys remain separate.

The runner uses independently issued local assets and real Comet contracts:

- `BLND`
- A BLND:USDC Comet LP, representing the incumbent designated token
- A BLND:XLM Comet LP, representing the candidate's next designated token and
  first-loss backstop tier
- `USDC`, representing the plain-USDC third backstop tier
- `XLM`, representing the isolated pair asset for the next Comet LP

The two LPs execute the checked-in Comet WASM and expose current reserve
composition to the candidate's integrated valuation and emission calculations.
Their isolated initial balances are deterministic fixtures; they do not
represent production market pricing or liquidity. Separate authenticated
SEP-40 fixtures provide the modeled pools' deterministic reserve prices and
are not used for backstop valuation.

The migration runner deploys matched Fixed and YieldBlox models against the
incumbent and candidate before opening the emitter queue. Both v3 pools receive
all three backstop tiers, cross the activation threshold, enter the reward
zone, and retain positive BLND:USDC and BLND:XLM emission weight. Fixed v3
routes its pool tranche to EURC borrowing, while YieldBlox v3 routes it to USDC
supply. Both incumbent pools receive BLND:USDC backstop capital and enter the
v2 reward zone.

The classic-asset issuer/operator and candidate depositor use separate local
identities. The runner creates the required modeled-asset trustlines,
initializes both Comet pools, and mints the LP positions before funding the two
backstops and lending positions.

The local operator pays for deployment and migration transactions but receives
no continuing protocol authority. The exercise uses only the deployed emitter's
permissionless queue and swap mechanism; it deploys no governance, multisig,
administrator recovery, or alternate upgrade path.

This test deployment configures the candidate with an empty discretionary
recipient list. During the queue, the candidate accrues up to 10 million BLND
through its ordinary 70/30 emission indexes: the backstop tranche is
BLND:USDC-only, while the pool tranche follows each pool's supply/borrow
configuration. After activation, permissionless candidate `drop()` funds
exactly that schedule through the emitter's initial drop; the finalize command
invokes and verifies it when the schedule is positive. The otherwise available
discretionary allocation remains undistributed in this deployment.

## Prerequisites

- Stellar CLI 27
- Docker with a running daemon
- Rust 1.81 and 1.91.1 toolchains configured by the repository
- `jq`, `make`, and `shasum`

The runner verifies and deploys the exact mainnet emitter and incumbent v2
WASMs recorded in `deployed-artifacts/mainnet/manifest.json`. The pinned SDK-22 source
submodule remains a separate build-and-test lane; its rebuilt bytes are not
substituted into this migration exercise. The candidate builds directly from
the pinned `blend-contracts-v3/` submodule, which also owns the contract
specifications. Its build rejects an optimized backstop above the conservative
120,000-byte deployment guard.

## Commands

Review the non-mutating deployment plan and validate all local artifacts:

```bash
make localnet-plan
make localnet-validate
```

Start a new ephemeral network, deploy both stacks, establish the strictly
greater candidate balance, and begin the migration:

```bash
make localnet-run
```

The runner deliberately does not shorten or bypass the production 31-day
emitter queue. Quickstart ledger timestamps advance with wall-clock time, so
the exercise is resumable. Keep the Mac, Colima VM, and named Quickstart
container running; restarting any of them destroys this ephemeral ledger.
Inspect the next required time gate with:

```bash
make localnet-status
```

Quickstart's local network closes one ledger per second, while the incumbent
and candidate TTL constants assume roughly five seconds per ledger. Run the
transaction-backed keepalive at least every five days while the queue is open;
daily execution is recommended:

```bash
make localnet-keepalive
```

This explicitly extends the deployed emitter, incumbent distribution
checkpoint, both Comets, all four modeled pools and their lending oracles,
configured assets, backstop positions, and relevant token balances. It also
refreshes each lending-oracle history. Before reporting success, it verifies
the saved queue target, next designated token, unlock time, and the candidate's
strict balance majority. It does not alter queue timestamps, balances, shares,
pool status, distribution timestamps, or migration eligibility. Each
keepalive result and resource-cost report is retained in the run evidence
directory.

If the Mac sleeps, the container may remain running while Stellar Core stops
closing ledgers and the RPC reports stale health. Recover that process and
retain the existing database and migration state with:

```bash
make localnet-recover
```

If the Colima VM or Quickstart container itself was stopped, the ephemeral
ledger is gone and cannot be recovered.

During the final seven days before the queued unlock:

```bash
make localnet-prepare
```

At or after the unlock:

```bash
make localnet-finalize
```

`localnet-finalize` is resumable across the v2-style sequence: it invokes the
emitter's existing swap, calls candidate `distribute()` to synchronize, calls
candidate `drop()` for a positive backfill schedule, and then verifies:

- The emitter points to the candidate.
- The emitter queue is empty.
- The candidate is active.
- The candidate, factory, and deployed pool retain their immutable bindings.
- The deployed pool retains its transferable v2-style local admin and remains
  Admin Active.
- The pool remains registered.
- The incumbent and next designated tokens match the deployment plan.
- Incumbent and candidate balances are conserved.
- The successful emitter swap proved the required strict incumbent-token
  comparison at transition time.

Verification can be repeated independently:

```bash
make localnet-verify
```

After verification, exercise the committed v2-compatible 70/30 ongoing
emission path:

```bash
make localnet-emissions
```

That command waits for positive emitter intervals, invokes the candidate's
permissionless distribution, derives its immutable 70/30 tranches from the
public total, verifies positive tier-specific `claimable` increases, compounds
both pools' BLND:USDC and BLND:XLM accruals into their originating tiers, and
gulps the two pool tranches into their seven-day reserve-token streams. After
the reported expiration:

```bash
make localnet-claim-emissions
```

Stop the ephemeral container when the exercise is complete:

```bash
make localnet-stop
```

Stopping Quickstart destroys the local ledger. The public state and evidence
files under `.localnet/` remain for inspection but cannot resume a destroyed
network.

## Evidence

`.localnet/state.json` contains the source commit, dirty-source indicator,
source-patch digest, artifact digests, public deployment identifiers, and
lifecycle timestamps. A dirty deployment also retains the contract-source
patch in its run directory. The runner rejects non-ignored untracked
repository files, because `git diff` cannot include their contents in that
patch. Every upload,
deployment, invocation, and view stores its result and `--cost` diagnostics in
the run directory recorded by that state file.
When an operation label is reused, the runner adds a UTC timestamp and, if
needed, a numeric suffix rather than overwriting earlier evidence.
Secrets are held only in the isolated ignored Stellar CLI configuration and
are never copied into `state.json` or the transaction logs.

Native and WASM compatibility tests remain the fast time-travel lane. They
advance the full queue and retry horizon without changing the production
constants used by this local-network exercise.
