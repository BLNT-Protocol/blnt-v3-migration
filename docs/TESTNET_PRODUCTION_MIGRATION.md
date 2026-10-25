# Legacy queued-emitter public-testnet regression lane

> This is not the active v3 launch design. It is retained to exercise the old
> deployed-emitter replacement and backfill path. Use
> [DUAL_EMITTER_DEPLOYMENT.md](DUAL_EMITTER_DEPLOYMENT.md) for the independent
> BLND/BLNT deployment.

This isolated lane exercises the candidate's integrated Comet V1.1 valuation with
native XLM and fresh Comet V1.1 contracts on public testnet. Its state, keys,
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
- a real 80:20 BLND:USDC Comet V1.1 pool initialized with 1,000 BLND and 25 USDC;
- a real 80:20 BLND:XLM Comet V1.1 pool initialized with 1,000 BLND and 100 native XLM;
- a dedicated controller shared by both Comet V1.1 pools, irreversibly locked at
  100/100/100 thresholds with every signer weight zero;
- bounded, authenticated, test-only SEP-40 lending oracles for the modeled
  pools;
- the optimized candidate backstop bound immutably to the canonical assets and
  both Comet V1.1 pools;
- matched Fixed and Custom v2/v3 lending pools;
- Fixed v3 BLND:XLM, BLND:USDC, and USDC tiers with 4:3:2 weights; and
- Custom v3 XLM and USDC tiers with 2:1 weights, plus single-tier incumbent
  backstop positions for both v2 pools.

Fixed v3 is deployed and receives the first plain-USDC backstop deposit before
Custom v3 is created. That transfer establishes the shared candidate USDC SAC
balance. The runner then reads its persistent Protocol-27 balance value and the
issuer-controlled balances held by both Comet V1.1 pools, and rejects the deployment
unless every entry is positive, currently authorized, and permanently marked
non-clawbackable. Native XLM has no issuer clawback. Complete modeled-pool
verification repeats this proof before the emitter queue opens.

The runner initializes both Comet V1.1 pools unfrozen, locks their dedicated controller,
verifies each exact token pair and 80:20 weights, and only then performs the
depositor LP joins. Any controller mismatch, nonzero signer weight, or threshold
other than 100 invalidates the lane. This reproduces the canonical mainnet
BLND:USDC Comet v1 pool's controller posture so neither testnet Comet V1.1 pool can later be
frozen or assigned a replacement controller.

The BLND:USDC Comet V1.1 pool is the backstop's USDC anchor. The backstop uses current
reserves from both 80:20 Comet V1.1 pools to value the LP assets and plain XLM, values
plain USDC one-for-one, and does not call an oracle. The controlled SEP-40
contracts are used only by the modeled lending pools for reserve pricing.

## Start the isolated migration

Review and validate:

```bash
make testnet-production-plan
make testnet-production-validate
```

Deploy the complete incumbent and candidate stacks, create and lock both Comet V1.1 pools,
deploy and fund both modeled pool pairs, verify their non-clawbackable backstop
custody, and then begin the unshortened 31-day deployed-emitter queue:

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
their reserve prices are stale. During the final seven days, then at or after
unlock within the seven-day activation grace period:

```bash
make testnet-production-prepare
make testnet-production-finalize
make testnet-production-verify
make testnet-production-emissions
```

## Run the emissions keeper

Soroban contracts do not execute on their own. The current dual-emitter keeper
advances the legacy BLND emitter and v2 backstop independently from the BLNT
emitter and v3 backstop. It then calls `gulp_emissions()` on every corresponding
reward-zone modeled pool. This keeps v2 BLND and v3 BLNT emissions moving
without coupling either lane to an emitter migration.
Before the v3 emissions pass, it simulates `reconcile_loss(asset)` for every
reserve in every modeled v3 pool and submits only positive custody deficits.
The v3 backstop advances its bound BLNT emitter from its own `distribute()`;
the legacy emitter remains bound to the v2 backstop.

Preview one complete pass without mutation, submit one pass, or run the keeper
continuously in the foreground:

```bash
make testnet-production-keeper-plan
make testnet-production-keeper-once
make testnet-production-keeper
```

Continuous mode performs its first pass immediately and schedules subsequent
passes hourly without allowing overlapping passes. It also refreshes all
controlled v2 and v3 lending-oracle histories when their last successful
refresh is at least one hour old. Oracle refresh runs before emissions, and a
failure remains due for retry on the next hourly pass. This keeps the fixture
prices comfortably inside Blend's one-day validity window.

Override either interval, in seconds, when an isolated test needs a different
schedule:

```bash
BLEND_KEEPER_INTERVAL_SECONDS=300 make testnet-production-keeper
BLEND_KEEPER_ORACLE_REFRESH_INTERVAL_SECONDS=21600 make testnet-production-keeper
```

Stop continuous mode with `Ctrl-C` or `SIGTERM`. An unexpected call failure is
reported without preventing the other configured pools from being serviced;
continuous mode retries on its next interval. Known v2/v3 no-work and timing
errors are treated as idempotent skips. The configured source account only
pays transaction fees and receives no protocol privilege. The keeper never
invokes emitter swap, migration `drop()`, claims, or pool-admin operations.

Automatic oracle refresh is guarded by the Stellar testnet passphrase and
uses the same controlled fixed-price fixture path as
`testnet-production-refresh-oracle`. It is not a production oracle service.
When deployment paths are overridden, the keeper verifies that the registry's
modeled assets and oracle contracts match the selected state file and forwards
the selected Stellar configuration and state file to the refresher. The
`BLEND_KEEPER_STATE_FILE` override selects a non-default deployment state.

The internal scheduler runs only while the process and host remain running. A
service manager such as `launchd` can supervise it for unattended operation,
but should not add a second periodic schedule.

After the seven-day backstop-tier and pool streams expire:

```bash
make testnet-production-claim-emissions
```

This lane proves deployment and transaction compatibility. Its controlled
lending oracles are test fixtures, not acceptable mainnet providers.

## Modeled lending pools

Every fresh deployment creates side-by-side Fixed and synthetic Custom
lending-pool fixtures. Fixed reserve parameters were captured from mainnet
Fixed Pool v2; Custom uses XLM, USDC, and PYUSD reserves. The setup
finishes before the emitter queue and first candidate `distribute()`, so both
candidate pools are prefunded before the backfill clock
begins. If the pool portion of deployment is interrupted, its idempotent setup
can be resumed; verification may also be repeated independently:

```bash
make testnet-production-deploy-modeled-pools
make testnet-production-verify-modeled-pools
```

If deployment is interrupted after modeled-pool verification but before the
emitter queue opens, resume only the queue step with:

```bash
make testnet-production-begin
```

Reserve initialization is resumable across the v2 two-transaction
`queue_set_reserve` / `set_reserve` boundary. For each absent reserve, the
runner first simulates `set_reserve`. An already queued and unlocked reserve is
finalized directly; otherwise the runner queues and then finalizes it. This
avoids v2's rejection of a duplicate queued reserve after an interrupted run.

The completed fixture has two SDK-22 v2 pools against the incumbent single
BLND:USDC backstop. Each v2 pool receives 22,000 BLND:USDC LP. Fixed v3 uses
the BLND:XLM → BLND:USDC → USDC waterfall and holds the full 44,000 LP plus one
base unit required for a strict aggregate migration majority. It also receives
10 BLND:XLM LP and 12,500 USDC. Custom v3 uses XLM → USDC with 2:1 take-rate
weights and receives 1,000 XLM plus 12,250 USDC, worth $12,500 at the fixture
price.

The v2 balance exceeds the v2 activation formula at the controlled Comet V1.1
ratio. The runner sets both v2 pools to Admin Active, configures their 30% pool
tranche for USDC suppliers, seeds 1,000 USDC of supply per pool, adds both to
the incumbent reward zone, advances the deployed emitter, and starts the
seven-day emission streams. The large synthetic BLND issuance used to create
the activation LP is test-only setup capital, not an emitter distribution.
Rerunning the modeled-pool setup command renews a missing or expired stream;
standalone verification rejects a null, zero-rate, or expired stream instead
of reporting it as emitting.

Fixed v3 directs its 30% pool tranche 20% to XLM suppliers, 40% to USDC
borrowers, and 40% to EURC borrowers; its deterministic
exercise supplies 500 EURC of liquidity, supplies 1,000 USDC as collateral,
and borrows 100 EURC. It enters the candidate reward zone and receives the
queue-independent backfill and ongoing 70/30 distributions. Custom v3 is
seeded with 1,000 USDC of lending supply and also exposes XLM and PYUSD. It
enters an open reward-zone slot but has no BLND-bearing backstop tier, so its
emission weight and BLND allocation remain zero. The finalize and claim exercises therefore
gulp and claim only Fixed's BLND:USDC and BLND:XLM backstop streams and its
XLM-supply, USDC-borrow, and EURC-borrow pool streams.

The optional No Backstop scenario reuses Fixed v3's XLM, USDC, and EURC
reserve configuration and controlled oracle. It has an immutable empty tier
list, a zero backstop take rate, zero active backstop value, and no reward-zone
eligibility. The runner activates it without a backstop deposit, seeds 1,000
USDC of ordinary supply, supplies another 1,000 USDC as collateral, supplies
500 EURC, and borrows 100 EURC. Deploy or reverify it independently with:

```bash
make testnet-v3-only-deploy-no-backstop-pool
make testnet-v3-only-verify-no-backstop-pool
```

The optional Traditional scenario also reuses Fixed v3's XLM, USDC, and EURC
reserve configuration, controlled oracle, and seeded lending positions. Its
immutable backstop contains two plain-USDC tiers with 3:2 take-rate weights,
funded with 15,000 USDC in FirstLoss and 10,000 USDC in SecondLoss. Because
neither tier contains BLNT, the active pool is not reward-zone eligible. Deploy
or reverify it independently with:

```bash
make testnet-v3-only-deploy-traditional-pool
make testnet-v3-only-verify-traditional-pool
```

The controlled lending-oracle fixtures use seven-decimal USD prices. Their
price setters require authorization from the isolated runner operator bound at
construction, and verification checks that immutable
binding. Other public-testnet accounts cannot rewrite the fixtures. They make
the pools usable for functional test transactions but are not suitable price
sources for production. Synthetic EURC and PYUSD are issued only by the
isolated runner identity. The exact pool configuration and source provenance are recorded in
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
