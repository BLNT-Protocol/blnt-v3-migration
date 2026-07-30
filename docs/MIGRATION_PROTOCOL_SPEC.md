# Blend V2-to-V3 Migration Protocol Specification

This document defines the on-chain emitter-replacement behavior implemented by
the Blend v3 candidate and the compatibility obligations imposed by the
deployed Blend v2 contracts and deployed emitter.

It composes with [SYSTEM_SPEC.md](SYSTEM_SPEC.md). Contract behavior unrelated
to migration is specified by the pinned `blend-contracts-v3` submodule.

Normative terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are acceptance
criteria.

## 1. Executable authority

The exact deployed emitter and v2 WASM artifacts under
`deployed-artifacts/mainnet` are the runtime compatibility boundary. The pinned
`blend-contracts-v2` source records the SDK-22 implementation and tests.
If this document conflicts with that executable behavior, the executable
behavior wins and this document MUST be corrected.

The candidate implementation is the pinned `blend-contracts-v3` submodule.
Migration runners and fixtures MUST invoke that implementation directly and
MUST NOT maintain a second production candidate.

## 2. Deployed-emitter behavior

### MIGRATION-EMITTER-001: Ongoing distribution

The deployed emitter:

- stores one BLND token, current backstop, and designated backstop token;
- mints ongoing BLND to the current backstop at one BLND per elapsed second;
- stores a separate last-distribution timestamp for each registered backstop;
  and
- permits permissionless distribution.

### MIGRATION-EMITTER-002: Backstop replacement

The emitter's replacement mechanism is permissionless and economically gated:

1. A proposed backstop MUST directly hold strictly more of the incumbent
   designated token than the incumbent backstop. Equality is insufficient.
2. Queue creation records the proposed backstop, proposed next designated
   token, and an unlock exactly 31 days later.
3. The incumbent designated token remains the comparison asset throughout the
   queue.
4. Finalization MUST reject before unlock or if the proposed backstop no longer
   holds the strictly greater balance.
5. Finalization first distributes BLND accrued through that timestamp to the
   incumbent.
6. It then registers the proposed backstop and proposed designated token and
   starts the new backstop's distribution clock at the finalization timestamp.

The emitter compares raw SEP-41 balances. It does not calculate market value,
LP composition, pool activation, risk, pool attribution, or user ownership.

### MIGRATION-EMITTER-003: Initial drop

The emitter permits one initial BLND drop per backstop address while that
address is registered as the current backstop. Re-registering the same address
does not restore eligibility. The invoked backstop supplies the recipient
list, whose aggregate allocation MUST NOT exceed 50 million BLND.

## 3. Incumbent isolation

Emitter replacement changes future BLND routing only. It MUST NOT move,
convert, close, import, or rewrite incumbent v2 supplier, borrower, collateral,
debt, or backstop positions.

BLND already held, allocated, or accrued under v2 remains governed by the
deployed v2 accounting and claim rules. Incumbent positions remain usable
through their ordinary v2 entry points after replacement.

## 4. Candidate migration boundary

The candidate uses the incumbent BLND:USDC Comet LP as both:

- its first-loss BLND:USDC backstop tier; and
- the direct-balance capital used to qualify for emitter replacement.

These are one continuous accounted position, not separate temporary and
post-replacement balances. Qualifying capital MUST remain in the candidate's
direct BLND:USDC balance. Market value, LP composition, pool attribution, and
candidate share accounting MUST NOT substitute for the emitter's raw balance
comparison.

Internal components MAY be separate contracts, but qualifying BLND:USDC
capital MUST remain in the top-level candidate's direct balance.

Raw BLND:USDC transferred directly to the candidate may affect the emitter
comparison, but it MUST NOT create tier shares, activation value, loss
capacity, take-rate eligibility, ongoing-emission weight, or migration
backfill.

## 5. Lifecycle

### 5.1 Immutable limits

| Limit | Value |
| --- | ---: |
| Original queue start after candidate construction | At most 31 days |
| Emitter queue delay | 31 days |
| Preparation window | Final 7 days before verified unlock |
| Direct-swap synchronization deadline | Verified unlock plus 7 days |
| Verified replacement queues | At most 2 |
| Recovery horizon | Original unlock plus 76 days |
| Absolute migration deadline | Construction plus 138 days |

Construction starts public prefunding and fixes the absolute deadline.

### 5.2 Queue operations

| Operation | Preconditions | Effect |
| --- | --- | --- |
| `begin_migration()` | No accounting epoch; within the original-start limit | Atomically creates the canonical emitter queue and records its original epoch |
| `open_migration_epoch()` | Another caller's correct queue remains observable | Records that queue as the original epoch without finalizing it |
| `prepare_migration()` | Open epoch; currently verifiable queue; strictly greater direct balance; preparation window | Idempotently records the verified queue; the original unlock permanently anchors the recovery horizon |

A replacement queue MUST target the configured candidate and BLND:XLM
designated token, receive fresh verification and preparation, and remain
within every immutable limit. Cancellation or re-queueing MUST NOT move either
absolute horizon.

`prepare_migration()` MUST NOT silently adopt a queue when no epoch has been
recorded. The emitter does not retain canceled queue history, so an unobserved
queue that is canceled cannot later be reconstructed on-chain.

The candidate MUST revalidate its qualifying balance at finalization. Loss of
the strictly greater balance before the emitter swap prevents finalization.

### 5.3 Finalization and synchronization

| Operation | Preconditions | Effect |
| --- | --- | --- |
| `finalize_migration()` | Prepared, currently verified and unlocked queue; strictly greater balance; within both horizons | Atomically invokes the emitter swap, excludes the final incumbent distribution from candidate accrual, verifies candidate registration, freezes backfill, and enables ongoing emissions |
| `sync_migration()` | Another caller already swapped the emitter; candidate remains Prepared; currently verified queue; before unlock plus seven days | Verifies candidate registration, freezes backfill at that queue's unlock, and enables ongoing emissions without moving the recovery anchor |

Both paths MUST preserve user positions and enforceable claims, swap no user
assets, award zero discretionary BLND, and either be idempotent or reject
repetition without duplicating value.

The synchronization deadline is seven days after the verified queue's unlock,
not seven days after the direct swap. A direct swap performed late leaves only
the remainder of that fixed grace period.

Once the emitter swap succeeds, later loss of the candidate's strictly greater
BLND:USDC balance MUST NOT undo the emitter transition or block
synchronization. Failure to synchronize before the deadline can leave the
emitter pointing to a candidate whose local lifecycle remains Prepared. This
candidate accepts that bounded operational risk rather than adding a
governance recovery path.

Because the emitter exposes no immutable direct-swap timestamp, recovery-path
synchronization MUST cap backfill at the verified queue's unlock. Canonical
finalization ends backfill at its atomic swap-and-activation timestamp. This
may omit the interval between unlock and a later direct swap, but no interval
may receive both migration backfill and ongoing BLND.

While the emitter points to an unsynchronized Prepared candidate, every
BLND:USDC or BLND:XLM weight mutation and reward-zone edit MUST fail. Plain
USDC operations remain available because they affect neither migration
backfill nor ongoing BLND weight.

## 6. Continuous BLND:USDC migration accounting

An accounted BLND:USDC deposit is ordinary first-loss tier capital from the
moment it is accepted, before or after migration.

Before migration succeeds, the same shares additionally:

- contribute their tokens to the candidate's direct emitter-qualifying
  balance; and
- accumulate migration-backfill weight while deposited if their deposit
  precedes the currently verified queue's unlock.

Backfill-eligible principal MUST record actual contributed LP-token units, not
the number of shares minted at a non-unit exchange rate. A withdrawal removes
principal and time basis pro rata to shares burned. Loss absorption does not
forfeit principal because the capital remained committed and was consumed
protecting its attributed candidate pool.

Deposits at or after a verified unlock remain ordinary tier capital but have
zero backfill weight for that queue. If a replacement queue is later verified,
new deposits before the replacement unlock are eligible from their deposit
time; an earlier ineligible deposit does not become eligible retroactively.

Queueing a withdrawal alone does not forfeit backfill. Actual withdrawal before
successful migration forfeits the withdrawn shares' proportional principal and
time basis.

Migration freezes only temporary backfill weight. It MUST NOT redeem, convert,
reissue, or change ownership of BLND:USDC tier shares. Activation value,
first-loss capacity, take-rate eligibility, withdrawal restrictions, and
underlying-BLND ongoing-emission weight continue to use the ordinary tier
position.

The pre-migration loss waterfall applies only to liabilities of candidate v3
pools connected to this backstop. Candidate capital does not absorb incumbent
v2 pool losses merely because it also qualifies the emitter replacement.

BLND:XLM and plain USDC MAY be deposited before migration but receive no
migration backfill and do not enter the incumbent balance comparison.

## 7. Backfill

The original observed emitter queue starts the backfill clock. If candidate
construction precedes that queue, otherwise eligible prefunding receives
weight from the prefunding start through finalization.

Scheduled backfill accrues at one BLND per second until the applicable
finalization cutoff and is capped at 10 million BLND:

\[
B = \min(t_f-t_0,\ 10{,}000{,}000)
\]

where \(t_0\) is the original queue start and \(t_f\) is the canonical or
recovery cutoff. A position deposited before \(t_0\) still uses its actual
deposit timestamp in the position-weight formula below; prefunding therefore
increases that position's relative time-weight without starting aggregate
scheduled BLND before the original queue.

Under the two-retry lifecycle limits, the longest scheduled interval is 107
days and produces at most 9,244,800 BLND. The 10-million-BLND ceiling is
therefore a safety cap rather than an expected award.

Successful migration awards zero discretionary BLND. The only permitted
initial-drop issuance is \(B\), sent to the candidate for the claims defined
here. The unused portion of the emitter's 50-million-BLND ceiling MUST remain
undistributed.

For position \(i\), eligible principal tranche \(A_{i,k}\), tranche deposit
timestamp \(T_{i,k}\), and cutoff \(t_f\):

\[
W_i = \sum_k A_{i,k}(t_f-T_{i,k})
\]

\[
B_i =
\left\lfloor
B\frac{W_i}{\sum_j W_j}
\right\rfloor
\]

Only principal still represented by deposited shares when migration succeeds
is included. Withdrawing early forfeits weight to the surviving positions
proportionally; an earlier small deposit receives no special allocation beyond
its actual principal-time weight.

The candidate MUST freeze the global aggregate weight and each surviving
position's weight without iterating over all users at finalization.

After migration, any caller MAY invoke one-time backfill funding. The candidate
MUST request exactly one initial-drop recipient—the candidate itself—for
exactly \(B\), and MUST verify an exact \(B\) increase in its configured BLND
balance before committing the funded reserve and BLND binding.

A missing eligible weight, failed emitter call, unexpected balance delta, or
repeat funding attempt MUST fail atomically.

Claims MUST use the frozen global and position weights in constant time. The
position owner MUST authorize the claim and MAY select its recipient. Each
position MAY claim at most once. State updates and the BLND transfer MUST be
atomic.

Floor-rounding dust remains in the segregated backfill reserve. It MUST NOT be
withdrawn, reassigned, counted as ongoing emissions, or distributed
discretionarily. Funded, claimed, and remaining totals MUST be externally
auditable without iteration.

## 8. Transition to ongoing BLND

Ongoing accounting MUST use BLND actually received after the emitter changes
recipients and MUST exclude the remaining backfill reserve.

The candidate MUST bind its BLND token immutably at construction and expose
whether that binding has passed an exact-delta proof.

Before accumulated-balance reconciliation is allowed, the configured BLND
binding MUST be verified by one of two exact-delta paths:

- successful backfill funding verifies the initial-drop balance increase; or
- if backfill was not funded, the first positive ongoing distribution
  snapshots the configured-token balance before invoking the emitter and
  requires the balance to increase by exactly the reported amount.

The successful proof MUST be stored and exposed. Failure MUST roll back the
emitter invocation and all candidate state changes. Directly prefunding the
configured BLND token MUST NOT bypass the exact current-call delta.

The permissionless distributor MUST invoke the emitter and reconcile the
candidate's actual BLND balance against the remaining backfill reserve and
previously accounted, unclaimed ongoing BLND. This supports an emitter
distribution invoked directly before the candidate entry point.

The emitter-reported amount for the current invocation MUST NOT exceed the
reconciled receipt, and its checkpoint MUST advance without exceeding ledger
time. Standalone distribution calls require at least five seconds since the
preceding emitter checkpoint.

Any other unaccounted BLND transferred directly to the candidate is treated as
donated ongoing emissions.

## 9. Next designated token

The successful replacement MUST register the candidate's exact BLND:XLM
80:20 Comet LP token as the emitter's next designated token. A different token
requires a different protocol candidate.

## 10. Authorization and recovery

Migration operations are permissionless and expose no governance, multisig,
protocol-wide administrator, emergency override, privileged WASM replacement,
or alternate upgrade path.

Deployment keys and transaction submitters are operational actors only and
MUST NOT receive continuing protocol authority merely because they deploy,
fund, prepare, finalize, or synchronize the candidate.

No recovery path may override the emitter's queue, balance comparison, or
unlock time. Bounded permissionless recovery may only observe, validate, or
synchronize state already established by the emitter.

## 11. Acceptance

A migration-ready candidate MUST:

1. keep the frozen v2 source and dependency graph unchanged;
2. pass inherited v2 tests and the candidate's native and optimized-WASM
   suites;
3. execute exact deployed SDK-22 v2 and emitter WASM on the Protocol-27 host;
4. complete canonical and direct-swap recovery paths within the documented
   limits;
5. preserve incumbent positions unchanged and operational;
6. conserve candidate tier capital, backfill, and ongoing BLND through every
   lifecycle transition; and
7. pass the cross-version and network matrix in
   [MIGRATION_TEST_PLAN.md](MIGRATION_TEST_PLAN.md).
