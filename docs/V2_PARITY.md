# V2 parity matrix

The candidate treats deployed Blend v2 behavior as the default. A v3
difference must be required by the three-tier backstop, legacy migration,
Protocol 27, a conservation/safety fix, or an explicitly approved exception in
the system specification.

| Area | V2 baseline | V3 policy | Status |
| --- | --- | --- | --- |
| Runtime | Rust 1.81, SDK 22 | Rust 1.91, SDK 27 | Implemented |
| Pool admin | Transferable two-step admin | Same | Implemented |
| Pool parameters | Admin-updatable take rate, maximum positions, and minimum collateral | Same | Implemented |
| Reserve configuration | Admin queue/cancel, seven-day post-Setup delay, permissionless apply | Same | Implemented |
| Status codes and action matrix | Codes 0-6 and v2 Q4W boundaries | Same | Implemented |
| Activation | BLND/USDC product threshold | Aggregate verified USDC across three tiers | Implemented tier delta |
| Activation hysteresis | One threshold | 12,500 USDC entry; 10,000 USDC maintenance | Approved exception |
| Q4W basis | Shares of one LP token | Verified value across three tier assets | Implemented tier delta |
| Reward zone | Permissionless, maximum 30 pools | Same shape; positive-BLND entry and zero-weight liveness removal | Implemented safety fix |
| Ongoing split | 70% backstop / 30% supply and borrow | Same, with scoped carry-forward | Backstop distribution and claims implemented |
| BLND receipt binding | Emitter owns the canonical token | Configured token must pass an exact emitter-mint balance delta | Implemented safety fix |
| Pool emission weight | Nonqueued BLND:USDC LP tokens | Underlying BLND across nonqueued BLND:USDC and BLND:XLM | Distribution and five-second mutation guard implemented |
| Backstop user weight | Nonqueued backstop shares | User underlying BLND across the same two tiers | Per-tier indexes and user claims implemented |
| Plain-USDC emissions | Not applicable | Zero weight | Formula implemented |
| 30% configuration | Mutable relative supply/debt-token weights selected by pool admin | Same | Specified; not implemented |
| Pending bad-debt emissions | Capital remains until disposed | Eligible until an actual fill transfers assets | Pool active-asset accounting implemented |
| Take-rate realization | One backstop-token interest auction | V2-shaped tier-specific auctions with `5:5:4` allocation | Implemented tier delta |
| Bad-debt realization | One backstop-token auction, then supplier loss | One tier per auction in strict waterfall order, then supplier loss | Implemented tier delta |
| Rounding dust | Some floor remainders remain unattributed | Carry forward at the same accounting scope | Top-level, reward-zone, tier-index, and user carry implemented |
| Protocol replacement | Legacy emitter queue and swap | Same | Implemented |
| Migration distribution | Emitter permits a bounded initial drop | Zero discretionary award; at most 10M BLND backfill | Implemented migration policy |

Implementation work should update this matrix whenever a parity gap closes or
a proposed behavior would create a new v2 departure.
