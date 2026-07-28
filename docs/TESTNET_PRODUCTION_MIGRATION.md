# Production-Adapter Public-Testnet Migration

This isolated lane exercises the production valuation adapter against fresh,
real Comet contracts on public testnet without modifying the original
`.testnet/` migration. Its state, keys, transaction results, and cost evidence
live below the ignored `.testnet-production/` directory.

The sanitized public deployment registry is versioned at
[`../deployments/testnet-production.json`](../deployments/testnet-production.json).
It contains public account, asset, contract, transaction, timestamp, artifact,
and provenance data only. Signing material remains exclusively in the ignored
local Stellar configuration.

The runner creates:

- a fresh deployment of the exact published legacy emitter WASM;
- synthetic test-only BLND and USDC Stellar assets;
- the native XLM Stellar asset contract, with transaction accounts funded by
  Friendbot;
- a real 80:20 BLND:USDC Comet initialized with 1,000 BLND and 25 USDC;
- a real 80:20 BLND:XLM Comet initialized with 1,000 BLND and 100 native XLM;
- a bounded, authenticated, test-only SEP-40 oracle with seven five-minute
  observations for BLND/USDC and XLM/USDC; and
- the optimized production valuation adapter bound immutably to that oracle,
  the three assets, and both Comets; and
- the modeled Fixed v3 pool as the candidate's primary migration,
  verification, and post-migration emission pool.

The fixture prices are 0.10 USDC per BLND and 0.25 USDC per XLM. Consequently,
each Comet starts with 100 USDC of BLND and 25 USDC of its paired asset,
matching its 80:20 normalized weights. The oracle is deliberately controlled
and is not a production price source; its purpose is to exercise the exact
production adapter and stateful SEP-40 call boundary on public testnet.

## Start the isolated migration

Review and validate:

```bash
make testnet-production-plan
make testnet-production-validate
```

Deploy the complete incumbent and candidate stacks, create both Comets, and
begin the unshortened 31-day legacy-emitter queue:

```bash
make testnet-production-run
```

The runner refuses to overwrite an existing `.testnet-production/state.json`.
It also uses distinct Stellar CLI identities from the original testnet lane.

## Operate the queue

Query status and run the transaction-backed keepalive at least every five
days. When modeled pools are present, the same command also renews their
instances, reserve/position entries, custom assets, and lending oracles:

```bash
make testnet-production-status
make testnet-production-keepalive
```

The keepalive refreshes both fixed-price oracle histories. To refresh only the
oracle before a manual quote or diagnostic:

```bash
make testnet-production-refresh-oracle
```

The production adapter rejects a latest observation more than one hour old.
Activation valuation and the emission exercise refresh the fixture
automatically. Preparation and finalization do not depend on that controlled
oracle, so an oracle-updater failure cannot block the permissionless migration
lifecycle. During the final seven-day window and after unlock:

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

This lane proves deployment and transaction compatibility. It does not qualify
the controlled oracle as an acceptable mainnet provider.

## Modeled lending pools

The same live lane can complete side-by-side lending-pool fixtures whose
reserve parameters were captured from mainnet Fixed Pool v2 and YieldBlox Pool
v2. On a fresh deployment, Fixed v3 already exists as the candidate's primary
pool, and Fixed v2 is its migration-qualification counterpart. The command
adds both YieldBlox versions, then finishes the additional three-tier funding.
The command is resumable and refuses to run after migration activation:

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
BLND:USDC → BLND:XLM → USDC waterfall. Each matched v2/v3 pool receives
22,000 BLND:USDC LP. On a fresh deployment, Fixed v3 receives one additional
base unit to preserve the strict aggregate migration majority. Each v3 pool
also receives 10 BLND:XLM LP and 12,500 USDC.

The v2 balance exceeds the legacy activation formula at the controlled Comet
ratio. The runner sets both v2 pools to Admin Active, configures their 30% pool
tranche for USDC suppliers, seeds 1,000 USDC of supply per pool, adds both to
the incumbent reward zone, advances the legacy emitter, and starts the
seven-day emission streams. The large synthetic BLND issuance used to create
the activation LP is test-only setup capital, not an emitter distribution.
Rerunning the deployment command renews a missing or expired stream; standalone
verification rejects a null, zero-rate, or expired stream instead of reporting
it as emitting.

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

The controlled lending-oracle fixtures use seven-decimal USD prices. Their
price and valuation setters require authorization from the isolated runner
operator bound at construction, and verification checks that immutable
binding. Other public-testnet accounts cannot rewrite the fixtures. They make
the pools usable for functional test transactions but are not suitable price
sources for production. Synthetic EURC, AQUA, USDGLO, USTRY, CETES, and PYUSD
are issued only by the isolated runner identity. The exact captured pool
configuration and source contract IDs are recorded in
[`../fixtures/testnet-modeled-pools.json`](../fixtures/testnet-modeled-pools.json).

## Current deployment

The current queue-open fixture was freshly deployed on 2026-07-28 after the
modeled v3 lending-oracle setters were placed behind an immutable,
operator-authorized fixture binding. Fixed v3 is the primary
migration-verification and post-migration emission pool from construction. Its
candidate holds
44,000.0000001 BLND:USDC LP across the two modeled pools, preserving a
one-base-unit strict lead over the incumbent's 44,000 BLND:USDC LP.

| Component | Contract |
| --- | --- |
| Legacy emitter | `CDO22NMYBVVSTNQHKISLTVKACELZTWRK4QECYVC4S2OILBGPRUT3ZF4H` |
| BLND:USDC Comet | `CAOMBYZORSCE7MTUTBUWSYHISB636C7OKWOYDKA6FGMLSK5GBISX47GE` |
| BLND:XLM Comet | `CD6SY5AKEBDPW4HGTLSRESEC7ENFSN4E7RU45XJX53OPKTL3MTEFBH2V` |
| Test SEP-40 oracle | `CCAS657JNYS5M4DTDDGI7MM5UJCQH3QQKUWJG5MYZKPCIILS5T43ESGM` |
| Production adapter | `CBGV5OKBB2R5SU4K4EIXVKY5WP3YGHVB2UMYILDYAT67NGM6BKIVA7CQ` |
| SDK-22 incumbent | `CBUVN5MNBK5J46DN6R6YKDDCDKE4VGVLE4WLL23ABYA24UT2VPAHUD6H` |
| SDK-27 candidate | `CB5CNXZG4VOXHHZL3AWSHWMPYMNQYVLKFMHTB2F6VUKSIJDYAVEL7J4J` |
| Fixed v2 model | `CCOQI5XUPY3YRMKWVNCGXUPIA6IVTDTXV2EZM2HR37PFTZ3WCU3UETGV` |
| YieldBlox v2 model | `CA4KXQUHCOPJIOX76XKRSJS7VHRHUN44QCQSOMEUYFAWRFHP4FTPWD7W` |
| Fixed v3 model | `CARATH443ZE4YHIVETIST7Q77OHTKUDHWH7GCEC4MZWI4ZBSWBSCKWKG` |
| YieldBlox v3 model | `CDTOKONG4ZUGP5XF3Z3XYSQI37VTRBMUTRZJEBIM26ZY2VEP5NX3CSHH` |

Both v2 pools are current incumbent reward-zone members with active USDC
supply streams. Fixed v3 and YieldBlox v3 are both current candidate
reward-zone members. Fixed is configured for EURC-borrow emissions (reserve
token ID 4) with the documented borrow fixture; YieldBlox is configured and
supplied for USDC-supply emissions (reserve token ID 3).

Live post-modeled-pool reads returned:

| Comet | BLND reserve | Paired reserve | LP supply | Weights |
| --- | ---: | ---: | ---: | ---: |
| BLND:USDC | 880,000.0000011 BLND | 22,000.0000002 USDC | 88,000.0000001 | 80% / 20% |
| BLND:XLM | 1,000 BLND | 100 native XLM | 100 | 80% / 20% |

At the fixture prices, the production adapter valued one LP share from either
Comet at approximately 1.25 USDC with 10 underlying BLND. The queue enters its
final preparation window at `2026-08-21T05:29:25Z` and unlocks at
`2026-08-28T05:29:25Z`.
