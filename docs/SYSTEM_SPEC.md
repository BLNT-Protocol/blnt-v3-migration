# Blend V3 Candidate System Specification

Status: Draft 0.1  
Implementation status: Experimental migration, backstop, status, and lending core

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

The implemented exchange-rate foundation stores checked `assets`, `shares`,
and `queued_shares` for every pool and tier. Accounted assets are pool-local:
a loss attributed to one pool MUST NOT impair another pool's tier. Global
per-tier asset totals provide a bounded conservation check but do not include
raw direct token transfers.

The first deposit into an empty pool-tier mints one share per token base unit.
Later deposits mint
\(\lfloor deposited\ assets \times existing\ shares / existing\ assets\rfloor\)
shares. A deposit that would mint zero shares MUST fail before custody or
accounting changes. In particular, a fully drained tier with outstanding
shares rejects new deposits rather than donating a recapitalization to the
worthless shares.

A withdrawal burns the requested shares and returns
\(\lfloor burned\ shares \times existing\ assets / existing\ shares\rfloor\)
token base units. Burning the final outstanding share balance returns all
remaining accounted assets, so conversion dust remains in its pool-tier until
the final redemption rather than being assigned to the protocol or a favored
depositor. Expired shares in a fully drained tier MAY be burned for zero
assets; only after all outstanding shares are removed can a later deposit
initialize a fresh one-to-one rate.

Active and queued shares in one tier MUST use the same share-to-asset exchange
rate. Queueing MUST NOT snapshot or guarantee the position's current asset
value. A gain or loss applied to the tier changes the redemption value of
active and queued shares proportionally; the implementation MUST NOT preserve
queued principal by charging the change only to active shares.

Canonical activation, status, and ongoing-emission valuation converts active
and queued shares independently at the common pool-tier exchange rate and
rounds both asset amounts down. Any partition rounding dust remains unallocated
in the pool-tier until later redemption. The exchange-rate mutation used by
the future waterfall is internal-only until the loss auction and disposal
policy is implemented; there is no administrator or external loss-adjustment
entry point.

### 7.2.1 Continuous BLND:USDC migration tier

The incumbent BLND:USDC LP token and the candidate's BLND:USDC first-loss tier
MUST use the same token contract and one continuous share ledger. A deposit
through the accounted BLND:USDC entry point is a tier deposit from the moment
it is accepted, whether it occurs before or after emitter migration.

Before migration succeeds, the same shares additionally:

- Contribute their tokens to the candidate's direct legacy-emitter qualifying
  balance.
- Accumulate migration-backfill weight while they remain deposited.

Once an exchange rate differs from one token per share, the position stores
backfill-eligible BLND:USDC principal separately from its tier shares. New
pre-migration deposits add their actual LP-token amount to backfill principal,
not the possibly larger or smaller number of shares minted. A withdrawal
forfeits principal and time basis pro rata to the shares burned. Loss absorption
does not forfeit backfill principal: those LP tokens remained committed and
were consumed protecting the attributed candidate pool rather than withdrawn
by the depositor.

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

The candidate MUST configure three distinct token contracts: the incumbent
BLND:USDC token, the emitter's next designated token representing BLND:XLM,
and plain USDC. The constructor and legacy read interface retain the name
`prefund_token` for plain USDC for deployment compatibility; `usdc_token`
provides the canonical read name.

BLND:XLM and USDC MUST:

- Accept pool-attributed deposits before and after emitter migration.
- Maintain independent, checked user, pool, and global total-share and
  queued-share ledgers.
- Participate in the same bounded Q4W and fail-closed pool-withdrawal rules as
  BLND:USDC.
- Remain separate from the incumbent emitter comparison, migration positions,
  and migration-backfill weights. BLND:XLM and USDC deposits receive no
  migration backfill.
- Supply activation, take-rate, and waterfall inputs through the eligibility
  rules defined for their respective tiers. BLND:XLM, but not plain USDC, is
  eligible for ongoing BLND-emission weight.

BLND:XLM and USDC use the same implemented checked pool-local share-to-asset
exchange rate, conservative conversion rounding, drained-tier behavior, and
final-redemption dust sweep as BLND:USDC. Active and queued shares use that same
rate and absorb changes proportionally.

Direct BLND:XLM or USDC transfers that do not use an attributed deposit entry
point MUST NOT create shares or count toward activation, loss capacity,
take-rate allocation, backfill, or emissions.

### 7.3 Withdrawals

All three backstop tiers inherit the v2 queue-for-withdrawal shape:

- Queuing creates a position with an expiry exactly 17 days after the queue
  transaction.
- A user may have at most 20 queue entries for each pool and tier.
- Expired withdrawals consume entries oldest-first.
- Dequeue operations restore active shares newest-first and MAY occur before or
  after expiry.
- Queued BLND:USDC shares remain in contract custody and continue contributing
  to the legacy emitter's raw balance comparison until the tokens are
  withdrawn.
- Queuing BLND:USDC alone does not forfeit migration-backfill weight. If the
  tokens are actually withdrawn before emitter migration succeeds, the
  withdrawn shares forfeit their proportional accumulated backfill weight.
  BLND:XLM and USDC never receive migration-backfill weight.
- Queued shares are excluded from activation value and ongoing BLND-emission
  weight immediately upon queueing.
- Queued shares remain eligible for take-rate realization while they remain in
  contract custody. A completed interest auction donates its tier-token bid
  against the tier's total shares, including active and queued shares, exactly
  once.
- Queued shares remain exposed to the tier's loss waterfall until actual
  withdrawal. Queueing does not reserve their underlying assets or protect
  their redemption value; a tier loss reduces active and queued share value
  proportionally through their common exchange rate, as in v2.
- Queueing and dequeueing do not change take-rate eligibility. While an
  interest-auction commitment is active, deposits into and actual withdrawals
  from its selected tier MUST fail so the share set that receives the donated
  bid cannot change between preparation and settlement. Queueing and
  dequeueing remain available because they do not change total shares. Other
  tiers are unaffected by this interest-only lock. Permissionless stale-auction
  deletion bounds the lock to 500 ledgers after the auction start.
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

The initial Protocol-27 pool stores bounded liability, committed-loss, and
unresolved-bad-debt maps in instance state. Liability and unresolved-bad-debt
maps are keyed by reserve: a zero amount removes its record, negative amounts
are invalid, and replacing an existing nonzero amount does not change the
count. General committed losses are presence-only records keyed by auction
identifier because a multi-asset auction has no single amount unit. The one
prepared bad-debt auction is an additional canonical singleton commitment
whose temporary lifetime exactly matches the backstop's reserved lot; its
presence contributes one committed-loss entry without duplicating its
identifier in the general map. Monetary amounts from reserves with different
units MUST NOT be combined. Borrow and repay synchronize the configured
backstop's reserve liability record.

The callback MUST return false for an address other than the pool's immutable
backstop or when any count is nonzero. Missing canonical loss records MUST fail
closed rather than being interpreted as empty. No administrator, governance
address, or mutable boolean override may make the callback return true.
An interest-auction commitment is a gain process rather than committed loss
capital and MUST NOT enter this pool-level loss callback. The backstop enforces
its narrower selected-tier deposit and withdrawal lock directly.

The withdrawal implementation MUST:

- Preserve a queue or delay sufficient to prevent capital from escaping known
  losses.
- Prevent withdrawal of capital committed to an outstanding loss process.
- Remain executable without iterating over an unbounded number of users or
  assets.

The bounded Q4W accounting and Protocol-27 pool implement this fail-closed
record and state boundary. Ordinary backstop borrow and repay create and
resolve liability records. Bad-debt auction preparation now creates the
canonical singleton commitment atomically with the backstop reservation.
Bad-debt fills atomically move scaled dToken liabilities to a healthy filler,
transfer the scaled tier-token lot, impair the pool-tier exchange rate, and
update the singleton commitment. Permissionless continuation either prepares
the next eligible strict-tier auction or, only after the backstop verifies
that all tiers are below the auction minimum, clears the residual canonical
liabilities while applying the loss to suppliers. A completed
user-liquidation auction that exhausts the liquidatee's collateral atomically
hands every residual dToken liability to the configured backstop and updates
the same canonical reserve-keyed records.

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
per eligible token.

The candidate binds one valuation-adapter address immutably at construction.
The adapter interface version is fixed by the candidate. A production adapter
MUST itself have immutable pricing dependencies and no privileged
configuration or WASM-upgrade path. For a positive LP-token amount it returns:

- underlying BLND in seven-decimal base units;
- USDC value in seven-decimal base units; and
- an inclusive `valid_until` ledger timestamp.

The candidate derives LP amounts from its own pool-attributed share ledgers,
calls the adapter separately for active and queued BLND:USDC and BLND:XLM
shares, and values accounted plain USDC exactly one-for-one. Zero LP amounts
do not require an external quote. A missing or incompatible adapter, failed
adapter invocation, negative result, or quote whose expiry precedes the
current ledger timestamp MUST fail the canonical valuation atomically.
Unattributed direct token transfers MUST NOT enter the calculation.

Canonical valuation converts active and queued shares independently through
their common pool-tier exchange rate before quoting LP token amounts or
plain-USDC value. Conservative rounding can leave conversion dust unallocated;
the final share redemption sweeps the remaining accounted assets.

A production adapter MUST still define manipulation-resistant BLND:USDC and
BLND:XLM LP valuation and its concrete oracle freshness policy before these
values can drive stored pool status. Instantaneous Comet reserve ratios and
caller-supplied values MUST NOT directly determine pool activation.

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

The implemented v3 pool stores status 6 at construction, stores its operator
immutably, and emits a status-change event after each successful transition.
The factory accepts the operator as an explicit deployment argument while the
authenticated deployer retains the deterministic salt domain. Permissionless
refresh and operator requests call the immutable backstop, reject disallowed
or invalid results, and store the returned status atomically. The pool exposes
the centralized v2-compatible action predicate enforced by the implemented
lending and user-liquidation requests. Bad-debt fills are explicit direct
entry points that remain available under every status. Tier-specific interest
auction creation, filling, and stale recovery are explicit direct entry points
that remain available under every status because they realize already accrued
credit and do not create or modify a user lending position.

### 8.2 Initial lending core

The pool factory fixes the complete pool-level lending configuration and
ordered reserve configuration when it deploys a pool. Those values are
immutable. In particular, the pool-local operator has no authority to add,
remove, enable, disable, or reconfigure reserves; change the oracle; alter
interest parameters, collateral factors, liability factors, supply caps,
minimum collateral, maximum positions, or the backstop take rate; or replace
the pool WASM.

The initial lending core implements request types 0 through 5:

| Code | Request |
| ---: | --- |
| 0 | Supply |
| 1 | Withdraw |
| 2 | Supply collateral |
| 3 | Withdraw collateral |
| 4 | Borrow |
| 5 | Repay |

Request types 6 and 9 implement user-liquidation fill and cancellation,
including the completed-auction bad-debt handoff. Tier-specific interest
auctions use their dedicated direct entry points rather than a submit request
type. Each submitted request MUST enforce the stored v2-compatible status
before changing accounting. Disabled reserves reject supply, collateral
supply, and borrow but continue to permit withdrawal and repayment.

The request owner is `from`, the account providing incoming tokens is
`spender`, and outgoing tokens are sent to `to`. `spender` MUST authorize every
submission. If `from` and `spender` differ, `from` MUST also authorize it.
There is no unauthenticated repay-on-behalf path in this initial core. Contract
addresses MAY own positions and use their ordinary Soroban authorization logic.

Reserve accounting retains the v2 bToken/dToken model and interest curve:

- Supply and collateral mint bTokens by rounding down.
- Withdrawal burns bTokens by rounding up, capped by the position balance.
- Borrow mints dTokens by rounding up.
- Partial repayment burns dTokens by rounding down.
- A repayment amount at or above current live debt burns the complete debt
  position and returns excess input to `to`.
- Interest increases dRate, assigns the immutable take-rate portion to the
  reserve's accrued backstop credit, and assigns the remainder to suppliers
  through bRate.
- All intermediate multiplication uses checked I256 fixed-point arithmetic;
  results that do not fit i128 fail the transaction.

Accrued backstop credit remains reserve-asset accounting state until a
tier-specific interest auction checkpoints and realizes it under Section 9.1.
No direct user claim or administrator distribution path exists.

Each submission MUST verify the pool's exact per-asset token-balance delta
across its transfers. Fee-on-transfer behavior, a downward rebase during the
call, or any other mismatch fails the entire transaction. This check does not
make later issuer clawback, deauthorization, or an out-of-band negative rebase
safe; production reserve-admission policy for those asset controls remains an
explicit deployment and risk decision.

Each pool supports at most 30 reserves, at most 60 effective collateral and
liability positions per account, and at most 60 requests per submission.
Reserve and user state use bounded persistent entries with explicit TTL
renewal. A submission that fails authorization, status, reserve, utilization,
health, price, cap, arithmetic, token-transfer, or position-count validation
MUST roll back all reserve, position, event, loss-record, and transfer effects.

Health checks consume an immutable oracle interface returning a positive price,
an exact decimal scale, and an inclusive validity timestamp. A quote with the
wrong decimals, a nonpositive price, or an expired validity timestamp fails
closed. Collateral value rounds down after its collateral factor; liability
value rounds up before division by its liability factor. Borrow and collateral
withdrawal require a health factor of at least 1.00001 and the immutable minimum
effective collateral value.

When the configured backstop itself borrows or repays, the lending transition
MUST update the corresponding canonical backstop-liability record atomically.
The stored value is the nonzero dToken balance in that reserve's own units; the
withdrawal callback uses only record presence and never sums values across
reserves.

### 8.3 User-liquidation auctions

The user-liquidation portion of the auction engine is v2-compatible. Auction
creation is permissionless and accepts a liquidatee, unique
reserve-asset lists for the dToken bid and bToken collateral lot, and a
liquidation percentage from 1 through 100. The liquidatee cannot be the pool or
its configured backstop and can have at most one active user-liquidation
auction. The total bid-plus-lot asset count MUST NOT exceed the pool's immutable
maximum-position limit.

Creation MUST use current accrued reserve rates and current valid oracle quotes.
The liquidatee must be economically unhealthy, every requested bid asset must
contain a liability position, and every requested lot asset must contain a
collateral position. Each asset's first validated oracle quote MUST be cached
and reused throughout one creation or fill submission. The inherited v2 quote
calculation:

- estimates the liquidation incentive from the selected positions' average
  collateral and inverse liability factors;
- rounds dToken bid shares and bToken lot shares up when constructing the base
  quote;
- constrains a non-full liquidation's estimated post-liquidation health factor
  to the inclusive 1.03-through-1.15 repair band; and
- treats an all-position liquidation above 95% as a checked 100% liquidation.

The auction begins on the ledger after creation. During its first 200 ledgers,
the bid remains at 100% while the lot increases by 0.5% per ledger from zero to
100%. During ledgers 201 through 400, the lot remains at 100% while the bid
decreases by 0.5% per ledger to zero. A requested partial-fill percentage
rounds the base bid up and base lot down before applying those ledger
modifiers. Base amounts not selected by the partial fill remain in the auction;
the time modifier affects only the positions transferred by that fill.

Request type 6 fills a user-liquidation auction by transferring the scaled
bToken collateral and dToken liability shares from the liquidatee to the
filler. It does not transfer underlying tokens or change aggregate reserve
bToken/dToken supply. The filler MUST NOT be the liquidatee and MUST finish the
entire submission within the ordinary maximum-position, minimum-collateral, and
health constraints. A configured-backstop filler MUST atomically update its
canonical reserve-liability records.

An account with an active user-liquidation auction cannot complete an ordinary
submission. Request type 9 may remove its own auction only when the stored pool
status permits auction cancellation and the resulting position is healthy.
Auction fills remain available under On-Ice and Frozen statuses. Independently,
any caller may delete an auction once 500 ledgers have elapsed from its start.
Auction entries use temporary storage with a 45-day renewal threshold and a
46-day bump. Creation and partial-fill writes apply that bounded TTL policy.

Matching v2, completion of a user-liquidation auction MUST check whether the
liquidatee has nonzero liabilities and no collateral. If so, the same
transaction MUST:

- move every residual dToken share from the liquidatee to the configured
  backstop without changing reserve dToken supply;
- accumulate each amount with any liability the backstop already holds in the
  same reserve;
- update every affected canonical reserve-keyed backstop-liability record;
- clear the liquidatee's residual liabilities; and
- emit one handoff event per affected reserve.

The handoff MUST NOT prepare, resize, fill, or settle a bad-debt auction in the
same submission. If an earlier bad-debt auction is active, its committed bid
and tier lot remain unchanged; the newly assigned liabilities remain in
canonical state and are picked up by a later permissionless continuation after
the active auction is cleared. This permits new liquidations to complete
without double-counting the earlier auction's bounded snapshot.

An incomplete auction fill never performs the handoff. A fill that exhausts
collateral while its auction remains incomplete MUST fail atomically. A fill
that leaves collateral and liabilities but reduces the liquidatee's effective
health factor MUST also fail atomically. These boundaries prevent a partial
fill or an ineligible completed fill from manufacturing unhandled bad debt.
The ordinary maximum-position, minimum-collateral, and health constraints
continue to apply to the filler for the complete submission. The handoff loop
is bounded by the immutable 30-reserve maximum and MUST batch its canonical
liability-map mutation into one load and store. The maximum-reserve optimized
WASM fill MUST remain within Protocol-27 mainnet invocation limits.

## 9. Loss waterfall

The candidate uses the following strict first-loss order:

1. BLND:USDC LP.
2. BLND:XLM LP.
3. Plain USDC.
4. Pool suppliers for any residual loss after every backstop tier is
   operationally exhausted under the minimum-value policy below.

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

After its next-ledger start, any authenticated account other than the pool or
backstop may fill from 1% through 100% of the current bad-debt auction. The
filler MUST NOT have an active user-liquidation auction and, after assuming the
filled dToken shares, MUST satisfy the pool's ordinary maximum-position,
minimum-collateral, and health requirements. Bad-debt fills remain available
under On-Ice and Frozen statuses, matching the v2 action matrix.

Bad-debt auctions use the same 400-ledger price curve and asymmetric rounding
as user-liquidation auctions. The requested percentage first selects a base
dToken bid with ceiling rounding and a base tier-token lot with floor rounding.
The current ledger modifier then scales only the dToken shares and tier tokens
actually transferred. The unselected base bid and lot remain in the auction.
The selected base amounts are removed from the auction even when the
time-scaled transfer is smaller.

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

For each affected reserve, the pool first accrues interest and then applies
Blend v2's default arithmetic:

```text
default_asset_amount = ceil(d_tokens * d_rate / 10^12)
b_rate_loss = ceil(default_asset_amount * 10^12 / b_supply)
new_b_rate = max(previous_b_rate - b_rate_loss, 0)
```

The default removes the configured backstop's full dToken position, reduces
reserve `d_supply` by the same shares, leaves `b_supply` unchanged, and emits
the dToken, underlying-asset, and before/after bRate amounts. A missing
supplier base, arithmetic failure, or inconsistent reserve state fails the
entire call. After every reserve succeeds, the canonical liability map is
cleared and backstop withdrawal eligibility is recomputed from the remaining
loss records. No token transfer occurs during supplier default. If the loss
exceeds supplier value, bRate intentionally saturates at zero as it does in
v2.

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

A tier with zero eligible value contributes zero to the numerator and
denominator. The formula therefore renormalizes across only populated,
eligible tiers.

For take-rate purposes, eligible value is derived from every accounted
pool-tier asset still held in contract custody, whether its corresponding
shares are active or queued for withdrawal. The canonical weighting quote MUST
use full accounted assets rather than subtract assets separately committed to
a bad-debt auction. A bad-debt commitment and an interest commitment may
coexist, but their transfers MUST remain independently accounted and atomic.

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

Interest fills inherit v2's 400-ledger auction curve. During the first 200
ledgers the bid is full while the lot ramps from zero to full; during the next
200 ledgers the lot is full while the bid declines from full to zero.
Percentage selection rounds the base bid up and base lot down. A partial fill
removes the selected base amounts from the auction, transfers only their
time-scaled amounts, releases any base-lot discount from the reservation, and
leaves the pool auction and backstop commitment synchronized. Only reserve
assets actually transferred from the pool are subtracted from both the
persistent pending tier amount and accrued backstop credit.

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
separate migration award.

The candidate MUST store the BLND token address immutably at construction.
After migration activation, any caller MAY invoke the one-time backfill-funding
entry point. The candidate MUST call the legacy emitter's initial-drop facility
with exactly one recipient, the candidate itself, and exactly the scheduled
amount \(B\). It MUST verify that its configured BLND balance increased by
exactly \(B\) before committing the funded reserve. A failed emitter call,
unexpected token-balance delta, missing eligible weight, or repeat funding
attempt MUST fail atomically.

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

### 10.1 Ongoing backstop-depositor emissions

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

This section's pool and user weighting applies only to
\(E_{\mathrm{backstop}}\). The 30% \(E_{\mathrm{pool}}\) tranche remains
segregated for pool supply and borrow positions. Its reserve-level and
supply-versus-borrow allocation requires a separately specified pool-side
accounting rule and MUST NOT be claimable through the backstop-depositor path.

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

- Production LP price sources, averaging, staleness, and failure policy.
- Production lending-reserve admission policy for issuer controls, clawback,
  deauthorization, rebasing, and other nonstandard token behavior.
- Post-migration designated token.
- Pool-side allocation of the 30% ongoing BLND tranche among supply and borrow
  positions.
- Ongoing-emission treatment of capital committed to a pending loss auction.
- Protocol-fee existence and incidence.
- Whether existing v2 pool positions remain isolated or receive an explicit
  position-migration mechanism.

## 17. Initial implementation milestones

1. Preserve the v2 incumbent and migration harness.
2. Add an isolated v3 backstop crate with emitter-compatible entry points.
3. Implement continuous BLND:USDC tier deposits and migration-backfill
   checkpoints.
4. Add independent pool-attributed BLND:XLM and plain-USDC tier ledgers with
   bounded Q4W accounting; neither tier receives migration backfill.
5. Implement a canonical, testable activation-policy interface.
6. Implement bounded withdrawal queues for all three tiers and their
   fail-closed pool callback, then connect that callback to canonical loss
   state without forced conversion.
7. Complete native, WASM, local-network, and testnet migration exercises. The
   local-network lane uses exact production queue constants, so its
   Quickstart runner is resumable across the wall-clock queue rather than
   deploying shortened-delay evidence. A transaction-backed keepalive
   compensates for Quickstart's faster ledger cadence without changing queue
   timestamps or economic state.
8. Implement single-tier bad-debt auction filling atomically across pool
   liabilities, filler health, backstop commitments, tier-token transfer, and
   pool-tier exchange-rate impairment; then add permissionless next-tier
   sequencing and v2-style supplier residual settlement.
9. Implement bounded tier-specific interest auctions that allocate accrued
   reserve credit with the canonical `5:5:4` weighting, cycle across qualifying
   tiers, donate the selected tier token without minting shares, and support
   atomic partial fills and permissionless stale recovery.
10. Implement the immutable ongoing-emission 70/30 split and canonical current
    Comet-composition reads for BLND:USDC and BLND:XLM, then connect them to
    bounded pool/user accrual after the 30% pool-side allocation and pending-loss
    eligibility policies are resolved.

Milestones 1 through 5, the bounded-Q4W and canonical-loss-state-boundary
portions of milestone 6, the reproducible deployment runner for the local
portion of milestone 7, and milestones 8 and 9 are
represented by the frozen v2 baseline, isolated Protocol-27 backstop crate,
cross-version legacy-emitter migration harness, the candidate's
pool-attributed continuous BLND:USDC shares, time-weighted migration
checkpoints, independent pool-attributed BLND:XLM and plain-USDC share ledgers,
canonical asset-neutral 12,500-USDC entry and 10,000-USDC maintenance
activation-policy arithmetic, an immutable versioned valuation-adapter
boundary that derives active and queued values from canonical pool-tier
asset/share state,
pool-local checked asset/share exchange rates that proportionally expose active
and queued shares to tier impairment, and 17-day bounded withdrawal queues for
all three tiers, exact one-time
backfill funding and owner-authorized constant-time claims, an immutable
Protocol-27 pool factory, immutable per-pool operator and stored status,
canonical permissionless and operator status transitions, the v2-compatible
action predicate, immutable lending and reserve configuration, the six
non-auction v2 request types, v2-compatible reserve interest and share
accounting, fail-closed oracle-valued health checks, v2-compatible
user-liquidation creation and fills with atomic completed-auction handoff of
collateral-free residual liabilities to the configured backstop,
pool-authorized preparation of one single-tier,
200-USDC-minimum bad-debt lot in strict waterfall order, atomic partial and
complete single-tier fills using the inherited v2 curve, direct tier-token
transfer with proportional active-and-queued exchange-rate impairment,
permissionless bounded continuation that reuses each eligible tier before
advancing, v2-style residual supplier loss after a verified no-tier quote, and
a fail-closed pool callback derived from bounded canonical loss records,
pool-and-reserve-isolated pending interest credit and carry, canonical
`5:5:4` credit allocation, cyclic single-tier interest-auction selection,
120%-value tier-token bids, v2-curve partial fills, share-free bid donation,
and permissionless stale-lot recovery.
The local runner deploys independently issued assets, a
coherent
SDK-22 incumbent stack, and the SDK-27 candidate stack under testnet resource
limits while recording transaction and cost evidence. A completed wall-clock
local run, the public-testnet exercise, a production LP valuation source for
non-emission policy, and ongoing BLND-emission accrual remain intentionally
unimplemented. Milestone 10's policy arithmetic and canonical spot-composition
read boundary are implemented; emitter funding, accumulators, and claims remain
blocked on the open decisions stated above.

New economic features should be introduced only after the corresponding open
decision is resolved in this specification.
