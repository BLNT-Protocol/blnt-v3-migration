# Blend V3 Candidate System Specification

Status: Draft 0.1  
Implementation status: Baseline and migration harness only

## 1. Purpose

This document defines the current technical boundaries and acceptance criteria
for an experimental Blend Protocol v3 candidate on Stellar.

The initial candidate focuses on:

- Compatibility with the legacy emitter migration mechanism.
- A backstop capable of supporting more than one approved asset or LP token.
- Explicit separation of migration capital, activation capital, emissions, and
  loss absorption.
- A safe path for legacy BLND:USDC backstop depositors.

This document does not claim that unresolved economic parameters have been
approved. It explicitly rejects a separate governance or administrative
upgrade system for this candidate.

Normative terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY` describe candidate
requirements.

## 2. System boundaries

### 2.0 Runtime baselines

The candidate is developed in two independent toolchain lanes:

- The incumbent v2 test baseline MUST remain on Rust 1.81 and Soroban SDK
  22.0.7.
- New v3 contracts MUST target Stellar Protocol 27 with Rust 1.91.0, Soroban
  SDK 27.0.2, and the `wasm32v1-none` target.
- The project MUST NOT introduce an SDK 26 compatibility lane.

Cross-version acceptance is based on migration behavior, contract interfaces,
authorization, state transitions, and accounting invariants. It does not
require v3 source code to compile against intermediate SDK versions.

### 2.1 In scope

- V3 backstop accounting and custody.
- Pool-specific activation calculations.
- Loss tiers and loss allocation.
- Ongoing BLND-emission distribution.
- Pre-migration backfill accounting.
- V2-to-v3 emitter migration.
- Continuous BLND:USDC tier deposits and withdrawals across migration.
- Deployment and testing of candidate pools and supporting contracts.

### 2.2 Out of scope for the first milestone

- Mainnet deployment.
- Any governance, multisig, or alternate upgradeability system.
- Cross-chain collateral.
- Permissioned lending hooks.
- Fixed-term or fixed-rate lending.
- A production frontend.
- A final protocol-fee policy.

These features require separate specifications before implementation.

## 3. Terms

- **Incumbent backstop:** The backstop currently registered in the legacy
  emitter.
- **Candidate backstop:** The proposed replacement backstop.
- **Designated token:** The SEP-41 token whose direct balances the emitter
  compares when deciding whether a candidate may replace the incumbent.
- **Migration capital:** The incumbent designated token held to qualify the
  candidate for migration.
- **Activation capital:** Pool-allocated backstop value counted by the
  candidate's activation policy.
- **Legacy position:** A pre-finalization position denominated in the v2
  BLND:USDC LP token.
- **Tier:** A collection of backstop assets sharing a position in the loss
  waterfall.
- **Queued capital:** Capital associated with a pending withdrawal.

## 4. Fixed legacy-emitter constraints

The deployed emitter is treated as an external, immutable compatibility
boundary.

The candidate MUST account for the following behavior:

1. The emitter recognizes one incumbent backstop address.
2. The emitter recognizes one designated SEP-41 token at a time.
3. Migration qualification compares the designated token's direct balance at
   the candidate address with its direct balance at the incumbent address.
4. The candidate balance must be strictly greater; equality is insufficient.
5. The comparison continues to use the old designated token during the
   migration queue.
6. The candidate must remain larger when migration is finalized.
7. Finalization is unavailable until the 31-day queue expires.
8. Finalization registers both the new backstop and the token that will be used
   to compare the next migration.
9. Finalization distributes all BLND accrued through the finalization timestamp
   to the incumbent backstop before changing the registered backstop.
10. Finalization initializes the candidate's ongoing-distribution clock at the
    finalization timestamp, so the candidate does not receive ongoing emissions
    for the pre-finalization interval.
11. The emitter sends ongoing BLND emissions to one registered backstop address.
12. The emitter permits one initial drop per registered backstop, with an
    aggregate maximum of 50 million BLND.

The emitter performs a raw token-balance comparison. It does not calculate
market value, risk-adjusted value, LP composition, backstop tiers, or pool
activation.

The candidate MUST NOT place qualifying migration capital in a child vault if
doing so removes it from the candidate backstop address's direct token balance.

The legacy emitter's permissionless, economically gated queue and swap are the
candidate's only protocol-wide replacement mechanism. The candidate MUST NOT
add a governance vote, multisig approval, administrator key, or alternate
upgrade entry point that can bypass or replace that mechanism.

Successful migration MUST award zero discretionary BLND. The emitter's
otherwise available initial-drop capacity MUST NOT be distributed to the
deployer, transaction submitters, a treasury, migration participants
generally, or any other discretionary recipient.

## 5. Logical architecture

The intended logical components are:

```text
Legacy emitter
      │ BLND emissions and migration registration
      ▼
V3 backstop coordinator
      ├── migration accounting
      ├── pool allocations
      ├── activation policy
      ├── emissions policy
      ├── withdrawal queues
      └── loss waterfall
               │
               ▼
          V3 lending pools
```

Pricing adapters, asset vaults, and legacy-claim contracts MAY be separate
contracts if required for isolation or resource limits. Contract boundaries
remain an implementation decision, but the top-level candidate must satisfy
the emitter's direct-balance rule.

The incumbent v2 contracts MUST remain available in the test workspace so the
same test can deploy an incumbent and a candidate.

## 6. Candidate lifecycle

### 6.1 Pre-migration

Before finalization, the candidate MUST:

- Start a public prefunding campaign at candidate construction.
- Require the original emitter queue to begin no more than 31 days after the
  prefunding campaign starts.
- Provide a permissionless `begin_migration` operation that atomically creates
  the canonical legacy-emitter queue and records its accounting epoch.
- Open a public accounting epoch from the original queue's deterministic
  31-day start without treating that early observation as finalization
  verification.
- Refuse to prepare migration unless the accounting epoch has already opened.
- Prepare migration during the final seven days before a verified emitter queue
  unlocks, using the registered original queue's unlock time as the immutable
  recovery-horizon anchor before backfill is finalized or claimable.
- Store an immutable absolute migration deadline at construction, 138 days
  after the prefunding campaign begins.
- Accept the incumbent BLND:USDC designated token directly into the continuous
  BLND:USDC first-loss tier. The same accounted shares MUST serve migration,
  activation, loss, take-rate, and emission roles without conversion or
  duplicate balances.
- Keep enough designated tokens directly at the candidate address to remain
  strictly larger than the incumbent.
- Record user, pool, amount, shares, deposit time, and withdrawal state needed
  for backfill and legacy claims.
- Distinguish migration eligibility from pool activation.
- Permit approved post-migration backstop assets to be prefunded without
  counting them toward the legacy emitter comparison.

### 6.2 Queue period

During the 31-day queue:

- The incumbent remains the emitter recipient.
- Repeated preparation of the same queue MUST be idempotent.
- At most two replacement queues MAY be verified. Each replacement MUST target
  the configured candidate and designated token, MUST be prepared during its
  final seven days, and MUST fit within the absolute recovery horizon.
- The absolute recovery horizon MUST end 76 days after the original unlock and
  MUST NOT be extended by cancellation or re-queueing.
- No queue, retry, recovery, synchronization, or backfill endpoint MAY extend
  beyond the deployment-anchored 138-day absolute migration deadline.
- Direct-swap synchronization MUST expire seven days after the currently
  verified queue unlocks. Because another replacement requires a fresh 31-day
  delay, an unverified queue cannot unlock during that synchronization window.
- The candidate MUST continue validating its qualifying balance.
- BLND:USDC tier positions remain subject to the same pool attribution,
  withdrawal, loss-commitment, activation, and reward rules that apply after
  migration. Migration lifecycle state MUST NOT create a lower-risk temporary
  version of the tier.
- A loss of the strictly-greater balance MUST prevent finalization.
- Accounting changes MUST NOT allow the same capital to support incompatible
  withdrawal and migration claims.
- Tests MUST cover balance changes immediately before finalization.

### 6.3 Finalization

Finalization MUST:

- Require a previously prepared and currently verified queue.
- Reject after the original queue's bounded recovery horizon, which itself
  cannot exceed the deployment-anchored absolute deadline.
- Invoke the legacy emitter swap in the canonical finalization path.
- Verify that the emitter now recognizes the candidate.
- End time-weighted backfill at the earlier of actual activation or 76 days
  after the original verified queue unlock.
- Account for the emitter's final pre-swap distribution to the incumbent
  without crediting it to candidate depositors.
- Stop new accrual under the pre-migration backfill regime.
- Award zero discretionary BLND for successful migration.
- Activate the post-migration emission policy.
- Preserve each depositor's enforceable legacy claim.
- Avoid swapping user assets without explicit authorization.
- Be idempotent or reject repeated execution without duplicating value.

Emitter finalization does not rewrite the candidate's WASM. Any lifecycle
transition required by v3 MUST already be encoded in candidate code and MUST
verify the emitter's state before becoming active.

The legacy emitter's `swap_backstop` entry point is permissionless. If it is
invoked directly, the candidate MUST permit permissionless lifecycle
synchronization during the currently verified queue's bounded grace period
after verifying that the emitter recognizes the candidate. Synchronization
MUST NOT modify the original recovery-horizon anchor. After that queue's grace
period, synchronization MUST reject rather than risk accepting an unverified
replacement queue.

The canonical operational path MUST call `candidate.finalize_migration()`, not
the emitter directly. Candidate finalization invokes `swap_backstop`, verifies
the emitter transition, and activates the candidate atomically.
`candidate.sync_migration()` is only the recovery path when another caller
invokes the emitter's permissionless `swap_backstop` first. Operators MUST
monitor the emitter during the finalization window and invoke synchronization
immediately after detecting that transition.

The synchronization deadline is seven days after the currently verified
queue's unlock time, not seven days after the direct swap transaction. A direct
swap performed late in that interval leaves only the remaining portion of the
same fixed grace period. Once the emitter swap succeeds, a subsequent loss of
the candidate's strictly-greater legacy-token balance does not undo the emitter
transition or prevent synchronization. Failure to synchronize before the fixed
deadline can leave the emitter pointing to a candidate whose lifecycle remains
`Prepared`; this draft accepts that bounded operational risk rather than adding
a governance recovery path.

The canonical queue-creation path MUST call `candidate.begin_migration()`.
That operation atomically invokes the legacy emitter's permissionless
`queue_swap_backstop` and records the returned queue as the candidate's
original accounting epoch. `prepare_migration()` MUST NOT silently adopt a
queue when no epoch has been recorded.

If another caller creates a correct emitter queue directly,
`candidate.open_migration_epoch()` is the permissionless recovery path.
Operators MUST invoke it immediately after detecting the queue and before the
queue can be canceled or replaced. The legacy emitter does not retain canceled
queue history, so a queue that is both unobserved and canceled cannot later be
reconstructed on-chain. This draft accepts that bounded operational assumption.
The deployment-anchored absolute deadline ensures that an observed replacement
can never extend migration beyond 138 days from candidate construction.

### 6.4 Post-migration

After finalization:

- Ongoing BLND emissions accrue according to the v3 emission policy.
- The new designated token controls qualification for the next protocol
  migration.
- Existing BLND:USDC positions continue in the same first-loss tier without a
  conversion, asset swap, or second opt-in.
- A depositor MAY withdraw under the tier's ordinary withdrawal rules.
  Unwithdrawn positions remain fully accounted for and MUST NOT be treated as
  unencumbered protocol capital.

## 7. Multi-asset backstop

### 7.1 Asset configuration

Each accepted asset MUST have an explicit configuration containing, directly
or by reference:

- Token contract address.
- Valuation method and precision.
- Oracle or market-data source, if applicable.
- Staleness and failure policy.
- Activation haircut.
- Concentration limit.
- Loss tier.
- Deposit and withdrawal status.
- Liquidation or disposal path.

The three required activation assets are:

- BLND:USDC 80:20 LP.
- BLND:XLM 80:20 LP.
- Plain USDC.

The candidate supports no optional custom backstop tiers. Adding another
backstop asset requires a new protocol candidate and the legacy-emitter
replacement process.

Approval for activation does not by itself make a position eligible. Capital
MUST also be allocated to the evaluated pool, committed to the defined loss
waterfall, and subject to the applicable withdrawal rules.

### 7.2 Position accounting

The backstop MUST store one immutable v3 pool-factory address, MUST verify the
factory's `backstop` and `is_pool` interfaces during construction, and MUST
reject a factory whose immutable backstop is not the candidate itself. Before
accepting an accounted BLND:USDC deposit, it MUST verify that the attributed
pool is registered by that factory and that the pool exposes the fail-closed
withdrawal callback. Registration or interface failure MUST abort before
custody or accounting changes. The callback's current boolean result does not
gate a deposit: a pool with an existing loss MAY accept new loss-absorbing
capital, while later withdrawal remains blocked until the pool reports it safe.

`backstop_withdrawal_allowed(backstop)` is a permanent compatibility boundary.
Every factory-approved pool MUST preserve its signature and fail-closed
semantics across any WASM upgrade. A pool upgrade MUST be rejected unless the
replacement implementation retains this interface.

Positions MUST be attributable to:

- Pool.
- Depositor.
- Asset.
- Tier.
- Active shares.
- Queued-withdrawal shares.
- Any capital already committed to a loss or auction.

Share conversion MUST specify rounding direction for every operation. A user
MUST NOT receive more assets than the position owns, and dust MUST have a
documented destination.

Active and queued shares in one tier MUST use the same share-to-asset exchange
rate. Queueing MUST NOT snapshot or guarantee the position's current asset
value. A gain or loss applied to the tier changes the redemption value of
active and queued shares proportionally; the implementation MUST NOT preserve
queued principal by charging the change only to active shares.

### 7.2.1 Continuous BLND:USDC migration tier

The incumbent BLND:USDC LP token and the candidate's BLND:USDC first-loss tier
MUST use the same token contract and one continuous share ledger. A deposit
through the accounted BLND:USDC entry point is a tier deposit from the moment
it is accepted, whether it occurs before or after emitter migration.

Before migration succeeds, the same shares additionally:

- Contribute their tokens to the candidate's direct legacy-emitter qualifying
  balance.
- Accumulate migration-backfill weight while they remain deposited.

Emitter migration freezes only the temporary backfill weight. It MUST NOT
redeem, convert, reissue, or change ownership of the tier shares. Deposits made
after migration join the same tier but receive no retroactive backfill.

Pool activation value, first-loss capacity, take-rate eligibility, withdrawal
restrictions, and underlying-BLND emission weight MUST use this one position
and the same eligibility rules on both sides of migration. Ongoing BLND
emissions remain unavailable before the emitter begins sending BLND to the
candidate. Backfill remains a separate temporary allocation and MUST NOT cause
the shares to be counted twice in ongoing emissions.

The loss waterfall applies before migration only to liabilities of candidate
v3 pools connected to this backstop. These shares do not absorb losses from
incumbent v2 pools merely because their token balance also qualifies the
candidate for emitter migration.

Raw BLND:USDC tokens transferred directly to the candidate may affect the
legacy emitter's balance comparison, but they MUST NOT create tier shares,
activation value, loss capacity, take-rate eligibility, or emission weight.

### 7.2.2 Initial single-asset prefunding lane

Before activation policy or loss handling is implemented, the candidate MAY
configure one post-migration prefunding token independently from both the
legacy migration token and the emitter's next designated token. The configured
prefunding token MUST differ from the legacy migration token so the same direct
balance cannot be counted as both legacy migration capital and post-migration
prefunding. It MAY equal the next designated token only through explicit
constructor configuration.

This initial prefunding lane MUST:

- Issue exactly one prefund share for each token base unit transferred through
  the deposit entry point, with no conversion rounding or dust.
- Attribute shares to depositor, pool, and the immutable configured token.
- Maintain checked user, pool, and global share totals.
- Remain separate from legacy emitter qualification, migration positions,
  backfill weights, activation, loss capacity, and ongoing emissions.
- Permit deposits before and after migration activation.
- Permit immediate withdrawal while the shares remain ineligible for
  activation and are not committed to losses.
- Preserve exact token conservation across deposits and withdrawals.

Direct token transfers that do not use the deposit entry point MUST NOT create
shares or count toward any protocol calculation. This prefunding mechanism does
not approve the configured token for activation or loss absorption; the full
asset configuration in Section 7.1 remains required before either use.

### 7.3 Withdrawals

The continuous BLND:USDC tier inherits the v2 queue-for-withdrawal shape:

- Queuing creates a position with an expiry exactly 17 days after the queue
  transaction.
- A user may have at most 20 queue entries for a pool.
- Expired withdrawals consume entries oldest-first.
- Dequeue operations restore active shares newest-first and MAY occur before or
  after expiry.
- Queued shares remain in contract custody and continue contributing to the
  legacy emitter's raw balance comparison until the tokens are withdrawn.
- Queuing alone does not forfeit migration-backfill weight. If the tokens are
  actually withdrawn before emitter migration succeeds, the withdrawn shares
  forfeit their proportional accumulated backfill weight.
- Queued shares are excluded from activation value and ongoing BLND-emission
  weight immediately upon queueing.
- Queued shares remain eligible for take-rate allocation while they remain in
  contract custody. Take-rate eligibility uses total accounted tier shares,
  including active and queued shares, exactly once.
- Queued shares remain exposed to the tier's loss waterfall until actual
  withdrawal. Queueing does not reserve their underlying assets or protect
  their redemption value; a tier loss reduces active and queued share value
  proportionally through their common exchange rate, as in v2.
- Queueing and dequeueing do not change take-rate eligibility. Actual
  withdrawal MUST checkpoint accrued take-rate value before removing the
  withdrawn shares from future take-rate allocations.
- User, pool, and global total shares are reduced only when an expired
  withdrawal transfers the tokens out of the contract.
- Every withdrawal calls the attributed pool's read-only
  `backstop_withdrawal_allowed(backstop)` interface. The pool MUST return true
  only when the backstop has no capital committed to a loss and no unresolved
  bad debt. A missing interface, failed call, or false result blocks withdrawal.

The canonical pool callback MUST derive its result from three protocol-owned
record sets:

- Nonzero liabilities held by the configured backstop.
- Capital committed to an active loss or auction.
- Unresolved bad-debt records.

The initial Protocol-27 pool stores these as three bounded canonical record
maps in instance state and derives each count directly from its map length. Each
map may contain at most 30 records. Liability and unresolved-bad-debt maps are
keyed by reserve: a zero amount removes its record, negative amounts are
invalid, and replacing an existing nonzero amount does not change the count.
Committed losses are presence-only records keyed by auction identifier because
a multi-asset auction has no single amount unit. Monetary amounts from reserves
with different units MUST NOT be combined. Future lending, auction, and
waterfall operations MUST update the records atomically with the transition
they represent.

The callback MUST return false for an address other than the pool's immutable
backstop or when any count is nonzero. Missing canonical loss records MUST fail
closed rather than being interpreted as empty. No administrator, governance
address, or mutable boolean override may make the callback return true.

The withdrawal implementation MUST:

- Preserve a queue or delay sufficient to prevent capital from escaping known
  losses.
- Prevent withdrawal of capital committed to an outstanding loss process.
- Remain executable without iterating over an unbounded number of users or
  assets.

The bounded Q4W accounting and Protocol-27 pool implement this fail-closed
record and state boundary, but the lending, auction, and loss-waterfall entry
points that create and resolve records during real protocol operations remain
unimplemented. Until those engines invoke the internal record transitions,
these positions MUST NOT be treated as authoritative executable loss capacity.

## 8. Pool activation

The activation policy is independent of the emitter's designated-token
comparison.

A pool's activation score is the sum of the eligible values of its three
required assets, denominated in 7-decimal USDC units:

\[
E_p =
V_{p,\mathrm{BLND:XLM}} +
V_{p,\mathrm{BLND:USDC}} +
V_{p,\mathrm{USDC}}
\]

where:

- \(E_p\) is pool \(p\)'s eligible activation value.
- \(V_{p,i}\) is the verified USDC value of eligible asset \(i\) allocated to
  pool \(p\).

Every eligible USDC of value has equal activation weight. The calculation has
no asset-specific activation haircut, concentration cap, or minimum
BLND-bearing allocation. A pool therefore MAY satisfy the threshold with any
combination of the three required assets, including one eligible asset alone.

The entry threshold for an inactive pool is:

\[
T_{\mathrm{entry}} = 12{,}500\ \mathrm{USDC}
\]

The maintenance threshold for an active pool is:

\[
T_{\mathrm{maintenance}} = 10{,}000\ \mathrm{USDC}
\]

An inactive pool activates exactly when \(E_p \geq T_{\mathrm{entry}}\). An
active pool remains active exactly when
\(E_p \geq T_{\mathrm{maintenance}}\). Equality qualifies at both boundaries.
If an active pool falls below the maintenance threshold, it becomes inactive
and MUST meet the entry threshold again before reactivating. This hysteresis
prevents repeated activation changes near a single boundary.

Asset risk is handled separately through loss priority, withdrawal rules,
deposit limits, and emissions rather than through the activation score.

Plain USDC is the unit of account and contributes one USDC of activation value
per eligible token. A production valuation adapter MUST define
manipulation-resistant BLND:USDC and BLND:XLM LP valuation, oracle staleness,
and failure behavior before its values can drive pool status. Instantaneous
Comet reserve ratios and caller-supplied values MUST NOT directly determine
pool activation.

The implemented activation policy MUST:

- Count only capital allocated to the evaluated pool.
- Count only assets available to the defined loss waterfall.
- Avoid double-counting common underlying assets across wrappers or claims.
- Specify treatment of queued withdrawals and committed loss capital.
- Fail safely when required prices are stale or unavailable.
- Resist same-transaction price, reserve, and LP-supply manipulation.
- Use one canonical result for pool status and reward eligibility.
- Define behavior exactly at the threshold.

Activation status MUST be refreshable permissionlessly.

### 8.1 V2-compatible pool statuses

V3 preserves the v2 numeric status codes and action restrictions:

| Code | Status | Selected by |
| ---: | --- | --- |
| 0 | Admin Active | Pool-local operator |
| 1 | Active | Permissionless status refresh |
| 2 | Admin On-Ice | Pool-local operator |
| 3 | On-Ice | Permissionless status refresh |
| 4 | Admin Frozen | Pool-local operator |
| 5 | Frozen | Permissionless status refresh |
| 6 | Setup | Pool initialization |

On-Ice status MUST disable new borrowing and auction cancellation. Frozen
status MUST additionally disable new supply and collateral supply, exactly
matching the v2 action matrix. Repayment, withdrawals, liquidation and auction
fills, and other actions not prohibited by that matrix MUST remain available
subject to their ordinary authorization, health, and accounting checks.

The pool's Q4W percentage MUST use verified USDC value because v3 tiers have
different assets and share units. For pool \(p\):

\[
Q_p =
\left\lceil
\frac{V_{p,\mathrm{queued}}}
{V_{p,\mathrm{active}} + V_{p,\mathrm{queued}}}
10^7
\right\rceil
\]

where the active and queued values use the same canonical, manipulation-
resistant valuation inputs. Take-rate weights MUST NOT affect this ratio. If
the denominator is zero, \(Q_p = 0\); the activation threshold independently
keeps the pool On-Ice. The upward rounding and every status comparison occur at
seven-decimal precision.

A permissionless status refresh MUST apply these inclusive boundaries:

- For status 0, failure of the active-pool maintenance threshold or
  \(Q_p \ge 50\%\) moves the pool to status 3; otherwise it remains status 0.
- For status 2, \(Q_p \ge 75\%\) moves the pool to status 5; otherwise it
  remains status 2.
- For statuses 1, 3, and 5, \(Q_p \ge 60\%\) produces status 5;
  \(Q_p \ge 30\%\) or failure of the applicable activation threshold produces
  status 3; otherwise the result is status 1.
- Statuses 4 and 6 reject permissionless refresh.

Statuses 0 and 1 use the 10,000-USDC maintenance threshold. Statuses 2, 3, 4,
5, and 6 are inactive and MUST meet the 12,500-USDC entry threshold before
returning to an active status. This preserves the v3 activation hysteresis
while adopting the v2 Q4W status thresholds.

Each pool MAY configure one immutable pool-local operator at construction.
That operator MAY request only:

- Status 0 when the applicable activation threshold is met and
  \(Q_p < 50\%\).
- Status 2 when \(Q_p < 75\%\).
- Status 3 when \(Q_p < 75\%\), allowing the operator to relinquish an
  override into ordinary permissionless On-Ice mode.
- Status 4 without an additional Q4W condition.

Status 0 is a bounded risk-on override. At \(30\% \le Q_p < 50\%\), ordinary
permissionless policy produces On-Ice, but an operator may select Admin Active
and re-enable borrowing when the applicable activation threshold is met.
Statuses 2 and 4 are bounded risk-off controls.

This status-selection authority is the sole continuing privileged
operating-authority exception. Its Admin On-Ice and Admin Frozen modes are the
sole exception to the prohibition on emergency control. It MUST NOT change
status codes, thresholds, valuation rules, backstop configuration, asset
configuration, the loss waterfall, withdrawal eligibility, contract WASM,
emitter migration, or any protocol-wide rule. It is therefore pool-local
operational authority rather than protocol governance, an administrative
recovery path, or a protocol-wide emergency override.

The operator is immutable and has no rotation or recovery path. A compromised
operator can always select Admin Frozen. Because status 4 rejects permissionless
refresh, an unavailable operator while status 4 is selected leaves the pool
Admin Frozen indefinitely. This availability risk is accepted to preserve the
no-governance and no-administrator-recovery model. Admin Frozen MUST continue to
permit the actions allowed by the v2 Frozen action matrix, including repayment,
withdrawals subject to ordinary safety checks, and auction fills.

Queueing shares does not itself change stored pool status. A caller MUST invoke
the permissionless status-refresh entry point, using canonical values
checkpointed for that transaction. Status refresh, operator status requests,
pool-status enforcement, and BLND reward eligibility MUST consume the same
canonical status result.

## 9. Loss waterfall

The candidate uses the following strict first-loss order:

1. BLND:USDC LP.
2. BLND:XLM LP.
3. Plain USDC.
4. Pool suppliers for any residual loss after all eligible backstop capital is
   exhausted.

Each backstop asset occupies its own tier. A loss MUST exhaust the eligible,
uncommitted capital in a tier before consuming capital from the next tier.
Activation weighting is independent of this ordering: one USDC of verified
eligible value continues to contribute one USDC to the activation score
regardless of tier.

Capital queued for withdrawal is excluded from activation but remains
loss-bearing capital while it is still in custody. Within a tier, a loss MUST
impair active and queued shares proportionally through their common
share-to-asset exchange rate. The implementation MUST NOT consume active
shares first, preserve the queued shares' pre-loss redemption value, or permit
withdrawal while the pool reports a committed or unresolved loss.

Losses and tier capacity MUST be compared in the same verified USDC unit used
by the activation policy. Rounding MUST NOT understate the loss passed to a
later tier. Capital already committed to an outstanding loss or auction MUST
NOT be counted again.

For every tier, the implementation MUST define:

- Priority relative to other tiers.
- How losses are valued.
- How losses are divided among assets in the same tier.
- Rounding and residual-loss behavior.
- The liquidation path for each asset.
- Behavior when an oracle or market is unavailable.
- When supplier balances absorb losses after backstop exhaustion.

The auction, transfer, or disposal mechanism used to realize each tier's value
remains an open implementation decision. A failed or unavailable disposal path
MUST NOT silently skip a tier or charge a later tier without behavior
explicitly defined and tested.

Emission weights do not need to follow loss priority. Any difference MUST be
explicit and tested.

No asset may count toward loss capacity unless the protocol can actually seize
or dispose of it under the specified loss process.

### 9.1 Take-rate allocation

The three backstop tiers have fixed take-rate weights in waterfall order:

| Tier | Asset | Weight |
| ---: | --- | ---: |
| 1 | BLND:USDC LP | 2.5 |
| 2 | BLND:XLM LP | 2.5 |
| 3 | Plain USDC | 2 |

The contract represents this exact relative ratio as the integers
`5:5:4`; no floating-point arithmetic is used.

For a pool-level take-rate distribution \(D\), tier \(i\)'s allocation is
proportional to both its reward-eligible verified USDC value \(R_i\) and its
fixed weight \(w_i\):

\[
D_i =
\left\lfloor
D
\frac{w_i R_i}{\sum_j w_j R_j}
\right\rfloor
\]

A tier with zero reward-eligible value contributes zero to the numerator and
denominator. The formula therefore renormalizes across only populated,
reward-eligible tiers. Within each tier, allocation MUST be proportional to
reward-eligible depositor shares.

For take-rate purposes, reward-eligible value and shares include every
accounted position still held in contract custody, whether active or queued
for withdrawal. Queue expiry alone does not change eligibility. The protocol
MUST checkpoint a position's accrued allocation before an actual withdrawal
removes its shares from the tier. Queueing or dequeueing MUST NOT add or remove
take-rate value, and active plus queued views MUST NOT cause the same share to
be counted twice.

The fixed weights compensate earlier-loss capital more per USDC of eligible
value without changing activation weighting. Take-rate weighting is separate
from BLND-emission allocation and does not determine the loss amount charged to
a tier.

The implementation MUST use accumulator/checkpoint accounting rather than
iterate across depositors. If \(D\) is denominated in a supplied reserve asset,
each tier accrues that asset directly; the calculation MUST NOT assume an
immediate conversion to a backstop asset. Integer remainder MUST remain in a
carry accumulator keyed by both pool and distributed reserve asset for a future
distribution of that same asset. Remainders from different reserve assets MUST
NOT be combined, converted, or silently assigned to a favored tier.

## 10. BLND emissions and backfill

The emitter supplies BLND to the top-level candidate; the candidate determines
the internal allocation.

The candidate MUST distinguish:

- Pre-migration backfill.
- Ongoing backstop-depositor emissions.
- Pool supply and borrow emissions.
- Any other explicitly approved allocation.

The migration backfill clock MUST start when the original legacy-emitter queue
begins:

\[
T_{\mathrm{start}} = T_{\mathrm{original\ unlock}} - 31\text{ days}
\]

Scheduled backfill MUST accrue at 1 BLND per second until the bounded backfill
end and retain the inherited 10 million BLND safety cap:

\[
B = \min(T_{\mathrm{end}} - T_{\mathrm{start}}, 10{,}000{,}000)
\text{ BLND}
\]

With two 38-day retry periods, the longest scheduled interval is 107 days and
therefore produces at most 9,244,800 BLND. It intentionally cannot reach the
theoretical 10 million BLND cap.

The one-time migration issuance policy is:

\[
A_{\mathrm{migration}} = 0
\]

\[
D_{\mathrm{one\mbox{-}time}} = B \le 10{,}000{,}000\text{ BLND}
\]

where \(A_{\mathrm{migration}}\) is the discretionary award for completing
migration and \(D_{\mathrm{one\mbox{-}time}}\) is the only permitted one-time
distribution: funding for scheduled backfill. If the emitter's initial-drop
facility funds backfill, its aggregate distribution MUST equal \(B\), MUST
NOT exceed 10 million BLND, and its only recipient MUST be the candidate
backstop coordinator that accounts the surviving BLND:USDC claims.
The remainder of the emitter's 50 million BLND ceiling, including the
otherwise available allocation of up to 40 million BLND, MUST remain
undistributed.

Backfill eligibility MUST be proportional to each surviving migration
position's BLND:USDC tier shares multiplied by their holding duration. Only
shares still deposited when emitter migration succeeds are eligible. A
withdrawal before emitter migration succeeds MUST forfeit the withdrawn
shares' complete accumulated weight, and that weight MUST NOT remain in the
denominator. A partial withdrawal MUST forfeit only the withdrawn shares'
proportional weight. Re-deposited shares begin a new holding period.

The final allocation weight for position \(i\) is:

\[
W_i = \sum_k A_{i,k}(T_{\mathrm{end}} - T_{i,k})
\]

where \(A_{i,k}\) and \(T_{i,k}\) are the amount and effective deposit time of
each surviving tranche, and \(T_{\mathrm{end}}\) is the earlier of activation
or the absolute recovery-horizon end. The effective deposit time MUST be the
actual deposit time. Prefunding used to establish the candidate's qualifying
balance therefore earns proportional allocation weight before the original
queue begins, while the 31-day campaign limit prevents unbounded early-weight
accumulation. Prefunding weight changes only how scheduled BLND is divided; it
does not move the 1-BLND-per-second scheduling start earlier than the original
queue. A position's allocation is:

\[
B_i = \left\lfloor B \frac{W_i}{\sum_j W_j} \right\rfloor
\]

This makes the funded allocation, including weight forfeited by early
withdrawals, available proportionally to positions that remain through
successful migration, apart from deterministic integer-rounding dust.
Finalization and claims MUST use accumulator/checkpoint accounting and MUST NOT
iterate over all depositors. The funding transaction MUST reject an amount
other than the immutable scheduled backfill obligation and MUST NOT create a
separate migration award. Claim authority and the rounding-dust destination
remain separate implementation decisions; neither may redirect value to a
discretionary migration recipient.

Ongoing accounting MUST be based on BLND actually received after the emitter
changes recipients. No interval or position may earn under both regimes unless
the specification explicitly intends and tests that result.

### 10.1 Ongoing backstop-depositor emissions

This policy applies only to the BLND tranche assigned to backstop depositors,
\(E_{\mathrm{backstop}}\). Pool supply and borrow emissions remain a separate
allocation. The inherited v2 70/30 split is not enforced by the emitter, and
the top-level v3 split remains an open economic decision.

Only active pools are eligible for ongoing backstop-depositor emissions. A
pool's weight is the eligible underlying BLND held through its BLND:USDC and
BLND:XLM backstop tiers:

\[
B_p =
B_{p,\mathrm{BLND:USDC}} +
B_{p,\mathrm{BLND:XLM}}
\]

Plain USDC contributes zero BLND-emission weight. The pool allocation is:

\[
E_p =
\left\lfloor
E_{\mathrm{backstop}}
\frac{B_p}{\sum_{q \in \mathrm{active}} B_q}
\right\rfloor
\]

Within active pool \(p\), user \(u\)'s weight is the user's eligible underlying
BLND across the same two tiers:

\[
B_{p,u} =
B_{p,u,\mathrm{BLND:USDC}} +
B_{p,u,\mathrm{BLND:XLM}}
\]

and the user's allocation is:

\[
E_{p,u} =
\left\lfloor
E_p
\frac{B_{p,u}}{B_p}
\right\rfloor
\]

BLND held through either eligible tier earns at the same rate per underlying
BLND. The USDC or XLM component of an LP position does not contribute emission
weight.

The production implementation MUST derive underlying BLND with
manipulation-resistant, checkpointed LP accounting. It MUST NOT use untrusted
caller values or a same-transaction reserve ratio. Pool and user allocation
MUST use bounded accumulator/checkpoint accounting rather than iteration over
all pools or depositors. Integer remainder MUST be retained at its allocation
scope and included in a later distribution at that same scope.

Queued withdrawals receive zero ongoing BLND-emission weight immediately after
their emissions are checkpointed at queue time. Dequeueing restores emission
eligibility only prospectively and MUST NOT grant emissions for the queued
interval. The accounting MUST separately define treatment of capital committed
to losses and activation-status transitions. A position MUST NOT earn
simultaneously through both tiers or through both active and ineligible
accounting. Changes in balances, underlying BLND per LP share, or pool status
MUST NOT retroactively alter already accrued emissions.

## 11. Protocol fees and interest accounting

No ongoing protocol fee is approved by this specification.

Before implementing one, the specification MUST define a conservation equation
showing how borrower interest is divided among:

- Supplier yield.
- Backstop compensation.
- Protocol or treasury revenue.
- Any other recipient.

It MUST state whether the fee increases the borrow rate, reduces supplier
yield, reduces backstop compensation, or combines those effects. Tests MUST
verify the equation under accrual, repayment, liquidation, and bad debt.

## 12. Designated token selection

The next designated token MAY be an LP token or a plain SEP-41 asset. The
legacy emitter only requires a compatible token balance interface.

Before queuing a token, the candidate design MUST verify:

- It implements the expected token interface.
- The candidate can custody it directly.
- Future candidates can acquire and transfer it.
- A raw-balance comparison is economically and operationally acceptable.
- Decimal conventions cannot make the next migration unintentionally trivial
  or impossible.

Selecting the designated token is separate from choosing which assets count
toward pool activation.

## 13. Authorization, immutability, and protocol replacement

The v3 candidate MUST NOT introduce protocol governance, multisig control,
administrator-controlled recovery, a protocol-wide emergency override, or an
independent WASM upgrade authority.

The candidate backstop, factory, migration policy, activation policy,
emissions policy, and loss-waterfall rules MUST be immutable after deployment.
Their contract interfaces MUST NOT expose an entry point that replaces contract
WASM or changes those protocol-wide rules through a privileged caller.

Future protocol-wide changes MUST deploy a new candidate and use the existing
legacy-emitter replacement mechanism:

1. The new candidate directly acquires a strictly greater balance of the
   incumbent designated token.
2. A permissionless caller creates and observes the emitter's 31-day queue.
3. Permissionless preparation verifies the queue during its final seven days.
4. A permissionless caller finalizes the emitter swap after unlock.

Deployment keys and transaction submitters are operational actors only. They
MUST NOT receive continuing protocol authority merely because they deploy,
fund, prepare, finalize, or synchronize a candidate.

The sole continuing privileged operating-authority exception is a separately
and explicitly configured immutable pool-local operator holding only the
bounded status-selection authority in Section 8.1. It includes the
v2-compatible risk-on Admin Active override and risk-off Admin On-Ice and Admin
Frozen controls. The authority is not inherited merely by deploying a pool and
cannot act as governance, recovery, a protocol-wide emergency override, or a
protocol-upgrade path.

User authorization remains required for custody-changing actions involving
that user's assets or shares. This is distinct from protocol governance and
MUST NOT create authority to alter global policy or bypass migration
invariants.

No recovery path may override the emitter's queue, balance comparison, or
unlock time. Bounded permissionless recovery may only observe, validate, or
synchronize state already established by the legacy emitter.

## 14. Numeric and resource safety

- Contract arithmetic MUST use integers and explicit fixed-point scales.
- Release overflow checks MUST remain enabled.
- Saturating arithmetic MAY be used only where all saturated results are
  intentionally equivalent, such as a threshold comparison, and must be
  documented.
- Conversions MUST define floor or ceiling behavior.
- Asset decimals MUST be validated.
- Critical calls MUST use bounded work and bounded ledger footprints.
- The top-level candidate MUST extend its code and instance TTL to 90 days at
  construction and whenever the remaining TTL falls below 89 days.
- TTL tests MUST advance ledger sequence, not only timestamp, and cover the
  83 days from earliest preparation through the two-retry recovery horizon.
- Large asset sets, tier sets, reward zones, and recipient lists MUST be
  processed in bounded batches or constrained by explicit limits.
- Contract storage and WASM TTL requirements MUST be tested on a network-style
  environment.

## 15. Security and testing acceptance

Before a candidate can be described as migration-ready:

1. The inherited v2 suite passes.
2. Native and WASM v2-to-v3 migration tests pass.
3. Queue, cancellation, expiry, balance-race, and repeat-finalization cases
   pass.
4. Activation boundary and manipulation tests pass for every supported asset.
5. Withdrawal, loss, auction, and emission interactions pass.
6. Authorization tests do not depend solely on `mock_all_auths()`.
7. Resource use is measured under maximum supported pools, assets, tiers, and
   positions.
8. An isolated local-network deployment completes the migration.
9. An isolated public-testnet deployment exercises production transaction
   assembly and operational monitoring.
10. Independent security review is completed before any mainnet proposal.

The detailed test matrix is maintained in `docs/MIGRATION_TEST_PLAN.md`.

## 16. Open decisions

The following require explicit resolution before their associated behavior is
implemented:

- Per-tier auction, transfer, and disposal mechanisms.
- Behavior when a tier's oracle or disposal market is unavailable.
- Production LP price sources, averaging, staleness, and failure policy.
- Exact lending, auction, and waterfall entry points that invoke each canonical
  pool loss-record transition.
- Exact take-rate accrual integration and distribution cadence.
- Post-migration designated token.
- Top-level BLND split between backstop depositors and pool supply/borrow
  positions.
- Protocol-fee existence and incidence.
- Whether existing v2 pool positions remain isolated or receive an explicit
  position-migration mechanism.

## 17. Initial implementation milestones

1. Preserve the v2 incumbent and migration harness.
2. Add an isolated v3 backstop crate with emitter-compatible entry points.
3. Implement continuous BLND:USDC tier deposits and migration-backfill
   checkpoints.
4. Add one independently configured, prefundable post-migration backstop
   asset without activation, loss, backfill, or emission eligibility.
5. Implement a canonical, testable activation-policy interface.
6. Implement the bounded BLND:USDC withdrawal queue and its fail-closed pool
   callback, then connect that callback to canonical loss state without forced
   conversion.
7. Complete native, WASM, local-network, and testnet migration exercises. The
   local-network lane uses exact production queue constants, so its
   Quickstart runner is resumable across the wall-clock queue rather than
   deploying shortened-delay evidence. A transaction-backed keepalive
   compensates for Quickstart's faster ledger cadence without changing queue
   timestamps or economic state.

Milestones 1 through 5, the bounded-Q4W and canonical-loss-state-boundary
portions of milestone 6, and the reproducible deployment runner for the local
portion of milestone 7 are
represented by the frozen v2 baseline, isolated Protocol-27 backstop crate,
cross-version legacy-emitter migration harness, the candidate's
pool-attributed continuous BLND:USDC shares, time-weighted migration
checkpoints, isolated single-asset prefunding lane, canonical asset-neutral
12,500-USDC entry and 10,000-USDC maintenance activation-policy arithmetic, and
the 17-day bounded withdrawal queue, an immutable Protocol-27 pool factory, and
a fail-closed pool callback derived directly from three bounded canonical
record maps. The local runner deploys independently issued assets, a coherent
SDK-22 incumbent stack, and the SDK-27 candidate stack under testnet resource
limits while recording transaction and cost evidence. A completed wall-clock
local run, the public-testnet exercise, the lending, auction, and waterfall
entry points that invoke those record transitions, the valuation adapter,
pool-status integration, and BLND funding and claims remain intentionally
unimplemented.

New economic features should be introduced only after the corresponding open
decision is resolved in this specification.
