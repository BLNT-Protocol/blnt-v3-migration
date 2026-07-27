# Local-Network Migration Exercise

The local-network runner deploys a frozen SDK-22 incumbent and an SDK-27
candidate against a Protocol-27 Stellar Quickstart network configured with
testnet Soroban resource limits. It records contract IDs, transaction results,
CLI `--cost` output, balances, and final invariants below the ignored
`.localnet/` directory.

The same transaction assembly is reused by the isolated public-testnet wrapper
documented in [TESTNET_MIGRATION.md](TESTNET_MIGRATION.md). Network-specific
state and keys remain separate.

The runner uses independently issued local assets and real Comet contracts:

- `BLND`
- A BLND:USDC Comet LP, representing the legacy backstop token
- A BLND:XLM Comet LP, representing the candidate's next designated token and
  second backstop tier
- `USDC`, representing the plain-USDC third backstop tier
- `XLM`, representing the isolated pair asset for the next Comet LP

The two LPs execute the checked-in Comet WASM and expose real current reserve
composition to the candidate's emission calculation. Their isolated initial
balances and the test valuation adapter remain deterministic fixtures; they do
not represent production market pricing or liquidity.

The runner also deploys a test-only immutable valuation adapter. It values
each configured LP token one-for-one in both USDC and underlying BLND so the
canonical valuation call path can be exercised deterministically, and exposes
the same one-USDC fixture price through the lending-oracle interface. This
fixture is not a production oracle and MUST NOT be used as a production pricing
source.

The migration runner deploys its registered pool with one isolated
`BLNDUSDC` reserve. It deposits enough plain USDC to cross the candidate's
$12,500 activation threshold, retains positive BLND:USDC weight, moves the
pool to Admin Active, adds it to the reward zone, configures the reserve's
supply side for emissions, and seeds a depositor supply position. This makes
the same wall-clock fixture capable of exercising ongoing BLND distribution
after migration.

The classic-asset issuer/operator and candidate depositor use separate local
identities. The runner creates the depositor's `BLND`, `USDC`, and `XLM`
trustlines explicitly, initializes both Comet pools, and mints the legacy LP
position before funding the candidate backstop and lending supply position.

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
through the independent `v3/` workspace. Candidate source documentation is
retained for native Rust builds but omitted from the deployed contract-spec
section to preserve Protocol-27 upload headroom. `make build-v3` rejects an
optimized backstop above the conservative 120,000-byte deployment guard.

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
checkpoint, valuation-adapter instance, both Comets, and all three configured
classic-asset contract instances for ten local days. It also refreshes the queued swap, candidate,
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
- The deployed pool retains its transferable v2-style local admin and remains
  Admin Active.
- The pool remains registered.
- The legacy and next designated tokens match the deployment plan.
- Incumbent and candidate balances are conserved.
- The candidate still holds strictly more legacy backstop tokens.

Verification can be repeated independently:

```bash
make localnet-verify
```

After verification, exercise the committed v2-compatible 70/30 ongoing
emission path:

```bash
make localnet-emissions
```

That command waits for a positive emitter interval, invokes the candidate's
permissionless distribution, verifies positive 70% and 30% allocations,
claims the backstop-depositor tranche, and gulps the pool tranche into its
seven-day reserve-token stream. After the reported expiration:

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
patch in its run directory. The runner rejects untracked files below `v3/`,
because `git diff` cannot include their contents in that patch. Every upload,
deployment, invocation, and view stores its result and `--cost` diagnostics in
the run directory recorded by that state file.
When an operation label is reused, the runner adds a UTC timestamp and, if
needed, a numeric suffix rather than overwriting earlier evidence.
Secrets are held only in the isolated ignored Stellar CLI configuration and
are never copied into `state.json` or the transaction logs.

Native and WASM compatibility tests remain the fast time-travel lane. They
advance the full queue and retry horizon without changing the production
constants used by this local-network exercise.
