# Blend V3 Candidate

Experimental Blend Protocol v3 candidate implementation for Stellar.

This repository starts from Blend Protocol v2 and is intended to develop and
test backwards-compatible protocol migration, multi-asset backstops, and
candidate v3 pool behavior. It is not an official or production-ready Blend
release.

## Baseline

The initial source baseline is Blend Protocol v2 commit
`ba22b487b2c5057a4ecc28b05b5193c28e4bd117`.

The original repository is configured as the `upstream` Git remote:

```bash
git remote -v
```

Reusable Git dependencies are kept beside this repository:

- `../blend-contracts` contains the legacy emitter source and history.
- `../xycloans` is checked out at the v2-pinned commit for source reference.
- The pinned `moderc3156` interface is vendored in `vendor/moderc3156` so fresh
  clones and CI builds do not need to fetch the Git repository.

See [docs/SYSTEM_SPEC.md](docs/SYSTEM_SPEC.md) for the candidate requirements,
[docs/BASELINE.md](docs/BASELINE.md) for the baseline policy, and
[docs/MIGRATION_TEST_PLAN.md](docs/MIGRATION_TEST_PLAN.md) for the migration
test requirements.

## Toolchain lanes

This repository deliberately maintains two independent Rust workspaces:

- The repository root is the historical v2 baseline, pinned to Rust 1.81,
  Soroban SDK 22.0.7, and `wasm32-unknown-unknown`.
- `v3/` is the candidate workspace, pinned to Rust 1.91.0, Soroban SDK 27.0.2,
  Protocol 27 tooling, and `wasm32v1-none`.

There is no SDK 26 compatibility lane. Compatibility tests span the historical
SDK-22 implementation and the current v3 candidate.

## Build and test

Build the historical v2 contracts:

```bash
make
```

Run the historical v2 test suite:

```bash
make test
```

Run the candidate migration baseline:

```bash
make test-v2-migration
```

Check the v3 workspace with its pinned current toolchain:

```bash
make check-v3
```

This builds the historical v2 WASM, the SDK-27 compatibility probe, and the
SDK-27 candidate contracts and test-only valuation adapter. The compatibility
harness verifies both a cross-contract call into the SDK-22 backstop and a
complete legacy-emitter migration into the v3 candidate on a Protocol-27
host. The deterministic adapter exercises the immutable valuation boundary;
it is not a production oracle. The candidate pool persists v2-compatible
status transitions using that canonical boundary and an immutable pool-local
operator. Its SDK-27 lending core executes the six non-auction v2 request
types against immutable reserve configuration with checked interest, share,
health, authorization, and token-transfer accounting. The pool also supports
permissionless v2-compatible user-liquidation auctions and fails closed before
any fill that would require the future bad-debt waterfall. The three-tier
backstop maintains pool-local checked asset/share exchange rates so active and
queued shares can absorb future losses proportionally.

Run both lanes:

```bash
make test-all
```

## Releases

Automated tag-triggered releases are intentionally disabled while the v3
candidate remains experimental and incomplete. Add a v3-specific release
workflow only after deployable v3 contract packages and their artifact checks
exist. The repository must not publish the root SDK-22 contracts as v3
artifacts.

## Development principles

- Preserve a passing v2 baseline before introducing behavioral changes.
- Keep the legacy emitter interface compatible.
- Test both native Rust contracts and deployed WASM.
- Treat activation, loss absorption, emissions, and emitter migration as
  separate protocol mechanisms.
- Keep `overflow-checks = true` for release builds.
- Do not treat this repository as audited until an independent audit is
  completed.

## License

This repository is derived from Blend Protocol v2 and remains licensed under
the GNU Affero General Public License v3.0. See [LICENSE](LICENSE).
