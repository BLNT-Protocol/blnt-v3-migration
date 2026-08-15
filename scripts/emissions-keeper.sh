#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGISTRY="${BLEND_KEEPER_REGISTRY:-${ROOT_DIR}/deployments/testnet-production.json}"
CONFIG_DIR="${BLEND_KEEPER_CONFIG_DIR:-${ROOT_DIR}/.testnet-production/stellar-config}"
SOURCE_ACCOUNT="${BLEND_KEEPER_SOURCE_ACCOUNT:-blend-v3-production-testnet-operator}"
INCLUSION_FEE="${BLEND_KEEPER_INCLUSION_FEE:-10000}"
INTERVAL_SECONDS="${BLEND_KEEPER_INTERVAL_SECONDS:-3600}"
STOP_REQUESTED=0
SLEEP_PID=""

usage() {
    cat <<'EOF'
Usage: scripts/emissions-keeper.sh COMMAND

Commands:
  plan  Simulate one v2-and-v3 keeper pass without mutation.
  once  Submit one v2-and-v3 keeper pass and exit.
  run   Run one pass immediately, then repeat on an internal schedule.
  help  Show this message.

Each pass advances the incumbent emitter when it still targets v2, checkpoints
the v2 and v3 backstops, then gulps every modeled pool for the corresponding
protocol version. The default run interval is one hour. Configuration may be
overridden with BLEND_KEEPER_REGISTRY,
BLEND_KEEPER_CONFIG_DIR, BLEND_KEEPER_SOURCE_ACCOUNT, BLEND_KEEPER_RPC_URL,
BLEND_KEEPER_INCLUSION_FEE, and BLEND_KEEPER_INTERVAL_SECONDS.
EOF
}

note() {
    printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" >&2
}

die() {
    note "error: $*"
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

require_contract_id() {
    local value="$1" label="$2"
    [[ "${value}" =~ ^C[A-Z2-7]{55}$ ]] || die "invalid ${label}: ${value}"
}

require_positive_integer() {
    local value="$1" label="$2"
    [[ "${value}" =~ ^[1-9][0-9]*$ ]] || die "${label} must be a positive integer"
}

stellar_cli() {
    stellar --config-dir "${CONFIG_DIR}" "$@"
}

invoke() {
    local send="$1" contract_id="$2" function_name="$3"

    stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${SOURCE_ACCOUNT}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send "${send}" \
        -- "${function_name}"
}

capture_invoke() {
    local send="$1" contract_id="$2" function_name="$3" output status

    set +e
    output="$(invoke "${send}" "${contract_id}" "${function_name}" 2>&1)"
    status=$?
    set -e
    printf '%s' "${output}"
    return "${status}"
}

is_contract_error() {
    local output="$1" code="$2"
    [[ "${output}" =~ \#${code}([^0-9]|$) ]]
}

is_expected_error() {
    local output="$1" code
    shift

    for code in "$@"; do
        if is_contract_error "${output}" "${code}"; then
            EXPECTED_ERROR_CODE="${code}"
            return 0
        fi
    done
    return 1
}

simulate_call() {
    local label="$1" contract_id="$2" function_name="$3" output

    if output="$(capture_invoke no "${contract_id}" "${function_name}")"; then
        note "${label}: ready (${output})"
        return 0
    fi
    printf '%s' "${output}"
    return 1
}

submit_call() {
    local label="$1" contract_id="$2" function_name="$3" output

    if output="$(capture_invoke yes "${contract_id}" "${function_name}")"; then
        note "${label}: submitted (${output})"
        return 0
    fi
    printf '%s' "${output}"
    return 1
}

read_emitter_recipient() {
    local output recipient

    if ! output="$(capture_invoke no "${EMITTER_ID}" get_backstop)"; then
        note "read emitter recipient failed: ${output}"
        return 1
    fi
    if ! recipient="$(printf '%s' "${output}" | jq -er 'select(type == "string")')"; then
        note "read emitter recipient returned an invalid address: ${output}"
        return 1
    fi
    if [[ ! "${recipient}" =~ ^C[A-Z2-7]{55}$ ]]; then
        note "read emitter recipient returned an invalid contract ID: ${recipient}"
        return 1
    fi
    printf '%s' "${recipient}"
}

plan_call() {
    local label="$1" contract_id="$2" function_name="$3" output
    shift 3

    if output="$(simulate_call "${label}" "${contract_id}" "${function_name}")"; then
        return
    fi
    if is_expected_error "${output}" "$@"; then
        note "${label}: not ready (contract error #${EXPECTED_ERROR_CODE})"
        return
    fi
    die "${label} simulation failed: ${output}"
}

run_call() {
    local label="$1" contract_id="$2" function_name="$3" output
    shift 3

    if ! output="$(simulate_call "${label}" "${contract_id}" "${function_name}")"; then
        if is_expected_error "${output}" "$@"; then
            note "${label}: skipped (contract error #${EXPECTED_ERROR_CODE})"
            return 0
        fi
        note "${label}: simulation failed: ${output}"
        return 1
    fi

    if ! output="$(submit_call "${label}" "${contract_id}" "${function_name}")"; then
        # Another permissionless keeper may win the race between simulation
        # and submission. Treat only known no-work/timing errors as no-ops.
        if is_expected_error "${output}" "$@"; then
            note "${label}: skipped after another keeper changed state (contract error #${EXPECTED_ERROR_CODE})"
            return 0
        fi
        note "${label}: submission failed: ${output}"
        return 1
    fi
}

load_configuration() {
    local pool_name pool_id

    [[ -f "${REGISTRY}" ]] || die "deployment registry not found: ${REGISTRY}"
    [[ -d "${CONFIG_DIR}" ]] || die "Stellar config directory not found: ${CONFIG_DIR}"

    NETWORK_PASSPHRASE="$(jq -er '.network.passphrase' "${REGISTRY}")"
    RPC_URL="${BLEND_KEEPER_RPC_URL:-$(jq -er '.network.rpc_url' "${REGISTRY}")}"
    EMITTER_ID="$(jq -er '.protocol.emitter' "${REGISTRY}")"
    V2_BACKSTOP_ID="$(jq -er '.protocol.v2.backstop' "${REGISTRY}")"
    V3_BACKSTOP_ID="$(jq -er '.protocol.v3.backstop' "${REGISTRY}")"
    require_contract_id "${EMITTER_ID}" "emitter contract ID"
    require_contract_id "${V2_BACKSTOP_ID}" "v2 backstop contract ID"
    require_contract_id "${V3_BACKSTOP_ID}" "v3 backstop contract ID"

    V2_POOL_NAMES=()
    V2_POOLS=()
    while IFS=$'\t' read -r pool_name pool_id; do
        require_contract_id "${pool_id}" "modeled v2 pool contract ID"
        V2_POOL_NAMES+=("${pool_name}")
        V2_POOLS+=("${pool_id}")
    done < <(jq -er '.modeled_pools | to_entries[] | [.key, .value.v2.pool] | @tsv' "${REGISTRY}")

    V3_POOL_NAMES=()
    V3_POOLS=()
    while IFS=$'\t' read -r pool_name pool_id; do
        require_contract_id "${pool_id}" "modeled v3 pool contract ID"
        V3_POOL_NAMES+=("${pool_name}")
        V3_POOLS+=("${pool_id}")
    done < <(jq -er '.modeled_pools | to_entries[] | [.key, .value.v3.pool] | @tsv' "${REGISTRY}")

    (( ${#V2_POOLS[@]} > 0 )) || die "deployment registry contains no modeled v2 pools"
    (( ${#V3_POOLS[@]} > 0 )) || die "deployment registry contains no modeled v3 pools"
}

plan_pass() {
    local index emitter_recipient

    emitter_recipient="$(read_emitter_recipient)" || die "cannot determine the emitter recipient"

    note "Planning v2 emissions pass: ${V2_BACKSTOP_ID}"
    if [[ "${emitter_recipient}" == "${V2_BACKSTOP_ID}" ]]; then
        plan_call "advance emitter for v2" "${EMITTER_ID}" distribute
    elif [[ "${emitter_recipient}" == "${V3_BACKSTOP_ID}" ]]; then
        note "Emitter targets v3; candidate distribute will advance it."
    else
        die "emitter targets unrecognized backstop ${emitter_recipient}"
    fi
    plan_call "v2 checkpoint protocol emissions" "${V2_BACKSTOP_ID}" distribute 1000 1010
    for index in "${!V2_POOLS[@]}"; do
        plan_call \
            "v2 gulp ${V2_POOL_NAMES[index]} pool ${V2_POOLS[index]}" \
            "${V2_POOLS[index]}" gulp_emissions 1000 1200
    done

    note "Planning v3 emissions pass: ${V3_BACKSTOP_ID}"
    plan_call "v3 checkpoint protocol emissions" "${V3_BACKSTOP_ID}" distribute 1030
    for index in "${!V3_POOLS[@]}"; do
        plan_call \
            "v3 gulp ${V3_POOL_NAMES[index]} pool ${V3_POOLS[index]}" \
            "${V3_POOLS[index]}" gulp_emissions 1034 1200
    done
    note "Plan only: pool simulations reflect current state before either simulated distribution."
}

run_pass() {
    local index emitter_recipient failed=0

    if ! emitter_recipient="$(read_emitter_recipient)"; then
        failed=1
        emitter_recipient=""
    fi

    note "Starting v2 emissions pass: ${V2_BACKSTOP_ID}"
    if [[ "${emitter_recipient}" == "${V2_BACKSTOP_ID}" ]]; then
        run_call "advance emitter for v2" "${EMITTER_ID}" distribute || failed=1
    elif [[ "${emitter_recipient}" == "${V3_BACKSTOP_ID}" ]]; then
        note "Emitter targets v3; candidate distribute will advance it."
    elif [[ -n "${emitter_recipient}" ]]; then
        note "Emitter targets unrecognized backstop ${emitter_recipient}; refusing to advance it."
        failed=1
    fi
    run_call "v2 checkpoint protocol emissions" "${V2_BACKSTOP_ID}" distribute 1000 1010 || failed=1
    for index in "${!V2_POOLS[@]}"; do
        run_call \
            "v2 gulp ${V2_POOL_NAMES[index]} pool ${V2_POOLS[index]}" \
            "${V2_POOLS[index]}" gulp_emissions 1000 1200 || failed=1
    done

    note "Starting v3 emissions pass: ${V3_BACKSTOP_ID}"
    run_call "v3 checkpoint protocol emissions" "${V3_BACKSTOP_ID}" distribute 1030 || failed=1
    for index in "${!V3_POOLS[@]}"; do
        run_call \
            "v3 gulp ${V3_POOL_NAMES[index]} pool ${V3_POOLS[index]}" \
            "${V3_POOLS[index]}" gulp_emissions 1034 1200 || failed=1
    done

    if (( failed != 0 )); then
        note "Emissions pass completed with one or more unexpected failures."
        return 1
    fi
    note "Emissions pass completed successfully."
}

request_stop() {
    STOP_REQUESTED=1
    if [[ -n "${SLEEP_PID}" ]]; then
        kill "${SLEEP_PID}" 2>/dev/null || true
    fi
}

run_forever() {
    local pass_number=0

    trap request_stop INT TERM
    note "Keeper started; interval is ${INTERVAL_SECONDS} seconds."
    while (( STOP_REQUESTED == 0 )); do
        pass_number=$((pass_number + 1))
        note "Beginning scheduled pass ${pass_number}."
        if ! run_pass; then
            note "Scheduled pass ${pass_number} failed; the keeper will retry on the next interval."
        fi
        if (( STOP_REQUESTED != 0 )); then
            break
        fi
        note "Next pass begins in ${INTERVAL_SECONDS} seconds."
        sleep "${INTERVAL_SECONDS}" &
        SLEEP_PID=$!
        wait "${SLEEP_PID}" || true
        SLEEP_PID=""
    done
    note "Keeper stopped."
}

main() {
    local command="${1:-}"

    case "${command}" in
        plan|once|run) ;;
        help|-h|--help) usage; return ;;
        *) usage >&2; exit 2 ;;
    esac

    require_command date
    require_command jq
    require_command stellar
    require_positive_integer "${INCLUSION_FEE}" "BLEND_KEEPER_INCLUSION_FEE"
    require_positive_integer "${INTERVAL_SECONDS}" "BLEND_KEEPER_INTERVAL_SECONDS"
    load_configuration
    export STELLAR_INCLUSION_FEE="${INCLUSION_FEE}"

    case "${command}" in
        plan) plan_pass ;;
        once) run_pass ;;
        run) run_forever ;;
    esac
}

main "$@"
