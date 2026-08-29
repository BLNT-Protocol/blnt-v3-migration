# Blend V2-to-V3 Transition Acceptance

This document defines the evidence required to accept the dual-emitter BLND to
BLNT transition in
[MIGRATION_PROTOCOL_SPEC.md](MIGRATION_PROTOCOL_SPEC.md). Production contract
behavior remains governed by the pinned `blend-contracts-v3` specification.

## Contract-suite prerequisites

Acceptance requires:

1. the frozen SDK-22 v2 tests and deployed-artifact verification to pass;
2. the complete native and optimized-WASM v3 contract suite to pass;
3. the Comet V2 workspace build and tests to pass;
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
  binding contains BLND or a BLND-bearing LP.

The accelerated v2 harness MUST establish supplier, borrower, collateral,
debt, and backstop positions in exact deployed SDK-22 contracts and prove they
remain unchanged and usable after the independent v3 lane is created.

## BLND-to-BLNT conversion

Native and optimized-WASM tests MUST cover:

- successful authorized 1:1 conversion in raw seven-decimal units;
- exact BLND receipt and complete burn, leaving no received BLND in the
  emitter;
- exact BLNT mint to an independently selected recipient;
- rejection of zero and negative amounts;
- rejection when BLND and BLNT bindings are identical, use incompatible
  decimals, or BLNT is not administered by the emitter;
- rejection at the exclusive deadline and success one second before it;
- immutable deadline calculation from the constructor ledger timestamp;
- cumulative swapped-total conservation and overflow failure;
- atomic rollback on transfer, balance, burn, mint, or reentrancy mismatch;
  and
- absence of any BLNT-to-BLND entry point.

## Initial allocation

The suite MUST prove that the current v3 backstop can execute one initial BLNT
drop, an empty list is valid, an exact 50-million-BLNT total is valid, a larger
total fails, and neither repetition nor re-registering the same backstop
restores eligibility. V3 launch MUST schedule no migration backfill.

## Comet V2 canonical LPs

Build and deployment evidence MUST identify the workspace's historically named
`comet-contracts-v1` checkout as the Comet V2 source and record its commit and
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
- a candidate with one additional raw BLNT:USDC LP unit can queue;
- unlock is exactly 31 days after queue creation;
- the candidate must retain its strict balance through finalization;
- cancellation becomes available only after qualification is lost;
- finalization distributes accrued BLNT to the incumbent before switching;
- the queued recipient and caller-selected next designated token are stored
  exactly; and
- the new recipient receives subsequent BLNT while the old recipient does not.

No test or deployment may add a privileged replacement route.

## Execution matrix

| Environment | Required evidence |
| --- | --- |
| Native Soroban `Env` | Conversion boundaries, direct activation, queue compatibility, conservation, and rollback |
| Optimized WASM | Public ABI, constructor encoding, serialization, cross-contract behavior, and resource limits |
| Protocol-27 compatibility harness | Exact deployed v2 and legacy emitter alongside independent optimized v3 emitter/backstop WASM |
| Local Quickstart | Reproducible dual-emitter deployment, Comet V2 bindings, initial activation, and recorded costs |
| Public testnet | Production transaction assembly, signer separation, artifact hashes, conversion deadline, and final bindings |

## Transition-ready gate

The design may be described as transition-ready only when every prerequisite
passes, legacy v2 positions remain isolated and operational, v3 direct BLNT
emissions and optional initial drop are demonstrated, the 60-day conversion is
fully boundary-tested, future BLNT:USDC replacement remains compatible, and an
isolated local plus public-testnet deployment records all final invariants.

Independent security review remains required before a mainnet deployment.
