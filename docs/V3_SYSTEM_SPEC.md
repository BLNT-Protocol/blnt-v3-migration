# Blend V3 Candidate Specification

Status: Draft 0.2

Implementation status: See
[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md).

## 1. Purpose

This document defines only the additions, replacements, extensions, approved
exceptions, and safety fixes introduced by the experimental Blend Protocol v3
candidate on Stellar. It composes with the frozen requirements in
[V2_SYSTEM_SPEC.md](V2_SYSTEM_SPEC.md) under the inheritance and precedence
rules in [SYSTEM_SPEC.md](SYSTEM_SPEC.md).

The initial candidate focuses on:

- Compatibility with the legacy emitter migration mechanism.
- A backstop capable of supporting more than one approved asset or LP token.
- Explicit separation of migration capital, activation capital, emissions, and
  loss absorption.
- A safe path for legacy BLND:USDC backstop depositors.

This specification authorizes only the economic policies stated explicitly
below. Any additional or modified economic policy MUST be explicitly
documented in this specification before it is implemented. The candidate
introduces no separate governance or administrative upgrade system.

Normative terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY` describe candidate
requirements.

### 1.1 Inheritance and delta classification

Classification, inheritance, and precedence are governed by
[SYSTEM_SPEC.md](SYSTEM_SPEC.md). A v3 difference MUST be required by:

- the three-tier backstop;
- legacy-emitter migration compatibility;
- Protocol 27 compatibility; or
- an approved conservation or safety fix; or
- a separately approved exception recorded in this specification.

Integer carry-forward at the same pool, reserve, tier, or user accounting
scope is an approved conservation fix. The 12,500-USDC entry / 10,000-USDC
maintenance hysteresis is the sole currently approved economic exception.
Implementation details MUST NOT introduce any other fee, allocation,
privileged role, governance system, or upgrade path.

[V2_PARITY.md](V2_PARITY.md) records the cross-reference and implementation
status of the major approved differences.

## 2. System boundaries

### 2.0 Runtime delta — **Replaced**

The project supports exactly two independent toolchain lanes:

- The frozen SDK-22 v2 baseline governed by `V2-RUNTIME-001`.
- New v3 contracts MUST target Stellar Protocol 27 with Rust 1.91.0, Soroban
  SDK 27.0.2, and the `wasm32v1-none` target.

Compatibility between the frozen SDK-22 v2 contracts and the SDK-27 v3
candidate MUST be demonstrated through migration behavior, contract
interfaces, authorization, state transitions, and accounting invariants. V3
does not need to compile against SDK 22 or any intermediate SDK version.

### 2.1 In scope

- V3 backstop accounting and custody.
- Pool-specific activation calculations.
- Loss tiers and loss allocation.
- Ongoing BLND-emission distribution.
- Pre-migration backfill accounting.
- V2-to-v3 emitter migration.
- Continuous BLND:USDC tier deposits and withdrawals across migration, except
  for the bounded fail-closed interval after a direct emitter swap and before
  candidate synchronization.
- Deployment and testing of candidate pools and supporting contracts.

### 2.2 Out of scope for the first milestone

- Mainnet deployment.
- Any governance, multisig, or alternate upgradeability system.
- Cross-chain collateral.
- Permissioned lending hooks.
- Fixed-term or fixed-rate lending.
- A production frontend.
- Any protocol fee. This candidate implements none.
- Any opt-in movement of a user's v2 position into v3.

These features require separate specifications before implementation.

Any later candidate that proposes a protocol fee MUST provide a conservation
equation for borrower interest and state whether the fee increases the borrow
rate, reduces supplier yield, reduces backstop compensation, or combines those
effects. Its tests MUST cover accrual, repayment, liquidation, and bad debt.

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
- **Tier:** A backstop asset's priority in the loss waterfall.
- **Queued capital:** Capital associated with a pending withdrawal.

## 4. Legacy-emitter compatibility — **Added obligations**

V3 inherits `V2-EMITTER-001` through `V2-EMITTER-003` without modifying the
deployed emitter. All additions are candidate-side obligations.

Qualifying migration capital MUST remain in the candidate's direct designated-
token balance. The candidate MUST verify the emitter queue, qualifying balance,
registered backstop, and next designated token at the lifecycle boundaries in
Section 6. It MUST NOT treat market value, LP composition, pool attribution, or
backstop accounting as a substitute for the emitter's raw balance comparison.

The inherited queue and swap remain the only protocol-wide replacement
mechanism, subject to Section 12. Successful migration awards zero
discretionary BLND; Section 10 defines the only permitted initial-drop use.

## 5. Candidate architecture — **Added**

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

Pricing adapters, asset vaults, and migration-backfill claims MAY use separate
contracts, but the top-level candidate contract MUST satisfy the emitter's
direct-balance rule.

The incumbent v2 contracts MUST remain available in the test workspace so the
same test can deploy an incumbent and a candidate.

V2 position and emission isolation remain governed by `V2-ISOLATION-001`; this
specification introduces no override.

## 6. Candidate lifecycle — **Added**

### 6.1 Pre-migration

The migration uses these immutable limits:

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
Permissionless `begin_migration()` atomically creates the canonical emitter
queue and records its original accounting epoch. If another caller creates the
correct queue first, permissionless `open_migration_epoch()` may record it
while it remains observable. Neither operation treats the queue as finalized.

Preparation requires an opened epoch, a currently verifiable queue, the
strictly greater direct designated-token balance, and invocation during that
queue's preparation window. The original unlock permanently anchors the
recovery horizon. Sections 7 and 10 govern the continuous BLND:USDC tier and
its backfill accounting; lifecycle state does not create a separate balance or
a lower-risk temporary position.

### 6.2 Queue period

During the 31-day queue:

- The incumbent remains the emitter recipient.
- Repeated preparation of the same queue MUST be idempotent.
- A replacement MUST target the configured candidate and designated token,
  receive fresh verification and preparation, and fit every limit in Section
  6.1. Cancellation or re-queueing MUST NOT move either absolute horizon.
- The candidate MUST continue validating its qualifying balance.
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
- End time-weighted backfill at the earlier of successful emitter migration or
  76 days after the original verified queue unlock.
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

The canonical path is `candidate.finalize_migration()`, which invokes
`swap_backstop`, verifies the emitter transition, and activates the candidate
atomically. `candidate.sync_migration()` is only the recovery path when another
caller invokes the emitter directly first.

The synchronization deadline is seven days after the currently verified
queue's unlock time, not seven days after the direct swap transaction. A direct
swap performed late in that interval leaves only the remaining portion of the
same fixed grace period. Once the emitter swap succeeds, a subsequent loss of
the candidate's strictly-greater legacy-token balance does not undo the emitter
transition or prevent synchronization. Failure to synchronize before the fixed
deadline can leave the emitter pointing to a candidate whose lifecycle remains
`Prepared`; this draft accepts that bounded operational risk rather than adding
a governance recovery path.

Because the emitter exposes no immutable timestamp for a direct swap,
recovery-path synchronization MUST cap migration backfill at the currently
verified queue's unlock time. Canonical candidate finalization continues to
end backfill at its atomic swap-and-activation timestamp. The recovery cutoff
may omit backfill between unlock and a later direct swap, but it guarantees
that no interval can receive both migration backfill and ongoing BLND emitted
after the direct swap.

BLND:USDC deposits at or after the currently verified queue's unlock MUST
remain ordinary tier capital but receive zero migration-backfill weight for
that queue. This prevents a post-cutoff deposit timestamp from making the
recovery-path aggregate weight negative. If a replacement queue is later
verified, new deposits before that replacement unlock are eligible from their
deposit time; an earlier ineligible deposit does not become eligible
retroactively.

If the emitter already points to the candidate while its lifecycle remains
`Prepared`, every BLND:USDC or BLND:XLM weight mutation and every reward-zone
edit MUST fail until `sync_migration()` succeeds. Plain-USDC operations remain
available because they affect neither migration backfill nor ongoing BLND
weight. This temporary fail-closed boundary prevents the unsynchronized
interval from being reweighted and guarantees that candidate mutations cannot
strand recovery.

`prepare_migration()` MUST NOT silently adopt an emitter queue when no epoch
has been recorded. Because the emitter does not retain canceled queue history,
an unobserved queue that is canceled cannot later be reconstructed on-chain.
The fixed limits in Section 6.1 bound this operational assumption.

### 6.4 Post-migration

After finalization:

- Ongoing BLND emissions accrue according to the v3 emission policy.
- The configured BLND:XLM Comet LP is the emitter-designated token controlling
  qualification for the next protocol migration.
- Existing candidate BLND:USDC tier positions continue in that same first-loss
  tier without conversion, an asset swap, or a second opt-in.
- Incumbent position and emission isolation remain exactly as specified by
  `V2-ISOLATION-001`.

## 7. Multi-asset backstop — **Replaced and extended**

This section replaces the single accepted token and generalizes the
pool-share and Q4W accounting in `V2-BACKSTOP-001` through
`V2-BACKSTOP-003` only as stated below.

### 7.1 Asset configuration

The candidate immutably binds exactly these three seven-decimal assets:

| Tier | Asset | USDC valuation | Migration role | Backfill | Ongoing BLND weight |
| ---: | --- | --- | --- | --- | --- |
| 1 | BLND:USDC 80:20 LP | Normative valuation adapter | Incumbent comparison | Eligible as specified in Section 10 | Yes |
| 2 | BLND:XLM 80:20 LP | Normative valuation adapter | Next designated token | No | Yes |
| 3 | Plain USDC | One-for-one | None | No | No |

All three tiers count equally per verified USDC for activation, participate in
the fixed-weight take-rate policy, and absorb loss in table order. There is no
asset-specific activation haircut or concentration limit.

The candidate supports no optional custom backstop tiers. Adding another
backstop asset requires a new protocol candidate and the legacy-emitter
replacement process.

Eligibility additionally depends on pool attribution and capital state as
specified in Section 7.3.

### 7.2 Position accounting

The backstop MUST store one immutable v3 pool-factory address, MUST verify the
factory's `backstop` and `is_pool` interfaces during construction, and MUST
reject a factory whose immutable backstop is not the candidate itself. Before
accepting an accounted deposit into any tier, it MUST verify that the
attributed pool is registered by that factory and that the pool exposes the
fail-closed withdrawal callback. Registration or interface failure MUST abort
before custody or accounting changes. The callback's current boolean result
does not gate a deposit: a pool with an existing loss MAY accept new
loss-absorbing capital, while later withdrawal remains blocked until the pool
reports it safe.

Canonical valuation MUST require factory registration but MUST NOT repeat the
pool loss-callback probe. A stored-status transition begins in the pool, calls
the backstop for canonical valuation, and therefore cannot safely introduce a
backstop-to-pool callback in the same invocation. The immutable factory deploys
only the approved pool WASM, so registration is the authoritative identity
check for this path. Custody entry points retain the stronger callback probe.

`backstop_withdrawal_allowed(backstop)` is a permanent compatibility boundary.
Every factory-deployed pool MUST expose its signature and fail-closed
semantics. The immutable factory deploys only the approved pool WASM, and this
candidate exposes no pool-WASM replacement path. Any future candidate that
proposes pool upgrades requires a separate specification and MUST preserve this
interface.

Each pool-tier independently applies the deposit, redemption, exchange-rate,
pool-isolation, and final-redemption rules in `V2-BACKSTOP-002`. Checked
`assets`, `shares`, and `queued_shares` are stored per pool and tier; global
per-tier totals provide a bounded conservation check but grant no cross-pool
claim. Raw direct transfers are excluded.

Active and queued shares use one exchange rate and absorb gains and losses
proportionally. Canonical policy converts the two partitions independently and
rounds asset amounts down; partition dust remains in the pool-tier until final
redemption.

As a v3 liveness safety fix, expired shares in a fully drained tier MAY be
burned for zero assets. A new deposit remains prohibited while worthless
shares exist and may initialize a fresh one-to-one exchange rate only after all
outstanding shares are removed. No administrator or external caller can adjust
an exchange rate directly.

### 7.2.1 Continuous BLND:USDC migration tier

The incumbent BLND:USDC LP token and the candidate's BLND:USDC first-loss tier
MUST use the same token contract and one continuous share ledger. A deposit
through the accounted BLND:USDC entry point is a tier deposit from the moment
it is accepted, whether it occurs before or after emitter migration.

Before migration succeeds, the same shares additionally:

- Contribute their tokens to the candidate's direct legacy-emitter qualifying
  balance.
- Accumulate migration-backfill weight while they remain deposited and their
  deposit precedes the currently verified queue's unlock.

Once an exchange rate differs from one token per share, the position stores
backfill-eligible BLND:USDC principal separately from its tier shares. New
pre-unlock deposits add their actual LP-token amount to backfill principal, not
the possibly larger or smaller number of shares minted. Deposits at or after
the verified unlock add no principal for that queue. A withdrawal forfeits
principal and time basis pro rata to the shares burned. Loss absorption does
not forfeit backfill principal: those LP tokens remained committed and were
consumed protecting the attributed candidate pool rather than withdrawn by the
depositor.

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

### 7.2.2 BLND:XLM and plain-USDC tiers

BLND:XLM and USDC accept pool-attributed deposits before and after migration
and apply Sections 7.2 and 7.3 independently. Neither enters the incumbent
emitter comparison or migration backfill. Their other roles are fixed by the
table in Section 7.1.

The constructor and legacy read interface retain the name `prefund_token` for
plain USDC for deployment compatibility; `usdc_token` is the canonical read
name.

### 7.3 Withdrawals

Each tier independently inherits the 17-day delay, 20-entry per-user/pool
bound, oldest-first withdrawal, newest-first dequeue, and common-exchange-rate
loss exposure in `V2-BACKSTOP-003`.

Capital state has these canonical policy effects:

| Capital state | Emitter comparison | Migration backfill | Activation/status | Loss | Take rate | Ongoing BLND |
| --- | --- | --- | --- | --- | --- | --- |
| Active accounted shares | BLND:USDC only | Eligible BLND:USDC only | Included | Available | Included | BLND-bearing tiers |
| Queued shares | BLND:USDC only, until withdrawn | Preserved until actual withdrawal | Excluded | Available | Included | Excluded |
| Bad-debt committed assets | Inherits underlying share state | Inherits underlying share state | Excluded | Reserved for that loss | Included | Inherits active-share state until transferred |
| Raw direct transfer | BLND:USDC only | Excluded | Excluded | Excluded | Excluded | Excluded |

Actual BLND:USDC withdrawal before successful migration forfeits the withdrawn
shares' proportional backfill weight. Queueing alone does not. User, pool, and
global shares decrease only when an expired withdrawal transfers custody out.

While an interest-auction commitment is active, deposits into and actual
withdrawals from its selected tier MUST fail; queueing and dequeueing remain
available because total shares do not change. Other tiers are unaffected.
Permissionless stale-auction deletion bounds this lock to the inherited
500-ledger stale boundary.

Every withdrawal calls
`backstop_withdrawal_allowed(backstop)` on the attributed pool. The callback
MUST return true only for the pool's immutable backstop and only when its
bounded canonical records contain no nonzero backstop liability, committed
loss, or unresolved bad debt. Liability and unresolved-debt amounts remain
reserve-keyed and MUST NOT be summed across asset units. Missing or inconsistent
records, a missing interface, a failed call, or any nonzero count fails closed.
No privileged override is permitted.

Borrow, repay, liquidation handoff, bad-debt preparation and fill,
continuation, stale release, and supplier default MUST update their affected
canonical records atomically. An interest-auction commitment is a gain process
and does not enter this pool-level loss callback; the backstop enforces its
selected-tier lock directly.

## 8. Pool activation — **Replaced**

This section replaces the v2 product threshold in `V2-BACKSTOP-004`.

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
per eligible token.

The candidate binds one valuation-adapter address immutably at construction.
Its interface, immutable asset binding, production pricing policy, conservative
80:20 Comet formula, TWAP requirements, rounding, and failure behavior are
normatively specified by
[VALUATION_ADAPTER.md](VALUATION_ADAPTER.md). Candidate construction MUST
reject a version or BLND, USDC, BLND:USDC, or BLND:XLM binding mismatch.

The candidate derives LP amounts from its own pool-attributed share ledgers,
calls the adapter separately for active and queued BLND:USDC and BLND:XLM
shares, and values accounted plain USDC exactly one-for-one. Zero LP amounts
do not require an external quote. A missing, incompatible, failed, negative, or
expired quote fails canonical valuation atomically. Eligibility follows Section
7.3; unattributed direct transfers cannot enter the calculation.

This canonical valuation is shared by permissionless activation refresh,
pool-status thresholds, reward-zone thresholds, take-rate allocation,
backstop-auction sizing, and supplier-loss eligibility. Instantaneous Comet
composition determines underlying-BLND emission weight only; it does not set
USDC value.

### 8.1 Pool-status valuation — **Extended and approved exception**

V3 inherits `V2-POOL-ADMIN-001`, `V2-POOL-CONFIG-001`,
`V2-POOL-STATUS-001`, and `V2-POOL-STATUS-002`. It replaces only the
activation threshold and Q4W measurement basis. Pool \(p\)'s Q4W percentage
uses verified USDC value:

\[
Q_p =
\left\lceil
\frac{V_{p,\mathrm{queued}}}
{V_{p,\mathrm{active}} + V_{p,\mathrm{queued}}}
10^7
\right\rceil
\]

Active and queued values MUST use the same canonical, manipulation-resistant
valuation inputs. Take-rate weights MUST NOT affect this ratio. If the
denominator is zero, \(Q_p = 0\); activation independently keeps the pool
On-Ice. Every comparison uses seven-decimal precision and rounds upward.

The inherited inclusive 30%, 50%, 60%, and 75% transitions and bounded admin
overrides apply to \(Q_p\) unchanged. Statuses 0 and 1 use the 10,000-USDC
maintenance threshold. Statuses 2 through 6 are inactive and MUST meet the
12,500-USDC entry threshold before returning to an active status. Statuses 4
and 6 continue to reject permissionless refresh.

Queueing does not itself change stored status. Status refresh and admin status
requests use canonical valuation. Stored status is not an additional
reward-zone gate. Bad-debt and tier-specific interest-auction direct entry
points remain available under every status.

### 8.2 Pool integration — **Inherited with safety extensions**

V3 inherits the lending request model, pool administration, reserve
configuration, bToken/dToken rounding, interest curve, health calculation,
limits, disabled-reserve behavior, and atomic rollback requirements in
`V2-POOL-ADMIN-001`, `V2-POOL-CONFIG-001`, and `V2-LENDING-001` through
`V2-LENDING-005`.

The v3 pool implements inherited request types 0 through 5.
User-liquidation fill and cancellation retain codes 6 and 9. Tier-specific
interest and bad-debt processes use dedicated direct entry points.

Only these integration and safety extensions apply:

- A submission contains at most 60 requests. This Protocol-27 resource-safety
  bound applies equally to direct, allowance, and flash-loan request sets.
- The immutable oracle interface returns a positive price, exact decimal scale,
  and inclusive validity timestamp. Wrong decimals, nonpositive prices, or an
  expired timestamp fail closed.
- Intermediate fixed-point multiplication uses checked I256 arithmetic.
  Results that do not fit i128 fail the transaction.
- Each submission verifies exact per-asset pool token-balance deltas. A
  fee-on-transfer token, downward rebase during the call, or other mismatch
  fails atomically. This does not make later issuer clawback,
  deauthorization, or out-of-band negative rebasing safe.
- When the configured backstop borrows or repays, the transition atomically
  synchronizes the reserve-keyed canonical backstop-liability record used by
  the withdrawal gate. Values remain in their reserve units and are never
  summed across assets.
- Reserve and user state use bounded persistent entries with explicit
  Protocol-27 TTL renewal.

These extensions participate in the inherited atomic rollback boundary,
including their backstop records and exact token-delta checks.

### 8.3 User-liquidation integration — **Inherited with safety extensions**

V3 inherits `V2-AUCTION-001` and `V2-AUCTION-002`, including creation,
health, incentive, repair-band, percentage, curve, fill, cancellation, stale
deletion, status, filler-health, and aggregate-share-supply behavior.

The v3 change affects only the completed-auction bad-debt handoff. When a
completed liquidation exhausts collateral while liabilities remain, the same
transaction MUST:

- Move every residual dToken share from the liquidatee to the configured
  backstop without changing reserve dToken supply.
- Accumulate each amount with any liability the backstop already holds in that
  reserve.
- Update every affected canonical reserve-keyed liability record.
- Clear the liquidatee's residual liabilities.
- Emit one handoff event per affected reserve.

The handoff MUST NOT prepare, resize, fill, or settle a bad-debt auction in the
same submission. If an earlier bad-debt auction is active, its committed bid
and tier lot remain unchanged; the new liabilities remain canonical input to a
later permissionless continuation.

An incomplete fill never performs the handoff. A fill that exhausts collateral
while its auction remains incomplete MUST fail atomically. A fill that leaves
collateral and liabilities but reduces health MUST also fail atomically.

The handoff loop is bounded by the inherited 30-reserve maximum and MUST batch
its canonical liability-map mutation into one load and store. The
maximum-reserve optimized WASM path MUST remain within Protocol-27 mainnet
invocation limits.

## 9. Loss waterfall — **Replaced**

This section replaces the single-token bad-debt lot and exhaustion sequence in
`V2-AUCTION-003`. It retains the inherited auction curve and supplier-default
arithmetic except where this section states a bounded multi-tier extension.

The candidate uses the following strict first-loss order:

1. BLND:USDC LP.
2. BLND:XLM LP.
3. Plain USDC.
4. Pool suppliers for any residual loss after every backstop tier is
   operationally exhausted under the minimum-value policy below.

Tier ordering and capital-state eligibility follow Sections 7.1 and 7.3. A
loss MUST exhaust eligible, uncommitted capital in one tier before consuming
the next tier. Active and queued shares absorb a tier loss through their
inherited common exchange rate. Loss and capacity comparisons use the
canonical verified USDC unit, rounding MUST NOT understate loss passed to a
later tier, and committed capital MUST NOT be counted twice.

The implemented auction boundary retains v2's
one-bad-debt-auction-per-pool shape and 120%-of-debt target, but each auction
MUST sell assets from only one tier. It examines currently uncommitted capital
in strict waterfall order and selects the first tier whose available capital
has at least 200 USDC of verified value. Exactly 200 USDC qualifies. A tier
below 200 USDC is skipped even if it could satisfy the current 120% target;
there is no solve-the-debt exception. Because the comparison is per tier, at
most just under 600 USDC of aggregate backstop value can be skipped before
supplier loss begins.

The 200-USDC threshold is an operational auction minimum, not a risk weight or
an activation threshold. Capital below it remains attributed to its
depositors, is not consumed by the current auction, and can become eligible
again if its verified value returns to the threshold before supplier
settlement. Unavailable, invalid, or stale valuation is not evidence that a
tier is below the minimum and MUST fail closed rather than advancing the
waterfall. Plain USDC is valued one-for-one.

The pool converts its oracle-valued reserve debt to seven-decimal USDC, and
only the registered pool contract can authorize the backstop commitment. The
selected lot is the smaller of the tier's available token amount and the token
amount needed for the 120% target. LP selection uses the immutable valuation
adapter and rounds a partially selected tier's token amount up so its
proportional value cannot underfill the target. Production adapters MUST
guarantee that a quote's per-share value is linear across the selected LP
amount.

Commitment does not transfer assets, burn shares, or change the pool-tier
exchange rate. Reserved assets are excluded from activation value and later
loss quotes so they cannot be counted twice. The pool auction and backstop
commitment are created atomically with identical fixed 46-day temporary
lifetimes; reads do not renew either entry. A stale auction can be deleted
permissionlessly after 500 ledgers, atomically releasing its lot. If both
temporary entries expire, the backstop liability records still keep
withdrawals blocked until the underlying debt is resolved. Once a tier lot is
committed, subsequent price movement below 200 USDC does not cancel or shrink
that commitment.

Bad-debt fills inherit the start, caller, percentage, filler-health, status,
400-ledger curve, and asymmetric base-selection rules in `V2-AUCTION-001` and
`V2-AUCTION-003`. The current ledger modifier scales only the dToken shares and
tier tokens actually transferred. The selected base amounts are removed from
the auction even when their time-scaled transfers are smaller.

The filler assumes the time-scaled dToken bid directly from the configured
backstop and receives the time-scaled selected tier token directly from the
backstop contract. The protocol performs no token swap and does not depend on
a disposal market at fill time. The percentage-selected base lot reduces the
commitment, but only the time-scaled token amount actually transferred reduces
the pool's and tier's accounted assets. Shares are not burned, so that actual
asset reduction impairs active and queued shares proportionally through their
common exchange rate. Any difference between the base lot and the transferred
lot becomes uncommitted capacity without causing an asset loss.

A partial fill stores the remaining base bid and lot and applies the same
45-day renewal threshold and 46-day bump to both temporary entries. Remaining
committed value is reduced in proportion to the remaining base lot; the
original debt, target, unfilled-target, and valuation-validity fields remain
creation metadata. A complete fill removes both the pool auction and backstop
commitment. Pool positions, canonical backstop liabilities, filler health,
auction state, commitment state, tier accounting, and the token transfer MUST
all succeed or roll back together.

At the late bid-discount portion of the inherited curve, a complete auction can
leave dToken liabilities assigned to the backstop because the removed base bid
can exceed the time-scaled bid assumed by the filler. Those canonical
liabilities continue to block withdrawal.

Once no bad-debt auction is active, any caller may invoke
`continue_bad_debt_resolution` with the identifier for a possible next
auction. The call requires no caller authorization and performs one bounded
step. It validates every configured-backstop liability against the pool's
canonical reserve-keyed records. Unknown, missing, negative, or inconsistent
records fail atomically. When the liability set is larger than one auction's
position bound, the next bid batch is selected deterministically in immutable
reserve-list order and contains at most
`min(max_positions - 1, MAX_BAD_DEBT_BID_ASSETS)` reserve assets. The
candidate fixes `MAX_BAD_DEBT_BID_ASSETS` at four so stateful per-reserve
oracle reads retain ledger-footprint headroom while the same call validates
the complete reserve set and may default it to suppliers.

The pool values that batch with its immutable reserve oracle and asks the
configured backstop for the next lot. If the first qualifying tier exists, the
pool commits the returned quote under the supplied identifier and creates the
next-ledger auction. The quote used for the decision and the committed quote
MUST be identical. The strict tier search begins again on every continuation,
so a tier is reused while its remaining uncommitted value is still at least
200 USDC; later tiers cannot be reached merely because an earlier auction
completed.

If and only if the backstop successfully returns no eligible tier, the pool
defaults every remaining canonical backstop liability to suppliers in the
same transaction. Because a no-lot result means each of BLND:USDC LP,
BLND:XLM LP, and plain USDC was successfully valued below 200 USDC, invalid,
unavailable, or stale valuation fails closed before supplier loss. The
supplier-default loop is bounded by the immutable 30-reserve maximum.

For each affected reserve, the pool first accrues interest and then applies the
supplier-default arithmetic and reserve transitions in `V2-AUCTION-003`
unchanged. After every reserve succeeds, the canonical liability map is
cleared and withdrawal eligibility is recomputed. Any failure rolls back the
complete continuation.

### 9.1 Take-rate allocation — **Replaced**

This section replaces the single-token interest-auction realization in
`V2-AUCTION-004` with tier-specific realization.

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

A tier with zero eligible value contributes zero to the numerator and
denominator. The formula therefore renormalizes across only populated,
eligible tiers.

Take-rate eligibility follows Section 7.3. The canonical weighting quote uses
full accounted assets without subtracting assets committed to bad debt. Bad-
debt and interest commitments may coexist, but their transfers MUST remain
independently accounted and atomic.

The fixed weights compensate earlier-loss capital more per USDC of eligible
value without changing activation weighting. Take-rate weighting is separate
from BLND-emission allocation and does not determine the loss amount charged to
a tier.

The allocation determines which tier receives each reserve-asset lot; it does
not transfer reserve assets directly to depositors. For every pool and reserve
asset, the pool MUST store pending amounts for each tier and one integer carry.
The carry remains isolated to that pool and reserve asset. New accrued credit
plus its carry is apportioned with the canonical `5:5:4` weighted-value quote.
The three floor-rounded tier amounts and the new carry MUST conserve the exact
input amount. Remainders from different pools or reserve assets MUST NOT be
combined, converted, or silently assigned to a favored tier.

Any caller may create one tier-specific interest auction for a pool by
supplying a unique auction identifier and a bounded, duplicate-free batch of
one to `min(max_positions - 1, 4)` configured reserve assets. Creation MUST:

1. Accrue and persist every reserve in the supplied batch.
2. Checkpoint each asset's newly available backstop credit into its three
   pending tier amounts through one canonical backstop weighting quote.
3. Value each tier's pending amounts with the pool's immutable reserve oracle.
4. Starting at the pool's stored cyclic tier cursor, select the first supplied
   tier whose pending lot is worth at least 200 USDC.
5. Reserve the selected pending amounts in a next-ledger auction and advance
   the cursor to the next tier.

Exactly 200 USDC qualifies; a smaller lot does not. A missing, nonpositive,
invalid-decimal, or stale reserve or LP valuation fails closed. Omitted reserve
assets remain uncheckpointed or pending and cannot be reassigned, but the
bounded selection guarantee covers only the caller-supplied batch. A pool may
have at most one active interest auction. Selection MUST be cyclic across the
qualifying tiers rather than follow the loss waterfall, so a continuously
qualifying tier cannot starve another qualifying tier.
The persistent pending amount continues to include the reserved lot. This
prevents simultaneous expiry of the matching temporary records from losing or
reweighting its tier allocation; a later creation can reserve it again.

The selected tier supplies the auction bid token:

| Selected tier | Bid token donated on fill |
| --- | --- |
| BLND:USDC | BLND:USDC LP |
| BLND:XLM | BLND:XLM LP |
| Plain USDC | USDC |

Each configured bid token MUST expose seven decimals. Interest-auction
commitment fails closed if that invariant is not satisfied.

The committed base bid MUST have a verified value equal to 120% of the selected
reserve-asset lot's verified USDC value, rounded up in token units. The pool
auction and backstop commitment MUST use the identical quote, identifier,
selected tier, and fixed 46-day temporary lifetime. Only the registered pool
may create, resize, settle, or release its commitment.

Interest fills inherit `V2-AUCTION-001`. A partial fill removes the selected
base amounts, transfers only their time-scaled amounts, releases any base-lot
discount from the reservation, and leaves the pool auction and backstop
commitment synchronized. Only reserve assets actually transferred from the
pool are subtracted from both the persistent pending tier amount and accrued
backstop credit.

The filler MUST authorize the call, MUST NOT be the pool or backstop, transfers
the actual tier-token bid directly to the backstop, and receives the actual
reserve-asset lot directly from the pool. Every token movement MUST pass an
exact source-balance-delta check. The donated bid increases the selected
pool-tier's accounted assets without minting shares. Its common exchange rate
therefore appreciates every active and queued share proportionally, with no
per-user claim. Donated assets immediately participate in activation, future
loss capacity, take-rate eligibility, and, for either BLND-bearing LP tier,
ongoing BLND-emission weight.

A complete fill deletes both temporary records. At or after 500 ledgers from
the start, any caller may delete a stale auction, atomically release the
backstop commitment, and leave every unfilled base lot in its original pending
pool/reserve/tier accumulator. Stale deletion changes no persistent credit,
token balance, or tier assets. Before that boundary deletion fails.

The `5:5:4` ratio governs allocation of reserve-credit lots, not a guaranteed
ratio of realized donated market value. The inherited time curve and different
fill times can cause the market value ultimately donated to each tier to
diverge from `5:5:4`. This execution-price effect MUST be observable and
tested; no protocol fee, user claim, or compensating governance adjustment is
applied.

## 10. BLND emissions and backfill — **Added and extended**

This section extends ongoing receipt from `V2-EMITTER-001`, replaces the v2
backfill policy in `V2-BACKSTOP-007`, and constrains use of the initial-drop
facility in `V2-EMITTER-003`.

The emitter supplies BLND to the top-level candidate; the candidate determines
the internal allocation.

The candidate MUST distinguish:

- Pre-migration backfill.
- Ongoing backstop-depositor emissions.
- Pool supply and borrow emissions.

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

Backfill eligibility MUST be proportional to each surviving, pre-unlock
migration position's BLND:USDC tier shares multiplied by their holding
duration. Only eligible shares still deposited when emitter migration succeeds
are included. A withdrawal before emitter migration succeeds MUST forfeit the
withdrawn shares' complete accumulated weight, and that weight MUST NOT remain
in the denominator. A partial withdrawal MUST forfeit only the withdrawn
shares' proportional weight. Re-deposited shares begin a new holding period
only when the new deposit precedes the current verified unlock.

The final allocation weight for position \(i\) is:

\[
W_i = \sum_k A_{i,k}(T_{\mathrm{end}} - T_{i,k})
\]

where \(A_{i,k}\) and \(T_{i,k}\) are the amount and effective deposit time of
each eligible surviving tranche, and \(T_{\mathrm{end}}\) is the earlier of
successful emitter migration or the absolute recovery-horizon end. For an
eligible tranche, the effective deposit time MUST be the actual deposit time.
A tranche deposited at or after the currently verified unlock has zero weight
for that queue and is omitted from this sum. Prefunding used to establish the
candidate's qualifying balance therefore earns proportional allocation weight
before the original queue begins, while the 31-day campaign limit prevents
unbounded early-weight accumulation. Prefunding weight changes only how
scheduled BLND is divided; it does not move the 1-BLND-per-second scheduling
start earlier than the original queue. A position's allocation is:

\[
B_i = \left\lfloor B \frac{W_i}{\sum_j W_j} \right\rfloor
\]

This makes the funded allocation, including weight forfeited by early
withdrawals, available proportionally to positions that remain through
successful migration, apart from deterministic integer-rounding dust.
Finalization and claims MUST use accumulator/checkpoint accounting and MUST NOT
iterate over all depositors. The funding transaction MUST reject an amount
other than the immutable scheduled backfill obligation and MUST NOT create a
separate migration award.

The candidate MUST store the BLND token address immutably at construction and
MUST expose whether that binding has been verified.
After migration activation, any caller MAY invoke the one-time backfill-funding
entry point. The candidate MUST call the legacy emitter's initial-drop facility
with exactly one recipient, the candidate itself, and exactly the scheduled
amount \(B\). It MUST verify that its configured BLND balance increased by
exactly \(B\) before committing the funded reserve and BLND-binding proof. A
failed emitter call, unexpected token-balance delta, missing eligible weight, or
repeat funding attempt MUST fail atomically.

Claims MUST use the frozen global weight and frozen per-position checkpoint in
constant time. The position owner MUST authorize the claim and MAY select its
recipient. Each position MAY claim at most once. A successful claim MUST mark
the position claimed and update the aggregate claimed amount before transferring
BLND; failure of any later operation MUST roll back the complete claim.

Each position receives the floor-rounded amount in \(B_i\). The resulting
integer dust MUST remain in the candidate's segregated backfill reserve. It MUST
NOT be withdrawn, reassigned, counted as ongoing emissions, or distributed to a
discretionary recipient. The candidate MUST expose funded, claimed, and
remaining-reserve totals so this invariant can be audited without iterating over
positions.

Ongoing accounting MUST be based on BLND actually received after the emitter
changes recipients. No interval or position may earn under both regimes unless
the specification explicitly intends and tests that result. Ongoing-emission
accounting MUST exclude the remaining backfill reserve even though both balances
are held by the top-level candidate.

Before the distributor may use accumulated-balance reconciliation, the
configured BLND token MUST have been verified by one of two exact-delta paths:

- Successful backfill funding verifies that the configured-token balance
  increased by exactly the initial-drop amount.
- If backfill funding has not verified the binding, the first successful
  ongoing distribution MUST snapshot the configured-token balance immediately
  before invoking the emitter, require a positive emitter-reported amount, and
  require the balance to increase by exactly that amount.

The candidate MUST persist and expose the successful proof. A failed proof MUST
roll back the emitter invocation and every candidate state change. Prefunding
the configured token MUST NOT bypass either exact current-call delta. A
successful migration with zero eligible backfill weight MUST therefore remain
able to verify BLND through its first positive ongoing distribution even though
backfill funding remains unavailable. Ongoing BLND delivered directly before
the proof succeeds remains unaccounted until that successful distribution
reconciles the complete balance.

The permissionless distributor MUST invoke the emitter and then reconcile the
candidate's actual BLND balance against the remaining backfill reserve and
previously accounted, unclaimed ongoing BLND. This permits the candidate to
account for an emitter distribution invoked directly before the candidate
entry point. The emitter-reported amount for the current invocation MUST NOT
exceed the reconciled receipt, and the emitter checkpoint MUST advance without
exceeding ledger time. Matching v2, standalone permissionless distribution
calls require at least five seconds since the preceding emitter checkpoint.
An operation that changes a reward-zone pool's active BLND:USDC or BLND:XLM
amount MUST observe a completed production distribution checkpoint no more than
five seconds old or fail closed. The candidate deliberately does not combine a
maximum-30-pool distribution with the weight mutation because that combined
invocation exceeds the Protocol-27 resource envelope. This guard limits any
unresolved interval that can observe the changed weight to one five-second
ledger window instead of allowing an arbitrarily stale interval to be
reassigned.
Any other unaccounted BLND transferred directly to the candidate is
deliberately treated as donated ongoing emissions rather than being left
outside protocol accounting.

### 10.1 Ongoing backstop-depositor emissions — **Extended and safety fixed**

V3 inherits the immutable split in `V2-EMISSIONS-002` but replaces v2's
single-tier, seven-day backstop stream in `V2-BACKSTOP-006`. A successful v3
distribution checkpoint assigns the 70% tranche immediately through the two
tier-specific cumulative indexes below. This prevents a later tier-composition
change from rewriting the allocation fixed at that checkpoint.

Each amount of ongoing BLND actually received from the emitter, together with
any top-level integer carry from the preceding split, MUST use the immutable
inherited 70/30 allocation:

\[
E_{\mathrm{backstop}} =
\left\lfloor E_{\mathrm{total}}\frac{7}{10}\right\rfloor
\]

\[
E_{\mathrm{pool}} =
\left\lfloor E_{\mathrm{total}}\frac{3}{10}\right\rfloor
\]

\[
C_{\mathrm{next}} =
E_{\mathrm{total}} - E_{\mathrm{backstop}} - E_{\mathrm{pool}}
\]

where \(E_{\mathrm{total}}\) includes the preceding top-level carry. The
candidate MUST retain \(C_{\mathrm{next}}\) and include it in the next split;
it MUST NOT assign split dust to a discretionary recipient. The ratio is
hard-coded and has no operator, administrator, governance, or runtime
configuration path.

Both tranches use the same reward-zone pool weight, preserving v2's pool-level
allocation shape. Within each pool, \(E_{\mathrm{backstop}}\) follows the
backstop user weighting in this section, while \(E_{\mathrm{pool}}\) remains
segregated for pool supply and borrow positions and follows the v2-compatible
configuration rule below.

The maximum-30-pool, permissionless, unique-membership, full-zone replacement,
and post-activation one-hour checkpoint shape in `V2-BACKSTOP-005` is
inherited with these changes:

- Entry requires the 12,500-USDC threshold and positive eligible underlying
  BLND.
- Ordinary removal requires failure of the 10,000-USDC maintenance threshold.
- A zero-underlying-BLND member may be removed regardless of activation value
  and without a checkpoint.
- Full-zone replacement compares eligible underlying BLND and remains strict.
- Before migration activation, qualified entry and ordinary removal require no
  checkpoint because no ongoing candidate emissions can accrue.

Pool activation remains independent of reward-zone admission. A pool backed
only by at least 12,500 USDC can activate, but it earns no BLND and cannot enter
the reward zone until an active, nonqueued BLND:USDC or BLND:XLM position gives
it positive eligible underlying-BLND weight. It may be admitted permissionlessly
after that deposit under the ordinary checkpoint rule.

Stored pool status is not an additional reward-zone or distribution gate.
Admin On-Ice, On-Ice, Admin Frozen, or Frozen status does not by itself remove
a threshold-qualified pool or erase already accrued emissions. This preserves
v2's separation between pool status and reward-zone membership while applying
v3's explicit activation hysteresis to entry and maintenance.

Only reward-zone pools participate in ongoing emission distribution. A pool's
weight is the eligible underlying BLND held through its BLND:USDC and BLND:XLM
backstop tiers:

\[
B_p =
B_{p,\mathrm{BLND:USDC}} +
B_{p,\mathrm{BLND:XLM}}
\]

Plain USDC contributes zero BLND-emission weight. The pool allocation is:

\[
E_{\mathrm{backstop},p} =
\left\lfloor
E_{\mathrm{backstop}}
\frac{B_p}{\sum_{q \in \mathrm{reward\ zone}} B_q}
\right\rfloor
\]

\[
E_{\mathrm{pool},p} =
\left\lfloor
E_{\mathrm{pool}}
\frac{B_p}{\sum_{q \in \mathrm{reward\ zone}} B_q}
\right\rfloor
\]

Within reward-zone pool \(p\), user \(u\)'s weight is the user's eligible
underlying BLND across the same two tiers:

\[
B_{p,u} =
B_{p,u,\mathrm{BLND:USDC}} +
B_{p,u,\mathrm{BLND:XLM}}
\]

and the user's allocation is:

\[
E_{\mathrm{backstop},p,u} =
\left\lfloor
E_{\mathrm{backstop},p}
\frac{B_{p,u}}{B_p}
\right\rfloor
\]

BLND held through either eligible tier earns at the same rate per underlying
BLND. The USDC or XLM component of an LP position does not contribute emission
weight. The formula above states the economic weighting; its bounded on-chain
realization uses one accumulator per eligible tier and scoped integer carry as
follows.

At the same distribution checkpoint that fixes
\(E_{\mathrm{backstop},p}\), the candidate MUST split that pool allocation
between BLND:USDC and BLND:XLM using the checkpoint's underlying-BLND
composition:

\[
E_{p,t} =
\left\lfloor
(E_{\mathrm{backstop},p} + C_{p,\mathrm{tier}})
\frac{B_{p,t}}{B_p}
\right\rfloor
\]

The sum not assigned by the two floor operations becomes the next
pool-local tier-split carry \(C_{p,\mathrm{tier}}\). This freezes tier
attribution in the distribution invocation; a later Comet-composition change
MUST NOT rewrite it.

Each eligible tier uses a \(10^{14}\)-scaled cumulative per-share index. For
active, nonqueued tier shares \(S_{p,t}\), the index update is:

\[
N_{p,t} = E_{p,t}10^{14} + C_{p,t,\mathrm{index}}
\]

\[
\Delta I_{p,t} =
\left\lfloor \frac{N_{p,t}}{S_{p,t}} \right\rfloor
\]

\[
C'_{p,t,\mathrm{index}} =
N_{p,t} - \Delta I_{p,t}S_{p,t}
\]

The index carry remains in that pool and tier. For user active shares
\(S_{p,t,u}\), checkpointing against an index change realizes:

\[
N_{p,t,u} =
S_{p,t,u}(I_{p,t}-I^{\mathrm{last}}_{p,t,u})
+ C_{p,t,u}
\]

\[
\Delta E_{p,t,u} =
\left\lfloor \frac{N_{p,t,u}}{10^{14}} \right\rfloor
\]

\[
C'_{p,t,u} =
N_{p,t,u} - \Delta E_{p,t,u}10^{14}
\]

The user's claimable amount is the sum of accrued BLND from the two eligible
tier checkpoints. A user's index MUST be checkpointed before any deposit,
queue, dequeue, or withdrawal changes that user's active eligible shares. A
new depositor starts at the current index and cannot receive prior accrual.
Queueing first realizes accrual through the current index and then makes the
queued shares ineligible; dequeueing starts those shares again at the current
index without retroactive credit.

At every emission-allocation checkpoint, the candidate MUST derive the current
underlying BLND in each BLND-bearing tier directly from that Comet contract's
same-invocation BLND reserve balance and total LP-token supply:

\[
B_t(A) =
\left\lfloor
A\frac{R_{\mathrm{BLND},t}}{S_t}
\right\rfloor
\]

where \(A\) is the candidate-accounted active LP-token amount, \(R\) is the
Comet contract's current BLND balance, and \(S\) is its current LP-token
supply. A zero or negative supply, negative reserve balance, negative LP
amount, or LP amount greater than total supply MUST fail closed. Plain USDC
does not use this formula and contributes zero BLND weight. Direct LP-token
transfers that did not mint candidate tier shares are excluded.

This deliberately uses current Comet composition rather than a TWAP, delayed
checkpoint, valuation adapter, or external price oracle. The same-transaction
reserve ratio is canonical for this emission calculation. Consequently, a
party able to move a Comet reserve ratio around the allocation checkpoint can
change that checkpoint's emission weights. This manipulation exposure is an
accepted economic policy of the candidate and MUST be documented in deployment
and security review materials.

Pool and user allocation MUST use bounded accumulator/checkpoint accounting
rather than iteration over all pools or depositors. Integer remainder MUST be
retained at its allocation scope and included in a later distribution at that
same scope.

After activation, every operation that changes a reward-zone pool's active
BLND:USDC or BLND:XLM amount MUST require a completed production distribution
checkpoint within the preceding five seconds. This applies to deposits, Q4W
queue and dequeue operations, withdrawals that change the active conversion
result, actual bad-debt loss, and interest-auction gain. A stale mutation fails
before changing candidate accounting. A weight mutation in a non-reward-zone
pool and every plain-USDC mutation require no ongoing-emission checkpoint.

Already allocated emissions MUST remain unchanged by a later weight mutation.
Emissions received after the checkpoint use the weights observed by the next
distribution, so a mutation may affect at most the unresolved five-second
window. This bounded one-ledger granularity is an explicit resource-safety
tradeoff, not an exact intra-ledger time-weighted accumulator.

Queued withdrawals receive zero ongoing BLND-emission weight immediately after
the guarded queue mutation. Dequeueing restores emission eligibility after the
same guard and MUST NOT grant emissions for an earlier queued interval beyond
the explicit one-ledger boundary above. Capital committed to a pending bad-debt
lot remains eligible until an auction fill actually transfers tier assets out
of the backstop. The fill MUST satisfy the recent-checkpoint guard before that
transfer; only the amount actually transferred loses weight. Mere commitment,
later discount, or stale release MUST NOT change already allocated emissions.

A position MUST NOT earn simultaneously through both tiers or through both
active and ineligible accounting. Changes in balances, underlying BLND per LP
share, reward-zone membership, or pool status MUST NOT alter already allocated
emissions. Active tier-amount mutations follow the five-second boundary above;
reward-zone membership follows the separately documented v2-compatible
one-hour boundary.

The claimable backstop-depositor amount is the exact result of the pool,
tier-index, and user-checkpoint formulas above, including their scoped carry.
The position owner MUST authorize a claim and MAY choose its BLND recipient.
One claim checkpoints and realizes both eligible tiers for one pool. It MUST
reduce only that pool's accrued 70% backstop tranche, update the aggregate
backstop-claimed and total-claimed counters, and transfer BLND atomically. It
MUST NOT include migration backfill or the pool's 30% supply/borrow tranche.
Claim realization MUST NOT redirect, reweight, or recompute entitlement based
on a preferred tier. No rule that compounds every claim into BLND:XLM or
another single v3 tier is introduced here.

### 10.2 Pool supply and borrow emissions — **Inherited with custody extension**

V3 inherits the identifier mapping, mutable admin-selected relative weights,
last-write-wins duplicate handling, disabled-reserve omission, once-daily
gulp, one-BLND minimum, seven-day streams, balance basis, and owner-authorized
claims in `V2-EMISSIONS-003`. It also inherits the immutable 70/30 split in
`V2-EMISSIONS-002`.

Matching the frozen setter, an empty configuration is valid and pauses future
pool-tranche distribution. As a v3 custody safety extension, `gulp` with an
empty configuration or no enabled positive weight MUST fail before reserving
the pool's candidate allocation. The accrued tranche remains available after
a valid configuration is restored. Configuration input is bounded to 60
entries for Protocol-27 resource safety, including entries later replaced by
the last-write-wins rule.

The v3 change begins after reward-zone allocation
\(E_{\mathrm{pool},p}\): integer remainder at the pool, reserve-token, and
user scopes is carried forward at that same scope, and the candidate contract
retains custody of the BLND allocated to the pool tranche.

A registered pool MUST authorize its call that moves accrued
\(E_{\mathrm{pool},p}\) into a pool-specific candidate reservation. A positive
reservation continues to obey the inherited once-per-24-hours and one-BLND
minimum. Because the candidate call and pool call are atomic, rejection leaves
the candidate's accrued tranche available for a later gulp. Reservation MUST
NOT grant a token allowance or count as a claim.

Each enabled configured identifier receives its floor-rounded proportional
allocation and the pool-local remainder carries into the next gulp. A
reserve-token allocation is streamed over seven days. A later gulp first
checkpoints the existing stream, combines its exact unvested amount with the
new allocation, and restarts the inherited seven-day horizon. Whole BLND base
units that cannot yet be represented by the scaled reserve-token index remain
reserve-token-local carry. The index scale is \(10^{d_r}\times10^7\), where
\(d_r\) is that reserve's immutable token-decimal count. Each user checkpoint
separately retains its sub-base-unit scaled carry.

Every supply, collateral, borrow, repay, liquidation transfer, bad-debt
handoff, supplier default, and applicable reserve-configuration mutation MUST
checkpoint the affected reserve-token stream and user balance before changing
that balance or its total supply. A late entrant therefore starts at the
current index and cannot receive earlier streamed BLND. Existing streams
continue after an identifier is removed from future configuration.

The position owner MUST authorize a claim over a nonempty bounded list of
unique valid identifiers and MAY select the recipient. The pool checkpoints
the selected streams, clears only those user accruals, and, when the sum is
positive, authorizes the candidate to consume exactly that amount from the
pool reservation and transfer BLND directly to the recipient. Pool state,
candidate reservation, aggregate claimed accounting, and token transfer MUST
commit or roll back atomically. This payout increases total ongoing claimed
BLND but not the backstop-depositor claimed counter.

## 11. Designated token selection — **Replaced**

The queue mechanism remains inherited from `V2-EMITTER-002`. This candidate
immutably selects the seven-decimal BLND:XLM Comet LP bound as Tier 2 in
Section 7.1 and MUST verify that exact address as the queued next designated
token. A different choice requires a new candidate specification and emitter
replacement.

## 12. Authorization, immutability, and replacement — **Added constraints**

These constraints extend `V2-IMMUTABILITY-001`, `V2-POOL-ADMIN-001`, and
`V2-AUTH-001` to the candidate's new coordinator and three-tier contracts.

The candidate backstop, factory, migration, activation, emissions, and
loss-waterfall policies MUST be immutable after deployment. Their interfaces
MUST expose no governance, multisig, protocol-wide administrator or emergency
override, privileged WASM replacement, or alternate upgrade path. Future
protocol-wide changes require a new candidate and the inherited emitter
replacement mechanism.

Deployment keys and transaction submitters are operational actors only. They
MUST NOT receive continuing protocol authority merely because they deploy,
fund, prepare, finalize, or synchronize a candidate.

The inherited transferable pool admin and user authorization rules remain
pool-local and custody-local respectively. Neither can alter a protocol-wide
binding or policy.

No recovery path may override the emitter's queue, balance comparison, or
unlock time. Bounded permissionless recovery may only observe, validate, or
synchronize state already established by the legacy emitter.

## 13. Numeric and resource safety — **Protocol-27 extensions**

These requirements extend `V2-SAFETY-001` and `V2-SAFETY-002` for the
Protocol-27 runtime and the candidate's longer migration horizon.

- Release overflow checks MUST remain enabled.
- Saturating arithmetic MAY be used only where all saturated results are
  intentionally equivalent, such as a threshold comparison, and must be
  documented.
- Critical calls MUST use bounded work and bounded ledger footprints.
- The top-level candidate MUST extend its code and instance TTL to 90 days at
  construction and whenever the remaining TTL falls below 89 days.
- TTL tests MUST advance ledger sequence, not only timestamp, and cover the
  83 days from earliest preparation through the two-retry recovery horizon.
- Large asset sets, tier sets, reward zones, and recipient lists MUST be
  processed in bounded batches or constrained by explicit limits.
- Contract storage and WASM TTL requirements MUST be tested on a network-style
  environment.

## 14. Security and testing acceptance

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

The detailed test matrix is maintained in
[MIGRATION_TEST_PLAN.md](MIGRATION_TEST_PLAN.md).

## 15. Remaining production decisions

- Selection and qualification of a concrete production SEP-40 provider whose
  deployed feeds and exact WASM satisfy the normative valuation-adapter policy.
- Production lending-reserve admission policy for issuer controls, clawback,
  deauthorization, rebasing, and other nonstandard token behavior.
