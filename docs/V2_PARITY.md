# V2 parity matrix

The candidate treats deployed Blend v2 behavior as the default. A v3
difference must be required by the three-tier backstop, legacy migration,
Protocol 27, a conservation/safety fix, or an explicitly approved exception in
the v3 specification. The identifier/section column ties every row to
the composed normative specification.

| Area | V2 baseline | V3 policy | Normative reference | Status |
| --- | --- | --- | --- | --- |
| Runtime | Rust 1.81, SDK 22 | Rust 1.91, SDK 27 | `V2-RUNTIME-001`; v3 §2.0 | Implemented |
| Pool admin | Transferable two-step admin | Same | `V2-POOL-ADMIN-001`; v3 §8.1 | Implemented |
| Pool parameters | Admin-updatable take rate, maximum positions, and minimum collateral | Same | `V2-POOL-ADMIN-001`; v3 §8.1 | Implemented |
| Reserve configuration | Admin queue/cancel, seven-day post-Setup delay, permissionless apply | Same | `V2-POOL-CONFIG-001`; v3 §8.1-8.2 | Implemented |
| Status codes and action matrix | Codes 0-6 and v2 Q4W boundaries | Same | `V2-POOL-STATUS-001` and `002`; v3 §8.1 | Implemented |
| Submission entry points | Direct and per-asset-netted allowance submissions | Same, with exact balance-delta verification | `V2-LENDING-001`; v3 §8.2 | Implemented safety extension |
| Flash loans | Zero-fee borrow before callback and allowance-settled requests | Same, with exact balance-delta verification | `V2-LENDING-001`; v3 §8.2 | Implemented safety extension |
| Request-count bound | No separate batch-length bound | Maximum 60 requests per submission | v3 §8.2 | Implemented Protocol-27 safety extension |
| Activation | BLND/USDC product threshold | Aggregate verified USDC across three tiers | `V2-BACKSTOP-004`; v3 §8 | Implemented tier delta |
| Activation hysteresis | One threshold | 12,500 USDC entry; 10,000 USDC maintenance | `V2-BACKSTOP-004`; v3 §8 and §8.1 | Approved exception |
| Q4W basis | Shares of one LP token | Verified value across three tier assets | `V2-POOL-STATUS-002`; v3 §8.1 | Implemented tier delta |
| Reward zone | Permissionless, maximum 30 pools | Same shape; positive-BLND entry and zero-weight liveness removal | `V2-BACKSTOP-005`; v3 §10.1 | Implemented safety fix |
| Ongoing split | 70% backstop / 30% supply and borrow | Same, with scoped carry-forward | `V2-EMISSIONS-002`; v3 §10.1 | Implemented end to end |
| Backstop accrual timing | Pool gulp streams the 70% tranche over seven days | Successful distribution checkpoints assign it immediately through two tier indexes | `V2-BACKSTOP-006`; v3 §10.1 | Implemented tier delta |
| BLND receipt binding | Emitter owns the canonical token | Configured token must pass an exact emitter-mint balance delta | `V2-EMITTER-001`; v3 §10 | Implemented safety fix |
| Pool emission weight | Nonqueued BLND:USDC LP tokens | Underlying BLND across nonqueued BLND:USDC and BLND:XLM | `V2-BACKSTOP-006`, `V2-EMISSIONS-001`; v3 §10.1 | Distribution and five-second mutation guard implemented |
| Backstop user weight | Nonqueued backstop shares | User underlying BLND across the same two tiers | `V2-BACKSTOP-006`; v3 §10.1 | Per-tier indexes and user claims implemented |
| Backstop claim destination | Claimed BLND compounds into the user's selected BLND:USDC backstop positions | Claimed BLND is paid directly to the owner-selected recipient | `V2-BACKSTOP-006`; v3 §10.1 | Implemented tier delta |
| Plain-USDC emissions | Not applicable | Zero weight | v3 §10.1 | Formula implemented |
| 30% configuration | Mutable relative supply/debt-token weights; empty accepted and duplicate identifiers use the last weight | Same semantics with a 60-entry input bound; empty-config gulp fails before candidate reservation | `V2-EMISSIONS-003`; v3 §10.2 | Configuration, streams, accrual, and claims implemented |
| Pending bad-debt emissions | Capital remains until disposed | Eligible until an actual fill transfers assets | `V2-AUCTION-003`; v3 §9 and §10.1 | Pool active-asset accounting implemented |
| Take-rate realization | One backstop-token interest auction | V2-shaped tier-specific auctions with `5:5:4` allocation | `V2-AUCTION-004`; v3 §9.1 | Implemented tier delta |
| Bad-debt realization | One backstop-token auction, then supplier loss | One tier per auction in strict waterfall order, then supplier loss | `V2-AUCTION-003`; v3 §9 | Implemented tier delta |
| Rounding dust | Some floor remainders remain unattributed | Carry forward at the same accounting scope | `V2-SAFETY-001`; v3 §1.1, §9.1, and §10 | Top-level, reward-zone, tier-index, and user carry implemented |
| Protocol replacement | Legacy emitter queue and swap | Same mechanism with v3 preparation and synchronization | `V2-EMITTER-002`; v3 §4, §6, and §12 | Implemented |
| Incumbent positions after replacement | Existing positions remain in their deployed v2 contracts | Same; emitter replacement moves no lending or backstop position | `V2-ISOLATION-001`; v3 §15 | Settled isolation policy |
| Incumbent emissions after replacement | Registered v2 backstop receives emitter distributions | While v3 remains registered, new distributions follow the emitter to v3; existing v2 accrual remains under v2 rules | `V2-EMITTER-001`, `V2-ISOLATION-001`; v3 §6 and §10 | Implemented emitter behavior |
| Migration distribution | Emitter permits a bounded initial drop | Zero discretionary award; at most 10M BLND backfill | `V2-EMITTER-003`, `V2-BACKSTOP-007`; v3 §10 | Implemented migration policy |

Implementation work should update this matrix whenever a parity gap closes or
a proposed behavior would create a new v2 departure.
