# Repository Instructions

These instructions apply to the entire repository.

## Mission

Build and evaluate an experimental Blend Protocol v3 candidate without losing
the ability to demonstrate compatibility with the deployed legacy emitter and
the incumbent v2 protocol.

This is financial smart-contract software. Prefer explicit invariants,
conservative state transitions, deterministic accounting, and tests over
implicit assumptions.

## Sources of truth

Read these before changing protocol behavior:

1. `docs/SYSTEM_SPEC.md`
2. `docs/MIGRATION_TEST_PLAN.md`
3. `docs/BASELINE.md`
4. Existing contract tests and the implementation being changed

If the documents conflict with executable legacy behavior, preserve the legacy
behavior needed for migration and document the conflict. Do not silently choose
a new economic policy.

## Local source policy

Prefer local source over repeated network fetches:

- `../blend-contracts-v2` is the clean upstream v2 checkout.
- `../blend-contracts` contains the legacy emitter source and `v1.0.0` tag.
- `../blend-utils` contains deployment and interaction scripts.
- `../xycloans` contains the pinned flash-loan dependency source.
- `vendor/moderc3156` is the reproducible interface used by this workspace.

Do not replace a vendored or local dependency with a Git dependency unless the
change is intentional and documented. Do not modify sibling repositories as a
side effect of work in this repository.

## Baseline architecture

- The repository root is the frozen incumbent v2 workspace. It MUST remain on
  Rust 1.81 and Soroban SDK 22.0.7.
- New v3 code belongs in the independent `v3/` workspace. It MUST target
  Protocol 27 with Rust 1.91.0, Soroban SDK 27.0.2, and `wasm32v1-none`.
- Do not add an SDK 26 compatibility lane. Migration compatibility is tested
  against the SDK-22 v2 contracts and deployed emitter behavior.
- Keep the incumbent v2 implementation available for side-by-side migration
  tests.
- Prefer new v3 crates or clearly isolated v3 modules over rewriting the only
  copy of an incumbent v2 contract.
- The legacy emitter is an external compatibility boundary. A candidate may
  interact with it but must not assume it can rewrite or upgrade it.
- Pool activation, reward eligibility, loss absorption, emission allocation,
  and emitter migration are separate mechanisms. Do not conflate their
  accounting.

## Critical invariants

- Never disable release overflow checks.
- Do not use floating-point arithmetic. Use explicit fixed-point scales,
  checked arithmetic, or intentional saturation with a written justification.
- Failed operations must not leave partially updated accounting.
- A token or share must not be double-counted across activation, migration,
  emissions, queued withdrawals, or loss absorption.
- Assets counted as backstop capital must be available to the specified loss
  waterfall.
- Legacy migration qualification uses the candidate contract's direct balance
  of the incumbent emitter-designated token and requires a strictly greater
  balance.
- An activation-policy change must produce consistent pool-status and
  reward-eligibility results from one canonical calculation.
- Never involuntarily swap a depositor's legacy backstop asset. Conversion
  requires explicit opt-in.
- Bound loops and ledger footprints. Critical operations must remain executable
  under Soroban transaction resource limits.

## Development workflow

Before editing:

- Inspect `git status` and preserve unrelated user changes.
- Identify the affected invariant and the tests that currently enforce it.
- Record unresolved economic or governance choices rather than guessing them.

For contract changes:

1. Add or update a regression test.
2. Test the native Soroban implementation.
3. Build optimized WASM and test the WASM path.
4. Exercise boundary values, authorization failures, overflow, stale prices,
   queued withdrawals, and resource growth where applicable.

Useful commands:

```bash
cargo fmt --all -- --check
cargo test -p test-suites --test test_v2_to_v3_candidate
cargo +1.91.0 check --manifest-path v3/Cargo.toml --all-targets --locked
make
make test
make check-v3
make test-all
```

Run the targeted test during iteration. Run `make test` before handing off a
change to core accounting, migration, authorization, auctions, pool status, or
emissions.

## Repository hygiene

- Use `apply_patch` for authored file changes.
- Do not commit `target/`, generated test snapshots, secrets, private keys, or
  environment files.
- Keep `Cargo.lock` committed and keep builds reproducible from a fresh clone.
- Preserve AGPL-3.0 notices and document the provenance of vendored code.
- Do not describe candidate code as audited, production-ready, or official.
- Do not commit, push, deploy, tag, publish, or alter external protocol state
  unless the user explicitly requests it.

## Specification discipline

Requirements marked `MUST` in `docs/SYSTEM_SPEC.md` are acceptance criteria.
Items listed under “Open decisions” are not implementation authorization.
When an open decision blocks safe implementation, update the specification or
ask for a decision before encoding policy.
