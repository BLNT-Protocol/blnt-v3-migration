# V2 to V3 Migration Test Plan

The emitter migration and an individual pool's activation rule are independent
mechanisms. Tests must keep those concepts separate.

## Legacy emitter migration

The test suite must verify:

- The candidate directly holds strictly more of the incumbent designated token
  than the incumbent backstop.
- Equality is insufficient.
- The comparison continues to use the old designated token throughout the
  queue period.
- A balance deficit permits cancellation and prevents finalization.
- Finalization cannot occur before the 31-day unlock time.
- Finalization distributes emissions accrued through the finalization timestamp
  to the incumbent before changing the registered backstop.
- The candidate's ongoing-emission clock starts at the finalization timestamp;
  it receives no ongoing emissions for the pre-finalization interval.
- Preparation is unavailable until the final seven days before unlock, verifies
  the registered original queue, and preserves its unlock time as the immutable
  recovery anchor.
- The canonical `candidate.begin_migration()` path atomically creates the
  emitter queue and opens the original accounting epoch.
- A directly created emitter queue cannot be prepared until the permissionless
  `candidate.open_migration_epoch()` recovery path records it.
- The accounting epoch can open at the original queue start without making the
  queue eligible for finalization before the final seven-day verification.
- At most two replacement queues can be verified, and each receives its own
  seven-day direct-synchronization window.
- Cancellation and re-queueing do not extend the absolute 76-day recovery
  horizon.
- The deployment-anchored absolute migration deadline is fixed at 138 days
  after candidate construction, and no accepted original or replacement queue
  can extend beyond it.
- Canonical finalization rejects after the original queue's bounded recovery
  horizon.
- The candidate rejects a queued next designated token that differs from its
  configured token.
- The candidate can synchronize its lifecycle after the permissionless emitter
  swap is invoked directly within the seven-day synchronization grace period
  without changing the original recovery anchor.
- The canonical `candidate.finalize_migration()` path performs the emitter swap
  and candidate activation atomically; direct emitter finalization followed by
  `candidate.sync_migration()` is tested only as the bypass-recovery path.
- The synchronization deadline is measured from the verified queue's unlock
  time rather than from the direct swap transaction, including when the direct
  swap occurs late in the grace period.
- After a successful direct emitter swap, loss of the candidate's
  strictly-greater legacy-token balance does not prevent lifecycle
  synchronization.
- Synchronization rejects a replacement queue finalized after cancellation,
  including one that registers an unexpected next designated token.
- A full early withdrawal receives zero backfill weight.
- A partial early withdrawal forfeits only the withdrawn shares' proportional
  weight.
- The entire funded backfill quote is allocated among surviving positions in
  proportion to LP amount multiplied by holding duration.
- Scheduled backfill starts exactly 31 days before the original unlock, accrues
  at 1 BLND per second, and is capped at 10 million BLND.
- Prefunding begins at candidate construction, the original queue must start
  within 31 days, and every surviving deposit is weighted from its actual
  deposit time.
- A candidate whose original queue begins after the 31-day prefunding campaign
  is rejected rather than receiving unbounded early-weight credit.
- The complete 107-day original-queue-plus-recovery horizon schedules
  9,244,800 BLND rather than the theoretical 10 million maximum.
- Finalization moves BLND emissions from the incumbent to the candidate.
- The newly queued designated token is used for the next migration.
- The initial BLND drop and backfilled emissions cannot be claimed twice.

## Candidate backstop

The v3 tests must verify:

- BLND:USDC migration deposits are accounted for independently from any
  post-migration backstop asset.
- The prefunding token is configured independently from both the legacy
  migration token and the emitter's next designated token.
- Prefund deposits mint one share per token base unit and conserve exact user,
  pool, and global totals through withdrawal.
- Prefund shares can be deposited before and after migration and remain
  immediately withdrawable while they have no activation or loss role.
- Prefund balances do not qualify the legacy emitter, earn migration backfill,
  or change scheduled backfill.
- A direct transfer of the prefunding token does not create accounted shares.
- Depositors cannot be double-counted across migration, activation, emissions,
  or loss tiers.
- Queued withdrawals receive the intended activation and emission treatment.
- Legacy BLND:USDC depositors can reclaim or explicitly convert their
  positions without an involuntary asset swap.
- Each accepted asset has an explicit valuation, haircut, concentration limit,
  withdrawal policy, liquidation path, and loss-waterfall position.

## Pool activation

For a multi-asset activation formula, test:

- BLND:XLM LP, BLND:USDC LP, plain USDC, Custom A, and Custom B independently.
- Unconfigured optional custom tiers contribute zero activation value.
- Combined verified values immediately below, exactly at, and immediately above
  the 12,500-USDC entry threshold.
- Active-pool values immediately below and exactly at the 10,000-USDC
  maintenance threshold.
- An inactive pool in the 10,000-to-12,500-USDC hysteresis band remains
  inactive and must meet the entry threshold before reactivation.
- Equal activation weight for every USDC of verified value, without
  asset-specific haircuts, concentration caps, or minimum BLND allocation.
- Negative values and checked-arithmetic overflow.
- Rejection of stale, unavailable, caller-supplied, and same-transaction
  manipulated valuation inputs by the production valuation adapter.
- Price, reserve, and LP-supply manipulation.
- Oracle staleness and unavailable-market behavior.
- Queued withdrawals and assets already committed to covering losses.
- Consistent results between pool status and reward-zone eligibility.

## Loss waterfall

The v3 tests must verify:

- BLND:USDC LP absorbs losses before BLND:XLM LP.
- BLND:XLM LP absorbs losses only after eligible BLND:USDC LP is exhausted.
- Plain USDC absorbs losses only after both LP tiers are exhausted.
- Optional Custom A absorbs losses only after plain USDC is exhausted.
- Optional Custom B absorbs losses only after Custom A is exhausted.
- Supplier balances absorb only the residual loss after all five backstop
  tiers are exhausted, ignoring unconfigured optional tiers.
- A partial loss contained by one tier does not modify later tiers.
- Losses crossing one, multiple, and all four backstop-tier boundaries are
  conserved exactly, subject to explicitly documented rounding.
- A loss that exhausts all five tiers passes only the remaining residual to
  suppliers.
- Capital committed to an existing loss or auction is not counted or consumed
  twice.
- An unavailable oracle or disposal path follows the explicitly selected
  failure policy and does not silently skip a tier.
- Activation value remains independent of loss priority.

## Take-rate allocation

The v3 tests must verify:

- Tier weights are `2.5,2.5,2,2,1` for BLND:USDC LP, BLND:XLM LP, USDC,
  Custom A, and Custom B respectively, represented exactly on-chain as
  `5:5:4:4:2`.
- Each tier receives take-rate assets in proportion to its verified
  reward-eligible USDC value multiplied by its weight.
- Equal-value tier positions receive relative allocations matching their
  weights, subject only to documented integer rounding.
- Empty and unconfigured optional tiers contribute zero and are omitted from
  the effective denominator.
- Adding or removing a reward-eligible position updates the denominator without
  retroactively changing already accrued allocations.
- Within a tier, depositors receive allocations proportional to their
  reward-eligible shares.
- Take-rate allocation does not change activation value, loss priority, or BLND
  emissions.
- Every distributed reserve asset is conserved, with integer remainder carried
  forward in an accumulator keyed by pool and distributed reserve asset rather
  than assigned to a tier.
- Remainders belonging to different pools or reserve assets remain isolated
  and can be applied only to a later distribution of the same reserve asset for
  the same pool.
- Accrual and claims use bounded accumulator/checkpoint accounting.

## Execution environments

1. Native Soroban `Env` tests for fast iteration and ledger time travel.
2. WASM tests for contract-interface and serialization compatibility.
3. Local Stellar Quickstart with testnet resource limits.
4. An isolated public-testnet deployment of an incumbent v2 stack and a v3
   candidate stack.

Native tests MUST advance ledger sequence through the 83 days from earliest
preparation through the two-retry horizon and verify the candidate's 89/90-day
TTL renewal policy.

The Protocol-27 migration harness MUST load v2 WASM compiled by the frozen
SDK-22 workspace. It MUST NOT recompile the v2 contracts against SDK 27 merely
to make them share the v3 crate graph. This verifies the actual compatibility
boundary: historical contract bytecode executing on a current host and
interacting with SDK-27 v3 bytecode.

The compatibility probe MUST invoke at least one v2 backstop entry point
through SDK-27 WASM. The migration harness MUST also exercise the complete
legacy-emitter swap into optimized SDK-27 candidate WASM.

Public testnet should use independently issued test assets rather than relying
on the supply of official test BLND or USDC.
