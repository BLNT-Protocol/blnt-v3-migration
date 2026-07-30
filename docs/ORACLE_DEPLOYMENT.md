# Production Oracle Qualification

The production
[backstop-valuation contract](../blend-contracts-v3/docs/BACKSTOP_VALUATION.md)
cannot be deployed safely until one concrete SEP-40 feed passes every
immutable constructor and runtime assumption. The isolated qualification
runner makes that decision reproducible:

```bash
make oracle-plan
make oracle-check
```

The default is a read-only check of the official Reflector Stellar-DEX mirror
on public testnet. Set `BLEND_ORACLE_NETWORK=mainnet` to check its mainnet
counterpart. Evidence is written below the ignored
`.oracle-validation/<network>/runs/<timestamp>/` directory. It never reads or
writes `.testnet/state.json`.

## Qualification gates

`oracle-check` derives or accepts the exact BLND, USDC, and XLM contract IDs,
then:

1. fetches the provider's deployed WASM and records its SHA-256 digest;
2. records `base`, `assets`, `decimals`, `resolution`, `version`, and `admin`;
3. requires the configured USDC Stellar asset as the feed base;
4. requires the exact BLND and XLM Stellar asset IDs in `assets`;
5. requires an oracle precision no greater than 18;
6. requires the configured record count and resolution to span 30 minutes
   through 24 hours;
7. requires maximum age to be at least one resolution and no more than one
   hour; and
8. fetches both histories and requires exactly the configured number of
   positive, unique, uniformly spaced, resolution-aligned, nonfuture, fresh
   observations.

Every invocation saves the CLI's simulated transaction envelope, resource
footprint, minimum resource fee, ledger, and result beside the decoded output.
Stellar RPC currently reports zero for the optional diagnostic CPU and memory
fields on these read-only simulations; the encoded transaction footprint and
fee are still retained. These provider reads do not replace the required
post-deployment four-quote valuation simulation.

An incompatible check exits nonzero. `make oracle-deploy` reruns the same gates
and refuses to submit a deployment transaction if any gate fails.

## Deployment review

Before a public deployment:

1. Independently review the concrete SEP-40 contract, publisher trust model,
   upgrade controls, outage behavior, USDC base, assets, precision, and
   resolution.
2. Select and disclose a record count and maximum age satisfying the on-chain
   bounds.
3. Verify the exact classic-asset contracts and both production Comets against
   independent sources.
4. Reproduce the generic optimized-WASM resource test and then repeat the
   maximum-record, four-quote measurement against the selected oracle's exact
   WASM and initialized storage. Record contract digests, constructor
   arguments, and measurements.
5. Exercise fresh, stale, unavailable, malformed, and imbalanced-market cases
   on the target network.

Reviewing the backstop-valuation contract cannot make an untrusted or
compromised feed trustworthy. Feed selection remains a deployment security
decision and must be disclosed to depositors.

## Recorded assessments

The dated
[Reflector assessment from 2026-07-27](evidence/REFLECTOR_ASSESSMENT_2026-07-27.md)
is historical evidence, not a current provider endorsement or approval.

## Deployment after a provider qualifies

Choose the two exact production Comets and a funded deployment identity:

```bash
BLEND_ORACLE_ID=C... \
BLEND_ORACLE_DEPLOY_SOURCE=deployer \
BLEND_ORACLE_BLND_USDC_COMET=C... \
BLEND_ORACLE_BLND_XLM_COMET=C... \
make oracle-deploy
```

An identity alias must exist in the isolated Stellar CLI configuration under
`.oracle-validation/<network>/stellar-config`, or
`BLEND_ORACLE_CONFIG_DIR` must point to the configuration that owns it. A
secret may be supplied directly, but doing so can expose it through shell
history and is not recommended.

The deploy command qualifies the provider, builds the optimized
backstop-valuation contract, and then repeats qualification immediately before
submission. A changed provider WASM or any second-check failure prevents
submission.

The default policy requests seven observations from a five-minute feed, a
30-minute first-to-last span, and a 600-second maximum age. Override it only
after documenting the selected provider:

```bash
BLEND_ORACLE_TWAP_RECORDS=7 \
BLEND_ORACLE_MAX_PRICE_AGE=600 \
make oracle-check
```

If the feed uses nondefault test assets, pass either classic-asset descriptors
or already resolved contract IDs as shown by
`bash scripts/valuation-oracle.sh --help`.

After deployment:

```bash
make oracle-verify
```

Verification reloads the saved asset and TWAP policy, rechecks the provider,
asserts the deployed valuation contract's complete immutable binding and
version, rejects a provider- or valuation-WASM digest change, and simulates the
active and queued quote shape for each Comet. The four quote calls are recorded
separately so failures are attributable. Before a production candidate is
accepted, also simulate its single atomic canonical-valuation call against the
exact deployed valuation contract, Comets, feed WASM, and initialized provider
storage.
