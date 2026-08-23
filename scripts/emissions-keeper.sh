#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGISTRY="${BLEND_KEEPER_REGISTRY:-${ROOT_DIR}/deployments/testnet-production.json}"
WORK_DIR="${BLEND_KEEPER_WORK_DIR:-${ROOT_DIR}/.testnet-production}"
CONFIG_DIR="${BLEND_KEEPER_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${WORK_DIR}/state.json"
SOURCE_ACCOUNT="${BLEND_KEEPER_SOURCE_ACCOUNT:-blend-v3-production-testnet-operator}"
INCLUSION_FEE="${BLEND_KEEPER_INCLUSION_FEE:-10000}"
INTERVAL_SECONDS="${BLEND_KEEPER_INTERVAL_SECONDS:-3600}"
ORACLE_REFRESH_INTERVAL_SECONDS="${BLEND_KEEPER_ORACLE_REFRESH_INTERVAL_SECONDS:-43200}"
TESTNET_PASSPHRASE="Test SDF Network ; September 2015"
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
the v2 and v3 backstops, then gulps each registered reward-zone pool for the
corresponding protocol version. It reconciles direct reserve-custody deficits
across every registered v3 pool and processes one bounded pending-USDC and
pending-XLM buy-and-burn batch when work is available. On Stellar testnet, it
refreshes the controlled fixture-oracle histories when they are at least 12
hours old. The default keeper interval is one hour. Configuration may be overridden with
BLEND_KEEPER_REGISTRY, BLEND_KEEPER_WORK_DIR, BLEND_KEEPER_CONFIG_DIR,
BLEND_KEEPER_SOURCE_ACCOUNT, BLEND_KEEPER_RPC_URL, BLEND_KEEPER_INCLUSION_FEE,
BLEND_KEEPER_INTERVAL_SECONDS, and BLEND_KEEPER_ORACLE_REFRESH_INTERVAL_SECONDS.
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

read_last_oracle_refresh_epoch() {
    local refreshed_at

    if [[ ! -f "${STATE_FILE}" ]]; then
        printf '0'
        return
    fi
    if refreshed_at="$(jq -er '
        (.modeled_oracles_refreshed_at // empty) | fromdateiso8601
    ' "${STATE_FILE}" 2>/dev/null)"; then
        printf '%s' "${refreshed_at}"
    else
        printf '0'
    fi
}

require_registry_state_match() {
    local registry_filter="$1" state_key="$2" label="$3"
    local registry_value state_value

    registry_value="$(jq -er "${registry_filter}" "${REGISTRY}")" ||
        die "deployment registry is missing ${label}"
    state_value="$(jq -er --arg key "${state_key}" '.[$key]' "${STATE_FILE}")" ||
        die "deployment state is missing ${label}"
    [[ "${registry_value}" == "${state_value}" ]] ||
        die "${label} differs between ${REGISTRY} and ${STATE_FILE}"
}

validate_oracle_refresh_configuration() {
    [[ "${NETWORK_PASSPHRASE}" == "${TESTNET_PASSPHRASE}" ]] || return
    [[ -f "${STATE_FILE}" ]] || die "deployment state not found: ${STATE_FILE}"

    require_registry_state_match '.assets.XLM' xlm_token "XLM asset"
    require_registry_state_match '.assets.USDC' usdc_token "USDC asset"
    require_registry_state_match '.assets.EURC' modeled_asset_eurc "EURC asset"
    require_registry_state_match '.assets.PYUSD' modeled_asset_pyusd "PYUSD asset"
    require_registry_state_match \
        '.modeled_pools.fixed.v2.oracle' modeled_v2_fixed_oracle "Fixed v2 oracle"
    require_registry_state_match \
        '.modeled_pools.custom.v2.oracle' modeled_v2_custom_oracle "Custom v2 oracle"
    require_registry_state_match \
        '.modeled_pools.fixed.v3.oracle' modeled_v3_fixed_oracle "Fixed v3 oracle"
    require_registry_state_match \
        '.modeled_pools.custom.v3.oracle' modeled_v3_custom_oracle "Custom v3 oracle"
    if jq -e '.test_scenarios.reserve_loss' "${REGISTRY}" >/dev/null; then
        require_registry_state_match \
            '.test_scenarios.reserve_loss.contracts.oracle' \
            reserve_loss_oracle "reserve-loss oracle"
    fi
}

oracle_refresh_remaining_seconds() {
    local now last_refresh age

    now="$(date +%s)"
    last_refresh="$(read_last_oracle_refresh_epoch)"
    if (( last_refresh <= 0 || now < last_refresh )); then
        printf '0'
        return
    fi
    age=$((now - last_refresh))
    if (( age >= ORACLE_REFRESH_INTERVAL_SECONDS )); then
        printf '0'
    else
        printf '%s' "$((ORACLE_REFRESH_INTERVAL_SECONDS - age))"
    fi
}

plan_oracle_refresh() {
    local remaining

    if [[ "${NETWORK_PASSPHRASE}" != "${TESTNET_PASSPHRASE}" ]]; then
        note "Fixture-oracle refresh is disabled outside Stellar testnet."
        return
    fi
    remaining="$(oracle_refresh_remaining_seconds)"
    if (( remaining == 0 )); then
        note "Fixture-oracle refresh is due and would run before the emissions pass."
    else
        note "Fixture-oracle refresh is current; next refresh is due in ${remaining} seconds."
    fi
}

refresh_oracles_if_due() {
    local remaining

    if [[ "${NETWORK_PASSPHRASE}" != "${TESTNET_PASSPHRASE}" ]]; then
        note "Fixture-oracle refresh skipped outside Stellar testnet."
        return 0
    fi
    remaining="$(oracle_refresh_remaining_seconds)"
    if (( remaining > 0 )); then
        note "Fixture-oracle refresh is current; next refresh is due in ${remaining} seconds."
        return 0
    fi

    note "Refreshing all modeled v2 and v3 fixture oracles."
    if BLEND_PRODUCTION_TESTNET_RPC_URL="${RPC_URL}" \
        BLEND_PRODUCTION_TESTNET_WORK_DIR="${WORK_DIR}" \
        BLEND_PRODUCTION_TESTNET_CONFIG_DIR="${CONFIG_DIR}" \
        BLEND_PRODUCTION_TESTNET_OPERATOR_IDENTITY="${SOURCE_ACCOUNT}" \
        BLEND_PRODUCTION_TESTNET_INCLUSION_FEE="${INCLUSION_FEE}" \
        bash "${ROOT_DIR}/scripts/testnet-production-migration.sh" refresh-oracle; then
        note "Fixture-oracle refresh completed successfully."
        return 0
    fi
    note "Fixture-oracle refresh failed; it remains due for the next keeper pass."
    return 1
}

stellar_cli() {
    stellar --config-dir "${CONFIG_DIR}" "$@"
}

invoke() {
    local send="$1" contract_id="$2" function_name="$3"
    shift 3

    stellar_cli contract invoke \
        --id "${contract_id}" \
        --source-account "${SOURCE_ACCOUNT}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send "${send}" \
        -- "${function_name}" "$@"
}

capture_invoke() {
    local send="$1" contract_id="$2" function_name="$3" output status
    shift 3

    set +e
    output="$(invoke "${send}" "${contract_id}" "${function_name}" "$@" 2>&1)"
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

is_zero_i128_result() {
    local output="$1"

    # Newer Stellar CLI releases can print diagnostic event lines before a
    # scalar contract return value. The final non-empty line remains the JSON
    # result, so validate that line instead of requiring the entire capture to
    # be a single JSON document.
    printf '%s\n' "${output}" | tail -n 1 | jq -e '
        (type == "number" and . == 0) or
        (type == "string" and . == "0")
    ' >/dev/null 2>&1
}

is_positive_i128_result() {
    local output="$1"

    printf '%s\n' "${output}" | tail -n 1 | jq -e '
        (type == "number" and . > 0) or
        (type == "string" and test("^[1-9][0-9]*$"))
    ' >/dev/null 2>&1
}

read_v3_reserve_assets() {
    local pool_id="$1" output

    if ! output="$(capture_invoke no "${pool_id}" get_reserve_list)"; then
        printf '%s' "${output}"
        return 1
    fi
    printf '%s' "${output}" | jq -er '
        if type == "array" and length > 0 and all(.[]; type == "string") then
            .[]
        else
            error("invalid reserve list")
        end
    '
}

plan_reconcile_losses() {
    local index asset assets output

    for index in "${!V3_ALL_POOLS[@]}"; do
        if ! assets="$(read_v3_reserve_assets "${V3_ALL_POOLS[index]}")"; then
            die "v3 ${V3_ALL_POOL_NAMES[index]} reserve-list simulation failed: ${assets}"
        fi
        while IFS= read -r asset; do
            require_contract_id "${asset}" "v3 reserve asset contract ID"
            if ! output="$(capture_invoke no \
                "${V3_ALL_POOLS[index]}" reconcile_loss --asset "${asset}")"; then
                die "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset} reconciliation simulation failed: ${output}"
            fi
            if is_zero_i128_result "${output}"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset}: custody reconciled"
            elif is_positive_i128_result "${output}"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset}: would reconcile ${output} base units"
            else
                die "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset} reconciliation returned an invalid amount: ${output}"
            fi
        done <<< "${assets}"
    done
}

run_reconcile_losses() {
    local index asset assets output failed=0

    for index in "${!V3_ALL_POOLS[@]}"; do
        if ! assets="$(read_v3_reserve_assets "${V3_ALL_POOLS[index]}")"; then
            note "v3 ${V3_ALL_POOL_NAMES[index]} reserve-list simulation failed: ${assets}"
            failed=1
            continue
        fi
        while IFS= read -r asset; do
            require_contract_id "${asset}" "v3 reserve asset contract ID"
            if ! output="$(capture_invoke no \
                "${V3_ALL_POOLS[index]}" reconcile_loss --asset "${asset}")"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset} reconciliation simulation failed: ${output}"
                failed=1
                continue
            fi
            if is_zero_i128_result "${output}"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset}: custody reconciled"
                continue
            fi
            if ! is_positive_i128_result "${output}"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset} reconciliation returned an invalid amount: ${output}"
                failed=1
                continue
            fi
            if ! output="$(capture_invoke yes \
                "${V3_ALL_POOLS[index]}" reconcile_loss --asset "${asset}")"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset} reconciliation submission failed: ${output}"
                failed=1
                continue
            fi
            if is_positive_i128_result "${output}"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset}: reconciled ${output} base units"
            elif is_zero_i128_result "${output}"; then
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset}: another keeper reconciled first"
            else
                note "v3 ${V3_ALL_POOL_NAMES[index]} reserve ${asset} reconciliation returned an invalid submitted amount: ${output}"
                failed=1
            fi
        done <<< "${assets}"
    done
    return "${failed}"
}

plan_buy_and_burn() {
    local asset output

    for asset in Usdc Xlm; do
        if ! output="$(capture_invoke no "${V3_BACKSTOP_ID}" buy_and_burn --asset "${asset}")"; then
            die "v3 ${asset} buy and burn simulation failed: ${output}"
        fi
        if is_zero_i128_result "${output}"; then
            note "v3 buy and burn: no pending ${asset}"
        elif is_positive_i128_result "${output}"; then
            note "v3 ${asset} buy and burn: ready (${output} BLND)"
        else
            die "v3 ${asset} buy and burn returned an invalid amount: ${output}"
        fi
    done
}

run_buy_and_burn() {
    local asset output failed=0

    for asset in Usdc Xlm; do
        if ! output="$(capture_invoke no "${V3_BACKSTOP_ID}" buy_and_burn --asset "${asset}")"; then
            note "v3 ${asset} buy and burn simulation failed: ${output}"
            failed=1
            continue
        fi
        if is_zero_i128_result "${output}"; then
            note "v3 buy and burn: no pending ${asset}"
            continue
        fi
        if ! is_positive_i128_result "${output}"; then
            note "v3 ${asset} buy and burn returned an invalid amount: ${output}"
            failed=1
            continue
        fi
        if ! output="$(capture_invoke yes "${V3_BACKSTOP_ID}" buy_and_burn --asset "${asset}")"; then
            note "v3 ${asset} buy and burn submission failed: ${output}"
            failed=1
            continue
        fi
        note "v3 ${asset} buy and burn: submitted (${output} BLND)"
    done
    return "${failed}"
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
    done < <(jq -er '
        .modeled_pools | to_entries[] |
        select(.value.v2.reward_zone_member == true) |
        [.key, .value.v2.pool] | @tsv
    ' "${REGISTRY}")

    V3_POOL_NAMES=()
    V3_POOLS=()
    while IFS=$'\t' read -r pool_name pool_id; do
        require_contract_id "${pool_id}" "modeled v3 pool contract ID"
        V3_POOL_NAMES+=("${pool_name}")
        V3_POOLS+=("${pool_id}")
    done < <(jq -er '
        (
            .modeled_pools | to_entries[] |
            select(.value.v3.reward_zone_member == true) |
            [.key, .value.v3.pool]
        ),
        (
            .test_scenarios.reserve_loss? |
            select(.backstop.reward_zone_member == true) |
            ["reserve_loss", .contracts.pool]
        ) | @tsv
    ' "${REGISTRY}")

    V3_ALL_POOL_NAMES=()
    V3_ALL_POOLS=()
    while IFS=$'\t' read -r pool_name pool_id; do
        require_contract_id "${pool_id}" "modeled v3 pool contract ID"
        V3_ALL_POOL_NAMES+=("${pool_name}")
        V3_ALL_POOLS+=("${pool_id}")
    done < <(jq -er '
        (.modeled_pools | to_entries[] | [.key, .value.v3.pool]),
        (.test_scenarios.reserve_loss? | ["reserve_loss", .contracts.pool]) |
        @tsv
    ' "${REGISTRY}")

    (( ${#V2_POOLS[@]} > 0 )) || die "deployment registry contains no modeled v2 pools"
    (( ${#V3_POOLS[@]} > 0 )) || die "deployment registry contains no modeled v3 pools"
    (( ${#V3_ALL_POOLS[@]} > 0 )) || die "deployment registry contains no modeled v3 pools"
    validate_oracle_refresh_configuration
}

plan_pass() {
    local index emitter_recipient

    plan_oracle_refresh
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
    plan_reconcile_losses
    plan_call "v3 checkpoint protocol emissions" "${V3_BACKSTOP_ID}" distribute 1030
    for index in "${!V3_POOLS[@]}"; do
        plan_call \
            "v3 gulp ${V3_POOL_NAMES[index]} pool ${V3_POOLS[index]}" \
            "${V3_POOLS[index]}" gulp_emissions 1034 1200
    done
    plan_buy_and_burn
    note "Plan only: pool simulations reflect current state before either simulated distribution."
}

run_pass() {
    local index emitter_recipient failed=0

    refresh_oracles_if_due || failed=1
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
    run_reconcile_losses || failed=1
    run_call "v3 checkpoint protocol emissions" "${V3_BACKSTOP_ID}" distribute 1030 || failed=1
    for index in "${!V3_POOLS[@]}"; do
        run_call \
            "v3 gulp ${V3_POOL_NAMES[index]} pool ${V3_POOLS[index]}" \
            "${V3_POOLS[index]}" gulp_emissions 1034 1200 || failed=1
    done
    run_buy_and_burn || failed=1

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
    note "Keeper started; emissions interval is ${INTERVAL_SECONDS} seconds and fixture-oracle refresh interval is ${ORACLE_REFRESH_INTERVAL_SECONDS} seconds."
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
    require_positive_integer \
        "${ORACLE_REFRESH_INTERVAL_SECONDS}" \
        "BLEND_KEEPER_ORACLE_REFRESH_INTERVAL_SECONDS"
    load_configuration
    export STELLAR_INCLUSION_FEE="${INCLUSION_FEE}"

    case "${command}" in
        plan) plan_pass ;;
        once) run_pass ;;
        run) run_forever ;;
    esac
}

main "$@"
