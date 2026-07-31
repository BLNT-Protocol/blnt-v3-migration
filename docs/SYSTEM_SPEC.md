# Blend V3 Migration System Specification

This document defines the scope and authority of the Blend v2-to-v3 migration,
compatibility, deployment, and evidence repository.

## Repository boundary

This repository owns:

- the pinned frozen v2 source and deployed-WASM compatibility fixtures;
- the pinned `blend-contracts-v3` production-contract submodule;
- cross-version compatibility and migration-only test contracts;
- localnet and public-testnet migration orchestration;
- integrated Comet-derived backstop valuation evidence; and
- reproducible deployment manifests and resource evidence.

Contract behavior and economic policy are owned by the contract specification
set composed by
[`blend-contracts-v3/docs/SYSTEM_SPEC.md`](../blend-contracts-v3/docs/SYSTEM_SPEC.md).
This repository MUST NOT redefine that behavior through a script, fixture,
deployment default, or test expectation.

## Migration specification set

1. [MIGRATION_PROTOCOL_SPEC.md](MIGRATION_PROTOCOL_SPEC.md) defines the
   normative emitter-replacement lifecycle, candidate migration accounting,
   backfill, incumbent isolation, and recovery behavior.
2. [BASELINE.md](BASELINE.md) defines the frozen v2 source, deployed artifacts,
   toolchain separation, and submodule provenance.
3. [MIGRATION_ACCEPTANCE.md](MIGRATION_ACCEPTANCE.md) defines the normative
   cross-version and network acceptance criteria.

Localnet and testnet documents are operational runbooks; they must satisfy
this specification set but do not authorize protocol behavior.

Declarative requirements and the terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY`
in the normative migration documents are acceptance criteria.

## Source and artifact authority

The exact deployed v2 and deployed-emitter WASMs under `deployed-artifacts/mainnet` are the
runtime migration boundary. The pinned `blend-contracts-v2` submodule
provides the frozen SDK-22 source and test baseline.

The pinned `blend-contracts-v3` submodule is the sole production v3 contract
source. Migration tests and runners MUST build and invoke its artifacts
directly. They MUST NOT maintain a second production implementation or copy
contract source into this repository.

Migration-only contracts and cross-version tests live under
`migration-harness/`. They MAY provide deterministic or controlled fixtures,
but those fixtures MUST be clearly identified as test-only and MUST NOT be
represented as production dependencies.

## Conflict resolution

Apply the following order:

1. Deployed v2 and deployed-emitter bytecode governs compatibility facts.
2. This migration specification governs emitter replacement, migration
   accounting, backfill, recovery, orchestration, evidence, and acceptance.
3. The contract repository's composed specification governs all other v3
   contract behavior.

If a migration test conflicts with either executable boundary, correct the
test or document a specification conflict. Do not silently change production
contract behavior or substitute rebuilt v2 bytecode.

Updating either submodule pin, a frozen artifact, or a network assumption is
an explicit reviewed change. Successful testnet execution is evidence, not
production approval.

## Open deployment decisions

The following production-readiness decisions do not authorize contract or
economic-policy changes:

- Define the production lending-reserve admission policy for issuer controls,
  clawback, deauthorization, rebasing, and other nonstandard token behavior.
