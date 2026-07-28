# V2 to V3 Migration Test Plan

This plan validates the frozen
[v2 baseline](V2_SYSTEM_SPEC.md), [v3 specification](V3_SYSTEM_SPEC.md), and
[valuation adapter](VALUATION_ADAPTER.md) as composed by the
[specification index](SYSTEM_SPEC.md).

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
- Except for the emitter's final time-accrued BLND mint to the incumbent,
  finalization changes no incumbent lending principal, collateral, debt, or
  backstop share. It invokes no position conversion, transfer, close, or import
  into v3.
- After finalization and while the candidate remains registered, new
  legacy-emitter distributions go only to the candidate. BLND already held,
  allocated, or accrued in v2 remains governed by v2 accounting and claim
  rules.
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
- Recovery synchronization caps backfill at the verified queue unlock, while
  ongoing reconciliation includes BLND received after the direct swap; tests
  prove the synchronization delay cannot be rewarded under both regimes.
- BLND:USDC deposited at or after the current verified unlock remains usable
  tier capital but receives zero backfill weight for that queue, so its later
  deposit timestamp cannot make recovery weight negative.
- Once a direct swap makes the candidate the emitter recipient, BLND-bearing
  tier-weight mutations and reward-zone edits fail until synchronization;
  synchronization remains permissionless and succeeds without those
  mutations reweighting the unresolved ongoing-emission interval.
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
- Queue creation, epoch opening, preparation, finalization, and direct-swap
  synchronization require no governance, multisig, or administrator
  authorization.
- Candidate interfaces expose no governance recovery, protocol-wide emergency
  override, privileged WASM replacement, or administrative migration bypass.
- The continuing privileged role is the transferable, pool-local v2-compatible
  admin described below. It may select bounded statuses, update the permitted
  pool parameters, queue or cancel reserve configuration, and configure the
  30% supply/borrow emission weights. It cannot alter migration, backstop-tier
  policy, withdrawal safety, contract WASM, or protocol-wide policy.
- Deployment and transaction-submission identities receive no continuing
  protocol authority.
- A full early withdrawal receives zero backfill weight.
- A partial early withdrawal forfeits only the withdrawn shares' proportional
  weight.
- The entire funded backfill quote is allocated among surviving positions in
  proportion to LP amount multiplied by holding duration.
- Scheduled backfill starts exactly 31 days before the original unlock, accrues
  at 1 BLND per second, and is capped at 10 million BLND.
- Prefunding begins at candidate construction, the original queue must start
  within 31 days, and every eligible surviving pre-unlock deposit is weighted
  from its actual deposit time.
- A candidate whose original queue begins after the 31-day prefunding campaign
  is rejected rather than receiving unbounded early-weight credit.
- The complete 107-day original-queue-plus-recovery horizon schedules
  9,244,800 BLND rather than the theoretical 10 million maximum.
- Finalization moves BLND emissions from the incumbent to the candidate.
- The newly queued BLND:XLM Comet LP is used as the designated token for the
  next migration.
- Successful migration awards zero discretionary BLND.
- If the initial BLND drop funds backfill, its aggregate distribution equals
  the immutable scheduled backfill obligation, does not exceed 10 million
  BLND, and names only the candidate backstop coordinator as recipient.
- The otherwise available allocation of up to 40 million BLND remains
  undistributed.
- No deployer, operator, treasury, or general migration-participant award can
  be funded, and backfill cannot be funded or claimed twice.
- Backfill funding is permissionless only after activation and invokes the
  published emitter with one recipient: the candidate itself.
- Funding records exactly the scheduled amount only when the configured BLND
  balance increases by that amount; an unexpected delta rolls back the drop and
  the candidate marker atomically.
- Funding rejects when no surviving migration weight exists, preventing a
  scheduled drop from becoming stranded discretionary capital.
- A claim requires the migration-position owner's authorization, may pay an
  owner-selected recipient, and uses frozen position and total-weight
  checkpoints without depositor iteration.
- Post-migration deposits receive zero backfill, and a position cannot claim
  its frozen entitlement twice.
- Funded, aggregate-claimed, and remaining-reserve reads conserve the initial
  drop exactly. Per-position floor-rounding dust remains reserved in the
  candidate and cannot become an ongoing emission or discretionary award.

## Candidate backstop

The v3 tests must verify:

- The candidate stores one immutable v3 pool-factory address and rejects a
  constructor configuration that does not expose the required `backstop` and
  `is_pool` interfaces or is bound to another backstop.
- Deposits into every accounted tier reject unregistered pools and registered
  addresses without the required loss-state callback before transferring
  tokens or minting shares.
- A registered compatible pool may accept new deposits while its callback
  blocks withdrawal.
- The candidate exposes no pool-WASM replacement path that could bypass the
  `backstop_withdrawal_allowed(backstop)` interface or its fail-closed
  semantics.
- The immutable v3 factory deploys a registered Protocol-27 pool configured
  with the candidate backstop.
- The canonical pool callback rejects any address other than its configured
  backstop and returns false independently for nonzero backstop liabilities,
  committed loss or auction records, and unresolved bad-debt records.
- Missing canonical pool loss records fail closed.
- Each canonical loss-record map is bounded to 30 entries. Reserve-keyed
  amounts reject negatives, remove zeroes, and preserve their count on
  replacement; general committed-loss auctions use presence-only identifiers.
- The one prepared bad-debt auction contributes one committed-loss count from
  its canonical singleton record without duplicating its identifier in the
  general committed-loss map.
- A failed multi-record loss transition rolls back every record change.
- Canonical loss-state mutation has no administrator or governance override and
  is not exposed as a public boolean setter.
- Native and WASM tests MUST prove each implemented lending, auction, and
  waterfall transition updates the corresponding aggregate count atomically
  and blocks withdrawals until resolution.
- BLND:USDC migration deposits and the BLND:USDC first-loss tier use the same
  token contract, user shares, pool totals, and global totals.
- BLND:USDC shares continue across migration without conversion, reissuance, or
  a second opt-in.
- Every pool and tier stores separate accounted assets, total shares, and
  queued shares; a loss in one pool cannot change another pool's exchange rate.
- Initial deposits mint one share per token base unit. Later deposits round
  shares down at the current exchange rate, and zero-share deposits fail
  atomically.
- Withdrawals round assets down, while redemption of the final pool-tier share
  sweeps its remaining conversion dust.
- A fully drained tier rejects new deposits while shares remain, permits
  expired worthless shares to burn for zero assets, and can restart at one to
  one only after every old share is removed.
- Active and queued shares are converted independently at one common
  pool-tier rate. A partial or complete tier impairment reduces both
  proportionally without iterating depositors.
- Direct token transfers change neither pool assets nor global accounted asset
  totals and cannot manipulate exchange rates.
- BLND:USDC backfill principal records actual LP tokens contributed separately
  from shares after an exchange-rate change. Withdrawals remove principal and
  time basis pro rata to shares burned; protocol loss absorption does not
  forfeit the depositor's backfill history.
- Pre-migration and post-migration BLND:USDC deposits follow the same
  activation, waterfall, take-rate, and ongoing-emission eligibility rules,
  except that ongoing emissions cannot accrue before the emitter migrates,
  direct-swap recovery temporarily fails closed until synchronization, and
  deposits at or after a verified unlock receive no backfill for that queue.
- Migration activation freezes only backfill weight. A later BLND:USDC deposit
  joins the tier without changing the depositor's or system's finalized
  backfill weight.
- A withdrawal updates the same tier totals on either side of migration; a
  pre-migration withdrawal additionally forfeits the withdrawn shares'
  proportional backfill weight.
- Every tier requires a 17-day Q4W entry and consumes expired entries
  oldest-first.
- Every tier's dequeue operations restore active shares newest-first.
- Each user/pool/tier queue is bounded to 20 entries.
- Queued shares remain in custody and in the raw migration balance, retain
  backfill eligibility until actual withdrawal, and are exposed separately
  from active shares so canonical activation and ongoing-emission integrations
  can exclude them.
- Queued shares remain take-rate eligible until actual withdrawal. Queueing
  and dequeueing leave the take-rate denominator unchanged, and withdrawal
  checkpoints accrued value before removing shares from future allocations.
- Queued shares remain exposed to tier losses until actual withdrawal.
  Queueing does not snapshot redemption value, and a loss changes active and
  queued share value proportionally through one common exchange rate.
- Queue, dequeue, and withdrawal conserve user, pool, and global active,
  queued, and total shares.
- Withdrawal before expiry is rejected.
- Withdrawal fails closed unless the attributed pool affirmatively reports that
  this backstop has no committed or unresolved loss.
- Direct BLND:USDC transfers can affect the emitter's raw balance comparison
  but do not mint tier shares or receive any other protocol role.
- The BLND:USDC, BLND:XLM, and plain-USDC token contracts are distinct.
- BLND:XLM and USDC deposits mint shares at their pool-tier exchange rate and
  conserve exact user, pool, queued, active, asset, and global totals through
  withdrawal.
- BLND:XLM and USDC shares can be deposited before and after migration and use
  the same bounded Q4W and pool loss-safety callback as BLND:USDC.
- BLND:XLM and USDC balances do not qualify the incumbent emitter, earn
  migration backfill, or change scheduled backfill.
- Direct transfers of BLND:XLM or USDC do not create accounted shares.
- Tests cover the checked share-to-asset exchange rate, conservative rounding,
  dust handling, cross-pool isolation, and proportional loss exposure of active
  and queued shares.
- One BLND:USDC position may serve migration, activation, emissions, and loss
  roles, but its shares cannot be duplicated within any calculation.
- The active-share interface excludes queued withdrawals; future canonical
  activation and ongoing-emission integrations MUST use it. Take-rate
  integration MUST instead use total accounted in-custody shares.
- BLND:USDC depositors can withdraw the original LP asset under the tier's
  ordinary withdrawal rules without an involuntary asset swap.
- Each accepted asset has an explicit valuation, haircut, concentration limit,
  withdrawal policy, liquidation path, and loss-waterfall position.

## Pool activation

For a multi-asset activation formula, test:

- BLND:XLM LP, BLND:USDC LP, and plain USDC independently.
- Combined verified values immediately below, exactly at, and immediately above
  the 12,500-USDC entry threshold.
- Active-pool values immediately below and exactly at the 10,000-USDC
  maintenance threshold.
- An inactive pool in the 10,000-to-12,500-USDC hysteresis band remains
  inactive and must meet the entry threshold before reactivation.
- Equal activation weight for every USDC of verified value, without
  asset-specific haircuts, concentration caps, or minimum BLND allocation.
- Negative values and checked-arithmetic overflow.
- Immutable adapter binding and rejection of an incompatible adapter version.
- Candidate construction rejects a version-compatible adapter whose immutable
  BLND, plain-USDC, BLND:USDC, or BLND:XLM binding differs from the candidate.
- Canonical derivation from pool-attributed active and queued shares, including
  exclusion of unattributed direct transfers.
- Separate LP quotes for active and queued BLND:USDC and BLND:XLM amounts;
  zero LP amounts skip the external quote.
- Exact one-to-one plain-USDC valuation without an adapter call.
- Underlying BLND derived from active LP shares for the shared emission basis.
- Rejection of negative, expired, unavailable, caller-supplied, and
  same-transaction manipulated valuation inputs by the production valuation
  adapter.
- Price, reserve, and LP-supply manipulation.
- Oracle staleness and unavailable-market behavior.
- Production SEP-40 construction rejects a non-USDC base, missing BLND or XLM
  feed membership, incompatible precision or resolution, a TWAP outside its
  immutable bounds, incorrect Comet tokens or weights, and non-seven-decimal
  tokens.
- Production TWAP validation accepts unordered uniform ticks, rounds the
  arithmetic mean down, and rejects missing, duplicate, gapped, nonpositive,
  future, misaligned, and expired ticks or a changed feed identity.
- Production Comet valuation uses the lower 80%-BLND and 20%-paired-reserve
  implied total. Increasing either reserve alone cannot increase the quote,
  while reducing either side conservatively reduces it.
- The optimized production adapter quotes both accepted Comets within
  Protocol-27 mainnet invocation limits while each of 25 oracle ticks per
  asset occupies an independent persistent ledger entry.
- The optimized candidate canonical valuation exercises four production
  adapter calls—active and queued BLND:USDC plus active and queued BLND:XLM—
  against that independent-tick footprint within Protocol-27 mainnet
  invocation limits.
- Before public deployment, repeat the resource regression against the exact
  selected oracle WASM and initialized storage layout. The conservative generic
  fixture does not replace provider-specific simulation. The isolated
  `scripts/valuation-oracle.sh` lane MUST fail closed on provider identity,
  asset membership, history shape, freshness, and exact-WASM collection before
  it can deploy the adapter or record the four post-deployment quote
  simulations.
- Queued withdrawals and assets already committed to covering losses.
- Consistent activation valuation for pool-status and reward-zone threshold
  decisions.

## Pool status

The v3 tests must verify:

- Status codes preserve v2 meanings: Admin Active `0`, Active `1`, Admin
  On-Ice `2`, On-Ice `3`, Admin Frozen `4`, Frozen `5`, and Setup `6`.
- Q4W percentage is the ceiling of queued verified USDC value divided by total
  in-custody verified USDC value at seven-decimal precision.
- Multi-asset Q4W calculation combines all configured tiers in verified USDC
  value without applying take-rate weights or mixing raw share units.
- With no pool-local override, Q4W immediately below 30% remains Active,
  exactly 30% becomes On-Ice, immediately below 60% remains On-Ice, and
  exactly 60% becomes Frozen.
- Admin Active remains active immediately below 50% Q4W and becomes On-Ice
  exactly at 50%.
- Admin On-Ice remains On-Ice immediately below 75% Q4W and becomes Frozen
  exactly at 75%.
- Falling below the 10,000-USDC maintenance threshold moves an active pool
  On-Ice independently of Q4W.
- An inactive pool in the hysteresis band cannot reactivate until it reaches
  the 12,500-USDC entry threshold.
- Permissionless refresh rejects Setup and Admin Frozen.
- The current transferable pool admin can select statuses 0, 2, and 4, or
  relinquish an override into ordinary On-Ice status 3. Admin Active requires
  the applicable activation threshold and Q4W below 50%; Admin On-Ice and
  ordinary On-Ice require Q4W below 75%; Admin Frozen is always selectable.
- The initial pool admin authorizes factory deployment and fixes its own
  deterministic salt domain. The current admin alone can propose a successor,
  the proposed admin must
  accept, and neither proposal nor failed acceptance changes current authority.
- Every successful permissionless or admin transition stores the canonical
  result atomically and emits its previous status, new status, and whether the
  transition was an admin action. Failed, disallowed, stale, or invalid
  quotes leave stored status unchanged.
- At or above 30% but below 50% Q4W, an otherwise qualified pool is ordinarily
  On-Ice, but the admin can select Admin Active and re-enable borrowing.
  Tests must treat this as an explicit bounded risk-on override rather than
  risk-off or permissionless behavior.
- The pool admin can update the v2-compatible pool parameters, queue or cancel
  reserve configuration, and configure the 30% supply/borrow emission weights.
  A new or updated reserve unlocks immediately in Setup and after seven days in
  every other status; any caller can apply it after unlock.
- The pool admin cannot change thresholds, valuation, the three backstop
  assets, `5:5:4` weighting, withdrawal safety, loss ordering, the 70/30 split,
  WASM, migration, or protocol-wide policy.
- On-Ice blocks borrowing and auction cancellation. Frozen additionally blocks
  both supply and collateral supply, exactly matching the v2 action matrix.
  Repayment, withdrawals, auction fills, and other actions not prohibited by
  that matrix remain subject to their ordinary safety checks.
- Queueing alone does not mutate stored status; permissionless refresh applies
  the transition using canonical checkpointed inputs.
- Pool-status enforcement uses the canonical stored status. Reward-zone
  admission and maintenance use the same canonical activation valuation but
  do not treat stored status as an additional eligibility gate.

## Initial lending core

The v3 tests must verify:

- Factory deployment fixes the oracle boundary and initial pool and reserve
  configuration. V2-compatible admin updates validate and atomically mutate
  only the permitted pool fields.
- Reserve queues reject duplicates and invalid changes, require the current
  admin for queue and cancellation, enforce the post-Setup seven-day timelock,
  and allow permissionless application after unlock.
- The queue input and event expose complete mutable reserve metadata without an
  asset or index, plus the separately supplied asset and unlock timestamp, for
  unambiguous pre-application auditing.
- Multiple new reserves may be queued concurrently; each receives the next
  immutable reserve index when its queue is applied and returns that final
  index, matching v2.
- Existing-reserve updates preserve the immutable index and decimals, reject
  removal of a nonzero liability factor, preserve the interest modifier for
  non-curve changes, and reset it when inherited curve parameters change.
- Supply, withdraw, collateral supply, collateral withdraw, borrow, and repay
  execute through both direct and allowance submissions against native and
  optimized pool WASM. The allowance path nets incoming and outgoing amounts
  independently for each asset.
- A zero-fee flash loan creates its borrow before the accompanying requests,
  invokes the receiver with `(from, asset, amount, 0)`, and settles the request
  set through `from`'s allowance. Immediate repayment and a healthy retained
  liability both work; callback, allowance, health, utilization, or balance
  failure rolls back the complete invocation.
- Every request enforces the stored status before changing reserve or position
  accounting. On-Ice rejects borrow; Frozen rejects supply and collateral
  supply while repayment and ordinary safe withdrawals remain available.
- `spender` authorization is always required, and a distinct position owner
  must also authorize. No pure debt-reduction exception permits
  unauthenticated repay-on-behalf.
- bToken minting rounds down, bToken withdrawal burns round up, dToken borrowing
  rounds up, and partial dToken repayment rounds down.
- Repayment reproduces v2's dToken comparison at a non-unit dRate. A
  rounded-down burn equal to the position balance clears the debt but collects
  the complete request in both transfer modes, even when rounding places it
  slightly above live debt. Only a burn strictly greater than the balance
  enters the over-repayment branch: direct submission returns the excess to
  `to`, while allowance submission nets it and pulls only live debt.
- Authenticated empty direct and allowance submissions succeed as no-ops and
  return the owner's current positions.
- The flash-loan event exactly preserves the v2 topics
  `["flash_loan", asset, from, contract]` and vector data
  `[tokens_out, d_tokens_minted]`.
- The adaptive interest curve matches inherited v2 reference vectors, increases
  dRate, allocates the immutable take-rate portion to accrued backstop credit,
  and allocates the remainder through bRate.
- Health calculation rounds collateral down and liabilities up, rejects a
  result below 1.00001 or below minimum effective collateral, and fails closed
  for nonpositive, expired, or wrong-decimal oracle quotes.
- Disabled reserves, supply caps, maximum utilization, maximum positions,
  unknown reserves, invalid constructor configuration, and arithmetic overflow
  fail atomically. Direct, allowance, and flash-loan paths reject more than 60
  requests before changing state; the bound does not assert that every
  60-request composition fits the network resource envelope.
- A failed health, status, authorization, price, token transfer, or arithmetic
  check leaves token balances, reserve state, user positions, loss records, and
  events unchanged.
- Every successful submission matches the exact expected pool token-balance
  delta for each touched reserve; fee-on-transfer or other in-call balance
  mismatches roll back.
- Borrow and repay by the configured backstop atomically create and clear its
  canonical reserve-liability record, changing withdrawal eligibility without
  combining units across reserves.
- Persistent reserve and user entries renew their TTL within the documented
  bounds.

## User-liquidation auctions

The v3 tests must verify:

- Auction creation is permissionless, accepts only an unhealthy non-pool,
  non-backstop liquidatee, rejects duplicate or unsupported bid/lot assets, and
  permits at most one active user-liquidation auction per account.
- Bid and lot asset counts are bounded by the pool's current configured
  maximum-position limit.
- Quote construction inherits the v2 average-factor incentive, rounding, full
  liquidation, and 1.03-through-1.15 post-liquidation repair constraints.
- An auction cannot fill before its next-ledger start.
- The first 200 ledgers scale the lot from 0% to 100% with a full bid, and the
  next 200 scale the bid from 100% to 0% with a full lot.
- Partial fills round the selected base bid up and base lot down, preserve the
  unfilled base quote, and apply the temporary entry's 45-day renewal
  threshold and 46-day bump.
- Request type 6 transfers only bToken collateral and dToken liability shares,
  conserves aggregate reserve shares, rejects self-fills, and leaves the filler
  within ordinary health, minimum-collateral, and maximum-position limits.
- An account with an active liquidation cannot complete another submission.
  Request type 9 can cancel the auction only when status permits and the final
  liquidatee position is healthy.
- Fills remain available under On-Ice and Frozen status. Cancellation is
  blocked under those statuses.
- Any caller can delete an auction at, but not before, 500 ledgers after its
  start.
- Completion of an auction that leaves nonzero liabilities and no collateral
  atomically transfers every residual dToken share to the configured backstop,
  accumulates same-reserve balances, updates canonical liability records, and
  leaves reserve dToken supply unchanged.
- Each transferred reserve emits a handoff event identifying the liquidatee,
  configured backstop, asset, and dToken amount.
- An incomplete fill that would exhaust collateral still fails atomically. A
  fill that leaves collateral and liabilities but worsens the liquidatee's
  effective health also fails atomically.
- The handoff creates, fills, and resizes no bad-debt auction. If an earlier
  bad-debt auction is active, its bid and committed tier lot remain unchanged;
  newly handed-off liabilities remain canonical for later permissionless
  continuation.
- Checked overflow or canonical-record-capacity failure during a multi-reserve
  handoff rolls back the auction, liquidatee, filler, backstop position,
  canonical records, and events.
- Native and optimized-WASM tests cover creation, partial or complete fills,
  active-auction blocking, status enforcement, bad-debt handoff, existing
  backstop-liability accumulation, active bad-debt-auction interaction, and
  rollback.
- An optimized-WASM test fills a completed liquidation with liabilities in all
  30 reserves, verifies one canonical record per reserve and unchanged reserve
  dToken supply, and measures the fill against SDK-27 mainnet instruction,
  memory, footprint, byte, write-entry, and contract-event limits.

## Loss waterfall

The v3 tests must verify:

- Bad-debt auction preparation accepts only canonical liabilities already
  assigned to the configured backstop and permits at most one active bad-debt
  auction per pool.
- Only the registered pool contract can commit, settle, or release its bad-debt
  lot; direct user calls fail authorization.
- The prepared lot targets 120% of the pool's oracle-valued debt, normalized
  to seven-decimal USDC.
- Each prepared lot contains assets from exactly one tier and selects the first
  tier with at least 200 USDC of verified, uncommitted value in strict
  BLND:USDC LP, BLND:XLM LP, then plain-USDC order.
- Exactly 200 USDC qualifies; 199.9999999 USDC does not.
- A tier below the minimum is skipped even when it alone could satisfy the
  current 120% target. No solve-the-debt exception combines or revives smaller
  tiers.
- A partial tier selection rounds its asset amount up so its proportional
  value does not underfill the target.
- A commitment is pool-local, changes neither shares nor exchange rates, and
  is excluded from activation value and subsequent bad-debt quotes.
- The pool auction and backstop commitment have identical fixed 46-day
  temporary lifetimes, and reads renew neither entry.
- A bad-debt auction cannot fill before its next-ledger start. Fills use the
  inherited 400-ledger curve, round the percentage-selected base dToken bid up
  and base tier-token lot down, and apply the ledger modifiers only to the
  amounts actually transferred.
- A filler cannot be the pool or backstop, cannot have an active
  user-liquidation auction, and must remain within ordinary maximum-position,
  minimum-collateral, and health limits after assuming the scaled dToken bid.
- A partial fill removes the percentage-selected base bid and lot from the
  auction and commitment, transfers only the time-scaled bid and lot, retains
  the synchronized remainder, and applies the common 45-day renewal threshold
  and 46-day bump to both temporary entries.
- Only the tier tokens actually transferred to the filler reduce accounted
  pool and global tier assets. Shares remain unchanged, so active and queued
  positions absorb that reduction through the same pool-tier exchange rate.
  The base-lot discount that is not transferred causes no asset impairment.
- A complete fill removes both the pool auction and backstop commitment. A
  time-discounted dToken remainder stays assigned to the backstop, remains in
  canonical liability state, and continues to block withdrawals.
- With no active bad-debt auction, any caller can invoke one bounded
  continuation without authorization. An active auction rejects continuation.
- Continuation validates the configured backstop's complete position against
  every canonical reserve-liability record. Missing, unknown, negative, or
  inconsistent state fails atomically.
- A liability batch is selected in immutable reserve order and contains no
  more than `min(max_positions - 1, 4)` assets. Repeated continuation
  eventually covers a larger bounded liability set without exceeding the
  auction position limit, while the four-asset cap preserves footprint
  headroom for stateful per-reserve oracle reads.
- When an eligible tier remains, continuation commits a quote identical to the
  preceding read-only quote and creates the next-ledger auction under the
  caller-supplied identifier. The same tier is selected again while it remains
  at or above 200 USDC.
- A backstop response indicating no eligible tier defaults every remaining
  canonical liability to suppliers in one atomic, at-most-30-reserve loop. A
  stale, invalid, or unavailable valuation traps and cannot enter this path.
- Supplier default accrues each reserve, removes the backstop dToken position,
  reduces `d_supply` by the same shares, preserves `b_supply`, and uses ceiling
  rounding for both the dToken-to-asset conversion and bRate loss. The new
  bRate saturates at zero exactly as in v2.
- Supplier default transfers no tokens. It clears canonical liability records
  only after every reserve succeeds, emits the before/after loss amounts, and
  re-enables withdrawal only when no other canonical loss record remains.
- The protocol transfers the selected tier token directly to the filler
  without a swap. Token-transfer failure rolls back the dToken movement,
  filler state, canonical liability, auction, commitment, and tier impairment.
- Native tests and optimized-WASM tests cover authorization, pre-start
  rejection, unhealthy-filler rollback, partial and full fills, direct token
  receipt, synchronized commitment state, exact tier exchange-rate impairment,
  repeated same-tier continuation, verified exhaustion, and exact supplier
  exchange-rate impairment.
- An optimized-WASM test constructs the maximum 30-reserve supplier-default
  path with nonzero sub-threshold capital in all three tiers. Reserve prices
  and both LP valuations use independent persistent entries. The test verifies
  that the continuation remains within the SDK-27 mainnet invocation limits
  for instructions, memory, footprint entries, read/write bytes, and contract
  events.
- Any caller may delete a prepared auction at, but not before, 500 ledgers,
  atomically releasing its backstop lot. The canonical liability remains and
  continues to block withdrawal until the debt is resolved.
- BLND:USDC LP is auctioned before BLND:XLM LP whenever its available verified
  value is at least 200 USDC.
- BLND:XLM LP is selected only after BLND:USDC LP is below 200 USDC or has
  otherwise been exhausted by completed settlement.
- Plain USDC is selected only after both LP tiers are below 200 USDC or have
  otherwise been exhausted by completed settlement.
- Supplier balances absorb only the residual loss after all three backstop
  tiers are each below 200 USDC or otherwise exhausted.
- Aggregate backstop value skipped solely because of the per-tier minimum is
  strictly less than 600 USDC and remains attributed to depositors.
- A partial loss contained by one tier does not modify later tiers.
- Losses crossing one or both backstop-tier boundaries are conserved exactly,
  subject to explicitly documented rounding.
- A loss that exhausts all three tiers passes only the remaining residual to
  suppliers.
- Capital committed to an existing loss or auction is not counted or consumed
  twice.
- A loss occurring after shares enter Q4W reduces the queued and active
  positions' redemption value proportionally. It does not charge active shares
  first or preserve the queued position's pre-loss value.
- A queued withdrawal remains blocked while the loss is committed or
  unresolved and, once permitted, redeems at the post-loss exchange rate.
- An unavailable, invalid, or stale oracle result fails closed and is not
  treated as a tier below the minimum. A failed tier-token transfer rolls back
  and does not silently skip a tier.
- Once a qualifying tier lot is committed, later price movement below the
  minimum does not cancel the commitment.
- Activation value remains independent of loss priority.

## Take-rate allocation

The v3 tests must verify:

- Tier weights are `2.5,2.5,2` for BLND:USDC LP, BLND:XLM LP, and USDC
  respectively, represented exactly on-chain as `5:5:4`.
- Each reserve asset's newly checkpointed backstop credit is allocated to
  tier-specific pending lots in proportion to verified pool-tier USDC value
  multiplied by its weight.
- Equal-value tier positions receive relative allocations matching their
  weights, subject only to documented integer rounding.
- Empty tiers contribute zero and are omitted from the effective denominator.
- Canonical take-rate valuation includes all accounted pool-tier assets and
  does not subtract a separately committed bad-debt lot.
- The three floor-rounded pending amounts plus a pool-and-reserve-specific
  carry conserve the exact checkpointed credit. Carries for different pools or
  reserve assets remain isolated.
- Adding or removing a tier position changes only later checkpoints and cannot
  rewrite already pending allocations.
- Take-rate allocation does not change activation value, loss priority, or BLND
  emissions.
- Creation accepts a duplicate-free batch of one through
  `min(max_positions - 1, 4)` configured reserve assets, accrues them, and
  rejects empty, oversized, duplicate, and unknown batches.
- A pool can have at most one active interest auction. Creation scans the three
  tiers cyclically from a stored cursor and selects the first supplied tier
  whose verified pending lot is at least 200 USDC, advancing the cursor after
  selection.
- Exactly 200 USDC qualifies and 199.9999999 USDC does not. A continuously
  qualifying tier cannot starve either later qualifying tier across successive
  auctions.
- A caller-supplied batch may omit pending assets without reallocating or
  destroying them; the next creation can include and realize those assets.
- Missing, nonpositive, invalid-decimal, or stale LP or reserve valuation fails
  closed rather than skipping a tier or declaring that no qualifying lot
  exists.
- The selected tier commits a bid of its own tier token worth 120% of the
  pending reserve-asset lot, rounded up. The pool auction and backstop
  commitment have identical identifiers, tier, quote, and fixed 46-day
  temporary lifetime.
- A selected tier token that does not expose exactly seven decimals fails
  commitment before any auction or reservation is persisted.
- Only the registered pool can create, resize, settle, or release its interest
  commitment. A direct user call fails authorization.
- Fills cannot execute before the next-ledger start and inherit the v2
  400-ledger curve: full bid with ramping lot for 200 ledgers, then full lot
  with declining bid for 200 ledgers.
- Percentage selection rounds base bid up and base lot down. Partial fills
  transfer only the time-scaled amounts, release the base-lot discount while
  leaving it pending in the same tier, and preserve exact synchronization
  between the remaining pool auction and backstop commitment.
- The filler authorizes the fill and cannot be the pool or backstop. Exact
  source token-balance deltas are enforced for every bid and lot transfer.
- The bid is donated to the selected pool-tier without minting shares. Active
  and queued shares appreciate through one common exchange rate and no
  depositor claim entry point exists.
- Deposits into and actual withdrawals from the selected tier fail while its
  interest commitment is active. Queue and dequeue operations, plus activity
  in other tiers, remain available.
- A completed fill removes both temporary records, decrements reserve
  backstop-credit accounting by only the lot actually transferred, and makes
  the donated tier assets immediately eligible for activation, loss capacity,
  later take-rate weighting, and the applicable BLND-emission weight.
- At, but not before, 500 ledgers any caller can delete a stale auction,
  release its commitment, and leave all unfilled base lots in their original
  pending pool/reserve/tier accumulators without changing token balances.
- Simultaneous expiry of the fixed temporary auction and commitment cannot
  erase or reweight the persistent pending tier allocation; a later creation
  can reserve the same credit again.
- Interest auction creation, fill, and stale recovery remain available under
  every stored pool status and never create or alter a lending position or
  pool loss record.
- BLND:USDC, BLND:XLM, and USDC each execute as both the selected tier and bid
  token. The USDC-selected case remains conserved when a reserve lot also uses
  the same USDC token.
- The `5:5:4` ratio controls base reserve-credit allocation; tests explicitly
  show that different fill times can produce a different realized donated-
  value ratio without invoking a protocol fee or corrective redistribution.
- Failure in authorization, quoting, commitment, transfer, accounting,
  synchronization, or exact-balance validation rolls back the credit,
  pending-lot, auction, commitment, tier-asset, and token state.
- Native and optimized-WASM tests exercise maximum four-asset creation,
  partial fill, complete fill, stale recovery, all three tier tokens, and
  resource use against SDK-27 mainnet invocation limits.

## Ongoing BLND-emission allocation

The v3 tests must verify:

- Every ongoing BLND amount uses an immutable 70% backstop-depositor and 30%
  pool supply/borrow split.
- The two floor-rounded tranches plus top-level carry exactly conserve the
  input, and the carry remains scoped to the next top-level split.
- The reward zone contains at most 30 registered pools.
- Entry requires both the 12,500-USDC threshold and strictly positive eligible
  underlying BLND. A USDC-only pool can activate but cannot enter the reward
  zone until BLND:USDC or BLND:XLM is deposited.
- An existing positive-weight member remains eligible at or above the
  10,000-USDC maintenance threshold.
- A zero-BLND member is permissionlessly removable despite remaining above the
  maintenance threshold and requires no distribution checkpoint.
- Permissionless full-zone replacement requires the entrant to have strictly
  more eligible underlying BLND than the named member.
- Multiple qualified pools can enter, and a positive-weight member below the
  maintenance threshold can be removed, before migration activation without a
  distribution checkpoint, when no ongoing candidate emissions can accrue.
- Every post-activation reward-zone edit after its first member requires a
  distribution checkpoint within the preceding hour except
  allocation-neutral zero-weight removal.
- The permissionless production distributor writes that checkpoint only after
  emitter reconciliation, every bounded pool allocation, carry update, and
  aggregate conservation check succeeds.
- Successful exact-delta backfill funding verifies the configured BLND token.
- If backfill has not been funded, the first successful ongoing distribution
  verifies a positive exact balance delta around its own emitter invocation
  before enabling accumulated-balance reconciliation. Prefunding a wrong
  configured token cannot bypass this proof, and failure leaves the emitter
  checkpoint unchanged.
- A migration qualified entirely by direct BLND:USDC transfers can activate
  with zero backfill weight. Backfill funding remains unavailable, but a later
  positive ongoing distribution verifies BLND and proceeds normally.
- Distribution excludes both the remaining migration-backfill reserve and
  previously accounted, unclaimed ongoing BLND.
- A direct emitter invocation followed by candidate distribution reconciles
  the accumulated actual receipt exactly once.
- A failed balance, BLND-binding, or timestamp check rolls back the emitter
  call and leaves the prior production checkpoint unchanged.
- Standalone distribution checkpoints enforce the v2-compatible five-second
  interval. Every active BLND:USDC or BLND:XLM amount mutation in a reward-zone
  pool requires a completed production checkpoint no more than five seconds old
  and fails before changing accounting when that checkpoint is stale.
- Stored Active, On-Ice, or Frozen status is not a separate distribution gate.
- Only reward-zone pools receive ongoing BLND.
- Both the backstop-depositor and pool supply/borrow tranches are allocated
  among reward-zone pools in proportion to eligible underlying BLND across
  BLND:USDC and BLND:XLM backstop positions.
- BLND held through either eligible tier has equal emission weight per
  underlying BLND.
- Plain USDC contributes zero BLND-emission weight.
- Within a pool, each user's allocation is proportional to the user's eligible
  underlying BLND across both eligible tiers.
- A pool or user with zero eligible underlying BLND receives zero.
- Pool weight cannot exceed total eligible underlying BLND across reward-zone
  pools, and user weight cannot exceed its pool's eligible underlying BLND.
- Negative values, invalid denominators, and checked-addition overflow are
  rejected; large proportional multiplication is handled with wide arithmetic.
- Pool-level and user-level integer remainders are conserved and carried
  forward only within the same allocation scope.
- Balance, LP-rate, and activation changes do not alter already allocated
  emissions; tests bound the unresolved interval that can observe a new weight
  to at most five seconds.
- A deposit into an existing reward-zone pool fails after a stale checkpoint,
  succeeds within five seconds of distribution, and affects later allocation.
- Queueing and dequeueing fail after a stale checkpoint and succeed within five
  seconds of distribution. Queued shares then receive zero ongoing BLND weight,
  and dequeueing restores that weight without changing prior allocations.
- Capital committed to a pending bad-debt lot remains eligible until an actual
  fill transfers tier assets. The fill requires a recent checkpoint, and only
  the amount transferred loses weight.
- Interest-auction gains require a recent checkpoint and increase active
  emission weight without changing prior allocations.
- Underlying BLND is floor-rounded from each Comet's current BLND reserve and
  total LP supply in the same invocation.
- Zero or negative LP supply, negative reserve/LP amounts, an LP amount above
  total supply, arithmetic overflow, and unregistered pools fail closed.
- Same-transaction Comet composition changes affect only the checkpoint that
  observes them; tests demonstrate this accepted manipulation exposure rather
  than rejecting it as stale.
- Direct LP-token transfers without candidate tier shares receive zero
  emission weight.
- Accrual and claims use bounded accumulator/checkpoint accounting.
- The pool's 70% allocation is frozen into BLND:USDC and BLND:XLM tier indexes
  using the same checkpoint's underlying-BLND composition. Later Comet changes
  do not rewrite that attribution.
- Tier indexes use \(10^{14}\) scaling with pool-and-tier index carry; user
  checkpoints retain sub-unit carry and conserve it within that user position.
- Deposits checkpoint before minting active shares so a late depositor cannot
  claim earlier BLND. Queue, dequeue, and withdrawal checkpoint before changing
  active eligibility, and queued shares earn nothing during the queued
  interval.
- One owner-authorized backstop claim checkpoints both eligible tiers for one
  pool, pays BLND to the selected recipient, updates pool and aggregate claimed
  accounting, and cannot include migration backfill or the 30% pool tranche.
- A second claim without new accrual fails, and failed authorization or token
  transfer rolls the complete claim back.
- The pool admin can configure positive relative weights among valid supply and
  debt token identifiers for the 30% tranche; disabled reserves are omitted and
  remaining weights renormalize as in v2.
- Matching the frozen setter, an empty configuration is accepted and duplicate
  identifiers use the last supplied weight. While the configuration is empty,
  `gulp` fails before consuming or reserving candidate emissions. More than 60
  configuration entries fail before checkpointing streams.
- Supply and borrow emissions remain separate from the backstop-depositor
  tranche and are claimable to an authorized user's chosen destination.
- A positive pool gulp is limited to once per 24 hours, rejects less than one
  BLND atomically as in v2, and moves the accrued 30% tranche into a
  pool-specific reservation without transferring BLND or creating an
  allowance.
- Reserve-token allocations stream for seven days. A later gulp checkpoints
  the existing index, carries the exact unvested amount, and restarts the
  seven-day horizon; pool, reserve-token, and user rounding carry remain
  isolated. The v2 reserve-decimal-derived index scale remains precise for
  large positions in every supported token precision.
- Supply, collateral, borrow, repay, liquidation, bad-debt handoff, supplier
  default, and reserve-configuration mutations checkpoint affected streams
  before changing balances or token supply. A halfway entrant cannot claim
  the first half of an existing stream.
- A reserve-token claim requires position-owner authorization, rejects
  duplicate or invalid identifiers, clears only the selected user accruals,
  and atomically consumes the registered pool's candidate reservation. The
  optimized maximum 60-identifier pool-to-candidate claim path remains within
  Protocol-27 mainnet invocation limits.
- A backstop claim equals the existing pool/user underlying-BLND formula and is
  not redirected or reweighted into one preferred tier.
- An optimized-WASM distribution over the maximum 30-pool reward zone remains
  within the Protocol-27 mainnet transaction-resource limits.
- An optimized-WASM eligible-tier deposit following a recent maximum-30-pool
  checkpoint also remains within those limits.
- An optimized-WASM owner claim following maximum-zone distribution remains
  within those limits and leaves the pool's 30% tranche untouched.

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
candidate, next designated token, and unlock time, the candidate retains a
strict legacy-token balance majority, and every deployed modeled pool and
lending oracle is renewed successfully. It MUST record its last-success
timestamp only after those mandatory renewals complete. Repeated monitoring
MUST retain earlier results and resource-cost evidence rather than overwrite
them.

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

The accelerated Protocol-27 harness MUST additionally establish supplier,
borrower, collateral, debt, and backstop-share positions in the optimized
SDK-22 v2 pool and backstop WASM before time-traveling through the production
31-day queue. Immediately before canonical finalization it MUST snapshot those
positions and relevant token balances, then prove that finalization changes
only the emitter's final time-accrued BLND mint to the incumbent. It MUST prove
that the next positive distribution goes only to v3 and that ordinary v2
repay, supply, and backstop-deposit operations remain usable afterward.

Public testnet should use independently issued test assets rather than relying
on the supply of official test BLND or USDC.

An additional isolated public-testnet lane MUST exercise the optimized
production valuation adapter with synthetic BLND and USDC, native Friendbot-
funded XLM, and freshly initialized real 80:20 BLND:USDC and BLND:XLM Comet
contracts. A bounded authenticated test SEP-40 fixture MAY provide controlled
BLND/USDC and XLM/USDC histories for this deployment-compatibility exercise,
but MUST be identified as non-production and MUST NOT count as provider
qualification. Its state directory and identities MUST remain independent of
any exact-clock testnet migration already in progress. The modeled Fixed v3
pool SHOULD serve as this lane's primary migration, verification, and
post-migration emission pool.

Every mutable test oracle deployed on public testnet MUST bind an immutable
runner administrator at construction and require that address's authorization
for fixture updates. Verification MUST compare each modeled v3 lending
oracle's reported administrator with the recorded isolated operator.

Modeled v2 reserve setup MUST be resumable across its non-atomic
`queue_set_reserve` / `set_reserve` sequence. If interruption occurs after the
queue transaction commits, the next run MUST finalize the existing unlocked
queue instead of submitting a duplicate queue.
