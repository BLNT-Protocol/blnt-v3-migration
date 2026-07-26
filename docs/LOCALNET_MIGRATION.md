# Local-Network Migration Exercise

The local-network runner deploys a frozen SDK-22 incumbent and an SDK-27
candidate against a Protocol-27 Stellar Quickstart network configured with
testnet Soroban resource limits. It records contract IDs, transaction results,
CLI `--cost` output, balances, and final invariants below the ignored
`.localnet/` directory.

The runner uses independently issued local assets:

- `BLND`
- `BLNDUSDC`, representing the legacy BLND:USDC backstop token
- `BLNDXLM`, representing the candidate's next designated token and second
  backstop tier
- `USDC`, representing the plain-USDC third backstop tier

These assets test token custody and migration behavior. They do not attempt to
reproduce Comet pricing or liquidity behavior.

The runner also deploys a test-only immutable valuation adapter. It values
each configured LP token one-for-one in both USDC and underlying BLND so the
canonical valuation call path can be exercised deterministically, and exposes
the same one-USDC fixture price through the lending-oracle interface. This
fixture is not a production oracle and MUST NOT be used as a production pricing
source.

The migration runner deploys its registered pool with an empty immutable
reserve list because this lane tests migration operations rather than lending.
Optimized-WASM compatibility tests separately deploy a configured two-reserve
pool and exercise supply, collateral supply, borrow, health, and status
enforcement.

The classic-asset issuer/operator and candidate depositor use separate local
identities. The runner creates the depositor's `BLNDUSDC` trustline explicitly
before minting and depositing the candidate's qualifying balance.

The local operator pays for deployment and migration transactions but receives
no continuing protocol authority. The exercise uses only the legacy emitter's
permissionless queue and swap mechanism; it deploys no governance, multisig,
administrator recovery, or alternate upgrade path.

Successful migration awards zero discretionary BLND. The candidate exposes a
permissionless post-activation funding path that invokes the emitter's
initial-drop facility only for the scheduled, time-weighted BLND:USDC backfill,
capped at 10 million BLND. The current runner does not yet invoke that path, so
the otherwise available allocation of up to 40 million BLND remains
undistributed.

## Prerequisites

- Stellar CLI 27
- Docker with a running daemon
- Rust 1.81 and 1.91 toolchains configured by the repository
- `jq`, `make`, and `shasum`

The runner uses the exact published emitter WASM embedded by
`blend-contract-sdk` 1.22.0 and verifies its SHA-256 digest before deployment.
It builds the incumbent through the frozen root workspace and the candidate
through the independent `v3/` workspace.

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

This explicitly extends the legacy emitter instance, incumbent distribution
checkpoint, valuation-adapter instance, and all four configured asset-contract
instances for ten local days. It also refreshes the queued swap, candidate,
factory registration, pool, depositor position, and relevant token balances
through their supported access paths. Before reporting success, it verifies
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

`localnet-finalize` invokes the candidate's atomic finalization path and then
verifies:

- The emitter points to the candidate.
- The emitter queue is empty.
- The candidate is active.
- The candidate, factory, and deployed pool retain their immutable bindings.
- The deployed pool retains its transferable v2-style local admin and remains in Setup
  until an authorized status transition occurs.
- The pool remains registered.
- The legacy and next designated tokens match the deployment plan.
- Incumbent and candidate balances are conserved.
- The candidate still holds strictly more legacy backstop tokens.

Verification can be repeated independently:

```bash
make localnet-verify
```

Stop the ephemeral container when the exercise is complete:

```bash
make localnet-stop
```

Stopping Quickstart destroys the local ledger. The public state and evidence
files under `.localnet/` remain for inspection but cannot resume a destroyed
network.

## Evidence

`.localnet/state.json` contains public deployment identifiers and lifecycle
timestamps. Every upload, deployment, invocation, and view stores its result
and `--cost` diagnostics in the run directory recorded by that state file.
When an operation label is reused, the runner adds a UTC timestamp and, if
needed, a numeric suffix rather than overwriting earlier evidence.
Secrets are held only in the isolated ignored Stellar CLI configuration and
are never copied into `state.json` or the transaction logs.

Native and WASM compatibility tests remain the fast time-travel lane. They
advance the full queue and retry horizon without changing the production
constants used by this local-network exercise.
