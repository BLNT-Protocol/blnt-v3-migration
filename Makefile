V2_DIR := blend-contracts-v2
V3_DIR := blnt-contracts-v3
MIGRATION_HARNESS_DIR := migration-harness
COMET_V2_DIR := comet-contracts-v2
BLNT_BACKFILL_DIR := blnt-backfill-contract

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

dual-emitter-plan:
	bash scripts/dual-emitter-deployment.sh plan

dual-emitter-validate:
	bash scripts/dual-emitter-deployment.sh validate

dual-emitter-start:
	bash scripts/dual-emitter-deployment.sh start

dual-emitter-deploy:
	bash scripts/dual-emitter-deployment.sh deploy

dual-emitter-run:
	bash scripts/dual-emitter-deployment.sh run

dual-emitter-status:
	bash scripts/dual-emitter-deployment.sh status

dual-emitter-stop:
	bash scripts/dual-emitter-deployment.sh stop

testnet-dual-emitter-validate:
	BLEND_MIGRATION_NETWORK=testnet bash scripts/dual-emitter-deployment.sh validate

testnet-dual-emitter-deploy:
	BLEND_MIGRATION_NETWORK=testnet bash scripts/dual-emitter-deployment.sh deploy

testnet-dual-emitter-run:
	BLEND_MIGRATION_NETWORK=testnet bash scripts/dual-emitter-deployment.sh run

testnet-dual-emitter-status:
	BLEND_MIGRATION_NETWORK=testnet bash scripts/dual-emitter-deployment.sh status

testnet-v3-only-plan:
	bash scripts/testnet-v3-only.sh plan

testnet-v3-only-validate:
	bash scripts/testnet-v3-only.sh validate

testnet-v3-only-run:
	bash scripts/testnet-v3-only.sh run

testnet-v3-only-deploy-modeled-pools:
	bash scripts/testnet-v3-only.sh deploy-modeled-pools

testnet-v3-only-deploy-reserve-loss-pool:
	bash scripts/testnet-v3-only.sh deploy-reserve-loss-pool

testnet-v3-only-verify-reserve-loss-pool:
	bash scripts/testnet-v3-only.sh verify-reserve-loss-pool

testnet-v3-only-status:
	bash scripts/testnet-v3-only.sh status

testnet-v3-only-write-registry:
	bash scripts/testnet-v3-only.sh write-registry

testnet-v3-only-refresh-oracle:
	bash scripts/testnet-v3-only.sh refresh-oracle

testnet-v3-only-keeper-plan:
	BLEND_KEEPER_REGISTRY=deployments/testnet-v3-only.json \
	BLEND_KEEPER_WORK_DIR=.testnet-v3-only \
	BLEND_KEEPER_CONFIG_DIR=.testnet-v3-only/stellar-config \
	BLEND_KEEPER_STATE_FILE=.testnet-v3-only/state.json \
	BLEND_KEEPER_SOURCE_ACCOUNT=blnt-v3-only-testnet-operator \
	bash scripts/emissions-keeper.sh plan

testnet-v3-only-keeper-once:
	BLEND_KEEPER_REGISTRY=deployments/testnet-v3-only.json \
	BLEND_KEEPER_WORK_DIR=.testnet-v3-only \
	BLEND_KEEPER_CONFIG_DIR=.testnet-v3-only/stellar-config \
	BLEND_KEEPER_STATE_FILE=.testnet-v3-only/state.json \
	BLEND_KEEPER_SOURCE_ACCOUNT=blnt-v3-only-testnet-operator \
	bash scripts/emissions-keeper.sh once

testnet-v3-only-keeper:
	BLEND_KEEPER_REGISTRY=deployments/testnet-v3-only.json \
	BLEND_KEEPER_WORK_DIR=.testnet-v3-only \
	BLEND_KEEPER_CONFIG_DIR=.testnet-v3-only/stellar-config \
	BLEND_KEEPER_STATE_FILE=.testnet-v3-only/state.json \
	BLEND_KEEPER_SOURCE_ACCOUNT=blnt-v3-only-testnet-operator \
	bash scripts/emissions-keeper.sh run

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

localnet-deploy-reserve-loss-pool:
	bash scripts/localnet-migration.sh deploy-reserve-loss-pool

localnet-verify-reserve-loss-pool:
	bash scripts/localnet-migration.sh verify-reserve-loss-pool

localnet-begin:
	bash scripts/localnet-migration.sh begin

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

testnet-production-deploy-reserve-loss-pool:
	bash scripts/testnet-production-migration.sh deploy-reserve-loss-pool

testnet-production-verify-reserve-loss-pool:
	bash scripts/testnet-production-migration.sh verify-reserve-loss-pool

testnet-production-begin:
	bash scripts/testnet-production-migration.sh begin

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

testnet-production-keeper-plan:
	bash scripts/emissions-keeper.sh plan

testnet-production-keeper-once:
	bash scripts/emissions-keeper.sh once

testnet-production-keeper:
	bash scripts/emissions-keeper.sh run

build-v3:
	$(MAKE) -C $(V3_DIR) build
	$(MAKE) -C $(COMET_V2_DIR) build
	$(MAKE) -C $(BLNT_BACKFILL_DIR) build
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-compat-probe --target wasm32v1-none --release --locked
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-test-sep40-oracle --target wasm32v1-none --release --locked
	mkdir -p $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_compat_probe.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_compat_probe.wasm
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_test_sep40_oracle.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_test_sep40_oracle.wasm

check-v3: verify-deployed-artifacts build-v3
	$(MAKE) -C $(V3_DIR) test
	$(MAKE) -C $(COMET_V2_DIR) test
	$(MAKE) -C $(BLNT_BACKFILL_DIR) test
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
	cargo +1.91.1 fmt --manifest-path $(COMET_V2_DIR)/Cargo.toml --all
	cargo +1.91.1 fmt --manifest-path $(BLNT_BACKFILL_DIR)/Cargo.toml --all
	cargo +1.91.1 fmt --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --all

clean:
	cargo +1.81 clean --manifest-path $(V2_DIR)/Cargo.toml
	$(MAKE) -C $(V3_DIR) clean
	$(MAKE) -C $(COMET_V2_DIR) clean
	$(MAKE) -C $(BLNT_BACKFILL_DIR) clean
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
