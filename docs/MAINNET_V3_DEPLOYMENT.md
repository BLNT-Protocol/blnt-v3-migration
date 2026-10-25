# Mainnet V3 Deployment

This runbook covers the reviewed BLNT V3 core deployment on Stellar mainnet. It reuses the already deployed BLNT asset and canonical BLNT:USDC and BLNT:XLM Comet V1.1 pools. It deploys the V3 emitter, backfill contract, pool factory, and backstop, uploads the lending-pool WASM, hands BLNT administration to the emitter, activates the backstop, and executes the 150-million-BLNT initial drop. It does not deploy lending pools.

## Immutable inputs

The runner pins the V3 and backfill source commits, the official Comet V1.1 release artifact, every optimized production WASM digest, deterministic deployment salts and addresses, the 74-million-BLNT backfill manifest, and a private 25-million-BLNT contributor-grant manifest. The committed deployment registry records only the private manifest's SHA-256 digest, recipient count, and total. Contributor addresses are never copied into committed state, documentation, deployment registries, or committed logs. They necessarily become public constructor data when the backfill contract is deployed.

The private input must exist at `.mainnet-v3/blnt-grants.json`, be mode `0600`, and match the digest pinned by the runner. This path is ignored by Git. The runner deploys the emitter, uploads the lending-pool WASM, and deploys the factory before it constructs the backfill contract. The backfill constructor starts the vesting clocks, so the remaining constructor-through-activation-and-funding sequence is continuous rather than intentionally paused.

## Read-only preparation

Run these commands before authorizing any mainnet transaction:

```bash
make mainnet-v3-plan
make mainnet-v3-validate
make mainnet-comet-status
make mainnet-v3-preflight
```

`plan` prints the deterministic contract addresses and allocation totals without recipient addresses. `validate` rebuilds the pinned contracts and verifies exact artifact and allocation digests. `preflight` requires a clean `main` equal to `origin/main`, validates the deployment and issuer signing identities, checks the existing assets and locked Comet pools, and proves all four predicted contract IDs are unused. These commands do not submit transactions.

Each mutating Stellar CLI command simulates its transaction immediately before submission and writes cost output and transaction evidence under the ignored `.mainnet-v3/core/runs/` directory. Exact Soroban rent and resource fees depend on the deployment ledger and therefore are not hardcoded in advance. The runner uses the configured inclusion-fee ceiling, which defaults to 100,000 stroops per transaction.

## Exclusive wallet access

The deployment wallet must have one sequence-number owner during deployment. Stop the V2.1 mainnet keeper and the BLND arbitrage bot before proceeding. The runner checks for those known processes and refuses to mutate mainnet if either is running. Do not restart them until `make mainnet-v3-status` reports a verified deployment.

## Reviewed execution

After reviewing the plan, preflight, working tree, and available wallet balance, run:

```bash
BLEND_MAINNET_V3_CONFIRM=DEPLOY_BLNT_V3_MAINNET \
BLEND_MAINNET_V3_SEQUENCE_LOCK_CONFIRMED=true \
make mainnet-v3-deploy-core
```

The two explicit confirmations are mandatory. The runner also requires a clean `main` whose commit equals local `origin/main`. It stores resumable state and transaction hashes in the ignored `.mainnet-v3/core/state.json`. If execution is interrupted after state is created, inspect the recorded evidence and use the recovery command only after confirming the last submitted transaction's outcome:

```bash
BLEND_MAINNET_V3_CONFIRM=DEPLOY_BLNT_V3_MAINNET \
BLEND_MAINNET_V3_SEQUENCE_LOCK_CONFIRMED=true \
make mainnet-v3-activate
```

The recovery path never silently adopts a predicted contract that exists without local transaction evidence. That condition requires manual review because submission status may have been ambiguous.

## Verification and records

After completion, run:

```bash
make mainnet-v3-status
```

Status re-fetches exact deployed WASM bytes and verifies the BLNT administrator, emitter recipient, empty emitter queue, BLNT:XLM migration token, backfill token bindings, allocation totals, vesting durations, conversion duration, 1:1 converted/burned accounting, and BLNT custody conservation. A successful deployment writes `deployments/mainnet-v3.json`; the registry contains contract IDs, asset IDs, artifact hashes, transaction hashes, and allocation digests, but no private contributor address.

Lending-pool deployment is a separate reviewed phase. Do not infer pool parameters, admins, oracle contracts, reserve configuration, or backstop tiers from this core runbook.
