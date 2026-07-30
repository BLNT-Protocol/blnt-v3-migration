# Blend V2-to-V3 Migration Acceptance

This document defines the cross-version and network evidence required to
accept the migration described by
[MIGRATION_PROTOCOL_SPEC.md](MIGRATION_PROTOCOL_SPEC.md). Production contract
behavior is tested in the pinned `blend-contracts-v3` repository and is not
restated here.

Normative terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are acceptance
criteria.

## Contract-suite prerequisite

Migration acceptance requires:

1. the frozen SDK-22 v2 tests to pass without modifying the pinned source or
   dependency graph;
2. the complete native and optimized-WASM v3 contract suite to pass;
3. the v3 resource and WASM-size guards to pass; and
4. the exact deployed v2 and emitter artifacts to pass
   `make verify-deployed-artifacts`.

The v3 contract specification and tests govern activation, lending, pool
status, withdrawals, auctions, loss settlement, take-rate allocation, and
ongoing emissions. A migration test MUST NOT redefine those rules through a
fixture or duplicate implementation.

## Deployed-emitter replacement

The cross-version suite MUST verify:

- The candidate directly holds strictly more of the incumbent designated
  token than the incumbent backstop; equality is insufficient.
- The incumbent designated token remains the comparison asset for the complete
  queue, and a balance deficit permits cancellation and prevents
  finalization.
- Queue unlock occurs exactly 31 days after creation and cannot be bypassed.
- `begin_migration()` atomically creates the canonical emitter queue and opens
  the accounting epoch.
- A compatible queue created directly on the emitter is not adopted until
  `open_migration_epoch()` records it.
- `prepare_migration()` is available only during the final seven days,
  verifies the live queue, and permanently anchors the recovery horizon.
- At most two replacement queues may be verified. Cancellation and requeueing
  do not extend the 76-day recovery horizon or 138-day construction-anchored
  deadline.
- The queued recipient is the exact candidate and the next designated token is
  its configured BLND:XLM Comet LP.
- Canonical `finalize_migration()` atomically performs the emitter swap and
  activates the candidate.
- If another caller swaps the emitter directly, `sync_migration()` can recover
  during the fixed seven-day grace period and cannot synchronize a canceled or
  incompatible queue.
- A successful direct swap remains synchronizable after the candidate loses
  its former strict BLND:USDC majority.
- The synchronization deadline is measured from the verified unlock, not from
  a later direct swap.
- While the emitter points to an unsynchronized Prepared candidate,
  BLND-bearing tier-weight mutations and reward-zone edits fail closed.
- Queue creation, observation, preparation, finalization, and synchronization
  are permissionless and expose no governance, multisig, administrator,
  emergency override, privileged WASM replacement, or alternate migration
  path.

Finalization MUST distribute the incumbent's time-accrued BLND before changing
the emitter recipient. The candidate's ongoing distribution clock begins at
that transition and receives no ongoing BLND for the preceding interval.

## Incumbent isolation

The accelerated harness MUST establish v2 supplier, borrower, collateral,
debt, and backstop-share positions in the exact deployed SDK-22 contracts
before migration.

Immediately before finalization it MUST snapshot those positions and relevant
token balances. Immediately afterward it MUST prove that:

- no v2 principal, collateral, debt, backstop share, or existing claim moved,
  converted, closed, or entered v3;
- the only incumbent balance change caused by finalization is the emitter's
  final time-accrued BLND distribution;
- the next positive emitter distribution goes only to the candidate; and
- ordinary v2 repay, supply, and backstop-deposit entry points remain usable.

## Migration capital and backfill

The suite MUST prove that accounted BLND:USDC is one continuous candidate
backstop position before and after migration. The same custody and shares
provide second-loss capital and the direct balance used by the emitter; no
temporary migration-only shares or involuntary conversion may exist.

It MUST additionally verify:

- Direct BLND:USDC transfers affect only the emitter's raw balance comparison
  and mint no shares or other entitlement.
- BLND:XLM and plain USDC never affect the incumbent balance comparison or
  migration backfill.
- Prefunding begins at candidate construction, and the original queue must
  start within 31 days.
- Only eligible BLND:USDC deposited before the applicable verified unlock
  accrues principal-time weight.
- Queueing preserves weight, while actual full or partial withdrawal forfeits
  the withdrawn position's proportional principal and time basis.
- Loss absorption does not erase the history of capital that remained
  committed to the candidate.
- Canonical finalization ends backfill at its atomic transition timestamp.
  Recovery synchronization caps it at the verified unlock, so no interval
  receives both backfill and ongoing emissions.
- Scheduled backfill accrues at one BLND per second from the original queue
  start, remains capped at 10 million BLND, and reaches at most 9,244,800 BLND
  under the bounded 107-day lifecycle.
- Successful migration awards zero discretionary BLND. The initial drop names
  only the candidate and requests exactly the scheduled backfill; the
  remaining emitter allocation stays undistributed.
- Funding succeeds once only after an exact configured-BLND balance increase
  and rolls back on missing eligible weight, an unexpected delta, or a repeat.
- Claims require position-owner authorization, permit an owner-selected
  recipient, use frozen constant-time checkpoints, and cannot be repeated.
- Funded, claimed, and reserved totals conserve the drop exactly. Floor dust
  remains segregated and cannot become ongoing emissions or a discretionary
  award.

## Transition to ongoing BLND

The suite MUST verify both configured-BLND exact-delta paths:

1. successful backfill funding; and
2. when no backfill is funded, the first positive ongoing emitter call.

Directly prefunded BLND cannot satisfy this proof. After verification, the
candidate MUST reconcile its actual BLND custody, exclude the remaining
backfill reserve and already-accounted claims, and account for a direct emitter
invocation exactly once. A failed balance, timestamp, or binding check MUST
roll back the emitter call and candidate checkpoint atomically.

Ongoing contract allocation is governed and tested by the v3 contract
specification.

## Execution matrix

| Environment | Required evidence |
| --- | --- |
| Native Soroban `Env` | Accelerated lifecycle, retry horizons, boundary values, and rollback |
| Optimized WASM | Contract ABI, serialization, cross-contract behavior, and resource limits |
| Protocol-27 compatibility harness | Exact deployed SDK-22 v2 and emitter WASM interacting with optimized v3 WASM |
| Local Quickstart | Unshortened production queue, resumability, TTL keepalive, transaction costs, and final invariants |
| Public testnet | Production-style transaction assembly and wall-clock queue operation using isolated test assets |
| Production backstop-valuation testnet lane | Real 80:20 Comets and optimized backstop valuation against an explicitly non-production SEP-40 fixture |

The compatibility harness MUST invoke at least one deployed v2 backstop entry
point through the Protocol-27 host and complete the deployed-emitter swap into
optimized v3 WASM. It MUST NOT rebuild v2 against SDK 27 or substitute a local
SDK-22 build for the recorded deployed artifacts.

Native tests MUST cover the full retry horizon and the candidate's migration
TTL policy without shortening production constants.

## Network evidence

Localnet and public-testnet runners MUST:

- use the production migration constants;
- verify source commits and every deployed artifact digest;
- use independently issued test assets instead of depending on official test
  BLND or USDC supply;
- record contract IDs, transaction results, simulated envelopes, resource
  costs, balances, queue timestamps, and final invariants;
- keep signing material outside versioned state and evidence;
- retain prior evidence rather than overwrite repeated operations; and
- prove the final emitter, candidate, factory, pool, asset, and queue bindings.

The local Quickstart runner MUST remain resumable across its wall-clock queue.
Its transaction-backed keepalive MUST preserve economic state and queue
timestamps, renew every required instance and ledger entry before expiry, and
record success only after validating the live queue and strict candidate
balance majority.

The production backstop-valuation testnet lane MAY use an authenticated
mutable SEP-40 fixture for deterministic histories, but it MUST label that feed
non-production and MUST NOT count the exercise as provider qualification.
Fixture administrators must be immutable, isolated runner identities, and
verification must check those bindings.

Modeled v2 reserve setup MUST resume safely across the separate
`queue_set_reserve` and `set_reserve` transactions rather than submitting a
duplicate queue after interruption.

## Migration-ready gate

The candidate may be described as migration-ready only when:

1. every contract-suite prerequisite passes;
2. canonical finalization and direct-swap synchronization paths pass;
3. cancellation, expiry, balance-race, retry, and repeated-call cases pass;
4. incumbent positions remain unchanged and operational;
5. migration capital, backfill, and ongoing BLND remain conserved;
6. maximum migration bounds fit Protocol-27 resource limits;
7. an isolated local-network migration completes; and
8. an isolated public-testnet migration exercises production transaction
   assembly and operational monitoring.

Independent security review remains required before any mainnet proposal.
