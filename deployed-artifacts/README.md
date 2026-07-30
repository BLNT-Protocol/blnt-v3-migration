# Deployed mainnet artifacts

This directory contains the exact deployed bytecode used to prove Blend v3
migration compatibility.

- `mainnet/` contains the exact WASM fetched from the deployed emitter and
  Blend v2 contracts. These files are the authoritative compatibility and
  isolated-deployment fixtures.
- `../blend-contracts-v2/` is the pinned SDK-22 source baseline used for
  inherited tests and audit reference.

Run `make verify-deployed-artifacts` before using any artifact. The
verification checks the v2 submodule commit and every SHA-256 digest in
`mainnet/manifest.json`.

Locally rebuilt SDK-22 WASM is useful for reproducibility checks, but it is not
silently substituted for the deployed bytecode. Optimizer and build-environment
differences can produce a different hash from the mainnet artifact.
