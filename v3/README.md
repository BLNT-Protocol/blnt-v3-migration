# Blend V3 Workspace

All new Blend v3 contract code belongs in this workspace.

The workspace targets Stellar Protocol 27 and pins:

- Rust 1.91.0
- Soroban SDK 27.0.2
- `wasm32v1-none`

The `compat-probe` contract and `compat-tests` harness enforce the initial
cross-version boundary: SDK-22 v2 WASM executes on the SDK-27 host and receives
a cross-contract call from SDK-27 WASM.

The historical v2 contracts remain in the repository root and continue to use
their SDK-22 toolchain. Do not move v3 code into the root workspace or upgrade
the root dependency graph.
