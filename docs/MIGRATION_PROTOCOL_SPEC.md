# Blend V2-to-V3 Token and Emitter Transition Specification

This document defines the approved transition from legacy BLND emissions in
Blend v1/v2 to independent BLNT emissions in Blend v3. It composes with
[SYSTEM_SPEC.md](SYSTEM_SPEC.md) and the production-contract requirements in
the pinned `blend-contracts-v3` and `blnt-backfill-contract` submodules.

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
BLNT:USDC LP as the designated replacement-comparison token. Once initialized,
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
use a one-entry immutable recipient list that transfers exactly 50 million
BLNT to the separately deployed BLNT backfill contract. Re-registering the same
backstop MUST NOT restore drop eligibility.

### 2.3 Legacy BLND conversion

The v3 emitter constructor records only a one-time initialization authority.
Initialization MUST require and permanently discard that authority, preventing
first-call races without creating an ongoing administrator. The emitter MUST
NOT bind legacy BLND or expose a conversion entry point.

The separately deployed `blnt-backfill-contract` immutably binds distinct
seven-decimal legacy BLND and BLNT Stellar Asset Contracts and receives the
complete 50-million-BLNT initial drop without receiving BLNT mint authority.
Its immutable address claim list MUST contain at most 100 unique claimants,
MUST contain only positive amounts, and MUST total no more than 30 million
BLNT. A claimant-authorized `claim(claimant, to)` transfers the claimant's
complete allocation exactly once. Claims do not expire.

The backfill contract owns `swap_blnd_for_blnt(from, to, amount)`, which:

1. requires `from` authorization and a positive amount;
2. transfers exactly `amount` legacy BLND from `from` to the backfill contract;
3. verifies the exact receipt and burns it completely;
4. transfers exactly the same raw seven-decimal amount of its pre-funded BLNT
   to `to`; and
5. increments the cumulative successful swap total.

The cumulative successful conversion total MUST NOT exceed 20 million BLNT.
Conversion does not expire. The operation MUST fail atomically on a transfer,
burn, payout, balance, authorization, cap, overflow, or reentrancy mismatch.
No BLNT-to-BLND path exists. The contract exposes immutable token bindings,
claimable amounts, aggregate claim and conversion accounting, and remaining
conversion capacity as read-only views. It exposes no administrator, upgrade,
mint, sweep, recovery, allocation-mutation, or deadline API.

## 3. Canonical v3 liquidity

V3 deploys new canonical 80:20 BLNT:USDC and BLNT:XLM LPs from the Protocol-27
Comet v2 source pinned in the workspace directory named
`comet-contracts-v1`. The directory name is historical; its pinned checkout is
the Comet v2 implementation used by this repository.

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
BLNT:USDC LP; equality is insufficient.

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
   immutable address claim list;
5. deploy and initialize the canonical Comet v2 BLNT:USDC and BLNT:XLM pools;
6. determine the v3 backstop address and deploy its factory;
7. initialize the v3 emitter with BLNT, the v3 backstop, and BLNT:USDC LP;
8. deploy the v3 backstop with the single 50-million-BLNT backfill-contract
   drop recipient;
9. call v3 backstop `distribute()` once to activate direct accounting; and
10. call v3 backstop `drop()` and verify exact backfill-contract custody.

Deployment evidence MUST record both emitter bindings, both token
administrators, the backfill-contract artifact and immutable bindings, exact
50-million-BLNT custody, the 20-million conversion cap, Comet v2 artifact hash
and pool configuration, v3 direct activation, and the absence of BLND in every
v3 pool, backstop, and emitter binding.
