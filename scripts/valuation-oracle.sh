#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NETWORK_MODE="${BLEND_ORACLE_NETWORK:-testnet}"

BLND_CLASSIC_ASSET_DEFAULT="BLND:GDJEHTBE6ZHUXSWFI642DCGLUOECLHPF3KSXHPXTSTJ7E3JF6MQ5EZYY"
USDC_CLASSIC_ASSET_DEFAULT="USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
XLM_CLASSIC_ASSET_DEFAULT="native"

case "${NETWORK_MODE}" in
    testnet)
        NETWORK_LABEL="Public testnet"
        RPC_URL="${BLEND_ORACLE_RPC_URL:-https://soroban-testnet.stellar.org}"
        NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
        ORACLE_ID="${BLEND_ORACLE_ID:-CAVLP5DH2GJPZMVO7IJY4CVOD5MWEFTJFVPD2YY2FQXOQHRGHK4D6HLP}"
        READ_SOURCE="${BLEND_ORACLE_READ_SOURCE:-GCLA2E3LQDPAPJLHYDMB5R65ASGLNXWGJCX4TX7XA75C7VTJ7Y2OTZXA}"
        ;;
    mainnet)
        NETWORK_LABEL="Public mainnet"
        RPC_URL="${BLEND_ORACLE_RPC_URL:-https://mainnet.sorobanrpc.com}"
        NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
        ORACLE_ID="${BLEND_ORACLE_ID:-CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M}"
        READ_SOURCE="${BLEND_ORACLE_READ_SOURCE:-GC2M2UTZ57GSENZBTPMLPB6QSAL2USIPHMXSUCT2I3V24GYWF3EI6RFA}"
        ;;
    *)
        printf 'error: unsupported BLEND_ORACLE_NETWORK: %s\n' "${NETWORK_MODE}" >&2
        exit 2
        ;;
esac

WORK_DIR="${BLEND_ORACLE_WORK_DIR:-${ROOT_DIR}/.oracle-validation/${NETWORK_MODE}}"
CONFIG_DIR="${BLEND_ORACLE_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${WORK_DIR}/state.json"
ADAPTER_WASM="${ROOT_DIR}/blend-contracts-v3/target/wasm32v1-none/optimized/blend_v3_backstop_valuation.wasm"

BLND_CLASSIC_ASSET="${BLEND_ORACLE_BLND_CLASSIC_ASSET:-${BLND_CLASSIC_ASSET_DEFAULT}}"
USDC_CLASSIC_ASSET="${BLEND_ORACLE_USDC_CLASSIC_ASSET:-${USDC_CLASSIC_ASSET_DEFAULT}}"
XLM_CLASSIC_ASSET="${BLEND_ORACLE_XLM_CLASSIC_ASSET:-${XLM_CLASSIC_ASSET_DEFAULT}}"
TWAP_RECORDS="${BLEND_ORACLE_TWAP_RECORDS:-7}"
MAX_PRICE_AGE="${BLEND_ORACLE_MAX_PRICE_AGE:-600}"

RUN_DIR=""
BLND_ID=""
USDC_ID=""
XLM_ID=""
FINDINGS=()

usage() {
    cat <<'EOF'
Usage: scripts/valuation-oracle.sh COMMAND

Commands:
  plan    Print the immutable provider and adapter configuration.
  check   Record provider metadata, history, WASM, costs, and compatibility.
  deploy  Check compatibility, then deploy the production valuation adapter.
  verify  Recheck the provider and simulate adapter identity and four quotes.

The default provider is the official Reflector Stellar-DEX feed for the
selected network. The check is read-only. Deploy requires these variables:

  BLEND_ORACLE_DEPLOY_SOURCE       funded Stellar CLI identity or secret
  BLEND_ORACLE_BLND_USDC_COMET     exact production Comet contract ID
  BLEND_ORACLE_BLND_XLM_COMET      exact production Comet contract ID

Classic-asset descriptors or resolved contract IDs can be overridden with:

  BLEND_ORACLE_{BLND,USDC,XLM}_CLASSIC_ASSET
  BLEND_ORACLE_{BLND,USDC,XLM}_CONTRACT

Set BLEND_ORACLE_NETWORK to testnet (default) or mainnet. All generated
artifacts are isolated below .oracle-validation/ by default and do not touch
the migration runner's .testnet/state.json.
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

require_contract_id() {
    [[ "$1" =~ ^C[A-Z2-7]{55}$ ]] || die "invalid contract ID for $2: $1"
}

sha256_file() {
    local value

    value="$(shasum -a 256 "$1")"
    printf '%s\n' "${value%% *}"
}

stellar_cli() {
    stellar --config-dir "${CONFIG_DIR}" "$@"
}

stellar_simulation_cli() {
    stellar \
        --config-dir "${CONFIG_DIR}" \
        --filter-logs soroban_cli::assembled=trace \
        "$@"
}

validate_tools() {
    local version_text version_line version major

    require_command cargo
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

validate_integer_config() {
    [[ "${TWAP_RECORDS}" =~ ^[0-9]+$ ]] ||
        die "BLEND_ORACLE_TWAP_RECORDS must be an unsigned integer"
    [[ "${MAX_PRICE_AGE}" =~ ^[0-9]+$ ]] ||
        die "BLEND_ORACLE_MAX_PRICE_AGE must be an unsigned integer"
    ((TWAP_RECORDS >= 2 && TWAP_RECORDS <= 25)) ||
        die "TWAP record count must be from 2 through 25"
    ((MAX_PRICE_AGE <= 3600)) ||
        die "maximum price age must not exceed 3600 seconds"
}

resolve_asset_contract() {
    local explicit_id="$1" classic_asset="$2" label="$3" resolved

    if [[ -n "${explicit_id}" ]]; then
        require_contract_id "${explicit_id}" "${label}"
        printf '%s\n' "${explicit_id}"
        return
    fi
    resolved="$(
        stellar_cli contract id asset \
            --asset "${classic_asset}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}"
    )"
    require_contract_id "${resolved}" "${label}"
    printf '%s\n' "${resolved}"
}

resolve_assets() {
    BLND_ID="$(
        resolve_asset_contract \
            "${BLEND_ORACLE_BLND_CONTRACT:-}" "${BLND_CLASSIC_ASSET}" BLND
    )"
    USDC_ID="$(
        resolve_asset_contract \
            "${BLEND_ORACLE_USDC_CONTRACT:-}" "${USDC_CLASSIC_ASSET}" USDC
    )"
    XLM_ID="$(
        resolve_asset_contract \
            "${BLEND_ORACLE_XLM_CONTRACT:-}" "${XLM_CLASSIC_ASSET}" XLM
    )"
}

new_run_dir() {
    local run_stamp suffix

    run_stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    RUN_DIR="${WORK_DIR}/runs/${run_stamp}"
    suffix=2
    while [[ -e "${RUN_DIR}" ]]; do
        RUN_DIR="${WORK_DIR}/runs/${run_stamp}-${suffix}"
        suffix=$((suffix + 1))
    done
    mkdir -p "${RUN_DIR}" "${CONFIG_DIR}"
}

capture() {
    local label="$1" stdout_file="${RUN_DIR}/$1.out"
    local stderr_file="${RUN_DIR}/$1.cost.log" command_status
    shift

    note "Executing ${label}..."
    if "$@" >"${stdout_file}" 2>"${stderr_file}"; then
        command_status=0
    else
        command_status=$?
    fi
    cat "${stderr_file}" >&2
    ((command_status == 0)) || return "${command_status}"
    cat "${stdout_file}"
}

capture_quiet() {
    local label="$1" stdout_file="${RUN_DIR}/$1.out"
    local stderr_file="${RUN_DIR}/$1.cost.log" command_status
    shift

    note "Executing ${label}..."
    if "$@" >"${stdout_file}" 2>"${stderr_file}"; then
        command_status=0
    else
        command_status=$?
    fi
    ((command_status == 0)) || return "${command_status}"
    cat "${stdout_file}"
}

oracle_call() {
    local label="$1"
    shift

    capture_quiet "${label}" \
        stellar_simulation_cli contract invoke \
        --id "${ORACLE_ID}" \
        --source-account "${READ_SOURCE}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send no \
        --cost \
        -- "$@"
}

record_finding() {
    FINDINGS+=("$1")
    note "incompatible: $1"
}

read_scalar() {
    jq -er '
        if type == "string" or type == "number" or type == "boolean"
        then tostring
        else empty
        end
    ' "$1"
}

read_single_json() {
    local path="$1" fallback="$2" decoded

    if decoded="$(
        jq -sc '
            if length == 1
            then .[0]
            else error("expected exactly one JSON value")
            end
        ' "${path}" 2>/dev/null
    )"
    then
        printf '%s\n' "${decoded}"
    else
        printf '%s\n' "${fallback}"
    fi
}

validate_history() {
    local label="$1" path="$2" now

    now="$(date -u +%s)"
    if ! jq -e --argjson records "${TWAP_RECORDS}" \
        --argjson resolution "$3" \
        --argjson max_age "${MAX_PRICE_AGE}" \
        --argjson now "${now}" '
            type == "array"
            and length == $records
            and all(.[];
                (.price | tonumber) > 0
                and (.timestamp | tonumber) <= $now
                and ((.timestamp | tonumber) % $resolution) == 0
            )
            and ([.[].timestamp | tonumber] | unique | length) == $records
            and (
                ([.[].timestamp | tonumber] | max)
                - ([.[].timestamp | tonumber] | min)
            ) == ($resolution * ($records - 1))
            and (
                $now - ([.[].timestamp | tonumber] | max)
            ) <= $max_age
        ' "${path}" >/dev/null
    then
        record_finding \
            "${label} history is missing, stale, malformed, or not exactly ${TWAP_RECORDS} uniform records"
    fi
}

write_check_summary() {
    local status="$1" base_json="$2" assets_json="$3"
    local decimals="$4" resolution="$5" version_json="$6" admin_json="$7"
    local provider_hash="$8" findings_json source_commit source_dirty

    if ((${#FINDINGS[@]} == 0)); then
        findings_json='[]'
    else
        findings_json="$(
            printf '%s\n' "${FINDINGS[@]}" |
                jq -Rsc 'split("\n") | map(select(length > 0))'
        )"
    fi
    source_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    if [[ -n "$(git -C "${ROOT_DIR}" status --short)" ]]; then
        source_dirty=true
    else
        source_dirty=false
    fi

    jq -n \
        --arg status "${status}" \
        --arg checked_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg network "${NETWORK_MODE}" \
        --arg network_label "${NETWORK_LABEL}" \
        --arg rpc_url "${RPC_URL}" \
        --arg oracle "${ORACLE_ID}" \
        --arg read_source "${READ_SOURCE}" \
        --arg blnd "${BLND_ID}" \
        --arg usdc "${USDC_ID}" \
        --arg xlm "${XLM_ID}" \
        --argjson base "${base_json}" \
        --argjson assets "${assets_json}" \
        --arg decimals "${decimals}" \
        --arg resolution "${resolution}" \
        --argjson version "${version_json}" \
        --argjson admin "${admin_json}" \
        --arg provider_wasm_sha256 "${provider_hash}" \
        --argjson twap_records "${TWAP_RECORDS}" \
        --argjson max_price_age "${MAX_PRICE_AGE}" \
        --arg source_commit "${source_commit}" \
        --argjson source_dirty "${source_dirty}" \
        --arg run_dir "${RUN_DIR}" \
        --argjson findings "${findings_json}" \
        '{
            status: $status,
            checked_at: $checked_at,
            network: $network,
            network_label: $network_label,
            rpc_url: $rpc_url,
            oracle: $oracle,
            read_source: $read_source,
            assets: {
                blnd: $blnd,
                usdc: $usdc,
                xlm: $xlm
            },
            oracle_metadata: {
                base: $base,
                assets: $assets,
                decimals: ($decimals | tonumber? // $decimals),
                resolution: ($resolution | tonumber? // $resolution),
                version: $version,
                admin: $admin,
                wasm_sha256: $provider_wasm_sha256
            },
            policy: {
                twap_records: $twap_records,
                max_price_age: $max_price_age
            },
            source_commit: $source_commit,
            source_dirty: $source_dirty,
            run_dir: $run_dir,
            findings: $findings
        }' >"${RUN_DIR}/summary.json"
}

run_check() {
    local base_json assets_json decimals resolution version_json admin_json
    local provider_hash="" history_span status

    FINDINGS=()
    validate_tools
    validate_integer_config
    require_contract_id "${ORACLE_ID}" oracle
    new_run_dir
    resolve_assets

    if ! capture "fetch-oracle-wasm" \
        stellar_cli contract fetch \
        --id "${ORACLE_ID}" \
        --out-file "${RUN_DIR}/oracle.wasm" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}"
    then
        record_finding "the exact provider WASM could not be fetched"
    elif [[ -s "${RUN_DIR}/oracle.wasm" ]]; then
        provider_hash="$(sha256_file "${RUN_DIR}/oracle.wasm")"
    else
        record_finding "the fetched provider WASM is empty"
    fi

    if ! oracle_call "oracle-base" base >/dev/null; then
        record_finding "base() invocation failed"
        printf 'null\n' >"${RUN_DIR}/oracle-base.out"
    fi
    if ! oracle_call "oracle-assets" assets >/dev/null; then
        record_finding "assets() invocation failed"
        printf '[]\n' >"${RUN_DIR}/oracle-assets.out"
    fi
    if ! oracle_call "oracle-decimals" decimals >/dev/null; then
        record_finding "decimals() invocation failed"
        printf 'null\n' >"${RUN_DIR}/oracle-decimals.out"
    fi
    if ! oracle_call "oracle-resolution" resolution >/dev/null; then
        record_finding "resolution() invocation failed"
        printf 'null\n' >"${RUN_DIR}/oracle-resolution.out"
    fi
    if ! oracle_call "oracle-version" version >/dev/null; then
        printf 'null\n' >"${RUN_DIR}/oracle-version.out"
    fi
    if ! oracle_call "oracle-admin" admin >/dev/null; then
        printf 'null\n' >"${RUN_DIR}/oracle-admin.out"
    fi
    if ! oracle_call "oracle-blnd-prices" prices \
        --asset "{\"Stellar\":\"${BLND_ID}\"}" \
        --records "${TWAP_RECORDS}" >/dev/null
    then
        record_finding "BLND prices() invocation failed"
        printf 'null\n' >"${RUN_DIR}/oracle-blnd-prices.out"
    fi
    if ! oracle_call "oracle-xlm-prices" prices \
        --asset "{\"Stellar\":\"${XLM_ID}\"}" \
        --records "${TWAP_RECORDS}" >/dev/null
    then
        record_finding "XLM prices() invocation failed"
        printf 'null\n' >"${RUN_DIR}/oracle-xlm-prices.out"
    fi

    base_json="$(read_single_json "${RUN_DIR}/oracle-base.out" null)"
    assets_json="$(read_single_json "${RUN_DIR}/oracle-assets.out" '[]')"
    decimals="$(read_scalar "${RUN_DIR}/oracle-decimals.out" 2>/dev/null || printf '')"
    resolution="$(read_scalar "${RUN_DIR}/oracle-resolution.out" 2>/dev/null || printf '')"
    version_json="$(read_single_json "${RUN_DIR}/oracle-version.out" null)"
    admin_json="$(read_single_json "${RUN_DIR}/oracle-admin.out" null)"

    [[ "${base_json}" == "{\"Stellar\":\"${USDC_ID}\"}" ]] ||
        record_finding "oracle base is not the configured USDC contract"
    jq -e --arg asset "${BLND_ID}" \
        'type == "array" and any(.[]; .Stellar? == $asset)' \
        "${RUN_DIR}/oracle-assets.out" >/dev/null 2>&1 ||
        record_finding "oracle assets do not advertise BLND"
    jq -e --arg asset "${XLM_ID}" \
        'type == "array" and any(.[]; .Stellar? == $asset)' \
        "${RUN_DIR}/oracle-assets.out" >/dev/null 2>&1 ||
        record_finding "oracle assets do not advertise XLM"

    if [[ ! "${decimals}" =~ ^[0-9]+$ ]] || ((decimals > 18)); then
        record_finding "oracle decimals are missing or exceed 18"
    fi
    if [[ ! "${resolution}" =~ ^[0-9]+$ ]] || ((resolution == 0)); then
        record_finding "oracle resolution is missing or zero"
    else
        history_span=$((resolution * (TWAP_RECORDS - 1)))
        ((history_span >= 1800 && history_span <= 86400)) ||
            record_finding "TWAP history span is outside 30 minutes through 24 hours"
        ((MAX_PRICE_AGE >= resolution)) ||
            record_finding "maximum price age is shorter than one oracle resolution"
        validate_history BLND "${RUN_DIR}/oracle-blnd-prices.out" "${resolution}"
        validate_history XLM "${RUN_DIR}/oracle-xlm-prices.out" "${resolution}"
    fi

    if ((${#FINDINGS[@]} == 0)); then
        status="compatible"
    else
        status="incompatible"
    fi
    write_check_summary \
        "${status}" "${base_json}" "${assets_json}" "${decimals}" "${resolution}" \
        "${version_json}" "${admin_json}" "${provider_hash}"

    note "Provider compatibility: ${status}"
    note "Evidence: ${RUN_DIR}/summary.json"
    if [[ "${status}" == "compatible" ]]; then
        return 0
    fi
    return 1
}

build_adapter() {
    note "Building the optimized production valuation adapter..."
    (
        cd "${ROOT_DIR}"
        make build-v3
    )
    [[ -s "${ADAPTER_WASM}" ]] ||
        die "missing optimized production adapter: ${ADAPTER_WASM}"
}

deploy_adapter() {
    local deploy_source blnd_usdc blnd_xlm raw adapter_id temporary
    local initial_provider_hash predeploy_provider_hash

    deploy_source="${BLEND_ORACLE_DEPLOY_SOURCE:-}"
    blnd_usdc="${BLEND_ORACLE_BLND_USDC_COMET:-}"
    blnd_xlm="${BLEND_ORACLE_BLND_XLM_COMET:-}"
    [[ -n "${deploy_source}" ]] ||
        die "BLEND_ORACLE_DEPLOY_SOURCE is required for deployment"
    require_contract_id "${blnd_usdc}" BLND:USDC-Comet
    require_contract_id "${blnd_xlm}" BLND:XLM-Comet
    [[ ! -e "${STATE_FILE}" ]] ||
        die "adapter deployment state already exists: ${STATE_FILE}"

    if ! run_check; then
        die "provider is incompatible; no deployment transaction was submitted"
    fi
    initial_provider_hash="$(jq -er '.oracle_metadata.wasm_sha256' "${RUN_DIR}/summary.json")"
    build_adapter
    if ! run_check; then
        die "provider failed its immediate predeployment recheck; no transaction was submitted"
    fi
    predeploy_provider_hash="$(jq -er '.oracle_metadata.wasm_sha256' "${RUN_DIR}/summary.json")"
    [[ "${predeploy_provider_hash}" == "${initial_provider_hash}" ]] ||
        die "provider WASM changed during qualification; no transaction was submitted"
    raw="$(
        capture "deploy-valuation-adapter" \
            stellar_simulation_cli contract deploy \
            --wasm "${ADAPTER_WASM}" \
            --optimize=false \
            --source-account "${deploy_source}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost \
            -- \
            --oracle "${ORACLE_ID}" \
            --oracle_base "{\"Stellar\":\"${USDC_ID}\"}" \
            --blnd "${BLND_ID}" \
            --usdc "${USDC_ID}" \
            --xlm "${XLM_ID}" \
            --blnd_usdc "${blnd_usdc}" \
            --blnd_xlm "${blnd_xlm}" \
            --twap_records "${TWAP_RECORDS}" \
            --max_price_age "${MAX_PRICE_AGE}"
    )"
    adapter_id="$(printf '%s' "${raw}" | jq -er 'tostring' 2>/dev/null || printf '%s' "${raw}")"
    require_contract_id "${adapter_id}" deployed-adapter

    mkdir -p "${WORK_DIR}"
    temporary="${STATE_FILE}.tmp"
    jq -n \
        --arg network "${NETWORK_MODE}" \
        --arg rpc_url "${RPC_URL}" \
        --arg oracle "${ORACLE_ID}" \
        --arg adapter "${adapter_id}" \
        --arg blnd "${BLND_ID}" \
        --arg usdc "${USDC_ID}" \
        --arg xlm "${XLM_ID}" \
        --arg blnd_usdc "${blnd_usdc}" \
        --arg blnd_xlm "${blnd_xlm}" \
        --argjson twap_records "${TWAP_RECORDS}" \
        --argjson max_price_age "${MAX_PRICE_AGE}" \
        --arg adapter_wasm_sha256 "$(sha256_file "${ADAPTER_WASM}")" \
        --arg provider_wasm_sha256 "$(sha256_file "${RUN_DIR}/oracle.wasm")" \
        --arg run_dir "${RUN_DIR}" \
        --arg deployed_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        '{
            network: $network,
            rpc_url: $rpc_url,
            oracle: $oracle,
            adapter: $adapter,
            blnd: $blnd,
            usdc: $usdc,
            xlm: $xlm,
            blnd_usdc: $blnd_usdc,
            blnd_xlm: $blnd_xlm,
            twap_records: $twap_records,
            max_price_age: $max_price_age,
            adapter_wasm_sha256: $adapter_wasm_sha256,
            provider_wasm_sha256: $provider_wasm_sha256,
            run_dir: $run_dir,
            deployed_at: $deployed_at
        }' >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
    note "Production valuation adapter deployed: ${adapter_id}"
    note "State: ${STATE_FILE}"
}

verify_adapter() {
    local adapter blnd_usdc blnd_xlm amount binding config version
    local saved_network saved_rpc saved_oracle saved_provider_hash current_provider_hash
    local saved_adapter_hash current_adapter_hash

    [[ -f "${STATE_FILE}" ]] ||
        die "deployment state not found; run deploy first"
    saved_network="$(jq -er '.network' "${STATE_FILE}")"
    saved_rpc="$(jq -er '.rpc_url' "${STATE_FILE}")"
    saved_oracle="$(jq -er '.oracle' "${STATE_FILE}")"
    saved_provider_hash="$(jq -er '.provider_wasm_sha256' "${STATE_FILE}")"
    saved_adapter_hash="$(jq -er '.adapter_wasm_sha256' "${STATE_FILE}")"
    [[ "${saved_network}" == "${NETWORK_MODE}" ]] ||
        die "saved network ${saved_network} does not match ${NETWORK_MODE}"
    [[ "${saved_rpc}" == "${RPC_URL}" ]] ||
        die "saved RPC ${saved_rpc} does not match ${RPC_URL}"
    [[ "${saved_oracle}" == "${ORACLE_ID}" ]] ||
        die "saved oracle ${saved_oracle} does not match ${ORACLE_ID}"
    adapter="$(jq -er '.adapter' "${STATE_FILE}")"
    BLEND_ORACLE_BLND_CONTRACT="$(jq -er '.blnd' "${STATE_FILE}")"
    BLEND_ORACLE_USDC_CONTRACT="$(jq -er '.usdc' "${STATE_FILE}")"
    BLEND_ORACLE_XLM_CONTRACT="$(jq -er '.xlm' "${STATE_FILE}")"
    blnd_usdc="$(jq -er '.blnd_usdc' "${STATE_FILE}")"
    blnd_xlm="$(jq -er '.blnd_xlm' "${STATE_FILE}")"
    TWAP_RECORDS="$(jq -er '.twap_records' "${STATE_FILE}")"
    MAX_PRICE_AGE="$(jq -er '.max_price_age' "${STATE_FILE}")"
    amount="${BLEND_ORACLE_QUOTE_AMOUNT:-10000000}"
    [[ "${amount}" =~ ^[1-9][0-9]*$ ]] ||
        die "BLEND_ORACLE_QUOTE_AMOUNT must be a positive integer"

    if ! run_check; then
        die "provider no longer passes compatibility checks"
    fi
    current_provider_hash="$(jq -er '.oracle_metadata.wasm_sha256' "${RUN_DIR}/summary.json")"
    [[ "${current_provider_hash}" == "${saved_provider_hash}" ]] ||
        die "provider WASM changed from ${saved_provider_hash} to ${current_provider_hash}"
    capture_quiet "fetch-adapter-wasm" \
        stellar_cli contract fetch \
        --id "${adapter}" \
        --out-file "${RUN_DIR}/adapter.wasm" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    current_adapter_hash="$(sha256_file "${RUN_DIR}/adapter.wasm")"
    [[ "${current_adapter_hash}" == "${saved_adapter_hash}" ]] ||
        die "adapter WASM changed from ${saved_adapter_hash} to ${current_adapter_hash}"
    binding="$(
        capture_quiet "adapter-binding" \
            stellar_simulation_cli contract invoke \
            --id "${adapter}" \
            --source-account "${READ_SOURCE}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --send no \
            --cost \
            -- binding
    )"
    config="$(
        capture_quiet "adapter-config" \
            stellar_simulation_cli contract invoke \
            --id "${adapter}" \
            --source-account "${READ_SOURCE}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --send no \
            --cost \
            -- config
    )"
    version="$(
        capture_quiet "adapter-version" \
            stellar_simulation_cli contract invoke \
            --id "${adapter}" \
            --source-account "${READ_SOURCE}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --send no \
            --cost \
            -- version
    )"

    jq -e \
        --arg blnd "${BLND_ID}" \
        --arg usdc "${USDC_ID}" \
        --arg blnd_usdc "${blnd_usdc}" \
        --arg blnd_xlm "${blnd_xlm}" '
            .blnd == $blnd
            and .usdc == $usdc
            and .blnd_usdc == $blnd_usdc
            and .blnd_xlm == $blnd_xlm
        ' <<<"${binding}" >/dev/null ||
        die "deployed adapter binding does not match saved assets"
    jq -e \
        --arg oracle "${ORACLE_ID}" \
        --arg blnd "${BLND_ID}" \
        --arg usdc "${USDC_ID}" \
        --arg xlm "${XLM_ID}" \
        --argjson records "${TWAP_RECORDS}" \
        --argjson age "${MAX_PRICE_AGE}" '
            .oracle == $oracle
            and .blnd == $blnd
            and .usdc == $usdc
            and .xlm == $xlm
            and .twap_records == $records
            and .max_price_age == $age
        ' <<<"${config}" >/dev/null ||
        die "deployed adapter configuration does not match the requested policy"
    [[ "$(printf '%s' "${version}" | jq -r 'tostring')" == "1" ]] ||
        die "deployed adapter version is not 1"

    for token in "${blnd_usdc}" "${blnd_xlm}"; do
        capture_quiet "adapter-quote-${token}-active" \
            stellar_simulation_cli contract invoke \
            --id "${adapter}" \
            --source-account "${READ_SOURCE}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --send no \
            --cost \
            -- quote \
            --token "${token}" \
            --amount "${amount}" >/dev/null
        capture_quiet "adapter-quote-${token}-queued" \
            stellar_simulation_cli contract invoke \
            --id "${adapter}" \
            --source-account "${READ_SOURCE}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --send no \
            --cost \
            -- quote \
            --token "${token}" \
            --amount "${amount}" >/dev/null
    done
    note "Adapter identity and four production-oracle quote simulations passed."
    note "Evidence: ${RUN_DIR}"
}

print_plan() {
    validate_integer_config
    cat <<EOF
Valuation-oracle deployment plan

  Network:                 ${NETWORK_LABEL}
  RPC:                     ${RPC_URL}
  Oracle:                  ${ORACLE_ID}
  BLND classic asset:      ${BLND_CLASSIC_ASSET}
  USDC classic asset:      ${USDC_CLASSIC_ASSET}
  XLM classic asset:       ${XLM_CLASSIC_ASSET}
  TWAP observations:       ${TWAP_RECORDS}
  Maximum price age:       ${MAX_PRICE_AGE} seconds
  Evidence directory:      ${WORK_DIR}

The check fetches and hashes the exact provider WASM, verifies the USDC base,
BLND/XLM membership, precision, resolution, exact uniform history, freshness,
and records CLI cost output. Deployment is blocked unless all checks pass and
never reads or writes the migration runner's .testnet/state.json.
EOF
}

case "${1:-}" in
    plan)
        print_plan
        ;;
    check)
        if run_check; then
            exit 0
        fi
        exit 1
        ;;
    deploy)
        deploy_adapter
        ;;
    verify)
        verify_adapter
        ;;
    -h | --help | help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
