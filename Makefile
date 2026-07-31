V2_DIR := blend-contracts-v2
V3_DIR := blend-contracts-v3
MIGRATION_HARNESS_DIR := migration-harness

default: build

verify-deployed-artifacts:
	bash scripts/verify-deployed-artifacts.sh

test: build
	cargo +1.81 test --manifest-path $(V2_DIR)/Cargo.toml --all --tests --locked

test-v2-migration: verify-deployed-artifacts build-v3
	cargo +1.91.1 test --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml \
		--package blend-v3-migration-tests \
		--test emitter_migrates_to_v3 \
		--test v2_positions_survive_migration \
		--locked

test-v2-positions-survive-migration: verify-deployed-artifacts build-v3
	cargo +1.91.1 test --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml \
		--package blend-v3-migration-tests \
		--test v2_positions_survive_migration \
		--locked

test-all: test check-v3

localnet-plan:
	bash scripts/localnet-migration.sh plan

localnet-validate:
	bash scripts/localnet-migration.sh validate

localnet-start:
	bash scripts/localnet-migration.sh start

localnet-deploy:
	bash scripts/localnet-migration.sh deploy

localnet-run:
	bash scripts/localnet-migration.sh run

localnet-status:
	bash scripts/localnet-migration.sh status

localnet-keepalive:
	bash scripts/localnet-migration.sh keepalive

localnet-recover:
	bash scripts/localnet-migration.sh recover

localnet-prepare:
	bash scripts/localnet-migration.sh prepare

localnet-finalize:
	bash scripts/localnet-migration.sh finalize

localnet-verify:
	bash scripts/localnet-migration.sh verify

localnet-emissions:
	bash scripts/localnet-migration.sh emissions

localnet-claim-emissions:
	bash scripts/localnet-migration.sh claim-emissions

localnet-stop:
	bash scripts/localnet-migration.sh stop

testnet-plan:
	bash scripts/testnet-migration.sh plan

testnet-validate:
	bash scripts/testnet-migration.sh validate

testnet-deploy:
	bash scripts/testnet-migration.sh deploy

testnet-run:
	bash scripts/testnet-migration.sh run

testnet-status:
	bash scripts/testnet-migration.sh status

testnet-keepalive:
	bash scripts/testnet-migration.sh keepalive

testnet-prepare:
	bash scripts/testnet-migration.sh prepare

testnet-finalize:
	bash scripts/testnet-migration.sh finalize

testnet-verify:
	bash scripts/testnet-migration.sh verify

testnet-emissions:
	bash scripts/testnet-migration.sh emissions

testnet-claim-emissions:
	bash scripts/testnet-migration.sh claim-emissions

testnet-production-plan:
	bash scripts/testnet-production-migration.sh plan

testnet-production-validate:
	bash scripts/testnet-production-migration.sh validate

testnet-production-run:
	bash scripts/testnet-production-migration.sh run

testnet-production-status:
	bash scripts/testnet-production-migration.sh status

testnet-production-keepalive:
	bash scripts/testnet-production-migration.sh keepalive

testnet-production-refresh-oracle:
	bash scripts/testnet-production-migration.sh refresh-oracle

testnet-production-deploy-modeled-pools:
	bash scripts/testnet-production-migration.sh deploy-modeled-pools

testnet-production-verify-modeled-pools:
	bash scripts/testnet-production-migration.sh verify-modeled-pools

testnet-production-prepare:
	bash scripts/testnet-production-migration.sh prepare

testnet-production-finalize:
	bash scripts/testnet-production-migration.sh finalize

testnet-production-verify:
	bash scripts/testnet-production-migration.sh verify

testnet-production-emissions:
	bash scripts/testnet-production-migration.sh emissions

testnet-production-claim-emissions:
	bash scripts/testnet-production-migration.sh claim-emissions

build-v3:
	$(MAKE) -C $(V3_DIR) build
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-compat-probe --target wasm32v1-none --release --locked
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-test-sep40-oracle --target wasm32v1-none --release --locked
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-test-valuation-adapter --target wasm32v1-none --release --locked
	mkdir -p $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_compat_probe.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_compat_probe.wasm
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_test_sep40_oracle.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_test_sep40_oracle.wasm
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_test_valuation_adapter.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_test_valuation_adapter.wasm

check-v3: verify-deployed-artifacts build-v3
	$(MAKE) -C $(V3_DIR) test
	cargo +1.91.1 fmt --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --all -- --check
	cargo +1.91.1 check --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --all-targets --locked
	cargo +1.91.1 test --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --workspace --locked

build: verify-deployed-artifacts
	cargo +1.81 rustc --manifest-path=$(V2_DIR)/pool-factory/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release --locked
	cargo +1.81 rustc --manifest-path=$(V2_DIR)/backstop/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release --locked
	cargo +1.81 rustc --manifest-path=$(V2_DIR)/pool/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release --locked

	mkdir -p $(V2_DIR)/target/wasm32-unknown-unknown/optimized
	stellar contract optimize \
		--wasm $(V2_DIR)/target/wasm32-unknown-unknown/release/pool_factory.wasm \
		--wasm-out $(V2_DIR)/target/wasm32-unknown-unknown/optimized/pool_factory.wasm
	stellar contract optimize \
		--wasm $(V2_DIR)/target/wasm32-unknown-unknown/release/backstop.wasm \
		--wasm-out $(V2_DIR)/target/wasm32-unknown-unknown/optimized/backstop.wasm
	stellar contract optimize \
		--wasm $(V2_DIR)/target/wasm32-unknown-unknown/release/pool.wasm \
		--wasm-out $(V2_DIR)/target/wasm32-unknown-unknown/optimized/pool.wasm
	cd $(V2_DIR)/target/wasm32-unknown-unknown/optimized/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

fmt:
	cargo +1.81 fmt --manifest-path $(V2_DIR)/Cargo.toml --all
	$(MAKE) -C $(V3_DIR) fmt
	cargo +1.91.1 fmt --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --all

clean:
	cargo +1.81 clean --manifest-path $(V2_DIR)/Cargo.toml
	$(MAKE) -C $(V3_DIR) clean
	cargo +1.91.1 clean --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml

generate-js:
	stellar contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./deployed-artifacts/mainnet/v2-backstop.wasm --output-dir ./js/js-backstop/ \
		--rpc-url http://localhost:8000 --network-passphrase "Standalone Network ; February 2017" --network Standalone
	stellar contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./deployed-artifacts/mainnet/v2-pool-factory.wasm --output-dir ./js/js-pool-factory/ \
		--rpc-url http://localhost:8000 --network-passphrase "Standalone Network ; February 2017" --network Standalone
	stellar contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./deployed-artifacts/mainnet/v2-pool.wasm --output-dir ./js/js-pool/ \
		--rpc-url http://localhost:8000 --network-passphrase "Standalone Network ; February 2017" --network Standalone
