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
accepts BLND:USDC deposits only for
factory-registered pools that expose the required loss-state callback.
Validation occurs before token custody or share accounting. The callback is a
permanent compatibility boundary that factory-approved pool upgrades must
preserve. It
intentionally does not yet implement BLND funding or claims, authoritative
pool-status activation, loss execution, or ongoing-emission accrual.
BLND:USDC withdrawals use a bounded 17-day queue
with at most 20 entries per user and pool, FIFO withdrawal, and LIFO dequeue.
Withdrawal also requires an affirmative, read-only response from the attributed
pool's `backstop_withdrawal_allowed` interface; missing, failed, and false
responses block the transfer. Queued shares remain in custody and retain
migration-backfill eligibility until actual withdrawal. Active-share queries
exclude them for use by the future canonical activation and ongoing-emission
integrations. Queued shares remain take-rate eligible until actual withdrawal,
and remain proportionally exposed to tier losses through the same exchange
rate as active shares, matching the inherited v2 economics. Total accounted
in-custody shares are the canonical take-rate and within-tier loss-allocation
input. The workspace also provides one independently configured SEP-41
prefunding lane with exact one-token/one-share accounting.
Prefund shares are isolated from migration and may be deposited or withdrawn
before or after activation; they do not yet count toward activation, losses,
backfill, or emissions.

The v3 pool stores bounded canonical record maps for backstop liabilities,
committed losses or auctions, and unresolved bad debt. It derives aggregate
counts directly from those maps without combining reserve-denominated amounts.
Each map permits at most 30 records. Reserve-keyed amounts reject negatives,
remove zeroes, and preserve their count on replacement. Committed losses use
presence-only auction identifiers because multi-asset auctions have no single
amount unit. The withdrawal callback returns true only for its immutable
backstop and only when all three maps are empty. Missing records trap rather
than being treated as clear. There is no administrator-controlled withdrawal
override. The mutation boundary is internal: future lending, auction, and
waterfall entry points must update these records atomically with real protocol
transitions. Those engines are not yet implemented, so the current pool is an
integration boundary rather than a complete lending pool.

The v3 pool factory stores immutable backstop and pool-WASM configuration,
deploys pools with that backstop, and maintains the registry consumed by the
backstop deposit path. Compatibility tests exercise the deterministic
candidate-address deployment order needed to configure the factory before the
candidate constructor validates it.

The backstop exposes the canonical Milestone-5 activation arithmetic for
verified, pool-specific values of BLND:XLM LP, BLND:USDC LP, plain USDC, and up
to two optional custom assets. Each USDC of eligible value counts equally. An
inactive pool qualifies at or above 12,500 USDC, while an active pool remains
qualified at or above 10,000 USDC. The public entry point returns a
non-authoritative quote. This arithmetic is not yet wired to pool status: a
production valuation adapter and loss-eligible position accounting are still
required, and untrusted caller inputs or instantaneous Comet reserve ratios
must not be used as authoritative values.

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
withdrawal safety, or WASM. The production pool-status storage, operator
authorization, action enforcement, and canonical valuation integration remain
implementation work.

The pool-local operator is immutable and has no rotation or recovery path. A
compromised operator can always select Admin Frozen; if the operator is
unavailable while Admin Frozen is selected, permissionless refresh cannot
release the pool and it remains frozen indefinitely. Repayment, ordinary safe
withdrawals, and auction fills remain available under the v2 Frozen action
matrix.

The specified, but not yet implemented, loss waterfall consumes BLND:USDC LP,
BLND:XLM LP, plain USDC, optional Custom A, and optional Custom B in that order.
Pool suppliers absorb only a remaining shortfall after all configured,
eligible backstop tiers are exhausted. The five tiers have take-rate weights
`2.5,2.5,2,2,1`, represented exactly on-chain as `5:5:4:4:2`; each tier's
allocation is proportional to its reward-eligible USDC value multiplied by its
weight. This priority and weighting do not change equal-USDC activation
accounting. Per-tier auction, take-rate accrual, and disposal mechanics remain
implementation work.

The backstop also exposes arithmetic-only ongoing BLND-emission quotes for the
backstop-depositor tranche. Active pools are weighted by eligible underlying
BLND across BLND:USDC and BLND:XLM positions, and users within each pool are
weighted by their underlying BLND across those same tiers. Plain USDC and the
custom tiers receive no BLND-emission weight. Production LP accounting,
activation integration, accumulators, and claims remain implementation work;
the top-level split between backstop and supply/borrow emissions remains an
open economic decision.

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
Backfill funding and claims remain implementation work and MUST NOT introduce
a separate migration award.

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
