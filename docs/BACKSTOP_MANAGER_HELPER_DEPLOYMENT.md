# YieldBlent Backstop Manager Helper Deployment

This runbook deploys the immutable `blnt-backstop-manager-helper` on Stellar
mainnet. The helper lets one configured owner atomically join both canonical
Comet V1.1 pools, deposit the resulting LP shares into the YieldBlent
FirstLoss and SecondLoss tiers, and claim both tiers through the existing
backstop-manager.

The helper never holds funds or positions. The existing backstop-manager
remains the custodian and final authorization boundary.

## Production bindings

- Owner: supplied through `BLNT_BACKSTOP_MANAGER_HELPER_OWNER`; the production
  instance uses the VTK Ledger wallet.
- Backstop manager:
  `CAJUD2IQOVKPDZFJZEX2MSG7PKIE2NF4CJQ2ERBU7VF52SEM5GQL7XEB`
- BLNT V3 backstop:
  `CAHCUKBPTW5DR5RNXOHCTJD5U76A6EEZXYZAQFRQEEQRXQ3CRUZ6Z3MO`
- YieldBlent pool:
  `CAUTPSQDAJEUV5XEU3EXFVYRGDJLEJGVTFJCFZCSPTDCREAMZ573VF4D`
- FirstLoss BLNT:XLM LP:
  `CA5HX3D47PQZWC2E7VWXCKH4534P2NOJEOIXAWU47MI4QV2FQUYY3FLC`
- SecondLoss BLNT:USDC LP:
  `CDQMIKIHKA5UY3FQJ2UDTLNX3GOPD3GPNZWSPTOBVOHEUMQJQXOPC7Y3`
- BLNT:
  `CCBNWP5L7ZSXA3DBWCMFAVKFVRW3ZHWO7B5J557W42O64DDZNPJP7VDS`
- XLM:
  `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`
- USDC:
  `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`

The runner verifies these relationships against live mainnet state before any
mutation. It also requires the backstop-manager's current owner to equal the
proposed helper owner, ensuring the nested manager calls can authorize that
address.

## Release artifact

Production deployment MUST use the exact optimized WASM published and
attested by the helper repository's `v1.0.0` release workflow. Download the
release asset and set:

```bash
export BLNT_BACKSTOP_MANAGER_HELPER_WASM="/absolute/path/to/blnt-backstop-manager-helper_v1.0.0.wasm"
export BLNT_BACKSTOP_MANAGER_HELPER_SOURCE_TAG="v1.0.0"
export BLNT_BACKSTOP_MANAGER_HELPER_RELEASE_TAG="v1.0.0_blnt-backstop-manager-helper_cli27.0.0"
```

The runner verifies the public repository, source tag, successful tagged
workflow, release asset identity, GitHub artifact attestation, embedded
`source_repo` metadata, and pinned parent submodule commit.

## Plan

Set the FTAL deployment account and VTK owner, then run the read-only plan:

```bash
export BLNT_BACKSTOP_MANAGER_HELPER_DEPLOYER="G..."
export BLNT_BACKSTOP_MANAGER_HELPER_OWNER="G..."
make mainnet-backstop-manager-helper-plan
```

The plan prints the deterministic contract ID, artifact digest, constructor
bindings, release provenance, and verified live relationships. It submits no
transaction.

## Deploy

The local account configuration must contain the declared deployer's
`STELLAR_PUBLIC_KEY` and `STELLAR_SECRET_KEY`:

```bash
export BLNT_BACKSTOP_MANAGER_HELPER_ACCOUNT_CONFIG="/absolute/path/to/mainnet.conf"
export BLNT_BACKSTOP_MANAGER_HELPER_CONFIRM_MAINNET=true
make mainnet-backstop-manager-helper-deploy
```

Deployment uploads the exact attested bytes without rebuilding or
re-optimizing them, deploys the deterministic instance, byte-compares the
live WASM, reads back the complete immutable configuration, and writes a
secret-free record under `deployments/`.

If deployment succeeds but registry generation is interrupted, preserve the
ignored work directory and run:

```bash
make mainnet-backstop-manager-helper-record
```

## Verify

With the release WASM path still configured:

```bash
make mainnet-backstop-manager-helper-status
```

The status command verifies the deployed bytes, metadata, deterministic ID,
constructor bindings, and current manager authorization relationship. The
explorer source link is checked separately after StellarExpert indexes the
attested deployment.

Funding and operational `deposit`, `claim`, or `deposit_and_claim` calls are
not part of deployment. Simulate and review those calls separately before
requesting a VTK Ledger signature.
