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
4. The swap MUST reject before unlock or if the proposed backstop no longer
   holds the strictly greater balance.
5. The swap first distributes BLND accrued through that timestamp to the
   incumbent.
6. It then registers the proposed backstop and proposed designated token and
   starts the new backstop's distribution clock at the swap timestamp.

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

- its second-loss BLND:USDC backstop tier; and
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

### 5.2 Queue observation and preparation

The migration uses the v2 emitter and backstop function names:

| Operation | Preconditions | Effect |
| --- | --- | --- |
| Emitter `queue_swap_backstop()` | Strictly greater candidate direct balance; no live queue | Creates the permissionless 31-day queue |
| Candidate `distribute()` before replacement | Live compatible queue within the immutable limits | Opens or checkpoints the original backfill epoch and, during the final seven days, records a verified prepared queue |

The original queue MUST be observed through `distribute()` before it disappears.
The emitter does not retain canceled queue history, so an unobserved queue that
is canceled or swapped cannot later be reconstructed on-chain.

A replacement queue MUST target the configured candidate and BLND:XLM
designated token, receive fresh verification and preparation through
`distribute()`, and remain within every immutable limit. Cancellation or
re-queueing MUST NOT move either absolute horizon.

Before a replacement queue is prepared, it MUST NOT advance backfill beyond
the original or most recently verified unlock. A valid replacement catches up
to its current checkpoint only when final-window preparation succeeds.

Observing a queue before a BLND-bearing weight mutation MUST open the epoch and
require a current distribution checkpoint. This prevents weight added after
queue creation from receiving retroactive backfill.

Preparation MUST revalidate the candidate's strictly greater direct balance.
The emitter independently repeats that comparison when its swap is invoked, so
loss of the majority before the swap prevents replacement.

### 5.3 Emitter swap and candidate synchronization

| Operation | Preconditions | Effect |
| --- | --- | --- |
| Emitter `swap_backstop()` | Prepared candidate operationally; emitter queue unlocked; strictly greater direct balance | Distributes the incumbent's final accrued BLND and registers the candidate and BLND:XLM token |
| First candidate `distribute()` after replacement | Candidate remains Prepared; before verified unlock plus seven days | Verifies candidate registration, checkpoints backfill through the verified unlock, activates ongoing accounting, and returns zero |

The candidate exposes no separate begin, prepare, finalize, or synchronization
mutator. Permissionless `distribute()` drives its local lifecycle around the
emitter's existing queue and swap operations, matching the v2 transition
surface. Repeated calls MUST NOT duplicate value.

The synchronization deadline is seven days after the verified queue's unlock,
not seven days after the direct swap. A direct swap performed late leaves only
the remainder of that fixed grace period.

Once the emitter swap succeeds, later loss of the candidate's strictly greater
BLND:USDC balance MUST NOT undo the emitter transition or block
synchronization. Failure to synchronize before the deadline can leave the
emitter pointing to a candidate whose local lifecycle remains Prepared. This
candidate accepts that bounded operational risk rather than adding a
governance recovery path.

Because the emitter exposes no immutable direct-swap timestamp,
synchronization MUST cap backfill at the verified queue's unlock. This may omit
the interval between unlock and a later direct swap, but no interval may
receive both migration backfill and ongoing BLND.

While the emitter points to an unsynchronized Prepared candidate, every
BLND:USDC or BLND:XLM weight mutation and reward-zone edit MUST fail. Plain
USDC operations remain available because they affect neither migration
backfill nor ongoing BLND weight.

## 6. Continuous BLND:USDC migration accounting

An accounted BLND:USDC deposit is ordinary second-loss tier capital from the
moment it is accepted, before or after migration. Before migration succeeds,
the same custody also contributes to the candidate's direct emitter-qualifying
balance and its active, nonqueued shares participate in backfill emissions.

The original queue opens the ordinary emission indexes at that queue's start.
Shares already deposited at that time begin accruing immediately. Later
deposits start at the current index and receive no retroactive credit.

Queueing first checkpoints the owner's accrued BLND, then removes the queued
shares from future emission weight. Dequeueing resumes at the current index.
Neither queueing nor withdrawing erases BLND already accrued. There is no
separate principal-time position, survivor checkpoint, or early-withdrawal
forfeiture rule.

Migration MUST NOT redeem, convert, reissue, or change ownership of BLND:USDC
tier shares. Activation value, second-loss capacity, take-rate eligibility,
withdrawal restrictions, and post-migration emission weight continue to use the
ordinary tier position.

The pre-migration loss waterfall applies only to liabilities of candidate v3
pools connected to this backstop. Candidate capital does not absorb incumbent
v2 pool losses merely because it also qualifies the emitter replacement.
The immutable BLND:XLM, BLND:USDC, then plain-USDC waterfall applies before
and after migration; replacement does not reorder tiers or swap their assets.

BLND:XLM and plain USDC MAY be deposited before migration but receive no
migration backfill and do not enter the incumbent balance comparison.

## 7. Backfill

The original observed emitter queue starts permissionless backfill
distribution. Each elapsed eligible second contributes one BLND, subject to the
10-million-BLND cap. An interval with no positive active BLND:USDC weight in the
reward zone advances the checkpoint but schedules no BLND.

Under the two-retry lifecycle limits and verified-unlock cutoff, continuous
eligibility over the longest 100-day interval produces at most 8,640,000 BLND.
The 10-million-BLND ceiling is therefore a safety cap rather than an expected
award.

Every backfill checkpoint uses the same fixed 70/30 split and cumulative-index
accounting as ongoing emissions:

- 70% is allocated among reward-zone pools by active, nonqueued underlying
  BLND in BLND:USDC, then to active BLND:USDC tier shares;
- 30% is allocated to the same pools for their configured supply/borrow
  emission positions; and
- BLND:XLM and plain USDC receive no migration backfill.

Pool, tier-index, user-index, and split rounding carry forward at their ordinary
scopes. A sufficiently recent checkpoint MUST precede weight changes, bounding
retroactive credit to the configured checkpoint window. Backfill earned before
shares are queued or withdrawn remains claimable, while those shares earn
nothing after leaving active weight.

The first post-swap `distribute()` checkpoints only through the verified
unlock. After that cutoff, both tranches use the post-migration emission rules.

Successful migration awards zero discretionary BLND. If scheduled backfill is
positive, any caller MAY fund it once through candidate `drop()`, which invokes
the emitter's initial drop. The
candidate MUST name itself as the only recipient, request exactly the scheduled
amount, and verify an equal increase in its configured BLND balance. The unused
portion of the emitter's 50-million-BLND ceiling remains undistributed.

Backstop and pool claims MUST remain disabled until migration is active and the
entire positive schedule is funded. There is no separate backfill claim:
BLND:USDC owners use the ordinary tier claim, which compounds BLND into
BLND:USDC LP and credits ordinary tier shares, while pools use the ordinary 30%
allowance and claim path. Repeat funding or an unexpected balance delta MUST
fail atomically.

## 8. Transition to ongoing BLND

Backfill and post-migration BLND share one allocation ledger. Before
the initial drop, the scheduled backfill is the unfunded portion of that
aggregate obligation. After exact funding, it is ordinary accounted,
unclaimed BLND rather than a segregated reserve.

The candidate MUST bind its BLND token immutably at construction and expose
whether that binding has passed an exact-delta proof.

The configured BLND binding MUST be verified by one of two exact-delta paths:

- successful positive backfill funding verifies the initial-drop balance
  increase; or
- if it occurs first, the first positive ongoing distribution snapshots the
  configured-token balance before invoking the emitter and requires the
  balance to increase by exactly the reported amount.

The successful proof MUST be stored and exposed. Failure MUST roll back the
emitter invocation and all candidate state changes. Directly prefunding the
configured BLND token MUST NOT bypass the exact current-call delta.

The permissionless post-migration distributor MUST invoke the emitter and
allocate one BLND per second elapsed since the candidate's preceding
checkpoint. It MUST verify that the amount returned for the current invocation
equals one BLND per second between the emitter checkpoints immediately before
and after that invocation. This accounts exactly once for a direct emitter
invocation made before the candidate entry point without observing pool
allowance claims.

The emitter checkpoint MUST advance without exceeding ledger time. Standalone
distribution calls require at least five seconds since the preceding candidate
checkpoint. Unrelated BLND transfers create no emission entitlement. After
migration, reward-zone pool weight includes active, nonqueued underlying BLND
in both BLND:USDC and BLND:XLM; the 70% pool tranche is divided between those
two tiers by the same underlying-BLND measure. Plain USDC remains ineligible.

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
queue, swap, distribute, or fund the candidate.

No recovery path may override the emitter's queue, balance comparison, or
unlock time. Bounded permissionless recovery may only observe, validate, or
synchronize state already established by the emitter.

## 11. Acceptance

A migration-ready candidate MUST:

1. keep the frozen v2 source and dependency graph unchanged;
2. pass inherited v2 tests and the candidate's native and optimized-WASM
   suites;
3. execute exact deployed SDK-22 v2 and emitter WASM on the Protocol-27 host;
4. complete the direct emitter-swap and candidate-distribution path within the
   documented limits;
5. preserve incumbent positions unchanged and operational;
6. conserve candidate tier capital, backfill, and ongoing BLND through every
   lifecycle transition; and
7. pass the cross-version and network matrix in
   [MIGRATION_ACCEPTANCE.md](MIGRATION_ACCEPTANCE.md).
