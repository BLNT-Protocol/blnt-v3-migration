#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${BLEND_LOCALNET_WORK_DIR:-${ROOT_DIR}/.localnet}"
CONFIG_DIR="${WORK_DIR}/stellar-config"
STATE_FILE="${WORK_DIR}/state.json"
CONTAINER_NAME="${BLEND_LOCALNET_CONTAINER:-blend-v3-local}"
QUICKSTART_CONTAINER_NAME="stellar-${CONTAINER_NAME}"
RPC_PORT="${BLEND_LOCALNET_RPC_PORT:-8000}"
RPC_URL="http://localhost:${RPC_PORT}/soroban/rpc"
NETWORK_PASSPHRASE="Standalone Network ; February 2017"
OPERATOR_IDENTITY="blend-v3-local-operator"
DEPOSITOR_IDENTITY="blend-v3-local-depositor"

SCALAR_7=10000000
INCUMBENT_BALANCE=$((100 * SCALAR_7))
CANDIDATE_BALANCE=$((INCUMBENT_BALANCE + 1))
PREPARATION_WINDOW_SECONDS=$((7 * 24 * 60 * 60))
# Quickstart closes one ledger per second. Explicitly renewed legacy entries get
# ten wall-clock days of TTL, leaving a two-times safety margin for the
# documented five-day keepalive cadence.
LOCALNET_TTL_EXTENSION_LEDGERS=$((10 * 24 * 60 * 60))

INCUMBENT_SALT="0000000000000000000000000000000000000000000000000000000000000011"
CANDIDATE_SALT="0000000000000000000000000000000000000000000000000000000000000022"
POOL_SALT="0000000000000000000000000000000000000000000000000000000000000033"

V2_FACTORY_WASM="${ROOT_DIR}/target/wasm32-unknown-unknown/optimized/pool_factory.wasm"
V2_BACKSTOP_WASM="${ROOT_DIR}/target/wasm32-unknown-unknown/optimized/backstop.wasm"
V2_POOL_WASM="${ROOT_DIR}/target/wasm32-unknown-unknown/optimized/pool.wasm"
V3_FACTORY_WASM="${ROOT_DIR}/v3/target/wasm32v1-none/optimized/blend_v3_pool_factory.wasm"
V3_BACKSTOP_WASM="${ROOT_DIR}/v3/target/wasm32v1-none/optimized/blend_v3_backstop.wasm"
V3_POOL_WASM="${ROOT_DIR}/v3/target/wasm32v1-none/optimized/blend_v3_pool.wasm"
V3_TEST_VALUATION_ADAPTER_WASM="${ROOT_DIR}/v3/target/wasm32v1-none/optimized/blend_v3_test_valuation_adapter.wasm"

EXPECTED_EMITTER_SHA256="438a5528cff17ede6fe515f095c43c5f15727af17d006971485e52462e7e7b89"
EMITTER_WASM=""
RUN_DIR=""
DOCKER_HOST_URI="${BLEND_LOCALNET_DOCKER_HOST:-}"

usage() {
    cat <<'EOF'
Usage: scripts/localnet-migration.sh COMMAND

Commands:
  plan      Describe the deployment and migration phases without mutation.
  validate  Build artifacts and validate the CLI and published emitter WASM.
  start     Start Protocol-27 Quickstart with testnet resource limits.
  deploy    Deploy, fund, and begin a new migration on the running network.
  run       Validate, start, deploy, and report status for a new local network.
  status    Query balances, queue state, candidate state, and the next time gate.
  keepalive Refresh TTLs affected by Quickstart's one-second ledger cadence.
  recover   Restart stalled Stellar Core after host sleep without replacing state.
  prepare   Verify the emitter queue during its final seven-day window.
  finalize  Atomically swap the emitter and activate the prepared candidate.
  verify    Assert final bindings, balances, and migration state.
  stop      Stop the named ephemeral Quickstart container.

The runner never weakens the production 31-day queue. Keep Quickstart running,
then call prepare and finalize at the timestamps reported by status. Generated
configuration, public state, results, and cost logs live below .localnet/.
EOF
}

note() {
    printf '%s\n' "$*" >&2
}

die() {
    note "error: $*"
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

stellar_cli() {
    stellar --config-dir "${CONFIG_DIR}" "$@"
}

resolve_emitter_wasm() {
    local candidate actual_hash
    local cargo_root="${CARGO_HOME:-${HOME}/.cargo}"
    local candidates=(
        "${ROOT_DIR}/../blend-contracts/blend-contract-sdk/wasm/emitter.wasm"
        "${ROOT_DIR}/../blend-utils/wasm_v1/emitter.wasm"
    )

    for candidate in \
        "${candidates[@]}" \
        "${cargo_root}"/registry/src/*/blend-contract-sdk-1.22.0/wasm/emitter.wasm
    do
        [[ -f "${candidate}" ]] || continue
        actual_hash="$(shasum -a 256 "${candidate}")"
        actual_hash="${actual_hash%% *}"
        if [[ "${actual_hash}" == "${EXPECTED_EMITTER_SHA256}" ]]; then
            EMITTER_WASM="${candidate}"
            return
        fi
    done

    die "published emitter WASM with SHA-256 ${EXPECTED_EMITTER_SHA256} was not found"
}

validate_tools() {
    local version_text version_line version major

    require_command cargo
    require_command curl
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

build_artifacts() {
    note "Building frozen SDK-22 and candidate SDK-27 artifacts..."
    (
        cd "${ROOT_DIR}"
        make build build-v3
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
        "${V3_TEST_VALUATION_ADAPTER_WASM}" \
        "${EMITTER_WASM}"
    do
        [[ -s "${artifact}" ]] || die "missing or empty WASM artifact: ${artifact}"
    done
    note "Validated SDK-22 incumbent, published emitter, and SDK-27 candidate artifacts."
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
    curl --fail --silent --show-error --max-time 2 \
        --header "Content-Type: application/json" \
        --request POST \
        --data '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' \
        "${RPC_URL}" 2>/dev/null |
        jq -e '.result.status == "healthy"' >/dev/null 2>&1
}

require_network() {
    network_healthy ||
        die "local RPC is unavailable or stale at ${RPC_URL}; run recover if the existing Quickstart container survived, otherwise run start"
}

wait_for_network() {
    local attempt

    for attempt in $(seq 1 30); do
        if network_healthy; then
            note "Local RPC is healthy at ${RPC_URL}."
            return
        fi
        sleep 2
    done
    die "local RPC did not become healthy within 60 seconds"
}

new_run_dir() {
    local run_stamp

    run_stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    RUN_DIR="${WORK_DIR}/runs/${run_stamp}"
    mkdir -p "${RUN_DIR}"
}

load_state() {
    [[ -f "${STATE_FILE}" ]] || die "deployment state not found; run deploy first"
    RUN_DIR="$(jq -er '.run_dir' "${STATE_FILE}")"
    [[ -d "${RUN_DIR}" ]] || die "recorded run directory does not exist: ${RUN_DIR}"
}

state_value() {
    jq -er --arg key "$1" '.[$key]' "${STATE_FILE}"
}

state_set() {
    local key="$1" value="$2" temporary="${STATE_FILE}.tmp"

    jq --arg key "${key}" --arg value "${value}" '.[$key] = $value' \
        "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
}

initialize_state() {
    local operator="$1"

    mkdir -p "${WORK_DIR}"
    jq -n \
        --arg network_passphrase "${NETWORK_PASSPHRASE}" \
        --arg rpc_url "${RPC_URL}" \
        --arg container_name "${CONTAINER_NAME}" \
        --arg operator "${operator}" \
        --arg run_dir "${RUN_DIR}" \
        --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        '{
            network_passphrase: $network_passphrase,
            rpc_url: $rpc_url,
            container_name: $container_name,
            operator: $operator,
            run_dir: $run_dir,
            created_at: $created_at
        }' >"${STATE_FILE}"
}

capture() {
    local label="$1"
    local stdout_file stderr_file timestamp original_label suffix
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
    "$@" >"${stdout_file}" 2> >(tee "${stderr_file}" >&2)
    cat "${stdout_file}"
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
        --ledgers-to-extend "${LOCALNET_TTL_EXTENSION_LEDGERS}" \
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
        --ledgers-to-extend "${LOCALNET_TTL_EXTENSION_LEDGERS}" \
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
    local queued_swap="$1" candidate_id="$2" next_token_id="$3" unlock_time="$4"
    local queued_candidate queued_next_token queued_unlock

    [[ "${queued_swap}" != "null" ]] ||
        die "legacy emitter queue is absent"
    queued_candidate="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop')"
    queued_next_token="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop_token')"
    queued_unlock="$(printf '%s' "${queued_swap}" | jq -er '.unlock_time | tostring')"
    [[ "${queued_candidate}" == "${candidate_id}" ]] ||
        die "live emitter queue targets ${queued_candidate}, not saved candidate ${candidate_id}"
    [[ "${queued_next_token}" == "${next_token_id}" ]] ||
        die "live emitter queue uses ${queued_next_token}, not saved next token ${next_token_id}"
    [[ "${queued_unlock}" == "${unlock_time}" ]] ||
        die "live emitter unlock ${queued_unlock} differs from saved unlock ${unlock_time}"
}

command_plan() {
    cat <<EOF
Blend v3 local migration plan

Network:
  Protocol: 27
  Soroban limits: testnet
  RPC: ${RPC_URL}
  Container: ${CONTAINER_NAME}

Artifacts:
  Frozen incumbent: SDK 22 pool, pool factory, and backstop
  Legacy emitter: blend-contract-sdk 1.22.0 published WASM
  Candidate: SDK 27 pool, pool factory, and backstop

Deployment:
  1. Create and fund isolated local issuer/operator and depositor identities.
  2. Issue local BLND, BLNDUSDC, BLNDXLM, and USDC assets.
  3. Deploy the legacy emitter and a coherent SDK-22 incumbent stack.
  4. Predict the candidate address, bind its immutable factory, and deploy it.
  5. Deploy a registered v3 pool and deposit ${CANDIDATE_BALANCE} BLNDUSDC units.
  6. Fund the incumbent with ${INCUMBENT_BALANCE} BLNDUSDC units.
  7. Assert strict candidate qualification and call begin_migration.

Time-gated continuation:
  8. Call prepare during the final seven days of the production 31-day queue.
  9. Call finalize at or after unlock.
 10. Verify the emitter, candidate, pool, balances, and cleared queue.

Mutation is limited to the named ephemeral container and ${WORK_DIR}.
EOF
}

command_validate() {
    validate_tools
    build_artifacts
    validate_artifacts
}

command_start() {
    validate_tools
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

command_deploy() {
    local operator depositor issuer emitter_id blnd_id legacy_token_id next_token_id prefund_token_id
    local v2_pool_hash incumbent_id v2_factory_id deployed_incumbent_id
    local valuation_adapter_id v3_pool_hash candidate_id v3_factory_id deployed_candidate_id
    local pool_id pool_raw
    local incumbent_balance candidate_balance begin_raw epoch_start queued_swap unlock_time
    local queued_candidate queued_next_token

    validate_tools
    require_network
    [[ ! -e "${STATE_FILE}" ]] ||
        die "deployment state already exists at ${STATE_FILE}; refusing to overwrite it"
    build_artifacts
    validate_artifacts
    new_run_dir

    note "Creating and funding isolated local operator ${OPERATOR_IDENTITY}..."
    stellar_cli keys generate "${OPERATOR_IDENTITY}" \
        --overwrite \
        --fund \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    operator="$(stellar_cli keys public-key "${OPERATOR_IDENTITY}")"
    issuer="${operator}"
    initialize_state "${operator}"
    state_set "emitter_wasm_sha256" "${EXPECTED_EMITTER_SHA256}"

    note "Creating and funding isolated local depositor ${DEPOSITOR_IDENTITY}..."
    stellar_cli keys generate "${DEPOSITOR_IDENTITY}" \
        --overwrite \
        --fund \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    depositor="$(stellar_cli keys public-key "${DEPOSITOR_IDENTITY}")"
    state_set "depositor" "${depositor}"

    emitter_id="$(deploy_wasm "deploy-legacy-emitter" "${EMITTER_WASM}" "")"
    state_set "emitter" "${emitter_id}"

    blnd_id="$(deploy_asset "deploy-asset-blnd" "BLND" "${issuer}")"
    legacy_token_id="$(deploy_asset "deploy-asset-blnd-usdc" "BLNDUSDC" "${issuer}")"
    next_token_id="$(deploy_asset "deploy-asset-blnd-xlm" "BLNDXLM" "${issuer}")"
    prefund_token_id="$(deploy_asset "deploy-asset-usdc" "USDC" "${issuer}")"
    state_set "blnd_token" "${blnd_id}"
    state_set "legacy_backstop_token" "${legacy_token_id}"
    state_set "next_backstop_token" "${next_token_id}"
    state_set "prefund_token" "${prefund_token_id}"

    # This deterministic one-to-one fixture exercises the immutable valuation
    # boundary locally. It is deliberately not a production pricing source.
    valuation_adapter_id="$(deploy_wasm \
        "deploy-test-valuation-adapter" "${V3_TEST_VALUATION_ADAPTER_WASM}" "" \
        --blnd_usdc "${legacy_token_id}" \
        --blnd_xlm "${next_token_id}")"
    state_set "valuation_adapter" "${valuation_adapter_id}"

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
        --backstop_token "${legacy_token_id}" \
        --emitter "${emitter_id}" \
        --blnd_token "${blnd_id}" \
        --usdc_token "${prefund_token_id}" \
        --pool_factory "${v2_factory_id}" \
        --drop_list "[]")"
    assert_equal "${incumbent_id}" "${deployed_incumbent_id}" \
        "incumbent deterministic address"

    invoke_transaction "initialize-legacy-emitter" "${emitter_id}" initialize \
        --blnd_token "${blnd_id}" \
        --backstop "${incumbent_id}" \
        --backstop_token "${legacy_token_id}" >/dev/null

    v3_pool_hash="$(upload_wasm "upload-v3-pool" "${V3_POOL_WASM}")"
    state_set "v3_pool_wasm_hash" "${v3_pool_hash}"
    candidate_id="$(predict_contract_id "${CANDIDATE_SALT}")"
    require_contract_id "${candidate_id}" "predicted candidate"
    state_set "candidate_backstop" "${candidate_id}"

    v3_factory_id="$(deploy_wasm \
        "deploy-v3-pool-factory" "${V3_FACTORY_WASM}" "" \
        --backstop "${candidate_id}" \
        --pool_wasm_hash "${v3_pool_hash}")"
    state_set "v3_pool_factory" "${v3_factory_id}"

    deployed_candidate_id="$(deploy_wasm \
        "deploy-v3-backstop" "${V3_BACKSTOP_WASM}" "${CANDIDATE_SALT}" \
        --emitter "${emitter_id}" \
        --blnd_token "${blnd_id}" \
        --pool_factory "${v3_factory_id}" \
        --legacy_backstop_token "${legacy_token_id}" \
        --next_backstop_token "${next_token_id}" \
        --prefund_token "${prefund_token_id}" \
        --valuation_adapter "${valuation_adapter_id}")"
    assert_equal "${candidate_id}" "${deployed_candidate_id}" \
        "candidate deterministic address"

    pool_raw="$(invoke_transaction "deploy-v3-pool" "${v3_factory_id}" deploy \
        --deployer "${operator}" --salt "${POOL_SALT}")"
    pool_id="$(normalize_scalar "${pool_raw}")"
    require_contract_id "${pool_id}" "deployed v3 pool"
    state_set "v3_pool" "${pool_id}"

    capture "create-depositor-blnd-usdc-trustline" \
        stellar_cli tx new change-trust \
        --source-account "${DEPOSITOR_IDENTITY}" \
        --line "BLNDUSDC:${issuer}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    invoke_transaction "mint-incumbent-legacy-token" "${legacy_token_id}" mint \
        --to "${incumbent_id}" --amount "${INCUMBENT_BALANCE}" >/dev/null
    invoke_transaction "mint-candidate-depositor-legacy-token" "${legacy_token_id}" mint \
        --to "${depositor}" --amount "${CANDIDATE_BALANCE}" >/dev/null
    capture "deposit-candidate-legacy-token" \
        stellar_cli contract invoke \
        --id "${candidate_id}" \
        --source-account "${DEPOSITOR_IDENTITY}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost \
        -- deposit_blnd_usdc \
        --from "${depositor}" \
        --pool "${pool_id}" \
        --amount "${CANDIDATE_BALANCE}" >/dev/null

    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "query-incumbent-legacy-balance" "${legacy_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "query-candidate-legacy-balance" "${legacy_token_id}" balance --id "${candidate_id}")")"
    assert_equal "${INCUMBENT_BALANCE}" "${incumbent_balance}" "incumbent balance"
    assert_equal "${CANDIDATE_BALANCE}" "${candidate_balance}" "candidate balance"
    (( candidate_balance > incumbent_balance )) ||
        die "candidate does not strictly exceed the incumbent legacy-token balance"

    begin_raw="$(invoke_transaction "begin-migration" "${candidate_id}" begin_migration)"
    epoch_start="$(normalize_scalar "${begin_raw}")"
    [[ "${epoch_start}" =~ ^[0-9]+$ ]] ||
        die "invalid migration epoch start: ${epoch_start}"
    queued_swap="$(invoke_view \
        "query-open-emitter-queue" "${emitter_id}" get_queued_swap)"
    unlock_time="$(printf '%s' "${queued_swap}" | jq -er '.unlock_time')"
    queued_candidate="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop')"
    queued_next_token="$(printf '%s' "${queued_swap}" | jq -er '.new_backstop_token')"
    [[ "${unlock_time}" =~ ^[0-9]+$ ]] || die "invalid migration unlock time: ${unlock_time}"
    assert_equal "${candidate_id}" "${queued_candidate}" "queued candidate"
    assert_equal "${next_token_id}" "${queued_next_token}" "queued next designated token"
    state_set "migration_epoch_start" "${epoch_start}"
    state_set "unlock_time" "${unlock_time}"
    state_set "prepare_not_before" "$((unlock_time - PREPARATION_WINDOW_SECONDS))"
    state_set "incumbent_balance" "${INCUMBENT_BALANCE}"
    state_set "candidate_balance" "${CANDIDATE_BALANCE}"
    state_set "phase" "open"

    stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted >"${RUN_DIR}/ledger-after-begin.json"
    note "Deployment and migration queue succeeded. State: ${STATE_FILE}"
    command_status
}

command_status() {
    local emitter_id incumbent_id candidate_id next_token_id legacy_token_id
    local unlock_time prepare_not_before phase
    local emitter_backstop incumbent_balance candidate_balance queued_swap migration_status now

    validate_tools
    require_network
    load_state
    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    next_token_id="$(state_value next_backstop_token)"
    legacy_token_id="$(state_value legacy_backstop_token)"
    unlock_time="$(state_value unlock_time)"
    prepare_not_before="$(state_value prepare_not_before)"
    phase="$(state_value phase)"

    emitter_backstop="$(normalize_scalar "$(invoke_view \
        "status-emitter-backstop" "${emitter_id}" get_backstop)")"
    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "status-incumbent-balance" "${legacy_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "status-candidate-balance" "${legacy_token_id}" balance --id "${candidate_id}")")"
    queued_swap="$(invoke_view "status-emitter-queue" "${emitter_id}" get_queued_swap)"
    migration_status="$(invoke_view "status-candidate-migration" "${candidate_id}" migration_status)"
    now="$(date +%s)"

    if [[ "${queued_swap}" != "null" ]]; then
        assert_queued_swap_matches \
            "${queued_swap}" "${candidate_id}" "${next_token_id}" "${unlock_time}"
    elif [[ "${phase}" != "active" && "${phase}" != "verified" ]]; then
        die "legacy emitter queue is absent while saved phase is ${phase}"
    fi

    cat <<EOF
Local migration status
  emitter backstop: ${emitter_backstop}
  incumbent: ${incumbent_id}
  candidate: ${candidate_id}
  incumbent BLNDUSDC: ${incumbent_balance}
  candidate BLNDUSDC: ${candidate_balance}
  candidate state: ${migration_status}
  queued swap: ${queued_swap}
  prepare not before: $(format_timestamp "${prepare_not_before}") (${prepare_not_before})
  unlock: $(format_timestamp "${unlock_time}") (${unlock_time})
EOF

    if (( now < prepare_not_before )); then
        note "Next action: prepare in $((prepare_not_before - now)) seconds."
    elif (( now < unlock_time )); then
        note "Next action: run prepare now; finalization opens in $((unlock_time - now)) seconds."
    else
        note "Queue is unlocked. If prepared, run finalize."
    fi
}

command_keepalive() {
    local emitter_id incumbent_id candidate_id factory_id pool_id valuation_adapter_id depositor
    local blnd_id legacy_token_id next_token_id prefund_token_id unlock_time
    local stamp prefix last_distro_key blnd_admin queued_swap
    local incumbent_balance candidate_balance

    validate_tools
    require_network
    load_state
    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    factory_id="$(state_value v3_pool_factory)"
    pool_id="$(state_value v3_pool)"
    valuation_adapter_id="$(state_value valuation_adapter)"
    depositor="$(state_value depositor)"
    blnd_id="$(state_value blnd_token)"
    legacy_token_id="$(state_value legacy_backstop_token)"
    next_token_id="$(state_value next_backstop_token)"
    prefund_token_id="$(state_value prefund_token)"
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
    assert_queued_swap_matches \
        "${queued_swap}" "${candidate_id}" "${next_token_id}" "${unlock_time}"
    extend_contract_key_xdr "${prefix}-emitter-last-distro" \
        "${emitter_id}" "${last_distro_key}" >/dev/null
    invoke_forced_transaction "${prefix}-candidate" \
        "${candidate_id}" migration_status >/dev/null
    invoke_forced_transaction "${prefix}-factory" \
        "${factory_id}" is_pool --pool "${pool_id}" >/dev/null
    invoke_forced_transaction "${prefix}-pool" \
        "${pool_id}" backstop >/dev/null
    extend_contract_instance "${prefix}-valuation-adapter" \
        "${valuation_adapter_id}" >/dev/null
    invoke_forced_transaction "${prefix}-position" \
        "${candidate_id}" blnd_usdc_shares \
        --user "${depositor}" \
        --pool "${pool_id}" >/dev/null
    extend_contract_instance "${prefix}-legacy-token-instance" \
        "${legacy_token_id}" >/dev/null
    incumbent_balance="$(normalize_scalar "$(invoke_forced_transaction \
        "${prefix}-incumbent-balance" \
        "${legacy_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_forced_transaction \
        "${prefix}-candidate-balance" \
        "${legacy_token_id}" balance --id "${candidate_id}")")"
    [[ "${incumbent_balance}" =~ ^[0-9]+$ ]] ||
        die "invalid incumbent legacy-token balance: ${incumbent_balance}"
    [[ "${candidate_balance}" =~ ^[0-9]+$ ]] ||
        die "invalid candidate legacy-token balance: ${candidate_balance}"
    (( candidate_balance > incumbent_balance )) ||
        die "candidate no longer strictly exceeds incumbent legacy-token balance"
    extend_contract_instance "${prefix}-blnd-instance" \
        "${blnd_id}" >/dev/null
    extend_contract_instance "${prefix}-next-token-instance" \
        "${next_token_id}" >/dev/null
    extend_contract_instance "${prefix}-prefund-token-instance" \
        "${prefund_token_id}" >/dev/null
    blnd_admin="$(normalize_scalar "$(invoke_view \
        "${prefix}-blnd-admin" "${blnd_id}" admin)")"
    assert_equal "${emitter_id}" "${blnd_admin}" "BLND administrator"

    state_set "last_keepalive_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Local migration TTL keepalive completed. Run this at least every five days; daily is recommended."
}

command_recover() {
    local running

    validate_tools
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
    local candidate_id result

    validate_tools
    require_network
    load_state
    candidate_id="$(state_value candidate_backstop)"
    result="$(normalize_scalar "$(invoke_transaction \
        "prepare-migration" "${candidate_id}" prepare_migration)")"
    [[ "${result}" =~ ^[0-9]+$ ]] || die "invalid preparation result: ${result}"
    state_set "phase" "prepared"
    state_set "prepared_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Migration queue prepared against original unlock ${result}."
    command_status
}

command_finalize() {
    local candidate_id result

    validate_tools
    require_network
    load_state
    candidate_id="$(state_value candidate_backstop)"
    result="$(normalize_scalar "$(invoke_transaction \
        "finalize-migration" "${candidate_id}" finalize_migration)")"
    [[ "${result}" =~ ^[0-9]+$ ]] || die "invalid activation timestamp: ${result}"
    state_set "phase" "active"
    state_set "activated_at" "${result}"
    command_verify
}

command_verify() {
    local operator emitter_id incumbent_id candidate_id factory_id pool_id
    local valuation_adapter_id legacy_token_id next_token_id emitter_backstop
    local candidate_legacy_token candidate_next_token candidate_valuation_adapter
    local factory_backstop pool_backstop pool_registered
    local incumbent_balance candidate_balance queued_swap migration_status activated_at

    validate_tools
    require_network
    load_state
    operator="$(state_value operator)"
    emitter_id="$(state_value emitter)"
    incumbent_id="$(state_value incumbent_backstop)"
    candidate_id="$(state_value candidate_backstop)"
    factory_id="$(state_value v3_pool_factory)"
    pool_id="$(state_value v3_pool)"
    valuation_adapter_id="$(state_value valuation_adapter)"
    legacy_token_id="$(state_value legacy_backstop_token)"
    next_token_id="$(state_value next_backstop_token)"

    emitter_backstop="$(normalize_scalar "$(invoke_view \
        "verify-emitter-backstop" "${emitter_id}" get_backstop)")"
    candidate_legacy_token="$(normalize_scalar "$(invoke_view \
        "verify-candidate-legacy-token" "${candidate_id}" backstop_token)")"
    candidate_next_token="$(normalize_scalar "$(invoke_view \
        "verify-candidate-next-token" "${candidate_id}" next_backstop_token)")"
    candidate_valuation_adapter="$(normalize_scalar "$(invoke_view \
        "verify-candidate-valuation-adapter" "${candidate_id}" valuation_adapter)")"
    factory_backstop="$(normalize_scalar "$(invoke_view \
        "verify-factory-backstop" "${factory_id}" backstop)")"
    pool_backstop="$(normalize_scalar "$(invoke_view \
        "verify-pool-backstop" "${pool_id}" backstop)")"
    pool_registered="$(normalize_scalar "$(invoke_view \
        "verify-pool-registration" "${factory_id}" is_pool --pool "${pool_id}")")"
    incumbent_balance="$(normalize_scalar "$(invoke_view \
        "verify-incumbent-balance" "${legacy_token_id}" balance --id "${incumbent_id}")")"
    candidate_balance="$(normalize_scalar "$(invoke_view \
        "verify-candidate-balance" "${legacy_token_id}" balance --id "${candidate_id}")")"
    queued_swap="$(invoke_view "verify-emitter-queue" "${emitter_id}" get_queued_swap)"
    migration_status="$(invoke_view \
        "verify-candidate-migration" "${candidate_id}" migration_status)"
    activated_at="$(normalize_scalar "$(invoke_view \
        "verify-candidate-activation" "${candidate_id}" activated_at)")"

    assert_equal "${candidate_id}" "${emitter_backstop}" "emitter backstop"
    assert_equal "${legacy_token_id}" "${candidate_legacy_token}" "candidate legacy token"
    assert_equal "${next_token_id}" "${candidate_next_token}" "candidate next token"
    assert_equal "${valuation_adapter_id}" "${candidate_valuation_adapter}" \
        "candidate valuation adapter"
    assert_equal "${candidate_id}" "${factory_backstop}" "v3 factory binding"
    assert_equal "${candidate_id}" "${pool_backstop}" "v3 pool binding"
    assert_equal "true" "${pool_registered}" "v3 pool registration"
    assert_equal "${INCUMBENT_BALANCE}" "${incumbent_balance}" "incumbent retained balance"
    assert_equal "${CANDIDATE_BALANCE}" "${candidate_balance}" "candidate retained balance"
    (( candidate_balance > incumbent_balance )) ||
        die "candidate no longer strictly exceeds incumbent legacy-token balance"
    printf '%s' "${queued_swap}" | jq -e '. == null' >/dev/null ||
        die "legacy emitter queue was not cleared: ${queued_swap}"
    printf '%s' "${migration_status}" | jq -e '
        . == "Active" or
        (type == "object" and (has("Active") or .tag? == "Active"))
    ' >/dev/null || die "candidate is not active: ${migration_status}"
    [[ "${activated_at}" =~ ^[0-9]+$ ]] || die "candidate activation timestamp is absent"

    state_set "phase" "verified"
    state_set "verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted >"${RUN_DIR}/ledger-after-verification.json"

    cat <<EOF
Local migration verified
  operator: ${operator}
  incumbent SDK-22 backstop: ${incumbent_id}
  legacy emitter: ${emitter_id}
  candidate SDK-27 backstop: ${candidate_id}
  candidate pool factory: ${factory_id}
  candidate pool: ${pool_id}
  candidate valuation adapter: ${valuation_adapter_id}
  activation timestamp: ${activated_at}
  evidence directory: ${RUN_DIR}
EOF
}

command_stop() {
    validate_tools
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
    command_deploy
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
        recover) command_recover ;;
        prepare) command_prepare ;;
        finalize) command_finalize ;;
        verify) command_verify ;;
        stop) command_stop ;;
        -h|--help|help) usage ;;
        *) usage >&2; exit 2 ;;
    esac
}

main "$@"
