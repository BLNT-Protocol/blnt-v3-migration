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
- `XLM`, representing the isolated pair asset for the next Comet LP and the
  fourth canonical asset available to an operator-defined tier

The two LPs execute the checked-in Comet WASM and expose current reserve
composition to the candidate's integrated valuation and emission calculations.
Their isolated initial balances are deterministic fixtures; they do not
represent production market pricing or liquidity. Separate authenticated
SEP-40 fixtures provide the modeled pools' deterministic reserve prices and
are not used for backstop valuation.

Both Comets are initialized unfrozen by one dedicated controller account. The
runner then sets that account's thresholds to 100 and its sole signer weight to
zero, verifies the controller and exact 80:20 token configuration, and performs
the depositor LP joins only after the lock. This mirrors the canonical mainnet
BLND:USDC Comet and makes later freeze or controller changes impossible.

The migration runner deploys matched Fixed and Custom models against the
incumbent and candidate before opening the emitter queue. Fixed v3 uses
BLND:XLM, BLND:USDC, and USDC with 4:3:2 weights, holds the candidate's full
migration-qualifying BLND:USDC balance, enters the reward zone, and routes its
pool tranche to EURC borrowing. Custom v3 has XLM first loss at weight 2 and
USDC second loss at weight 1. It crosses the activation threshold, enters an
open reward-zone slot, and supports XLM, USDC, and PYUSD lending. Because it has
no BLND-bearing tier, it receives zero BLND allocation. Both incumbent pools
receive BLND:USDC capital and enter the v2 reward zone.

The classic-asset issuer/operator, candidate depositor, and one-way Comet
controller use separate local identities. The runner creates the required
modeled-asset trustlines, initializes and locks both Comet pools, and mints the
depositor LP positions before funding the two backstops and lending positions.

Fixed v3 is created and receives the first plain-USDC backstop deposit before
Custom v3 is deployed. The runner then decodes the shared backstop USDC balance
and both Comets' underlying SAC balance entries directly from persistent
ledger state. It fails the deployment unless each entry is positive,
authorized, and records `clawback = false`; local issued XLM is checked at both
the Comet and shared backstop after its first deposit. Modeled-pool verification
repeats the complete proof before the emitter queue may open.

The local operator pays for deployment and migration transactions but receives
no continuing protocol authority. The exercise uses only the deployed emitter's
permissionless queue and swap mechanism; it deploys no governance, multisig,
administrator recovery, or alternate upgrade path.

This test deployment configures the candidate with an empty discretionary
recipient list. Its first candidate distribution starts the independent
backfill clock immediately before the queue. Backfill accrues through the
ordinary 70/30 pending-and-stream pipeline: the backstop tranche enters only
the BLND:USDC seven-day stream, while the pool tranche follows each pool's
supply/borrow configuration. After
activation, permissionless candidate `drop()` funds exactly that schedule
through the emitter's initial drop; the finalize command invokes and verifies
it when the schedule is positive. The otherwise available discretionary
allocation remains undistributed in this deployment.

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

During the final seven days before unlock:

```bash
make localnet-prepare
```

This records the compatible candidate, BLND:XLM token, and unlock without
changing the contract's v2-named migration surface.

At or after unlock, and no later than seven days afterward:

```bash
make localnet-finalize
```

`localnet-finalize` is resumable across the v2-style sequence: it checkpoints
backfill and refreshes the queue attestation, invokes the emitter's existing
swap, calls candidate `distribute()` within the grace period to transition to
ongoing accounting, calls candidate `drop()` for a positive backfill schedule,
and then verifies:

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
public total, and gulps Fixed v3. That gulp starts the BLND:USDC and BLND:XLM
backstop streams and the pool's reserve-token stream. Custom v3 remains a
zero-weight reward-zone member. After the reported seven-day expiration, the
next command compounds both Fixed backstop-tier accruals and claims its pool
stream:

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
advance the full queue and exercise delayed and canceled migration paths
without changing the emitter's production delay.
