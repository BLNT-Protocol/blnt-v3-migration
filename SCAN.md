# Security Scan — BLNT v3 candidate contracts

Date: 2026-09-04
Scope: `blnt-contracts-v3/{pool,backstop,emitter,pool-factory,compat}`, `blnt-backfill-contract`,
diffed against the frozen `blend-contracts-v2` sources to isolate v3-introduced behavior.

This is a code review, not an audit. The `comet-contracts-v2` fork was **not** audited, and
findings 1 and 2 both depend on it.

Build and tests are clean at the time of scan (134 + 24 + 351 + 14 unit tests, plus all of
`test-suites`), so none of the findings below are caught by the existing suites.

---

## 1. HIGH — Auction quotes are frozen at a manipulable Comet spot valuation

**Where:** `backstop/src/backstop/pool.rs`, `pool/src/auctions/tier_interest.rs`,
`pool/src/auctions/protocol_fee_auction.rs`, `pool/src/auctions/tier_bad_debt.rs`

`backstop/src/backstop/pool.rs` values every backstop tier from the *current* Comet v2 reserves:
LP value = `pair_reserve * 5 * amount / total_supply` (`pool_tier_valuation`), and
`quote_blnt_price` = `4 * pair_reserve / blnt_reserve`.

Three v3 auction paths bake that spot number into a stored auction that is filled in a **later**
ledger:

- `tier_interest.rs:build_interest_bid` — `bid_amount = ceil(1.2 * lot_value * tokens / value)`.
  Inflate the tier's `value` and the LP bid shrinks proportionally.
- `protocol_fee_auction.rs:74` — `bid_amount = ceil(target_value * SCALAR_7 / blnt_price)`.
  Inflate `blnt_price` and the BLNT bid shrinks.
- `tier_bad_debt.rs:allocate_bad_debt_tier` — `lot_amount = ceil(target_value * tokens / value)`,
  and `available_value <= target_value` returns **the entire tier**. Deflate `value` and the lot
  balloons.

**Exploit path.** `new_auction` (`pool/src/contract.rs:677`) is permissionless and
`create_*_auction_data` sets `block = sequence + 1`, so a single transaction can swap the Comet
pool, create the auction at the skewed quote, and unwind the swap. The mispriced auction persists
and is fillable on the next ledger at true prices. Nothing revalidates the quote at fill.
`has_tier_auction` / `has_auction` blocks a competing honest auction, and `del_auction` requires
500 stale ledgers (~42 min), so the mispriced auction cannot be cancelled in time.

**Cost.** For an 80:20 pool, doubling `usdc/blnt` requires roughly +74% USDC / -13% BLNT on the
reserves; the round trip costs about two swap fees plus arbitrage leakage, against a lot that can
be arbitrarily large.

**v2 comparison.** v2 had one instance of this (`token_spot_price` in its bad-debt lot).
`AGENTS.md` forbids adding a price oracle here, but v3 widened the blast radius from one lot
calculation to both new auction *bid* prices plus activation, pool status, and reward-zone weight.

**Mitigations that do not add an oracle:** revalidate the quote at fill (or bound the fill to a
band around the live quote), snapshot reserves across a delay, or reject creation when reserves
moved sharply in the same ledger.

---

## 2. HIGH — The v3 emitter's designated comparison token makes takeover cheap

**Where:** `emitter/src/backstop_manager.rs:is_new_backstop_is_larger`,
`scripts/v3-only-deployment.sh:525`

`is_new_backstop_is_larger` compares raw balances of `backstop_token`, and the launch emitter is
initialized with `--backstop_token ${blnt_xlm}`.

In v2 that token *was* the sole backstop deposit token, so "more deposits" meant "bigger
backstop." In v3 the backstop custodies up to four assets across three tiers and pools pick 1-3 of
them, so the bar for replacing the emitter is only the backstop's **BLNT:XLM LP** balance — which
may be a small minority of TVL, or near zero.

The repo's own fixture states the consequence plainly (`test-suites/src/test_fixture.rs:141`):

> "One unattributed LP base unit is sufficient because the synthetic incumbent holds none."

The balance is also unattributed — a plain token transfer counts — so this is a snapshot check at
queue time and at execute time, not a lock-up.

**Impact.** A winner receives the full 1 BLNT/sec emission stream plus a fresh `drop` of up to
`MAX_MIGRATION_DROP` = 50M BLNT (`emitter/src/emitter.rs:36`). The 31-day queue plus community
top-ups is the only defense, and it degenerates into a capital race over one minority asset.

**Mitigation.** If BLNT:XLM is the intended designation, compare aggregate configured-tier value
rather than a single token balance.

---

## 3. MEDIUM — Bad-debt auction can be left with an empty bid, giving the next filler a free lot

**Where:** `pool/src/auctions/tier_bad_debt.rs:scale_prepared_bad_debt_auction`,
`fill_bad_debt_auction`

The bid is scaled with **ceil** and the lot with **floor**, and the residual is stored with no
guard:

```rust
let complete = remaining_bid.is_empty() && remaining_lot_amount == 0;
...
if !fill.complete {
    set_prepared_bad_debt_auction(e, &TierAuctionData {
        auction: AuctionData { bid: remaining_bid, lot: map![e, (…, remaining_lot_amount)], … },
        …
    });
}
```

With a dust bid (`amount <= 100/(100-percent)`, e.g. any dToken amount <= 99 at `percent = 99`),
`ceil` consumes the bid entirely while the floored lot leaves a remainder. The stored auction then
has `bid = {}` and `lot > 0`. On the next fill the empty bid map is scaled to nothing,
`rm_positions` / `add_positions` are no-ops, and `draw` still transfers the lot to the filler for
nothing.

It also blocks `default_backstop_bad_debt` and any new bad-debt auction
(`require_no_active_bad_debt_auction`) until the 500-ledger stale window opens.

**This path is the outlier.** Both siblings already carry exactly the guard that prevents it:
`store_remaining_protocol_fee_auction` and `store_remaining_interest_auction` each do
`if remaining_bid <= 0 { panic InvalidLot }`.

**Fix.** Apply the same treatment in the bad-debt path — treat an empty residual bid as complete,
or reject it.

**Chaining.** With finding 1, crushing a tier's `value` makes `allocate_bad_debt_tier` hand over
the *whole* tier as the lot against a dust bid.

---

## 4. LOW — Initial drop of 150M conflicts with the stated 50M emitter ceiling

**Where:** `emitter/src/constants.rs`, `backstop/src/constants.rs`

`MAX_INITIAL_DROP = 150_000_000 * SCALAR_7`, while `AGENTS.md` states the initial-drop allocation
plus the 10M backfill "must fit within the emitter's 50-million-BLND ceiling."

The migration path is internally consistent (40M list + 10M backfill = 50M), and the 150M matches
`blnt-backfill-contract` funding (74M backfill + 25M grants + 51M conversion), so this appears
deliberate (commit `848f62d`). Under the repo's own specification discipline it still needs a
recorded exception, since it triples launch-time mint authority.

The constructor guard (`backstop/src/contract.rs:200`) only permits the extended drop when the
emitter already names this contract, which the deployment script satisfies via a predicted address.

---

## 5. INFO — Smaller items

- **Stale cached emission weights.** `backstop/src/emissions/claim.rs:70` returns early (and `:131`
  `continue`s) after `prepare_pool_weight_change`, so `finish_pool_weight_change` never runs and the
  cached `active_blnt_*` weights stay stale until the next weight change. No value leak, but the
  next allocation checkpoint can read stale weights.

- **Non-SAC USDC binding would permanently trap valuation.** `configured_tier_is_transferable`
  (`backstop/src/backstop/pool.rs`) calls `StellarAssetClient::authorized` directly, while
  `Reserve::is_authorized` deliberately uses `try_authorized` with a fallback.
  `validate_backstop_assets` never checks `executable() == StellarAsset` for USDC/XLM/BLNT — only
  the emitter checks it, and only for BLNT. A non-SAC USDC binding would trap valuation for every
  pool with a USDC tier, and the bindings are immutable.

---

## Reviewed and found clean

- `overflow-checks = true` in all three workspaces.
- `compat/soroban-fixed-point-math` floor/ceil implementations — correct in all four sign quadrants.
- Pool user / liquidation / health / interest logic — byte-identical to v2 apart from the
  deauthorized-collateral gate.
- Q4W queue / dequeue / withdraw ordering and the aggregate 20-entry cap.
- Emission index and carry math in both distributors.
- Backstop storage key derivation across tiers.
- The pool-to-backstop ABI (imported from the built WASM, so no struct drift).
- `blnt-backfill-contract` vesting, caps, and `burn_expired` reserve preservation.
- Authorization on every external entry point.
