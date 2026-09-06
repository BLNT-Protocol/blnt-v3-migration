# Blend V2-to-V3 Token and Emitter Transition Specification

This document defines the approved transition from legacy BLND emissions in
Blend v1/v2 to independent BLNT emissions in Blend v3. It composes with
[SYSTEM_SPEC.md](SYSTEM_SPEC.md) and the production-contract requirements in
the pinned `blnt-contracts-v3` and `blnt-backfill-contract` submodules.

Normative terms `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are acceptance
criteria.

## 1. Independent protocol lanes

The deployed legacy emitter, BLND token, v1/v2 backstops, and existing v1/v2
positions remain unchanged. The legacy emitter continues to point at its v2
backstop and MAY continue distributing BLND according to its deployed rules.
V3 MUST NOT queue a swap against that emitter, become its recipient, obtain
BLND emission authority, or use BLND as a reserve-emission or backstop asset.

V3 deploys a new emitter and a new seven-decimal BLNT Stellar Asset Contract.
The v3 emitter is the BLNT administrator and begins with the v3 backstop as its
recipient. V3 emission accounting, reward-zone weight, pool emissions,
buy-and-burn, and canonical LP valuation use BLNT only.

Creating the v3 emitter while the legacy emitter can still mint BLND permits a
temporary period of independent issuance. That overlap is an explicit launch
decision, not a claim on either protocol lane.

## 2. V3 emitter

### 2.1 Compatibility surface

The v3 emitter preserves the deployed emitter's public `initialize`,
`distribute`, `get_last_distro`, `get_backstop`, `queue_swap_backstop`,
`get_queued_swap`, `cancel_swap_backstop`, `swap_backstop`, and `drop`
encodings. The `initialize` parameter historically named `blnd_token` contains
BLNT in v3.

Initialization MUST verify that BLNT is a seven-decimal Stellar Asset Contract
administered by the emitter, set the v3 backstop as recipient, and set canonical
BLNT:XLM LP as the designated replacement-comparison token. Once initialized,
the emitter mints one BLNT per elapsed second to its current backstop. Only that
current backstop may invoke the emitter's `distribute`; callers retain
permissionless checkpoint access through the backstop's own `distribute` entry
point.

The v3 backstop's first `distribute()` MUST activate directly when the emitter
already names it and no prior replacement epoch exists. V3 launch requires no
legacy-emitter queue, direct-balance contest, 31-day wait, migration backfill,
or skipped emission window.

### 2.2 Initial allocation

The inherited `drop` lifecycle supplies the initial BLNT allocation. It is
callable by the current backstop once for that backstop address. V3 launch MUST
use a one-entry immutable recipient list that transfers exactly 150 million
BLNT to the separately deployed BLNT backfill contract. Re-registering the same
backstop MUST NOT restore drop eligibility.

### 2.3 Legacy BLND conversion

The v3 emitter constructor records only a one-time initialization authority.
Initialization MUST require and permanently discard that authority, preventing
first-call races without creating an ongoing administrator. The emitter MUST
NOT bind legacy BLND or expose a conversion entry point.

The separately deployed `blnt-backfill-contract` immutably binds distinct
seven-decimal legacy BLND and BLNT Stellar Asset Contracts and receives the
complete 150-million-BLNT initial drop without receiving BLNT mint authority.
Its immutable backfill list MUST contain at most 434 unique claimants, MUST
contain only positive amounts, and MUST total no more than 74 million BLNT. The
production list MUST assign exactly 74 million BLNT in proportion to
each of the 423 account and 11 contract addresses with a positive
`flattened_total_cpal_raw` value in the committed pre-incident ownership
snapshot, using deterministic largest-remainder rounding. Each claimant's
allocation MUST vest linearly per second over 180 days from construction. A
claimant-authorized `claim_backfill(user)` transfers all cumulative vested BLNT
not previously claimed back to that same address and MAY be called repeatedly
as vesting advances.
Claims do not expire after vesting completes.

Construction MUST also accept an independent immutable grant list containing
at most 100 unique positive allocations totaling at most 25 million BLNT.
`claim_grant(user)` MUST transfer vested BLNT back to that same authorized
address, use separate progress, and linearly vest grants per second for 360 days
from the shared construction timestamp. An address MAY appear in both lists
without either method reading or consuming the other allocation. Backfill and
grant allocations together MUST NOT exceed 99 million BLNT, preserving the
separate 51-million-BLNT conversion reserve.

The backfill contract owns `swap_blnd_for_blnt(user, blnt_amount)`, which:

1. requires `user` authorization and a positive `blnt_amount`;
2. transfers exactly `2 * blnt_amount` legacy BLND from `user` to the contract;
3. verifies the exact receipt and immediately burns that BLND;
4. transfers exactly `blnt_amount` of its pre-funded BLNT back to `user`; and
5. increments cumulative successful BLNT output by `blnt_amount`.

The conversion MUST NOT expose a refund path or retain successfully converted
BLND in escrow. Cumulative BLNT output MUST NOT exceed 51 million BLNT.
Conversion MUST expire exactly 270 days after backfill-contract construction;
swaps at or after that timestamp MUST fail. Each operation MUST fail atomically
on a transfer, burn, payout, balance, authorization, cap, deadline, overflow,
or reentrancy mismatch.

After expiry, permissionless `burn_expired()` MUST destroy exactly the unused
BLNT conversion reserve while
preserving every outstanding backfill and grant claim. It MUST reject before
expiry and return zero after the reserve has already been finalized.
The contract exposes immutable token bindings, vesting and conversion
timestamps, currently claimable amounts, separate and aggregate claim
accounting, cumulative BLNT output and BLND burn accounting, expiry burn
accounting, and remaining pre-expiry conversion capacity as read-only views. It
exposes no administrator, upgrade, mint, sweep, recovery, refund,
allocation-mutation, or claim-deadline API.

## 3. Canonical v3 liquidity

V3 deploys new canonical 80:20 BLNT:USDC and BLNT:XLM LPs from the Protocol-27
Comet v2 source pinned in the workspace directory `comet-contracts-v2`.

Both pools MUST contain exactly their expected seven-decimal Stellar Asset
Contracts, use normalized weights 80% BLNT and 20% paired asset, start
unfrozen, and satisfy the controller-lock and non-clawbackable-custody
requirements in the v3 contract specification. No BLND-bearing LP is a v3
backstop asset or v3 emissions weight.

## 4. Legacy position isolation

Deploying v3 MUST NOT move, convert, close, import, or rewrite any v1/v2
supplier, borrower, collateral, debt, auction, backstop, or emission position.
BLND already held, allocated, or accrued remains governed by the legacy
contracts. Conversion is an explicit holder action against free BLND balance;
it does not traverse or mutate a legacy protocol position.

The legacy emitter and v3 emitter maintain separate recipients, distribution
clocks, designated tokens, queues, and drop records. A transaction in one lane
MUST NOT update accounting in the other.

## 5. Future protocol replacement

The v3 emitter retains the deployed permissionless replacement mechanism so a
future protocol version can become the BLNT recipient. A queue candidate MUST
directly hold strictly more of the emitter's current designated token than the
incumbent backstop. For the initial v3 binding, that token is canonical
BLNT:XLM LP; equality is insufficient.

Queue creation records the proposed backstop, proposed next designated token,
and an unlock exactly 31 days later. The current designated token remains the
comparison asset throughout the queue. Finalization is available through
exactly seven days after unlock, revalidates the strict balance, distributes
the incumbent's accrued BLNT, changes recipient and designated token, and
starts the new recipient's clock. Before expiration, cancellation remains
permissionless only after the candidate loses qualification. After expiration,
any caller may use the same cancellation entry point regardless of balance and
then create a fresh queue through the existing queue entry point. The stored
queue and public method encodings do not change. Recipient activation remains
a separate permissionless backstop checkpoint; the emitter stores no
activation or pause state, and an inactive recipient does not gate a later
qualified queue or replacement.

The emitter adds no administrator, governance, multisig, privileged WASM
replacement, emergency override, or alternate protocol-upgrade path.

## 6. Deployment ordering

A deployment SHOULD use the following deterministic order:

1. retain and verify the existing legacy BLND emitter and v2 binding;
2. deploy the v3 emitter with the one-time initializer as its constructor
   input;
3. create BLNT with the v3 emitter as administrator;
4. deploy the BLNT backfill contract bound to legacy BLND, BLNT, and the
   immutable backfill and grant lists;
5. deploy and initialize the canonical Comet v2 BLNT:USDC and BLNT:XLM pools;
6. determine the v3 backstop address and deploy its factory;
7. initialize the v3 emitter with BLNT, the v3 backstop, and BLNT:XLM LP;
8. deploy the v3 backstop with the single 150-million-BLNT backfill-contract
   drop recipient;
9. call v3 backstop `distribute()` once to activate direct accounting; and
10. call v3 backstop `drop()` and verify exact backfill-contract custody.

Deployment evidence MUST record both emitter bindings, both token
administrators, the backfill-contract artifact and immutable bindings, exact
150-million-BLNT custody, the immutable 180-day backfill and 360-day grant
vesting timestamps and allocations, the
51-million-BLNT output cap,
102-million-BLND input capacity, and 270-day deadline, Comet v2 artifact hash
and pool configuration, v3 direct
activation, and the absence of BLND in every v3 pool, backstop, and emitter
binding.
