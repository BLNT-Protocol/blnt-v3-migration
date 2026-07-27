default: build

V3_BACKSTOP_MAX_WASM_BYTES := 120000

test: build
	cargo test --all --tests

test-v2-migration: build
	cargo test -p test-suites --test test_v2_to_v3_candidate --locked

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

build-v3:
	cargo +1.91.0 build --manifest-path v3/Cargo.toml --package blend-v3-backstop --target wasm32v1-none --release --locked
	cargo +1.91.0 build --manifest-path v3/Cargo.toml --package blend-v3-compat-probe --target wasm32v1-none --release --locked
	cargo +1.91.0 build --manifest-path v3/Cargo.toml --package blend-v3-pool --target wasm32v1-none --release --locked
	cargo +1.91.0 build --manifest-path v3/Cargo.toml --package blend-v3-pool-factory --target wasm32v1-none --release --locked
	cargo +1.91.0 build --manifest-path v3/Cargo.toml --package blend-v3-test-valuation-adapter --target wasm32v1-none --release --locked
	mkdir -p v3/target/wasm32v1-none/optimized
	stellar contract optimize \
		--wasm v3/target/wasm32v1-none/release/blend_v3_backstop.wasm \
		--wasm-out v3/target/wasm32v1-none/optimized/blend_v3_backstop.wasm
	@test "$$(wc -c < v3/target/wasm32v1-none/optimized/blend_v3_backstop.wasm)" \
		-le "$(V3_BACKSTOP_MAX_WASM_BYTES)" || \
		(printf 'error: optimized v3 backstop exceeds %s-byte deployment guard\n' \
			"$(V3_BACKSTOP_MAX_WASM_BYTES)" >&2; exit 1)
	stellar contract optimize \
		--wasm v3/target/wasm32v1-none/release/blend_v3_pool.wasm \
		--wasm-out v3/target/wasm32v1-none/optimized/blend_v3_pool.wasm
	stellar contract optimize \
		--wasm v3/target/wasm32v1-none/release/blend_v3_pool_factory.wasm \
		--wasm-out v3/target/wasm32v1-none/optimized/blend_v3_pool_factory.wasm
	stellar contract optimize \
		--wasm v3/target/wasm32v1-none/release/blend_v3_test_valuation_adapter.wasm \
		--wasm-out v3/target/wasm32v1-none/optimized/blend_v3_test_valuation_adapter.wasm

check-v3: build build-v3
	cargo +1.91.0 fmt --manifest-path v3/Cargo.toml --all -- --check
	cargo +1.91.0 check --manifest-path v3/Cargo.toml --all-targets --locked
	cargo +1.91.0 test --manifest-path v3/Cargo.toml --workspace --locked

build:
	cargo rustc --manifest-path=pool-factory/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=backstop/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	cargo rustc --manifest-path=pool/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
	
	mkdir -p target/wasm32-unknown-unknown/optimized
	stellar contract optimize \
		--wasm target/wasm32-unknown-unknown/release/pool_factory.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/pool_factory.wasm
	stellar contract optimize \
		--wasm target/wasm32-unknown-unknown/release/backstop.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/backstop.wasm
	stellar contract optimize \
		--wasm target/wasm32-unknown-unknown/release/pool.wasm \
		--wasm-out target/wasm32-unknown-unknown/optimized/pool.wasm
	cd target/wasm32-unknown-unknown/optimized/ && \
		for i in *.wasm ; do \
			ls -l "$$i"; \
		done

fmt:
	cargo fmt --all

clean:
	cargo clean
	cargo +1.91.0 clean --manifest-path v3/Cargo.toml

generate-js:
	stellar contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/backstop.wasm --output-dir ./js/js-backstop/ \
		--rpc-url http://localhost:8000 --network-passphrase "Standalone Network ; February 2017" --network Standalone
	stellar contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/pool_factory.wasm --output-dir ./js/js-pool-factory/ \
		--rpc-url http://localhost:8000 --network-passphrase "Standalone Network ; February 2017" --network Standalone
	stellar contract bindings typescript --overwrite \
		--contract-id CBWH54OKUK6U2J2A4J2REJEYB625NEFCHISWXLOPR2D2D6FTN63TJTWN \
		--wasm ./target/wasm32-unknown-unknown/optimized/pool.wasm --output-dir ./js/js-pool/ \
		--rpc-url http://localhost:8000 --network-passphrase "Standalone Network ; February 2017" --network Standalone
