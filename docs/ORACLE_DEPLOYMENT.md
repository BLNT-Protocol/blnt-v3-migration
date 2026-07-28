# Production Oracle Qualification

The production valuation adapter cannot be deployed safely until one concrete
SEP-40 feed passes every immutable constructor and runtime assumption. The
isolated qualification runner makes that decision reproducible:

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
post-deployment four-quote adapter simulation.

An incompatible check exits nonzero. `make oracle-deploy` reruns the same gates
and refuses to submit a deployment transaction if any gate fails.

## Reflector assessment on 2026-07-27

The defaults below are the Reflector addresses published by
[Stellar's oracle-provider documentation](https://developers.stellar.org/docs/data/oracles/oracle-providers).
This assessment is a point-in-time compatibility check, not an audit or an
endorsement.

| Property | Testnet mainnet-DEX mirror | Mainnet Stellar-DEX feed |
| --- | --- | --- |
| Contract | `CAVLP5DH2GJPZMVO7IJY4CVOD5MWEFTJFVPD2YY2FQXOQHRGHK4D6HLP` | `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M` |
| Version | 6 | 6 |
| Decimals | 14 | 14 |
| Resolution | 300 seconds | 300 seconds |
| Base | projected USDC SAC | mainnet USDC SAC |
| Advertised assets | 9 | 49 |
| Exact BLND | missing | missing |
| Exact native XLM | missing | present |
| BLND history | unavailable | unavailable |
| `prices(XLM, 7)` | unavailable | returned 8 observations |
| WASM SHA-256 | `8ecd1857496df2c15aaab4d18d2d7689542a62814245e9b2c613c609b86bd11c` | `7944a96d512f6e51532687811cc0c932505b0c051be9d351f5332739d70f753c` |
| Admin | `GCLA2E3LQDPAPJLHYDMB5R65ASGLNXWGJCX4TX7XA75C7VTJ7Y2OTZXA` | `GC2M2UTZ57GSENZBTPMLPB6QSAL2USIPHMXSUCT2I3V24GYWF3EI6RFA` |
| Candidate verdict | incompatible | incompatible |

The production adapter intentionally requires an exact history length. The
observed eight-element response to `prices(XLM, 7)` therefore fails even
though its timestamps are otherwise uniform and fresh. This behavior must be
resolved with the selected provider; the adapter must not silently discard an
extra observation because that would make the configured averaging window
ambiguous.

Reflector documents a distributed publisher network and regular five-minute
updates, but both checked contracts also expose an administrator. Selecting
one therefore trusts the provider's publisher consensus, administrator and
upgrade process, availability, and market methodology. The adapter rechecks
base, decimals, and resolution on every quote, but it cannot prove that an
external contract at the same address retained the previously reviewed WASM.

No production provider has been approved and no production adapter was
deployed by this assessment. A custom Reflector feed or another independently
reviewed SEP-40 deployment must first provide the exact USDC base, BLND and
native-XLM membership, and the configured history semantics.

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

The deploy command qualifies the provider, builds the optimized adapter, and
then repeats qualification immediately before submission. A changed provider
WASM or any second-check failure prevents submission.

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
asserts the deployed adapter's complete immutable binding and version, rejects
a provider- or adapter-WASM digest change, and simulates the active and queued
quote shape for each Comet. The four quote calls are recorded separately so
failures are attributable. Before a production candidate is accepted, also
simulate the candidate's single atomic canonical-valuation call against the
exact deployed adapter, Comets, feed WASM, and initialized provider storage.
