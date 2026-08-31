# Dual-Emitter BLND/BLNT Deployment

The active deployment lane uses independent emitters: the exact deployed
legacy emitter continues BLND to v2, while the new v3 emitter sends BLNT to v3
from launch. It deploys canonical BLNT:USDC and BLNT:XLM pools from the
workspace's Comet v2 artifact and does not queue a legacy-emitter replacement.
The v3 emitter constructor binds the deployment operator as a one-time
initializer; successful initialization consumes that authority, leaving no
ongoing emitter administrator. A separate immutable backfill contract receives
the complete 150-million-BLNT initial drop, manages address claims, and owns a
270-day refundable conversion path capped at 120 million BLNT net. After that
window, anyone may burn the unrefunded BLND escrow and unused BLNT conversion
reserve without affecting claim liabilities.

## Local Quickstart

Preview and execute the complete isolated deployment:

```bash
make dual-emitter-plan
make dual-emitter-run
make dual-emitter-status
```

`run` verifies and builds artifacts, starts Protocol-27 Quickstart, deploys the
two independent lanes, activates v3 directly, executes the exact
150-million-BLNT backfill drop, converts and partially refunds a small test
balance, deploys and activates the modeled Fixed and Custom V2/V3 pools, adds
each pair to its respective reward zone, advances each emitter, and verifies
all final bindings. Stop the isolated network with:

```bash
make dual-emitter-stop
```

To reuse an already-running network, run `make dual-emitter-deploy` after
`make dual-emitter-validate`.

`run` and `deploy` use the committed snapshot manifest. It assigns
exactly 20 million BLNT proportionally across the 434 positive
`flattened_total_cpal_raw` rows (423 accounts and 11 contracts) in
`comet_cpal_flattened_ownership_before_41c898a1.csv`, with deterministic
largest-remainder rounding. The runner regenerates the manifest from the CSV
and requires a byte-for-byte match, then verifies the source hash, claimant
types and count, unique addresses, positive amounts, and exact aggregate before
deployment. The runner always constructs the immutable `(address, amount)`
claim vector from that verified manifest; it cannot be overridden through the
environment. The selected list cannot be changed after construction, and the
backfill contract receives the complete 150 million BLNT.
Every address allocation vests linearly per second for 180 days beginning at
backfill-contract construction. Each claim transfers only the cumulative
vested amount not previously claimed, and unclaimed amounts remain available
after vesting completes.

Set `BLEND_BLNT_GRANT_LIST` to the Stellar CLI JSON encoding of the immutable
`(address, amount)` grant vector. It defaults to `[]`, accepts at most 100
unique positive entries, and may total no more than 10 million BLNT. Grants
vest linearly per second for 720 days from the same construction timestamp and
are claimed independently through `claim_grant`.

## Public testnet

The same script uses production-style public-testnet transaction assembly:

```bash
make testnet-dual-emitter-validate
make testnet-dual-emitter-run
make testnet-dual-emitter-status
```

Signing identities live only below the ignored testnet Stellar configuration
directory. The runner never reads a mainnet wallet or submits to mainnet.

## BLNT asset metadata

The on-chain classic asset and SAC expose the asset code `BLNT`; the
human-readable name and icon are issuer metadata hosted by the issuer's home
domain. Set `BLEND_BLNT_HOME_DOMAIN=blent.trade` during deployment, then publish
the following currency entry at `https://blent.trade/.well-known/stellar.toml`,
replacing the issuer placeholder with the deployment operator account:

```toml
[[CURRENCIES]]
code = "BLNT"
issuer = "<deployment operator account>"
display_decimals = 7
name = "Blent Token"
image = "https://blent.trade/assets/blnt.png"
```

## Recorded evidence

The network-specific `dual-emitter-state.json` and timestamped run directory
record:

- exact legacy and v3 emitter WASM hashes;
- the Comet v2 source commit and optimized WASM hash;
- BLND, BLNT, USDC, and XLM contract IDs;
- the BLNT metadata name `Blent Token` and configured issuer home domain;
- BLND:USDC, BLNT:USDC, and BLNT:XLM Comet v2 IDs;
- separate emitter and backstop IDs;
- the backfill-contract artifact hash, address, and immutable token bindings;
- the immutable 180-day backfill and 720-day grant vesting timestamps and
  allocation totals;
- both token administrators and emitter recipients;
- direct v3 activation and exact 150-million-BLNT backfill custody;
- exact same-user BLND escrow, pre-funded BLNT transfer, and partial refund events;
- positive independent BLND and BLNT distributions;
- active modeled Fixed and Custom pools in their respective V2 and V3 reward
  zones, including verified seven-day V2 USDC-supply emission streams; and
- the immutable 120-million-BLNT net conversion cap, 270-day deadline, zero
  initial BLND/BLNT expiry burns, and empty initial v3 queue.

The runner initializes all three Comet v2 pools at 80:20, verifies positive authorized
non-clawbackable custody, and then locks their shared classic controller at
100/100/100 thresholds with its only signer weight set to zero.

## Future upgrade

The initial v3 designated token is canonical BLNT:USDC LP. A future protocol
does not use this launch runner to replace v3; it calls the v3 emitter's
preserved `queue_swap_backstop` and `swap_backstop` entry points after directly
holding strictly more of that designated token than the incumbent.
