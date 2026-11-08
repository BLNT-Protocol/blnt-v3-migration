# BLNT Lockup Backfill Mainnet Deployment

This runbook deploys the separately funded BLNT lockup-backfill contract to
Stellar mainnet. It binds the immutable production BLNT asset, canonical BLNT
backfill contract, source-attested YieldBlent backstop manager, deployment-
supplied owner, and the four-entry production allocation manifest.

It does not fund the contract, claim the owner's contributor grant, transfer
manager ownership, join either Comet pool, deposit backstop LP shares, or claim
emissions. Those operations remain separate so each state transition can be
reviewed and verified independently.

The shared
[contract deployment readiness gate](CONTRACT_DEPLOYMENT_READINESS.md) applies
in full.

## Immutable and reviewed bindings

- BLNT Stellar Asset Contract:
  `CCBNWP5L7ZSXA3DBWCMFAVKFVRW3ZHWO7B5J557W42O64DDZNPJP7VDS`
- Source BLNT backfill contract:
  `CANIE64BDFKLHROLOIY2KTQVBWUZNUKZP75DLXC272QM7TIN5N3M4LVA`
- Source-attested YieldBlent backstop manager:
  `CAJUD2IQOVKPDZFJZEX2MSG7PKIE2NF4CJQ2ERBU7VF52SEM5GQL7XEB`
- Required operational manager: YieldBlox DAO
  `CANSYFVMIP7JVYEZQ463Y2I2VLEVNLDJJ4QNZTDBGLOOGKURPTW4A6FQ`, scope `2`
- Production allocation manifest:
  `blnt-lockup-backfill-contract/allocations/production.json`
- Allocation count: `4`
- Aggregate allocation: `37,442,405,803,143` raw units, or
  `3,744,240.5803143 BLNT`
- Canonical source metadata:
  `source_repo=github:BLNT-Protocol/blnt-lockup-backfill-contract`
- Deterministic deployment salt:
  `149fd4c49b974c03e82fd107d1e80e59ee39b32cfd62eb87739476b7fd62a3c3`

The immutable owner is deliberately not hardcoded. Supply it through
`BLNT_LOCKUP_BACKFILL_OWNER`. The runner requires the deployed manager's live
`owner()` value to equal that address before it will print a successful plan or
deploy. The deployer is independently supplied through
`BLNT_LOCKUP_BACKFILL_DEPLOYER`; it need not be the immutable owner.

## 1. Commit and release the exact source

Before using the runner:

1. Review and commit the contract repository.
2. Push that exact commit to the canonical public GitHub repository.
3. Create and push a new immutable source tag for the reviewed release.
4. Wait for the `Stellar Expert WASM Release` workflow to finish successfully.
5. Record both the source tag and the generated GitHub release tag. The
   reusable workflow may publish a derived release tag containing the package
   and CLI versions, so these two tags need not be identical.
6. Download the release WASM without rebuilding or re-optimizing it.
7. Commit and push the migration-repository submodule pin to the exact tagged
   source commit.

The runner independently verifies all of the following:

- the source tag exists locally and remotely at the checked-out source commit;
- the canonical source repository is public and active;
- the generated release is public, final, and contains exactly one WASM whose
  bytes match the supplied file;
- a successful tag-triggered release workflow built that source commit;
- `gh attestation verify` accepts the artifact with the
  pinned `stellar-expert/soroban-build-workflow` release workflow at commit
  `88068ec50cba931a96436869727ed08edeb76ade`, exact source digest, and exact
  source-tag ref; and
- the supplied WASM embeds the canonical `source_repo` metadata.

## 2. Prepare public deployment inputs

Export the exact downloaded release asset and its two release identifiers:

```bash
export BLNT_LOCKUP_BACKFILL_WASM="/absolute/path/to/release/blnt_lockup_backfill_contract.wasm"
export BLNT_LOCKUP_BACKFILL_SOURCE_TAG="<source-tag>"
export BLNT_LOCKUP_BACKFILL_RELEASE_TAG="<generated-release-tag>"
```

Supply the deployment source and immutable owner addresses. Keep the owner in
the local deployment environment; it will become public constructor state and
will be written to the deployment record only after deployment.

```bash
export BLNT_LOCKUP_BACKFILL_DEPLOYER="G..."
export BLNT_LOCKUP_BACKFILL_OWNER="G..."
```

The runner reads the allocation constructor argument only from:

```bash
make -C blnt-lockup-backfill-contract validate-production-allocations
make -C blnt-lockup-backfill-contract production-allocation-args
```

It also verifies the committed manager deployment record and live manager
WASM, checks the manager's current owner, reads the source BLNT and vesting
bindings, and confirms the 360-day epoch. A new deployment plan additionally
requires the proposed owner to have a positive currently claimable contributor
grant.

## 3. Review the non-mutating plan

```bash
make mainnet-lockup-backfill-plan
```

The plan prints the source and release provenance, artifact digest, attestation
count, predicted contract ID, constructor bindings, vesting epoch, allocation
manifest digest, aggregate allocation, and deployment salt. It does not sign or
submit a transaction.

Do not proceed unless the parent repository and contract submodule are clean,
pushed, and the parent commit pins the tagged source commit. The deploy command
enforces those conditions again.

## 4. Deploy

The account configuration is a local, uncommitted shell file containing the
deployer credentials used by Stellar CLI:

```bash
STELLAR_PUBLIC_KEY="G..."
STELLAR_SECRET_KEY="S..."
```

Deploy only after reviewing the plan:

```bash
export BLNT_LOCKUP_BACKFILL_ACCOUNT_CONFIG="/absolute/path/to/mainnet.conf"
export BLNT_LOCKUP_BACKFILL_CONFIRM_MAINNET=true
make mainnet-lockup-backfill-deploy
```

The runner uploads the exact attested release bytes with optimization disabled,
deploys the predicted contract, and supplies the BLNT, source-backfill, owner,
manager, and generated allocation-list constructor arguments. It then fetches
the deployed WASM, byte-compares it with the release asset, checks the embedded
source metadata, and reads all constructor state back from mainnet.

Local transaction evidence and cost logs are written beneath the ignored
`.mainnet-v3-lockup-backfill-v1.0.1/` directory. The runner captures signed
transaction hashes before submission completion and verifies successful
results through Horizon, allowing an interrupted or ambiguous submission to be
reconciled without blindly retrying it.

Deployment does not transfer or fund BLNT. Initial totals must read back as the
full outstanding allocation, zero claims, zero custody, and the full funding
gap, with claims open.

## 5. Record or recover an interrupted deployment

After successful byte and constructor verification, deployment writes the
secret-free public registry:

`deployments/mainnet-v3-lockup-backfill-v1.0.1.json`

The record includes the network, contract ID, salt, WASM hash and size, source
commit and tag, generated release tag and URL, release asset, workflow run,
attestation count, deployment transaction and ledger, constructor bindings,
allocation manifest digest and entries, source vesting timestamps, tool
version, and parent commit.

If deployment and verification completed but registry generation was
interrupted, recover it without submitting another transaction:

```bash
make mainnet-lockup-backfill-record
```

The recovery path requires the original ignored state and transaction logs,
re-verifies release provenance and live contract state, and refuses to write a
registry without successful deployment evidence.

## 6. Read-only verification

With the exact release WASM available and either local state or the committed
registry present:

```bash
make mainnet-lockup-backfill-status
```

The status command verifies:

- the deterministic contract ID;
- byte-for-byte equality with the attested release WASM;
- canonical source metadata and GitHub attestation;
- BLNT, source-backfill, immutable-owner, and manager bindings;
- source vesting timestamps;
- the exact four allocations and aggregate;
- claims-closure state; and
- internally consistent live claimed, custody, outstanding, and funding-gap
  accounting. Initial zero claims and custody are enforced when the deployment
  registry is first created.

Because `claim_and_fund()` requires the immutable owner to continue owning the
bound manager, the plan, deploy, recovery, and status paths fail if manager
ownership no longer matches. Manager ownership should not be transferred until
the intended grant-claim and manager-funding lifecycle is complete.

## 7. Explorer source association

The deployed bytes must be the exact GitHub-attested release asset carrying
the canonical `source_repo` metadata. After deployment verification and
registry creation, allow Stellar Expert time to index the contract. Explorer
indexing delay is not a substitute for the runner's byte comparison,
attestation verification, or constructor readback.

Do not fund the contract until the deployment registry is committed, the
read-only status command passes from a clean checkout, and Stellar Expert's
source association has been checked.
