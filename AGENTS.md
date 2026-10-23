# Repository Instructions

These instructions apply to the entire repository.

## Mission

Build and evaluate migration from the incumbent Blend v2 protocol and deployed
emitter to the experimental v3 candidate while preserving demonstrable
cross-version compatibility.

Treat deployed v2 behavior as the default specification. V3 behavioral changes
require an explicit justification tied to the configurable backstop waterfall, emitter
migration compatibility, Protocol 27 compatibility, a conservation/safety
fix, or a separately approved exception recorded in the v3 specification.
Carrying integer dust forward at the same accounting scope and the
12,500-USDC activation threshold are approved exceptions.

This is financial smart-contract software. Prefer explicit invariants,
conservative state transitions, deterministic accounting, and tests over
implicit assumptions.

## Sources of truth

Read these before changing protocol behavior:

1. `docs/SYSTEM_SPEC.md`
2. `docs/MIGRATION_PROTOCOL_SPEC.md`
3. `blnt-contracts-v3/docs/SYSTEM_SPEC.md`
4. `blnt-contracts-v3/docs/V2_SYSTEM_SPEC.md`
5. `blnt-contracts-v3/docs/V3_SYSTEM_SPEC.md`
6. `docs/MIGRATION_ACCEPTANCE.md`
7. `docs/BASELINE.md`
8. Existing contract tests and the implementation being changed

If the documents conflict with deployed executable behavior, preserve the
behavior needed for migration and document the conflict. Do not silently
choose a new economic policy.

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
- New v3 code belongs in the `blnt-contracts-v3/` submodule. It MUST target
  Protocol 27 with Rust 1.91.1, Soroban SDK 27.0.3, and `wasm32v1-none`.
- Comet V1.1 source review belongs in the `comet-contracts-v1/` submodule.
  Do not modify the upstream source or replace the official
  `v1.1.0_contracts_pkg1.0.0_cli25.1.0` release artifact used by V3.
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
  pool across all configured tiers. FIFO withdrawal and LIFO dequeue remain
  tier-local.
- Only authorization-aware transferable value counts as backstop capital.
  Deauthorized plain USDC has zero activation, status, reward-zone, take-rate,
  auction, and supplier-loss value until reauthorized; its shares, queues, and
  pending interest remain accounted for.
- Bad-debt auctions use at most one backstop tier at a time in the pool's
  immutable configured order. Every tier with positive
  available capital and verified value is auction-eligible; zero-value and
  deauthorized plain-USDC tiers are skipped, while negative, inconsistent, or
  failed valuation is not exhaustion. Supplier loss begins only after all
  configured tiers have no usable transferable value. Once a lot is committed,
  later price movement does not cancel it, but issuer deauthorization permits
  immediate permissionless cancellation so selection can continue.
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
  reports that all configured tiers have no usable value. It must accrue each
  affected reserve and apply Blend v2's ceiling-rounded bToken exchange-rate
  loss with intentional saturation at zero; invalid or unavailable valuation
  fails closed.
- The sole exception is reconciliation of an unexplained lending-reserve
  custody deficit. That reserve absorbs the deficit through unpaid protocol
  credit, its supplier rate, and then its unpaid take-rate credit, without creating backstop debt or
  touching another reserve. Only subsequent borrower bad debt follows the
  configured waterfall. Complete supplier-value exhaustion sets the bToken
  exchange rate exactly to zero. New supply, collateral supply, borrowing, and
  flash loans stop below a 0.1 b_rate and resume once accrued interest restores
  at least that rate. Zero b_supply alone is a normal empty-reserve state and
  permits the first supply while b_rate remains at least 0.1; inherited
  liquidity and health checks continue to govern other actions. When both rate
  and supply are zero, new risk remains disabled but existing liabilities
  continue accruing on the ordinary 100%-utilization curve; all resulting
  interest and positive custody surplus are credited to the backstop because
  no supplier denominator remains. Repayment and liquidation remain available.
- A deauthorized lending reserve is quarantined without being written off:
  nonzero custody transfers fail atomically, allowance-based zero-net internal
  adjustments remain possible, collateral has zero effective health value,
  liabilities and interest continue, internal liquidation positions remain
  transferable, and unauctionable take-rate credit stays pending until
  reauthorization. Authorization revocation alone is not a custody deficit.
- Emitter migration qualification uses the candidate contract's direct balance
  of the incumbent emitter-designated token and requires a strictly greater
  balance.
- Do not introduce protocol governance, multisig control, protocol-wide
  administrator recovery, privileged WASM replacement, a protocol-wide
  emergency override, or an alternate upgrade path. Future protocol
  replacement must use the deployed emitter's permissionless queue and swap
  mechanism. V2-compatible transferable pool administration is pool-local and
  is not a protocol-upgrade path.
- The v3 constructor binds an immutable initial-drop recipient list. Its
  allocation plus the maximum 10-million-BLND migration backfill must fit
  within the emitter's 50-million-BLND ceiling. Recipient selection is a
  deployment decision; migration test lanes use an empty list. Backfill uses
  a v2-style queue-independent clock opened by the first pre-replacement
  `distribute`, the ordinary 70/30 split, and before migration only BLND:USDC
  is eligible for its backstop tranche.
- The candidate must attest a compatible BLND:XLM emitter queue no earlier
  than its final seven days and activate within seven days after unlock.
- Pool-status and reward-zone threshold decisions must use the same canonical
  activation valuation. Stored pool status is not an additional reward-zone
  eligibility gate.
- Backstop value uses current reserves from the immutable BLND:USDC and
  BLND:XLM 80:20 Comet V1.1 pools. BLND:USDC is the USDC anchor, authorized plain USDC
  is valued one-for-one, and plain XLM is priced by the BLND reserve ratio
  between the two Comet V1.1 pools. Do not introduce a backstop price oracle or caller-
  supplied production values.
- Every positive borrower-interest accrual reserves an immutable 0.2% of gross
  interest as separate protocol credit before applying the pool take rate.
  Protocol credit is sold for BLNT and the exact bid burned in an independent
  type-3 auction; one type-2 and one type-3 auction may coexist. Every type-2
  bid is donated in full, and no legacy buy-and-burn path or accounting remains.
- Candidate construction must validate its immutable BLND, USDC, XLM,
  BLND:USDC, and BLND:XLM bindings exactly.
- Pool status preserves v2 codes and hard Q4W thresholds: normal On-Ice at
  30%, Admin Active On-Ice at 50%, normal Frozen at 60%, and Admin On-Ice
  Frozen at 75%. The transferable pool admin may select statuses 0, 2, and 4
  or relinquish an override into ordinary On-Ice status 3. It includes the
  bounded risk-on Admin Active override between the normal 30% and admin 50%
  Q4W limits and the v2 risk-off Admin On-Ice and Admin Frozen controls.
- Pool administration follows v2's two-step `propose_admin` / `accept_admin`
  lifecycle. The current admin may update the v2-compatible pool parameters,
  queue or cancel reserve configuration, and set relative supply/borrow
  emission weights. The admin cannot change the pool's one-to-three backstop
  tier configuration, activation threshold, configured take-rate weights, loss
  waterfall, migration
  policy, 70/30 emission split, contract WASM, or any protocol-wide rule.
- Each pool factory deployment immutably selects one to three unique assets
  from canonical BLND:XLM LP, BLND:USDC LP, USDC, and XLM and assigns integer
  take-rate weights from 1 through 100 independently of tier order. Only the
  two LP assets are BLND-emission eligible; plain USDC and XLM otherwise use
  the same full-donation interest settlement as the LP tiers.
- Preserve the v2-compatible reward-zone shape: at most 30 pools,
  permissionless threshold-qualified entry/removal/replacement, and no
  additional stored pool-status gate. V3 replacement weight uses eligible
  underlying BLND across active BLND:USDC and BLND:XLM tier positions; plain
  USDC and Q4W positions have zero emission weight.
- Backfill and ongoing 70% allocations use v2-style seven-day pool gulp
  streams. BLND:USDC accrual compounds into BLND:USDC, and post-migration
  BLND:XLM accrual compounds into BLND:XLM. The LP shares return to the same
  user's active position in the same pool; already streamed accrual is never
  forfeited.
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
`docs/SYSTEM_SPEC.md` and `blnt-contracts-v3/docs/SYSTEM_SPEC.md` are
acceptance criteria under their respective composition rules.
Items listed under “Open decisions” are not implementation authorization.
When an open decision blocks safe implementation, update the specification or
ask for a decision before encoding policy.
