# Dual-Emitter BLND/BLNT Deployment

The active deployment lane uses independent emitters: the exact deployed
legacy emitter continues BLND to v2, while the new v3 emitter sends BLNT to v3
from launch. It deploys canonical BLNT:USDC and BLNT:XLM pools from the
workspace's Comet V2 artifact and does not queue a legacy-emitter replacement.
The v3 emitter constructor binds the deployment operator as a one-time
initializer; successful initialization consumes that authority, leaving no
ongoing emitter administrator.

## Local Quickstart

Preview and execute the complete isolated deployment:

```bash
make dual-emitter-plan
make dual-emitter-run
make dual-emitter-status
```

`run` verifies and builds artifacts, starts Protocol-27 Quickstart, deploys the
two independent lanes, activates v3 directly, executes the empty initial BLNT
drop, converts a small test balance from BLND to BLNT, advances each emitter,
and verifies all final bindings. Stop the isolated network with:

```bash
make dual-emitter-stop
```

To reuse an already-running network, run `make dual-emitter-deploy` after
`make dual-emitter-validate`.

## Public testnet

The same script uses production-style public-testnet transaction assembly:

```bash
make testnet-dual-emitter-validate
make testnet-dual-emitter-run
make testnet-dual-emitter-status
```

Signing identities live only below the ignored testnet Stellar configuration
directory. The runner never reads a mainnet wallet or submits to mainnet.

## Recorded evidence

The network-specific `dual-emitter-state.json` and timestamped run directory
record:

- exact legacy and v3 emitter WASM hashes;
- the Comet V2 source commit and optimized WASM hash;
- BLND, BLNT, USDC, and XLM contract IDs;
- BLND:USDC, BLNT:USDC, and BLNT:XLM Comet V2 IDs;
- separate emitter and backstop IDs;
- both token administrators and emitter recipients;
- direct v3 activation and the one-call initial drop;
- exact BLND burn and BLNT mint conversion events;
- positive independent BLND and BLNT distributions; and
- the immutable conversion deadline and empty initial v3 queue.

The runner initializes all three Comets at 80:20, verifies positive authorized
non-clawbackable custody, and then locks their shared classic controller at
100/100/100 thresholds with its only signer weight set to zero.

## Future upgrade

The initial v3 designated token is canonical BLNT:USDC LP. A future protocol
does not use this launch runner to replace v3; it calls the v3 emitter's
preserved `queue_swap_backstop` and `swap_backstop` entry points after directly
holding strictly more of that designated token than the incumbent.
