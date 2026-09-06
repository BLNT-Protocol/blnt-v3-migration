# Blend V2-to-V3 Transition Acceptance

This document defines the evidence required to accept the dual-emitter BLND to
BLNT transition in
[MIGRATION_PROTOCOL_SPEC.md](MIGRATION_PROTOCOL_SPEC.md). Production contract
behavior remains governed by the pinned `blnt-contracts-v3` and
`blnt-backfill-contract` specifications.

## Contract-suite prerequisites

Acceptance requires:

1. the frozen SDK-22 v2 tests and deployed-artifact verification to pass;
2. the complete native and optimized-WASM v3 contract suite to pass;
3. the Comet v2 workspace build and tests to pass;
4. the v3 WASM-size and resource guards to pass; and
5. the Protocol-27 compatibility harness to pass without rebuilding deployed
   v2 or the legacy emitter as SDK-27 contracts.

## Independent emitters and tokens

The compatibility suite MUST prove that:

- the deployed legacy emitter remains configured for BLND and its v2 backstop;
- the v3 emitter is separately configured for BLNT and the v3 backstop;
- the BLND administrator is the legacy emitter and the BLNT administrator is
  the v3 emitter;
- elapsed legacy distribution mints only BLND to v2, while elapsed v3
  distribution mints only BLNT to v3;
- the v3 backstop activates on its first checkpoint without a legacy-emitter
  queue, backfill epoch, or 31-day delay; and
- no v3 constructor, pool, backstop tier, reward weight, claim, or buyback
  binding contains BLND or a BLND-bearing LP; and
- the v3 emitter constructor and public ABI contain no BLND conversion binding
  or entry point.

The accelerated v2 harness MUST establish supplier, borrower, collateral,
debt, and backstop positions in exact deployed SDK-22 contracts and prove they
remain unchanged and usable after the independent v3 lane is created.

## BLND-to-BLNT conversion

Native and optimized-WASM backfill-contract tests MUST cover:

- successful authorized 2-BLND-to-1-BLNT conversion in raw seven-decimal units;
- exact BLND receipt and immediate burn plus exact pre-funded BLNT transfer
  back to the same authorized address, with no BLNT mint or escrow;
- rejection of zero and negative amounts;
- rejection when BLND and BLNT bindings are identical or use incompatible
  decimals;
- conversion through the ledger immediately before the 270-day deadline,
  rejection at the deadline, and permissionless finalization;
- exact success at the 51-million-BLNT output cap and rejection above it;
- exact immediate BLND and expired BLNT-reserve burn accounting, idempotent
  zero-return behavior, and
  preservation of every outstanding backfill and grant claim after burning;
- BLNT-output, BLND-burn, and remaining-capacity conservation;
- atomic rollback on transfer, balance, burn, payout, cap, overflow, or
  reentrancy mismatch;
- authorized immutable address allocations totaling no more than 99 million
  BLNT, including duplicate, non-positive, count, and total bounds;
- zero backfill claimability at construction followed by per-second linear
  vesting through the exact full allocation over 180 days,
  repeated partial claims, zero-currently-claimable rejection, and indefinite
  post-vesting claims;
- independent grant allocations for at most 100 recipients and 25 million
  BLNT, linearly vested over 360 days through `claim_grant`, including an
  address present in both lists without cross-lane consumption or acceleration;
- exact conservation of the 74-million-BLNT production allocation across the
  434 positive `flattened_total_cpal_raw` snapshot rows (423 accounts and 11
  contracts), bound to the recorded source hash with deterministic
  largest-remainder rounding;
- exact aggregate conservation across the 74-million backfill, 25-million
  grant, and 51-million conversion lanes; and
- absence of any conversion refund path.

## Initial allocation

The suite MUST prove that the current v3 backstop can execute one initial BLNT
drop, an exact 150-million-BLNT launch total is valid, a larger launch total
fails, every later migration remains capped at 50 million BLNT, and neither
repetition nor re-registering the same backstop restores eligibility. The v3
launch harness MUST use exactly one recipient, verify that it is the immutable
backfill-contract address, and verify that contract receives exactly 150 million
BLNT. V3 launch MUST schedule no migration backfill.

## Comet v2 canonical LPs

Build and deployment evidence MUST identify the workspace's
`comet-contracts-v2` checkout as the Comet v2 source and record its commit and
optimized WASM hash. Tests and network verification MUST prove that canonical
v3 BLNT:USDC and BLNT:XLM pools:

- are deployed from that artifact;
- contain only the expected Stellar Asset Contracts;
- use exact normalized 80:20 weights and seven-decimal LP shares;
- begin unfrozen and satisfy the specified irreversible controller posture;
- have positive, authorized, non-clawbackable underlying custody; and
- are the only LPs accepted by the v3 backstop constructor.

## Future replacement compatibility

The v3 emitter suite MUST preserve the established queue ABI and prove:

- equality in current designated-token balances is insufficient;
- a candidate with one additional raw BLNT:XLM LP unit can queue;
- unlock is exactly 31 days after queue creation;
- finalization succeeds at exactly seven days after unlock and fails after
  that inclusive execution boundary;
- the candidate must retain its strict balance through finalization;
- before expiration, cancellation becomes available only after qualification
  is lost;
- after expiration, the existing cancellation entry point succeeds regardless
  of candidate balance and permits a new queue to be created;
- finalization distributes accrued BLNT to the incumbent before switching;
- a later qualified candidate can queue and finalize even if the current
  recipient has not called its backstop `distribute` entry point;
- the queued recipient and caller-selected next designated token are stored
  exactly; and
- the new recipient receives subsequent BLNT while the old recipient does not.

No test or deployment may add a privileged replacement route.

## Execution matrix

| Environment | Required evidence |
| --- | --- |
| Native Soroban `Env` | Conversion cap, address claims, direct activation, queue compatibility, conservation, and rollback |
| Optimized WASM | Public ABI, constructor encoding, serialization, cross-contract behavior, and resource limits |
| Protocol-27 compatibility harness | Exact deployed v2 and legacy emitter alongside independent optimized v3 emitter/backstop WASM |
| Local Quickstart | Reproducible dual-emitter deployment, Comet v2 bindings, initial activation, and recorded costs |
| Public testnet | Production transaction assembly, signer separation, artifact hashes, backfill custody and caps, and final bindings |

## Transition-ready gate

The design may be described as transition-ready only when every prerequisite
passes, legacy v2 positions remain isolated and operational, v3 direct BLNT
emissions and the exact initial backfill drop are demonstrated, perpetual
capped conversion and address claims are fully tested, future BLNT:XLM
replacement remains compatible, and an isolated local plus public-testnet
deployment records all final invariants.

Independent security review remains required before a mainnet deployment.
