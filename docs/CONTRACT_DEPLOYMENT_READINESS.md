# Contract Deployment Readiness

This checklist is the mandatory release and evidence gate for every production
contract deployed or recorded by this repository. Contract-specific runbooks
may add stricter checks, but they must not weaken or bypass this gate.

The purpose of the gate is to ensure that a deployed contract is tied to
reviewed source, reproducible release evidence, exact constructor bindings, and
an immutable deployment record. Passing local tests or embedding a source URL
alone is not sufficient.

## Artifact classes

Classify the artifact before preparing a deployment:

- **Project-built artifact:** source is maintained by BLNT Protocol. The
  project release must embed canonical source metadata and publish a GitHub
  artifact attestation for the exact WASM that will be deployed.
- **Pinned upstream release artifact:** source and release are maintained by a
  third party, such as the official Comet V1.1 release. Preserve the upstream
  release bytes and provenance; do not rebuild, re-optimize, or represent the
  artifact as a BLNT Protocol build.
- **Previously deployed artifact:** exact on-chain bytes are the compatibility
  boundary. Record and verify the live hash and source provenance without
  silently substituting a local rebuild.

The remainder of this document applies to project-built artifacts unless a
step explicitly describes an upstream or previously deployed artifact.

## 1. Source readiness

Before creating a production release:

- [ ] The canonical source repository is public and accessible without local
      credentials. A private repository cannot provide a public source link.
- [ ] The contract embeds exactly
      `source_repo=github:<owner>/<repository>` in the production WASM.
- [ ] The source repository, package metadata, license, system specification,
      and deployment-facing README identify the same contract and repository.
- [ ] The reviewed source commit is pushed to the canonical repository.
- [ ] The contract repository is clean, and its lockfile and toolchain are
      pinned.
- [ ] Native tests, optimized-WASM tests, metadata checks, and any production
      manifest or allocation self-tests pass at that exact commit.
- [ ] The migration repository pins that exact source commit as a submodule;
      the checked-out submodule HEAD and parent gitlink must match.
- [ ] The deployment ABI and constructor arguments have been reviewed against
      the system specification. Any private constructor input has a reviewed,
      locally stored digest and is excluded from version control.

`source_repo` metadata identifies where source should be found. It is not a
build attestation and does not prove that a WASM was produced from that source.

## 2. Release and attestation readiness

For each project-built production WASM:

- [ ] A tag identifies the exact reviewed source commit. Do not move or
      overwrite an existing release tag.
- [ ] The tag-triggered GitHub release workflow uses the pinned
      `stellar-expert/soroban-build-workflow` reusable workflow, or an
      equivalently reviewed source-build and attestation workflow.
- [ ] Workflow permissions include `id-token: write`, `contents: write`, and
      `attestations: write`.
- [ ] The release workflow completes successfully and publishes the expected
      optimized WASM asset.
- [ ] The downloaded release asset passes GitHub artifact-attestation
      verification against the canonical repository.
- [ ] Inspection of the downloaded release asset shows the exact canonical
      `source_repo` metadata.
- [ ] The release asset's SHA-256 digest (the uploaded WASM hash) is captured
      before any network mutation.
- [ ] The deployment runner consumes the downloaded release asset itself. It
      must not rebuild, re-optimize, strip, patch, or otherwise transform the
      artifact after attestation.

Typical evidence commands are:

```bash
gh release download <tag> --repo <owner>/<repository> --pattern '*.wasm' --dir <release-dir>
gh attestation verify <release-dir>/<artifact>.wasm --repo <owner>/<repository>
shasum -a 256 <release-dir>/<artifact>.wasm
stellar contract info meta --wasm <release-dir>/<artifact>.wasm
```

Record the successful workflow run, release URL, release asset name, tag,
source commit, attestation result, and digest. A local rebuild that happens to
use the same source is not a substitute for the attested release asset.

## 3. Deployment-plan readiness

Before enabling a mutating deployment command:

- [ ] The contract-specific runbook names the network, artifact, release,
      deterministic salt if applicable, predicted contract ID, deployer, owner,
      authority model, constructor bindings, and expected initial state.
- [ ] Every contract and asset address is read from the target network and
      checked for the expected interface, token pair, decimals, authority, and
      other contract-specific invariants.
- [ ] The plan verifies that the intended owner or manager relationships
      already satisfy constructor requirements.
- [ ] The parent repository and relevant submodules are clean and pushed, and
      the parent commit pins the exact released source commit.
- [ ] A read-only plan prints the artifact digest, predicted IDs, constructor
      bindings, and expected state without submitting a transaction.
- [ ] Mainnet mutation requires an explicit mainnet confirmation and verifies
      that the credential source matches the declared deployer.
- [ ] The deployment has a recovery path for ambiguous submission results that
      verifies the predicted contract before considering a resubmission.

Funding, authority transfers, and integrations should remain separate from
deployment when they can be verified independently. Combining them requires a
written atomicity or safety justification in the contract-specific runbook.

## 4. Deployment and post-deployment evidence

Deploy only after sections 1 through 3 pass:

- [ ] Upload the exact attested release bytes and deploy the reviewed instance.
- [ ] Read the live executable hash and byte-compare the downloaded on-chain
      WASM with the attested release artifact.
- [ ] Read every constructor-bound value and relevant initial-state getter back
      from the deployed instance.
- [ ] Verify authority, custody, and token bindings against the contract-
      specific system specification before funding or granting operational
      authority.
- [ ] Write a secret-free deployment record under `deployments/`.
- [ ] Run the committed read-only status or validation command from a clean
      checkout using only the deployment record and public inputs.
- [ ] Confirm that the explorer resolves the canonical source repository after
      indexing. Explorer indexing delay does not replace any of the preceding
      byte, metadata, or attestation checks.

The deployment record must include, where applicable:

- network and network passphrase;
- contract ID, executable/WASM hash, deployment salt, and predicted ID;
- deploy transaction hash, ledger, and UTC timestamp;
- artifact filename and SHA-256 digest;
- canonical source repository, source commit, release tag, release URL,
  workflow run URL, and artifact-attestation status;
- compiler, Rust, Stellar CLI, Soroban SDK, target, and optimizer versions;
- deployer and all public constructor bindings;
- submodule commit pinned by the migration repository; and
- the command used for subsequent read-only verification.

Never commit seed phrases, secret keys, credential files, private constructor
manifests, or raw logs that may contain them.

## 5. Release decision

A contract is **ready to deploy** only when every applicable checkbox above is
supported by reproducible evidence. If any item is deliberately inapplicable,
the contract-specific runbook must state why and identify the replacement
evidence.

A deployment is **complete** only after live byte equality, constructor-state
verification, deployment-record creation, and clean-checkout status validation
all pass. A contract visible on-chain is not, by itself, a completed or
source-verified deployment.
