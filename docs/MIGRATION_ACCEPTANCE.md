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
  queue, and a balance deficit permits cancellation and prevents the swap.
- Queue unlock occurs exactly 31 days after creation and cannot be bypassed.
- The queue is created through the emitter's existing
  `queue_swap_backstop()` function.
- The first pre-swap candidate `distribute()` succeeds without a queue, opens
  its accounting epoch at the invocation timestamp, and returns zero.
- Every pre-swap candidate `distribute()` validates any live queue. Calls no
  earlier than its final seven days attest its compatible recipient, token,
  and unlock without introducing a separate preparation entry point.
- Queue cancellation and requeueing do not start, pause, or reset backfill.
- The queued recipient is the exact candidate and the next designated token is
  its configured BLND:XLM Comet LP.
- A swap executed from an unattested or incompatible queue cannot activate the
  candidate after the emitter deletes that queue.
- The emitter's existing `swap_backstop()` performs the only replacement path.
- The first candidate `distribute()` after that swap activates the candidate
  only when the compatible queue was attested and the seven-day post-unlock
  grace period has not expired.
- The candidate exposes v2-named `distribute() -> i128` and `drop()` migration
  entry points and no separate begin, prepare, finalize, synchronization, or
  backfill-funding mutator.
- The SDK reconstructs the exact simplified candidate lifecycle from one
  contract-instance ledger entry, without migration-state contract views or a
  `Prepared` status.
- A successful direct swap remains transitionable throughout the fixed grace
  period despite a later loss of the candidate's former strict BLND:USDC
  majority.
- While the emitter points to a locally inactive candidate,
  BLND-bearing tier-weight mutations and reward-zone edits fail closed.
- Queue creation, observation, emitter swap, and candidate transition are
  permissionless and expose no governance, multisig,
  administrator, emergency override, privileged WASM replacement, or alternate
  migration path.

The emitter swap MUST distribute the incumbent's time-accrued BLND before
changing its recipient. The candidate's ongoing distribution clock begins at
that transition and receives no ongoing BLND for the preceding interval.

## Incumbent isolation

The accelerated harness MUST establish v2 supplier, borrower, collateral,
debt, and backstop-share positions in the exact deployed SDK-22 contracts
before migration.

Immediately before the emitter swap it MUST snapshot those positions and
relevant token balances. Immediately afterward it MUST prove that:

- no v2 principal, collateral, debt, backstop share, or existing claim moved,
  converted, closed, or entered v3;
- the only incumbent balance change caused by the swap is the emitter's
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
- Prefunding begins at candidate construction. The first pre-swap
  `distribute()` starts the backfill clock rather than construction.
- Before migration, only active, nonqueued BLND:USDC LP tokens in reward-zone
  pools supply emission weight. Pool weight uses those LP-token amounts
  directly rather than converting them to underlying BLND. The 70% allocation
  becomes pending BLND:USDC until a pool gulp starts its seven-day stream.
- Queueing checkpoints the tier stream and already accrued BLND, then stops
  future pool and stream weight for the queued shares. Dequeueing resumes at
  the current index; withdrawal cannot erase prior streamed accrual.
- BLND:XLM becomes emission-eligible only after migration. Plain USDC remains
  ineligible before and after.
- The first post-swap candidate `distribute()` ends backfill at the latest
  completed pre-swap checkpoint and resets ongoing accounting to the emitter's
  candidate checkpoint. The intervening gap is omitted, so no interval receives
  both backfill and ongoing emissions.
- Eligible backfill accrues at one BLND per second from the first
  pre-replacement `distribute()`, uses the ordinary fixed 70/30 split and
  pending-pool accounting, remains capped at 10 million BLND, and can reach
  that cap after 10,000,000 continuously eligible seconds regardless of queue
  timing. Intervals with no positive eligible pool weight schedule no BLND.
- The pre-migration 70% tranche enters only the BLND:USDC seven-day stream; the
  30% tranche uses each pool's ordinary supply/borrow stream.
- The test deployment supplies an empty discretionary recipient list.
  Candidate `drop()` therefore names only the candidate for its scheduled
  backfill, and the remaining emitter allocation stays undistributed.
- Positive funding succeeds once only after an exact configured-BLND balance
  increase and rolls back on an unexpected delta or repeat.
- No backstop-tier claim can consume the obligation before activation and
  complete funding. Thereafter backfilled BLND uses the same tier-compounding
  and pool-allowance claim paths as ongoing BLND; no separate backfill claim
  exists.
- Aggregate allocations, scoped rounding carries, granted pool allowances,
  backstop claims, and the unfunded amount conserve the scheduled drop exactly.
- One Protocol-27 differential test MUST run the same prefunding,
  distribution/gulp, queue, partial dequeue, withdrawal, reward-zone removal,
  emitter replacement, funding, and claim timeline through the exact deployed
  v2 backstop and optimized v3 backstop. Scheduled BLND, the 30% pool tranche,
  and withdrawn LP MUST match exactly; depositor claims may differ only by the
  explicitly retained v3 rounding carries.

## Transition to ongoing BLND

The suite MUST verify both configured-BLND exact-delta paths:

1. successful backfill funding; and
2. the first positive ongoing emitter call when it occurs before funding.

Directly prefunded BLND cannot satisfy this proof. After verification, each
candidate call MUST allocate elapsed checkpoint time once, verify the current
emitter mint against its immediately surrounding checkpoints, account for a
prior direct emitter invocation exactly once, and ignore unrelated BLND
transfers. A failed amount, timestamp, or binding check MUST roll back the
emitter call and candidate checkpoint atomically.

Ongoing contract allocation is governed and tested by the v3 contract
specification. Optimized-WASM acceptance MUST exercise separate BLND:USDC and
BLND:XLM backstop claims, verify exact BLND consumption and LP receipt, and
confirm that each result increases the same user's active shares only in its
originating pool tier.

## Execution matrix

| Environment | Required evidence |
| --- | --- |
| Native Soroban `Env` | Accelerated lifecycle, queue attestation, cancellation, grace boundaries, and rollback |
| Optimized WASM | Contract ABI, serialization, cross-contract behavior, and resource limits |
| Protocol-27 compatibility harness | Exact deployed SDK-22 v2 and emitter WASM interacting with optimized v3 WASM |
| Local Quickstart | Unshortened production queue, resumability, TTL keepalive, transaction costs, and final invariants |
| Native-XLM public testnet | Production-style transaction assembly, wall-clock queue operation, real 80:20 Comets, and integrated oracle-free backstop valuation |

The compatibility harness MUST invoke at least one deployed v2 backstop entry
point through the Protocol-27 host and complete the deployed-emitter swap into
optimized v3 WASM. It MUST NOT rebuild v2 against SDK 27 or substitute a local
SDK-22 build for the recorded deployed artifacts.

Native tests MUST cover queue replacement, attestation and activation-grace
boundaries, the 10-million-BLND cap, and post-swap fail-closed accounting.

## Network evidence

Localnet and public-testnet runners MUST:

- use the production migration constants;
- deploy and prefund matched Fixed and YieldBlox v2/v3 pools before opening
  the emitter queue;
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

Modeled lending pools MAY use authenticated mutable SEP-40 fixtures for
deterministic reserve prices, but those feeds MUST be labeled non-production.
The backstop's integrated valuation MUST remain independent of them and read
only the configured assets and Comets.

Modeled v2 reserve setup MUST resume safely across the separate
`queue_set_reserve` and `set_reserve` transactions rather than submitting a
duplicate queue after interruption.

## Migration-ready gate

The candidate may be described as migration-ready only when:

1. every contract-suite prerequisite passes;
2. the direct emitter-swap and candidate-distribution path passes;
3. cancellation, balance-race, delayed-transition, and repeated-call cases pass;
4. incumbent positions remain unchanged and operational;
5. migration capital, backfill, and ongoing BLND remain conserved;
6. migration operations fit Protocol-27 resource limits;
7. an isolated local-network migration completes; and
8. an isolated public-testnet migration exercises production transaction
   assembly and operational monitoring.

Independent security review remains required before any mainnet proposal.
