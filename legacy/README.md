# Frozen legacy inputs

This directory separates historical source from the bytecode used to prove
Blend v3 migration compatibility.

- `blend-contracts-v2/` is a Git submodule pinned to the frozen SDK-22 source
  baseline. It is used for inherited source tests and audit reference.
- `artifacts/` contains the exact mainnet WASM fetched from the deployed
  legacy emitter and Blend v2 contracts. These files are the authoritative
  compatibility and isolated-deployment fixtures.

Run `make verify-legacy` before using any artifact. The verification checks the
submodule commit and every SHA-256 digest in `artifacts/manifest.json`.

Locally rebuilt SDK-22 WASM is useful for reproducibility checks, but it is not
silently substituted for the deployed bytecode. Optimizer and build-environment
differences can produce a different hash from the mainnet artifact.
