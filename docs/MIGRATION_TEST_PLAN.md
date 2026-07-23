# V2 to V3 Migration Test Plan

The emitter migration and an individual pool's activation rule are independent
mechanisms. Tests must keep those concepts separate.

## Legacy emitter migration

The test suite must verify:

- The candidate directly holds strictly more of the incumbent designated token
  than the incumbent backstop.
- Equality is insufficient.
- The comparison continues to use the old designated token throughout the
  queue period.
- A balance deficit permits cancellation and prevents finalization.
- Finalization cannot occur before the 31-day unlock time.
- Finalization moves BLND emissions from the incumbent to the candidate.
- The newly queued designated token is used for the next migration.
- The initial BLND drop and backfilled emissions cannot be claimed twice.

## Candidate backstop

The v3 tests must verify:

- BLND:USDC migration deposits are accounted for independently from any
  post-migration backstop asset.
- Depositors cannot be double-counted across migration, activation, emissions,
  or loss tiers.
- Queued withdrawals receive the intended activation and emission treatment.
- Legacy BLND:USDC depositors can reclaim or explicitly convert their
  positions without an involuntary asset swap.
- Each accepted asset has an explicit valuation, haircut, concentration limit,
  withdrawal policy, liquidation path, and loss-waterfall position.

## Pool activation

For a multi-asset activation formula, test:

- Each approved backstop asset independently.
- Combined deposits at, immediately below, and immediately above the threshold.
- Per-asset haircuts and minimum junior-capital requirements.
- Price, reserve, and LP-supply manipulation.
- Oracle staleness and unavailable-market behavior.
- Queued withdrawals and assets already committed to covering losses.
- Consistent results between pool status and reward-zone eligibility.

## Execution environments

1. Native Soroban `Env` tests for fast iteration and ledger time travel.
2. WASM tests for contract-interface and serialization compatibility.
3. Local Stellar Quickstart with testnet resource limits.
4. An isolated public-testnet deployment of an incumbent v2 stack and a v3
   candidate stack.

Public testnet should use independently issued test assets rather than relying
on the supply of official test BLND or USDC.
