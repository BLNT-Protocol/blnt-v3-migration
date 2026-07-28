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

Every frozen v2 requirement is inherited unless this document explicitly
classifies a v3 requirement as:

- **Added:** behavior with no v2 equivalent.
- **Replaced:** behavior that supersedes an identified v2 rule.
- **Extended:** inherited behavior generalized only as stated here.
- **Safety fix:** an approved corrective deviation.
- **Approved exception:** an approved economic or administrative deviation.

A v3 difference MUST be required by:

- the three-tier backstop;
- legacy-emitter migration compatibility;
- Protocol 27 compatibility; or
- an approved conservation or safety fix; or
- a separately approved exception recorded in this specification.

Integer carry-forward at the same pool, reserve, tier, or user accounting
scope is an approved conservation fix. The 12,500-USDC entry / 10,000-USDC
maintenance hysteresis is the sole currently approved economic exception to
strict v2 parity. Except for the differences explicitly stated in this
specification, v3 MUST preserve v2 economic and administrative behavior.
Implementation details MUST NOT introduce additional fees, allocations,
privileged roles, governance, or upgrade paths.

Omission means inheritance. Contextual references to v2 do not redefine the
baseline. [V2_PARITY.md](V2_PARITY.md) records the cross-reference and
implementation status of the major approved differences.

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

Existing v2 supplier, borrower, and backstop positions remain governed
exclusively by the deployed v2 code. Emitter migration redirects future BLND
distributions without moving or modifying those positions.
Immediately before the recipient changes, the emitter mints its final
time-accrued BLND distribution directly to the incumbent. Subsequent allocation
and claims from BLND held by v2 remain governed by the existing v2 emission
rules.

## 6. Candidate lifecycle — **Added**

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
- The configured BLND:XLM Comet LP is the emitter-designated token controlling
  qualification for the next protocol migration.
- Existing candidate BLND:USDC tier positions continue in that same first-loss
  tier without conversion, an asset swap, or a second opt-in.
- Incumbent v2 positions remain in their original contracts. Migration MUST
  NOT move, convert, close, or import them into v3.
- While the candidate remains registered, the incumbent v2 backstop receives
  no new legacy-emitter distributions after the swap. BLND already held,
  allocated, or accrued by v2 remains governed by the existing v2 accounting
  and claim rules.
- Incumbent v2 lending and position management MAY continue independently of
  v3, subject only to the immutable v2 rules and available market liquidity.

## 7. Multi-asset backstop — **Replaced and extended**

This section replaces the single accepted token and generalizes the
pool-share and Q4W accounting in `V2-BACKSTOP-001` through
`V2-BACKSTOP-003` only as stated below.

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
Every factory-deployed pool MUST expose its signature and fail-closed
semantics. The immutable factory deploys only the approved pool WASM, and this
candidate exposes no pool-WASM replacement path. Any future candidate that
proposes pool upgrades requires a separate specification and MUST preserve this
interface.

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
The adapter interface version is fixed by the candidate. The interface also
returns an immutable binding containing BLND, plain USDC, BLND:USDC, and
BLND:XLM addresses. Candidate construction MUST reject an adapter unless all
four addresses exactly match the candidate constructor arguments. A production
adapter MUST itself have immutable pricing dependencies and no privileged
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

The production adapter uses one immutable SEP-40 feed whose base asset is the
configured Stellar USDC asset. The feed MUST publish BLND and XLM prices in
that base. BLND, USDC, XLM, both Comet LP tokens, the feed, and every averaging
parameter are fixed at adapter construction. The adapter exposes no
administrator, setter, alternate price path, or WASM-upgrade function.

Let \(D_o\) be the oracle's immutable precision, \(P_b\) and \(P_q\) the
validated arithmetic-mean prices of BLND and the paired asset, and
\(R_b\), \(R_q\), and \(S\) the Comet's current BLND reserve, paired reserve,
and LP-token supply. All token quantities use seven decimals. Reserve values
are:

\[
U_b = \left\lfloor \frac{R_bP_b}{10^{D_o}} \right\rfloor
\]

\[
U_q =
\begin{cases}
R_q & \text{for BLND:USDC}\\
\left\lfloor \frac{R_qP_q}{10^{D_o}} \right\rfloor
& \text{for BLND:XLM}
\end{cases}
\]

Because the accepted Comets have immutable 80% BLND and 20% paired-asset
weights, each side independently implies a total LP value:

\[
T_b = \left\lfloor U_b\frac{10^7}{8{,}000{,}000}\right\rfloor,
\qquad
T_q = \left\lfloor U_q\frac{10^7}{2{,}000{,}000}\right\rfloor
\]

For LP amount \(A\), the adapter returns:

\[
V(A) =
\left\lfloor
A\frac{\min(T_b,T_q)}{S}
\right\rfloor
\]

and:

\[
B(A) = \left\lfloor A\frac{R_b}{S}\right\rfloor
\]

as `usdc_value` and `underlying_blnd`, respectively. Taking the lesser
reserve-implied total makes a one-sided donation or swap imbalance unable to
increase the verified LP value. A manipulation can reduce the quote, which is
the conservative failure direction. A proportionate liquidity contribution
increases reserve value together with LP supply and therefore does not inflate
the value per LP share.

The production TWAP accepts 2 through 25 uniform SEP-40 records. The immutable
record count and oracle resolution MUST define a window from 30 minutes
through 24 hours. Maximum price age MUST be at least one oracle resolution and
no more than one hour. Every returned record MUST have a positive price, a
nonfuture timestamp aligned to the immutable resolution, and a unique tick;
the records, in any order, MUST span exactly
`resolution * (record_count - 1)`. The arithmetic mean rounds down. A quote is
valid inclusively through the latest accepted tick plus the configured maximum
age. BLND:XLM uses the earlier expiry of the BLND and XLM averages.

Construction MUST verify the USDC oracle base, BLND and XLM feed membership,
seven-decimal token interfaces, exact two-token Comet membership, and exact
80:20 normalized weights. Every quote rechecks the oracle base, precision, and
resolution and the applicable Comet weights. A nonpositive or excessive LP
amount, missing or malformed history, future or stale tick, changed oracle
identity, nonpositive supply or reserve, unsupported token, or arithmetic
failure MUST fail closed. A positive LP amount whose conservative value or
underlying BLND rounds below one seven-decimal base unit returns zero for that
field; valuation dust MUST NOT make the entire pool unavailable.

Instantaneous Comet composition remains canonical only for
`underlying_blnd`. It does not by itself set activation value. This preserves
the separately approved spot-composition emission policy while activation,
take-rate, auction, and supplier-loss accounting use the conservative
externally priced value. See
[VALUATION_ADAPTER.md](VALUATION_ADAPTER.md) for deployment and security
constraints.

The implemented activation policy MUST:

- Count only capital allocated to the evaluated pool.
- Count only assets available to the defined loss waterfall.
- Avoid double-counting common underlying assets across wrappers or claims.
- Specify treatment of queued withdrawals and committed loss capital.
- Fail safely when required prices are stale or unavailable.
- Resist same-transaction price, reserve, and LP-supply manipulation.
- Use one canonical activation valuation for pool-status and reward-zone
  threshold decisions.
- Define behavior exactly at the threshold.

Activation status MUST be refreshable permissionlessly.

### 8.1 Pool-status valuation — **Extended and approved exception**

V3 inherits the numeric status codes, action matrix, Q4W boundaries,
permissionless refresh shape, transferable admin, pool parameters, and reserve
configuration rules in `V2-POOL-ADMIN-001`, `V2-POOL-CONFIG-001`,
`V2-POOL-STATUS-001`, and `V2-POOL-STATUS-002`.

V3 replaces only the activation threshold and Q4W measurement basis. Because
the three tiers have different token and share units, pool \(p\)'s Q4W
percentage uses verified USDC value:

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

A permissionless refresh MUST apply the inherited inclusive Q4W boundaries
with the v3 activation hysteresis:

- For status 0, failure of the 10,000-USDC maintenance threshold or
  \(Q_p \ge 50\%\) moves the pool to status 3; otherwise it remains status 0.
- For status 2, \(Q_p \ge 75\%\) moves the pool to status 5; otherwise it
  remains status 2.
- For statuses 1, 3, and 5, \(Q_p \ge 60\%\) produces status 5;
  \(Q_p \ge 30\%\) or failure of the applicable activation threshold
  produces status 3; otherwise the result is status 1.
- Statuses 4 and 6 reject permissionless refresh.

Statuses 0 and 1 use the 10,000-USDC maintenance threshold. Statuses 2 through
6 are inactive and MUST meet the 12,500-USDC entry threshold before returning
to an active status.

The inherited pool admin MAY request:

- Status 0 when the applicable activation threshold is met and
  \(Q_p < 50\%\).
- Status 2 or status 3 when \(Q_p < 75\%\).
- Status 4 without an additional Q4W condition.

Status 0 retains v2's bounded risk-on override between the ordinary 30% and
admin 50% Q4W boundaries. Statuses 2 and 4 remain risk-off controls.

The admin MUST NOT change the v3 activation thresholds or valuation,
three-tier configuration, `5:5:4` take-rate weights, waterfall, withdrawal
eligibility, migration rules, immutable 70/30 split, contract WASM, or any
protocol-wide policy.

Queueing shares does not itself change stored pool status. Status refresh,
admin status requests, and action enforcement MUST consume the same canonical
status result. Reward-zone entry and maintenance MUST consume the same
canonical activation valuation, but stored pool status is not an additional
reward-zone gate.

The SDK-27 pool persists the inherited status and admin lifecycle and calls the
immutable backstop for canonical v3 valuation. Bad-debt and tier-specific
interest-auction direct entry points remain available under every status
because they settle existing loss or accrued credit rather than create a new
user lending position.

### 8.2 Pool integration — **Inherited with safety extensions**

V3 inherits the lending request model, pool administration, reserve
configuration, bToken/dToken rounding, interest curve, health calculation,
limits, disabled-reserve behavior, and atomic rollback requirements in
`V2-POOL-ADMIN-001`, `V2-POOL-CONFIG-001`, and `V2-LENDING-001` through
`V2-LENDING-005`.

The v3 pool implements inherited request types 0 through 5. User-liquidation
fill and cancellation retain codes 6 and 9. Tier-specific interest and
bad-debt processes use the dedicated direct entry points defined by this spec
rather than changing inherited position semantics. V3 introduces no
unauthenticated repay-on-behalf path.

Only these integration and safety extensions apply:

- Both inherited submission entry points are available. `submit` performs
  direct transfers, while `submit_with_allowance` nets each asset's aggregate
  incoming and outgoing transfer actions and collects a positive net through
  SEP-41 `transfer_from`. Both accept an authenticated empty request list as a
  no-op.
- The inherited zero-fee `flash_loan` entry point creates its borrow before the
  request set, invokes `exec_op(from, asset, amount, 0)`, and settles the
  requests through the allowance path. The flash borrow may be repaid in that
  set or remain only if the final position is healthy. Its event preserves the
  v2 topics `["flash_loan", asset, from, contract]` and vector data
  `[tokens_out, d_tokens_minted]`.
- A submission contains at most 60 requests. This Protocol-27 resource-safety
  bound applies equally to direct, allowance, and flash-loan request sets.
- The immutable oracle interface returns a positive price, exact decimal scale,
  and inclusive validity timestamp. Wrong decimals, nonpositive prices, or an
  expired timestamp fail closed.
- Collateral value still rounds down and liability value rounds up under the
  inherited factors. Borrow and collateral withdrawal require a health factor
  of at least 1.00001 and the immutable minimum effective collateral value.
- Intermediate fixed-point multiplication uses checked I256 arithmetic.
  Results that do not fit i128 fail the transaction.
- Each submission verifies exact per-asset pool token-balance deltas. A
  fee-on-transfer token, downward rebase during the call, or other mismatch
  fails atomically. This does not make later issuer clawback,
  deauthorization, or out-of-band negative rebasing safe.
- Accrued `backstop_credit` remains isolated by pool and reserve until a
  tier-specific v3 interest auction realizes it. No direct user or admin claim
  is added.
- When the configured backstop borrows or repays, the transition atomically
  synchronizes the reserve-keyed canonical backstop-liability record used by
  the withdrawal gate. Values remain in their reserve units and are never
  summed across assets.
- Reserve and user state use bounded persistent entries with explicit
  Protocol-27 TTL renewal.

A failed authorization, status, reserve, utilization, health, price, cap,
arithmetic, transfer-delta, or position-count check MUST roll back every pool,
backstop-record, event, position, reserve, and token effect.

### 8.3 User-liquidation integration — **Inherited with safety extensions**

V3 inherits `V2-AUCTION-001` and `V2-AUCTION-002`, including creation,
health, incentive, repair-band, percentage, curve, fill, cancellation, stale
deletion, status, filler-health, and aggregate-share-supply behavior.

The v3 delta affects only the completed-auction bad-debt handoff. When a
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
activation or the absolute recovery-horizon end. For an eligible tranche, the
effective deposit time MUST be the actual deposit time. A tranche deposited at
or after the currently verified unlock has zero weight for that queue and is
omitted from this sum. Prefunding used to establish the candidate's qualifying
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

The candidate MUST preserve v2's bounded reward-zone shape:

- The reward zone contains at most 30 registered pools.
- Any caller may add a nonmember that meets the 12,500-USDC entry threshold and
  has strictly positive eligible underlying BLND.
- Any caller may remove a member that falls below the 10,000-USDC maintenance
  threshold.
- Any caller may remove a member with zero eligible underlying BLND regardless
  of its remaining activation value and without a distribution checkpoint.
  Zero-weight removal is allocation-neutral and is the fail-open liveness path
  if all of a member's BLND-bearing capital becomes queued, withdrawn, or
  impaired.
- If the zone is full, an addition must name one existing member to remove and
  the entrant must have strictly greater eligible underlying-BLND weight than
  that member.
- A pool cannot appear twice.
- Before migration activation, any number of qualified pools up to the
  30-member bound may enter, and a positive-weight member below maintenance may
  be removed, without a distribution checkpoint. No ongoing candidate
  emissions can accrue during that phase, so these edits cannot reassign an
  unresolved interval.
- After migration activation, reward-zone edits after the first entry require
  a distribution checkpoint within the preceding hour, matching v2's
  protection against retroactive disruption, except for the
  allocation-neutral removal of a zero-weight member. This separately
  inherited rule means any post-activation weight-bearing membership edit can
  affect an unresolved interval of up to one hour.

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

The v3 delta begins after reward-zone allocation
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

The queue mechanism remains inherited from `V2-EMITTER-002`; this section
replaces only the next designated-token choice and its candidate-side binding.

This candidate immutably selects the configured BLND:XLM Comet LP as the next
designated token. The legacy emitter only requires a compatible token balance
interface, but the candidate additionally binds this address as its BLND:XLM
backstop tier and verifies the queued emitter token against that binding.

Before queuing BLND:XLM, the candidate design MUST verify:

- It implements the expected token interface.
- The candidate can custody it directly.
- Future candidates can acquire and transfer it.
- A raw-balance comparison is economically and operationally acceptable.
- Decimal conventions cannot make the next migration unintentionally trivial
  or impossible.

Selecting the designated token is separate from choosing which assets count
toward pool activation. A future candidate MAY propose another LP token or a
plain SEP-41 asset, but that choice requires its own specification and must not
retroactively change this candidate.

## 12. Authorization, immutability, and replacement — **Added constraints**

These constraints extend `V2-IMMUTABILITY-001`, `V2-POOL-ADMIN-001`, and
`V2-AUTH-001` to the candidate's new coordinator and three-tier contracts.

The v3 candidate MUST NOT introduce protocol governance, multisig control,
protocol-wide administrator-controlled recovery, a protocol-wide emergency
override, or an independent WASM upgrade authority. A transferable v2-style
pool admin is a pool-local operating role and is not a protocol replacement
mechanism.

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

The continuing pool-local authority is the explicitly configured transferable
pool admin described in Section 8.1. It includes v2-compatible configuration,
reserve-queue, emission-weight, Admin Active, Admin On-Ice, and Admin Frozen
controls. It cannot alter the candidate backstop, factory, migration policy,
activation policy, tier rules, loss waterfall, immutable 70/30 split, or
contract WASM and cannot act as a protocol-upgrade path.

User authorization remains required for custody-changing actions involving
that user's assets or shares. This is distinct from protocol governance and
MUST NOT create authority to alter global policy or bypass migration
invariants.

No recovery path may override the emitter's queue, balance comparison, or
unlock time. Bounded permissionless recovery may only observe, validate, or
synchronize state already established by the legacy emitter.

## 13. Numeric and resource safety — **Protocol-27 extensions**

These requirements extend `V2-SAFETY-001` and `V2-SAFETY-002` for the
Protocol-27 runtime and the candidate's longer migration horizon.

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

## 15. Settled policy and remaining production decisions

The candidate has settled the following protocol policies:

- Production LP valuation uses the immutable SEP-40 USDC-base TWAP,
  staleness, fail-closed, and conservative Comet policy defined in Section 8.
- BLND:XLM Comet LP is the immutable next emitter-designated token.
- The candidate implements no protocol or treasury fee.
- Incumbent v2 contracts and their supplier, borrower, and backstop positions
  remain isolated and unchanged. The emitter swap ends new v2 BLND
  distributions but performs no position migration.

The following production-readiness decisions remain:

- Selection and qualification of a concrete production SEP-40 provider whose
  deployed feeds and exact WASM satisfy the implemented valuation policy.
- Production lending-reserve admission policy for issuer controls, clawback,
  deauthorization, rebasing, and other nonstandard token behavior.

Any future opt-in mechanism for moving a user from v2 to v3 is a separate
product and protocol proposal. It is not part of this candidate and MUST NOT
be inferred from the emitter migration.
