# Blend V3 Workspace

All new Blend v3 contract code belongs in this workspace.

The workspace targets Stellar Protocol 27 and pins:

- Rust 1.91.0
- Soroban SDK 27.0.2
- `wasm32v1-none`

The workspace contains:

- `backstop`: the first isolated v3 migration-lifecycle contract.
- `pool`: the Protocol-27 pool boundary for canonical backstop-loss state.
- `pool-factory`: an immutable Protocol-27 factory that deploys and registers
  pools against one candidate backstop.
- `compat-probe`: a minimal SDK-27 caller for the historical v2 backstop.
- `compat-tests`: Protocol-27 WASM tests covering the SDK-22 boundary and the
  complete legacy-emitter swap into optimized v3 backstop WASM.

The repository-level local-network runner deploys the published legacy emitter,
a coherent frozen SDK-22 incumbent stack, independently issued local assets,
and the SDK-27 candidate stack on Protocol-27 Quickstart with testnet resource
limits. It preserves the production 31-day queue and exposes resumable
deployment, status, preparation, finalization, and verification commands.
See `docs/LOCALNET_MIGRATION.md`.

The v3 backstop currently implements migration configuration, two verified
retry queues within an immutable 76-day recovery horizon, emitter finalization,
bounded per-queue direct-swap synchronization, continuous BLND:USDC tier
deposits and withdrawals, deterministic 1-BLND-per-second backfill scheduling
from the original queue start, a bounded 31-day prefunding campaign, and
time-weighted survivor-allocation quotes. The same accounted BLND:USDC shares
exist before and after migration; migration freezes only their temporary
backfill weight, and later deposits receive no retroactive backfill. It
stores one immutable v3 pool-factory address, validates both the factory
interface and its immutable binding to this candidate during construction, and
accepts accounted deposits in all three tiers only for factory-registered
pools that expose the required loss-state callback.
Validation occurs before token custody or share accounting. The callback is a
permanent compatibility boundary that factory-approved pool upgrades must
preserve. After activation, any caller can fund the exact scheduled backfill
through the published emitter's one-time drop. The immutable BLND token binding
is used to verify the exact balance delta. Position owners can then direct one
floor-rounded claim per frozen position without iteration; remaining integer
dust stays segregated in the backfill reserve. The backstop can reserve and
settle a single-tier bad-debt auction by transferring the time-scaled tier
token directly to the filler and impairing the pool-tier exchange rate by
exactly that transferred amount. It intentionally does not yet implement
user-liquidation bad-debt handoff, interest auctions, or ongoing-emission
accrual. The pool implements next-step bad-debt continuation as a separate
permissionless transaction: it either commits the next eligible strict-tier
lot or, after a verified no-eligible-tier quote, reduces affected reserves'
bToken exchange rates using Blend v2's supplier-default arithmetic.
Each pool and tier stores separate accounted assets and shares. Deposits mint
shares with floor rounding at the current exchange rate, withdrawals redeem
assets with floor rounding, and the final share sweeps residual conversion
dust. Active and queued shares use the same rate, so a pool-local tier
impairment affects both proportionally without depositor iteration. Drained
tiers reject deposits until their zero-value shares are burned, and direct
token transfers remain outside accounted assets. BLND:USDC migration backfill
tracks actual contributed LP principal separately from tier shares once the
exchange rate differs from one.
All three tier withdrawals use independent bounded 17-day queues with at most
20 entries per user, pool, and tier, FIFO withdrawal, and LIFO dequeue.
Withdrawal also requires an affirmative, read-only response from the attributed
pool's `backstop_withdrawal_allowed` interface; missing, failed, and false
responses block the transfer. Queued shares remain in custody and retain
migration-backfill eligibility until actual withdrawal. Active-share queries
exclude them from the raw active-share view. Canonical activation converts that
view to redeemable assets, and ongoing-emission integration will use the same
separation. Queued shares remain take-rate eligible until actual withdrawal,
and remain proportionally exposed to tier losses through the same exchange
rate as active shares, matching the inherited v2 economics. Total accounted
in-custody shares are the canonical take-rate and within-tier loss-allocation
input. BLND:XLM and plain USDC now have independent user, pool, queued, active,
and global share accounting. Both may be deposited before or after migration;
neither qualifies the incumbent emitter or earns migration backfill. Canonical
pool valuation converts their active and queued shares to redeemable assets at
the common pool-tier rate before applying USDC or LP valuation.

The v3 pool stores immutable lending and reserve configuration, v2-compatible
reserve interest accounting, bToken and dToken positions, oracle-valued health
checks, and atomic supply, withdraw, collateral-supply, collateral-withdraw,
borrow, and repay requests. The immutable pool-local operator has no lending
configuration authority. The request path requires spender authorization and,
when distinct, position-owner authorization; a repayment at or above live debt
clears the position and returns excess input to the designated recipient.
Stored status is enforced before accounting mutations. The lending price
boundary requires positive, unexpired quotes at the pool's immutable decimal
scale. The included adapter is deterministic test infrastructure, not a
production oracle.

The v3 pool also stores bounded canonical record maps for backstop liabilities,
committed losses or auctions, and unresolved bad debt. It derives aggregate
counts directly from those maps without combining reserve-denominated amounts.
Each map permits at most 30 records. Reserve-keyed amounts reject negatives,
remove zeroes, and preserve their count on replacement. Committed losses use
presence-only auction identifiers because multi-asset auctions have no single
amount unit. The withdrawal callback returns true only for its immutable
backstop and only when all three maps are empty. Missing records trap rather
than being treated as clear. There is no administrator-controlled withdrawal
override. Ordinary borrow and repay operations synchronize the configured
backstop's liability record atomically. Bad-debt fills move their scaled dToken
bid from the backstop to a healthy authenticated filler and synchronize those
same records atomically. Permissionless bad-debt continuation either prepares
the next strict-tier auction or clears all residual canonical liabilities
atomically while applying their asset loss to suppliers. Future
interest-auction entry points must update the other records with their real
protocol transitions. The pool now implements permissionless v2-compatible
user-liquidation creation and request-type-6 fills, including the inherited
400-ledger auction curve, partial fills, status-gated request-type-9
cancellation, and permissionless stale deletion. Active auctions block ordinary
liquidatee submissions. Until user-liquidation bad-debt handoff exists, a fill
that would leave collateral-free debt or worsen the liquidatee's effective
health fails atomically. The pool can prepare one v2-shaped bad-debt auction
for liabilities already assigned to its backstop. Preparation atomically
commits a
single-tier, 120%-of-debt lot from the first tier with at least $200 of verified
value in strict BLND:USDC LP, BLND:XLM LP, then plain-USDC order. A smaller tier
is skipped without a solve-the-debt exception. Only the registered pool can
commit or release the lot. The reservation is excluded from activation and
later quotes without transferring tokens, burning shares, or changing an
exchange rate. The pool auction and backstop commitment share one fixed 46-day
temporary lifetime, and permissionless stale deletion after 500 ledgers
releases the commitment. A permissionless authenticated filler may assume the
time-scaled dToken bid and receive the selected tier token directly. Partial
fills remove the percentage-selected base lot from the reservation while only
the time-scaled transfer impairs active and queued shares. The pool and
backstop update atomically, including token-transfer failure. Once no auction
is active, any caller can continue resolution. A qualifying tier creates the
next auction; a verified result that all tiers are below $200 defaults every
remaining backstop liability through the affected reserves' supplier exchange
rates. User-liquidation bad-debt handoff and interest auctions remain
unimplemented, so the current pool is not yet a complete lending market.

The v3 pool factory stores immutable backstop and pool-WASM configuration,
deploys pools with explicit immutable operators, lending configuration, and
ordered reserve configuration, and maintains the registry consumed by the
backstop deposit path. Compatibility tests exercise the deterministic
candidate-address deployment order needed to configure the factory before the
candidate constructor validates it.

The backstop exposes the canonical Milestone-5 activation arithmetic for
verified, pool-specific values of BLND:XLM LP, BLND:USDC LP, and plain USDC.
Each USDC of eligible value counts equally. An
inactive pool qualifies at or above 12,500 USDC, while an active pool remains
qualified at or above 10,000 USDC. The public entry point returns a quote, and
stored pool-status transitions use the same canonical adapter boundary. A
production valuation adapter and loss-eligible position accounting are still
required; untrusted caller inputs or instantaneous Comet reserve ratios must
not be used as authoritative values.

The backstop also exposes non-authoritative arithmetic for the v2-compatible
pool statuses `0` through `6`. Multi-asset Q4W percentage uses verified USDC
value and rounds upward to seven decimals. Normal status becomes On-Ice at 30%
Q4W and Frozen at 60%; Admin Active becomes On-Ice at 50%, and Admin On-Ice
becomes Frozen at 75%. Admin modes 0, 2, and 4 are bounded pool-local operator
controls, not protocol governance; the operator may also relinquish an override
into ordinary On-Ice status 3. Admin Active is a bounded risk-on override that
can re-enable borrowing from 30% through immediately below 50% Q4W when the
activation threshold is met. Admin On-Ice and Admin Frozen are risk-off
controls. On-Ice blocks borrowing and auction cancellation. Frozen additionally
blocks both supply and collateral supply, matching the v2 action matrix. This
status authority is the sole continuing privileged operating-authority
exception and cannot alter protocol-wide policy, migration, configuration,
withdrawal safety, or WASM. Status storage, operator authorization, canonical
valuation, and enforcement for the implemented lending requests are now
integrated; future auction entry points must enforce the same predicate.

The pool-local operator is immutable and has no rotation or recovery path. A
compromised operator can always select Admin Frozen; if the operator is
unavailable while Admin Frozen is selected, permissionless refresh cannot
release the pool and it remains frozen indefinitely. Repayment, ordinary safe
withdrawals, and auction fills remain available under the v2 Frozen action
matrix.

The loss-waterfall auction boundary selects at most one of BLND:USDC LP,
BLND:XLM LP, and plain USDC in that order. A tier must have at least $200 of
verified value to be auctioned; suppliers absorb a residual only after
all three tiers are below that operational minimum or otherwise exhausted. The
implemented fill transfers the selected token directly to a healthy filler;
the protocol performs no swap. Only the actual time-scaled transfer impairs
the tier exchange rate, including active and queued shares proportionally. The
tiers have take-rate weights `2.5,2.5,2`, represented exactly on-chain as
`5:5:4`; each tier's allocation is proportional to its reward-eligible USDC
value multiplied by its weight. This priority and weighting do not change
equal-USDC activation accounting. Permissionless next-tier sequencing and
v2-style supplier residual settlement are implemented as a separate bounded
continuation call. Take-rate accrual remains implementation work.

The backstop also exposes arithmetic-only ongoing BLND-emission quotes for the
backstop-depositor tranche. Active pools are weighted by eligible underlying
BLND across BLND:USDC and BLND:XLM positions, and users within each pool are
weighted by their underlying BLND across those same tiers. Plain USDC receives
no BLND-emission weight. Production LP accounting, activation integration,
accumulators, and claims remain implementation work; the top-level split
between backstop and supply/borrow emissions remains an open economic decision.

The contract extends its code and instance TTL to 90 days at construction and
renews it whenever fewer than 89 days remain. This does not change the
emitter's 31-day migration delay.

The candidate has no protocol-governance, multisig, administrator-recovery,
protocol-wide emergency override, or WASM-upgrade path. Its only continuing
privileged operating-authority exception is the bounded pool-local
v2-compatible status authority described above. Protocol replacement uses only
the legacy emitter's permissionless, economically gated queue and swap
mechanism. Deployment and transaction-submission identities retain no authority
over the deployed candidate. User authorization is still required for actions
that move that user's assets or shares.

Successful migration awards zero discretionary BLND. The candidate schedules
only the time-weighted BLND:USDC backfill, capped at 10 million BLND, and the
otherwise available allocation of up to 40 million BLND remains undistributed.
`fund_backfill()` is permissionless after activation but can invoke the
emitter's initial drop only once, for the exact schedule, and only to the
candidate. `claim_backfill()` requires the position owner's authorization.
Funded, claimed, and remaining-reserve totals are public; rounding dust cannot
be redirected or treated as ongoing emissions.

## Migration operations

Create the original migration queue by calling
`candidate.begin_migration()`. It atomically calls the legacy emitter's
`queue_swap_backstop` and records the queue's accounting epoch. If another
caller creates the correct emitter queue directly, call
`candidate.open_migration_epoch()` immediately as the permissionless recovery
path. `prepare_migration()` will not silently adopt an unrecorded queue.

The legacy emitter does not retain canceled queue history. An unobserved queue
that is canceled cannot be reconstructed later, so operators must monitor
direct queue creation and open its epoch before it can be replaced. Regardless
of queue observation or retries, the candidate's immutable absolute migration
deadline is no later than 138 days after construction. Canonical finalization
rejects after the original queue's recovery horizon.

After a verified emitter queue unlocks, call
`candidate.finalize_migration()`. This is the canonical path: the candidate
invokes the legacy emitter's `swap_backstop`, verifies the emitter transition,
and activates its own lifecycle in one atomic contract invocation.

Call `candidate.sync_migration()` only if another caller invokes the legacy
emitter's permissionless `swap_backstop` directly before candidate
finalization. Synchronize immediately after detecting the transition. The
deadline is seven days after the verified queue's unlock time, not seven days
after the direct swap. A late direct swap therefore has less than seven days
remaining.

Once the direct emitter swap succeeds, later migration-position withdrawals or
loss of the candidate's strictly-greater legacy-token balance do not prevent
synchronization. If synchronization is not invoked before the fixed deadline,
the emitter can remain pointed at a candidate whose lifecycle is still
`Prepared`. This migration design addresses that case through operational
monitoring rather than a governance-authorized recovery mechanism.

The historical v2 contracts remain in the repository root and continue to use
their SDK-22 toolchain. Do not move v3 code into the root workspace or upgrade
the root dependency graph.
