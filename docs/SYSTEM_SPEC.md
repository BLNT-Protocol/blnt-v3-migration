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
- The candidate MUST continue validating its qualifying balance.
- A loss of the strictly-greater balance MUST prevent finalization.
- Accounting changes MUST NOT allow the same capital to support incompatible
  withdrawal and migration claims.
- Tests MUST cover balance changes immediately before finalization.

### 6.3 Finalization

Finalization MUST:

- Verify that the emitter now recognizes the candidate.
- Use the emitter finalization timestamp as the unambiguous cutoff between
  pre-migration backfill and ongoing candidate emissions.
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

Examples under consideration include BLND:USDC LP, BLND:XLM LP, and plain
USDC. Their inclusion is not yet approved by this draft.

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

A candidate MAY combine eligible value from multiple approved assets. A
general form is:

\[
E_p = \sum_i h_i \cdot \min(V_{p,i}, C_{p,i})
\]

where:

- \(E_p\) is pool \(p\)'s eligible activation value.
- \(V_{p,i}\) is the eligible value of asset \(i\) allocated to the pool.
- \(h_i\) is its activation haircut.
- \(C_{p,i}\) is its concentration cap.

This formula is illustrative; the final valuation units, weights, thresholds,
and minimum junior-capital requirement are open decisions.

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

The candidate MAY support one or more tiers. Multiple assets MAY share a tier,
including a co-junior tier.

For every tier, the implementation MUST define:

- Priority relative to other tiers.
- How losses are valued.
- How losses are divided among assets in the same tier.
- Rounding and residual-loss behavior.
- The liquidation path for each asset.
- Behavior when an oracle or market is unavailable.
- When supplier balances absorb losses after backstop exhaustion.

Emission weights do not need to match loss weights. Any difference MUST be
explicit and tested.

No asset may count toward loss capacity unless the protocol can actually seize
or dispose of it under the specified loss process.

## 10. BLND emissions and backfill

The emitter supplies BLND to the top-level candidate; the candidate determines
the internal allocation.

The candidate MUST distinguish:

- Pre-migration backfill.
- Ongoing backstop-depositor emissions.
- Pool supply and borrow emissions.
- Any other explicitly approved allocation.

Backfill MUST use a deterministic checkpoint and MUST be based only on eligible
pre-finalization positions. Ongoing emissions MUST begin from a deterministic
post-finalization checkpoint. No interval or position may earn under both
regimes unless the specification explicitly intends and tests that result.

The inherited v2 70/30 split is not enforced by the emitter. The v3 split is an
open economic decision and MUST NOT be changed implicitly during unrelated
implementation work.

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

- Accepted backstop assets at launch.
- Number and priority of loss tiers.
- Co-tier loss allocation method.
- Activation valuation method, thresholds, haircuts, and concentration caps.
- Treatment of queued capital for activation and emissions.
- Minimum junior-capital requirement.
- Post-migration designated token.
- Ongoing BLND-emission allocation.
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
4. Add one prefundable post-migration backstop asset.
5. Implement a canonical, testable activation-policy interface.
6. Implement legacy withdrawal without forced conversion.
7. Complete native, WASM, local-network, and testnet migration exercises.

New economic features should be introduced only after the corresponding open
decision is resolved in this specification.
