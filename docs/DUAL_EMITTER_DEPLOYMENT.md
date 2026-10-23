# Dual-Emitter BLND/BLNT Deployment

The active deployment lane uses independent emitters: the exact deployed
legacy emitter continues BLND to v2, while the new v3 emitter sends BLNT to v3
from launch. It deploys canonical BLNT:USDC and BLNT:XLM pools from the
official Comet V1.1 release artifact and does not queue a legacy-emitter replacement.
The v3 emitter constructor binds the deployment operator as a one-time
initializer; successful initialization consumes that authority, leaving no
ongoing emitter administrator. A separate immutable backfill contract receives
the complete 150-million-BLNT initial drop, manages address claims, and owns a
270-day, 1:1 BLND-to-BLNT conversion path capped at 51 million BLNT output.
Each conversion burns BLND immediately. After that window, anyone may burn the
unused BLNT conversion reserve without affecting claim liabilities.

## Local Quickstart

Preview and execute the complete isolated deployment:

```bash
make dual-emitter-plan
make dual-emitter-run
make dual-emitter-status
```

`run` verifies and builds artifacts, starts Protocol-27 Quickstart, deploys the
two independent lanes, activates v3 directly, executes the exact
150-million-BLNT backfill drop, converts and burns a small test
balance, deploys and activates the modeled Fixed and Custom V2/V3 pools, adds
each pair to its respective reward zone, advances each emitter, and verifies
all final bindings. On local Quickstart it then deploys an isolated V3 auction
pool and starts and fills all four public auction types. Stop the isolated
network with:

```bash
make dual-emitter-stop
```

To reuse an already-running network, run `make dual-emitter-deploy` after
`make dual-emitter-validate`, then run the auction coverage separately:

```bash
make dual-emitter-auctions
```

Re-running the command after a successful pass revalidates the recorded pool
against the live ledger and confirms that all four completed auctions remain
absent. Retained evidence from a stopped ephemeral ledger is never accepted as
a live success.

The auction pass uses real deployed-WASM transactions and two isolated signer
accounts. A deliberately high but valid interest-rate configuration reaches
the inclusive 20-USDC protocol-credit minimum without altering production
contracts. It proves type 2 (interest) and type 3 (protocol fee) can coexist,
then fills both through submit discriminants 8 and 10. It next crashes only the
test oracle's collateral price, starts type 0 (user liquidation), and waits for
ledger 201 of the inherited curve before filling. The discounted debt leaves a
real residual backstop liability, which is then started and filled as type 1
(bad debt) through submit discriminant 7. Expect that curve wait to take about
three and a half minutes on the one-ledger-per-second local network.

`run` and `deploy` use the committed snapshot manifest. It assigns
exactly 74 million BLNT proportionally across the 434 positive
`flattened_total_cpal_raw` rows (423 accounts and 11 contracts) in
`comet_cpal_flattened_ownership_before_41c898a1.csv`, with deterministic
largest-remainder rounding. The runner regenerates the manifest from the CSV
and requires a byte-for-byte match, then verifies the source hash, claimant
types and count, unique addresses, positive amounts, and exact aggregate before
deployment. The runner always constructs the immutable `(address, amount)`
claim vector from that verified manifest; it cannot be overridden through the
environment. The selected list cannot be changed after construction, and the
backfill contract receives the complete 150 million BLNT.
Every address allocation vests linearly per second for 180 days from
backfill-contract construction. Each claim transfers only the cumulative
vested amount not previously claimed, and
unclaimed amounts remain available after vesting completes.

Set `BLEND_BLNT_GRANT_LIST` to the Stellar CLI JSON encoding of the immutable
`(address, amount)` grant vector. It must contain at most 100 unique positive
entries totaling exactly 25 million BLNT. If omitted, the dual-emitter runner
assigns the complete grant to its deployment operator; the V3-only testnet
wrapper assigns it to `BLEND_V3_USER_WALLET`. Grants
vest linearly per second for 360 days from the same construction timestamp and
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
domain. Set `BLEND_BLNT_HOME_DOMAIN=blnt.trade` during deployment, then publish
the following currency entry at `https://blnt.trade/.well-known/stellar.toml`,
replacing the issuer placeholder with the deployment operator account:

```toml
NETWORK_PASSPHRASE = "Test SDF Network ; September 2015"

[[CURRENCIES]]
code = "BLNT"
issuer = "<deployment operator account>"
display_decimals = 7
name = "Blent Token"
desc = "Testnet BLNT used by Blend V3"
image = "https://blnt.trade/icons/tokens/blnd-yellow.svg"
```

## Recorded evidence

The network-specific `dual-emitter-state.json` and timestamped run directory
record:

- exact legacy and v3 emitter WASM hashes;
- the pinned Comet V1.1 source commit and official release WASM hash;
- BLND, BLNT, USDC, and XLM contract IDs;
- the BLNT metadata name `Blent Token` and configured issuer home domain;
- BLND:USDC, BLNT:USDC, and BLNT:XLM Comet V1.1 IDs;
- separate emitter and backstop IDs;
- the backfill-contract artifact hash, address, and immutable token bindings;
- the immutable 180-day backfill and 360-day grant vesting timestamps and
  allocation totals;
- both token administrators and emitter recipients;
- direct v3 activation and exact 150-million-BLNT backfill custody;
- exact same-user 1:1 BLND burn and pre-funded BLNT transfer events;
- positive independent BLND and BLNT distributions;
- active modeled Fixed and Custom pools in their respective V2 and V3 reward
  zones, including verified seven-day V2 USDC-supply emission streams; and
- a dedicated local V3 pool where auction types 0, 1, 2, and 3 were each
  started, filled, removed, and checked for their corresponding position,
  custody, donation, tier-impairment, or BLNT-burn balance deltas; and
- the immutable 51-million-BLNT output cap, 51-million-BLND input capacity,
  270-day deadline, zero initial BLNT expiry burns, and empty initial v3 queue.

The runner initializes all three Comet V1.1 pools at 80:20, verifies positive authorized
non-clawbackable custody, and then locks their shared classic controller at
100/100/100 thresholds with its only signer weight set to zero.

## Future upgrade

The initial v3 designated token is canonical BLNT:XLM LP. A future protocol
does not use this launch runner to replace v3; it calls the v3 emitter's
preserved `queue_swap_backstop` and `swap_backstop` entry points after directly
holding strictly more of that designated token than the incumbent.
