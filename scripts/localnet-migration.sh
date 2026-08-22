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
        COMET_CONTROLLER_IDENTITY="${BLEND_MIGRATION_COMET_CONTROLLER_IDENTITY:-blend-v3-local-comet-controller}"
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
        COMET_CONTROLLER_IDENTITY="${BLEND_MIGRATION_COMET_CONTROLLER_IDENTITY:-blend-v3-production-testnet-comet-controller}"
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
RESERVE_LOSS_ISSUER_IDENTITY="${BLEND_RESERVE_LOSS_ISSUER_IDENTITY:-blend-v3-${NETWORK_MODE}-reserve-loss-issuer}"
RESERVE_LOSS_USER_IDENTITY="${BLEND_RESERVE_LOSS_USER_IDENTITY:-blend-v3-${NETWORK_MODE}-reserve-loss-user}"
CONFIG_DIR="${BLEND_MIGRATION_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${WORK_DIR}/state.json"
CONTAINER_NAME="${BLEND_LOCALNET_CONTAINER:-blend-v3-local}"
QUICKSTART_CONTAINER_NAME="stellar-${CONTAINER_NAME}"

SCALAR_7=10000000
MODELED_BACKSTOP_LP_BALANCE=$((22000 * SCALAR_7))
# Every fresh lane uses modeled Fixed and Custom pools. Fixed v3 holds the
# candidate's complete migration-qualifying BLND:USDC position because Custom
# v3 deliberately uses only XLM and USDC backstop tiers.
INCUMBENT_BALANCE="${MODELED_BACKSTOP_LP_BALANCE}"
AGGREGATE_INCUMBENT_BALANCE=$((2 * MODELED_BACKSTOP_LP_BALANCE))
AGGREGATE_CANDIDATE_BALANCE=$((AGGREGATE_INCUMBENT_BALANCE + 1))
ACTIVATION_USDC_BALANCE=$((12500 * SCALAR_7))
POOL_SUPPLY_BALANCE=$((1000 * SCALAR_7))
BLND_USDC_LP_JOIN_AMOUNT=$((100 * SCALAR_7))
COMET_INITIAL_BLND=$((1000 * SCALAR_7))
COMET_INITIAL_PAIR_TOKEN=$((25 * SCALAR_7))
COMET_INITIAL_XLM=$((100 * SCALAR_7))
COMET_CONTROLLER_THRESHOLD=100
DEPOSITOR_BLND_MINT=$((15000 * SCALAR_7))
DEPOSITOR_USDC_MINT=$((ACTIVATION_USDC_BALANCE + POOL_SUPPLY_BALANCE + 500 * SCALAR_7))
DEPOSITOR_XLM_MINT=$((COMET_INITIAL_PAIR_TOKEN))
FIXTURE_ORACLE_RESOLUTION=300
FIXTURE_ORACLE_RECORDS=7
QUEUE_ATTESTATION_WINDOW_SECONDS=$((7 * 24 * 60 * 60))
EMISSION_START_DELAY_SECONDS=5
# Explicitly renewed deployed-emitter and v2 entries get ten wall-clock days
# of TTL, leaving a two-times safety margin for the documented five-day
# keepalive cadence.
MIGRATION_TTL_EXTENSION_LEDGERS="${BLEND_MIGRATION_TTL_EXTENSION_LEDGERS:-$((10 * 24 * 60 * 60 / LEDGER_SECONDS))}"

INCUMBENT_SALT="0000000000000000000000000000000000000000000000000000000000000011"
CANDIDATE_SALT="0000000000000000000000000000000000000000000000000000000000000022"
MODELED_V2_FIXED_SALT="0000000000000000000000000000000000000000000000000000000000000044"
MODELED_V2_CUSTOM_SALT="0000000000000000000000000000000000000000000000000000000000000055"
MODELED_V3_FIXED_SALT="0000000000000000000000000000000000000000000000000000000000000066"
MODELED_V3_CUSTOM_SALT="0000000000000000000000000000000000000000000000000000000000000088"
RESERVE_LOSS_BACKSTOP_SALT="00000000000000000000000000000000000000000000000000000000000000ad"
RESERVE_LOSS_POOL_SALT="00000000000000000000000000000000000000000000000000000000000000ae"
MODELED_BLND_XLM_LP_BALANCE=$((10 * SCALAR_7))
MODELED_V3_USDC_BALANCE=$((12500 * SCALAR_7))
MODELED_CUSTOM_V3_XLM_BALANCE=$((1000 * SCALAR_7))
MODELED_CUSTOM_V3_USDC_BALANCE=$((12250 * SCALAR_7))
MODELED_V2_USDC_SUPPLY_BALANCE=$((1000 * SCALAR_7))
MODELED_FIXED_V3_USDC_COLLATERAL=$((1000 * SCALAR_7))
MODELED_FIXED_V3_EURC_SUPPLY=$((500 * SCALAR_7))
MODELED_FIXED_V3_EURC_BORROW=$((100 * SCALAR_7))
MODELED_ASSET_BALANCE=$((1000000 * SCALAR_7))
MODELED_POOLS_FIXTURE="${ROOT_DIR}/fixtures/testnet-modeled-pools.json"
RESERVE_LOSS_FIXTURE="${ROOT_DIR}/fixtures/reserve-loss-pool.json"
RESERVE_LOSS_BACKSTOP_USDC=$((12500 * SCALAR_7))
RESERVE_LOSS_BACKSTOP_LP=$((10 * SCALAR_7))
RESERVE_LOSS_SUPPLY=$((1000 * SCALAR_7))
RESERVE_LOSS_HEALTHY_CLAWBACK=$((250 * SCALAR_7))
RESERVE_LOSS_IMPAIRED_CLAWBACK=$((400 * SCALAR_7))

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
            Resume or reverify the mandatory Fixed and Custom pool setup.
  verify-modeled-pools
            Verify modeled pool configs, registration, status, and backstops.
  deploy-reserve-loss-pool
            Deploy and exercise the isolated three-reserve clawback scenario.
  verify-reserve-loss-pool
            Verify healthy, impaired, and exhausted reserve outcomes.
  begin     Begin the migration queue after modeled-pool verification succeeds.
  recover   Restart stalled local Stellar Core without replacing state.
  prepare   Attest the compatible queue during its final seven days.
  finalize  Checkpoint backfill, swap through the emitter, transition, and drop backfill.
  verify    Assert final bindings, balances, and migration state.
  emissions Start the post-migration seven-day 70/30 emission streams.
  claim-emissions
            Claim completed backstop-tier and pool emission streams.
  stop      Stop the named local Quickstart container.

The runner never weakens the production 31-day queue. Call prepare during its
final seven days and finalize from unlock through the following seven-day grace
period. The contract retains v2's distribute/drop migration surface. Generated
configuration, public state, results, and cost logs live below the
network-specific ignored work directory.
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
    for _ in {1..12}; do
        if network_healthy; then
            return
        fi
        sleep 2
    done
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
    local account="$1" now visible_since=""

    for _ in {1..120}; do
        if curl --fail --silent --show-error --max-time 5 \
            "${HORIZON_URL}/accounts/${account}" >/dev/null 2>&1 &&
            stellar_cli ledger entry fetch account \
                --account "${account}" \
                --rpc-url "${RPC_URL}" \
                --network-passphrase "${NETWORK_PASSPHRASE}" \
                --output json 2>/dev/null |
                jq -e '.entries | length == 1' >/dev/null 2>&1
        then
            now="$(date +%s)"
            if [[ -z "${visible_since}" ]]; then
                visible_since="${now}"
            elif (( now - visible_since >= 3 * LEDGER_SECONDS )); then
                return
            fi
        else
            visible_since=""
            curl --fail --silent --show-error --max-time 10 \
                --get --data-urlencode "addr=${account}" \
                "${FRIENDBOT_URL}" >/dev/null 2>&1 || true
        fi
        sleep 1
    done
    die "funded account did not become visible through Horizon: ${account}"
}

horizon_account_sequence() {
    local account="$1" sequence

    for _ in {1..30}; do
        sequence="$(curl --fail --silent --show-error --max-time 5 \
            "${HORIZON_URL}/accounts/${account}" 2>/dev/null | \
            jq -er '.sequence' 2>/dev/null || true)"
        if [[ "${sequence}" =~ ^[0-9]+$ ]]; then
            printf '%s\n' "${sequence}"
            return
        fi
        sleep 1
    done
    die "unable to read ${account} sequence from Horizon"
}

read_horizon_account() {
    local account="$1" response

    for _ in {1..30}; do
        response="$(curl --fail --silent --show-error --max-time 5 \
            "${HORIZON_URL}/accounts/${account}" 2>/dev/null || true)"
        if printf '%s' "${response}" | jq -e \
            --arg account "${account}" '.account_id == $account' >/dev/null 2>&1
        then
            printf '%s\n' "${response}"
            return
        fi
        sleep 1
    done
    die "unable to read ${account} from Horizon"
}

wait_for_testnet_sequence_convergence() {
    local account="$1" previous_sequence="$2"
    local horizon_sequence rpc_sequence confirmations=0

    [[ "${NETWORK_MODE}" == "testnet" ]] || return
    for _ in {1..120}; do
        horizon_sequence="$(horizon_account_sequence "${account}")"
        rpc_sequence="$(stellar_cli ledger entry fetch account \
            --account "${account}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --output json 2>/dev/null | \
            jq -er '.entries[0].val.account.seq_num' 2>/dev/null || true)"
        if [[ "${rpc_sequence}" =~ ^[0-9]+$ &&
            "${rpc_sequence}" == "${horizon_sequence}" ]] &&
            (( rpc_sequence > previous_sequence ))
        then
            confirmations=$((confirmations + 1))
            (( confirmations >= 3 )) && return
        else
            confirmations=0
        fi
        sleep 1
    done
    die "Soroban RPC account sequence did not converge for ${account}"
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
    local source_identity="" source_account="" sequence_before="" read_only=false
    local argument_index
    local -a command
    shift
    command=("$@")
    for ((argument_index = 0; argument_index < ${#command[@]}; argument_index++)); do
        case "${command[argument_index]}" in
            --source-account)
                source_identity="${command[argument_index + 1]:-}"
                ;;
            --send)
                [[ "${command[argument_index + 1]:-}" == "no" ]] && read_only=true
                ;;
        esac
    done
    if [[ "${NETWORK_MODE}" == "testnet" && -n "${source_identity}" &&
        "${read_only}" == "false" ]]
    then
        source_account="$(stellar_cli keys public-key "${source_identity}")"
        sequence_before="$(horizon_account_sequence "${source_account}")"
    fi
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

    CAPTURE_STDOUT_FILE="${stdout_file}"
    CAPTURE_STDERR_FILE="${stderr_file}"

    note "Executing ${label}..."
    if "$@" >"${stdout_file}" 2> >(tee "${stderr_file}" >&2); then
        command_status=0
    else
        command_status=$?
    fi
    if (( command_status == 0 )) && [[ -n "${sequence_before}" ]] &&
        [[ "$(<"${stderr_file}")" == *"Signing transaction:"* ]]
    then
        wait_for_testnet_sequence_convergence \
            "${source_account}" "${sequence_before}"
    fi
    cat "${stdout_file}"
    return "${command_status}"
}

is_transient_rpc_error() {
    local stderr_output="$1"

    [[ "${stderr_output}" == *"client error (Connect)"* ||
        "${stderr_output}" == *"client error (SendRequest)"* ||
        "${stderr_output}" == *"Request timeout"* ||
        "${stderr_output}" == *"connection reset"* ]]
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

# SAC clawback events can trigger a Stellar CLI 27 display panic after a
# successful submission because the native event's asset string is decoded
# against the invoking contract's typed event schema. Quiet mode reduces that
# presentation path; scenario callers additionally confirm custody before
# accepting any nonzero CLI exit.
invoke_transaction_quiet_as() {
    local label="$1" source_identity="$2" contract_id="$3" function_name="$4"
    shift 4
    capture "${label}" \
        stellar_cli --quiet contract invoke \
        --id "${contract_id}" \
        --source-account "${source_identity}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        -- "${function_name}" "$@"
}

invoke_view() {
    local label="$1" contract_id="$2" function_name="$3"
    local attempt status stderr_output
    shift 3
    for attempt in 1 2 3 4 5; do
        if capture "${label}" \
            stellar_cli contract invoke \
            --id "${contract_id}" \
            --source-account "${OPERATOR_IDENTITY}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --send no \
            --cost \
            -- "${function_name}" "$@"
        then
            status=0
        else
            status=$?
        fi
        stderr_output="$(<"${CAPTURE_STDERR_FILE}")"
        if (( status == 0 )) && [[ -s "${CAPTURE_STDOUT_FILE}" ]]; then
            return 0
        fi
        if [[ "${NETWORK_MODE}" != "testnet" || "${attempt}" == "5" ]] ||
            ! is_transient_rpc_error "${stderr_output}"
        then
            (( status != 0 )) || status=1
            return "${status}"
        fi
        note "Retrying ${label} after a transient public-testnet RPC error (${attempt}/5)..."
        sleep 2
    done
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

read_migration_state() {
    local label="$1" candidate_id="$2" instance
    local attempt status stderr_output

    for attempt in 1 2 3 4 5; do
        if capture "${label}" \
            stellar_cli ledger entry fetch contract-data \
            --contract "${candidate_id}" \
            --instance \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --output json >/dev/null
        then
            status=0
        else
            status=$?
        fi
        stderr_output="$(<"${CAPTURE_STDERR_FILE}")"
        if (( status == 0 )) && [[ -s "${CAPTURE_STDOUT_FILE}" ]]; then
            instance="$(<"${CAPTURE_STDOUT_FILE}")"
            break
        fi
        if [[ "${NETWORK_MODE}" != "testnet" || "${attempt}" == "5" ]] ||
            ! is_transient_rpc_error "${stderr_output}"
        then
            (( status != 0 )) || status=1
            return "${status}"
        fi
        note "Retrying ${label} after a transient public-testnet RPC error (${attempt}/5)..."
        sleep 2
    done
    printf '%s' "${instance}" | jq -ce '
        .entries[0].val.contract_data.val.contract_instance.storage as $storage |
        def key_name:
            if .key.symbol? != null then .key.symbol
            elif .key.vec? != null then .key.vec[0].symbol
            else null
            end;
        def stored($name):
            first($storage[] | select(key_name == $name) | .val) // null;
        {
            activated_at: (stored("ActivatedAt") | if . == null then null else .u64 end),
            backfill_end: (stored("BackfillEnd") | if . == null then null else .u64 end),
            blnd_binding_verified:
                (stored("BlndBound") | if . == null then false else .bool end),
            funded_backfill:
                (stored("BFundAmt") | if . == null then null else .i128 end),
            migration_epoch_start:
                (stored("MigrationEpochStart") | if . == null then null else .u64 end),
            scheduled_backfill:
                (stored("ScheduledBackfill") | if . == null then "0" else .i128 end),
            verified_queue_unlock:
                (stored("VerifiedQueueUnlock") | if . == null then null else .u64 end)
        }
        | .status =
            (if .activated_at != null then "Active"
             elif .migration_epoch_start != null then "Open"
             else "Pending"
             end)
    '
}

sac_balance_key_xdr() {
    local holder="$1" key_json

    require_contract_id "${holder}" "SAC contract-balance holder"
    key_json="$(jq -cn --arg holder "${holder}" '
        {vec: [{symbol: "Balance"}, {address: $holder}]}
    ')"
    stellar_cli xdr encode \
        --type ScVal \
        --output single-base64 \
        "${key_json}"
}

read_sac_contract_balance() {
    local label="$1" token="$2" holder="$3"
    local key_xdr attempt status stderr_output

    require_contract_id "${token}" "${label} SAC"
    key_xdr="$(sac_balance_key_xdr "${holder}")"
    for attempt in 1 2 3 4 5; do
        if capture "${label}" \
            stellar_cli ledger entry fetch contract-data \
            --contract "${token}" \
            --key-xdr "${key_xdr}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --output json
        then
            status=0
        else
            status=$?
        fi
        stderr_output="$(<"${CAPTURE_STDERR_FILE}")"
        if (( status == 0 )) && [[ -s "${CAPTURE_STDOUT_FILE}" ]]; then
            return 0
        fi
        if [[ "${NETWORK_MODE}" != "testnet" || "${attempt}" == "5" ]] ||
            ! is_transient_rpc_error "${stderr_output}"
        then
            (( status != 0 )) || status=1
            return "${status}"
        fi
        note "Retrying ${label} after a transient public-testnet RPC error (${attempt}/5)..."
        sleep 2
    done
}

verify_non_clawbackable_sac_balance() {
    local label="$1" token="$2" holder="$3"
    local entry facts amount authorized clawback

    entry="$(read_sac_contract_balance "${label}" "${token}" "${holder}")" ||
        die "${label} contract-balance entry is unavailable"
    facts="$(printf '%s' "${entry}" | jq -ce '
        def field($name):
            ([.[] | select(.key.symbol? == $name) | .val] |
                if length == 1 then .[0]
                else error("missing or duplicate " + $name)
                end);
        if (.entries | length) != 1 then
            error("expected exactly one ledger entry")
        else
            .entries[0].val.contract_data.val.map as $fields |
            {
                amount: ($fields | field("amount") | .i128),
                authorized: ($fields | field("authorized") | .bool),
                clawback: ($fields | field("clawback") | .bool)
            }
        end
    ')" || die "${label} is not a Protocol-27 SAC contract-balance entry"
    amount="$(printf '%s' "${facts}" | jq -er '.amount | tostring')"
    authorized="$(printf '%s' "${facts}" | jq -er '.authorized | tostring')"
    clawback="$(printf '%s' "${facts}" | jq -er '.clawback | tostring')"

    [[ "${amount}" =~ ^[1-9][0-9]*$ ]] ||
        die "${label} must hold a positive balance; found ${amount}"
    [[ "${authorized}" == "true" ]] ||
        die "${label} is currently deauthorized"
    [[ "${clawback}" == "false" ]] ||
        die "${label} is clawbackable; rejecting the deployment"
    note "Verified ${label}: positive, authorized, and non-clawbackable."
}

verify_locked_comet_controller() {
    local label="$1" comet="$2" expected_controller="$3"
    local first_token="$4" second_token="$5"
    local actual_controller tokens first_weight second_weight account

    actual_controller="$(normalize_scalar "$(invoke_view \
        "query-${label}-controller" "${comet}" get_controller)")"
    assert_equal "${expected_controller}" "${actual_controller}" \
        "${label} controller"

    tokens="$(invoke_view "query-${label}-tokens" "${comet}" get_tokens)"
    printf '%s' "${tokens}" | jq -e \
        --arg first "${first_token}" --arg second "${second_token}" \
        '. == [$first, $second]' >/dev/null ||
        die "${label} does not contain the expected canonical token pair"
    first_weight="$(normalize_scalar "$(invoke_view \
        "query-${label}-first-weight" "${comet}" get_normalized_weight \
        --token "${first_token}")")"
    second_weight="$(normalize_scalar "$(invoke_view \
        "query-${label}-second-weight" "${comet}" get_normalized_weight \
        --token "${second_token}")")"
    assert_equal "8000000" "${first_weight}" "${label} BLND weight"
    assert_equal "2000000" "${second_weight}" "${label} pair-token weight"

    account="$(read_horizon_account "${expected_controller}")"
    printf '%s' "${account}" | jq -e \
        --arg controller "${expected_controller}" \
        --argjson threshold "${COMET_CONTROLLER_THRESHOLD}" '
        .thresholds.low_threshold == $threshold and
        .thresholds.med_threshold == $threshold and
        .thresholds.high_threshold == $threshold and
        (.signers | length == 1) and
        .signers[0].key == $controller and
        .signers[0].type == "ed25519_public_key" and
        .signers[0].weight == 0
    ' >/dev/null ||
        die "${label} controller is not irreversibly locked"
    note "Verified ${label}: canonical 80:20 pair with a zero-weight controller."
}

verify_canonical_comet_controller_lock() {
    local controller blnd usdc xlm

    controller="$(state_value comet_controller)"
    blnd="$(state_value blnd_token)"
    usdc="$(state_value usdc_token)"
    xlm="$(state_value xlm_token)"
    verify_locked_comet_controller \
        "blnd-usdc-comet" "$(state_value blnd_usdc_token)" \
        "${controller}" "${blnd}" "${usdc}"
    verify_locked_comet_controller \
        "blnd-xlm-comet" "$(state_value blnd_xlm_token)" \
        "${controller}" "${blnd}" "${xlm}"
    state_set "canonical_comet_controllers_verified_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

verify_fixed_v3_backstop_custody() {
    local candidate blnd_usdc blnd_xlm blnd usdc xlm

    verify_canonical_comet_controller_lock
    candidate="$(state_value candidate_backstop)"
    blnd_usdc="$(state_value blnd_usdc_token)"
    blnd_xlm="$(state_value blnd_xlm_token)"
    blnd="$(state_value blnd_token)"
    usdc="$(state_value usdc_token)"
    xlm="$(state_value xlm_token)"

    verify_non_clawbackable_sac_balance \
        "verify-blnd-usdc-comet-blnd-custody" "${blnd}" "${blnd_usdc}"
    verify_non_clawbackable_sac_balance \
        "verify-blnd-usdc-comet-usdc-custody" "${usdc}" "${blnd_usdc}"
    verify_non_clawbackable_sac_balance \
        "verify-blnd-xlm-comet-blnd-custody" "${blnd}" "${blnd_xlm}"
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        verify_non_clawbackable_sac_balance \
            "verify-blnd-xlm-comet-xlm-custody" "${xlm}" "${blnd_xlm}"
    fi
    verify_non_clawbackable_sac_balance \
        "verify-fixed-v3-backstop-usdc-custody" "${usdc}" "${candidate}"

    state_set "fixed_v3_non_clawbackable_custody_verified_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

verify_all_v3_backstop_custody() {
    local candidate xlm

    verify_fixed_v3_backstop_custody
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        candidate="$(state_value candidate_backstop)"
        xlm="$(state_value xlm_token)"
        verify_non_clawbackable_sac_balance \
            "verify-custom-v3-backstop-xlm-custody" "${xlm}" "${candidate}"
    fi
    state_set "backstop_non_clawbackable_custody_verified_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
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
        v2:custom) printf '%s\n' "${MODELED_V2_CUSTOM_SALT}" ;;
        v3:fixed) printf '%s\n' "${MODELED_V3_FIXED_SALT}" ;;
        v3:custom) printf '%s\n' "${MODELED_V3_CUSTOM_SALT}" ;;
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
        for model in fixed custom; do
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
            printf '%s' "${reserve_list}" | jq -e \
                --arg asset "${asset_id}" 'index($asset) != null' >/dev/null ||
                die "${model} v2 reserve ${label} was not committed"
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

modeled_v3_backstop_config() {
    local model="$1"

    jq -c --arg model "${model}" \
        '.pools[$model].backstop_config' "${MODELED_POOLS_FIXTURE}"
}

modeled_v3_tiers() {
    local model="$1"

    jq -r --arg model "${model}" '
        ["FirstLoss", "SecondLoss", "ThirdLoss"] as $tiers |
        range(0; (.pools[$model].backstop_config | length)) |
        $tiers[.]
    ' "${MODELED_POOLS_FIXTURE}"
}

ensure_modeled_v3_pool() {
    local model="$1" oracle="$2" state_key pool factory salt raw name
    local take_rate max_positions min_collateral reserve_list asset asset_id metadata
    local backstop_config index=0 label status

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
        backstop_config="$(modeled_v3_backstop_config "${model}")"
        raw="$(invoke_transaction "deploy-${model}-v3-pool" "${factory}" deploy \
            --admin "$(state_value operator)" \
            --name "${name}" \
            --salt "${salt}" \
            --oracle "${oracle}" \
            --backstop_take_rate "${take_rate}" \
            --max_positions "${max_positions}" \
            --min_collateral "${min_collateral}" \
            --backstop_config "${backstop_config}")"
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
            printf '%s' "${reserve_list}" | jq -e \
                --arg asset "${asset_id}" 'index($asset) != null' >/dev/null ||
                die "${model} v3 reserve ${label} was not committed"
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
    if jq -e --arg model "${model}" \
        '.pools[$model].backstop_config | any(.asset == "BlndUsdc")' \
        "${MODELED_POOLS_FIXTURE}" >/dev/null
    then
        printf '%s\n' "${MODELED_BACKSTOP_LP_BALANCE}"
    else
        printf '0\n'
    fi
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

ensure_modeled_xlm_balance() {
    local required="$1" balance

    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        ensure_token_balance "$(state_value xlm_token)" \
            "${required}" "modeled-xlm"
        return
    fi
    balance="$(normalize_scalar "$(invoke_view \
        "query-modeled-xlm-available" "$(state_value xlm_token)" balance \
        --id "$(state_value depositor)")")"
    [[ "${balance}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled XLM depositor balance: ${balance}"
    (( balance >= required )) ||
        die "native XLM balance is below the Custom v3 backstop target"
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
        for model in fixed custom; do
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
                while IFS= read -r tier; do
                    invoke_forced_transaction \
                        "${stamp}-${version}-${model}-${tier}-position" \
                        "${candidate}" user_balance \
                        --tier "${tier}" \
                        --pool "${pool}" \
                        --user "${depositor}" >/dev/null
                done < <(modeled_v3_tiers "${model}")
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

    asset_id="$(state_value "$(modeled_asset_state_key EURC)")"
    extend_contract_instance \
        "${stamp}-asset-EURC-instance" "${asset_id}" >/dev/null
    asset_id="$(state_value "$(modeled_asset_state_key PYUSD)")"
    extend_contract_instance \
        "${stamp}-asset-PYUSD-instance" "${asset_id}" >/dev/null
    state_set "modeled_pools_keepalive_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

command_plan() {
    local asset_description valuation_description
    local incumbent_funding_description pool_funding_description

    if [[ "${XLM_ASSET_MODE}" == "native" ]]; then
        asset_description="synthetic BLND and USDC plus Friendbot-funded native XLM"
    else
        asset_description="isolated BLND, USDC, and XLM assets"
    fi
    valuation_description="integrated Comet-derived backstop valuation and bounded test-only lending oracles"
    pool_funding_description="Fixed's three candidate tiers, Custom's 2:1 XLM/USDC tiers, and their lending positions"
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
  1. Create and fund isolated issuer/operator, depositor, and one-way Comet-controller identities.
  2. Provision ${asset_description}; deploy real 80:20 BLND:USDC and BLND:XLM Comets; then lock their shared controller at zero signer weight.
  3. Deploy the exact emitter WASM and a coherent SDK-22 incumbent stack.
  4. Predict the candidate address, bind its immutable factory, and deploy it.
  5. Deploy the modeled Fixed v2/v3 pools with transferable v2-style admins.
  6. Fund Fixed v3 first and reject the deployment unless its shared USDC and canonical-Comet custody entries prove non-clawbackable.
  7. Deploy the modeled Custom v2/v3 pools, fund all remaining tiers with ${pool_funding_description}, activate the pools, admit them to their reward zones, and configure selected reserve-position emissions.
  8. ${incumbent_funding_description}
  9. Assert strict candidate qualification, start backfill through distribute, queue the emitter swap, and observe it through distribute.

Time-gated continuation:
 10. During the final seven days, attest the compatible queue through distribute.
 11. At or after unlock, checkpoint backfill, call the emitter swap, and transition through distribute.
 12. Verify the emitter, candidate, pool, balances, custody flags, and cleared queue.
 13. Distribute ongoing BLND and gulp both backstop-tier and pool tranches.
 14. Claim all completed streams after their seven-day vesting period.

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

command_begin() {
    local phase

    validate_tools
    validate_source_provenance
    require_network
    load_state
    phase="$(state_value phase)"
    if [[ "${phase}" == "open" ]]; then
        note "The migration queue is already open."
        command_status
        return
    fi
    assert_equal "deploying" "${phase}" "pre-migration deployment phase"
    [[ -n "$(state_optional modeled_pools_verified_at)" ]] ||
        die "modeled pools must pass verify-modeled-pools before beginning migration"
    [[ -n "$(state_optional backstop_non_clawbackable_custody_verified_at)" ]] ||
        die "backstop custody must pass non-clawbackable verification before beginning migration"
    [[ -n "$(state_optional canonical_comet_controllers_verified_at)" ]] ||
        die "canonical Comet controllers must be irreversibly locked before beginning migration"
    begin_deployed_migration
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

    migration_state="$(read_migration_state \
        "query-candidate-before-begin" "${candidate_id}")"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    if printf '%s' "${migration_status}" | jq -e '
        . == "Pending" or
        (type == "object" and (has("Pending") or .tag? == "Pending"))
    ' >/dev/null; then
        if printf '%s' "${migration_state}" | jq -e \
            '.migration_epoch_start == null' >/dev/null
        then
            invoke_transaction \
                "start-backfill-clock" "${candidate_id}" distribute >/dev/null
            migration_state="$(read_migration_state \
                "query-started-backfill-state" "${candidate_id}")"
        fi
        epoch_start="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
            jq -c '.migration_epoch_start')")"
    elif printf '%s' "${migration_status}" | jq -e '
        . == "Open" or
        (type == "object" and (has("Open") or .tag? == "Open"))
    ' >/dev/null; then
        epoch_start="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
            jq -c '.migration_epoch_start')")"
    else
        die "cannot begin deployment from candidate migration state ${migration_status}"
    fi
    [[ "${epoch_start}" =~ ^[0-9]+$ ]] ||
        die "invalid migration epoch start: ${epoch_start}"
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
        "$((unlock_time - QUEUE_ATTESTATION_WINDOW_SECONDS))"
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
    local operator depositor comet_controller issuer emitter_id blnd_id xlm_id
    local blnd_usdc_token_id blnd_xlm_token_id
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

    note "Creating and funding one-way ${NETWORK_LABEL} Comet controller ${COMET_CONTROLLER_IDENTITY}..."
    stellar_cli keys generate "${COMET_CONTROLLER_IDENTITY}" \
        --overwrite >/dev/null
    comet_controller="$(stellar_cli keys public-key "${COMET_CONTROLLER_IDENTITY}")"
    fund_and_wait_for_account "${comet_controller}"
    state_set "comet_controller" "${comet_controller}"

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
    capture "create-comet-controller-blnd-trustline" \
        stellar_cli tx new change-trust \
        --source-account "${COMET_CONTROLLER_IDENTITY}" \
        --line "BLND:${issuer}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    capture "create-comet-controller-usdc-trustline" \
        stellar_cli tx new change-trust \
        --source-account "${COMET_CONTROLLER_IDENTITY}" \
        --line "USDC:${issuer}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        capture "create-comet-controller-xlm-trustline" \
            stellar_cli tx new change-trust \
            --source-account "${COMET_CONTROLLER_IDENTITY}" \
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
    invoke_transaction "mint-comet-controller-blnd" "${blnd_id}" mint \
        --to "${comet_controller}" --amount "$((2 * COMET_INITIAL_BLND))" >/dev/null
    invoke_transaction "mint-comet-controller-usdc" "${usdc_token_id}" mint \
        --to "${comet_controller}" --amount "${COMET_INITIAL_PAIR_TOKEN}" >/dev/null
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        invoke_transaction "mint-comet-controller-xlm" "${xlm_id}" mint \
            --to "${comet_controller}" --amount "${COMET_INITIAL_PAIR_TOKEN}" >/dev/null
    fi
    invoke_transaction_as "initialize-comet-blnd-usdc" \
        "${COMET_CONTROLLER_IDENTITY}" "${blnd_usdc_token_id}" init \
        --controller "${comet_controller}" \
        --tokens "[\"${blnd_id}\",\"${usdc_token_id}\"]" \
        --weights '["8000000","2000000"]' \
        --balances "[\"${COMET_INITIAL_BLND}\",\"${COMET_INITIAL_PAIR_TOKEN}\"]" \
        --swap_fee 30000 >/dev/null
    invoke_transaction_as "initialize-comet-blnd-xlm" \
        "${COMET_CONTROLLER_IDENTITY}" "${blnd_xlm_token_id}" init \
        --controller "${comet_controller}" \
        --tokens "[\"${blnd_id}\",\"${xlm_id}\"]" \
        --weights '["8000000","2000000"]' \
        --balances "[\"${COMET_INITIAL_BLND}\",\"${comet_xlm_balance}\"]" \
        --swap_fee 30000 >/dev/null
    capture "lock-canonical-comet-controller" \
        stellar_cli tx new set-options \
        --source-account "${COMET_CONTROLLER_IDENTITY}" \
        --low-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --med-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --high-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --master-weight 0 \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    verify_canonical_comet_controller_lock
    invoke_transaction_as "join-comet-blnd-usdc" \
        "${DEPOSITOR_IDENTITY}" "${blnd_usdc_token_id}" join_pool \
        --pool_amount_out "${BLND_USDC_LP_JOIN_AMOUNT}" \
        --max_amounts_in \
        "[\"$((13000 * SCALAR_7))\",\"$((500 * SCALAR_7))\"]" \
        --user "${depositor}" >/dev/null
    invoke_transaction_as "join-comet-blnd-xlm" \
        "${DEPOSITOR_IDENTITY}" "${blnd_xlm_token_id}" join_pool \
        --pool_amount_out "${MODELED_BLND_XLM_LP_BALANCE}" \
        --max_amounts_in \
        "[\"$((1000 * SCALAR_7))\",\"$((100 * SCALAR_7))\"]" \
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

    # Every lane establishes Fixed first, then resumes the complete Fixed and
    # Custom topology through command_deploy_modeled_pools. The candidate's
    # backstop valuation remains independent from the test-only lending oracles.
    ensure_modeled_asset EURC
    ensure_modeled_asset PYUSD
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
    state_set "modeled_v3_fixed_blnd_usdc_target" \
        "${AGGREGATE_CANDIDATE_BALANCE}"

    ensure_modeled_blnd_usdc_lp_capacity \
        "$((INCUMBENT_BALANCE + AGGREGATE_CANDIDATE_BALANCE))"
    invoke_transaction_as "deposit-candidate-blnd-usdc-lp" \
        "${DEPOSITOR_IDENTITY}" "${candidate_id}" deposit \
        --tier SecondLoss \
        --from "${depositor}" \
        --pool-address "${pool_id}" \
        --amount "${AGGREGATE_CANDIDATE_BALANCE}" >/dev/null
    ensure_modeled_v2_backstop_deposit fixed "${fixed_v2_pool}"
    invoke_transaction "activate-fixed-v2-primary-pool" \
        "${fixed_v2_pool}" set_status --pool_status 0 >/dev/null
    invoke_transaction_as "deposit-candidate-usdc" \
        "${DEPOSITOR_IDENTITY}" "${candidate_id}" deposit \
        --tier ThirdLoss \
        --from "${depositor}" \
        --pool-address "${pool_id}" \
        --amount "${ACTIVATION_USDC_BALANCE}" >/dev/null
    # This first real plain-USDC deposit creates the candidate's one shared
    # SAC balance entry. Verify it and both Comets' underlying custody before
    # any later pool is deployed; a failure invalidates this deployment lane.
    verify_fixed_v3_backstop_custody
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

    # Complete both modeled v2/v3 pool pairs before starting the backfill
    # clock. Fixed already holds the candidate's full migration-qualifying
    # BLND:USDC balance; Custom uses XLM and USDC only. The setup command is
    # idempotent and can resume an interrupted deployment.
    state_set "phase" "deploying"
    command_deploy_modeled_pools
}

command_deploy_modeled_pools() {
    local phase fixed_v2_oracle custom_v2_oracle fixed_v3_oracle custom_v3_oracle
    local fixed_v2_pool custom_v2_pool fixed_v3_pool custom_v3_pool
    local usdc_required fixed_usdc_shares custom_usdc_shares
    local fixed_v2_shares custom_v2_shares fixed_v3_shares
    local fixed_supply custom_supply lp_required fixed_v3_target
    local incumbent_balance candidate_balance pool_status reward_zone stream
    local now refresh_streams=false

    validate_tools
    validate_source_provenance
    require_network
    load_state
    require_modeled_pool_support
    phase="$(state_value phase)"
    [[ "${phase}" == "deploying" || "${phase}" == "open" ]] ||
        die "modeled pre-migration pools require a deploying or open migration; saved phase is ${phase}"

    record_modeled_pools_provenance deployment
    state_set "modeled_pools_fixture_sha256" \
        "$(sha256_file "${MODELED_POOLS_FIXTURE}")"
    state_set "modeled_v3_lending_oracle_wasm_sha256" \
        "$(sha256_file "${V3_TEST_SEP40_ORACLE_WASM}")"

    ensure_modeled_asset EURC
    ensure_modeled_asset PYUSD

    fixed_v2_oracle="$(ensure_modeled_v2_oracle fixed)"
    custom_v2_oracle="$(ensure_modeled_v2_oracle custom)"
    fixed_v3_oracle="$(ensure_modeled_v3_oracle fixed)"
    custom_v3_oracle="$(ensure_modeled_v3_oracle custom)"

    fixed_v2_pool="$(ensure_modeled_v2_pool fixed "${fixed_v2_oracle}")"
    custom_v2_pool="$(ensure_modeled_v2_pool custom "${custom_v2_oracle}")"
    fixed_v3_pool="$(ensure_modeled_v3_pool fixed "${fixed_v3_oracle}")"

    # Preserve the custody gate even when this resumable command is entered
    # before the top-level deploy path reaches its first Fixed deposit. Custom
    # v3 must not be deployed until Fixed has established and verified the
    # candidate's shared plain-USDC balance.
    fixed_usdc_shares="$(modeled_v3_user_shares "${fixed_v3_pool}" ThirdLoss)"
    [[ "${fixed_usdc_shares}" =~ ^[0-9]+$ ]] ||
        die "invalid fixed v3 USDC share state"
    (( fixed_usdc_shares <= MODELED_V3_USDC_BALANCE )) ||
        die "fixed v3 USDC shares exceed the fixture target"
    ensure_token_balance "$(state_value usdc_token)" \
        "$((MODELED_V3_USDC_BALANCE - fixed_usdc_shares))" \
        "fixed-v3-usdc-custody"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" ThirdLoss \
        "${MODELED_V3_USDC_BALANCE}"
    verify_fixed_v3_backstop_custody

    custom_v3_pool="$(ensure_modeled_v3_pool custom "${custom_v3_oracle}")"

    fixed_v2_shares="$(modeled_v2_user_shares "${fixed_v2_pool}")"
    custom_v2_shares="$(modeled_v2_user_shares "${custom_v2_pool}")"
    fixed_v3_shares="$(modeled_v3_user_shares "${fixed_v3_pool}" SecondLoss)"
    fixed_v3_target="$(modeled_v3_blnd_usdc_target fixed)"
    for value in "${fixed_v2_shares}" "${custom_v2_shares}"; do
        [[ "${value}" =~ ^[0-9]+$ ]] ||
            die "invalid modeled BLND:USDC share state"
        (( value <= MODELED_BACKSTOP_LP_BALANCE )) ||
            die "modeled BLND:USDC shares exceed the active fixture target"
    done
    [[ "${fixed_v3_shares}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled v3 BLND:USDC share state"
    (( fixed_v3_shares <= fixed_v3_target )) ||
        die "fixed v3 BLND:USDC shares exceed the active fixture target"
    lp_required=$((
        (MODELED_BACKSTOP_LP_BALANCE - fixed_v2_shares) +
        (MODELED_BACKSTOP_LP_BALANCE - custom_v2_shares) +
        (fixed_v3_target - fixed_v3_shares)
    ))
    ensure_modeled_blnd_usdc_lp_capacity "${lp_required}"

    # Fund Fixed v3 first so the candidate never loses its strict aggregate
    # BLND:USDC LP majority while the two v2 pools are funded.
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" SecondLoss \
        "${fixed_v3_target}"
    ensure_modeled_v2_backstop_deposit fixed "${fixed_v2_pool}"
    ensure_modeled_v2_backstop_deposit custom "${custom_v2_pool}"

    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" FirstLoss \
        "${MODELED_BLND_XLM_LP_BALANCE}"
    ensure_modeled_xlm_balance "${MODELED_CUSTOM_V3_XLM_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        custom "${custom_v3_pool}" FirstLoss \
        "${MODELED_CUSTOM_V3_XLM_BALANCE}"

    fixed_usdc_shares="$(modeled_v3_user_shares "${fixed_v3_pool}" ThirdLoss)"
    custom_usdc_shares="$(modeled_v3_user_shares "${custom_v3_pool}" SecondLoss)"
    [[ "${fixed_usdc_shares}" =~ ^[0-9]+$ &&
        "${custom_usdc_shares}" =~ ^[0-9]+$ ]] ||
        die "invalid modeled v3 USDC share state"
    usdc_required=$((
        (MODELED_V3_USDC_BALANCE - fixed_usdc_shares) +
        (MODELED_CUSTOM_V3_USDC_BALANCE - custom_usdc_shares)
    ))
    (( usdc_required >= 0 )) ||
        die "modeled v3 USDC shares exceed their fixture targets"
    ensure_token_balance "$(state_value usdc_token)" \
        "${usdc_required}" "modeled-usdc"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" ThirdLoss \
        "${MODELED_V3_USDC_BALANCE}"
    verify_fixed_v3_backstop_custody
    ensure_modeled_v3_backstop_deposit \
        custom "${custom_v3_pool}" SecondLoss \
        "${MODELED_CUSTOM_V3_USDC_BALANCE}"
    verify_all_v3_backstop_custody

    for pool in "${fixed_v3_pool}" "${custom_v3_pool}"; do
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

    reward_zone="$(invoke_view "query-modeled-v3-reward-zone" \
        "$(state_value candidate_backstop)" reward_zone)"
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${custom_v3_pool}" 'index($pool) != null' >/dev/null
    then
        invoke_transaction "add-custom-v3-reward" \
            "$(state_value candidate_backstop)" add_reward \
            --to_add "${custom_v3_pool}" --to_remove null >/dev/null
    fi

    for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
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
    ensure_modeled_v3_supply fixed "${fixed_v3_pool}"
    ensure_modeled_v3_supply custom "${custom_v3_pool}"
    ensure_fixed_v3_eurc_borrow "${fixed_v3_pool}"
    state_set "primary_pool_emission_reserve_token_id" "4"
    state_set "modeled_v3_emissions_prepared_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    if [[ -z "$(state_optional modeled_v2_emissions_configured_at)" ]]; then
        for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
            invoke_transaction "configure-modeled-v2-emissions" \
                "${pool}" set_emissions_config \
                --res_emission_metadata \
                '[{"res_index":1,"res_type":1,"share":1}]' >/dev/null
        done
        state_set "modeled_v2_emissions_configured_at" \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fi

    fixed_supply="$(modeled_v2_user_supply "${fixed_v2_pool}")"
    custom_supply="$(modeled_v2_user_supply "${custom_v2_pool}")"
    usdc_required=$((
        (MODELED_V2_USDC_SUPPLY_BALANCE - fixed_supply) +
        (MODELED_V2_USDC_SUPPLY_BALANCE - custom_supply)
    ))
    (( usdc_required >= 0 )) ||
        die "modeled v2 USDC supply exceeds its fixture target"
    ensure_token_balance "$(state_value usdc_token)" \
        "${usdc_required}" "modeled-v2-usdc-supply"
    ensure_modeled_v2_supply fixed "${fixed_v2_pool}"
    ensure_modeled_v2_supply custom "${custom_v2_pool}"

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
        --arg pool "${custom_v2_pool}" 'index($pool) != null' >/dev/null
    then
        advance_modeled_v2_emissions "prime-modeled-v2-reward-zone"
        invoke_transaction "add-custom-v2-reward" \
            "$(state_value incumbent_backstop)" add_reward \
            --to_add "${custom_v2_pool}" --to_remove null >/dev/null
    fi

    now="$(date +%s)"
    for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
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
    for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
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
    local backstop_config expected_backstop_config
    local status v2_shares blnd_usdc_shares blnd_xlm_shares usdc_shares
    local custom_xlm_shares custom_usdc_shares
    local incumbent_balance candidate_balance reward_zone stream supply
    local pool_data_value
    local fixed_v3_target now
    local oracle_admin
    local positions usdc_collateral eurc_supply eurc_borrow

    validate_tools
    require_network
    load_state
    require_modeled_pool_support
    validate_source_provenance
    record_modeled_pools_provenance verification
    verify_all_v3_backstop_custody

    for version in v2 v3; do
        for model in fixed custom; do
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
                backstop_config="$(invoke_view \
                    "verify-${model}-v3-backstop-config" \
                    "${factory}" backstop_config --pool_address "${pool}")"
                expected_backstop_config="$(modeled_v3_backstop_config "${model}")"
                assert_json_equal "${expected_backstop_config}" \
                    "${backstop_config}" "${version} ${model} backstop config"
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
    now="$(date +%s)"
    for model in fixed custom; do
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
        pool_data_value="$(invoke_view \
            "verify-${model}-v3-backstop-value" \
            "$(state_value candidate_backstop)" pool_data --pool "${pool}" |
            jq -er '([.tiers[].value | tonumber] | add) | tostring')"
        [[ "${pool_data_value}" =~ ^[0-9]+$ ]] ||
            die "${model} v3 backstop value is invalid: ${pool_data_value}"
        (( pool_data_value >= ACTIVATION_USDC_BALANCE )) ||
            die "${model} v3 backstop is below the activation threshold: ${pool_data_value}"
        if [[ "${model}" == "fixed" ]]; then
            blnd_usdc_shares="$(modeled_v3_user_shares "${pool}" SecondLoss)"
            blnd_xlm_shares="$(modeled_v3_user_shares "${pool}" FirstLoss)"
            usdc_shares="$(modeled_v3_user_shares "${pool}" ThirdLoss)"
            assert_equal "${fixed_v3_target}" \
                "${blnd_usdc_shares}" "${model} v3 BLND:USDC shares"
            assert_equal "${MODELED_BLND_XLM_LP_BALANCE}" \
                "${blnd_xlm_shares}" "${model} v3 BLND:XLM shares"
            assert_equal "${MODELED_V3_USDC_BALANCE}" \
                "${usdc_shares}" "${model} v3 USDC shares"
        else
            custom_xlm_shares="$(modeled_v3_user_shares "${pool}" FirstLoss)"
            custom_usdc_shares="$(modeled_v3_user_shares "${pool}" SecondLoss)"
            assert_equal "${MODELED_CUSTOM_V3_XLM_BALANCE}" \
                "${custom_xlm_shares}" "${model} v3 XLM shares"
            assert_equal "${MODELED_CUSTOM_V3_USDC_BALANCE}" \
                "${custom_usdc_shares}" "${model} v3 USDC shares"
        fi
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
    pool="$(state_value modeled_v3_fixed_pool)"
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
        die "Fixed v3 is missing from the candidate reward zone"
    pool="$(state_value modeled_v3_custom_pool)"
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
        die "Custom v3 is missing from the candidate reward zone"
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
        "$(state_value modeled_v2_custom_pool)"
    do
        printf '%s' "${reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
            die "modeled v2 pool is missing from the incumbent reward zone: ${pool}"
    done

    state_set "modeled_pools_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    cat <<EOF
${NETWORK_LABEL} modeled pools verified
  v2 Fixed:  $(state_value modeled_v2_fixed_pool) (Admin Active, emitting)
  v2 Custom: $(state_value modeled_v2_custom_pool) (Admin Active, emitting)
  v3 Fixed:  $(state_value modeled_v3_fixed_pool) (active, EURC-borrow emissions prepared)
  v3 Custom: $(state_value modeled_v3_custom_pool) (active, reward-zone member with zero BLND weight)
  incumbent BLND:USDC balance: ${incumbent_balance}
  candidate BLND:USDC balance: ${candidate_balance}
EOF
}

require_reserve_loss_support() {
    [[ -s "${RESERVE_LOSS_FIXTURE}" ]] ||
        die "reserve-loss fixture is missing: ${RESERVE_LOSS_FIXTURE}"
    [[ -s "${V3_TEST_SEP40_ORACLE_WASM}" ]] ||
        die "test lending-oracle WASM is missing: ${V3_TEST_SEP40_ORACLE_WASM}"
}

ensure_reserve_loss_identity() {
    local state_key="$1" identity="$2" label="$3" account saved

    saved="$(state_optional "${state_key}")"
    if [[ -z "${saved}" ]]; then
        note "Creating and funding ${NETWORK_LABEL} ${label} ${identity}..."
        stellar_cli keys generate "${identity}" --overwrite >/dev/null
        account="$(stellar_cli keys public-key "${identity}")"
        fund_and_wait_for_account "${account}"
        state_set "${state_key}" "${account}"
    else
        account="$(stellar_cli keys public-key "${identity}")"
        assert_equal "${saved}" "${account}" "${label} identity"
    fi
    printf '%s\n' "${account}"
}

ensure_reserve_loss_issuer() {
    local issuer account

    issuer="$(ensure_reserve_loss_identity \
        reserve_loss_issuer "${RESERVE_LOSS_ISSUER_IDENTITY}" \
        "reserve-loss asset issuer")"
    account="$(read_horizon_account "${issuer}")"
    if ! printf '%s' "${account}" | jq -e \
        '.flags.auth_clawback_enabled == true' >/dev/null
    then
        capture "enable-reserve-loss-clawback" \
            stellar_cli tx new set-options \
            --source-account "${RESERVE_LOSS_ISSUER_IDENTITY}" \
            --set-revocable \
            --set-clawback-enabled \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null ||
            die "failed to enable revocable and clawback flags on the reserve-loss issuer"
        account="$(read_horizon_account "${issuer}")"
    fi
    printf '%s' "${account}" | jq -e '
        .flags.auth_revocable == true and
        .flags.auth_clawback_enabled == true
    ' >/dev/null || die "reserve-loss issuer is not revocable and clawback-enabled"
    printf '%s\n' "${issuer}"
}

reserve_loss_asset_state_key() {
    local label

    label="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
    printf 'reserve_loss_asset_%s\n' "${label}"
}

reserve_loss_asset_id() {
    state_value "$(reserve_loss_asset_state_key "$1")"
}

ensure_reserve_loss_asset() {
    local code="$1" issuer="$2" user="$3" state_key token label

    label="$(printf '%s' "${code}" | tr '[:upper:]' '[:lower:]')"
    state_key="$(reserve_loss_asset_state_key "${code}")"
    token="$(state_optional "${state_key}")"
    if [[ -z "${token}" ]]; then
        token="$(deploy_asset \
            "deploy-reserve-loss-${label}" "${code}" "${issuer}")"
        state_set "${state_key}" "${token}"
    fi
    require_contract_id "${token}" "reserve-loss ${code}"

    if [[ -z "$(state_optional "${state_key}_user_trustline")" ]]; then
        capture "create-reserve-loss-${label}-trustline" \
            stellar_cli tx new change-trust \
            --source-account "${RESERVE_LOSS_USER_IDENTITY}" \
            --line "${code}:${issuer}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        state_set "${state_key}_user_trustline" "created"
    fi
    printf '%s\n' "${token}"
}

reserve_loss_pool_asset_ids() {
    local result="[]" code token

    while IFS= read -r code; do
        token="$(reserve_loss_asset_id "${code}")"
        result="$(printf '%s' "${result}" | jq -c \
            --arg token "${token}" '. + [{Stellar: $token}]')"
    done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")
    printf '%s\n' "${result}"
}

seed_reserve_loss_oracle() {
    local oracle="$1" code token price prices label

    while IFS= read -r code; do
        label="$(printf '%s' "${code}" | tr '[:upper:]' '[:lower:]')"
        token="$(reserve_loss_asset_id "${code}")"
        price="$(jq -er --arg code "${code}" \
            '.reserves[] | select(.asset == $code) | .price' \
            "${RESERVE_LOSS_FIXTURE}")"
        prices="$(fixture_price_series "${price}")"
        invoke_transaction "seed-reserve-loss-oracle-${label}" \
            "${oracle}" set_prices \
            --asset "{\"Stellar\":\"${token}\"}" \
            --prices "${prices}" >/dev/null
    done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")
    state_set "reserve_loss_oracle_refreshed_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

ensure_reserve_loss_oracle() {
    local oracle assets

    oracle="$(state_optional reserve_loss_oracle)"
    if [[ -z "${oracle}" ]]; then
        assets="$(reserve_loss_pool_asset_ids)"
        oracle="$(deploy_wasm \
            "deploy-reserve-loss-oracle" \
            "${V3_TEST_SEP40_ORACLE_WASM}" "" \
            --admin "$(state_value operator)" \
            --base '{"Other":"USD"}' \
            --assets "${assets}" \
            --decimals 7 \
            --resolution "${FIXTURE_ORACLE_RESOLUTION}")"
        state_set "reserve_loss_oracle" "${oracle}"
    fi
    seed_reserve_loss_oracle "${oracle}"
    printf '%s\n' "${oracle}"
}

ensure_reserve_loss_backstop() {
    local pool_hash backstop factory deployed reward_zone

    pool_hash="$(state_optional reserve_loss_pool_wasm_hash)"
    if [[ -z "${pool_hash}" ]]; then
        pool_hash="$(upload_wasm \
            "upload-reserve-loss-pool" "${V3_POOL_WASM}")"
        state_set "reserve_loss_pool_wasm_hash" "${pool_hash}"
    fi

    backstop="$(state_optional reserve_loss_backstop)"
    if [[ -z "${backstop}" ]]; then
        backstop="$(predict_contract_id "${RESERVE_LOSS_BACKSTOP_SALT}")"
        state_set "reserve_loss_backstop" "${backstop}"
    fi

    factory="$(state_optional reserve_loss_pool_factory)"
    if [[ -z "${factory}" ]]; then
        factory="$(deploy_wasm \
            "deploy-reserve-loss-pool-factory" "${V3_FACTORY_WASM}" "" \
            --pool_init_meta \
            "{\"backstop\":\"${backstop}\",\"blnd_id\":\"$(state_value blnd_token)\",\"pool_hash\":\"${pool_hash}\"}")"
        state_set "reserve_loss_pool_factory" "${factory}"
    fi

    if ! reward_zone="$(invoke_view \
        "probe-reserve-loss-backstop" "${backstop}" reward_zone 2>/dev/null)"
    then
        deployed="$(deploy_wasm \
            "deploy-reserve-loss-backstop" "${V3_BACKSTOP_WASM}" \
            "${RESERVE_LOSS_BACKSTOP_SALT}" \
            --emitter "$(state_value emitter)" \
            --blnd_token "$(state_value blnd_token)" \
            --pool_factory "${factory}" \
            --blnd_usdc_token "$(state_value blnd_usdc_token)" \
            --blnd_xlm_token "$(state_value blnd_xlm_token)" \
            --usdc_token "$(state_value usdc_token)" \
            --xlm_token "$(state_value xlm_token)" \
            --drop_list '[]')"
        assert_equal "${backstop}" "${deployed}" \
            "reserve-loss backstop deterministic address"
    fi
    printf '%s\n' "${backstop}"
}

reserve_loss_reserve_config() {
    local index="$1"

    jq -c --argjson index "${index}" \
        '.reserves[$index].config + {
            decimals: 7,
            enabled: true,
            index: $index
        }' "${RESERVE_LOSS_FIXTURE}"
}

ensure_reserve_loss_pool() {
    local oracle="$1" pool factory raw reserve_list code token metadata label
    local index=0 status

    pool="$(state_optional reserve_loss_pool)"
    if [[ -z "${pool}" ]]; then
        factory="$(state_value reserve_loss_pool_factory)"
        raw="$(invoke_transaction \
            "deploy-reserve-loss-pool" "${factory}" deploy \
            --admin "$(state_value operator)" \
            --name "$(jq -er '.name' "${RESERVE_LOSS_FIXTURE}")" \
            --salt "${RESERVE_LOSS_POOL_SALT}" \
            --oracle "${oracle}" \
            --backstop_take_rate \
            "$(jq -er '.backstop_take_rate' "${RESERVE_LOSS_FIXTURE}")" \
            --max_positions \
            "$(jq -er '.max_positions' "${RESERVE_LOSS_FIXTURE}")" \
            --min_collateral \
            "$(jq -er '.min_collateral' "${RESERVE_LOSS_FIXTURE}")" \
            --backstop_config \
            "$(jq -c '.backstop_config' "${RESERVE_LOSS_FIXTURE}")")"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "reserve-loss pool"
        state_set "reserve_loss_pool" "${pool}"
    fi

    reserve_list="$(invoke_view \
        "query-reserve-loss-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        label="$(printf '%s' "${code}" | tr '[:upper:]' '[:lower:]')"
        token="$(reserve_loss_asset_id "${code}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg token "${token}" 'index($token) != null' >/dev/null
        then
            metadata="$(reserve_loss_reserve_config "${index}")"
            invoke_transaction "queue-reserve-loss-${label}" \
                "${pool}" queue_set_reserve \
                --asset "${token}" --metadata "${metadata}" >/dev/null
            invoke_transaction "set-reserve-loss-${label}" \
                "${pool}" set_reserve --asset "${token}" >/dev/null
            reserve_list="$(invoke_view \
                "query-reserve-loss-reserves-${label}" \
                "${pool}" get_reserve_list)"
        fi
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")

    status="$(invoke_view "query-reserve-loss-status" \
        "${pool}" get_config | jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        invoke_transaction "on-ice-reserve-loss-pool" \
            "${pool}" set_status --pool_status 2 >/dev/null
    fi
    printf '%s\n' "${pool}"
}

reserve_loss_user_shares() {
    local backstop="$1" pool="$2" tier="$3"

    invoke_view "query-reserve-loss-${tier}-shares" \
        "${backstop}" user_balance \
        --tier "${tier}" \
        --pool "${pool}" \
        --user "$(state_value depositor)" |
        jq -er '((.shares | tonumber) + ([.q4w[]?.amount | tonumber] | add // 0)) | tostring'
}

ensure_reserve_loss_backstop_funding() {
    local backstop="$1" pool="$2" current delta

    current="$(reserve_loss_user_shares "${backstop}" "${pool}" FirstLoss)"
    (( current <= RESERVE_LOSS_BACKSTOP_LP )) ||
        die "reserve-loss BLND:USDC shares exceed the fixture target"
    if (( current < RESERVE_LOSS_BACKSTOP_LP )); then
        delta=$((RESERVE_LOSS_BACKSTOP_LP - current))
        ensure_modeled_blnd_usdc_lp_capacity "${delta}"
        invoke_transaction_as "deposit-reserve-loss-blnd-usdc" \
            "${DEPOSITOR_IDENTITY}" "${backstop}" deposit \
            --tier FirstLoss \
            --from "$(state_value depositor)" \
            --pool-address "${pool}" \
            --amount "${delta}" >/dev/null
    fi

    current="$(reserve_loss_user_shares "${backstop}" "${pool}" SecondLoss)"
    (( current <= RESERVE_LOSS_BACKSTOP_USDC )) ||
        die "reserve-loss USDC shares exceed the fixture target"
    if (( current < RESERVE_LOSS_BACKSTOP_USDC )); then
        delta=$((RESERVE_LOSS_BACKSTOP_USDC - current))
        ensure_token_balance "$(state_value usdc_token)" \
            "${delta}" "reserve-loss-backstop-usdc"
        invoke_transaction_as "deposit-reserve-loss-usdc" \
            "${DEPOSITOR_IDENTITY}" "${backstop}" deposit \
            --tier SecondLoss \
            --from "$(state_value depositor)" \
            --pool-address "${pool}" \
            --amount "${delta}" >/dev/null
    fi

    verify_non_clawbackable_sac_balance \
        "reserve-loss backstop USDC" "$(state_value usdc_token)" "${backstop}"
}

ensure_reserve_loss_pool_active() {
    local backstop="$1" pool="$2" status reward_zone

    status="$(invoke_view "query-funded-reserve-loss-status" \
        "${pool}" get_config | jq -er '.status | tostring')"
    if [[ "${status}" != "0" && "${status}" != "1" ]]; then
        invoke_transaction "activate-reserve-loss-pool" \
            "${pool}" set_status --pool_status 0 >/dev/null
    fi

    reward_zone="$(invoke_view \
        "query-reserve-loss-reward-zone" "${backstop}" reward_zone)"
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null
    then
        invoke_transaction "add-reserve-loss-reward" \
            "${backstop}" add_reward \
            --to_add "${pool}" --to_remove null >/dev/null
    fi
}

reserve_loss_position_supply() {
    local pool="$1" user="$2" index="$3"

    invoke_view "query-reserve-loss-position-${index}" \
        "${pool}" get_positions --address "${user}" |
        jq -er --arg index "${index}" '(.supply[$index] // "0") | tostring'
}

ensure_reserve_loss_supply_positions() {
    local pool="$1" issuer="$2" user="$3" position code token balance deficit
    local requests="[]" index=0

    position="$(reserve_loss_position_supply "${pool}" "${user}" 1)"
    if (( position == 0 )); then
        while IFS= read -r code; do
            token="$(reserve_loss_asset_id "${code}")"
            balance="$(normalize_scalar "$(invoke_view_as \
                "query-reserve-loss-user-${code}-balance" \
                "${RESERVE_LOSS_USER_IDENTITY}" "${token}" balance \
                --id "${user}")")"
            if (( balance < RESERVE_LOSS_SUPPLY )); then
                deficit=$((RESERVE_LOSS_SUPPLY - balance))
                invoke_transaction_as "mint-reserve-loss-${code}" \
                    "${RESERVE_LOSS_ISSUER_IDENTITY}" "${token}" mint \
                    --to "${user}" --amount "${deficit}" >/dev/null
            fi
            requests="$(printf '%s' "${requests}" | jq -c \
                --arg token "${token}" \
                --arg amount "${RESERVE_LOSS_SUPPLY}" \
                '. + [{address: $token, amount: $amount, request_type: 0}]')"
            index=$((index + 1))
        done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")
        invoke_transaction_as "seed-reserve-loss-supplies" \
            "${RESERVE_LOSS_USER_IDENTITY}" "${pool}" submit \
            --from "${user}" --spender "${user}" --to "${user}" \
            --requests "${requests}" >/dev/null
    fi
}

reserve_loss_reserve_field() {
    local pool="$1" token="$2" field="$3"

    invoke_view "query-reserve-loss-${field}" \
        "${pool}" get_reserve --asset "${token}" |
        jq -er --arg field "${field}" '.data[$field] | tostring'
}

reserve_loss_token_balance() {
    local token="$1" pool="$2"

    normalize_scalar "$(invoke_view \
        "query-reserve-loss-token-balance" "${token}" balance --id "${pool}")"
}

verify_clawbackable_sac_balance() {
    local label="$1" token="$2" holder="$3" entry clawback authorized

    entry="$(read_sac_contract_balance "${label}" "${token}" "${holder}")" ||
        die "${label} contract-balance entry is unavailable"
    clawback="$(printf '%s' "${entry}" | jq -er '
        .entries[0].val.contract_data.val.map[] |
        select(.key.symbol == "clawback") | .val.bool
    ')"
    authorized="$(printf '%s' "${entry}" | jq -er '
        .entries[0].val.contract_data.val.map[] |
        select(.key.symbol == "authorized") | .val.bool
    ')"
    assert_equal "true" "${clawback}" "${label} clawback flag"
    assert_equal "true" "${authorized}" "${label} authorization"
}

exercise_reserve_loss_pool() {
    local pool="$1" user="$2" healthy impaired exhausted balance b_rate

    healthy="$(reserve_loss_asset_id HLTY)"
    impaired="$(reserve_loss_asset_id IMPR)"
    exhausted="$(reserve_loss_asset_id EXHT)"
    for token in "${healthy}" "${impaired}" "${exhausted}"; do
        verify_clawbackable_sac_balance \
            "reserve-loss pool asset" "${token}" "${pool}"
    done

    balance="$(reserve_loss_token_balance "${healthy}" "${pool}")"
    if (( balance == RESERVE_LOSS_SUPPLY )); then
        if ! invoke_transaction_quiet_as "pool-clawback-reserve-loss-healthy" \
            "${RESERVE_LOSS_ISSUER_IDENTITY}" "${pool}" clawback \
            --asset "${healthy}" --from "${user}" \
            --amount "${RESERVE_LOSS_HEALTHY_CLAWBACK}" >/dev/null
        then
            balance="$(reserve_loss_token_balance "${healthy}" "${pool}")"
            assert_equal \
                "$((RESERVE_LOSS_SUPPLY - RESERVE_LOSS_HEALTHY_CLAWBACK))" \
                "${balance}" \
                "healthy custody after Stellar CLI event-rendering failure"
            note "Accepted the confirmed healthy clawback after the Stellar CLI event-rendering failure."
        fi
    fi

    balance="$(reserve_loss_token_balance "${impaired}" "${pool}")"
    if (( balance == RESERVE_LOSS_SUPPLY )); then
        if ! invoke_transaction_quiet_as "sac-clawback-reserve-loss-impaired" \
            "${RESERVE_LOSS_ISSUER_IDENTITY}" "${impaired}" clawback \
            --from "${pool}" \
            --amount "${RESERVE_LOSS_IMPAIRED_CLAWBACK}" >/dev/null
        then
            balance="$(reserve_loss_token_balance "${impaired}" "${pool}")"
            assert_equal \
                "$((RESERVE_LOSS_SUPPLY - RESERVE_LOSS_IMPAIRED_CLAWBACK))" \
                "${balance}" \
                "impaired custody after Stellar CLI event-rendering failure"
            note "Accepted the confirmed impaired SAC clawback after the Stellar CLI event-rendering failure."
        fi
    fi
    b_rate="$(reserve_loss_reserve_field "${pool}" "${impaired}" b_rate)"
    if (( b_rate == 1000000000000 )); then
        invoke_transaction "reconcile-reserve-loss-impaired" \
            "${pool}" reconcile_loss --asset "${impaired}" >/dev/null
    fi

    balance="$(reserve_loss_token_balance "${exhausted}" "${pool}")"
    if (( balance == RESERVE_LOSS_SUPPLY )); then
        if ! invoke_transaction_quiet_as "sac-clawback-reserve-loss-exhausted" \
            "${RESERVE_LOSS_ISSUER_IDENTITY}" "${exhausted}" clawback \
            --from "${pool}" --amount "${RESERVE_LOSS_SUPPLY}" >/dev/null
        then
            balance="$(reserve_loss_token_balance "${exhausted}" "${pool}")"
            assert_equal "0" "${balance}" \
                "exhausted custody after Stellar CLI event-rendering failure"
            note "Accepted the confirmed exhausted SAC clawback after the Stellar CLI event-rendering failure."
        fi
    fi
    b_rate="$(reserve_loss_reserve_field "${pool}" "${exhausted}" b_rate)"
    if (( b_rate != 0 )); then
        invoke_transaction "reconcile-reserve-loss-exhausted" \
            "${pool}" reconcile_loss --asset "${exhausted}" >/dev/null
    fi
    state_set "reserve_loss_exercised_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

command_deploy_reserve_loss_pool() {
    local issuer user code oracle backstop pool

    validate_tools
    require_network
    load_state
    require_reserve_loss_support
    validate_source_provenance
    build_artifacts
    validate_artifacts

    issuer="$(ensure_reserve_loss_issuer)"
    user="$(ensure_reserve_loss_identity \
        reserve_loss_user "${RESERVE_LOSS_USER_IDENTITY}" \
        "reserve-loss supplier")"
    while IFS= read -r code; do
        ensure_reserve_loss_asset "${code}" "${issuer}" "${user}" >/dev/null
    done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")
    oracle="$(ensure_reserve_loss_oracle)"
    backstop="$(ensure_reserve_loss_backstop)"
    pool="$(ensure_reserve_loss_pool "${oracle}")"
    ensure_reserve_loss_backstop_funding "${backstop}" "${pool}"
    ensure_reserve_loss_pool_active "${backstop}" "${pool}"
    ensure_reserve_loss_supply_positions "${pool}" "${issuer}" "${user}"
    exercise_reserve_loss_pool "${pool}" "${user}"
    state_set "reserve_loss_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    command_verify_reserve_loss_pool
}

command_verify_reserve_loss_pool() {
    local issuer user oracle backstop factory pool registered status reward_zone
    local active_value expected_reserves="[]" actual_reserves code token index=0
    local expected_config actual_config b_rate b_supply backstop_credit balance supply

    validate_tools
    require_network
    load_state
    require_reserve_loss_support

    issuer="$(state_value reserve_loss_issuer)"
    user="$(state_value reserve_loss_user)"
    oracle="$(state_value reserve_loss_oracle)"
    backstop="$(state_value reserve_loss_backstop)"
    factory="$(state_value reserve_loss_pool_factory)"
    pool="$(state_value reserve_loss_pool)"

    registered="$(normalize_scalar "$(invoke_view \
        "verify-reserve-loss-registration" "${factory}" is_pool \
        --pool_address "${pool}")")"
    assert_equal "true" "${registered}" "reserve-loss factory registration"
    status="$(invoke_view "verify-reserve-loss-status" \
        "${pool}" get_config | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "reserve-loss pool is not active: status ${status}"
    active_value="$(invoke_view "verify-reserve-loss-backstop-value" \
        "${backstop}" pool_data --pool "${pool}" |
        jq -er '.active_value | tostring')"
    (( active_value >= ACTIVATION_USDC_BALANCE )) ||
        die "reserve-loss pool is below the activation threshold: ${active_value}"
    reward_zone="$(invoke_view "verify-reserve-loss-reward-zone" \
        "${backstop}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
        die "reserve-loss pool is missing from its reward zone"

    actual_reserves="$(invoke_view "verify-reserve-loss-reserve-list" \
        "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(reserve_loss_asset_id "${code}")"
        expected_reserves="$(printf '%s' "${expected_reserves}" | jq -c \
            --arg token "${token}" '. + [$token]')"
        expected_config="$(reserve_loss_reserve_config "${index}")"
        actual_config="$(invoke_view "verify-reserve-loss-${code}-config" \
            "${pool}" get_reserve --asset "${token}" | jq -c '.config')"
        assert_json_equal "${expected_config}" "${actual_config}" \
            "reserve-loss ${code} config"
        verify_clawbackable_sac_balance \
            "reserve-loss ${code}" "${token}" "${pool}"
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")
    assert_json_equal "${expected_reserves}" "${actual_reserves}" \
        "reserve-loss reserve list"

    token="$(reserve_loss_asset_id HLTY)"
    b_rate="$(reserve_loss_reserve_field "${pool}" "${token}" b_rate)"
    b_supply="$(reserve_loss_reserve_field "${pool}" "${token}" b_supply)"
    backstop_credit="$(reserve_loss_reserve_field "${pool}" "${token}" backstop_credit)"
    balance="$(reserve_loss_token_balance "${token}" "${pool}")"
    supply="$(reserve_loss_position_supply "${pool}" "${user}" 0)"
    assert_equal "1000000000000" "${b_rate}" "healthy reserve b_rate"
    assert_equal "$((RESERVE_LOSS_SUPPLY - RESERVE_LOSS_HEALTHY_CLAWBACK))" \
        "${b_supply}" "healthy reserve b_supply"
    assert_equal "${b_supply}" "${supply}" "healthy user supply"
    assert_equal "${b_supply}" "${balance}" "healthy reserve custody"
    assert_equal "0" "${backstop_credit}" "healthy reserve backstop credit"

    token="$(reserve_loss_asset_id IMPR)"
    b_rate="$(reserve_loss_reserve_field "${pool}" "${token}" b_rate)"
    b_supply="$(reserve_loss_reserve_field "${pool}" "${token}" b_supply)"
    backstop_credit="$(reserve_loss_reserve_field "${pool}" "${token}" backstop_credit)"
    balance="$(reserve_loss_token_balance "${token}" "${pool}")"
    supply="$(reserve_loss_position_supply "${pool}" "${user}" 1)"
    assert_equal "600000000000" "${b_rate}" "impaired reserve b_rate"
    assert_equal "${RESERVE_LOSS_SUPPLY}" "${b_supply}" "impaired reserve b_supply"
    assert_equal "${RESERVE_LOSS_SUPPLY}" "${supply}" "impaired user supply"
    assert_equal "$((RESERVE_LOSS_SUPPLY - RESERVE_LOSS_IMPAIRED_CLAWBACK))" \
        "${balance}" "impaired reserve custody"
    assert_equal "0" "${backstop_credit}" "impaired reserve backstop credit"

    token="$(reserve_loss_asset_id EXHT)"
    b_rate="$(reserve_loss_reserve_field "${pool}" "${token}" b_rate)"
    b_supply="$(reserve_loss_reserve_field "${pool}" "${token}" b_supply)"
    backstop_credit="$(reserve_loss_reserve_field "${pool}" "${token}" backstop_credit)"
    balance="$(reserve_loss_token_balance "${token}" "${pool}")"
    supply="$(reserve_loss_position_supply "${pool}" "${user}" 2)"
    assert_equal "0" "${b_rate}" "exhausted reserve b_rate"
    assert_equal "${RESERVE_LOSS_SUPPLY}" "${b_supply}" "exhausted reserve b_supply"
    assert_equal "${RESERVE_LOSS_SUPPLY}" "${supply}" "exhausted user supply"
    assert_equal "0" "${balance}" "exhausted reserve custody"
    assert_equal "0" "${backstop_credit}" "exhausted reserve backstop credit"

    for code in HLTY IMPR EXHT; do
        token="$(reserve_loss_asset_id "${code}")"
        assert_equal "0" "$(normalize_scalar "$(invoke_view \
            "verify-reserve-loss-${code}-reconciled" \
            "${pool}" reconcile_loss --asset "${token}")")" \
            "reserve-loss ${code} residual deficit"
    done

    state_set "reserve_loss_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    cat <<EOF
${NETWORK_LABEL} reserve-loss pool verified
  pool:      ${pool}
  backstop:  ${backstop} (reward-zone member; active value ${active_value})
  issuer:    ${issuer}
  supplier:  ${user}
  HLTY:      healthy after pool.clawback (b_rate 1.0, 750 supplied)
  IMPR:      recoverably impaired after SAC clawback + reconcile_loss (b_rate 0.6)
  EXHT:      fully exhausted after SAC clawback + reconcile_loss (b_rate 0)
EOF
}

command_status() {
    local emitter_id incumbent_id candidate_id pool_id blnd_xlm_token_id blnd_usdc_token_id
    local unlock_time prepare_not_before activation_deadline phase
    local verified_queue_unlock verified_unlock_raw
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
    activation_deadline=$((unlock_time + QUEUE_ATTESTATION_WINDOW_SECONDS))
    phase="$(state_value phase)"

    emitter_backstop="$(normalize_scalar "$(invoke_view \
        "status-emitter-backstop" "${emitter_id}" get_backstop)")"
    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "status-incumbent-balance" "${blnd_usdc_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "status-candidate-balance" "${blnd_usdc_token_id}" balance --id "${candidate_id}")")"
    queued_swap="$(invoke_view "status-emitter-queue" "${emitter_id}" get_queued_swap)"
    migration_state="$(read_migration_state \
        "status-candidate-migration" "${candidate_id}")"
    verified_unlock_raw="$(printf '%s' "${migration_state}" | \
        jq -c '.verified_queue_unlock')"
    if printf '%s' "${verified_unlock_raw}" | jq -e '. == null' >/dev/null; then
        verified_queue_unlock=""
    else
        verified_queue_unlock="$(normalize_scalar "${verified_unlock_raw}")"
    fi
    pool_operator="$(normalize_scalar "$(invoke_view \
        "status-pool-admin" "${pool_id}" get_admin)")"
    pool_status="$(invoke_view \
        "status-pool-status" "${pool_id}" get_config |
        jq -er '.status | tostring')"
    now="$(date +%s)"

    if [[ "${queued_swap}" != "null" ]]; then
        assert_queued_swap_matches \
            "${queued_swap}" "${candidate_id}" "${blnd_xlm_token_id}" "${unlock_time}"
    elif [[ "${emitter_backstop}" != "${candidate_id}" &&
        "${phase}" != "active" &&
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
  queue attestation: ${verified_queue_unlock:-none}
  attest not before: $(format_timestamp "${prepare_not_before}") (${prepare_not_before})
  unlock: $(format_timestamp "${unlock_time}") (${unlock_time})
  activation deadline: $(format_timestamp "${activation_deadline}") (${activation_deadline})
EOF

    if [[ "${queued_swap}" == "null" ]]; then
        case "${phase}" in
            deploying|open|prepared)
                if (( now > activation_deadline )); then
                    note "Emitter replacement completed, but the candidate activation grace period has expired."
                else
                    note "Emitter replacement completed externally. Next action: run finalize to transition and fund backfill."
                fi
                ;;
            active)
                note "Emitter swap completed. Next action: run verify."
                ;;
            verified)
                note "Migration verified. Next action: run emissions."
                ;;
            emissions_streaming)
                expiration="$(state_value pool_emission_expiration)"
                if (( now < expiration )); then
                    note "Backstop and pool emissions are streaming. Claims open in $((expiration - now)) seconds."
                else
                    note "Emission streams have expired. Next action: run claim-emissions."
                fi
                ;;
            emissions_claimed)
                note "Migration and the ongoing-emission exercise are complete."
                ;;
        esac
    elif (( now < prepare_not_before )); then
        note "Queue attestation opens in $((prepare_not_before - now)) seconds."
    elif (( now > activation_deadline )); then
        note "Queue attestation and candidate activation grace period expired."
    elif [[ -z "${verified_queue_unlock}" ]]; then
        note "Queue attestation is open. Next action: run prepare."
    elif (( now < unlock_time )); then
        note "Queue attested. The emitter swap opens in $((unlock_time - now)) seconds."
    else
        note "Queue is unlocked. Run finalize to checkpoint, swap, transition, and drop backfill."
    fi
}

command_refresh_oracle() {
    local stamp reserve_loss_oracle

    validate_tools
    require_network
    load_state
    require_modeled_pool_support
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    refresh_modeled_pool_oracles "refresh-lending-oracles-${stamp}"
    reserve_loss_oracle="$(state_optional reserve_loss_oracle)"
    if [[ -n "${reserve_loss_oracle}" ]]; then
        require_reserve_loss_support
        seed_reserve_loss_oracle "${reserve_loss_oracle}"
        note "Refreshed the reserve-loss lending oracle."
    fi
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
    extend_contract_instance "${prefix}-candidate-instance" \
        "${candidate_id}" >/dev/null
    invoke_forced_transaction "${prefix}-factory" \
        "${factory_id}" is_pool --pool_address "${pool_id}" >/dev/null
    invoke_forced_transaction "${prefix}-pool" \
        "${pool_id}" get_config >/dev/null
    invoke_forced_transaction "${prefix}-position" \
        "${candidate_id}" user_balance \
        --tier SecondLoss \
        --pool "${pool_id}" \
        --user "${depositor}" >/dev/null
    invoke_forced_transaction "${prefix}-usdc-position" \
        "${candidate_id}" user_balance \
        --tier ThirdLoss \
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
    local candidate_id unlock_time result migration_state verified_unlock

    validate_tools
    require_network
    load_state
    candidate_id="$(state_value candidate_backstop)"
    unlock_time="$(state_value unlock_time)"
    result="$(normalize_scalar "$(invoke_transaction \
        "attest-migration-queue" "${candidate_id}" distribute)")"
    [[ "${result}" =~ ^[0-9]+$ ]] ||
        die "invalid attestation distribution: ${result}"
    migration_state="$(read_migration_state \
        "verify-attested-migration" "${candidate_id}")"
    verified_unlock="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.verified_queue_unlock')")"
    assert_equal "${unlock_time}" "${verified_unlock}" "attested queue unlock"
    state_set "phase" "prepared"
    state_set "prepared_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Migration queue attested at unlock ${verified_unlock}; distributed ${result} backfill base units."
    command_status
}

command_finalize() {
    local emitter_id candidate_id emitter_backstop migration_state migration_status result
    local unlock_time activation_deadline now verified_raw verified_unlock
    local activated scheduled funded_raw funded

    validate_tools
    require_network
    load_state
    emitter_id="$(state_value emitter)"
    candidate_id="$(state_value candidate_backstop)"
    unlock_time="$(state_value unlock_time)"
    activation_deadline=$((unlock_time + QUEUE_ATTESTATION_WINDOW_SECONDS))
    migration_state="$(read_migration_state \
        "query-finalize-candidate-state" "${candidate_id}")"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    if printf '%s' "${migration_status}" | jq -e '
        . == "Pending" or . == "Open" or
        (type == "object" and
            (has("Pending") or has("Open") or .tag? == "Pending" or .tag? == "Open"))
    ' >/dev/null; then
        emitter_backstop="$(normalize_scalar "$(invoke_view \
            "query-finalize-emitter-backstop" "${emitter_id}" get_backstop)")"
        if [[ "${emitter_backstop}" != "${candidate_id}" ]]; then
            result="$(normalize_scalar "$(invoke_transaction \
                "checkpoint-before-emitter-swap" "${candidate_id}" distribute)")"
            [[ "${result}" =~ ^[0-9]+$ ]] ||
                die "invalid pre-swap backfill distribution: ${result}"
            migration_state="$(read_migration_state \
                "verify-finalize-queue-attestation" "${candidate_id}")"
            verified_raw="$(printf '%s' "${migration_state}" | \
                jq -c '.verified_queue_unlock')"
            verified_unlock="$(normalize_scalar "${verified_raw}")"
            assert_equal "${unlock_time}" "${verified_unlock}" \
                "finalize queue attestation"
            now="$(date +%s)"
            (( now <= activation_deadline )) ||
                die "candidate activation grace period expired at ${activation_deadline}"
            invoke_transaction \
                "swap-emitter-backstop" "${emitter_id}" swap_backstop >/dev/null
        fi
        result="$(normalize_scalar "$(invoke_transaction \
            "transition-candidate-emissions" "${candidate_id}" distribute)")"
        assert_equal "0" "${result}" "first post-swap distribution"
    elif ! printf '%s' "${migration_status}" | jq -e '
        . == "Active" or
        (type == "object" and (has("Active") or .tag? == "Active"))
    ' >/dev/null; then
        die "cannot finalize deployment from candidate migration state ${migration_status}"
    fi

    migration_state="$(read_migration_state \
        "read-finalized-migration-state" "${candidate_id}")"
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
            migration_state="$(read_migration_state \
                "read-funded-migration-state" "${candidate_id}")"
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
    local pool_operator pool_registered pool_status
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
        --tier SecondLoss --pool "${pool_id}")")"
    candidate_blnd_xlm_token="$(normalize_scalar "$(invoke_view \
        "verify-candidate-blnd-xlm-lp" "${candidate_id}" backstop_token \
        --tier FirstLoss --pool "${pool_id}")")"
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
    migration_state="$(read_migration_state \
        "verify-candidate-migration" "${candidate_id}")"
    migration_status="$(printf '%s' "${migration_state}" | jq -c '.status')"
    activated_at="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.activated_at')")"
    scheduled_backfill="$(normalize_scalar "$(printf '%s' "${migration_state}" | \
        jq -c '.scheduled_backfill')")"

    verify_all_v3_backstop_custody

    assert_equal "${candidate_id}" "${emitter_backstop}" "emitter backstop"
    assert_equal "${blnd_usdc_token_id}" "${candidate_blnd_usdc_token}" "candidate BLND:USDC LP token"
    assert_equal "${blnd_xlm_token_id}" "${candidate_blnd_xlm_token}" \
        "candidate BLND:XLM LP token"
    assert_equal "${operator}" "${pool_operator}" "v3 pool admin"
    assert_equal "0" "${pool_status}" "active v3 pool status"
    assert_equal "true" "${pool_registered}" "v3 pool registration"
    printf '%s' "${candidate_pool_data}" | jq -e '
        (.active_value | tonumber) > 0 and
        (.tiers | length) == 3 and
        ([.tiers[].value | tonumber] | all(. > 0))
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

read_token_balance() {
    local label="$1" token_id="$2" account_id="$3" balance

    balance="$(normalize_scalar "$(invoke_view \
        "${label}" "${token_id}" balance --id "${account_id}")")"
    [[ "${balance}" =~ ^[0-9]+$ ]] ||
        die "${label} returned invalid token balance: ${balance}"
    printf '%s' "${balance}"
}

read_pool_tier_assets() {
    local label="$1" candidate_id="$2" pool="$3" tier="$4"
    local index assets

    case "${tier}" in
        FirstLoss) index=0 ;;
        SecondLoss) index=1 ;;
        *) die "${label} cannot read non-BLND emission tier ${tier}" ;;
    esac
    assets="$(invoke_view "${label}" "${candidate_id}" pool_data \
        --pool "${pool}" | jq -er --argjson index "${index}" \
        '.tiers[$index].tokens | tostring')"
    [[ "${assets}" =~ ^[0-9]+$ ]] ||
        die "${label} returned invalid tier assets: ${assets}"
    printf '%s' "${assets}"
}

read_user_tier_shares() {
    local label="$1" candidate_id="$2" pool="$3" tier="$4" user="$5" shares

    shares="$(invoke_view "${label}" "${candidate_id}" user_balance \
        --tier "${tier}" --pool "${pool}" --user "${user}" | \
        jq -er '.shares | tostring')"
    [[ "${shares}" =~ ^[0-9]+$ ]] ||
        die "${label} returned invalid user tier shares: ${shares}"
    printf '%s' "${shares}"
}

command_emissions() {
    local phase candidate_id pool_id distribution received
    local expected_backstop_tranche expected_pool_tranche pool_gulped stream expiration
    local emission_reserve_token_id

    validate_tools
    require_network
    load_state
    refresh_modeled_pool_oracles "emissions"
    phase="$(state_value phase)"
    [[ "${phase}" == "verified" ]] ||
        die "ongoing emissions require a verified migration; saved phase is ${phase}"
    candidate_id="$(state_value candidate_backstop)"
    pool_id="$(state_value v3_pool)"
    emission_reserve_token_id="$(
        state_value primary_pool_emission_reserve_token_id
    )"
    [[ "${emission_reserve_token_id}" =~ ^[0-9]+$ ]] ||
        die "invalid primary-pool emission reserve token ID"

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

    state_set "ongoing_received" "${received}"
    state_set "ongoing_expected_backstop_tranche" "${expected_backstop_tranche}"
    state_set "ongoing_expected_pool_tranche" "${expected_pool_tranche}"
    state_set "ongoing_fixed_pool_gulped" "${pool_gulped}"
    state_set "ongoing_pool_gulped" "${pool_gulped}"
    state_set "fixed_pool_emission_expiration" "${expiration}"
    state_set "pool_emission_expiration" "${expiration}"
    state_set "phase" "emissions_streaming"
    state_set "emissions_started_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    cat <<EOF
${NETWORK_LABEL} post-migration emissions verified
  emitter BLND received: ${received}
  expected backstop 70% tranche: ${expected_backstop_tranche}
  expected pool 30% tranche: ${expected_pool_tranche}
  Fixed pool tranche streamed: ${pool_gulped}
  backstop and pool stream expiration: $(format_timestamp "${expiration}") (${expiration})
  Custom v3 reward status: member with zero BLND weight and no BLND allocation
  next action: claim backstop and pool emissions after the streams expire
EOF
}

claim_backstop_tier_resumable() {
    local label="$1" candidate_id="$2" user="$3" pool="$4" tier="$5"
    local result_key="$6" blnd_id completed lp_out claimed_blnd
    local assets_key balance_key shares_key assets_before assets_current assets_after assets_delta
    local blnd_before blnd_current blnd_after shares_before shares_current shares_after pool_addresses

    pool_addresses="$(jq -cn --arg pool "${pool}" '[ $pool ]')"
    blnd_id="$(state_value blnd_token)"

    completed="$(state_optional "${result_key}")"
    if [[ -n "${completed}" ]]; then
        [[ "${completed}" =~ ^[0-9]+$ ]] ||
            die "${label} saved LP result is invalid: ${completed}"
        printf '%s\n' "${completed}"
        return
    fi

    assets_key="${result_key}_assets_before"
    balance_key="${result_key}_blnd_before"
    shares_key="${result_key}_shares_before"
    assets_before="$(state_optional "${assets_key}")"
    blnd_before="$(state_optional "${balance_key}")"
    shares_before="$(state_optional "${shares_key}")"
    if [[ -z "${assets_before}" || -z "${blnd_before}" || -z "${shares_before}" ]]; then
        assets_before="$(read_pool_tier_assets \
            "query-${label}-tier-assets-before-claim" \
            "${candidate_id}" "${pool}" "${tier}")"
        blnd_before="$(read_token_balance \
            "query-${label}-blnd-before-claim" \
            "${blnd_id}" "${candidate_id}")"
        shares_before="$(read_user_tier_shares \
            "query-${label}-shares-before-claim" \
            "${candidate_id}" "${pool}" "${tier}" "${user}")"
        state_set "${assets_key}" "${assets_before}"
        state_set "${balance_key}" "${blnd_before}"
        state_set "${shares_key}" "${shares_before}"
    fi
    [[ "${assets_before}" =~ ^[0-9]+$ ]] ||
        die "${label} saved pre-claim tier assets are invalid: ${assets_before}"
    [[ "${blnd_before}" =~ ^[0-9]+$ ]] ||
        die "${label} saved pre-claim BLND balance is invalid: ${blnd_before}"
    [[ "${shares_before}" =~ ^[0-9]+$ ]] ||
        die "${label} saved pre-claim tier shares are invalid: ${shares_before}"

    assets_current="$(read_pool_tier_assets \
        "query-${label}-tier-assets-current" \
        "${candidate_id}" "${pool}" "${tier}")"
    blnd_current="$(read_token_balance \
        "query-${label}-blnd-current" "${blnd_id}" "${candidate_id}")"
    shares_current="$(read_user_tier_shares \
        "query-${label}-shares-current" \
        "${candidate_id}" "${pool}" "${tier}" "${user}")"
    assets_delta=$((assets_current - assets_before))
    if (( assets_delta > 0 )); then
        (( shares_current > shares_before )) ||
            die "${label} tier assets increased without crediting user shares"
        claimed_blnd=$((blnd_before - blnd_current))
        (( claimed_blnd > 0 )) ||
            die "${label} compounded tier assets without consuming BLND"
        state_set "${result_key}_claimed_blnd" "${claimed_blnd}"
        state_set "${result_key}" "${assets_delta}"
        printf '%s\n' "${assets_delta}"
        return
    fi
    (( assets_delta == 0 )) ||
        die "${label} tier assets decreased unexpectedly: ${assets_delta}"
    assert_equal "${shares_before}" "${shares_current}" \
        "${label} unchanged pre-claim tier shares"
    (( blnd_current >= blnd_before )) ||
        die "${label} BLND decreased without a compounded tier-assets delta"
    if (( blnd_current > blnd_before )); then
        blnd_before="${blnd_current}"
        state_set "${balance_key}" "${blnd_before}"
    fi

    lp_out="$(normalize_scalar "$(invoke_transaction_as \
        "claim-${label}-backstop-stream" "${DEPOSITOR_IDENTITY}" \
        "${candidate_id}" claim \
        --tier "${tier}" \
        --from "${user}" \
        --pool_addresses "${pool_addresses}" \
        --min_lp_tokens_out 0)")"
    if [[ ! "${lp_out}" =~ ^[0-9]+$ ]] || (( lp_out <= 0 )); then
        die "${label} backstop claim did not compound positive LP: ${lp_out}"
    fi
    assets_after="$(read_pool_tier_assets \
        "query-${label}-tier-assets-after-claim" \
        "${candidate_id}" "${pool}" "${tier}")"
    blnd_after="$(read_token_balance \
        "query-${label}-blnd-after-claim" "${blnd_id}" "${candidate_id}")"
    shares_after="$(read_user_tier_shares \
        "query-${label}-shares-after-claim" \
        "${candidate_id}" "${pool}" "${tier}" "${user}")"
    assert_equal "${lp_out}" "$((assets_after - assets_before))" \
        "${label} compounded tier-assets delta"
    claimed_blnd=$((blnd_before - blnd_after))
    (( claimed_blnd > 0 )) ||
        die "${label} backstop claim did not consume positive BLND"
    (( shares_after > shares_before )) ||
        die "${label} backstop claim did not credit positive user shares"
    state_set "${result_key}_claimed_blnd" "${claimed_blnd}"
    state_set "${result_key}" "${lp_out}"
    printf '%s\n' "${lp_out}"
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
    local phase candidate_id depositor pool_id expiration now claimed
    local emission_reserve_token_id
    local fixed_blnd_usdc_lp fixed_blnd_xlm_lp

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
        die "emission streams remain locked for $((expiration - now)) seconds"

    candidate_id="$(state_value candidate_backstop)"
    depositor="$(state_value depositor)"

    note "Refreshing the distribution checkpoint before tier compounding..."
    capture_ongoing_distribution \
        "refresh-before-fixed-blnd-usdc-stream-claim" "${candidate_id}" >/dev/null
    fixed_blnd_usdc_lp="$(claim_backstop_tier_resumable \
        fixed-blnd-usdc "${candidate_id}" "${depositor}" "${pool_id}" SecondLoss \
        ongoing_fixed_blnd_usdc_lp_compounded)"

    note "Waiting ${EMISSION_START_DELAY_SECONDS} seconds before the next tier checkpoint..."
    sleep "${EMISSION_START_DELAY_SECONDS}"
    capture_ongoing_distribution \
        "refresh-before-fixed-blnd-xlm-stream-claim" "${candidate_id}" >/dev/null
    fixed_blnd_xlm_lp="$(claim_backstop_tier_resumable \
        fixed-blnd-xlm "${candidate_id}" "${depositor}" "${pool_id}" FirstLoss \
        ongoing_fixed_blnd_xlm_lp_compounded)"

    claimed="$(claim_pool_emissions_resumable \
        fixed "${pool_id}" "${emission_reserve_token_id}" \
        ongoing_fixed_pool_claimed)"
    state_set "ongoing_fixed_pool_claimed" "${claimed}"
    state_set "ongoing_pool_claimed" "${claimed}"
    state_set "phase" "emissions_claimed"
    state_set "emissions_claimed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    note "${NETWORK_LABEL} seven-day emissions claimed: Fixed tiers ${fixed_blnd_usdc_lp}/${fixed_blnd_xlm_lp} LP and pool tranche ${claimed} BLND base units; Custom remains at zero BLND weight."
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
        deploy-reserve-loss-pool) command_deploy_reserve_loss_pool ;;
        verify-reserve-loss-pool) command_verify_reserve_loss_pool ;;
        begin) command_begin ;;
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

if [[ "${BLEND_MIGRATION_LIBRARY_ONLY:-false}" != "true" ]]; then
    main "$@"
fi
