# Public-Testnet Migration Exercise

This lane uses the deterministic valuation adapter and the ignored `.testnet/`
state directory. The independent production-adapter lane, including native XLM
and fresh 80:20 Comets, is documented in
[`TESTNET_PRODUCTION_MIGRATION.md`](TESTNET_PRODUCTION_MIGRATION.md).

The public-testnet runner reuses the exact migration transaction assembly used
by the local Quickstart lane against Stellar testnet. It deploys a frozen
SDK-22 incumbent, the exact deployed emitter, and the optimized SDK-27
candidate without shortening the production 31-day queue.

All keys, public deployment state, transaction results, and CLI cost reports
live below the ignored `.testnet/` directory. Secrets remain only in the
runner's isolated Stellar CLI configuration. `state.json` records the source
commit, dirty-source indicator and patch digest, and SHA-256 digest of every
deployed WASM artifact. Deployment rejects non-ignored untracked repository
files so the complete tracked-source patch cannot silently omit deployment
inputs.

The runner issues isolated test assets and initializes real checked-in Comet
contracts rather than relying on official test BLND, USDC, or liquidity:

- `BLND`
- `USDC`
- `XLM`
- BLND:USDC Comet LP
- BLND:XLM Comet LP

It uses the deterministic test valuation adapter solely to exercise the
candidate's immutable valuation interface. Its mutable fixture setters require
authorization from the isolated runner operator bound at construction. This
adapter is not a production price source. The fixture deploys matched
Fixed-pool models against v2 and v3, each with XLM, USDC, and EURC reserves. It
funds the v2 pool with BLND:USDC, activates the v3 pool with BLND:USDC and
plain-USDC backstop value, adds the v3 pool to the reward zone, and creates its
EURC-borrow emission position so the ongoing-emission path can be verified
after migration.

## Commands

Review the plan and validate the artifact set:

```bash
make testnet-plan
make testnet-validate
```

Deploy both stacks, create and Friendbot-fund isolated operator and depositor
accounts, establish the candidate's strict incumbent designated-token majority, and begin
the exact migration queue:

```bash
make testnet-run
```

After deployment, the time-gated lifecycle is resumable. Query the live queue
and saved bindings with:

```bash
make testnet-status
```

Run the transaction-backed TTL keepalive at least every five days while the
queue is open; daily execution is recommended:

```bash
make testnet-keepalive
```

During the final seven-day preparation window and after unlock:

```bash
make testnet-prepare
make testnet-finalize
```

Finalization immediately runs the same invariant verification as localnet.
It can be repeated with:

```bash
make testnet-verify
```

Then exercise the immediate 70/30 distribution and, after the resulting
seven-day stream expires, its supply-side claim:

```bash
make testnet-emissions
make testnet-claim-emissions
```

Public testnet has no runner-owned container to start, recover, or stop.
Native and optimized-WASM tests remain the accelerated time-travel lane while
this exercise records production-style transaction and wall-clock evidence.
