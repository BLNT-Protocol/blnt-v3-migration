# Blend V3 Workspace

All new Blend v3 contract code belongs in this workspace.

The workspace targets Stellar Protocol 27 and pins:

- Rust 1.91.0
- Soroban SDK 27.0.2
- `wasm32v1-none`

The workspace contains:

- `backstop`: the first isolated v3 migration-lifecycle contract.
- `compat-probe`: a minimal SDK-27 caller for the historical v2 backstop.
- `compat-tests`: Protocol-27 WASM tests covering the SDK-22 boundary and the
  complete legacy-emitter swap into optimized v3 backstop WASM.

The v3 backstop currently implements migration configuration, two verified
retry queues within an immutable 76-day recovery horizon, emitter finalization,
bounded per-queue direct-swap synchronization, migration deposits and
withdrawals, deterministic 1-BLND-per-second backfill scheduling from the
original queue start, a bounded 31-day prefunding campaign, and time-weighted
survivor-allocation quotes. It intentionally does not yet implement BLND
funding or claims, activation policy, loss tiers, or ongoing-emission
allocation.

The contract extends its code and instance TTL to 90 days at construction and
renews it whenever fewer than 89 days remain. This does not change the
emitter's 31-day migration delay.

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
