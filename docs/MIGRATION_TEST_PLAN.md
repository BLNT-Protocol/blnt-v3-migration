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

- The candidate stores one immutable v3 pool-factory address and rejects a
  constructor configuration that does not expose the required `backstop` and
  `is_pool` interfaces or is bound to another backstop.
- BLND:USDC deposits reject unregistered pools and registered addresses without
  the required loss-state callback before transferring tokens or minting shares.
- A registered compatible pool may accept new deposits while its callback
  blocks withdrawal.
- Once pool WASM upgrades are implemented, upgrade tests preserve the
  `backstop_withdrawal_allowed(backstop)` interface and fail-closed semantics.
- The immutable v3 factory deploys a registered Protocol-27 pool configured
  with the candidate backstop.
- The canonical pool callback rejects any address other than its configured
  backstop and returns false independently for nonzero backstop liabilities,
  committed loss or auction records, and unresolved bad-debt records.
- Missing canonical pool loss records fail closed.
- Each canonical loss-record map is bounded to 30 entries. Reserve-keyed
  amounts reject negatives, remove zeroes, and preserve their count on
  replacement; committed-loss auctions use presence-only identifiers.
- A failed multi-record loss transition rolls back every record change.
- Canonical loss-state mutation has no administrator or governance override and
  is not exposed as a public boolean setter.
- Once the lending, auction, and waterfall engines are implemented, native and
  WASM tests MUST prove each real record transition updates the corresponding
  aggregate count atomically and blocks withdrawals until resolution.
- BLND:USDC migration deposits and the BLND:USDC first-loss tier use the same
  token contract, user shares, pool totals, and global totals.
- BLND:USDC shares continue across migration without conversion, reissuance, or
  a second opt-in.
- Pre-migration and post-migration BLND:USDC deposits follow the same
  activation, waterfall, take-rate, and ongoing-emission eligibility rules,
  except that ongoing emissions cannot accrue before the emitter migrates.
- Migration activation freezes only backfill weight. A later BLND:USDC deposit
  joins the tier without changing the depositor's or system's finalized
  backfill weight.
- A withdrawal updates the same tier totals on either side of migration; a
  pre-migration withdrawal additionally forfeits the withdrawn shares'
  proportional backfill weight.
- BLND:USDC withdrawals require a 17-day Q4W entry and consume expired entries
  oldest-first.
- BLND:USDC dequeue operations restore active shares newest-first.
- Each user/pool BLND:USDC queue is bounded to 20 entries.
- Queued shares remain in custody and in the raw migration balance, retain
  backfill eligibility until actual withdrawal, and are exposed separately
  from active shares so the future canonical activation, take-rate, and
  ongoing-emission integrations can exclude them.
- Queue, dequeue, and withdrawal conserve user, pool, and global active,
  queued, and total shares.
- Withdrawal before expiry is rejected.
- Withdrawal fails closed unless the attributed pool affirmatively reports that
  this backstop has no committed or unresolved loss.
- Direct BLND:USDC transfers can affect the emitter's raw balance comparison
  but do not mint tier shares or receive any other protocol role.
- The prefunding token is configured independently from both the legacy
  migration token and the emitter's next designated token.
- Prefund deposits mint one share per token base unit and conserve exact user,
  pool, and global totals through withdrawal.
- Prefund shares can be deposited before and after migration and remain
  immediately withdrawable while they have no activation or loss role.
- Prefund balances do not qualify the legacy emitter, earn migration backfill,
  or change scheduled backfill.
- A direct transfer of the prefunding token does not create accounted shares.
- One BLND:USDC position may serve migration, activation, emissions, and loss
  roles, but its shares cannot be duplicated within any calculation.
- The active-share interface excludes queued withdrawals; future canonical
  activation, take-rate, and ongoing-emission integrations MUST use it.
- BLND:USDC depositors can withdraw the original LP asset under the tier's
  ordinary withdrawal rules without an involuntary asset swap.
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

## Ongoing BLND-emission allocation

The v3 tests must verify:

- Only active pools receive the backstop-depositor BLND tranche.
- Pool allocation is proportional to eligible underlying BLND across
  BLND:USDC and BLND:XLM backstop positions.
- BLND held through either eligible tier has equal emission weight per
  underlying BLND.
- Plain USDC, Custom A, and Custom B contribute zero BLND-emission weight.
- Within a pool, each user's allocation is proportional to the user's eligible
  underlying BLND across both eligible tiers.
- A pool or user with zero eligible underlying BLND receives zero.
- Pool weight cannot exceed total eligible underlying BLND across active
  pools, and user weight cannot exceed its pool's eligible underlying BLND.
- Negative values, invalid denominators, and checked-addition overflow are
  rejected; large proportional multiplication is handled with wide arithmetic.
- Pool-level and user-level integer remainders are conserved and carried
  forward only within the same allocation scope.
- Balance, LP-rate, and activation changes do not retroactively alter accrued
  emissions.
- Queued withdrawals and capital committed to losses receive the explicitly
  selected eligibility treatment.
- Underlying BLND values reject stale, caller-supplied, and same-transaction
  manipulated LP data.
- Accrual and claims use bounded accumulator/checkpoint accounting.
- Supply and borrow emissions remain separate from the backstop-depositor
  tranche.

## Execution environments

1. Native Soroban `Env` tests for fast iteration and ledger time travel.
2. WASM tests for contract-interface and serialization compatibility.
3. Local Stellar Quickstart with testnet resource limits.
4. An isolated public-testnet deployment of an incumbent v2 stack and a v3
   candidate stack.

The local-network runner MUST use the exact production queue constants and
MUST NOT deploy a shortened-delay candidate as migration evidence. It MUST
record contract IDs, transaction results, CLI resource-cost diagnostics,
balances, queue timestamps, and final invariants. Because Quickstart ledger
timestamps advance with wall-clock time, the runner is resumable across the
open, prepared, and active phases; native and WASM tests remain the accelerated
time-travel environments.

The local runner MUST use independently issued local assets and verify the
published legacy emitter WASM digest before deployment. Final verification MUST
prove that the emitter points to the active candidate, its queue is cleared,
the candidate/factory/pool bindings remain consistent, the pool is registered,
the incumbent and candidate balances are conserved, and the candidate retains
a strictly greater legacy-token balance.

Because local Quickstart closes ledgers faster than the five-second cadence
assumed by incumbent and candidate TTL constants, the runner MUST provide a
transaction-backed keepalive for the emitter queue, candidate, pool factory
registration, pool, depositor position, relevant token balances, and BLND
asset state. The keepalive MUST preserve all economic state and queue
timestamps, and its cadence MUST remain safely inside the shortest effective
local TTL. Legacy instance and distribution-checkpoint entries that lack
read-path TTL renewal MUST be extended explicitly rather than mutated through
an economic contract entry point. Asset-contract instances whose supported
read paths renew less than that cadence MUST also be extended explicitly.
The keepalive MUST fail unless the live queue still matches the saved
candidate, next designated token, and unlock time and the candidate retains a
strict legacy-token balance majority. Repeated monitoring MUST retain earlier
results and resource-cost evidence rather than overwrite them.

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
