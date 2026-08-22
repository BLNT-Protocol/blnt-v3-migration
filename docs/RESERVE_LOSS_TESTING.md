# Reserve-loss test pool

The reserve-loss fixture deploys a separate Blend v3 factory, backstop, and
pool against the current local contract artifacts. It reuses the migration
lane's canonical BLND, USDC, XLM, and Comet contracts, but it does not replace
the Fixed or Custom pools or redirect the emitter. Its isolated reward zone is
therefore suitable for activation and accounting tests, not live BLND emission
tests.

The pool has three synthetic, seven-decimal reserve assets. Each is priced at
$1 by a controlled test oracle and begins with 1,000 supplied units:

| Asset | Exercise | Final state |
| --- | --- | --- |
| `HLTY` | The issuer calls `pool.clawback` for 250 units. | Custody and the user's bTokens both fall to 750; `b_rate` remains 1.0. |
| `IMPR` | The issuer directly SAC-claws back 400 pool-held units, then anyone calls `reconcile_loss`. | The 1,000 bTokens remain and `b_rate` falls to 0.6. |
| `EXHT` | The issuer directly SAC-claws back all 1,000 pool-held units, then anyone calls `reconcile_loss`. | The 1,000 worthless bTokens remain and `b_rate` becomes zero. |

The dedicated asset issuer enables clawback before any reserve trustline is
created. Verification confirms all three pool balance entries are authorized
and clawbackable. The shared backstop is funded with 12,500 USDC plus 10
BLND:USDC LP tokens, verifies that its USDC balance is non-clawbackable,
activates the pool, and adds it to the isolated reward zone.

Run the complete resumable scenario with:

```bash
make localnet-deploy-reserve-loss-pool
make testnet-production-deploy-reserve-loss-pool
```

Recheck the resulting on-chain state without mutation with:

```bash
make localnet-verify-reserve-loss-pool
make testnet-production-verify-reserve-loss-pool
```

The deployment identifiers and timestamps are recorded under `reserve_loss_*`
keys in the network's ignored `state.json`.
