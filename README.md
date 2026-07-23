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

## Build and test

The inherited v2 workspace is pinned to Rust 1.81 and Soroban SDK 22.

Build optimized contracts:

```bash
make
```

Run the full test suite:

```bash
make test
```

Run the candidate migration baseline:

```bash
cargo test -p test-suites --test test_v2_to_v3_candidate
```

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
