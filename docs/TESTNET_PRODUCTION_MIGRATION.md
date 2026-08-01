# Comet-Valued Public-Testnet Migration

This isolated lane exercises the candidate's integrated Comet valuation with
native XLM and fresh Comet contracts on public testnet. Its state, keys,
transaction results, and cost evidence live below the ignored
`.testnet-production/` directory.

The ignored `.testnet-production/` state records the live run. After a fresh
deployment is verified, its public account, asset, contract, transaction,
timestamp, integrated pool-valuation, artifact, and provenance data SHOULD be
copied into a sanitized versioned registry. Signing material remains
exclusively in the ignored local Stellar configuration.

The runner creates:

- a fresh deployment of the exact deployed emitter WASM;
- synthetic test-only BLND and USDC Stellar assets;
- the native XLM Stellar asset contract, with transaction accounts funded by
  Friendbot;
- a real 80:20 BLND:USDC Comet initialized with 1,000 BLND and 25 USDC;
- a real 80:20 BLND:XLM Comet initialized with 1,000 BLND and 100 native XLM;
- bounded, authenticated, test-only SEP-40 lending oracles for the modeled
  pools;
- the optimized candidate backstop bound immutably to the three assets and
  both Comets;
- matched Fixed and YieldBlox v2/v3 lending pools; and
- three-tier candidate and single-tier incumbent backstop positions for both
  pool pairs.

The BLND:USDC Comet is the backstop's USDC anchor. The backstop uses its current
80:20 reserve ratio to value both LP tiers and does not call an oracle. The
controlled SEP-40 contracts are used only by the modeled lending pools for
reserve pricing.

## Start the isolated migration

Review and validate:

```bash
make testnet-production-plan
make testnet-production-validate
```

Deploy the complete incumbent and candidate stacks, create both Comets,
deploy and fund both modeled pool pairs, and then begin the unshortened 31-day
deployed-emitter queue:

```bash
make testnet-production-run
```

The runner refuses to overwrite an existing `.testnet-production/state.json`.

## Operate the queue

Query status and run the transaction-backed keepalive at least every five
days. The same command renews all modeled pool instances, reserve/position
entries, custom assets, and lending oracles:

```bash
make testnet-production-status
make testnet-production-keepalive
```

The keepalive refreshes the modeled pools' fixed-price oracle histories. To
refresh only those lending oracles before a pool diagnostic:

```bash
make testnet-production-refresh-oracle
```

Backstop activation and the migration lifecycle do not depend on those
controlled oracles. Modeled lending-pool operations can still fail closed when
their reserve prices are stale. During the final seven-day window and after
unlock:

```bash
make testnet-production-prepare
make testnet-production-finalize
make testnet-production-verify
make testnet-production-emissions
```

After the seven-day pool stream expires:

```bash
make testnet-production-claim-emissions
```

This lane proves deployment and transaction compatibility. Its controlled
lending oracles are test fixtures, not acceptable mainnet providers.

## Modeled lending pools

Every fresh deployment creates the side-by-side lending-pool fixtures whose
reserve parameters were captured from mainnet Fixed Pool v2 and YieldBlox Pool
v2. The setup finishes before `begin_migration()`, so both candidate pools are
prefunded before the backfill clock begins. If the pool portion of deployment
is interrupted, its idempotent setup can be resumed; verification may also be
repeated independently:

```bash
make testnet-production-deploy-modeled-pools
make testnet-production-verify-modeled-pools
```

Reserve initialization is resumable across the v2 two-transaction
`queue_set_reserve` / `set_reserve` boundary. For each absent reserve, the
runner first simulates `set_reserve`. An already queued and unlocked reserve is
finalized directly; otherwise the runner queues and then finalizes it. This
avoids v2's rejection of a duplicate queued reserve after an interrupted run.

The completed fixture has two SDK-22 v2 pools against the incumbent single
BLND:USDC backstop, plus equivalent SDK-27 v3 pools against the candidate's
BLND:XLM → BLND:USDC → USDC waterfall. Each matched v2/v3 pool receives
22,000 BLND:USDC LP. On a fresh deployment, Fixed v3 receives one additional
base unit to preserve the strict aggregate migration majority. Each v3 pool
also receives 10 BLND:XLM LP and 12,500 USDC.

The v2 balance exceeds the v2 activation formula at the controlled Comet
ratio. The runner sets both v2 pools to Admin Active, configures their 30% pool
tranche for USDC suppliers, seeds 1,000 USDC of supply per pool, adds both to
the incumbent reward zone, advances the deployed emitter, and starts the
seven-day emission streams. The large synthetic BLND issuance used to create
the activation LP is test-only setup capital, not an emitter distribution.
Rerunning the modeled-pool setup command renews a missing or expired stream;
standalone verification rejects a null, zero-rate, or expired stream instead
of reporting it as emitting.

The v3 pools use distinct 30% pool-tranche configurations. Fixed v3 directs
the tranche to EURC borrowers; its deterministic exercise supplies 500 EURC
of liquidity, supplies 1,000 USDC as collateral, and borrows 100 EURC.
YieldBlox v3 directs the tranche to USDC suppliers and is seeded with 1,000
USDC. Both Fixed v3 and YieldBlox v3 enter the candidate reward zone during
prefunding. Before migration activation, no candidate emissions can accrue,
so qualified pools may enter without a distribution checkpoint. After
activation, later weight-bearing reward-zone edits retain the one-hour
checkpoint guard. Consequently, the first
`make testnet-production-emissions` transaction after migration considers both
pools for the 70% backstop and 30% pool tranches. The exercise gulps and later
claims the Fixed EURC-borrow and YieldBlox USDC-supply streams independently.
It also claims each pool's two backstop-tier accruals separately and compounds
the BLND into the originating BLND:USDC or BLND:XLM Comet position.

The controlled lending-oracle fixtures use seven-decimal USD prices. Their
price setters require authorization from the isolated runner operator bound at
construction, and verification checks that immutable
binding. Other public-testnet accounts cannot rewrite the fixtures. They make
the pools usable for functional test transactions but are not suitable price
sources for production. Synthetic EURC, AQUA, USDGLO, USTRY, CETES, and PYUSD
are issued only by the isolated runner identity. The exact captured pool
configuration and source contract IDs are recorded in
[`../fixtures/testnet-modeled-pools.json`](../fixtures/testnet-modeled-pools.json).

## Live deployment state

Query the live network and next required operation from the ignored deployment
state with:

```bash
make testnet-production-status
```

After verification, publish a sanitized registry under `deployments/` without
any signing material or local identity names. The verified valuation record is
read directly from the candidate backstop.
