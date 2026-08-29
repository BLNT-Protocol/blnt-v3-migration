# Blend V2-to-V3 Token and Emitter Transition Specification

This document defines the approved transition from legacy BLND emissions in
Blend v1/v2 to independent BLNT emissions in Blend v3. It composes with
[SYSTEM_SPEC.md](SYSTEM_SPEC.md) and the production-contract requirements in
the pinned `blend-contracts-v3` submodule.

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

Initialization MUST bind distinct seven-decimal legacy BLND and BLNT tokens,
verify that BLNT is a Stellar Asset Contract administered by the emitter, set
the v3 backstop as recipient, and set canonical BLNT:USDC LP as the designated
replacement-comparison token. Once initialized, the emitter mints one BLNT per
elapsed second to its current backstop.

The v3 backstop's first `distribute()` MUST activate directly when the emitter
already names it and no prior replacement epoch exists. V3 launch requires no
legacy-emitter queue, direct-balance contest, 31-day wait, migration backfill,
or skipped emission window.

### 2.2 Initial allocation

The inherited `drop` lifecycle supplies the optional initial BLNT allocation.
It is callable by the current backstop once for that backstop address, and its
immutable recipient list MUST total no more than 50 million BLNT. Re-registering
the same backstop MUST NOT restore drop eligibility.

### 2.3 Legacy BLND conversion

The v3 emitter constructor immutably binds the legacy BLND token, records a
one-time initialization authority, and fixes an exclusive deadline exactly 60
days after emitter instantiation. Initialization MUST require that authority
and permanently discard it on success, preventing first-call races without
creating an ongoing administrator. Before the deadline,
deadline, `swap_blnd_for_blnt(from, to, amount)`:

1. requires `from` authorization and a positive amount;
2. transfers exactly `amount` legacy BLND from `from` to the emitter;
3. verifies the exact receipt and burns it completely;
4. mints exactly the same raw seven-decimal amount of BLNT to `to`; and
5. increments the cumulative successful swap total.

The operation MUST fail atomically on a transfer, burn, mint, balance,
authorization, overflow, or reentrancy mismatch. A call at or after the
deadline fails, the deadline cannot be extended, and no BLNT-to-BLND path
exists. The contract exposes the deadline, immutable legacy token, and
cumulative swapped amount as read-only views.

## 3. Canonical v3 liquidity

V3 deploys new canonical 80:20 BLNT:USDC and BLNT:XLM LPs from the Protocol-27
Comet V2 source pinned in the workspace directory named
`comet-contracts-v1`. The directory name is historical; its pinned checkout is
the Comet V2 implementation used by this repository.

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
comparison asset throughout the queue. Finalization revalidates the strict
balance, distributes the incumbent's accrued BLNT, changes recipient and
designated token, and starts the new recipient's clock. Cancellation remains
permissionless only after the candidate loses qualification.

The emitter adds no administrator, governance, multisig, privileged WASM
replacement, emergency override, or alternate protocol-upgrade path.

## 6. Deployment ordering

A deployment SHOULD use the following deterministic order:

1. retain and verify the existing legacy BLND emitter and v2 binding;
2. deploy the v3 emitter with legacy BLND and the one-time initializer as its
   constructor inputs;
3. create BLNT with the v3 emitter as administrator;
4. deploy and initialize the canonical Comet V2 BLNT:USDC and BLNT:XLM pools;
5. determine the v3 backstop address and deploy its factory;
6. initialize the v3 emitter with BLNT, the v3 backstop, and BLNT:USDC LP;
7. deploy the v3 backstop bound to the v3 emitter, BLNT, and both Comet V2 LPs;
8. call v3 backstop `distribute()` once to activate direct accounting; and
9. optionally call v3 backstop `drop()` for the immutable initial allocation.

Deployment evidence MUST record both emitter bindings, both token
administrators, the conversion deadline, Comet V2 artifact hash and pool
configuration, v3 direct activation, and the absence of BLND in every v3 pool
and backstop binding.
