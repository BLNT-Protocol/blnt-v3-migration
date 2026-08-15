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

### 5.1 Limits

The emitter's immutable queue delay is 31 days and candidate backfill is capped
at 10 million BLND. A compatible queue may be attested beginning seven days
before unlock, and candidate activation must occur no later than seven days
after unlock. The candidate adds no construction deadline, latest queue start,
retry count, or multi-queue recovery horizon. The cap bounds the financial
obligation even when prefunding, queueing, cancellation, or replacement takes
longer.

### 5.2 Queue observation and distribution

The migration uses the v2 emitter and backstop function names:

| Operation | Preconditions | Effect |
| --- | --- | --- |
| Emitter `queue_swap_backstop()` | Strictly greater candidate direct balance; no live queue | Creates the permissionless 31-day queue |
| First candidate `distribute()` before replacement | Emitter still points to the incumbent | Opens the queue-independent backfill epoch at the current timestamp and returns zero |
| Later candidate `distribute()` before replacement | Emitter still points to the incumbent | Validates any live queue, checkpoints backfill, and records a compatible queue during its attestation window |

Every live queue observed by candidate `distribute()` MUST target the
configured candidate and BLND:XLM designated token. The deployed emitter
remains the authority for the strict raw-balance comparison at queue creation
and swap execution. The candidate does not duplicate that comparison; its only
additional timing checks are the queue-attestation window and activation grace
defined above.

Queue creation, cancellation, and re-queueing MUST NOT start, pause, reset, or
extend the independent backfill clock. A changed or absent queue, or a
checkpoint after the grace period, invalidates any stale attestation. There is
no candidate retry counter or multi-queue recovery deadline.

As in v2, ordinary BLND-bearing weight mutations require no current global
distribution checkpoint. They checkpoint the affected pool-tier stream and
user where applicable, while the next `distribute()` samples current active
weight for its complete elapsed interval.

### 5.3 Emitter swap and candidate transition

| Operation | Preconditions | Effect |
| --- | --- | --- |
| Emitter `swap_backstop()` | Emitter queue unlocked; strictly greater direct balance | Distributes the incumbent's final accrued BLND and registers the candidate and queued designated token |
| First candidate `distribute()` after replacement | Emitter points to the candidate; compatible queue attested; no later than unlock plus seven days | Ends backfill at the latest pre-replacement checkpoint, resets to the emitter's candidate checkpoint, activates ongoing accounting, and returns zero |

The candidate exposes no separate begin, prepare, finalize, or synchronization
mutator. Permissionless `distribute()` drives its local lifecycle around the
emitter's existing queue and swap operations, matching the v2 transition
surface. Repeated calls MUST NOT duplicate value.

The SDK MUST reconstruct an exact migration snapshot from the candidate's
contract-instance storage, including the epoch start, attested queue unlock,
activation and backfill-end timestamps, scheduled and funded backfill, BLND
binding verification, and derived `Pending`, `Open`, or `Active` status. The
contract MUST NOT add migration-state view entry points.

Like v2, an uncheckpointed interval between the final pre-swap candidate call
and the emitter swap receives no backfill. The emitter's candidate checkpoint
starts ongoing accrual, so no interval receives both backfill and ongoing
BLND. A candidate that was never called before replacement activates with zero
scheduled backfill.

Once the emitter swap succeeds, later loss of the former BLND:USDC majority
MUST NOT undo the emitter transition or block the candidate's local transition
during the fixed seven-day grace period. The attestation and grace are required
because the deployed emitter deletes its queue and exposes no designated-token
getter after replacement.

While the emitter points to a candidate that has not completed this
transition, every BLND:USDC or BLND:XLM weight mutation and reward-zone edit
MUST fail. Plain USDC operations remain available because they affect neither
migration backfill nor ongoing BLND weight.

## 6. Continuous BLND:USDC migration accounting

An accounted BLND:USDC deposit is ordinary second-loss tier capital from the
moment it is accepted, before or after migration. Before migration succeeds,
the same custody also contributes to the candidate's direct emitter-qualifying
balance and its active, nonqueued shares participate in backfill emissions.

The first pre-replacement `distribute()` opens the ordinary allocation epoch at
its invocation timestamp, independently of any queue. Reward-zone pools are
weighted by their active, nonqueued BLND:USDC LP-token amounts directly,
matching v2; BLND:XLM and plain USDC have zero backfill weight. A checkpoint
assigns the 70% tranche to pending BLND:USDC, and a later pool gulp starts or
refreshes its seven-day depositor stream.

Queueing first checkpoints the tier stream and owner's accrued BLND, then
removes the queued shares from future pool and stream weight. Dequeueing
resumes at the current index. Neither queueing nor withdrawing erases BLND
already streamed to the owner. Pending or unstreamed BLND remains governed by
the ordinary v2-style stream rather than a principal-time entitlement.

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

The first pre-replacement `distribute()` starts permissionless backfill
distribution and returns zero. Each later elapsed eligible second contributes
one BLND, subject to the 10-million-BLND cap. An interval with no positive
active BLND:USDC weight in the reward zone advances the checkpoint but
schedules no BLND.

With continuous eligibility, the full cap is reached after 10,000,000 seconds
(115 days, 17 hours, 46 minutes, and 40 seconds). The original queue may be
timed so its unlock occurs at or after that point, allowing one uninterrupted
migration queue to schedule the full cap.

Every backfill checkpoint uses the same fixed 70/30 pending-and-stream
accounting as ongoing emissions:

- 70% is allocated among reward-zone pools by active, nonqueued BLND:USDC LP
  tokens and becomes pending only for each pool's BLND:USDC tier;
- 30% is allocated to the same pools for their configured supply/borrow
  emission positions; and
- BLND:XLM and plain USDC receive no migration backfill.

At most once per 24 hours, a permissionless pool gulp starts or refreshes the
BLND:USDC depositor stream and the pool's configured reserve streams. Pending
BLND and unfinished emissions combine into fresh seven-day schedules. Pool,
tier-stream, user-index, and split rounding carry forward at their ordinary
scopes. Ordinary weight changes require no global checkpoint, matching v2's
sampling behavior. Queueing preserves already streamed accrual and removes the
shares from the active weight sampled by the next distribution.

The first post-swap `distribute()` preserves the latest completed pre-swap
checkpoint as the backfill cutoff. After that cutoff, both tranches use the
post-migration emission rules.

The candidate constructor binds an immutable initial-drop recipient list. Its
aggregate allocation plus the maximum 10-million-BLND backfill MUST NOT exceed
the emitter's 50-million-BLND ceiling. Recipient selection, including an empty
list, is a deployment decision.

After activation, any caller MAY invoke candidate `drop()` once. It MUST submit
the configured list and, when positive, append the candidate's exact scheduled
backfill. The candidate MUST verify that its configured BLND balance increases
by the total allocation directed to itself.

Backstop-tier claims MUST remain disabled until migration is active and the
entire positive schedule is funded. There is no separate backfill claim:
BLND:USDC owners use the ordinary tier claim, which compounds BLND into
BLND:USDC LP and credits ordinary tier shares, while pools use the inherited
30% allowance and claim path. Repeat funding or an unexpected balance delta
MUST fail atomically.

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
unlock time. Permissionless callers may repeat ordinary distribution and
funding operations subject to their idempotence checks.

## 11. Acceptance

A migration-ready candidate MUST:

1. keep the frozen v2 source and dependency graph unchanged;
2. pass inherited v2 tests and the candidate's native and optimized-WASM
   suites;
3. execute exact deployed SDK-22 v2 and emitter WASM on the Protocol-27 host;
4. complete the direct emitter-swap and candidate-distribution path;
5. preserve incumbent positions unchanged and operational;
6. conserve candidate tier capital, backfill, and ongoing BLND through every
   lifecycle transition; and
7. pass the cross-version and network matrix in
   [MIGRATION_ACCEPTANCE.md](MIGRATION_ACCEPTANCE.md).
