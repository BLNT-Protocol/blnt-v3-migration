#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NETWORK_MODE="${BLEND_MIGRATION_NETWORK:-local}"
case "${NETWORK_MODE}" in
    local)
        NETWORK_LABEL="Local"
        WORK_DIR="${BLEND_LOCALNET_WORK_DIR:-${ROOT_DIR}/.localnet}"
        RPC_PORT="${BLEND_LOCALNET_RPC_PORT:-8000}"
        RPC_URL="${BLEND_MIGRATION_RPC_URL:-http://127.0.0.1:${RPC_PORT}/soroban/rpc}"
        HORIZON_URL="${BLEND_MIGRATION_HORIZON_URL:-http://127.0.0.1:${RPC_PORT}}"
        FRIENDBOT_URL="${BLEND_MIGRATION_FRIENDBOT_URL:-http://127.0.0.1:${RPC_PORT}/friendbot}"
        NETWORK_PASSPHRASE="Standalone Network ; February 2017"
        OPERATOR_IDENTITY="${BLEND_MIGRATION_OPERATOR_IDENTITY:-blend-v3-local-operator}"
        DEPOSITOR_IDENTITY="${BLEND_MIGRATION_DEPOSITOR_IDENTITY:-blend-v3-local-depositor}"
        DEFAULT_XLM_ASSET_MODE="issued"
        LEDGER_SECONDS=1
        ;;
    testnet)
        NETWORK_LABEL="Public testnet"
        WORK_DIR="${BLEND_TESTNET_WORK_DIR:-${ROOT_DIR}/.testnet-production}"
        RPC_PORT=""
        RPC_URL="${BLEND_MIGRATION_RPC_URL:-https://soroban-testnet.stellar.org}"
        HORIZON_URL="${BLEND_MIGRATION_HORIZON_URL:-https://horizon-testnet.stellar.org}"
        FRIENDBOT_URL="${BLEND_MIGRATION_FRIENDBOT_URL:-https://friendbot.stellar.org/}"
        NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
        OPERATOR_IDENTITY="${BLEND_MIGRATION_OPERATOR_IDENTITY:-blend-v3-production-testnet-operator}"
        DEPOSITOR_IDENTITY="${BLEND_MIGRATION_DEPOSITOR_IDENTITY:-blend-v3-production-testnet-depositor}"
        DEFAULT_XLM_ASSET_MODE="native"
        LEDGER_SECONDS=5
        ;;
    *)
        printf 'error: unsupported BLEND_MIGRATION_NETWORK: %s\n' "${NETWORK_MODE}" >&2
        exit 2
        ;;
esac
XLM_ASSET_MODE="${BLEND_MIGRATION_XLM_ASSET_MODE:-${DEFAULT_XLM_ASSET_MODE}}"
case "${XLM_ASSET_MODE}" in
    issued) ;;
    native)
        [[ "${NETWORK_MODE}" == "testnet" ]] || {
            printf 'error: native XLM asset mode is testnet-only\n' >&2
            exit 2
        }
        ;;
    *)
        printf 'error: unsupported BLEND_MIGRATION_XLM_ASSET_MODE: %s\n' \
            "${XLM_ASSET_MODE}" >&2
        exit 2
        ;;
esac
CONFIG_DIR="${WORK_DIR}/stellar-config"
STATE_FILE="${WORK_DIR}/state.json"
CONTAINER_NAME="${BLEND_LOCALNET_CONTAINER:-blend-v3-local}"
QUICKSTART_CONTAINER_NAME="stellar-${CONTAINER_NAME}"

SCALAR_7=10000000
MODELED_BACKSTOP_LP_BALANCE=$((22000 * SCALAR_7))
# Every fresh lane uses the modeled Fixed and YieldBlox pools. The Fixed v3
# position carries one extra base unit so the candidate retains the deployed
# emitter's strict aggregate BLND:USDC majority requirement.
INCUMBENT_BALANCE="${MODELED_BACKSTOP_LP_BALANCE}"
CANDIDATE_BALANCE=$((INCUMBENT_BALANCE + 1))
AGGREGATE_INCUMBENT_BALANCE=$((2 * MODELED_BACKSTOP_LP_BALANCE))
AGGREGATE_CANDIDATE_BALANCE=$((AGGREGATE_INCUMBENT_BALANCE + 1))
ACTIVATION_USDC_BALANCE=$((12500 * SCALAR_7))
POOL_SUPPLY_BALANCE=$((1000 * SCALAR_7))
BLND_USDC_LP_JOIN_AMOUNT=$((100 * SCALAR_7))
COMET_INITIAL_BLND=$((1000 * SCALAR_7))
COMET_INITIAL_PAIR_TOKEN=$((25 * SCALAR_7))
COMET_INITIAL_XLM=$((100 * SCALAR_7))
DEPOSITOR_BLND_MINT=$((15000 * SCALAR_7))
DEPOSITOR_USDC_MINT=$((ACTIVATION_USDC_BALANCE + POOL_SUPPLY_BALANCE + 500 * SCALAR_7))
DEPOSITOR_XLM_MINT=$((COMET_INITIAL_PAIR_TOKEN))
FIXTURE_ORACLE_RESOLUTION=300
FIXTURE_ORACLE_RECORDS=7
PREPARATION_WINDOW_SECONDS=$((7 * 24 * 60 * 60))
EMISSION_START_DELAY_SECONDS=5
MULTI_POOL_EMISSION_DELAY_SECONDS=10
# Explicitly renewed deployed-emitter and v2 entries get ten wall-clock days
# of TTL, leaving a two-times safety margin for the documented five-day
# keepalive cadence.
MIGRATION_TTL_EXTENSION_LEDGERS="${BLEND_MIGRATION_TTL_EXTENSION_LEDGERS:-$((10 * 24 * 60 * 60 / LEDGER_SECONDS))}"

INCUMBENT_SALT="0000000000000000000000000000000000000000000000000000000000000011"
CANDIDATE_SALT="0000000000000000000000000000000000000000000000000000000000000022"
MODELED_V2_FIXED_SALT="0000000000000000000000000000000000000000000000000000000000000044"
MODELED_V2_YIELDBLOX_SALT="0000000000000000000000000000000000000000000000000000000000000055"
MODELED_V3_FIXED_SALT="0000000000000000000000000000000000000000000000000000000000000066"
MODELED_V3_YIELDBLOX_SALT="0000000000000000000000000000000000000000000000000000000000000077"
MODELED_BLND_XLM_LP_BALANCE=$((10 * SCALAR_7))
MODELED_V3_USDC_BALANCE=$((12500 * SCALAR_7))
MODELED_V2_USDC_SUPPLY_BALANCE=$((1000 * SCALAR_7))
MODELED_FIXED_V3_USDC_COLLATERAL=$((1000 * SCALAR_7))
MODELED_FIXED_V3_EURC_SUPPLY=$((500 * SCALAR_7))
MODELED_FIXED_V3_EURC_BORROW=$((100 * SCALAR_7))
MODELED_ASSET_BALANCE=$((1000000 * SCALAR_7))
MODELED_POOLS_FIXTURE="${ROOT_DIR}/fixtures/testnet-modeled-pools.json"

V2_FACTORY_WASM="${ROOT_DIR}/deployed-artifacts/mainnet/v2-pool-factory.wasm"
V2_BACKSTOP_WASM="${ROOT_DIR}/deployed-artifacts/mainnet/v2-backstop.wasm"
V2_POOL_WASM="${ROOT_DIR}/deployed-artifacts/mainnet/v2-pool.wasm"
V3_FACTORY_WASM="${ROOT_DIR}/blend-contracts-v3/target/wasm32v1-none/optimized/pool_factory.wasm"
V3_BACKSTOP_WASM="${ROOT_DIR}/blend-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
V3_POOL_WASM="${ROOT_DIR}/blend-contracts-v3/target/wasm32v1-none/optimized/pool.wasm"
V3_TEST_SEP40_ORACLE_WASM="${ROOT_DIR}/migration-harness/target/wasm32v1-none/optimized/blend_v3_test_sep40_oracle.wasm"
COMET_WASM="${ROOT_DIR}/blend-contracts-v2/comet.wasm"

EXPECTED_EMITTER_SHA256="438a5528cff17ede6fe515f095c43c5f15727af17d006971485e52462e7e7b89"
EMITTER_WASM="${ROOT_DIR}/deployed-artifacts/mainnet/emitter.wasm"
RUN_DIR=""
DOCKER_HOST_URI="${BLEND_LOCALNET_DOCKER_HOST:-}"

usage() {
    cat <<'EOF'
Usage: scripts/localnet-migration.sh COMMAND

Commands:
  plan      Describe the deployment and migration phases without mutation.
  validate  Build candidate artifacts and verify all exact deployed WASM.
  start     Start Protocol-27 Quickstart locally; verify RPC on public testnet.
  deploy    Deploy, fund, and begin a new migration on the running network.
  run       Validate, connect or start, deploy, and report status.
  status    Query balances, queue state, candidate state, and the next time gate.
  keepalive Refresh the migration's bounded persistent and instance TTLs.
  refresh-oracle
            Refresh every modeled lending-pool oracle.
  deploy-modeled-pools
            Resume or reverify the mandatory Fixed and YieldBlox pool setup.
  verify-modeled-pools
            Verify modeled pool configs, registration, status, and backstops.
  recover   Restart stalled local Stellar Core without replacing state.
  prepare   Checkpoint backfill and verify the queue in its final seven-day window.
  finalize  Swap through the emitter, synchronize through distribute, and drop backfill.
  verify    Assert final bindings, balances, and migration state.
  emissions Exercise the immediate post-migration 70/30 emission distribution.
  claim-emissions
            Claim the completed seven-day pool emission stream.
  stop      Stop the named local Quickstart container.

The runner never weakens the production 31-day queue. Call prepare and finalize
at the timestamps reported by status. The contract retains v2's distribute/drop
migration surface. Generated configuration, public state, results, and cost
logs live below the network-specific ignored work directory.
EOF
}

note() {
    printf '%s\n' "$*" >&2
}

die() {
    note "error: $*"
    exit 1
}

require_local_mode() {
    [[ "${NETWORK_MODE}" == "local" ]] ||
        die "command is local-only; ${NETWORK_LABEL} uses the existing public RPC"
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

sha256_file() {
    local value

    value="$(shasum -a 256 "$1")"
    printf '%s\n' "${value%% *}"
}

stellar_cli() {
    stellar --config-dir "${CONFIG_DIR}" "$@"
}

resolve_emitter_wasm() {
    local actual_hash

    [[ -f "${EMITTER_WASM}" ]] ||
        die "frozen emitter WASM is missing: ${EMITTER_WASM}"
    actual_hash="$(sha256_file "${EMITTER_WASM}")"
    [[ "${actual_hash}" == "${EXPECTED_EMITTER_SHA256}" ]] ||
        die "frozen emitter WASM has SHA-256 ${actual_hash}; expected ${EXPECTED_EMITTER_SHA256}"
}

validate_tools() {
    local version_text version_line version major

    require_command cargo
    require_command curl
    require_command git
    require_command jq
    require_command make
    require_command shasum
    require_command stellar

    version_text="$(stellar --version)"
    version_line="${version_text%%$'\n'*}"
    version="${version_line#stellar }"
    major="${version%%.*}"
    [[ "${major}" == "27" ]] ||
        die "Stellar CLI major version 27 is required; found ${version_line}"
}

validate_source_provenance() {
    local untracked

    untracked="$(git -C "${ROOT_DIR}" ls-files --others --exclude-standard)"
    [[ -z "${untracked}" ]] ||
        die "untracked deployment input cannot be recorded in source.patch; stage or remove: ${untracked}"
}

build_artifacts() {
    note "Verifying deployed mainnet artifacts and building SDK-27 candidate artifacts..."
    (
        cd "${ROOT_DIR}"
        make verify-deployed-artifacts build-v3
    )
}

validate_artifacts() {
    local artifact

    resolve_emitter_wasm
    for artifact in \
        "${V2_FACTORY_WASM}" \
        "${V2_BACKSTOP_WASM}" \
        "${V2_POOL_WASM}" \
        "${V3_FACTORY_WASM}" \
        "${V3_BACKSTOP_WASM}" \
        "${V3_POOL_WASM}" \
        "${V3_TEST_SEP40_ORACLE_WASM}" \
        "${COMET_WASM}" \
        "${EMITTER_WASM}"
    do
        [[ -s "${artifact}" ]] || die "missing or empty WASM artifact: ${artifact}"
    done
    [[ -s "${MODELED_POOLS_FIXTURE}" ]] ||
        die "modeled-pool fixture is missing: ${MODELED_POOLS_FIXTURE}"
    note "Validated exact deployed incumbent and emitter plus SDK-27 candidate artifacts."
}

require_docker() {
    local active_context

    require_command docker
    docker info >/dev/null 2>&1 || die "Docker is installed but its daemon is unavailable"
    if [[ -z "${DOCKER_HOST_URI}" ]]; then
        active_context="$(docker context show)"
        DOCKER_HOST_URI="$(
            docker context inspect "${active_context}" \
                --format '{{.Endpoints.docker.Host}}'
        )"
    fi
    [[ -n "${DOCKER_HOST_URI}" ]] ||
        die "unable to resolve the active Docker context socket"
}

network_healthy() {
    curl --fail --silent --show-error --max-time 5 \
        --header "Content-Type: application/json" \
        --request POST \
        --data '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
        "${RPC_URL}" 2>/dev/null |
        jq -e '.result.status == "healthy"' >/dev/null 2>&1
}

require_network() {
    network_healthy ||
        die "${NETWORK_LABEL} RPC is unavailable or stale at ${RPC_URL}"
}

wait_for_network() {
    for _ in {1..30}; do
        if network_healthy; then
            note "Local RPC is healthy at ${RPC_URL}."
            return
        fi
        sleep 2
    done
    die "local RPC did not become healthy within 60 seconds"
}

fund_and_wait_for_account() {
    local account="$1"

    for _ in {1..120}; do
        if curl --fail --silent --show-error --max-time 5 \
            "${HORIZON_URL}/accounts/${account}" >/dev/null 2>&1
        then
            return
        fi
        curl --fail --silent --show-error --max-time 10 \
            --get --data-urlencode "addr=${account}" \
            "${FRIENDBOT_URL}" >/dev/null 2>&1 || true
        sleep 1
    done
    die "funded account did not become visible through Horizon: ${account}"
}

new_run_dir() {
    local run_stamp

    run_stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    RUN_DIR="${WORK_DIR}/runs/${run_stamp}"
    mkdir -p "${RUN_DIR}"
}

load_state() {
    local saved_mode saved_rpc saved_xlm_asset_mode

    [[ -f "${STATE_FILE}" ]] || die "deployment state not found; run deploy first"
    RUN_DIR="$(jq -er '.run_dir' "${STATE_FILE}")"
    [[ -d "${RUN_DIR}" ]] || die "recorded run directory does not exist: ${RUN_DIR}"
    saved_mode="$(jq -er '.network_mode // "local"' "${STATE_FILE}")"
    saved_xlm_asset_mode="$(jq -er '.xlm_asset_mode // "issued"' "${STATE_FILE}")"
    saved_rpc="$(jq -er '.rpc_url' "${STATE_FILE}")"
    [[ "${saved_mode}" == "${NETWORK_MODE}" ]] ||
        die "saved network mode ${saved_mode} does not match requested ${NETWORK_MODE}"
    [[ "${saved_xlm_asset_mode}" == "${XLM_ASSET_MODE}" ]] ||
        die "saved XLM asset mode ${saved_xlm_asset_mode} does not match requested ${XLM_ASSET_MODE}"
    [[ "${saved_rpc}" == "${RPC_URL}" ]] ||
        die "saved RPC ${saved_rpc} does not match requested ${RPC_URL}"
}

state_value() {
    jq -er --arg key "$1" '.[$key]' "${STATE_FILE}"
}

state_optional() {
    jq -er --arg key "$1" '.[$key] // empty' "${STATE_FILE}" 2>/dev/null || true
}

state_set() {
    local key="$1" value="$2" temporary="${STATE_FILE}.tmp"

    jq --arg key "${key}" --arg value "${value}" '.[$key] = $value' \
        "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
}

initialize_state() {
    local operator="$1"
    local source_commit source_dirty source_patch_sha256

    validate_source_provenance
    mkdir -p "${WORK_DIR}"
    source_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    git -C "${ROOT_DIR}" diff --binary HEAD -- . >"${RUN_DIR}/source.patch"
    if [[ -s "${RUN_DIR}/source.patch" ]]; then
        source_dirty=true
        source_patch_sha256="$(sha256_file "${RUN_DIR}/source.patch")"
    else
        source_dirty=false
        source_patch_sha256=""
    fi
    jq -n \
        --arg network_mode "${NETWORK_MODE}" \
        --arg network_label "${NETWORK_LABEL}" \
        --arg xlm_asset_mode "${XLM_ASSET_MODE}" \
        --arg network_passphrase "${NETWORK_PASSPHRASE}" \
        --arg rpc_url "${RPC_URL}" \
        --arg container_name "${CONTAINER_NAME}" \
        --arg operator "${operator}" \
        --arg run_dir "${RUN_DIR}" \
        --arg source_commit "${source_commit}" \
        --argjson source_dirty "${source_dirty}" \
        --arg source_patch_sha256 "${source_patch_sha256}" \
        --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        '{
            network_mode: $network_mode,
            network_label: $network_label,
            xlm_asset_mode: $xlm_asset_mode,
            network_passphrase: $network_passphrase,
            rpc_url: $rpc_url,
            container_name: $container_name,
            operator: $operator,
            run_dir: $run_dir,
            source_commit: $source_commit,
            source_dirty: $source_dirty,
            source_patch_sha256: $source_patch_sha256,
            created_at: $created_at
        }' >"${STATE_FILE}"
}

capture() {
    local label="$1"
    local stdout_file stderr_file timestamp original_label suffix command_status
    shift
    stdout_file="${RUN_DIR}/${label}.out"
    stderr_file="${RUN_DIR}/${label}.cost.log"

    if [[ -e "${stdout_file}" || -e "${stderr_file}" ]]; then
        original_label="${label}"
        timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
        label="${original_label}-${timestamp}"
        suffix=2
        while [[ -e "${RUN_DIR}/${label}.out" ||
            -e "${RUN_DIR}/${label}.cost.log" ]]
        do
            label="${original_label}-${timestamp}-${suffix}"
            suffix=$((suffix + 1))
        done
        stdout_file="${RUN_DIR}/${label}.out"
        stderr_file="${RUN_DIR}/${label}.cost.log"
    fi

    note "Executing ${label}..."
    if "$@" >"${stdout_file}" 2> >(tee "${stderr_file}" >&2); then
        command_status=0
    else
        command_status=$?
    fi
    cat "${stdout_file}"
    return "${command_status}"
}

normalize_scalar() {
    local raw="$1" decoded

    if decoded="$(printf '%s' "${raw}" | jq -er '
        if type == "string" or type == "number" or type == "boolean"
        then tostring
        else empty
        end
    ' 2>/dev/null)"
    then
        printf '%s\n' "${decoded}"
    else
        printf '%s\n' "${raw}"
    fi
}

require_contract_id() {
    [[ "$1" =~ ^C[A-Z2-7]{55}$ ]] || die "invalid contract ID returned for $2: $1"
}

deploy_wasm() {
    local label="$1" wasm="$2" salt="$3"
    local raw contract_id
    local command
    shift 3
    command=(
        stellar_cli contract deploy
        --wasm "${wasm}"
        --optimize=false
        --source-account "${OPERATOR_IDENTITY}"
        --rpc-url "${RPC_URL}"
        --network-passphrase "${NETWORK_PASSPHRASE}"
        --cost
    )
    if [[ -n "${salt}" ]]; then
        command+=(--salt "${salt}")
    fi
    if [[ "$#" -gt 0 ]]; then
        command+=(-- "$@")
    fi

    raw="$(capture "${label}" "${command[@]}")"
    contract_id="$(normalize_scalar "${raw}")"
    require_contract_id "${contract_id}" "${label}"
    printf '%s\n' "${contract_id}"
}

upload_wasm() {
    local label="$1" wasm="$2" raw hash

    raw="$(capture "${label}" \
        stellar_cli contract upload \
        --wasm "${wasm}" \
        --optimize=false \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost)"
    hash="$(normalize_scalar "${raw}")"
    [[ "${hash}" =~ ^[0-9a-fA-F]{64}$ ]] || die "invalid WASM hash for ${label}: ${hash}"
    printf '%s\n' "${hash}"
}

deploy_asset() {
    local label="$1" code="$2" issuer="$3" raw contract_id

    raw="$(capture "${label}" \
        stellar_cli contract asset deploy \
        --asset "${code}:${issuer}" \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost)"
    contract_id="$(normalize_scalar "${raw}")"
    require_contract_id "${contract_id}" "${label}"
    printf '%s\n' "${contract_id}"
}

resolve_native_asset() {
    local label="$1" raw contract_id

    raw="$(capture "${label}" \
        stellar_cli contract id asset \
        --asset native \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
    contract_id="$(normalize_scalar "${raw}")"
    require_contract_id "${contract_id}" "${label}"
    printf '%s\n' "${contract_id}"
}

invoke_transaction() {
    local label="$1" contract_id="$2" function_name="$3"
    shift 3
    capture "${label}" \
        stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost \
        -- "${function_name}" "$@"
}

invoke_transaction_as() {
    local label="$1" source_identity="$2" contract_id="$3" function_name="$4"
    shift 4
    capture "${label}" \
        stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${source_identity}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost \
        -- "${function_name}" "$@"
}

invoke_view() {
    local label="$1" contract_id="$2" function_name="$3"
    shift 3
    capture "${label}" \
        stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send no \
        --cost \
        -- "${function_name}" "$@"
}

invoke_view_as() {
    local label="$1" source_identity="$2" contract_id="$3" function_name="$4"
    shift 4
    capture "${label}" \
        stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${source_identity}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send no \
        --cost \
        -- "${function_name}" "$@"
}

invoke_forced_transaction() {
    local label="$1" contract_id="$2" function_name="$3"
    shift 3
    capture "${label}" \
        stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send yes \
        --cost \
        -- "${function_name}" "$@"
}

extend_contract_instance() {
    local label="$1" contract_id="$2"

    capture "${label}" \
        stellar_cli contract extend \
        --id "${contract_id}" \
        --ledgers-to-extend "${MIGRATION_TTL_EXTENSION_LEDGERS}" \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost
}

extend_contract_key_xdr() {
    local label="$1" contract_id="$2" key_xdr="$3"

    capture "${label}" \
        stellar_cli contract extend \
        --id "${contract_id}" \
        --key-xdr "${key_xdr}" \
        --durability persistent \
        --ledgers-to-extend "${MIGRATION_TTL_EXTENSION_LEDGERS}" \
        --source-account "${OPERATOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost
}

last_distribution_key_xdr() {
    local backstop="$1"

    jq -nc --arg backstop "${backstop}" \
        '{vec: [{symbol: "LastDistro"}, {address: $backstop}]}' |
        stellar xdr encode --type ScVal
}

predict_contract_id() {
    local salt="$1" raw

    raw="$(
        stellar_cli contract id wasm \
            --salt "${salt}" \
            --source-account "${OPERATOR_IDENTITY}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}"
    )"
    normalize_scalar "${raw}"
}

format_timestamp() {
    local timestamp="$1" formatted

    if formatted="$(date -u -r "${timestamp}" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null)"; then
        printf '%s\n' "${formatted}"
    else
        date -u -d "@${timestamp}" +%Y-%m-%dT%H:%M:%SZ
    fi
}

assert_equal() {
    [[ "$2" == "$1" ]] || die "$3: expected $1, found $2"
}

assert_queued_swap_matches() {
    local queued_swap="$1" candidate_id="$2" blnd_xlm_token_id="$3" unlock_time="$4"
    local queued_candidate queued_blnd_xlm_token queued_unlock

    [[ "${queued_swap}" != "null" ]] ||
        die "deployed emitter queue is absent"
    queued_candidate="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop')"
    queued_blnd_xlm_token="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop_token')"
    queued_unlock="$(printf '%s' "${queued_swap}" | jq -er '.unlock_time | tostring')"
    [[ "${queued_candidate}" == "${candidate_id}" ]] ||
        die "live emitter queue targets ${queued_candidate}, not saved candidate ${candidate_id}"
    [[ "${queued_blnd_xlm_token}" == "${blnd_xlm_token_id}" ]] ||
        die "live emitter queue uses ${queued_blnd_xlm_token}, not saved next token ${blnd_xlm_token_id}"
    [[ "${queued_unlock}" == "${unlock_time}" ]] ||
        die "live emitter unlock ${queued_unlock} differs from saved unlock ${unlock_time}"
}

fixture_price_series() {
    local price="$1" now newest

    now="$(date +%s)"
    newest=$(((now / FIXTURE_ORACLE_RESOLUTION - 1) * FIXTURE_ORACLE_RESOLUTION))
    jq -nc \
        --arg price "${price}" \
        --argjson newest "${newest}" \
        --argjson resolution "${FIXTURE_ORACLE_RESOLUTION}" \
        --argjson records "${FIXTURE_ORACLE_RECORDS}" \
        '[
            range(0; $records) |
            {
                price: $price,
                timestamp: ($newest - (($records - 1 - .) * $resolution))
            }
        ]'
}

require_modeled_pool_support() {
    [[ -s "${MODELED_POOLS_FIXTURE}" ]] ||
        die "modeled-pool fixture is missing: ${MODELED_POOLS_FIXTURE}"
    [[ -s "${V3_TEST_SEP40_ORACLE_WASM}" ]] ||
        die "test lending-oracle WASM is missing: ${V3_TEST_SEP40_ORACLE_WASM}"
}

modeled_asset_state_key() {
    case "$1" in
        XLM) printf '%s\n' "xlm_token" ;;
        USDC) printf '%s\n' "usdc_token" ;;
        EURC) printf '%s\n' "modeled_asset_eurc" ;;
        AQUA) printf '%s\n' "modeled_asset_aqua" ;;
        USDGLO) printf '%s\n' "modeled_asset_usdglo" ;;
        USTRY) printf '%s\n' "modeled_asset_ustry" ;;
        CETES) printf '%s\n' "modeled_asset_cetes" ;;
        PYUSD) printf '%s\n' "modeled_asset_pyusd" ;;
        *) die "unknown modeled asset: $1" ;;
    esac
}

modeled_asset_id() {
    state_value "$(modeled_asset_state_key "$1")"
}

modeled_pool_salt() {
    case "$1:$2" in
        v2:fixed) printf '%s\n' "${MODELED_V2_FIXED_SALT}" ;;
        v2:yieldblox) printf '%s\n' "${MODELED_V2_YIELDBLOX_SALT}" ;;
        v3:fixed) printf '%s\n' "${MODELED_V3_FIXED_SALT}" ;;
        v3:yieldblox) printf '%s\n' "${MODELED_V3_YIELDBLOX_SALT}" ;;
        *) die "unknown modeled pool salt: $1 $2" ;;
    esac
}

modeled_pool_state_key() {
    printf 'modeled_%s_%s_pool\n' "$1" "$2"
}

modeled_oracle_state_key() {
    printf 'modeled_%s_%s_oracle\n' "$1" "$2"
}

modeled_pool_asset_ids() {
    local model="$1" result="[]" asset asset_id

    while IFS= read -r asset; do
        asset_id="$(modeled_asset_id "${asset}")"
        result="$(printf '%s' "${result}" | jq -c \
            --arg asset "${asset_id}" '. + [{Stellar: $asset}]')"
    done < <(jq -r --arg model "${model}" \
        '.pools[$model].reserves[].asset' "${MODELED_POOLS_FIXTURE}")
    printf '%s\n' "${result}"
}

modeled_v2_reserve_config() {
    local model="$1" index="$2"

    jq -c --arg model "${model}" --argjson index "${index}" \
        '.pools[$model].reserves[$index].config + {
            decimals: 7,
            enabled: true,
            index: $index
        }' "${MODELED_POOLS_FIXTURE}"
}

ensure_modeled_asset() {
    local asset="$1" operator depositor state_key asset_id balance deficit label

    label="$(printf '%s' "${asset}" | tr '[:upper:]' '[:lower:]')"
    state_key="$(modeled_asset_state_key "${asset}")"
    asset_id="$(state_optional "${state_key}")"
    if [[ -z "${asset_id}" ]]; then
        operator="$(state_value operator)"
        asset_id="$(deploy_asset \
            "deploy-modeled-asset-${label}" "${asset}" "${operator}")"
        state_set "${state_key}" "${asset_id}"
    fi
    require_contract_id "${asset_id}" "modeled ${asset}"

    depositor="$(state_value depositor)"
    if [[ -z "$(state_optional "${state_key}_trustline")" ]]; then
        capture "create-modeled-${label}-trustline" \
            stellar_cli tx new change-trust \
            --source-account "${DEPOSITOR_IDENTITY}" \
            --line "${asset}:$(state_value operator)" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        state_set "${state_key}_trustline" "created"
    fi
    balance="$(normalize_scalar "$(invoke_view \
        "query-modeled-${label}-balance" "${asset_id}" balance \
        --id "${depositor}")")"
    [[ "${balance}" =~ ^[0-9]+$ ]] ||
        die "invalid ${asset} depositor balance: ${balance}"
    if (( balance < MODELED_ASSET_BALANCE )); then
        deficit=$((MODELED_ASSET_BALANCE - balance))
        invoke_transaction "mint-modeled-${label}" "${asset_id}" mint \
            --to "${depositor}" --amount "${deficit}" >/dev/null
    fi
}

seed_modeled_sep40_oracle() {
    local version="$1" model="$2" oracle="$3" asset asset_id price prices label

    while IFS= read -r asset; do
        label="$(printf '%s' "${asset}" | tr '[:upper:]' '[:lower:]')"
        asset_id="$(modeled_asset_id "${asset}")"
        price="$(jq -er --arg asset "${asset}" \
            '.prices[$asset]' "${MODELED_POOLS_FIXTURE}")"
        prices="$(fixture_price_series "${price}")"
        invoke_transaction "seed-${model}-${version}-oracle-${label}" \
            "${oracle}" set_prices \
            --asset "{\"Stellar\":\"${asset_id}\"}" \
            --prices "${prices}" >/dev/null
    done < <(jq -r --arg model "${model}" \
        '.pools[$model].reserves[].asset' "${MODELED_POOLS_FIXTURE}")
}

refresh_modeled_pool_oracles() {
    local label_prefix="$1" version model oracle

    for version in v2 v3; do
        for model in fixed yieldblox; do
            oracle="$(state_value "$(modeled_oracle_state_key "${version}" "${model}")")"
            seed_modeled_sep40_oracle "${version}" "${model}" "${oracle}"
        done
    done
    state_set "modeled_oracles_refreshed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Refreshed the modeled lending-pool oracles (${label_prefix})."
}

ensure_modeled_v2_oracle() {
    local model="$1" state_key oracle assets

    state_key="$(modeled_oracle_state_key v2 "${model}")"
    oracle="$(state_optional "${state_key}")"
    if [[ -z "${oracle}" ]]; then
        assets="$(modeled_pool_asset_ids "${model}")"
        oracle="$(deploy_wasm \
            "deploy-${model}-v2-oracle" "${V3_TEST_SEP40_ORACLE_WASM}" "" \
            --admin "$(state_value operator)" \
            --base '{"Other":"USD"}' \
            --assets "${assets}" \
            --decimals 7 \
            --resolution "${FIXTURE_ORACLE_RESOLUTION}")"
        state_set "${state_key}" "${oracle}"
    fi
    seed_modeled_sep40_oracle v2 "${model}" "${oracle}"
    printf '%s\n' "${oracle}"
}

ensure_modeled_v3_oracle() {
    local model="$1" state_key oracle assets

    state_key="$(modeled_oracle_state_key v3 "${model}")"
    oracle="$(state_optional "${state_key}")"
    if [[ -z "${oracle}" ]]; then
        assets="$(modeled_pool_asset_ids "${model}")"
        oracle="$(deploy_wasm \
            "deploy-${model}-v3-lending-oracle" \
            "${V3_TEST_SEP40_ORACLE_WASM}" "" \
            --admin "$(state_value operator)" \
            --base '{"Other":"USD"}' \
            --assets "${assets}" \
            --decimals 7 \
            --resolution "${FIXTURE_ORACLE_RESOLUTION}")"
        state_set "${state_key}" "${oracle}"
    fi
    seed_modeled_sep40_oracle v3 "${model}" "${oracle}"
    printf '%s\n' "${oracle}"
}

ensure_modeled_v2_pool() {
    local model="$1" oracle="$2" state_key pool factory salt raw name
    local take_rate max_positions min_collateral reserve_list asset asset_id metadata
    local index=0 label status

    state_key="$(modeled_pool_state_key v2 "${model}")"
    pool="$(state_optional "${state_key}")"
    if [[ -z "${pool}" ]]; then
        factory="$(state_value v2_pool_factory)"
        salt="$(modeled_pool_salt v2 "${model}")"
        name="$(jq -er --arg model "${model}" \
            '.pools[$model].name' "${MODELED_POOLS_FIXTURE}")"
        take_rate="$(jq -er --arg model "${model}" \
            '.pools[$model].backstop_take_rate' "${MODELED_POOLS_FIXTURE}")"
        max_positions="$(jq -er --arg model "${model}" \
            '.pools[$model].max_positions' "${MODELED_POOLS_FIXTURE}")"
        min_collateral="$(jq -er --arg model "${model}" \
            '.pools[$model].min_collateral' "${MODELED_POOLS_FIXTURE}")"
        raw="$(invoke_transaction "deploy-${model}-v2-pool" "${factory}" deploy \
            --admin "$(state_value operator)" \
            --name "${name}" \
            --salt "${salt}" \
            --oracle "${oracle}" \
            --backstop_take_rate "${take_rate}" \
            --max_positions "${max_positions}" \
            --min_collateral "${min_collateral}")"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "${model} v2 pool"
        state_set "${state_key}" "${pool}"
    fi

    reserve_list="$(invoke_view \
        "query-${model}-v2-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r asset; do
        label="$(printf '%s' "${asset}" | tr '[:upper:]' '[:lower:]')"
        asset_id="$(modeled_asset_id "${asset}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg asset "${asset_id}" 'index($asset) != null' >/dev/null
        then
            metadata="$(modeled_v2_reserve_config "${model}" "${index}")"
            if invoke_view "probe-${model}-v2-queued-reserve-${label}" \
                "${pool}" set_reserve --asset "${asset_id}" >/dev/null
            then
                invoke_transaction "resume-${model}-v2-reserve-${label}" \
                    "${pool}" set_reserve --asset "${asset_id}" >/dev/null
            else
                invoke_transaction "queue-${model}-v2-reserve-${label}" \
                    "${pool}" queue_set_reserve \
                    --asset "${asset_id}" \
                    --metadata "${metadata}" >/dev/null
                invoke_transaction "set-${model}-v2-reserve-${label}" \
                    "${pool}" set_reserve --asset "${asset_id}" >/dev/null
            fi
            reserve_list="$(invoke_view \
                "query-${model}-v2-reserves-${label}" \
                "${pool}" get_reserve_list)"
        fi
        index=$((index + 1))
    done < <(jq -r --arg model "${model}" \
        '.pools[$model].reserves[].asset' "${MODELED_POOLS_FIXTURE}")

    status="$(invoke_view "query-${model}-v2-config" "${pool}" get_config |
        jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        # Exit setup before backstop funding. The deployment command activates
        # the pool only after its v2 activation threshold balance has been deposited.
        invoke_transaction "on-ice-${model}-v2-pool" \
            "${pool}" set_status --pool_status 2 >/dev/null
    fi
    printf '%s\n' "${pool}"
}

ensure_modeled_v3_pool() {
    local model="$1" oracle="$2" state_key pool factory salt raw name
    local take_rate max_positions min_collateral reserve_list asset asset_id metadata
    local index=0 label status

    state_key="$(modeled_pool_state_key v3 "${model}")"
    pool="$(state_optional "${state_key}")"
    if [[ -z "${pool}" ]]; then
        factory="$(state_value v3_pool_factory)"
        salt="$(modeled_pool_salt v3 "${model}")"
        name="$(jq -er --arg model "${model}" \
            '.pools[$model].name' "${MODELED_POOLS_FIXTURE}")"
        take_rate="$(jq -er --arg model "${model}" \
            '.pools[$model].backstop_take_rate' "${MODELED_POOLS_FIXTURE}")"
        max_positions="$(jq -er --arg model "${model}" \
            '.pools[$model].max_positions' "${MODELED_POOLS_FIXTURE}")"
        min_collateral="$(jq -er --arg model "${model}" \
            '.pools[$model].min_collateral' "${MODELED_POOLS_FIXTURE}")"
        raw="$(invoke_transaction "deploy-${model}-v3-pool" "${factory}" deploy \
            --admin "$(state_value operator)" \
            --name "${name}" \
            --salt "${salt}" \
            --oracle "${oracle}" \
            --backstop_take_rate "${take_rate}" \
            --max_positions "${max_positions}" \
            --min_collateral "${min_collateral}")"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "${model} v3 pool"
        state_set "${state_key}" "${pool}"
    fi

    reserve_list="$(invoke_view \
        "query-${model}-v3-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r asset; do
        label="$(printf '%s' "${asset}" | tr '[:upper:]' '[:lower:]')"
        asset_id="$(modeled_asset_id "${asset}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg asset "${asset_id}" 'index($asset) != null' >/dev/null
        then
            metadata="$(modeled_v2_reserve_config "${model}" "${index}")"
            if invoke_view "probe-${model}-v3-queued-reserve-${label}" \
                "${pool}" set_reserve --asset "${asset_id}" >/dev/null
            then
                invoke_transaction "resume-${model}-v3-reserve-${label}" \
                    "${pool}" set_reserve --asset "${asset_id}" >/dev/null
            else
                invoke_transaction "queue-${model}-v3-reserve-${label}" \
                    "${pool}" queue_set_reserve \
                    --asset "${asset_id}" \
                    --metadata "${metadata}" >/dev/null
                invoke_transaction "set-${model}-v3-reserve-${label}" \
                    "${pool}" set_reserve --asset "${asset_id}" >/dev/null
            fi
            reserve_list="$(invoke_view \
                "query-${model}-v3-reserves-${label}" \
                "${pool}" get_reserve_list)"
        fi
        index=$((index + 1))
    done < <(jq -r --arg model "${model}" \
        '.pools[$model].reserves[].asset' "${MODELED_POOLS_FIXTURE}")

    status="$(invoke_view "query-${model}-v3-config" "${pool}" get_config |
        jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        invoke_transaction "on-ice-${model}-v3-pool" \
            "${pool}" set_status --pool_status 2 >/dev/null
    fi
    printf '%s\n' "${pool}"
}

modeled_v2_user_shares() {
    local pool="$1"

    invoke_view "query-modeled-v2-user-shares" \
        "$(state_value incumbent_backstop)" user_balance \
        --pool "${pool}" \
        --user "$(state_value depositor)" |
        jq -er '.shares | tostring'
}

modeled_v3_user_shares() {
    local pool="$1" tier="$2"

    invoke_view "query-modeled-v3-${tier}-balance" \
        "$(state_value candidate_backstop)" user_balance \
        --tier "${tier}" \
        --pool "${pool}" \
        --user "$(state_value depositor)" |
        jq -er '((.shares | tonumber) + ([.q4w[]?.amount | tonumber] | add // 0)) | tostring'
}

modeled_v3_blnd_usdc_target() {
    local model="$1" explicit_target

    explicit_target="$(state_optional "modeled_v3_${model}_blnd_usdc_target")"
    if [[ -n "${explicit_target}" ]]; then
        [[ "${explicit_target}" =~ ^[0-9]+$ ]] ||
            die "invalid ${model} v3 BLND:USDC target: ${explicit_target}"
        printf '%s\n' "${explicit_target}"
        return
    fi
    printf '%s\n' "${MODELED_BACKSTOP_LP_BALANCE}"
}

ensure_modeled_v2_backstop_deposit() {
    local model="$1" pool="$2" current delta

    current="$(modeled_v2_user_shares "${pool}")"
    [[ "${current}" =~ ^[0-9]+$ ]] ||
        die "invalid ${model} v2 backstop shares: ${current}"
    (( current <= MODELED_BACKSTOP_LP_BALANCE )) ||
        die "${model} v2 backstop exceeds fixture target: ${current}"
    if (( current < MODELED_BACKSTOP_LP_BALANCE )); then
        delta=$((MODELED_BACKSTOP_LP_BALANCE - current))
        invoke_transaction_as "deposit-${model}-v2-blnd-usdc" \
            "${DEPOSITOR_IDENTITY}" "$(state_value incumbent_backstop)" deposit \
            --from "$(state_value depositor)" \
            --pool_address "${pool}" \
            --amount "${delta}" >/dev/null
    fi
}

ensure_modeled_v3_backstop_deposit() {
    local model="$1" pool="$2" tier="$3" target="$4"
    local current delta label

    current="$(modeled_v3_user_shares "${pool}" "${tier}")"
    [[ "${current}" =~ ^[0-9]+$ ]] ||
        die "invalid ${model} v3 ${tier} shares: ${current}"
    (( current <= target )) ||
        die "${model} v3 ${tier} shares exceed fixture target: ${current}"
    if (( current < target )); then
        delta=$((target - current))
        label="$(printf '%s' "${tier}" | tr '[:upper:]' '[:lower:]')"
        invoke_transaction_as "deposit-${model}-v3-${label}" \
            "${DEPOSITOR_IDENTITY}" "$(state_value candidate_backstop)" \
            deposit \
            --tier "${tier}" \
            --from "$(state_value depositor)" \
            --pool-address "${pool}" \
            --amount "${delta}" >/dev/null
    fi
}

ensure_token_balance() {
    local token="$1" required="$2" label="$3" balance deficit

    balance="$(normalize_scalar "$(invoke_view \
        "query-${label}-available" "${token}" balance \
        --id "$(state_value depositor)")")"
    [[ "${balance}" =~ ^[0-9]+$ ]] ||
        die "invalid ${label} depositor balance: ${balance}"
    if (( balance < required )); then
        deficit=$((required - balance))
        invoke_transaction "mint-${label}-capacity" "${token}" mint \
            --to "$(state_value depositor)" \
            --amount "${deficit}" >/dev/null
    fi
}

ensure_classic_blnd_balance() {
    local required="$1" balance deficit

    balance="$(normalize_scalar "$(invoke_view \
        "query-modeled-blnd-available" "$(state_value blnd_token)" balance \
        --id "$(state_value depositor)")")"
    [[ "${balance}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled BLND depositor balance: ${balance}"
    if (( balance < required )); then
        deficit=$((required - balance))
        capture "issue-modeled-blnd-capacity" \
            stellar_cli tx new payment \
            --source-account "${OPERATOR_IDENTITY}" \
            --destination "$(state_value depositor)" \
            --asset "BLND:$(state_value operator)" \
            --amount "${deficit}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    fi
}

ensure_modeled_blnd_usdc_lp_capacity() {
    local required="$1" token available deficit max_blnd max_usdc

    (( required > 0 )) || return 0
    token="$(state_value blnd_usdc_token)"
    available="$(normalize_scalar "$(invoke_view \
        "query-modeled-blnd-usdc-lp-available" "${token}" balance \
        --id "$(state_value depositor)")")"
    [[ "${available}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled BLND:USDC LP depositor balance: ${available}"
    (( available >= required )) && return 0

    deficit=$((required - available))
    # The controlled Comet is initialized at 10 BLND and 0.25 USDC per LP.
    # These transaction bounds leave margin for integer rounding while keeping
    # the synthetic test-only issuance explicitly bounded.
    max_blnd=$((deficit * 11))
    max_usdc="${deficit}"
    ensure_classic_blnd_balance "${max_blnd}"
    ensure_token_balance "$(state_value usdc_token)" \
        "${max_usdc}" "modeled-usdc-lp"
    invoke_transaction_as "join-modeled-blnd-usdc-capacity" \
        "${DEPOSITOR_IDENTITY}" "${token}" join_pool \
        --pool_amount_out "${deficit}" \
        --user "$(state_value depositor)" \
        --max_amounts_in "[\"${max_blnd}\",\"${max_usdc}\"]" >/dev/null
}

modeled_v2_user_supply() {
    local pool="$1"

    invoke_view "query-modeled-v2-user-supply" \
        "${pool}" get_positions --address "$(state_value depositor)" |
        jq -er '(.supply["1"] // "0") | tostring'
}

modeled_v3_user_supply() {
    local pool="$1" reserve_index="$2"

    invoke_view "query-modeled-v3-user-supply" \
        "${pool}" get_positions --address "$(state_value depositor)" |
        jq -er --arg index "${reserve_index}" \
            '(.supply[$index] // "0") | tostring'
}

ensure_modeled_v2_supply() {
    local model="$1" pool="$2" current delta

    current="$(modeled_v2_user_supply "${pool}")"
    [[ "${current}" =~ ^[0-9]+$ ]] ||
        die "invalid ${model} v2 USDC supply: ${current}"
    (( current <= MODELED_V2_USDC_SUPPLY_BALANCE )) ||
        die "${model} v2 USDC supply exceeds fixture target: ${current}"
    if (( current < MODELED_V2_USDC_SUPPLY_BALANCE )); then
        delta=$((MODELED_V2_USDC_SUPPLY_BALANCE - current))
        invoke_transaction_as "seed-${model}-v2-usdc-supply" \
            "${DEPOSITOR_IDENTITY}" "${pool}" submit \
            --from "$(state_value depositor)" \
            --spender "$(state_value depositor)" \
            --to "$(state_value depositor)" \
            --requests \
            "[{\"address\":\"$(state_value usdc_token)\",\"amount\":\"${delta}\",\"request_type\":0}]" \
            >/dev/null
    fi
}

ensure_modeled_v3_emissions_config() {
    local model="$1" pool="$2" desired

    case "${model}" in
        fixed)
            desired='[{"res_index":2,"res_type":0,"share":1}]'
            ;;
        yieldblox)
            desired='[{"res_index":1,"res_type":1,"share":1}]'
            ;;
        *) die "unsupported modeled v3 emission profile: ${model}" ;;
    esac

    invoke_transaction "configure-${model}-v3-emissions" \
        "${pool}" set_emissions_config \
        --res_emission_metadata "${desired}" >/dev/null
}

ensure_modeled_v3_supply() {
    local model="$1" pool="$2" current

    current="$(modeled_v3_user_supply "${pool}" 1)"
    [[ "${current}" =~ ^[0-9]+$ ]] ||
        die "invalid ${model} v3 USDC supply shares: ${current}"
    if (( current == 0 )); then
        ensure_token_balance "$(state_value usdc_token)" \
            "${POOL_SUPPLY_BALANCE}" "${model}-v3-usdc-supply"
        invoke_transaction_as "seed-${model}-v3-usdc-supply" \
            "${DEPOSITOR_IDENTITY}" "${pool}" submit \
            --from "$(state_value depositor)" \
            --spender "$(state_value depositor)" \
            --to "$(state_value depositor)" \
            --requests \
            "[{\"address\":\"$(state_value usdc_token)\",\"amount\":\"${POOL_SUPPLY_BALANCE}\",\"request_type\":0}]" \
            >/dev/null
    elif (( current != POOL_SUPPLY_BALANCE )); then
        die "${model} v3 USDC supply is neither empty nor the fixture target: ${current}"
    fi
}

ensure_fixed_v3_eurc_borrow() {
    local pool="$1" positions usdc_collateral eurc_supply eurc_borrow
    local usdc eurc

    positions="$(invoke_view \
        "query-fixed-v3-emission-positions" "${pool}" get_positions \
        --address "$(state_value depositor)")"
    usdc_collateral="$(printf '%s' "${positions}" |
        jq -er '(.collateral["1"] // "0") | tostring')"
    eurc_supply="$(printf '%s' "${positions}" |
        jq -er '(.supply["2"] // "0") | tostring')"
    eurc_borrow="$(printf '%s' "${positions}" |
        jq -er '(.liabilities["2"] // "0") | tostring')"
    for value in "${usdc_collateral}" "${eurc_supply}" "${eurc_borrow}"; do
        [[ "${value}" =~ ^[0-9]+$ ]] ||
            die "invalid Fixed v3 EURC-borrow fixture position"
    done

    if (( usdc_collateral == 0 && eurc_supply == 0 && eurc_borrow == 0 )); then
        usdc="$(state_value usdc_token)"
        eurc="$(modeled_asset_id EURC)"
        ensure_token_balance "${usdc}" \
            "${MODELED_FIXED_V3_USDC_COLLATERAL}" \
            "fixed-v3-usdc-collateral"
        ensure_token_balance "${eurc}" \
            "${MODELED_FIXED_V3_EURC_SUPPLY}" \
            "fixed-v3-eurc-liquidity"
        invoke_transaction_as "seed-fixed-v3-eurc-borrow" \
            "${DEPOSITOR_IDENTITY}" "${pool}" submit \
            --from "$(state_value depositor)" \
            --spender "$(state_value depositor)" \
            --to "$(state_value depositor)" \
            --requests \
            "[{\"address\":\"${eurc}\",\"amount\":\"${MODELED_FIXED_V3_EURC_SUPPLY}\",\"request_type\":0},{\"address\":\"${usdc}\",\"amount\":\"${MODELED_FIXED_V3_USDC_COLLATERAL}\",\"request_type\":2},{\"address\":\"${eurc}\",\"amount\":\"${MODELED_FIXED_V3_EURC_BORROW}\",\"request_type\":4}]" \
            >/dev/null
    elif (( usdc_collateral != MODELED_FIXED_V3_USDC_COLLATERAL ||
        eurc_supply != MODELED_FIXED_V3_EURC_SUPPLY ||
        eurc_borrow != MODELED_FIXED_V3_EURC_BORROW ))
    then
        die "Fixed v3 EURC-borrow fixture is partial or differs from its target"
    fi
}

advance_modeled_v2_emissions() {
    local label="$1"

    invoke_transaction "${label}-emitter-distribute" \
        "$(state_value emitter)" distribute >/dev/null
    invoke_transaction "${label}-backstop-distribute" \
        "$(state_value incumbent_backstop)" distribute >/dev/null
}

modeled_v2_emission_stream_is_active() {
    local stream="$1" now="$2"

    printf '%s' "${stream}" | jq -e --argjson now "${now}" '
        . != null
        and ((.expiration | tonumber) > $now)
        and ((.eps | tonumber) > 0)
    ' >/dev/null 2>&1
}

record_modeled_pools_provenance() {
    local stage="$1" patch commit dirty=false digest="" state_prefix

    case "${stage}" in
        deployment|verification) ;;
        *) die "invalid modeled-pool provenance stage: ${stage}" ;;
    esac
    patch="${RUN_DIR}/modeled-pools-${stage}-source.patch"
    state_prefix="modeled_pools_${stage}_source"

    commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    git -C "${ROOT_DIR}" diff --binary HEAD -- . >"${patch}"
    if [[ -s "${patch}" ]]; then
        dirty=true
        digest="$(sha256_file "${patch}")"
    fi
    state_set "${state_prefix}_commit" "${commit}"
    state_set "${state_prefix}_dirty" "${dirty}"
    state_set "${state_prefix}_patch_sha256" "${digest}"
}

keepalive_modeled_pools() {
    local stamp="$1" version model pool oracle asset asset_id label tier
    local factory candidate incumbent depositor state_key

    factory="$(state_value v2_pool_factory)"
    candidate="$(state_value candidate_backstop)"
    incumbent="$(state_value incumbent_backstop)"
    depositor="$(state_value depositor)"

    for version in v2 v3; do
        for model in fixed yieldblox; do
            state_key="$(modeled_pool_state_key "${version}" "${model}")"
            pool="$(state_value "${state_key}")"
            oracle="$(state_value "$(modeled_oracle_state_key "${version}" "${model}")")"
            extend_contract_instance \
                "${stamp}-${version}-${model}-pool-instance" "${pool}" >/dev/null
            extend_contract_instance \
                "${stamp}-${version}-${model}-oracle-instance" "${oracle}" >/dev/null
            if [[ "${version}" == "v2" ]]; then
                invoke_forced_transaction \
                    "${stamp}-${version}-${model}-registration" \
                    "${factory}" is_pool --pool_address "${pool}" >/dev/null
                invoke_forced_transaction \
                    "${stamp}-${version}-${model}-position" \
                    "${incumbent}" user_balance \
                    --pool "${pool}" --user "${depositor}" >/dev/null
            else
                invoke_forced_transaction \
                    "${stamp}-${version}-${model}-registration" \
                    "$(state_value v3_pool_factory)" is_pool \
                    --pool_address "${pool}" >/dev/null
                for tier in BlndUsdc BlndXlm Usdc; do
                    invoke_forced_transaction \
                        "${stamp}-${version}-${model}-${tier}-position" \
                        "${candidate}" user_balance \
                        --tier "${tier}" \
                        --pool "${pool}" \
                        --user "${depositor}" >/dev/null
                done
            fi
            while IFS= read -r asset; do
                asset_id="$(modeled_asset_id "${asset}")"
                label="$(printf '%s' "${asset}" |
                    tr '[:upper:]' '[:lower:]')"
                if [[ "${version}" == "v2" ]]; then
                    invoke_forced_transaction \
                        "${stamp}-${version}-${model}-${label}-reserve" \
                        "${pool}" get_reserve --asset "${asset_id}" >/dev/null
                else
                    invoke_forced_transaction \
                        "${stamp}-${version}-${model}-${label}-reserve" \
                        "${pool}" get_reserve --asset "${asset_id}" >/dev/null
                fi
            done < <(jq -r --arg model "${model}" \
                '.pools[$model].reserves[].asset' "${MODELED_POOLS_FIXTURE}")
            seed_modeled_sep40_oracle "${version}" "${model}" "${oracle}"
        done
    done

    for asset in EURC AQUA USDGLO USTRY CETES PYUSD; do
        asset_id="$(state_value "$(modeled_asset_state_key "${asset}")")"
        extend_contract_instance \
            "${stamp}-asset-${asset}-instance" "${asset_id}" >/dev/null
    done
    state_set "modeled_pools_keepalive_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

command_plan() {
    local asset_description valuation_description pool_description
    local incumbent_funding_description pool_funding_description

    if [[ "${XLM_ASSET_MODE}" == "native" ]]; then
        asset_description="synthetic BLND and USDC plus Friendbot-funded native XLM"
    else
        asset_description="isolated BLND, USDC, and XLM assets"
    fi
    valuation_description="integrated Comet-derived backstop valuation and bounded test-only lending oracles"
    pool_description="matched Fixed and YieldBlox v2/v3 pools from captured reserve configurations"
    pool_funding_description="three candidate backstop tiers plus the configured Fixed and YieldBlox emission positions"
    incumbent_funding_description="Fund both modeled v2 pools with ${INCUMBENT_BALANCE} BLNDUSDC each while retaining the candidate's one-base-unit aggregate majority."
    cat <<EOF
Blend v3 ${NETWORK_LABEL} migration plan

Network:
  Protocol: 27
  Mode: ${NETWORK_MODE}
  XLM asset mode: ${XLM_ASSET_MODE}
  RPC: ${RPC_URL}
  Work directory: ${WORK_DIR}

Artifacts:
  Frozen incumbent: exact deployed mainnet v2 pool, pool factory, and backstop WASM
  Deployed emitter: exact deployed mainnet emitter WASM
  Candidate: SDK 27 pool, pool factory, and backstop with ${valuation_description}

Deployment:
  1. Create and fund isolated issuer/operator and depositor identities.
  2. Provision ${asset_description} and deploy real 80:20 BLND:USDC and BLND:XLM Comet pools.
  3. Deploy the exact emitter WASM and a coherent SDK-22 incumbent stack.
  4. Predict the candidate address, bind its immutable factory, and deploy it.
  5. Deploy ${pool_description} with transferable v2-style admins.
  6. Fund them with ${pool_funding_description}, activate them, add them to their reward zones, and configure their selected reserve-position emissions.
  7. ${incumbent_funding_description}
  8. Assert strict candidate qualification, queue the emitter swap, and call distribute.

Time-gated continuation:
  9. Call distribute during the final seven days to prepare the queue.
 10. Call the emitter swap and then distribute at or after unlock.
 11. Verify the emitter, candidate, pool, balances, and cleared queue.
 12. Distribute ongoing BLND, compound both backstop tiers, and gulp both pool tranches.
 13. Claim both pool streams after their seven-day vesting period.

Mutation is limited to ${NETWORK_LABEL} and ${WORK_DIR}.
EOF
}

command_validate() {
    validate_tools
    validate_source_provenance
    build_artifacts
    validate_artifacts
}

command_start() {
    validate_tools
    if [[ "${NETWORK_MODE}" == "testnet" ]]; then
        mkdir -p "${CONFIG_DIR}"
        require_network
        note "${NETWORK_LABEL} RPC is healthy at ${RPC_URL}."
        return
    fi
    require_local_mode
    require_docker
    mkdir -p "${CONFIG_DIR}"
    network_healthy && die "a network already responds at ${RPC_URL}; refusing to replace it"

    note "Starting ${CONTAINER_NAME} at Protocol 27 with testnet limits..."
    stellar_cli container start local \
        --docker-host "${DOCKER_HOST_URI}" \
        --name "${CONTAINER_NAME}" \
        --limits testnet \
        --protocol-version 27 \
        --ports-mapping "${RPC_PORT}:8000"
    wait_for_network
}

begin_deployed_migration() {
    local emitter_id incumbent_id candidate_id blnd_usdc_token_id blnd_xlm_token_id
    local incumbent_balance candidate_balance epoch_start queued_swap unlock_time
    local queued_candidate queued_blnd_xlm_token migration_state migration_status

    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    blnd_usdc_token_id="$(state_value blnd_usdc_token)"
    blnd_xlm_token_id="$(state_value blnd_xlm_token)"

    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "query-incumbent-blnd-usdc-balance" "${blnd_usdc_token_id}" balance \
        --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "query-candidate-blnd-usdc-balance" "${blnd_usdc_token_id}" balance \
        --id "${candidate_id}")")"
    assert_equal "${AGGREGATE_INCUMBENT_BALANCE}" \
        "${incumbent_balance}" "incumbent balance"
    assert_equal "${AGGREGATE_CANDIDATE_BALANCE}" \
        "${candidate_balance}" "candidate balance"
    (( candidate_balance > incumbent_balance )) ||
        die "candidate does not strictly exceed the incumbent BLND:USDC LP balance"

    migration_state="$(invoke_view \
        "query-candidate-before-begin" "${candidate_id}" migration_state)"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    if printf '%s' "${migration_status}" | jq -e '
        . == "Pending" or
        (type == "object" and (has("Pending") or .tag? == "Pending"))
    ' >/dev/null; then
        queued_swap="$(invoke_view \
            "query-emitter-queue-before-begin" "${emitter_id}" get_queued_swap)"
        if printf '%s' "${queued_swap}" | jq -e '. == null' >/dev/null; then
            invoke_transaction \
                "queue-migration" "${emitter_id}" queue_swap_backstop \
                --new_backstop "${candidate_id}" \
                --new_backstop_token "${blnd_xlm_token_id}" >/dev/null
        fi
        invoke_transaction \
            "observe-migration-queue" "${candidate_id}" distribute >/dev/null
        migration_state="$(invoke_view \
            "query-open-migration-state" "${candidate_id}" migration_state)"
        epoch_start="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
            jq -c '.migration_epoch_start')")"
    elif printf '%s' "${migration_status}" | jq -e '
        . == "Open" or . == "Prepared" or
        (type == "object" and
            (has("Open") or has("Prepared") or .tag? == "Open" or .tag? == "Prepared"))
    ' >/dev/null; then
        epoch_start="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
            jq -c '.migration_epoch_start')")"
    else
        die "cannot begin deployment from candidate migration state ${migration_status}"
    fi
    [[ "${epoch_start}" =~ ^[0-9]+$ ]] ||
        die "invalid migration epoch start: ${epoch_start}"
    queued_swap="$(invoke_view \
        "query-open-emitter-queue" "${emitter_id}" get_queued_swap)"
    unlock_time="$(printf '%s' "${queued_swap}" | jq -er '.unlock_time')"
    queued_candidate="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop')"
    queued_blnd_xlm_token="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop_token')"
    [[ "${unlock_time}" =~ ^[0-9]+$ ]] ||
        die "invalid migration unlock time: ${unlock_time}"
    assert_equal "${candidate_id}" "${queued_candidate}" "queued candidate"
    assert_equal "${blnd_xlm_token_id}" \
        "${queued_blnd_xlm_token}" "queued next designated token"
    state_set "migration_epoch_start" "${epoch_start}"
    state_set "unlock_time" "${unlock_time}"
    state_set "prepare_not_before" \
        "$((unlock_time - PREPARATION_WINDOW_SECONDS))"
    state_set "incumbent_balance" "${AGGREGATE_INCUMBENT_BALANCE}"
    state_set "candidate_balance" "${AGGREGATE_CANDIDATE_BALANCE}"
    state_set "activation_usdc_balance" "${ACTIVATION_USDC_BALANCE}"
    state_set "pool_supply_balance" "${POOL_SUPPLY_BALANCE}"
    state_set "phase" "open"

    stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted >"${RUN_DIR}/ledger-after-begin.json"
    note "Deployment and migration queue succeeded. State: ${STATE_FILE}"
    command_status
}

command_deploy() {
    local skip_build="${1:-false}"
    local operator depositor issuer emitter_id blnd_id xlm_id blnd_usdc_token_id blnd_xlm_token_id
    local usdc_token_id
    local v2_pool_hash incumbent_id v2_factory_id deployed_incumbent_id
    local v3_pool_hash candidate_id v3_factory_id
    local deployed_candidate_id
    local fixed_v2_oracle fixed_v2_pool fixed_v3_oracle
    local pool_id
    local pool_status
    local comet_xlm_balance="${COMET_INITIAL_PAIR_TOKEN}"

    if [[ "${XLM_ASSET_MODE}" == "native" ]]; then
        comet_xlm_balance="${COMET_INITIAL_XLM}"
    fi

    validate_tools
    validate_source_provenance
    require_network
    [[ ! -e "${STATE_FILE}" ]] ||
        die "deployment state already exists at ${STATE_FILE}; refusing to overwrite it"
    if [[ "${skip_build}" != "true" ]]; then
        build_artifacts
    fi
    validate_artifacts
    new_run_dir

    note "Creating and funding isolated ${NETWORK_LABEL} operator ${OPERATOR_IDENTITY}..."
    stellar_cli keys generate "${OPERATOR_IDENTITY}" \
        --overwrite >/dev/null
    operator="$(stellar_cli keys public-key "${OPERATOR_IDENTITY}")"
    fund_and_wait_for_account "${operator}"
    issuer="${operator}"
    initialize_state "${operator}"
    state_set "emitter_wasm_sha256" "${EXPECTED_EMITTER_SHA256}"
    state_set "v2_factory_wasm_sha256" "$(sha256_file "${V2_FACTORY_WASM}")"
    state_set "v2_backstop_wasm_sha256" "$(sha256_file "${V2_BACKSTOP_WASM}")"
    state_set "v2_pool_wasm_sha256" "$(sha256_file "${V2_POOL_WASM}")"
    state_set "v3_factory_wasm_sha256" "$(sha256_file "${V3_FACTORY_WASM}")"
    state_set "v3_backstop_wasm_sha256" "$(sha256_file "${V3_BACKSTOP_WASM}")"
    state_set "v3_pool_wasm_sha256" "$(sha256_file "${V3_POOL_WASM}")"
    state_set "test_sep40_oracle_wasm_sha256" \
        "$(sha256_file "${V3_TEST_SEP40_ORACLE_WASM}")"
    state_set "comet_wasm_sha256" "$(sha256_file "${COMET_WASM}")"

    note "Creating and funding isolated ${NETWORK_LABEL} depositor ${DEPOSITOR_IDENTITY}..."
    stellar_cli keys generate "${DEPOSITOR_IDENTITY}" \
        --overwrite >/dev/null
    depositor="$(stellar_cli keys public-key "${DEPOSITOR_IDENTITY}")"
    fund_and_wait_for_account "${depositor}"
    state_set "depositor" "${depositor}"

    emitter_id="$(deploy_wasm "deploy-deployed-emitter" "${EMITTER_WASM}" "")"
    state_set "emitter" "${emitter_id}"

    blnd_id="$(deploy_asset "deploy-asset-blnd" "BLND" "${issuer}")"
    usdc_token_id="$(deploy_asset "deploy-asset-usdc" "USDC" "${issuer}")"
    if [[ "${XLM_ASSET_MODE}" == "native" ]]; then
        xlm_id="$(resolve_native_asset "resolve-asset-native-xlm")"
    else
        xlm_id="$(deploy_asset "deploy-asset-xlm" "XLM" "${issuer}")"
    fi
    blnd_usdc_token_id="$(deploy_wasm "deploy-comet-blnd-usdc" "${COMET_WASM}" "")"
    blnd_xlm_token_id="$(deploy_wasm "deploy-comet-blnd-xlm" "${COMET_WASM}" "")"
    state_set "blnd_token" "${blnd_id}"
    state_set "xlm_token" "${xlm_id}"
    state_set "blnd_usdc_token" "${blnd_usdc_token_id}"
    state_set "blnd_xlm_token" "${blnd_xlm_token_id}"
    state_set "usdc_token" "${usdc_token_id}"
    state_set "xlm_asset_mode" "${XLM_ASSET_MODE}"

    capture "create-depositor-blnd-trustline" \
        stellar_cli tx new change-trust \
        --source-account "${DEPOSITOR_IDENTITY}" \
        --line "BLND:${issuer}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    capture "create-depositor-usdc-trustline" \
        stellar_cli tx new change-trust \
        --source-account "${DEPOSITOR_IDENTITY}" \
        --line "USDC:${issuer}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        capture "create-depositor-xlm-trustline" \
            stellar_cli tx new change-trust \
            --source-account "${DEPOSITOR_IDENTITY}" \
            --line "XLM:${issuer}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    fi
    invoke_transaction "mint-depositor-blnd" "${blnd_id}" mint \
        --to "${depositor}" --amount "${DEPOSITOR_BLND_MINT}" >/dev/null
    invoke_transaction "mint-depositor-usdc" "${usdc_token_id}" mint \
        --to "${depositor}" --amount "${DEPOSITOR_USDC_MINT}" >/dev/null
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        invoke_transaction "mint-depositor-xlm" "${xlm_id}" mint \
            --to "${depositor}" --amount "${DEPOSITOR_XLM_MINT}" >/dev/null
    fi
    invoke_transaction_as "initialize-comet-blnd-usdc" \
        "${DEPOSITOR_IDENTITY}" "${blnd_usdc_token_id}" init \
        --controller "${depositor}" \
        --tokens "[\"${blnd_id}\",\"${usdc_token_id}\"]" \
        --weights '["8000000","2000000"]' \
        --balances "[\"${COMET_INITIAL_BLND}\",\"${COMET_INITIAL_PAIR_TOKEN}\"]" \
        --swap_fee 30000 >/dev/null
    invoke_transaction_as "initialize-comet-blnd-xlm" \
        "${DEPOSITOR_IDENTITY}" "${blnd_xlm_token_id}" init \
        --controller "${depositor}" \
        --tokens "[\"${blnd_id}\",\"${xlm_id}\"]" \
        --weights '["8000000","2000000"]' \
        --balances "[\"${COMET_INITIAL_BLND}\",\"${comet_xlm_balance}\"]" \
        --swap_fee 30000 >/dev/null
    invoke_transaction_as "join-comet-blnd-usdc" \
        "${DEPOSITOR_IDENTITY}" "${blnd_usdc_token_id}" join_pool \
        --pool_amount_out "${BLND_USDC_LP_JOIN_AMOUNT}" \
        --max_amounts_in \
        "[\"$((13000 * SCALAR_7))\",\"$((500 * SCALAR_7))\"]" \
        --user "${depositor}" >/dev/null

    invoke_transaction "set-emitter-as-blnd-admin" "${blnd_id}" set_admin \
        --new_admin "${emitter_id}" >/dev/null

    v2_pool_hash="$(upload_wasm "upload-v2-pool" "${V2_POOL_WASM}")"
    state_set "v2_pool_wasm_hash" "${v2_pool_hash}"
    incumbent_id="$(predict_contract_id "${INCUMBENT_SALT}")"
    require_contract_id "${incumbent_id}" "predicted incumbent"
    state_set "incumbent_backstop" "${incumbent_id}"

    v2_factory_id="$(deploy_wasm \
        "deploy-v2-pool-factory" "${V2_FACTORY_WASM}" "" \
        --pool_init_meta \
        "{\"backstop\":\"${incumbent_id}\",\"blnd_id\":\"${blnd_id}\",\"pool_hash\":\"${v2_pool_hash}\"}")"
    state_set "v2_pool_factory" "${v2_factory_id}"

    deployed_incumbent_id="$(deploy_wasm \
        "deploy-v2-backstop" "${V2_BACKSTOP_WASM}" "${INCUMBENT_SALT}" \
        --backstop_token "${blnd_usdc_token_id}" \
        --emitter "${emitter_id}" \
        --blnd_token "${blnd_id}" \
        --usdc_token "${usdc_token_id}" \
        --pool_factory "${v2_factory_id}" \
        --drop_list "[]")"
    assert_equal "${incumbent_id}" "${deployed_incumbent_id}" \
        "incumbent deterministic address"

    invoke_transaction "initialize-deployed-emitter" "${emitter_id}" initialize \
        --blnd_token "${blnd_id}" \
        --backstop "${incumbent_id}" \
        --backstop_token "${blnd_usdc_token_id}" >/dev/null

    v3_pool_hash="$(upload_wasm "upload-v3-pool" "${V3_POOL_WASM}")"
    state_set "v3_pool_wasm_hash" "${v3_pool_hash}"
    candidate_id="$(predict_contract_id "${CANDIDATE_SALT}")"
    require_contract_id "${candidate_id}" "predicted candidate"
    state_set "candidate_backstop" "${candidate_id}"

    v3_factory_id="$(deploy_wasm \
        "deploy-v3-pool-factory" "${V3_FACTORY_WASM}" "" \
        --pool_init_meta \
        "{\"backstop\":\"${candidate_id}\",\"blnd_id\":\"${blnd_id}\",\"pool_hash\":\"${v3_pool_hash}\"}")"
    state_set "v3_pool_factory" "${v3_factory_id}"

    deployed_candidate_id="$(deploy_wasm \
        "deploy-v3-backstop" "${V3_BACKSTOP_WASM}" "${CANDIDATE_SALT}" \
        --emitter "${emitter_id}" \
        --blnd_token "${blnd_id}" \
        --pool_factory "${v3_factory_id}" \
        --blnd_usdc_token "${blnd_usdc_token_id}" \
        --blnd_xlm_token "${blnd_xlm_token_id}" \
        --usdc_token "${usdc_token_id}" \
        --xlm_token "${xlm_id}" \
        --drop_list "[]")"
    assert_equal "${candidate_id}" "${deployed_candidate_id}" \
        "candidate deterministic address"

    # Every lane uses the modeled Fixed pair as its incumbent/candidate lending
    # topology. The candidate's integrated backstop valuation remains
    # independent from the test-only lending oracles used by these pools.
    ensure_modeled_asset EURC
    state_set "modeled_pools_fixture_sha256" \
        "$(sha256_file "${MODELED_POOLS_FIXTURE}")"
    state_set "modeled_v3_lending_oracle_wasm_sha256" \
        "$(sha256_file "${V3_TEST_SEP40_ORACLE_WASM}")"
    fixed_v2_oracle="$(ensure_modeled_v2_oracle fixed)"
    fixed_v3_oracle="$(ensure_modeled_v3_oracle fixed)"
    fixed_v2_pool="$(ensure_modeled_v2_pool fixed "${fixed_v2_oracle}")"
    pool_id="$(ensure_modeled_v3_pool fixed "${fixed_v3_oracle}")"
    state_set "v3_pool" "${pool_id}"
    state_set "primary_pool_supply_asset" "${usdc_token_id}"
    state_set "primary_pool_emission_reserve_token_id" "4"
    state_set "modeled_v3_fixed_blnd_usdc_target" "${CANDIDATE_BALANCE}"

    ensure_modeled_blnd_usdc_lp_capacity \
        "$((INCUMBENT_BALANCE + CANDIDATE_BALANCE))"
    invoke_transaction_as "deposit-candidate-blnd-usdc-lp" \
        "${DEPOSITOR_IDENTITY}" "${candidate_id}" deposit \
        --tier BlndUsdc \
        --from "${depositor}" \
        --pool-address "${pool_id}" \
        --amount "${CANDIDATE_BALANCE}" >/dev/null
    ensure_modeled_v2_backstop_deposit fixed "${fixed_v2_pool}"
    invoke_transaction "activate-fixed-v2-primary-pool" \
        "${fixed_v2_pool}" set_status --pool_status 0 >/dev/null
    invoke_transaction_as "deposit-candidate-usdc" \
        "${DEPOSITOR_IDENTITY}" "${candidate_id}" deposit \
        --tier Usdc \
        --from "${depositor}" \
        --pool-address "${pool_id}" \
        --amount "${ACTIVATION_USDC_BALANCE}" >/dev/null
    invoke_transaction \
        "activate-v3-pool" "${pool_id}" set_status --pool_status 0 >/dev/null
    pool_status="$(invoke_view \
        "query-active-v3-pool" "${pool_id}" get_config |
        jq -er '.status | tostring')"
    assert_equal "0" "${pool_status}" "activated pool status"
    invoke_transaction "add-v3-pool-to-reward-zone" "${candidate_id}" add_reward \
        --to_add "${pool_id}" \
        --to_remove null >/dev/null
    invoke_transaction "configure-v3-pool-emissions" "${pool_id}" set_emissions_config \
        --res_emission_metadata \
        '[{"res_index":2,"res_type":0,"share":1}]' \
        >/dev/null
    invoke_transaction_as "seed-v3-pool-supply" \
        "${DEPOSITOR_IDENTITY}" "${pool_id}" submit \
        --from "${depositor}" \
        --spender "${depositor}" \
        --to "${depositor}" \
        --requests "[{\"address\":\"${usdc_token_id}\",\"amount\":\"${POOL_SUPPLY_BALANCE}\",\"request_type\":0}]" >/dev/null
    ensure_fixed_v3_eurc_borrow "${pool_id}"

    # Complete both modeled v2/v3 pool pairs before opening the migration
    # epoch, so both candidate BLND:USDC positions are prefunded before
    # backfill begins. The setup command is idempotent and remains available
    # to resume an interrupted deployment.
    state_set "phase" "deploying"
    command_deploy_modeled_pools
}

command_deploy_modeled_pools() {
    local phase fixed_v2_oracle yield_v2_oracle fixed_v3_oracle yield_v3_oracle
    local fixed_v2_pool yield_v2_pool fixed_v3_pool yield_v3_pool
    local usdc_required fixed_usdc_shares yield_usdc_shares
    local fixed_v2_shares yield_v2_shares fixed_v3_shares yield_v3_shares
    local fixed_supply yield_supply lp_required fixed_v3_target yield_v3_target
    local incumbent_balance candidate_balance pool_status reward_zone stream
    local now refresh_streams=false

    validate_tools
    validate_source_provenance
    require_network
    load_state
    require_modeled_pool_support
    phase="$(state_value phase)"
    [[ "${phase}" == "deploying" || "${phase}" == "open" ||
        "${phase}" == "prepared" ]] ||
        die "modeled pre-migration pools require a deploying, open, or prepared migration; saved phase is ${phase}"

    record_modeled_pools_provenance deployment
    state_set "modeled_pools_fixture_sha256" \
        "$(sha256_file "${MODELED_POOLS_FIXTURE}")"
    state_set "modeled_v3_lending_oracle_wasm_sha256" \
        "$(sha256_file "${V3_TEST_SEP40_ORACLE_WASM}")"

    for asset in EURC AQUA USDGLO USTRY CETES PYUSD; do
        ensure_modeled_asset "${asset}"
    done

    fixed_v2_oracle="$(ensure_modeled_v2_oracle fixed)"
    yield_v2_oracle="$(ensure_modeled_v2_oracle yieldblox)"
    fixed_v3_oracle="$(ensure_modeled_v3_oracle fixed)"
    yield_v3_oracle="$(ensure_modeled_v3_oracle yieldblox)"

    fixed_v2_pool="$(ensure_modeled_v2_pool fixed "${fixed_v2_oracle}")"
    yield_v2_pool="$(ensure_modeled_v2_pool yieldblox "${yield_v2_oracle}")"
    fixed_v3_pool="$(ensure_modeled_v3_pool fixed "${fixed_v3_oracle}")"
    yield_v3_pool="$(ensure_modeled_v3_pool yieldblox "${yield_v3_oracle}")"

    fixed_v2_shares="$(modeled_v2_user_shares "${fixed_v2_pool}")"
    yield_v2_shares="$(modeled_v2_user_shares "${yield_v2_pool}")"
    fixed_v3_shares="$(modeled_v3_user_shares "${fixed_v3_pool}" BlndUsdc)"
    yield_v3_shares="$(modeled_v3_user_shares "${yield_v3_pool}" BlndUsdc)"
    fixed_v3_target="$(modeled_v3_blnd_usdc_target fixed)"
    yield_v3_target="$(modeled_v3_blnd_usdc_target yieldblox)"
    for value in "${fixed_v2_shares}" "${yield_v2_shares}"; do
        [[ "${value}" =~ ^[0-9]+$ ]] ||
            die "invalid modeled BLND:USDC share state"
        (( value <= MODELED_BACKSTOP_LP_BALANCE )) ||
            die "modeled BLND:USDC shares exceed the active fixture target"
    done
    [[ "${fixed_v3_shares}" =~ ^[0-9]+$ &&
        "${yield_v3_shares}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled v3 BLND:USDC share state"
    (( fixed_v3_shares <= fixed_v3_target )) ||
        die "fixed v3 BLND:USDC shares exceed the active fixture target"
    (( yield_v3_shares <= yield_v3_target )) ||
        die "YieldBlox v3 BLND:USDC shares exceed the active fixture target"
    lp_required=$((
        (MODELED_BACKSTOP_LP_BALANCE - fixed_v2_shares) +
        (MODELED_BACKSTOP_LP_BALANCE - yield_v2_shares) +
        (fixed_v3_target - fixed_v3_shares) +
        (yield_v3_target - yield_v3_shares)
    ))
    ensure_modeled_blnd_usdc_lp_capacity "${lp_required}"

    # Fund candidate BLND:USDC first for each matched pair, so the candidate
    # never loses its strict BLND:USDC LP majority while the v2 side is funded.
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" BlndUsdc \
        "${fixed_v3_target}"
    ensure_modeled_v2_backstop_deposit fixed "${fixed_v2_pool}"
    ensure_modeled_v3_backstop_deposit \
        yieldblox "${yield_v3_pool}" BlndUsdc \
        "${yield_v3_target}"
    ensure_modeled_v2_backstop_deposit yieldblox "${yield_v2_pool}"

    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" BlndXlm \
        "${MODELED_BLND_XLM_LP_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        yieldblox "${yield_v3_pool}" BlndXlm \
        "${MODELED_BLND_XLM_LP_BALANCE}"

    fixed_usdc_shares="$(modeled_v3_user_shares "${fixed_v3_pool}" Usdc)"
    yield_usdc_shares="$(modeled_v3_user_shares "${yield_v3_pool}" Usdc)"
    [[ "${fixed_usdc_shares}" =~ ^[0-9]+$ &&
        "${yield_usdc_shares}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled v3 USDC share state"
    usdc_required=$((
        (MODELED_V3_USDC_BALANCE - fixed_usdc_shares) +
        (MODELED_V3_USDC_BALANCE - yield_usdc_shares)
    ))
    (( usdc_required >= 0 )) ||
        die "modeled v3 USDC shares exceed their fixture targets"
    ensure_token_balance "$(state_value usdc_token)" \
        "${usdc_required}" "modeled-usdc"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" Usdc \
        "${MODELED_V3_USDC_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        yieldblox "${yield_v3_pool}" Usdc \
        "${MODELED_V3_USDC_BALANCE}"

    for pool in "${fixed_v3_pool}" "${yield_v3_pool}"; do
        pool_status="$(invoke_view \
            "query-modeled-v3-status" "${pool}" get_config |
            jq -er '.status | tostring')"
        if [[ "${pool_status}" != "0" && "${pool_status}" != "1" ]]; then
            invoke_transaction \
                "activate-modeled-v3-pool" \
                "${pool}" set_status --pool_status 0 >/dev/null
            pool_status="$(invoke_view \
                "query-active-modeled-v3-pool" "${pool}" get_config |
                jq -er '.status | tostring')"
        fi
        [[ "${pool_status}" == "0" || "${pool_status}" == "1" ]] ||
            die "modeled v3 pool did not activate: ${pool} status ${pool_status}"
    done

    for pool in "${fixed_v2_pool}" "${yield_v2_pool}"; do
        pool_status="$(invoke_view "query-modeled-v2-active-status" \
            "${pool}" get_config | jq -er '.status | tostring')"
        if [[ "${pool_status}" == "2" ]]; then
            invoke_transaction "activate-modeled-v2-pool" \
                "${pool}" set_status --pool_status 0 >/dev/null
            pool_status="0"
        fi
        assert_equal "0" "${pool_status}" "modeled v2 active status"
    done

    ensure_modeled_v3_emissions_config fixed "${fixed_v3_pool}"
    ensure_modeled_v3_emissions_config yieldblox "${yield_v3_pool}"
    ensure_modeled_v3_supply fixed "${fixed_v3_pool}"
    ensure_modeled_v3_supply yieldblox "${yield_v3_pool}"
    ensure_fixed_v3_eurc_borrow "${fixed_v3_pool}"
    state_set "primary_pool_emission_reserve_token_id" "4"
    state_set "modeled_v3_emissions_prepared_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    admit_modeled_v3_yieldblox_reward "$(state_value candidate_backstop)"

    if [[ -z "$(state_optional modeled_v2_emissions_configured_at)" ]]; then
        for pool in "${fixed_v2_pool}" "${yield_v2_pool}"; do
            invoke_transaction "configure-modeled-v2-emissions" \
                "${pool}" set_emissions_config \
                --res_emission_metadata \
                '[{"res_index":1,"res_type":1,"share":1}]' >/dev/null
        done
        state_set "modeled_v2_emissions_configured_at" \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fi

    fixed_supply="$(modeled_v2_user_supply "${fixed_v2_pool}")"
    yield_supply="$(modeled_v2_user_supply "${yield_v2_pool}")"
    usdc_required=$((
        (MODELED_V2_USDC_SUPPLY_BALANCE - fixed_supply) +
        (MODELED_V2_USDC_SUPPLY_BALANCE - yield_supply)
    ))
    (( usdc_required >= 0 )) ||
        die "modeled v2 USDC supply exceeds its fixture target"
    ensure_token_balance "$(state_value usdc_token)" \
        "${usdc_required}" "modeled-v2-usdc-supply"
    ensure_modeled_v2_supply fixed "${fixed_v2_pool}"
    ensure_modeled_v2_supply yieldblox "${yield_v2_pool}"

    reward_zone="$(invoke_view "query-modeled-v2-reward-zone" \
        "$(state_value incumbent_backstop)" reward_zone)"
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${fixed_v2_pool}" 'index($pool) != null' >/dev/null
    then
        if [[ "$(printf '%s' "${reward_zone}" | jq -er 'length')" != "0" ]]; then
            advance_modeled_v2_emissions "refresh-before-fixed-v2-reward"
        fi
        invoke_transaction "add-fixed-v2-reward" \
            "$(state_value incumbent_backstop)" add_reward \
            --to_add "${fixed_v2_pool}" --to_remove null >/dev/null
        if [[ "$(printf '%s' "${reward_zone}" | jq -er 'length')" == "0" ]]; then
            # Initialize the v2 backstop's local distribution checkpoint before
            # advancing the emitter, preserving the first emitted tranche.
            invoke_transaction "initialize-modeled-v2-backstop-distribution" \
                "$(state_value incumbent_backstop)" distribute >/dev/null
        fi
        reward_zone="$(invoke_view "query-modeled-v2-reward-zone-fixed" \
            "$(state_value incumbent_backstop)" reward_zone)"
    fi
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${yield_v2_pool}" 'index($pool) != null' >/dev/null
    then
        advance_modeled_v2_emissions "prime-modeled-v2-reward-zone"
        invoke_transaction "add-yieldblox-v2-reward" \
            "$(state_value incumbent_backstop)" add_reward \
            --to_add "${yield_v2_pool}" --to_remove null >/dev/null
    fi

    now="$(date +%s)"
    for pool in "${fixed_v2_pool}" "${yield_v2_pool}"; do
        stream="$(invoke_view "query-modeled-v2-emission-stream" \
            "${pool}" get_reserve_emissions --reserve_token_index 3)"
        if ! modeled_v2_emission_stream_is_active "${stream}" "${now}"; then
            refresh_streams=true
        fi
    done
    if [[ "${refresh_streams}" == "true" ]]; then
        sleep "${EMISSION_START_DELAY_SECONDS}"
        advance_modeled_v2_emissions "start-or-renew-modeled-v2-stream"
    fi
    now="$(date +%s)"
    for pool in "${fixed_v2_pool}" "${yield_v2_pool}"; do
        stream="$(invoke_view "query-modeled-v2-emission-stream-ready" \
            "${pool}" get_reserve_emissions --reserve_token_index 3)"
        if ! modeled_v2_emission_stream_is_active "${stream}" "${now}"; then
            invoke_transaction "gulp-modeled-v2-emissions" \
                "${pool}" gulp_emissions >/dev/null
        fi
    done

    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "modeled-pools-incumbent-balance" \
        "$(state_value blnd_usdc_token)" balance \
        --id "$(state_value incumbent_backstop)")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "modeled-pools-candidate-balance" \
        "$(state_value blnd_usdc_token)" balance \
        --id "$(state_value candidate_backstop)")")"
    (( candidate_balance > incumbent_balance )) ||
        die "modeled pool funding broke the candidate's strict BLND:USDC LP majority"

    state_set "modeled_v2_emissions_started_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    state_set "modeled_pools_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    command_verify_modeled_pools
    if [[ "${phase}" == "deploying" ]]; then
        begin_deployed_migration
    fi
}

assert_json_equal() {
    local expected="$1" actual="$2" label="$3"
    local expected_normalized actual_normalized

    expected_normalized="$(printf '%s' "${expected}" | jq -S -c .)"
    actual_normalized="$(printf '%s' "${actual}" | jq -S -c .)"
    [[ "${actual_normalized}" == "${expected_normalized}" ]] ||
        die "${label}: expected ${expected_normalized}, found ${actual_normalized}"
}

verify_modeled_pool_reserves() {
    local version="$1" model="$2" pool="$3" expected_list="[]" actual_list
    local index=0 asset asset_id expected actual

    while IFS= read -r asset; do
        asset_id="$(modeled_asset_id "${asset}")"
        expected_list="$(printf '%s' "${expected_list}" |
            jq -c --arg asset "${asset_id}" '. + [$asset]')"
        if [[ "${version}" == "v2" ]]; then
            expected="$(modeled_v2_reserve_config "${model}" "${index}")"
            actual="$(invoke_view "verify-${model}-v2-reserve-${index}" \
                "${pool}" get_reserve --asset "${asset_id}" |
                jq -c '.config')"
        else
            expected="$(modeled_v2_reserve_config "${model}" "${index}")"
            actual="$(invoke_view "verify-${model}-v3-reserve-${index}" \
                "${pool}" get_reserve --asset "${asset_id}" |
                jq -c '.config')"
        fi
        assert_json_equal "${expected}" "${actual}" \
            "${version} ${model} reserve ${asset}"
        index=$((index + 1))
    done < <(jq -r --arg model "${model}" \
        '.pools[$model].reserves[].asset' "${MODELED_POOLS_FIXTURE}")

    if [[ "${version}" == "v2" ]]; then
        actual_list="$(invoke_view "verify-${model}-v2-reserve-list" \
            "${pool}" get_reserve_list)"
    else
        actual_list="$(invoke_view "verify-${model}-v3-reserve-list" \
            "${pool}" get_reserve_list)"
    fi
    assert_json_equal "${expected_list}" "${actual_list}" \
        "${version} ${model} reserve list"
}

command_verify_modeled_pools() {
    local version model pool oracle factory registered config expected_config
    local status v2_shares blnd_usdc_shares blnd_xlm_shares usdc_shares
    local incumbent_balance candidate_balance reward_zone stream supply
    local fixed_v3_target yield_v3_target now
    local oracle_admin
    local positions usdc_collateral eurc_supply eurc_borrow

    validate_tools
    require_network
    load_state
    require_modeled_pool_support
    validate_source_provenance
    record_modeled_pools_provenance verification

    for version in v2 v3; do
        for model in fixed yieldblox; do
            pool="$(state_value "$(modeled_pool_state_key "${version}" "${model}")")"
            oracle="$(state_value "$(modeled_oracle_state_key "${version}" "${model}")")"
            if [[ "${version}" == "v2" ]]; then
                factory="$(state_value v2_pool_factory)"
                registered="$(normalize_scalar "$(invoke_view \
                    "verify-${model}-v2-registration" "${factory}" is_pool \
                    --pool_address "${pool}")")"
                config="$(invoke_view "verify-${model}-v2-config" \
                    "${pool}" get_config)"
                expected_config="$(jq -c \
                    --arg model "${model}" \
                    --arg oracle "${oracle}" \
                    '.pools[$model] | {
                        bstop_rate: .backstop_take_rate,
                        max_positions,
                        min_collateral: (.min_collateral | tostring),
                        oracle: $oracle,
                        status: 0
                    }' "${MODELED_POOLS_FIXTURE}")"
            else
                factory="$(state_value v3_pool_factory)"
                registered="$(normalize_scalar "$(invoke_view \
                    "verify-${model}-v3-registration" "${factory}" is_pool \
                    --pool_address "${pool}")")"
                oracle_admin="$(normalize_scalar "$(invoke_view \
                    "verify-${model}-v3-oracle-admin" \
                    "${oracle}" admin)")"
                assert_equal "$(state_value operator)" "${oracle_admin}" \
                    "${version} ${model} lending-oracle admin"
                config="$(invoke_view "verify-${model}-v3-config" \
                    "${pool}" get_config)"
                expected_config="$(jq -c \
                    --arg model "${model}" \
                    --arg oracle "${oracle}" \
                    '.pools[$model] | {
                        bstop_rate: .backstop_take_rate,
                        max_positions,
                        min_collateral: (.min_collateral | tostring),
                        oracle: $oracle,
                        status: 0
                    }' "${MODELED_POOLS_FIXTURE}")"
            fi
            assert_equal "true" "${registered}" \
                "${version} ${model} factory registration"
            assert_json_equal "${expected_config}" "${config}" \
                "${version} ${model} lending config"
            verify_modeled_pool_reserves "${version}" "${model}" "${pool}"
        done
    done

    fixed_v3_target="$(modeled_v3_blnd_usdc_target fixed)"
    yield_v3_target="$(modeled_v3_blnd_usdc_target yieldblox)"
    now="$(date +%s)"
    for model in fixed yieldblox; do
        pool="$(state_value "$(modeled_pool_state_key v2 "${model}")")"
        v2_shares="$(modeled_v2_user_shares "${pool}")"
        assert_equal "${MODELED_BACKSTOP_LP_BALANCE}" "${v2_shares}" \
            "${model} v2 BLND:USDC backstop shares"
        supply="$(modeled_v2_user_supply "${pool}")"
        assert_equal "${MODELED_V2_USDC_SUPPLY_BALANCE}" "${supply}" \
            "${model} v2 USDC supply"
        stream="$(invoke_view "verify-${model}-v2-emission-stream" \
            "${pool}" get_reserve_emissions --reserve_token_index 3)"
        modeled_v2_emission_stream_is_active "${stream}" "${now}" ||
            die "${model} v2 USDC supply emission stream is missing or expired"

        pool="$(state_value "$(modeled_pool_state_key v3 "${model}")")"
        status="$(invoke_view \
            "verify-${model}-v3-status" "${pool}" get_config |
            jq -er '.status | tostring')"
        [[ "${status}" == "0" || "${status}" == "1" ]] ||
            die "${model} v3 pool is not active: status ${status}"
        blnd_usdc_shares="$(modeled_v3_user_shares "${pool}" BlndUsdc)"
        blnd_xlm_shares="$(modeled_v3_user_shares "${pool}" BlndXlm)"
        usdc_shares="$(modeled_v3_user_shares "${pool}" Usdc)"
        if [[ "${model}" == "fixed" ]]; then
            assert_equal "${fixed_v3_target}" \
                "${blnd_usdc_shares}" "${model} v3 BLND:USDC shares"
        else
            assert_equal "${yield_v3_target}" \
                "${blnd_usdc_shares}" "${model} v3 BLND:USDC shares"
        fi
        assert_equal "${MODELED_BLND_XLM_LP_BALANCE}" \
            "${blnd_xlm_shares}" "${model} v3 BLND:XLM shares"
        assert_equal "${MODELED_V3_USDC_BALANCE}" \
            "${usdc_shares}" "${model} v3 USDC shares"
        supply="$(modeled_v3_user_supply "${pool}" 1)"
        assert_equal "${POOL_SUPPLY_BALANCE}" "${supply}" \
            "${model} v3 USDC supply"
    done

    pool="$(state_value modeled_v3_fixed_pool)"
    positions="$(invoke_view \
        "verify-fixed-v3-emission-positions" "${pool}" get_positions \
        --address "$(state_value depositor)")"
    usdc_collateral="$(printf '%s' "${positions}" |
        jq -er '(.collateral["1"] // "0") | tostring')"
    eurc_supply="$(printf '%s' "${positions}" |
        jq -er '(.supply["2"] // "0") | tostring')"
    eurc_borrow="$(printf '%s' "${positions}" |
        jq -er '(.liabilities["2"] // "0") | tostring')"
    assert_equal "${MODELED_FIXED_V3_USDC_COLLATERAL}" \
        "${usdc_collateral}" "Fixed v3 USDC collateral"
    assert_equal "${MODELED_FIXED_V3_EURC_SUPPLY}" \
        "${eurc_supply}" "Fixed v3 EURC supply"
    assert_equal "${MODELED_FIXED_V3_EURC_BORROW}" \
        "${eurc_borrow}" "Fixed v3 EURC borrow"

    reward_zone="$(invoke_view \
        "verify-modeled-v3-candidate-reward-zone" \
        "$(state_value candidate_backstop)" reward_zone)"
    for pool in \
        "$(state_value modeled_v3_fixed_pool)" \
        "$(state_value modeled_v3_yieldblox_pool)"
    do
        printf '%s' "${reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
            die "modeled v3 pool is missing from the candidate reward zone: ${pool}"
    done
    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "verify-modeled-incumbent-balance" \
        "$(state_value blnd_usdc_token)" balance \
        --id "$(state_value incumbent_backstop)")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "verify-modeled-candidate-balance" \
        "$(state_value blnd_usdc_token)" balance \
        --id "$(state_value candidate_backstop)")")"
    (( candidate_balance > incumbent_balance )) ||
        die "candidate no longer strictly exceeds the incumbent BLND:USDC LP balance"

    reward_zone="$(invoke_view "verify-modeled-v2-reward-zone" \
        "$(state_value incumbent_backstop)" reward_zone)"
    for pool in \
        "$(state_value modeled_v2_fixed_pool)" \
        "$(state_value modeled_v2_yieldblox_pool)"
    do
        printf '%s' "${reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
            die "modeled v2 pool is missing from the incumbent reward zone: ${pool}"
    done

    state_set "modeled_pools_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    cat <<EOF
${NETWORK_LABEL} modeled pools verified
  v2 Fixed:     $(state_value modeled_v2_fixed_pool) (Admin Active, emitting)
  v2 YieldBlox: $(state_value modeled_v2_yieldblox_pool) (Admin Active, emitting)
  v3 Fixed:     $(state_value modeled_v3_fixed_pool) (active, EURC-borrow emissions prepared)
  v3 YieldBlox: $(state_value modeled_v3_yieldblox_pool) (active, USDC-supply emissions prepared)
  incumbent BLND:USDC balance: ${incumbent_balance}
  candidate BLND:USDC balance: ${candidate_balance}
EOF
}

command_status() {
    local emitter_id incumbent_id candidate_id pool_id blnd_xlm_token_id blnd_usdc_token_id
    local unlock_time prepare_not_before phase
    local emitter_backstop incumbent_balance candidate_balance queued_swap migration_state
    local pool_operator pool_status now expiration

    validate_tools
    require_network
    load_state
    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    pool_id="$(state_value v3_pool)"
    blnd_xlm_token_id="$(state_value blnd_xlm_token)"
    blnd_usdc_token_id="$(state_value blnd_usdc_token)"
    unlock_time="$(state_value unlock_time)"
    prepare_not_before="$(state_value prepare_not_before)"
    phase="$(state_value phase)"

    emitter_backstop="$(normalize_scalar "$(invoke_view \
        "status-emitter-backstop" "${emitter_id}" get_backstop)")"
    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "status-incumbent-balance" "${blnd_usdc_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "status-candidate-balance" "${blnd_usdc_token_id}" balance --id "${candidate_id}")")"
    queued_swap="$(invoke_view "status-emitter-queue" "${emitter_id}" get_queued_swap)"
    migration_state="$(invoke_view \
        "status-candidate-migration" "${candidate_id}" migration_state)"
    pool_operator="$(normalize_scalar "$(invoke_view \
        "status-pool-admin" "${pool_id}" get_admin)")"
    pool_status="$(invoke_view \
        "status-pool-status" "${pool_id}" get_config |
        jq -er '.status | tostring')"
    now="$(date +%s)"

    if [[ "${queued_swap}" != "null" ]]; then
        assert_queued_swap_matches \
            "${queued_swap}" "${candidate_id}" "${blnd_xlm_token_id}" "${unlock_time}"
    elif [[ "${phase}" != "active" &&
        "${phase}" != "verified" &&
        "${phase}" != "emissions_streaming" &&
        "${phase}" != "emissions_claimed" ]]
    then
        die "deployed emitter queue is absent while saved phase is ${phase}"
    fi

    cat <<EOF
${NETWORK_LABEL} migration status
  emitter backstop: ${emitter_backstop}
  incumbent: ${incumbent_id}
  candidate: ${candidate_id}
  incumbent BLNDUSDC: ${incumbent_balance}
  candidate BLNDUSDC: ${candidate_balance}
  candidate state: ${migration_state}
  primary candidate pool: ${pool_id}
  candidate pool admin: ${pool_operator}
  candidate pool status: ${pool_status}
  queued swap: ${queued_swap}
  prepare not before: $(format_timestamp "${prepare_not_before}") (${prepare_not_before})
  unlock: $(format_timestamp "${unlock_time}") (${unlock_time})
EOF

    if [[ "${queued_swap}" == "null" ]]; then
        case "${phase}" in
            active)
                note "Emitter swap completed. Next action: run verify."
                ;;
            verified)
                note "Migration verified. Next action: run emissions."
                ;;
            emissions_streaming)
                expiration="$(state_value pool_emission_expiration)"
                if (( now < expiration )); then
                    note "Pool emissions are streaming. Claim opens in $((expiration - now)) seconds."
                else
                    note "Pool emission stream has expired. Next action: run claim-emissions."
                fi
                ;;
            emissions_claimed)
                note "Migration and the ongoing-emission exercise are complete."
                ;;
        esac
    elif (( now < prepare_not_before )); then
        note "Next action: checkpoint and prepare in $((prepare_not_before - now)) seconds."
    elif (( now < unlock_time )); then
        note "Next action: run prepare now; the emitter swap opens in $((unlock_time - now)) seconds."
    else
        note "Queue is unlocked. If prepared, run finalize to swap, synchronize, and drop backfill."
    fi
}

command_refresh_oracle() {
    local stamp

    validate_tools
    require_network
    load_state
    require_modeled_pool_support
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    refresh_modeled_pool_oracles "refresh-lending-oracles-${stamp}"
}

command_keepalive() {
    local emitter_id incumbent_id candidate_id factory_id pool_id depositor
    local blnd_id xlm_id blnd_usdc_token_id blnd_xlm_token_id usdc_token_id unlock_time
    local stamp prefix last_distro_key blnd_admin queued_swap emitter_backstop
    local incumbent_balance candidate_balance primary_pool_supply_asset
    local primary_pool_emission_reserve_token_id

    validate_tools
    require_network
    load_state
    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    factory_id="$(state_value v3_pool_factory)"
    pool_id="$(state_value v3_pool)"
    depositor="$(state_value depositor)"
    blnd_id="$(state_value blnd_token)"
    xlm_id="$(state_value xlm_token)"
    blnd_usdc_token_id="$(state_value blnd_usdc_token)"
    blnd_xlm_token_id="$(state_value blnd_xlm_token)"
    usdc_token_id="$(state_value usdc_token)"
    primary_pool_supply_asset="$(state_value primary_pool_supply_asset)"
    primary_pool_emission_reserve_token_id="$(
        state_value primary_pool_emission_reserve_token_id
    )"
    unlock_time="$(state_value unlock_time)"
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    prefix="keepalive-${stamp}"
    last_distro_key="$(last_distribution_key_xdr "${incumbent_id}")"
    [[ -n "${last_distro_key}" ]] ||
        die "failed to encode the incumbent emitter distribution key"

    extend_contract_instance "${prefix}-emitter-instance" \
        "${emitter_id}" >/dev/null
    queued_swap="$(invoke_forced_transaction \
        "${prefix}-emitter" "${emitter_id}" get_queued_swap)"
    emitter_backstop="$(normalize_scalar "$(invoke_forced_transaction \
        "${prefix}-emitter-backstop" "${emitter_id}" get_backstop)")"
    if [[ "${queued_swap}" != "null" ]]; then
        assert_queued_swap_matches \
            "${queued_swap}" "${candidate_id}" "${blnd_xlm_token_id}" "${unlock_time}"
    else
        assert_equal "${candidate_id}" "${emitter_backstop}" \
            "post-migration emitter backstop"
    fi
    extend_contract_key_xdr "${prefix}-emitter-last-distro" \
        "${emitter_id}" "${last_distro_key}" >/dev/null
    invoke_forced_transaction "${prefix}-candidate" \
        "${candidate_id}" migration_state >/dev/null
    invoke_forced_transaction "${prefix}-factory" \
        "${factory_id}" is_pool --pool_address "${pool_id}" >/dev/null
    invoke_forced_transaction "${prefix}-pool" \
        "${pool_id}" get_config >/dev/null
    invoke_forced_transaction "${prefix}-position" \
        "${candidate_id}" user_balance \
        --tier BlndUsdc \
        --pool "${pool_id}" \
        --user "${depositor}" >/dev/null
    invoke_forced_transaction "${prefix}-usdc-position" \
        "${candidate_id}" user_balance \
        --tier Usdc \
        --pool "${pool_id}" \
        --user "${depositor}" >/dev/null
    invoke_forced_transaction "${prefix}-pool-data" \
        "${candidate_id}" pool_data \
        --pool "${pool_id}" >/dev/null
    invoke_forced_transaction "${prefix}-reward-zone" \
        "${candidate_id}" reward_zone >/dev/null
    invoke_forced_transaction "${prefix}-pool-reserves" \
        "${pool_id}" get_reserve_list >/dev/null
    invoke_forced_transaction "${prefix}-pool-reserve" \
        "${pool_id}" get_reserve --asset "${primary_pool_supply_asset}" >/dev/null
    invoke_forced_transaction "${prefix}-pool-position" \
        "${pool_id}" get_positions --address "${depositor}" >/dev/null
    invoke_forced_transaction "${prefix}-pool-emissions" \
        "${pool_id}" get_reserve_emissions \
        --reserve_token_index "${primary_pool_emission_reserve_token_id}" >/dev/null
    extend_contract_instance "${prefix}-blnd-usdc-lp-instance" \
        "${blnd_usdc_token_id}" >/dev/null
    invoke_forced_transaction "${prefix}-blnd-usdc-lp-supply" \
        "${blnd_usdc_token_id}" get_total_supply >/dev/null
    invoke_forced_transaction "${prefix}-blnd-usdc-lp-list" \
        "${blnd_usdc_token_id}" get_tokens >/dev/null
    invoke_forced_transaction "${prefix}-blnd-usdc-lp-blnd-reserve" \
        "${blnd_usdc_token_id}" get_balance --token "${blnd_id}" >/dev/null
    invoke_forced_transaction "${prefix}-blnd-usdc-lp-usdc-reserve" \
        "${blnd_usdc_token_id}" get_balance --token "${usdc_token_id}" >/dev/null
    incumbent_balance="$(normalize_scalar "$(invoke_forced_transaction \
        "${prefix}-incumbent-balance" \
        "${blnd_usdc_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_forced_transaction \
        "${prefix}-candidate-balance" \
        "${blnd_usdc_token_id}" balance --id "${candidate_id}")")"
    [[ "${incumbent_balance}" =~ ^[0-9]+$ ]] ||
        die "invalid incumbent BLND:USDC LP balance: ${incumbent_balance}"
    [[ "${candidate_balance}" =~ ^[0-9]+$ ]] ||
        die "invalid candidate BLND:USDC LP balance: ${candidate_balance}"
    if [[ "${emitter_backstop}" != "${candidate_id}" ]]; then
        (( candidate_balance > incumbent_balance )) ||
            die "candidate no longer strictly exceeds incumbent BLND:USDC LP balance"
    fi
    invoke_forced_transaction "${prefix}-pool-token-balance" \
        "${primary_pool_supply_asset}" balance --id "${pool_id}" >/dev/null
    extend_contract_instance "${prefix}-blnd-instance" \
        "${blnd_id}" >/dev/null
    extend_contract_instance "${prefix}-xlm-instance" \
        "${xlm_id}" >/dev/null
    extend_contract_instance "${prefix}-blnd-xlm-lp-instance" \
        "${blnd_xlm_token_id}" >/dev/null
    invoke_forced_transaction "${prefix}-blnd-xlm-lp-supply" \
        "${blnd_xlm_token_id}" get_total_supply >/dev/null
    invoke_forced_transaction "${prefix}-blnd-xlm-lp-list" \
        "${blnd_xlm_token_id}" get_tokens >/dev/null
    invoke_forced_transaction "${prefix}-blnd-xlm-lp-blnd-reserve" \
        "${blnd_xlm_token_id}" get_balance --token "${blnd_id}" >/dev/null
    invoke_forced_transaction "${prefix}-blnd-xlm-lp-xlm-reserve" \
        "${blnd_xlm_token_id}" get_balance --token "${xlm_id}" >/dev/null
    extend_contract_instance "${prefix}-usdc-token-instance" \
        "${usdc_token_id}" >/dev/null
    invoke_forced_transaction "${prefix}-candidate-usdc-balance" \
        "${usdc_token_id}" balance --id "${candidate_id}" >/dev/null
    invoke_forced_transaction "${prefix}-blnd-usdc-comet-blnd-balance" \
        "${blnd_id}" balance --id "${blnd_usdc_token_id}" >/dev/null
    invoke_forced_transaction "${prefix}-next-comet-blnd-balance" \
        "${blnd_id}" balance --id "${blnd_xlm_token_id}" >/dev/null
    invoke_forced_transaction "${prefix}-blnd-usdc-comet-usdc-balance" \
        "${usdc_token_id}" balance --id "${blnd_usdc_token_id}" >/dev/null
    invoke_forced_transaction "${prefix}-next-comet-xlm-balance" \
        "${xlm_id}" balance --id "${blnd_xlm_token_id}" >/dev/null
    blnd_admin="$(normalize_scalar "$(invoke_view \
        "${prefix}-blnd-admin" "${blnd_id}" admin)")"
    assert_equal "${emitter_id}" "${blnd_admin}" "BLND administrator"

    if ! (
        keepalive_modeled_pools \
            "keepalive-modeled-$(date -u +%Y%m%dT%H%M%SZ)"
    ); then
        die "modeled-pool keepalive failed; the migration keepalive is incomplete"
    fi
    state_set "last_keepalive_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "${NETWORK_LABEL} migration TTL keepalive completed. Run this at least every five days; daily is recommended."
}

command_recover() {
    local running

    validate_tools
    require_local_mode
    require_docker
    running="$(
        docker inspect \
            --format '{{.State.Running}}' \
            "${QUICKSTART_CONTAINER_NAME}" 2>/dev/null
    )" || die "Quickstart container ${QUICKSTART_CONTAINER_NAME} does not exist; its ephemeral ledger cannot be recovered"
    [[ "${running}" == "true" ]] ||
        die "Quickstart container ${QUICKSTART_CONTAINER_NAME} is stopped; its ephemeral ledger cannot be recovered"

    note "Restarting stalled Stellar Core inside ${QUICKSTART_CONTAINER_NAME} without replacing its database..."
    docker exec "${QUICKSTART_CONTAINER_NAME}" \
        supervisorctl restart stellar-core
    wait_for_network
    command_status
}

command_prepare() {
    local candidate_id result migration_state migration_status verified_unlock

    validate_tools
    require_network
    load_state
    candidate_id="$(state_value candidate_backstop)"
    result="$(normalize_scalar "$(invoke_transaction \
        "prepare-migration" "${candidate_id}" distribute)")"
    [[ "${result}" =~ ^[0-9]+$ ]] || die "invalid preparation distribution: ${result}"
    migration_state="$(invoke_view \
        "verify-prepared-migration" "${candidate_id}" migration_state)"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    printf '%s' "${migration_status}" | jq -e '
        . == "Prepared" or
        (type == "object" and (has("Prepared") or .tag? == "Prepared"))
    ' >/dev/null || die "candidate did not enter Prepared state: ${migration_status}"
    verified_unlock="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.verified_queue_unlock')")"
    [[ "${verified_unlock}" =~ ^[0-9]+$ ]] ||
        die "invalid verified queue unlock: ${verified_unlock}"
    state_set "phase" "prepared"
    state_set "prepared_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Migration queue prepared at unlock ${verified_unlock}; distributed ${result} backfill base units."
    command_status
}

command_finalize() {
    local emitter_id candidate_id emitter_backstop migration_state migration_status result
    local activated scheduled funded_raw funded

    validate_tools
    require_network
    load_state
    emitter_id="$(state_value emitter)"
    candidate_id="$(state_value candidate_backstop)"
    migration_state="$(invoke_view \
        "query-finalize-candidate-state" "${candidate_id}" migration_state)"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    if printf '%s' "${migration_status}" | jq -e '
        . == "Prepared" or
        (type == "object" and (has("Prepared") or .tag? == "Prepared"))
    ' >/dev/null; then
        emitter_backstop="$(normalize_scalar "$(invoke_view \
            "query-finalize-emitter-backstop" "${emitter_id}" get_backstop)")"
        if [[ "${emitter_backstop}" != "${candidate_id}" ]]; then
            invoke_transaction \
                "swap-emitter-backstop" "${emitter_id}" swap_backstop >/dev/null
        fi
        result="$(normalize_scalar "$(invoke_transaction \
            "synchronize-migration" "${candidate_id}" distribute)")"
        assert_equal "0" "${result}" "first post-swap distribution"
    elif ! printf '%s' "${migration_status}" | jq -e '
        . == "Active" or
        (type == "object" and (has("Active") or .tag? == "Active"))
    ' >/dev/null; then
        die "cannot finalize deployment from candidate migration state ${migration_status}"
    fi

    migration_state="$(invoke_view \
        "read-finalized-migration-state" "${candidate_id}" migration_state)"
    activated="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.activated_at')")"
    [[ "${activated}" =~ ^[0-9]+$ ]] || die "invalid activation timestamp: ${activated}"
    state_set "phase" "active"
    state_set "activated_at" "${activated}"
    scheduled="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.scheduled_backfill')")"
    [[ "${scheduled}" =~ ^[0-9]+$ ]] || die "invalid scheduled backfill: ${scheduled}"
    state_set "scheduled_backfill" "${scheduled}"
    if (( scheduled > 0 )); then
        funded_raw="$(printf '%s' "${migration_state}" | jq -c '.funded_backfill')"
        if printf '%s' "${funded_raw}" | jq -e '. == null' >/dev/null; then
            invoke_transaction \
                "drop-scheduled-backfill" "${candidate_id}" drop >/dev/null
            migration_state="$(invoke_view \
                "read-funded-migration-state" "${candidate_id}" migration_state)"
            funded_raw="$(printf '%s' "${migration_state}" | jq -c '.funded_backfill')"
        fi
        funded="$(normalize_scalar "${funded_raw}")"
        assert_equal "${scheduled}" "${funded}" "funded migration backfill"
        state_set "funded_backfill" "${funded}"
    else
        state_set "funded_backfill" "0"
    fi
    command_verify
}

command_verify() {
    local operator emitter_id incumbent_id candidate_id factory_id pool_id
    local blnd_usdc_token_id blnd_xlm_token_id emitter_backstop
    local candidate_blnd_usdc_token candidate_blnd_xlm_token
    local factory_backstop pool_operator pool_registered pool_status
    local candidate_pool_data incumbent_balance candidate_balance queued_swap
    local migration_state migration_status activated_at saved_phase
    local scheduled_backfill funded_backfill

    validate_tools
    require_network
    load_state
    operator="$(state_value operator)"
    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    factory_id="$(state_value v3_pool_factory)"
    pool_id="$(state_value v3_pool)"
    blnd_usdc_token_id="$(state_value blnd_usdc_token)"
    blnd_xlm_token_id="$(state_value blnd_xlm_token)"
    saved_phase="$(state_value phase)"

    emitter_backstop="$(normalize_scalar "$(invoke_view \
        "verify-emitter-backstop" "${emitter_id}" get_backstop)")"
    candidate_blnd_usdc_token="$(normalize_scalar "$(invoke_view \
        "verify-candidate-blnd-usdc-lp" "${candidate_id}" backstop_token \
        --tier BlndUsdc)")"
    candidate_blnd_xlm_token="$(normalize_scalar "$(invoke_view \
        "verify-candidate-blnd-xlm-lp" "${candidate_id}" backstop_token \
        --tier BlndXlm)")"
    factory_backstop="$(normalize_scalar "$(invoke_view \
        "verify-factory-backstop" "${factory_id}" backstop)")"
    pool_operator="$(normalize_scalar "$(invoke_view \
        "verify-pool-admin" "${pool_id}" get_admin)")"
    pool_status="$(invoke_view \
        "verify-pool-status" "${pool_id}" get_config |
        jq -er '.status | tostring')"
    pool_registered="$(normalize_scalar "$(invoke_view \
        "verify-pool-registration" "${factory_id}" is_pool \
        --pool_address "${pool_id}")")"
    candidate_pool_data="$(invoke_view \
        "verify-candidate-pool-data" "${candidate_id}" pool_data \
        --pool "${pool_id}")"
    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "verify-incumbent-balance" "${blnd_usdc_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "verify-candidate-balance" "${blnd_usdc_token_id}" balance --id "${candidate_id}")")"
    queued_swap="$(invoke_view "verify-emitter-queue" "${emitter_id}" get_queued_swap)"
    migration_state="$(invoke_view \
        "verify-candidate-migration" "${candidate_id}" migration_state)"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    activated_at="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.activated_at')")"
    scheduled_backfill="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.scheduled_backfill')")"

    assert_equal "${candidate_id}" "${emitter_backstop}" "emitter backstop"
    assert_equal "${blnd_usdc_token_id}" "${candidate_blnd_usdc_token}" "candidate BLND:USDC LP token"
    assert_equal "${blnd_xlm_token_id}" "${candidate_blnd_xlm_token}" \
        "candidate BLND:XLM LP token"
    assert_equal "${candidate_id}" "${factory_backstop}" "v3 factory binding"
    assert_equal "${operator}" "${pool_operator}" "v3 pool admin"
    assert_equal "0" "${pool_status}" "active v3 pool status"
    assert_equal "true" "${pool_registered}" "v3 pool registration"
    printf '%s' "${candidate_pool_data}" | jq -e '
        (.blnd_usdc.active_blnd | tonumber) > 0 and
        (.blnd_xlm.active_blnd | tonumber) > 0 and
        (.blnd_usdc.active_value | tonumber) > 0 and
        (.blnd_xlm.active_value | tonumber) > 0 and
        (.usdc.active_value | tonumber) > 0
    ' >/dev/null ||
        die "integrated candidate pool valuation is not positive for every tier: ${candidate_pool_data}"
    [[ "${incumbent_balance}" =~ ^[0-9]+$ ]] ||
        die "invalid post-migration incumbent BLND:USDC LP balance"
    [[ "${candidate_balance}" =~ ^[0-9]+$ ]] ||
        die "invalid post-migration candidate BLND:USDC LP balance"
    # The emitter's successful transition proves the strict BLND:USDC LP
    # comparison passed. That old designated-token majority is intentionally
    # no longer required after migration, when ordinary tier withdrawals may
    # reduce the candidate's BLND:USDC balance.
    printf '%s' "${queued_swap}" | jq -e '. == null' >/dev/null ||
        die "deployed emitter queue was not cleared: ${queued_swap}"
    printf '%s' "${migration_status}" | jq -e '
        . == "Active" or
        (type == "object" and (has("Active") or .tag? == "Active"))
    ' >/dev/null || die "candidate is not active: ${migration_status}"
    [[ "${activated_at}" =~ ^[0-9]+$ ]] || die "candidate activation timestamp is absent"
    [[ "${scheduled_backfill}" =~ ^[0-9]+$ ]] ||
        die "invalid scheduled backfill: ${scheduled_backfill}"
    if (( scheduled_backfill > 0 )); then
        funded_backfill="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
            jq -c '.funded_backfill')")"
        assert_equal "${scheduled_backfill}" "${funded_backfill}" \
            "funded migration backfill"
    else
        funded_backfill="0"
    fi

    case "${saved_phase}" in
        emissions_streaming|emissions_claimed) ;;
        *) state_set "phase" "verified" ;;
    esac
    state_set "verified_primary_pool_data" "${candidate_pool_data}"
    state_set "scheduled_backfill" "${scheduled_backfill}"
    state_set "funded_backfill" "${funded_backfill}"
    state_set "verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted >"${RUN_DIR}/ledger-after-verification.json"

    cat <<EOF
${NETWORK_LABEL} migration verified
  operator: ${operator}
  incumbent SDK-22 backstop: ${incumbent_id}
  deployed emitter: ${emitter_id}
  candidate SDK-27 backstop: ${candidate_id}
  candidate pool factory: ${factory_id}
  candidate pool: ${pool_id}
  candidate pool admin: ${pool_operator}
  candidate pool status: ${pool_status}
  candidate valuation: integrated Comet reserve valuation
  activation timestamp: ${activated_at}
  scheduled and funded backfill: ${scheduled_backfill}
  evidence directory: ${RUN_DIR}
EOF
}

admit_modeled_v3_yieldblox_reward() {
    local candidate_id="$1" yield_pool reward_zone

    yield_pool="$(state_value modeled_v3_yieldblox_pool)"

    reward_zone="$(invoke_view \
        "query-v3-reward-zone-before-yieldblox" \
        "${candidate_id}" reward_zone)"
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${yield_pool}" 'index($pool) != null' >/dev/null
    then
        # Once either backfill or ongoing distribution begins, the contract
        # requires a recent successful checkpoint before this weight mutation.
        invoke_transaction "add-yieldblox-v3-reward" \
            "${candidate_id}" add_reward \
            --to_add "${yield_pool}" \
            --to_remove null >/dev/null
    fi

    reward_zone="$(invoke_view \
        "verify-v3-reward-zone-with-yieldblox" \
        "${candidate_id}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${yield_pool}" 'index($pool) != null' >/dev/null ||
        die "YieldBlox v3 was not admitted to the candidate reward zone"
    state_set "modeled_v3_yieldblox_reward_enabled_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

capture_ongoing_distribution() {
    local label="$1" candidate_id="$2"
    local distributed expected_backstop_tranche expected_pool_tranche

    distributed="$(normalize_scalar "$(invoke_transaction \
        "${label}" "${candidate_id}" distribute)")"

    [[ "${distributed}" =~ ^[0-9]+$ ]] ||
        die "${label} returned an invalid ongoing distribution: ${distributed}"
    # The public v2-shaped ABI returns only the total. The deployed emitter
    # releases whole BLND base-unit amounts, so the immutable 70/30 policy has
    # no split dust in this lane. Tier claims and pool gulps below exercise the
    # resulting obligations through their public entrypoints.
    (( distributed % 10 == 0 )) ||
        die "${label} distribution cannot be split exactly 70/30: ${distributed}"
    expected_backstop_tranche=$((distributed * 7 / 10))
    expected_pool_tranche=$((distributed * 3 / 10))

    jq -cn \
        --arg distributed "${distributed}" \
        --arg expected_backstop_tranche "${expected_backstop_tranche}" \
        --arg expected_pool_tranche "${expected_pool_tranche}" \
        '{
            distributed: ($distributed | tonumber),
            expected_backstop_tranche: ($expected_backstop_tranche | tonumber),
            expected_pool_tranche: ($expected_pool_tranche | tonumber)
        }'
}

read_claimable() {
    local label="$1" candidate_id="$2" user="$3" pool="$4" tier="$5"
    local amount

    amount="$(normalize_scalar "$(invoke_view \
        "${label}" "${candidate_id}" claimable \
        --user "${user}" --pool "${pool}" --tier "${tier}")")"
    [[ "${amount}" =~ ^[0-9]+$ ]] ||
        die "${label} returned invalid claimable BLND: ${amount}"
    printf '%s' "${amount}"
}

command_emissions() {
    local phase candidate_id pool_id depositor distribution received claim_refresh
    local expected_backstop_tranche expected_pool_tranche pool_gulped stream expiration
    local fixed_blnd_usdc_lp fixed_blnd_xlm_lp
    local fixed_blnd_usdc_claimable fixed_blnd_xlm_claimable
    local fixed_blnd_usdc_before fixed_blnd_xlm_before
    local fixed_blnd_usdc_ongoing fixed_blnd_xlm_ongoing
    local emission_reserve_token_id yieldblox_reward_status
    local yield_pool yield_distribution yield_received yield_expected_backstop_tranche
    local yield_expected_pool_tranche yield_blnd_usdc_lp yield_blnd_xlm_lp
    local yield_blnd_usdc_claimable yield_blnd_xlm_claimable
    local yield_blnd_usdc_before yield_blnd_xlm_before
    local yield_blnd_usdc_ongoing yield_blnd_xlm_ongoing
    local yield_pool_gulped yield_stream
    local yield_expiration

    validate_tools
    require_network
    load_state
    refresh_modeled_pool_oracles "emissions"
    phase="$(state_value phase)"
    [[ "${phase}" == "verified" ]] ||
        die "ongoing emissions require a verified migration; saved phase is ${phase}"
    candidate_id="$(state_value candidate_backstop)"
    pool_id="$(state_value v3_pool)"
    yield_pool="$(state_value modeled_v3_yieldblox_pool)"
    depositor="$(state_value depositor)"
    emission_reserve_token_id="$(
        state_value primary_pool_emission_reserve_token_id
    )"
    [[ "${emission_reserve_token_id}" =~ ^[0-9]+$ ]] ||
        die "invalid primary-pool emission reserve token ID"

    fixed_blnd_usdc_before="$(read_claimable \
        "query-fixed-blnd-usdc-before-ongoing" "${candidate_id}" \
        "${depositor}" "${pool_id}" BlndUsdc)"
    fixed_blnd_xlm_before="$(read_claimable \
        "query-fixed-blnd-xlm-before-ongoing" "${candidate_id}" \
        "${depositor}" "${pool_id}" BlndXlm)"
    yield_blnd_usdc_before="$(read_claimable \
        "query-yieldblox-blnd-usdc-before-ongoing" "${candidate_id}" \
        "${depositor}" "${yield_pool}" BlndUsdc)"
    yield_blnd_xlm_before="$(read_claimable \
        "query-yieldblox-blnd-xlm-before-ongoing" "${candidate_id}" \
        "${depositor}" "${yield_pool}" BlndXlm)"

    note "Waiting ${EMISSION_START_DELAY_SECONDS} seconds for a positive emitter interval..."
    sleep "${EMISSION_START_DELAY_SECONDS}"
    distribution="$(capture_ongoing_distribution \
        "distribute-ongoing-blnd" "${candidate_id}")"
    received="$(printf '%s' "${distribution}" | jq -er '.distributed | tostring')"
    expected_backstop_tranche="$(
        printf '%s' "${distribution}" | jq -er '.expected_backstop_tranche | tostring'
    )"
    expected_pool_tranche="$(
        printf '%s' "${distribution}" | jq -er '.expected_pool_tranche | tostring'
    )"
    if [[ ! "${received}" =~ ^[0-9]+$ ]] || (( received <= 0 )); then
        die "ongoing distribution did not receive positive BLND: ${distribution}"
    fi
    if [[ ! "${expected_backstop_tranche}" =~ ^[0-9]+$ ]] ||
        (( expected_backstop_tranche <= 0 ))
    then
        die "ongoing distribution did not produce a positive expected backstop tranche: ${distribution}"
    fi
    if [[ ! "${expected_pool_tranche}" =~ ^[0-9]+$ ]] ||
        (( expected_pool_tranche < SCALAR_7 ))
    then
        die "expected pool tranche is below the one-BLND gulp minimum: ${distribution}"
    fi

    admit_modeled_v3_yieldblox_reward "${candidate_id}"
    yieldblox_reward_status="eligible and allocated after a shared distribution"
    note "Waiting ${MULTI_POOL_EMISSION_DELAY_SECONDS} seconds for a shared two-pool interval..."
    sleep "${MULTI_POOL_EMISSION_DELAY_SECONDS}"
    yield_distribution="$(capture_ongoing_distribution \
        "distribute-ongoing-blnd-to-modeled-pools" \
        "${candidate_id}")"
    yield_received="$(printf '%s' "${yield_distribution}" |
        jq -er '.distributed | tostring')"
    yield_expected_backstop_tranche="$(printf '%s' "${yield_distribution}" |
        jq -er '.expected_backstop_tranche | tostring')"
    yield_expected_pool_tranche="$(printf '%s' "${yield_distribution}" |
        jq -er '.expected_pool_tranche | tostring')"
    if [[ ! "${yield_received}" =~ ^[0-9]+$ ]] ||
        (( yield_received <= 0 ))
    then
        die "shared distribution did not receive positive BLND: ${yield_distribution}"
    fi
    if [[ ! "${yield_expected_backstop_tranche}" =~ ^[0-9]+$ ]] ||
        (( yield_expected_backstop_tranche <= 0 ))
    then
        die "shared distribution did not produce a positive expected backstop tranche: ${yield_distribution}"
    fi
    if [[ ! "${yield_expected_pool_tranche}" =~ ^[0-9]+$ ]] ||
        (( yield_expected_pool_tranche < 2 * SCALAR_7 ))
    then
        die "shared expected pool tranche is too small for two one-BLND gulps: ${yield_distribution}"
    fi
    received=$((received + yield_received))
    expected_backstop_tranche=$((expected_backstop_tranche + yield_expected_backstop_tranche))
    expected_pool_tranche=$((expected_pool_tranche + yield_expected_pool_tranche))

    fixed_blnd_usdc_claimable="$(read_claimable \
        "query-fixed-blnd-usdc-claimable" "${candidate_id}" \
        "${depositor}" "${pool_id}" BlndUsdc)"
    fixed_blnd_usdc_ongoing=$((fixed_blnd_usdc_claimable - fixed_blnd_usdc_before))
    (( fixed_blnd_usdc_ongoing > 0 )) ||
        die "Fixed BLND:USDC claimable BLND did not increase during ongoing distribution"
    fixed_blnd_usdc_lp="$(normalize_scalar "$(invoke_transaction_as \
        "claim-backstop-ongoing-blnd" "${DEPOSITOR_IDENTITY}" \
        "${candidate_id}" claim \
        --tier BlndUsdc \
        --user "${depositor}" \
        --pool "${pool_id}" \
        --min_lp_tokens_out 0)")"
    if [[ ! "${fixed_blnd_usdc_lp}" =~ ^[0-9]+$ ]] ||
        (( fixed_blnd_usdc_lp <= 0 ))
    then
        die "Fixed backstop depositor did not compound positive BLND:USDC LP: ${fixed_blnd_usdc_lp}"
    fi

    note "Waiting ${EMISSION_START_DELAY_SECONDS} seconds to refresh the checkpoint for the next tier claim..."
    sleep "${EMISSION_START_DELAY_SECONDS}"
    claim_refresh="$(capture_ongoing_distribution \
        "refresh-before-fixed-blnd-xlm-claim" "${candidate_id}")"
    received=$((received + $(printf '%s' "${claim_refresh}" | jq -er '.distributed | tonumber')))
    expected_backstop_tranche=$((expected_backstop_tranche + $(printf '%s' "${claim_refresh}" | jq -er '.expected_backstop_tranche | tonumber')))
    expected_pool_tranche=$((expected_pool_tranche + $(printf '%s' "${claim_refresh}" | jq -er '.expected_pool_tranche | tonumber')))
    fixed_blnd_xlm_claimable="$(read_claimable \
        "query-fixed-blnd-xlm-claimable" "${candidate_id}" \
        "${depositor}" "${pool_id}" BlndXlm)"
    fixed_blnd_xlm_ongoing=$((fixed_blnd_xlm_claimable - fixed_blnd_xlm_before))
    (( fixed_blnd_xlm_ongoing > 0 )) ||
        die "Fixed BLND:XLM claimable BLND did not increase during ongoing distribution"
    fixed_blnd_xlm_lp="$(normalize_scalar "$(invoke_transaction_as \
        "claim-fixed-blnd-xlm-ongoing-blnd" "${DEPOSITOR_IDENTITY}" \
        "${candidate_id}" claim \
        --tier BlndXlm \
        --user "${depositor}" \
        --pool "${pool_id}" \
        --min_lp_tokens_out 0)")"
    if [[ ! "${fixed_blnd_xlm_lp}" =~ ^[0-9]+$ ]] ||
        (( fixed_blnd_xlm_lp <= 0 ))
    then
        die "Fixed backstop depositor did not compound positive BLND:XLM LP: ${fixed_blnd_xlm_lp}"
    fi

    note "Waiting ${EMISSION_START_DELAY_SECONDS} seconds to refresh the YieldBlox claim checkpoint..."
    sleep "${EMISSION_START_DELAY_SECONDS}"
    claim_refresh="$(capture_ongoing_distribution \
        "refresh-before-yieldblox-blnd-usdc-claim" "${candidate_id}")"
    received=$((received + $(printf '%s' "${claim_refresh}" | jq -er '.distributed | tonumber')))
    expected_backstop_tranche=$((expected_backstop_tranche + $(printf '%s' "${claim_refresh}" | jq -er '.expected_backstop_tranche | tonumber')))
    expected_pool_tranche=$((expected_pool_tranche + $(printf '%s' "${claim_refresh}" | jq -er '.expected_pool_tranche | tonumber')))
    yield_blnd_usdc_claimable="$(read_claimable \
        "query-yieldblox-blnd-usdc-claimable" "${candidate_id}" \
        "${depositor}" "${yield_pool}" BlndUsdc)"
    yield_blnd_usdc_ongoing=$((yield_blnd_usdc_claimable - yield_blnd_usdc_before))
    (( yield_blnd_usdc_ongoing > 0 )) ||
        die "YieldBlox BLND:USDC claimable BLND did not increase during ongoing distribution"
    yield_blnd_usdc_lp="$(normalize_scalar "$(invoke_transaction_as \
        "claim-yieldblox-backstop-ongoing-blnd" \
        "${DEPOSITOR_IDENTITY}" "${candidate_id}" claim \
        --tier BlndUsdc \
        --user "${depositor}" \
        --pool "${yield_pool}" \
        --min_lp_tokens_out 0)")"
    if [[ ! "${yield_blnd_usdc_lp}" =~ ^[0-9]+$ ]] ||
        (( yield_blnd_usdc_lp <= 0 ))
    then
        die "YieldBlox backstop depositor did not compound positive BLND:USDC LP: ${yield_blnd_usdc_lp}"
    fi

    note "Waiting ${EMISSION_START_DELAY_SECONDS} seconds to refresh the final tier claim checkpoint..."
    sleep "${EMISSION_START_DELAY_SECONDS}"
    claim_refresh="$(capture_ongoing_distribution \
        "refresh-before-yieldblox-blnd-xlm-claim" "${candidate_id}")"
    received=$((received + $(printf '%s' "${claim_refresh}" | jq -er '.distributed | tonumber')))
    expected_backstop_tranche=$((expected_backstop_tranche + $(printf '%s' "${claim_refresh}" | jq -er '.expected_backstop_tranche | tonumber')))
    expected_pool_tranche=$((expected_pool_tranche + $(printf '%s' "${claim_refresh}" | jq -er '.expected_pool_tranche | tonumber')))
    yield_blnd_xlm_claimable="$(read_claimable \
        "query-yieldblox-blnd-xlm-claimable" "${candidate_id}" \
        "${depositor}" "${yield_pool}" BlndXlm)"
    yield_blnd_xlm_ongoing=$((yield_blnd_xlm_claimable - yield_blnd_xlm_before))
    (( yield_blnd_xlm_ongoing > 0 )) ||
        die "YieldBlox BLND:XLM claimable BLND did not increase during ongoing distribution"
    yield_blnd_xlm_lp="$(normalize_scalar "$(invoke_transaction_as \
        "claim-yieldblox-blnd-xlm-ongoing-blnd" \
        "${DEPOSITOR_IDENTITY}" "${candidate_id}" claim \
        --tier BlndXlm \
        --user "${depositor}" \
        --pool "${yield_pool}" \
        --min_lp_tokens_out 0)")"
    if [[ ! "${yield_blnd_xlm_lp}" =~ ^[0-9]+$ ]] ||
        (( yield_blnd_xlm_lp <= 0 ))
    then
        die "YieldBlox backstop depositor did not compound positive BLND:XLM LP: ${yield_blnd_xlm_lp}"
    fi

    pool_gulped="$(normalize_scalar "$(invoke_transaction \
        "gulp-pool-ongoing-blnd" "${pool_id}" gulp_emissions)")"
    if [[ ! "${pool_gulped}" =~ ^[0-9]+$ ]] ||
        (( pool_gulped < SCALAR_7 ))
    then
        die "pool did not gulp at least one BLND: ${pool_gulped}"
    fi
    stream="$(invoke_view \
        "query-pool-emission-stream" "${pool_id}" get_reserve_emissions \
        --reserve_token_index "${emission_reserve_token_id}")"
    expiration="$(printf '%s' "${stream}" | jq -er '.expiration | tostring')"
    [[ "${expiration}" =~ ^[0-9]+$ ]] ||
        die "pool emission stream has no expiration: ${stream}"

    yield_pool_gulped="$(normalize_scalar "$(invoke_transaction \
        "gulp-yieldblox-pool-ongoing-blnd" \
        "${yield_pool}" gulp_emissions)")"
    if [[ ! "${yield_pool_gulped}" =~ ^[0-9]+$ ]] ||
        (( yield_pool_gulped < SCALAR_7 ))
    then
        die "YieldBlox pool did not gulp at least one BLND: ${yield_pool_gulped}"
    fi
    yield_stream="$(invoke_view \
        "query-yieldblox-pool-emission-stream" \
        "${yield_pool}" get_reserve_emissions \
        --reserve_token_index 3)"
    yield_expiration="$(printf '%s' "${yield_stream}" |
        jq -er '.expiration | tostring')"
    [[ "${yield_expiration}" =~ ^[0-9]+$ ]] ||
        die "YieldBlox pool emission stream has no expiration: ${yield_stream}"

    state_set "ongoing_received" "${received}"
    state_set "ongoing_expected_backstop_tranche" "${expected_backstop_tranche}"
    state_set "ongoing_expected_pool_tranche" "${expected_pool_tranche}"
    state_set "ongoing_fixed_blnd_usdc_claimed_blnd" "${fixed_blnd_usdc_claimable}"
    state_set "ongoing_fixed_blnd_xlm_claimed_blnd" "${fixed_blnd_xlm_claimable}"
    state_set "ongoing_yieldblox_blnd_usdc_claimed_blnd" "${yield_blnd_usdc_claimable}"
    state_set "ongoing_yieldblox_blnd_xlm_claimed_blnd" "${yield_blnd_xlm_claimable}"
    state_set "ongoing_fixed_blnd_usdc_claimable_delta" "${fixed_blnd_usdc_ongoing}"
    state_set "ongoing_fixed_blnd_xlm_claimable_delta" "${fixed_blnd_xlm_ongoing}"
    state_set "ongoing_yieldblox_blnd_usdc_claimable_delta" "${yield_blnd_usdc_ongoing}"
    state_set "ongoing_yieldblox_blnd_xlm_claimable_delta" "${yield_blnd_xlm_ongoing}"
    state_set "ongoing_fixed_blnd_usdc_lp_compounded" "${fixed_blnd_usdc_lp}"
    state_set "ongoing_fixed_blnd_xlm_lp_compounded" "${fixed_blnd_xlm_lp}"
    state_set "ongoing_yieldblox_blnd_usdc_lp_compounded" "${yield_blnd_usdc_lp}"
    state_set "ongoing_yieldblox_blnd_xlm_lp_compounded" "${yield_blnd_xlm_lp}"
    state_set "ongoing_fixed_pool_gulped" "${pool_gulped}"
    state_set "ongoing_yieldblox_pool_gulped" "${yield_pool_gulped}"
    state_set "ongoing_pool_gulped" "$((pool_gulped + yield_pool_gulped))"
    state_set "fixed_pool_emission_expiration" "${expiration}"
    state_set "yieldblox_pool_emission_expiration" "${yield_expiration}"
    if (( yield_expiration > expiration )); then
        expiration="${yield_expiration}"
    fi
    state_set "pool_emission_expiration" "${expiration}"
    state_set "phase" "emissions_streaming"
    state_set "emissions_started_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    cat <<EOF
${NETWORK_LABEL} post-migration emissions verified
  emitter BLND received: ${received}
  expected backstop 70% tranche: ${expected_backstop_tranche}
  Fixed BLND:USDC claimed/LP compounded: ${fixed_blnd_usdc_claimable}/${fixed_blnd_usdc_lp}
  Fixed BLND:XLM claimed/LP compounded: ${fixed_blnd_xlm_claimable}/${fixed_blnd_xlm_lp}
  YieldBlox BLND:USDC claimed/LP compounded: ${yield_blnd_usdc_claimable}/${yield_blnd_usdc_lp}
  YieldBlox BLND:XLM claimed/LP compounded: ${yield_blnd_xlm_claimable}/${yield_blnd_xlm_lp}
  expected pool 30% tranche: ${expected_pool_tranche}
  Fixed pool stream reserved: ${pool_gulped}
  YieldBlox pool stream reserved: ${yield_pool_gulped}
  final pool stream expiration: $(format_timestamp "${expiration}") (${expiration})
  YieldBlox v3 reward status: ${yieldblox_reward_status}
  next action: run claim-emissions after the stream expires
EOF
}

claim_pool_emissions_resumable() {
    local label="$1" pool_id="$2" reserve_token_id="$3" result_key="$4"
    local depositor blnd_id expected_key balance_key completed expected
    local balance_before balance_current balance_after balance_delta claimed

    completed="$(state_optional "${result_key}")"
    if [[ -n "${completed}" ]]; then
        [[ "${completed}" =~ ^[0-9]+$ ]] ||
            die "${label} saved claim result is invalid: ${completed}"
        printf '%s\n' "${completed}"
        return
    fi

    depositor="$(state_value depositor)"
    blnd_id="$(state_value blnd_token)"
    expected_key="${result_key}_expected"
    balance_key="${result_key}_balance_before"
    expected="$(state_optional "${expected_key}")"
    balance_before="$(state_optional "${balance_key}")"
    if [[ -z "${expected}" || -z "${balance_before}" ]]; then
        expected="$(normalize_scalar "$(invoke_view_as \
            "preview-${label}-pool-emission-claim" \
            "${DEPOSITOR_IDENTITY}" "${pool_id}" claim \
            --from "${depositor}" \
            --reserve_token_ids "[${reserve_token_id}]" \
            --to "${depositor}")")"
        if [[ ! "${expected}" =~ ^[0-9]+$ ]] || (( expected <= 0 )); then
            die "${label} pool has no positive completed BLND claim: ${expected}"
        fi
        balance_before="$(normalize_scalar "$(invoke_view \
            "query-${label}-claim-balance-before" "${blnd_id}" balance \
            --id "${depositor}")")"
        [[ "${balance_before}" =~ ^[0-9]+$ ]] ||
            die "${label} pre-claim BLND balance is invalid: ${balance_before}"
        state_set "${expected_key}" "${expected}"
        state_set "${balance_key}" "${balance_before}"
    fi
    if [[ ! "${expected}" =~ ^[0-9]+$ ]] || (( expected <= 0 )); then
        die "${label} saved expected claim is invalid: ${expected}"
    fi
    [[ "${balance_before}" =~ ^[0-9]+$ ]] ||
        die "${label} saved pre-claim BLND balance is invalid: ${balance_before}"

    balance_current="$(normalize_scalar "$(invoke_view \
        "query-${label}-claim-balance-current" "${blnd_id}" balance \
        --id "${depositor}")")"
    [[ "${balance_current}" =~ ^[0-9]+$ ]] ||
        die "${label} current BLND balance is invalid: ${balance_current}"
    balance_delta=$((balance_current - balance_before))
    if (( balance_delta == expected )); then
        # The prior transaction committed but the runner stopped before its
        # local completion checkpoint. Reconcile the exact token delta instead
        # of submitting an already-consumed claim again.
        state_set "${result_key}" "${expected}"
        printf '%s\n' "${expected}"
        return
    fi
    (( balance_delta == 0 )) ||
        die "${label} claim has an ambiguous BLND balance delta: ${balance_delta}"

    claimed="$(normalize_scalar "$(invoke_transaction_as \
        "claim-${label}-pool-ongoing-blnd" \
        "${DEPOSITOR_IDENTITY}" "${pool_id}" claim \
        --from "${depositor}" \
        --reserve_token_ids "[${reserve_token_id}]" \
        --to "${depositor}")")"
    assert_equal "${expected}" "${claimed}" "${label} pool claim return"
    balance_after="$(normalize_scalar "$(invoke_view \
        "query-${label}-claim-balance-after" "${blnd_id}" balance \
        --id "${depositor}")")"
    [[ "${balance_after}" =~ ^[0-9]+$ ]] ||
        die "${label} post-claim BLND balance is invalid: ${balance_after}"
    assert_equal "${expected}" "$((balance_after - balance_before))" \
        "${label} pool claim balance delta"
    state_set "${result_key}" "${claimed}"
    printf '%s\n' "${claimed}"
}

command_claim_emissions() {
    local phase pool_id expiration now claimed emission_reserve_token_id
    local yield_pool yield_claimed

    validate_tools
    require_network
    load_state
    phase="$(state_value phase)"
    [[ "${phase}" == "emissions_streaming" ]] ||
        die "pool emissions must be streaming before claim; saved phase is ${phase}"
    pool_id="$(state_value v3_pool)"
    emission_reserve_token_id="$(
        state_value primary_pool_emission_reserve_token_id
    )"
    [[ "${emission_reserve_token_id}" =~ ^[0-9]+$ ]] ||
        die "invalid primary-pool emission reserve token ID"
    expiration="$(state_value pool_emission_expiration)"
    now="$(date +%s)"
    (( now >= expiration )) ||
        die "pool emission stream remains locked for $((expiration - now)) seconds"

    claimed="$(claim_pool_emissions_resumable \
        fixed "${pool_id}" "${emission_reserve_token_id}" \
        ongoing_fixed_pool_claimed)"
    yield_pool="$(state_value modeled_v3_yieldblox_pool)"
    state_value modeled_v3_yieldblox_reward_enabled_at >/dev/null
    yield_claimed="$(claim_pool_emissions_resumable \
        yieldblox "${yield_pool}" 3 ongoing_yieldblox_pool_claimed)"
    state_set "ongoing_fixed_pool_claimed" "${claimed}"
    state_set "ongoing_yieldblox_pool_claimed" "${yield_claimed}"
    state_set "ongoing_pool_claimed" "$((claimed + yield_claimed))"
    state_set "phase" "emissions_claimed"
    state_set "emissions_claimed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    note "${NETWORK_LABEL} seven-day pool emission claims succeeded: Fixed ${claimed}, YieldBlox ${yield_claimed} BLND base units."
}

command_stop() {
    validate_tools
    require_local_mode
    require_docker
    note "Stopping ephemeral container ${CONTAINER_NAME}."
    stellar_cli container stop \
        --docker-host "${DOCKER_HOST_URI}" \
        "${CONTAINER_NAME}"
    note "The ledger is gone; retained files under ${WORK_DIR} are evidence only."
}

command_run() {
    command_validate
    command_start
    command_deploy true
}

main() {
    case "${1:-}" in
        plan) command_plan ;;
        validate) command_validate ;;
        start) command_start ;;
        deploy) command_deploy ;;
        run) command_run ;;
        status) command_status ;;
        keepalive) command_keepalive ;;
        refresh-oracle) command_refresh_oracle ;;
        deploy-modeled-pools) command_deploy_modeled_pools ;;
        verify-modeled-pools) command_verify_modeled_pools ;;
        recover) command_recover ;;
        prepare) command_prepare ;;
        finalize) command_finalize ;;
        verify) command_verify ;;
        emissions) command_emissions ;;
        claim-emissions) command_claim_emissions ;;
        stop) command_stop ;;
        -h|--help|help) usage ;;
        *) usage >&2; exit 2 ;;
    esac
}

main "$@"
