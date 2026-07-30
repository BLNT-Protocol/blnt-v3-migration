# Repository Instructions

These instructions apply to the entire repository.

## Mission

Build and evaluate migration from the incumbent Blend v2 protocol and deployed
emitter to the experimental v3 candidate while preserving demonstrable
cross-version compatibility.

Treat deployed v2 behavior as the default specification. V3 behavioral changes
require an explicit justification tied to the three-tier backstop, emitter
migration compatibility, Protocol 27 compatibility, a conservation/safety
fix, or a separately approved exception recorded in the v3 specification.
Carrying integer dust forward at the same accounting scope and the
12,500-USDC entry / 10,000-USDC maintenance hysteresis are approved exceptions.

This is financial smart-contract software. Prefer explicit invariants,
conservative state transitions, deterministic accounting, and tests over
implicit assumptions.

## Sources of truth

Read these before changing protocol behavior:

1. `docs/SYSTEM_SPEC.md`
2. `docs/MIGRATION_PROTOCOL_SPEC.md`
3. `blend-contracts-v3/docs/SYSTEM_SPEC.md`
4. `blend-contracts-v3/docs/V2_SYSTEM_SPEC.md`
5. `blend-contracts-v3/docs/V3_SYSTEM_SPEC.md`
6. `blend-contracts-v3/docs/BACKSTOP_VALUATION.md`
7. `docs/MIGRATION_ACCEPTANCE.md`
8. `docs/BASELINE.md`
9. Existing contract tests and the implementation being changed

If the documents conflict with deployed executable behavior, preserve the
behavior needed for migration and document the conflict. Do not silently
choose a new economic policy.

## Local source policy

Initialize the pinned deployed-v2 source after cloning:

```bash
git submodule update --init --recursive
```

- `blend-contracts-v2` is the authoritative SDK-22 source submodule.
- `blend-contracts-v3` is the sole production v3 contract source submodule.
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
- New v3 code belongs in the `blend-contracts-v3/` submodule. It MUST target
  Protocol 27 with Rust 1.91.1, Soroban SDK 27.0.3, and `wasm32v1-none`.
- Migration-only tests and fixtures belong in `migration-harness/` and MUST
  NOT become production contract dependencies.
- Do not add an SDK 26 compatibility lane. Migration compatibility is tested
  against the SDK-22 v2 contracts and deployed emitter behavior.
- Keep the incumbent v2 implementation available for side-by-side migration
  tests.
- Prefer new v3 crates or clearly isolated v3 modules over rewriting the only
  copy of an incumbent v2 contract.
- The deployed emitter is an external compatibility boundary. A candidate may
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
- Shares queued for withdrawal remain take-rate eligible until actually
  withdrawn, but are excluded immediately from activation value and ongoing
  BLND-emission weight. Queueing does not shield their redemption value from
  losses absorbed by their tier.
- Each user may have at most 20 queued-withdrawal entries in aggregate for one
  pool across all three tiers. FIFO withdrawal and LIFO dequeue remain
  tier-local.
- Assets counted as backstop capital must be available to the specified loss
  waterfall.
- Bad-debt auctions use at most one backstop tier at a time in strict
  BLND:USDC, BLND:XLM, then plain-USDC order. A tier is auction-eligible only
  when its available capital has at least 200 USDC of verified value; equality
  qualifies and there is no exception for a smaller tier that could cover the
  current debt. A successfully valued smaller tier is skipped, but a failed or
  unavailable valuation is not exhaustion. Supplier loss begins only after all
  three tiers are below that threshold. Once a lot is committed, later price
  movement across the threshold does not cancel it.
- A bad-debt fill transfers only its time-scaled tier-token lot to the
  authenticated filler and impairs the pool-tier exchange rate by exactly that
  transferred amount. The percentage-selected base lot, not the smaller
  time-scaled transfer, reduces the outstanding commitment. Pool positions,
  canonical backstop liabilities, the pool auction, the backstop commitment,
  the token transfer, and the exchange-rate impairment must succeed or roll
  back atomically.
- Bad-debt continuation is a separate permissionless bounded transaction and
  requires no active bad-debt auction. It restarts strict tier selection and
  must reuse an earlier tier while that tier remains eligible.
- Supplier loss is permitted only after the configured backstop successfully
  reports that all three tiers are below the $200 minimum. It must accrue each
  affected reserve and apply Blend v2's ceiling-rounded bToken exchange-rate
  loss with intentional saturation at zero; stale or unavailable valuation
  fails closed.
- Emitter migration qualification uses the candidate contract's direct balance
  of the incumbent emitter-designated token and requires a strictly greater
  balance.
- Do not introduce protocol governance, multisig control, protocol-wide
  administrator recovery, privileged WASM replacement, a protocol-wide
  emergency override, or an alternate upgrade path. Future protocol
  replacement must use the deployed emitter's permissionless queue and swap
  mechanism. V2-compatible transferable pool administration is pool-local and
  is not a protocol-upgrade path.
- A successful migration awards zero discretionary BLND. Any one-time BLND
  distributed in connection with migration must fund only the scheduled,
  time-weighted BLND:USDC backfill and must not exceed 10 million BLND.
- Pool-status and reward-zone threshold decisions must use the same canonical
  activation valuation. Stored pool status is not an additional reward-zone
  eligibility gate.
- Production LP value uses the immutable SEP-40 USDC-base TWAP backstop
  valuation and the lower of the 80%-BLND and 20%-paired-reserve implied Comet
  totals. Spot Comet BLND composition remains canonical only for
  ongoing-emission weight. Do not replace production backstop valuation with
  caller-supplied values or the deterministic test fixture.
- Candidate construction must match the backstop-valuation contract's
  immutable BLND, plain-USDC, BLND:USDC, and BLND:XLM binding exactly. Generic
  resource tests must model independent oracle tick storage; the selected
  provider's exact WASM and initialized layout still require predeployment
  simulation.
- Pool status preserves v2 codes and hard Q4W thresholds: normal On-Ice at
  30%, Admin Active On-Ice at 50%, normal Frozen at 60%, and Admin On-Ice
  Frozen at 75%. The transferable pool admin may select statuses 0, 2, and 4
  or relinquish an override into ordinary On-Ice status 3. It includes the
  bounded risk-on Admin Active override between the normal 30% and admin 50%
  Q4W limits and the v2 risk-off Admin On-Ice and Admin Frozen controls.
- Pool administration follows v2's two-step `propose_admin` / `accept_admin`
  lifecycle. The current admin may update the v2-compatible pool parameters,
  queue or cancel reserve configuration, and set relative supply/borrow
  emission weights. The admin cannot change the three backstop tiers,
  activation thresholds, `5:5:4` take-rate weighting, loss waterfall, migration
  policy, 70/30 emission split, contract WASM, or any protocol-wide rule.
- Preserve the v2-compatible reward-zone shape: at most 30 pools,
  permissionless threshold-qualified entry/removal/replacement, and no
  additional stored pool-status gate. V3 replacement weight uses eligible
  underlying BLND across active BLND:USDC and BLND:XLM tier positions; plain
  USDC and Q4W positions have zero emission weight.
- Preserve the v2 action matrix: On-Ice blocks borrowing and auction
  cancellation; Frozen additionally blocks both supply and collateral supply.
- Never involuntarily swap a depositor's BLND:USDC backstop asset. Conversion
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
make
make test
make test-v2-migration
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
- Keep every workspace lockfile committed and keep builds reproducible from a
  fresh clone with initialized submodules.
- Preserve AGPL-3.0 notices and document the provenance of imported code and
  frozen bytecode.
- Do not describe candidate code as audited, production-ready, or official.
- Do not commit, push, deploy, tag, publish, or alter external protocol state
  unless the user explicitly requests it.

## Specification discipline

Requirements marked `MUST` in the normative specifications listed by
`docs/SYSTEM_SPEC.md` and `blend-contracts-v3/docs/SYSTEM_SPEC.md` are
acceptance criteria under their respective composition rules.
Items listed under “Open decisions” are not implementation authorization.
When an open decision blocks safe implementation, update the specification or
ask for a decision before encoding policy.
