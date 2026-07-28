# Blend V3 Candidate Implementation Status

Status: Experimental candidate

This document records implementation and deployment progress. It is
non-normative and does not authorize protocol behavior. Normative
requirements are defined by [SYSTEM_SPEC.md](SYSTEM_SPEC.md),
[V2_SYSTEM_SPEC.md](V2_SYSTEM_SPEC.md), and
[V3_SYSTEM_SPEC.md](V3_SYSTEM_SPEC.md).

## Initial implementation milestones

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
10. Implement the immutable ongoing-emission 70/30 split, the v2-compatible
    maximum-30 reward zone, canonical current Comet-composition reads for
    BLND:USDC and BLND:XLM, mutable admin-selected 30% supply/borrow weights,
    and bounded pool/user accrual and claims.

Milestones 1 through 5, the bounded-Q4W and canonical-loss-state-boundary
portions of milestone 6, the reproducible local and public-testnet runners for
the queue-open portion of milestone 7, and milestones 8 and 9 are
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
Protocol-27 pool factory, transferable per-pool admin and stored status,
canonical permissionless and admin status transitions, v2-compatible pool
parameter updates and timelocked reserve configuration, the v2-compatible
action predicate, the six
non-auction v2 request types through both direct and allowance submissions,
the v2 zero-fee flash-loan callback path, a Protocol-27 60-request safety
bound, v2-compatible reserve interest and share accounting, fail-closed
oracle-valued health checks, v2-compatible
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
The local runner and isolated public-testnet wrappers deploy independently
issued assets, a coherent
SDK-22 incumbent stack, and the SDK-27 candidate stack under testnet resource
limits while recording source commits, artifact digests, transaction results,
and cost evidence. The deployable backstop spec omits prose documentation from
the WASM target while retaining it in native Rust documentation, and the build
enforces a conservative 120,000-byte optimized-WASM ceiling after deployment
testing found that the earlier 126,044-byte artifact exceeded Protocol-27
testnet CPU limits during upload. A completed wall-clock local run and the
time-gated completion of both public-testnet exercises remain in progress.
The deterministic public-testnet lane has completed its queue-open deployment,
and its unshortened legacy-emitter queue unlocks on 2026-08-27. The separate
production-adapter public-testnet lane uses native XLM, synthetic BLND and
USDC, fresh real 80:20 BLND:USDC and BLND:XLM Comets, a bounded test-only
SEP-40 oracle, and the optimized production valuation adapter. Its two live
adapter quotes and exact Comet bindings passed. The current production fixture
was freshly redeployed with Fixed v3 as its primary migration-verification and
post-migration emission pool from construction, with YieldBlox v3 registered
alongside it, and its unshortened queue unlocks on 2026-08-28. This controlled
fixture is deployment evidence, not provider qualification.

A fail-closed, evidence-producing provider qualification, deployment, and
four-quote simulation lane is implemented, but no concrete
production-provider SEP-40 deployment has passed it: the official Reflector
public Stellar-DEX feeds checked on 2026-07-27 lack the exact BLND asset, and
the testnet mirror also lacks the expected native-XLM asset.
Concrete-provider approval and the final atomic candidate resource simulation
against that provider's exact WASM and initialized storage therefore remain
intentionally incomplete. The production LP valuation contract, immutable
TWAP policy, conservative Comet formula, and a generic optimized-WASM
regression with 25 independently stored ticks per asset are implemented.

Milestone 10's policy arithmetic, admin and pending-loss policies, canonical
spot-composition read boundary, permissionless maximum-30 reward-zone
membership, emitter reconciliation, pool-level accrual and carry, production
distribution checkpoint writer, two-tier per-share indexes, per-user backstop
checkpoints, and owner-authorized backstop claims are implemented. The 30%
path now includes v2-compatible admin-selected reserve-token weights,
once-daily pool reservation, seven-day supply/debt streams, mutation-time
checkpoints, scoped carry, and owner-authorized claims paid directly from
candidate custody.

New economic features require a separately approved specification. Remaining
production-readiness decisions do not authorize changes to the settled
candidate economics.
