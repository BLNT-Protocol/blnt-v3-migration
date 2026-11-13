# Repository Instructions

These instructions apply to the entire repository.

## Mission

Maintain migration, compatibility, deployment, and evidence tooling for BLNT
V3, an immutable lending protocol derived from Blend and designed to support
real-world assets, while preserving demonstrable cross-version compatibility.

Treat the canonical BLNT V3 production WASMs and their source, dependency,
toolchain, and metadata pins as frozen. This repository may evolve migration,
verification, compatibility, operational, and deployment-evidence tooling
without changing those artifacts. Any contract-behavior change must target a
separately identified future protocol version and migration path; it must not
be described as a patch to BLNT V3.

This is financial smart-contract software. Prefer explicit invariants,
conservative state transitions, deterministic accounting, and tests over
implicit assumptions.

## Sources of truth

Read these before changing migration, compatibility, deployment behavior, or
documentation that describes protocol behavior:

1. `docs/SYSTEM_SPEC.md`
2. `docs/MIGRATION_PROTOCOL_SPEC.md`
3. `blnt-contracts-v3/docs/SYSTEM_SPEC.md`
4. `blnt-contracts-v3/docs/V2_SYSTEM_SPEC.md`
5. `blnt-contracts-v3/docs/V3_SYSTEM_SPEC.md`
6. `docs/MIGRATION_ACCEPTANCE.md`
7. `docs/BASELINE.md`
8. Existing contract tests and the implementation being changed

If documentation conflicts with the exact deployed artifacts, preserve the
deployed artifacts and correct the documentation. Do not silently choose a new
economic policy or rebuild a different binary under the BLNT V3 identity.

## Local source policy

Initialize the pinned source repositories after cloning:

```bash
git submodule update --init --recursive
```

- `blend-contracts-v2` is the authoritative SDK-22 source submodule.
- `blnt-contracts-v3` is the sole production v3 contract source submodule.
- `comet-contracts-v1` is the upstream Comet V1.1 source submodule pinned to
  tag `v1.1.0` commit `b3a2d827ed6c158341f8e67ea91eb3b0395d3871`.
  Deployment uses the official release WASM embedded at
  `blnt-contracts-v3/comet.wasm`, not a locally rebuilt substitute.
- `blnt-sdk-js` and `blnt-ui` are pinned client-integration source
  submodules.
- `backstop-manager` is the pinned YieldBlent backstop-management source
  submodule. Its production WASM MUST carry the canonical
  `source_repo=github:BLNT-Protocol/backstop-manager` metadata.
- `blnt-lockup-backfill-contract` is the pinned, separately deployed
  contributor-funded remediation source for pre-incident Blend lockup
  positions. It MUST remain isolated from immutable V3 protocol behavior.
- `blnt-backstop-manager-helper` is the pinned, separately deployed atomic
  orchestration source for owner-authorized YieldBlent deposits and
  two-tier emissions claims. It MUST remain isolated from immutable V3 protocol
  behavior.
- `auctioneer-bot` is the pinned off-chain auction automation submodule. Its
  mainnet example configuration SHOULD monitor the reviewed Fixed Pool V3 and
  YieldBlent Pool deployments by default, and it MUST remain outside every
  contract build lane.
- `deployed-artifacts/mainnet` contains exact deployed mainnet WASM compatibility
  fixtures, verified by `make verify-deployed-artifacts`.
- `migration-harness` contains cross-version tests and test-only support
  contracts.
- `../blend-utils` remains an optional local deployment-script reference.

The submodule URLs use GitHub SSH. Do not edit a submodule, advance a pinned
commit, replace frozen artifacts, or fetch a different v2 source as
an incidental part of migration work. Do not modify sibling repositories as
a side effect of work in this repository.

## Baseline architecture

- `blend-contracts-v2` is the frozen incumbent v2 source workspace. It
  MUST remain on Rust 1.81 and Soroban SDK 22.0.7.
- `deployed-artifacts/mainnet` is the authoritative bytecode boundary for compatibility
  tests and isolated migration deployments. A local source rebuild MUST NOT be
  silently substituted for those files.
- `blnt-contracts-v3/` contains the public, frozen production source for the
  deployed BLNT V3 WASMs. Its Rust 1.91.1, Soroban SDK 27.0.3, Protocol 27,
  and `wasm32v1-none` build inputs MUST reproduce the recorded artifacts.
  Do not modify production source or pins in a way that changes those hashes.
- YieldBlent backstop-management code belongs in the `backstop-manager/`
  submodule and uses the Protocol-27 Rust 1.91.1 / Soroban SDK 27.0.3 build
  lane.
- Lockup-position remediation code belongs in the
  `blnt-lockup-backfill-contract/` submodule and MUST NOT become a dependency
  of the immutable V3 protocol contracts.
- Two-tier backstop-manager orchestration code belongs in the
  `blnt-backstop-manager-helper/` submodule and MUST NOT become a dependency of
  the immutable V3 protocol contracts or change the manager's authority model.
- Auction automation belongs in the `auctioneer-bot/` submodule and MUST NOT
  become a production contract dependency or alter any deployed WASM hash.
- Comet V1.1 source review belongs in the `comet-contracts-v1/` submodule.
  Do not modify the upstream source or replace the official
  `v1.1.0_contracts_pkg1.0.0_cli25.1.0` release artifact used by V3.
- Migration-only tests and fixtures belong in `migration-harness/` and MUST
  NOT become production contract dependencies.
- Do not add an SDK 26 compatibility lane. Migration compatibility is tested
  against the SDK-22 v2 contracts and deployed emitter behavior.
- Keep the incumbent v2 implementation available for side-by-side migration
  tests.
- A future protocol implementation must be separately named, versioned, and
  reviewed; it must not replace or rewrite the frozen source identified as
  BLNT V3.
- The deployed legacy BLND emitter and the separate BLNT V3 emitter are
  external compatibility boundaries. Migration tooling may verify and invoke
  their published interfaces but must not assume either can be rewritten or
  upgraded.
- Pool activation, reward eligibility, loss absorption, emission allocation,
  and emitter migration are separate mechanisms. Do not conflate their
  accounting.

## Migration and evidence invariants

Contract-level behavior is defined by the composed specification and the
instructions inside `blnt-contracts-v3/`; do not duplicate those rules here.
For work in this parent repository:

- Treat the exact deployed mainnet WASMs, public source commits, build inputs,
  constructor bindings, contract IDs, transaction hashes, and artifact hashes
  recorded by the canonical deployment registries as one verification unit.
- Do not substitute a local rebuild for an attested release artifact or
  describe an artifact as canonical when its source, metadata, hash, or
  constructor bindings differ from the deployment record.
- Keep legacy Blend V1/V2 and BLND state separate from BLNT V3. The legacy
  emitter remains authoritative for V1/V2; BLNT V3 launched with its own BLNT
  emitter and does not replace or depend on the legacy BLND emitter.
- Describe the deployed V3 asset topology exactly: canonical BLNT, USDC, and
  XLM assets; canonical 80:20 BLNT:USDC and BLNT:XLM Comet V1.1 pools; and no
  BLND-bearing V3 backstop tier.
- Preserve the direct-launch allocation evidence. The V3 emitter sent its
  150-million-BLNT initial drop to the immutable backfill contract: 74 million
  for pre-incident claims, 25 million for contributor grants, and 51 million
  for the capped 1:1 legacy-BLND conversion reserve.
- Keep future emitter migration semantics distinct from the initial launch.
  The deployed BLNT emitter retains its permissionless queue-and-swap upgrade
  path; migration tests must not rewrite current deployment history.
- Keep private signing material, account configuration, and uncommitted
  constructor manifests out of version control. Public allocation results,
  source repositories, deployment registries, and reproducibility evidence
  must not be described as private.
- Keep off-chain automation, remediation contracts, management contracts, and
  helpers isolated from the immutable V3 core. Their deployment and authority
  records must remain independently reviewable.
- Bound loops and ledger footprints in migration and verification tooling.
  Failed submitted operations must not be reported as completed, and reruns
  must preserve or explicitly reconcile existing network state.

## Development workflow

Before editing:

- Inspect `git status` and preserve unrelated user changes.
- Identify the affected invariant and the tests that currently enforce it.
- Record unresolved economic or governance choices rather than guessing them.

For migration, compatibility, verification, and deployment-tooling changes:

1. Add or update a regression test.
2. Exercise the affected localnet or read-only verification path.
3. Confirm that no canonical deployed artifact or pinned production-source
   commit changed unintentionally.
4. Exercise retry, partial-state, authorization, timeout, and resource-boundary
   behavior where applicable.

If a task requires different production contract behavior, follow the child
repository instructions and target a separately identified future protocol
version. Do not modify frozen BLNT V3 production code through this parent
repository.

Before planning or performing any production contract deployment, follow every
applicable gate in
[`docs/CONTRACT_DEPLOYMENT_READINESS.md`](docs/CONTRACT_DEPLOYMENT_READINESS.md).
A project-built production WASM MUST carry canonical `source_repo` metadata,
MUST be the exact artifact produced and attested by its tagged release workflow,
and MUST be pinned by this repository before deployment. A local rebuild is not
a substitute for the attested release artifact. Do not describe a contract as
ready to deploy until the source, release, attestation, constructor-binding,
and deployment-evidence gates pass.

Useful commands:

```bash
cargo fmt --all -- --check
make
make test
make test-v2-migration
make check-v3
make test-all
```

Run the targeted test during iteration. Run the applicable repository-wide
checks before handing off a migration, compatibility, artifact-verification,
or deployment-tooling change.

## Repository hygiene

- Use `apply_patch` for authored file changes.
- Do not commit `target/`, generated test snapshots, secrets, private keys, or
  environment files.
- Keep every workspace lockfile committed and keep builds reproducible from a
  fresh clone with initialized submodules.
- Preserve AGPL-3.0 notices and document the provenance of imported code and
  frozen bytecode.
- Do not describe BLNT V3 as independently audited, production-ready, or an
  official Blend release.
- Do not commit, push, deploy, tag, publish, or alter external protocol state
  unless the user explicitly requests it.

## Specification discipline

Requirements marked `MUST` in the normative specifications listed by
`docs/SYSTEM_SPEC.md` and `blnt-contracts-v3/docs/SYSTEM_SPEC.md` are
acceptance criteria under their respective composition rules.
Items listed under “Open decisions” are not implementation authorization.
When an open decision blocks safe implementation, update the specification or
ask for a decision before encoding policy.
