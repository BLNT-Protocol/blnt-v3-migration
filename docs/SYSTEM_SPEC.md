# Blend V3 Migration System Specification

This document defines the scope and authority of the Blend v2-to-v3 migration,
compatibility, deployment, and evidence repository.

## Repository boundary

This repository owns:

- the pinned frozen v2 source and deployed-WASM compatibility fixtures;
- the pinned `blnt-contracts-v3` production-contract submodule;
- the pinned `blnt-backfill-contract` production transition-contract
  submodule;
- the pinned SDK and UI client-integration source submodules;
- cross-version compatibility and migration-only test contracts;
- localnet and public-testnet migration orchestration;
- integrated Comet V1.1-derived backstop valuation evidence; and
- reproducible deployment manifests and resource evidence.

Contract behavior and economic policy are owned by the contract specification
set composed by
[`blnt-contracts-v3/docs/SYSTEM_SPEC.md`](../blnt-contracts-v3/docs/SYSTEM_SPEC.md).
This repository MUST NOT redefine that behavior through a script, fixture,
deployment default, or test expectation.

## Migration specification set

1. [MIGRATION_PROTOCOL_SPEC.md](MIGRATION_PROTOCOL_SPEC.md) defines the
   normative dual-emitter topology, BLND-to-BLNT conversion, direct v3 launch,
   incumbent isolation, Comet V1.1 LPs, and future replacement behavior.
2. [BASELINE.md](BASELINE.md) defines the frozen v2 source, deployed artifacts,
   toolchain separation, and submodule provenance.
3. [MIGRATION_ACCEPTANCE.md](MIGRATION_ACCEPTANCE.md) defines the normative
   cross-version and network acceptance criteria.

Localnet and testnet documents are operational runbooks; they must satisfy
this specification set but do not authorize protocol behavior.

Declarative requirements and the terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY`
in the normative migration documents are acceptance criteria.

## Source and artifact authority

The exact deployed v2 and legacy-emitter WASMs under `deployed-artifacts/mainnet`
are the legacy compatibility boundary. The pinned `blend-contracts-v2`
submodule provides the frozen SDK-22 source and test baseline. The v3 emitter
is built from the pinned `blnt-contracts-v3` submodule.

The pinned `blnt-contracts-v3` submodule is the production v3 protocol
contract source. The pinned `blnt-backfill-contract` submodule is the sole
production source for initial BLNT address claims and legacy-token conversion.
Migration tests and runners MUST build and invoke both artifacts directly.
They MUST NOT maintain a second production implementation or copy either
contract source into this repository.

The upstream `comet-contracts-v1` submodule MUST remain pinned to the
`v1.1.0` tag at commit
`b3a2d827ed6c158341f8e67ea91eb3b0395d3871`. Canonical V3 liquidity MUST use
the official `v1.1.0_contracts_pkg1.0.0_cli25.1.0` Comet release WASM with
SHA-256 `d735c3395f59510172cf5cf838823a1389b97cfec6bf24e580e9bd77d2b3e687`.
Migration runners MUST reject a different source pin or artifact and MUST NOT
substitute a local rebuild.

Migration-only contracts and cross-version tests live under
`migration-harness/`. They MAY provide deterministic or controlled fixtures,
but those fixtures MUST be clearly identified as test-only and MUST NOT be
represented as production dependencies.

## Conflict resolution

Apply the following order:

1. Deployed v2 and legacy-emitter bytecode governs legacy compatibility facts.
2. This transition specification and the pinned backfill-contract
   specification govern independent emitters, initial distribution,
   conversion, direct v3 activation, future replacement, orchestration,
   evidence, and acceptance.
3. The contract repository's composed specification governs all other v3
   contract behavior.

If a migration test conflicts with either executable boundary, correct the
test or document a specification conflict. Do not silently change production
contract behavior or substitute rebuilt v2 bytecode.

Updating either submodule pin, a frozen artifact, or a network assumption is
an explicit reviewed change. Successful testnet execution is evidence, not
production approval.

## Backstop custody deployment gate

The deployment runner MUST establish Fixed Pool v3 before any other v3 pool
and use its first plain-USDC backstop deposit to create the candidate's shared
USDC SAC balance. It MUST then read the Protocol-27 persistent balance entries
for that custody and for both canonical Comet V1.1 pools' issuer-controlled underlying
assets. Every entry MUST exist, be positive and authorized, and record
`clawback = false`. A local issued-XLM fixture MUST satisfy the same check for
both Comet V1.1 and backstop custody; native XLM requires no issuer check.

The complete modeled-pool verification MUST repeat the checks before v3 is
declared ready. Any missing, malformed, deauthorized, zero, or clawbackable
entry invalidates the deployment. This is evidence for the
contract specification's non-clawbackable-custody requirement, not a new
contract entry point or substitute for lending-reserve admission policy.

Both canonical Comet V1.1 pools MUST use the same dedicated classic-account controller.
After both Comet V1.1 pools are initialized unfrozen, the runner MUST atomically set that
account's low, medium, and high thresholds to 100 and its only signer weight to
zero, matching the canonical mainnet BLND:USDC Comet v1 pool. It MUST then verify each
Comet V1.1 pool's exact token pair, 80:20 weights, controller address, and the controller
account's thresholds and signer weights. A failed check invalidates the
deployment, and successful locking makes later freeze or controller changes
unfulfillable.

## Open deployment decisions

The following production-readiness decisions do not authorize contract or
economic-policy changes:

- Define the production lending-reserve admission policy for issuer controls,
  clawback, deauthorization, rebasing, and other nonstandard token behavior.
