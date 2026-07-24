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

This document does not claim that unresolved economic parameters or governance
choices have been approved.

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
- Legacy backstop withdrawal or opt-in conversion.
- Deployment and testing of candidate pools and supporting contracts.

### 2.2 Out of scope for the first milestone

- Mainnet deployment.
- A final governance or upgradeability system.
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
- Accept and account for migration deposits of the incumbent designated token.
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
- Legacy users MAY withdraw their original asset or explicitly opt into a
  supported v3 position.
- Unclaimed legacy positions MUST remain fully accounted for and MUST NOT be
  treated as unencumbered protocol capital.

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

The candidate MAY additionally configure up to two optional custom assets,
identified as Custom A and Custom B. An unconfigured custom tier has zero
balance, zero activation value, zero loss capacity, and zero take-rate
allocation. A custom asset MUST satisfy every configuration and safety
requirement in this section before any of those roles become available.

Approval for activation does not by itself make a position eligible. Capital
MUST also be allocated to the evaluated pool, committed to the defined loss
waterfall, and subject to the applicable withdrawal rules.

### 7.2 Position accounting

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

### 7.2.1 Initial single-asset prefunding lane

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

Withdrawal rules MUST:

- Preserve a queue or delay sufficient to prevent capital from escaping known
  losses.
- Define whether and how queued capital earns emissions.
- Define whether and how queued capital counts toward activation.
- Prevent withdrawal of capital committed to an outstanding loss process.
- Remain executable without iterating over an unbounded number of users or
  assets.

## 8. Pool activation

The activation policy is independent of the emitter's designated-token
comparison.

A pool's activation score is the sum of the eligible values of its three
required and up to two configured custom assets, denominated in 7-decimal USDC
units:

\[
E_p =
V_{p,\mathrm{BLND:XLM}} +
V_{p,\mathrm{BLND:USDC}} +
V_{p,\mathrm{USDC}} +
V_{p,\mathrm{CustomA}} +
V_{p,\mathrm{CustomB}}
\]

where:

- \(E_p\) is pool \(p\)'s eligible activation value.
- \(V_{p,i}\) is the verified USDC value of eligible asset \(i\) allocated to
  pool \(p\).

Every eligible USDC of value has equal activation weight. The calculation has
no asset-specific activation haircut, concentration cap, or minimum
BLND-bearing allocation. A pool therefore MAY satisfy the threshold with any
combination of the required and configured optional assets, including one
eligible asset alone.

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

Activation status MUST be refreshable permissionlessly unless a later
governance specification explicitly defines another safe mechanism.

## 9. Loss waterfall

The candidate uses the following strict first-loss order:

1. BLND:USDC LP.
2. BLND:XLM LP.
3. Plain USDC.
4. Optional Custom A.
5. Optional Custom B.
6. Pool suppliers for any residual loss after all eligible backstop capital is
   exhausted.

Each backstop asset occupies its own tier. A loss MUST exhaust the eligible,
uncommitted capital in a tier before consuming capital from the next tier.
Activation weighting is independent of this ordering: one USDC of verified
eligible value continues to contribute one USDC to the activation score
regardless of tier. An unconfigured optional tier contributes zero.

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

The five backstop tiers have fixed take-rate weights in waterfall order:

| Tier | Asset | Weight |
| ---: | --- | ---: |
| 1 | BLND:USDC LP | 2.5 |
| 2 | BLND:XLM LP | 2.5 |
| 3 | Plain USDC | 2 |
| 4 | Optional Custom A | 2 |
| 5 | Optional Custom B | 1 |

The contract represents this exact relative ratio as the integers
`5:5:4:4:2`; no floating-point arithmetic is used.

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

An unconfigured optional tier or a tier with zero reward-eligible value
contributes zero to the numerator and denominator. The formula therefore
renormalizes across only populated, reward-eligible tiers. Within each tier,
allocation MUST be proportional to reward-eligible depositor shares.

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

Backfill eligibility MUST be proportional to each surviving migration
position's deposited LP amount multiplied by its holding duration. Only shares
still deposited when migration succeeds are eligible. A pre-activation
withdrawal MUST forfeit the withdrawn shares' complete accumulated weight, and
that weight MUST NOT remain in the denominator. A partial withdrawal MUST
forfeit only the withdrawn shares' proportional weight. Re-deposited shares
begin a new holding period.

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
iterate over all depositors. The funding transaction, claim authority, and
rounding-dust destination remain separate implementation decisions.

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

Plain USDC, Custom A, and Custom B contribute zero BLND-emission weight. The
pool allocation is:

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

The accounting MUST define treatment of queued withdrawals, capital committed
to losses, and activation-status transitions. A position MUST NOT earn
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

## 13. Authorization and administration

Every privileged operation MUST identify its authorizing principal and event.
Administrative powers MUST be minimized and separately documented.

The final system must specify:

- Whether contract WASM is upgradeable.
- Who may change asset configurations and risk parameters.
- Applicable timelocks and bounds.
- Emergency pause powers and their scope.
- How authority can be revoked.

Until that specification exists, new unrestricted administrative setters MUST
NOT be added.

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
- Treatment of queued capital for activation and emissions.
- Treatment of queued capital for take-rate allocation.
- Exact take-rate accrual integration and distribution cadence.
- Post-migration designated token.
- Top-level BLND split between backstop depositors and pool supply/borrow
  positions.
- Legacy-position withdrawal and optional conversion interface.
- Initial BLND drop recipient policy and lockups.
- Protocol-fee existence and incidence.
- Administrative, pause, and WASM-upgrade authority.
- Whether existing v2 pool positions remain isolated or receive an explicit
  position-migration mechanism.

## 17. Initial implementation milestones

1. Preserve the v2 incumbent and migration harness.
2. Add an isolated v3 backstop crate with emitter-compatible entry points.
3. Implement legacy BLND:USDC migration deposits and checkpoints.
4. Add one independently configured, prefundable post-migration backstop
   asset without activation, loss, backfill, or emission eligibility.
5. Implement a canonical, testable activation-policy interface.
6. Implement legacy withdrawal without forced conversion.
7. Complete native, WASM, local-network, and testnet migration exercises.

Milestones 1 through 5 are represented by the frozen v2 baseline, isolated
Protocol-27 backstop crate, cross-version legacy-emitter migration harness, and
the candidate's pool-attributed legacy migration deposits, time-weighted
checkpoints, isolated single-asset prefunding lane, and canonical asset-neutral
12,500-USDC entry and 10,000-USDC maintenance activation-policy arithmetic. The
production valuation adapter, pool-status integration, and BLND funding and
claims remain intentionally unimplemented.

New economic features should be introduced only after the corresponding open
decision is resolved in this specification.
