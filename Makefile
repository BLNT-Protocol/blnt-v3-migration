V2_DIR := blend-contracts-v2
V3_DIR := blnt-contracts-v3
MIGRATION_HARNESS_DIR := migration-harness
COMET_V1_DIR := comet-contracts-v1
COMET_V1_COMMIT := b3a2d827ed6c158341f8e67ea91eb3b0395d3871
COMET_V1_WASM := $(V3_DIR)/comet.wasm
COMET_V1_WASM_SHA256 := d735c3395f59510172cf5cf838823a1389b97cfec6bf24e580e9bd77d2b3e687
BLNT_BACKFILL_DIR := blnt-backfill-contract

default: build

verify-deployed-artifacts:
	bash scripts/verify-deployed-artifacts.sh

mainnet-v3-fetch-release-artifacts:
	bash scripts/fetch-mainnet-v3-release-artifacts.sh

mainnet-v3-redeployment-plan:
	bash scripts/mainnet-v3-redeployment.sh plan

mainnet-v3-redeployment-validate:
	bash scripts/mainnet-v3-redeployment.sh validate

mainnet-v3-redeployment-preflight:
	bash scripts/mainnet-v3-redeployment.sh preflight

mainnet-v3-redeployment-comet-plan:
	bash scripts/mainnet-v3-redeployment.sh comet-plan

mainnet-v3-redeployment-core-plan:
	bash scripts/mainnet-v3-redeployment.sh core-plan

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

dual-emitter-auctions:
	bash scripts/dual-emitter-deployment.sh auctions

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

testnet-v3-only-resume-core:
	bash scripts/testnet-v3-only.sh resume-core

testnet-v3-only-deploy-modeled-pools:
	bash scripts/testnet-v3-only.sh deploy-modeled-pools

testnet-v3-only-deploy-reserve-loss-pool:
	bash scripts/testnet-v3-only.sh deploy-reserve-loss-pool

testnet-v3-only-verify-reserve-loss-pool:
	bash scripts/testnet-v3-only.sh verify-reserve-loss-pool

testnet-v3-only-deploy-permissioned-pool:
	bash scripts/testnet-v3-only.sh deploy-permissioned-pool

testnet-v3-only-verify-permissioned-pool:
	bash scripts/testnet-v3-only.sh verify-permissioned-pool

testnet-v3-only-deploy-sep41-pool:
	bash scripts/testnet-v3-only.sh deploy-sep41-pool

testnet-v3-only-verify-sep41-pool:
	bash scripts/testnet-v3-only.sh verify-sep41-pool

testnet-v3-only-deploy-no-backstop-pool:
	bash scripts/testnet-v3-only.sh deploy-no-backstop-pool

testnet-v3-only-verify-no-backstop-pool:
	bash scripts/testnet-v3-only.sh verify-no-backstop-pool

testnet-v3-only-deploy-traditional-pool:
	bash scripts/testnet-v3-only.sh deploy-traditional-pool

testnet-v3-only-verify-traditional-pool:
	bash scripts/testnet-v3-only.sh verify-traditional-pool

testnet-v3-only-status:
	bash scripts/testnet-v3-only.sh status

testnet-v3-only-write-registry:
	bash scripts/testnet-v3-only.sh write-registry

testnet-v3-only-refresh-oracle:
	bash scripts/testnet-v3-only.sh refresh-oracle

testnet-v3-only-fund-user:
	bash scripts/testnet-v3-only.sh fund-user

mainnet-comet-plan:
	bash scripts/mainnet-comet-deployment.sh plan

mainnet-comet-validate:
	bash scripts/mainnet-comet-deployment.sh validate

mainnet-comet-preflight:
	bash scripts/mainnet-comet-deployment.sh preflight

mainnet-comet-deploy:
	bash scripts/mainnet-comet-deployment.sh deploy

mainnet-comet-status:
	bash scripts/mainnet-comet-deployment.sh status

mainnet-v3-plan:
	bash scripts/mainnet-v3-deployment.sh plan

mainnet-v3-validate:
	bash scripts/mainnet-v3-deployment.sh validate

mainnet-v3-preflight:
	bash scripts/mainnet-v3-deployment.sh preflight

mainnet-v3-deploy-core:
	bash scripts/mainnet-v3-deployment.sh deploy-core

mainnet-v3-activate:
	bash scripts/mainnet-v3-deployment.sh activate

mainnet-v3-status:
	bash scripts/mainnet-v3-deployment.sh status

mainnet-v3-fixed-plan:
	bash scripts/mainnet-v3-fixed-pool.sh plan

mainnet-v3-fixed-validate:
	bash scripts/mainnet-v3-fixed-pool.sh validate

mainnet-v3-fixed-preflight:
	bash scripts/mainnet-v3-fixed-pool.sh preflight

mainnet-v3-fixed-deploy:
	bash scripts/mainnet-v3-fixed-pool.sh deploy

mainnet-v3-fixed-resume:
	bash scripts/mainnet-v3-fixed-pool.sh resume

mainnet-v3-fixed-status:
	bash scripts/mainnet-v3-fixed-pool.sh status

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

verify-comet-v1.1:
	test "$$(git -C $(COMET_V1_DIR) rev-parse HEAD)" = "$(COMET_V1_COMMIT)"
	test "$$(shasum -a 256 $(COMET_V1_WASM) | cut -d ' ' -f 1)" = "$(COMET_V1_WASM_SHA256)"

build-v3: verify-comet-v1.1
	$(MAKE) -C $(V3_DIR) build
	$(MAKE) -C $(BLNT_BACKFILL_DIR) build
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-compat-probe --target wasm32v1-none --release --locked
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blnt-test-sep41-token --target wasm32v1-none --release --locked
	cargo +1.91.1 build --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --package blend-v3-test-sep40-oracle --target wasm32v1-none --release --locked
	mkdir -p $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized
	cd $(MIGRATION_HARNESS_DIR) && \
		stellar contract build --package blnt-test-access-controller --locked \
			--out-dir target/wasm32v1-none/optimized
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_compat_probe.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_compat_probe.wasm
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blnt_test_sep41_token.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blnt_test_sep41_token.wasm
	stellar contract optimize \
		--wasm $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/release/blend_v3_test_sep40_oracle.wasm \
		--wasm-out $(MIGRATION_HARNESS_DIR)/target/wasm32v1-none/optimized/blend_v3_test_sep40_oracle.wasm

check-v3: verify-deployed-artifacts build-v3
	$(MAKE) -C $(V3_DIR) test
	$(MAKE) -C $(COMET_V1_DIR) test
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
	cargo +1.91.1 fmt --manifest-path $(BLNT_BACKFILL_DIR)/Cargo.toml --all
	cargo +1.91.1 fmt --manifest-path $(MIGRATION_HARNESS_DIR)/Cargo.toml --all

clean:
	cargo +1.81 clean --manifest-path $(V2_DIR)/Cargo.toml
	$(MAKE) -C $(V3_DIR) clean
	cargo +1.92.0 clean --manifest-path $(COMET_V1_DIR)/Cargo.toml
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
