#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${BLEND_MAINNET_COMET_WORK_DIR:-${ROOT_DIR}/.mainnet-v3/comet}"
CONFIG_DIR="${BLEND_MAINNET_COMET_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${BLEND_MAINNET_COMET_STATE_FILE:-${WORK_DIR}/state.json}"
REGISTRY_FILE="${BLEND_MAINNET_COMET_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-comet-v1.1.json}"
ACCOUNT_CONFIG="${BLEND_MAINNET_ACCOUNT_CONFIG:-/Users/donovan/mainnet.conf}"
RPC_URL="${BLEND_MAINNET_RPC_URL:-https://soroban-rpc.creit.tech}"
HORIZON_URL="${BLEND_MAINNET_HORIZON_URL:-https://horizon.stellar.org}"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
INCLUSION_FEE="${BLEND_MAINNET_INCLUSION_FEE:-100000}"

COMET_WASM="${ROOT_DIR}/blnt-contracts-v3/comet.wasm"
COMET_RELEASE_URL="https://github.com/CometDEX/comet-contracts-v1/releases/download/v1.1.0_contracts_pkg1.0.0_cli25.1.0/contracts_v1.0.0.wasm"
COMET_SHA256="d735c3395f59510172cf5cf838823a1389b97cfec6bf24e580e9bd77d2b3e687"
COMET_SOURCE_COMMIT="b3a2d827ed6c158341f8e67ea91eb3b0395d3871"

WALLET="GDEA2WMOWYQ3T56A2PZH5QYYEQBAORW2AUUJYX4FFD5GDPESAICTFTAL"
BLNT_ISSUER="GBRDT5KNVHGUNRK432N2HDQLXLA33KAFXSNXLUUFZOZEJF2C76HI4UZC"
USDC_ISSUER="GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
BLNT_ASSET="BLNT:${BLNT_ISSUER}"
USDC_ASSET="USDC:${USDC_ISSUER}"
BLNT="CBZAURTT3AEFWR6DFMDA5U6RD72HYRXSPHUM3C3JRDW4LNVDQRKTRQLQ"
USDC="CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
XLM="CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
LEGACY_BLND="CD25MNVTZDL4Y3XBCPCJXGXATV5WUHHOWMYFF4YBEGU5FCPGMYTVG5JY"
LEGACY_BLND_USDC_COMET="CASYAKK3SHDN6S2E6IUOBVRXT3KKPVQCVQO4GNBDTVE6XZQQMW4UMIHE"

BLNT_USDC_SALT="d0dd5db736b4ddef9e4e15e8bfd4dab4d3396396b0396c54bd759c2f1cd2db2f"
BLNT_XLM_SALT="bf617a2afd2946f3e764e89d1a83f706edfeef209922dff1a8ad6c590c87ace3"
CONTROLLER_IDENTITY="blnt-v3-mainnet-comet-controller"
CONTROLLER_STARTING_XLM=250000000
CONTROLLER_THRESHOLD=100

SCALAR_7=10000000
WEIGHT_BLNT=8000000
WEIGHT_PAIR=2000000
SWAP_FEE=30000
TARGET_BLNT_PRICE=47000
MIN_BLND_DISCOUNT_BPS=900

INIT_BLNT=10000000000
INIT_USDC=11750000
INIT_XLM=70000000
JOIN_BLNT=9990000000000
JOIN_USDC=11738250000
JOIN_XLM=69930000000
JOIN_LP=999000000000
INITIAL_LP=1000000000
FINAL_LP=1000000000000
FINAL_BLNT=10000000000000
FINAL_USDC=11750000000
FINAL_XLM=70000000000

RUN_DIR=""
LAST_TX_HASH=""
STELLAR_PUBLIC_KEY=""
STELLAR_SECRET_KEY=""

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

sha256_file() {
    local value
    value="$(shasum -a 256 "$1")"
    printf '%s\n' "${value%% *}"
}

stellar_cli() {
    STELLAR_INCLUSION_FEE="${INCLUSION_FEE}" \
        stellar --config-dir "${CONFIG_DIR}" "$@"
}

wallet_stellar() {
    [[ -n "${STELLAR_SECRET_KEY}" ]] || die "mainnet wallet secret is not loaded"
    STELLAR_SIGN_WITH_KEY="${STELLAR_SECRET_KEY}" \
        STELLAR_INCLUSION_FEE="${INCLUSION_FEE}" \
        stellar --config-dir "${CONFIG_DIR}" "$@"
}

load_wallet_config() {
    [[ -f "${ACCOUNT_CONFIG}" ]] || die "mainnet account config is missing: ${ACCOUNT_CONFIG}"
    # shellcheck disable=SC1090
    source "${ACCOUNT_CONFIG}"
    [[ "${STELLAR_PUBLIC_KEY:-}" == "${WALLET}" ]] ||
        die "mainnet account config does not identify the approved deployment wallet"
    [[ "${STELLAR_SECRET_KEY:-}" =~ ^S[A-Z2-7]{55}$ ]] ||
        die "mainnet account config does not contain a valid secret key"
}

capture() {
    local label="$1" stdout_file stderr_file status
    shift
    [[ -n "${RUN_DIR}" ]] || die "run directory is not initialized"
    stdout_file="${RUN_DIR}/${label}.out"
    stderr_file="${RUN_DIR}/${label}.cost.log"
    note "Executing ${label}..."
    if "$@" >"${stdout_file}" 2> >(tee "${stderr_file}" >&2); then
        status=0
    else
        status=$?
    fi
    LAST_TX_HASH="$(sed -n 's/.*Signing transaction: *\([0-9a-f]\{64\}\).*/\1/p' \
        "${stderr_file}" | tail -1)"
    cat "${stdout_file}"
    return "${status}"
}

state_exists() {
    [[ -f "${STATE_FILE}" ]]
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

record_transaction() {
    local label="$1" temporary="${STATE_FILE}.tmp"
    [[ "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]] ||
        die "${label} completed without a captured transaction hash"
    jq --arg label "${label}" --arg hash "${LAST_TX_HASH}" \
        '.transactions[$label] = $hash' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
}

initialize_state() {
    local controller blnt_usdc blnt_xlm source_commit

    mkdir -p "${WORK_DIR}" "${CONFIG_DIR}"
    chmod 700 "${WORK_DIR}" "${CONFIG_DIR}"
    if ! stellar_cli keys public-key "${CONTROLLER_IDENTITY}" >/dev/null 2>&1; then
        stellar_cli keys generate "${CONTROLLER_IDENTITY}" >/dev/null
    fi
    controller="$(stellar_cli keys public-key "${CONTROLLER_IDENTITY}")"
    blnt_usdc="$(stellar_cli contract id wasm \
        --salt "${BLNT_USDC_SALT}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
    blnt_xlm="$(stellar_cli contract id wasm \
        --salt "${BLNT_XLM_SALT}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
    [[ "${controller}" =~ ^G[A-Z2-7]{55}$ ]] || die "invalid controller address"
    [[ "${blnt_usdc}" =~ ^C[A-Z2-7]{55}$ ]] || die "invalid BLNT:USDC contract prediction"
    [[ "${blnt_xlm}" =~ ^C[A-Z2-7]{55}$ ]] || die "invalid BLNT:XLM contract prediction"
    source_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    RUN_DIR="${WORK_DIR}/runs/$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "${RUN_DIR}"
    jq -n \
        --arg network "public" \
        --arg network_passphrase "${NETWORK_PASSPHRASE}" \
        --arg rpc_url "${RPC_URL}" \
        --arg wallet "${WALLET}" \
        --arg controller "${controller}" \
        --arg blnt_usdc "${blnt_usdc}" \
        --arg blnt_xlm "${blnt_xlm}" \
        --arg run_dir "${RUN_DIR}" \
        --arg source_commit "${source_commit}" \
        --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg comet_sha256 "${COMET_SHA256}" \
        --arg comet_source_commit "${COMET_SOURCE_COMMIT}" \
        '{
            schema_version: 1,
            network: $network,
            network_passphrase: $network_passphrase,
            rpc_url: $rpc_url,
            wallet: $wallet,
            comet_controller: $controller,
            blnt_usdc_token: $blnt_usdc,
            blnt_xlm_token: $blnt_xlm,
            run_dir: $run_dir,
            source_commit: $source_commit,
            created_at: $created_at,
            comet_v1_1_wasm_sha256: $comet_sha256,
            comet_v1_1_source_commit: $comet_source_commit,
            phase: "planned",
            transactions: {}
        }' >"${STATE_FILE}"
}

load_state() {
    state_exists || die "mainnet Comet deployment state does not exist"
    RUN_DIR="$(state_value run_dir)"
    [[ -d "${RUN_DIR}" ]] || die "recorded run directory is missing: ${RUN_DIR}"
    [[ "$(state_value network_passphrase)" == "${NETWORK_PASSPHRASE}" ]] ||
        die "state file is not for Stellar public network"
    [[ "$(state_value wallet)" == "${WALLET}" ]] || die "state wallet mismatch"
    [[ "$(state_value comet_v1_1_wasm_sha256)" == "${COMET_SHA256}" ]] ||
        die "state Comet artifact mismatch"
}

invoke_view() {
    local contract="$1" function="$2"
    shift 2
    stellar_cli contract invoke \
        --id "${contract}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send no \
        -- "${function}" "$@"
}

normalize_scalar() {
    local raw="$1" decoded
    if decoded="$(printf '%s' "${raw}" | jq -er \
        'if type == "string" or type == "number" then tostring else empty end' \
        2>/dev/null)"
    then
        printf '%s\n' "${decoded}"
    else
        printf '%s\n' "${raw}"
    fi
}

horizon_account() {
    curl --fail --silent --show-error --max-time 15 \
        "${HORIZON_URL}/accounts/$1"
}

account_exists() {
    curl --fail --silent --max-time 10 "${HORIZON_URL}/accounts/$1" >/dev/null 2>&1
}

wait_for_account() {
    local account="$1"
    for _ in {1..30}; do
        account_exists "${account}" && return
        sleep 2
    done
    die "account did not become visible through Horizon: ${account}"
}

account_has_trustline() {
    local account="$1" code="$2" issuer="$3"
    horizon_account "${account}" | jq -e \
        --arg code "${code}" --arg issuer "${issuer}" \
        '.balances[] | select(.asset_code == $code and .asset_issuer == $issuer)' \
        >/dev/null
}

token_balance() {
    local token="$1" account="$2" raw
    raw="$(invoke_view "${token}" balance --id "${account}")"
    normalize_scalar "${raw}"
}

pool_value() {
    local pool="$1" function="$2"
    shift 2
    normalize_scalar "$(invoke_view "${pool}" "${function}" "$@")"
}

fetch_contract_artifact() {
    local destination="$1" description="$2" attempt stderr_file stderr_output temporary_dir
    shift 2
    for attempt in 1 2 3; do
        temporary_dir="$(mktemp -d)"
        stderr_file="${temporary_dir}/fetch.err"
        rm -f "${destination}"
        if stellar_cli contract fetch "$@" \
            --out-file "${destination}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            2>"${stderr_file}"
        then
            rm -r -- "${temporary_dir}"
            return 0
        fi
        stderr_output="$(<"${stderr_file}")"
        if [[ "${stderr_output}" == *"Contract not found"* ||
            "${stderr_output}" == *"Contract Code not found"* ||
            "${stderr_output}" == *"Wasm not found"* ||
            "${stderr_output}" == *"not found in ledger"* ]]
        then
            rm -r -- "${temporary_dir}"
            return 1
        fi
        if (( attempt == 3 )); then
            note "Failed to fetch ${description} after three attempts."
            sed -n '1,20p' "${stderr_file}" >&2
            rm -r -- "${temporary_dir}"
            die "unable to determine the on-chain state of ${description}"
        fi
        rm -r -- "${temporary_dir}"
    done
}

contract_matches_release() {
    local contract="$1" fetched temporary_dir
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/${contract}.wasm"
    if ! fetch_contract_artifact "${fetched}" "contract ${contract}" \
        --id "${contract}"
    then
        rm -r -- "${temporary_dir}"
        return 1
    fi
    if ! cmp --silent "${COMET_WASM}" "${fetched}"; then
        rm -r -- "${temporary_dir}"
        die "deployed contract ${contract} does not use the exact release WASM"
    fi
    rm -r -- "${temporary_dir}"
}

pool_initialized() {
    local pool="$1" first="$2" second="$3" tokens
    tokens="$(invoke_view "${pool}" get_tokens 2>/dev/null || true)"
    printf '%s' "${tokens}" | jq -e \
        --arg first "${first}" --arg second "${second}" \
        '. == [$first, $second]' >/dev/null 2>&1
}

verify_pool_config() {
    local label="$1" pool="$2" first="$3" second="$4"
    local tokens first_weight second_weight fee controller decimals
    tokens="$(invoke_view "${pool}" get_tokens)"
    printf '%s' "${tokens}" | jq -e \
        --arg first "${first}" --arg second "${second}" \
        '. == [$first, $second]' >/dev/null || die "${label} token order mismatch"
    first_weight="$(pool_value "${pool}" get_normalized_weight --token "${first}")"
    second_weight="$(pool_value "${pool}" get_normalized_weight --token "${second}")"
    fee="$(pool_value "${pool}" get_swap_fee)"
    controller="$(normalize_scalar "$(invoke_view "${pool}" get_controller)")"
    decimals="$(pool_value "${pool}" decimals)"
    [[ "${first_weight}" == "${WEIGHT_BLNT}" ]] || die "${label} BLNT weight mismatch"
    [[ "${second_weight}" == "${WEIGHT_PAIR}" ]] || die "${label} pair weight mismatch"
    [[ "${fee}" == "${SWAP_FEE}" ]] || die "${label} swap fee mismatch"
    [[ "${controller}" == "$(state_value comet_controller)" ]] ||
        die "${label} controller mismatch"
    [[ "${decimals}" == "7" ]] || die "${label} LP decimals mismatch"
}

verify_pool_balances() {
    local label="$1" pool="$2" pair="$3" expected_pair="$4"
    local blnt_balance pair_balance supply wallet_lp controller_lp
    blnt_balance="$(pool_value "${pool}" get_balance --token "${BLNT}")"
    pair_balance="$(pool_value "${pool}" get_balance --token "${pair}")"
    supply="$(pool_value "${pool}" get_total_supply)"
    wallet_lp="$(pool_value "${pool}" balance --id "${WALLET}")"
    controller_lp="$(pool_value "${pool}" balance --id "$(state_value comet_controller)")"
    [[ "${blnt_balance}" == "${FINAL_BLNT}" ]] || die "${label} BLNT reserve mismatch"
    [[ "${pair_balance}" == "${expected_pair}" ]] || die "${label} pair reserve mismatch"
    [[ "${supply}" == "${FINAL_LP}" ]] || die "${label} LP supply mismatch"
    [[ "${wallet_lp}" == "${JOIN_LP}" ]] || die "${label} wallet LP balance mismatch"
    [[ "${controller_lp}" == "${INITIAL_LP}" ]] || die "${label} locked LP balance mismatch"
}

sac_balance_key_xdr() {
    local holder="$1" key_json
    key_json="$(jq -cn --arg holder "${holder}" \
        '{vec: [{symbol: "Balance"}, {address: $holder}]}')"
    stellar_cli xdr encode --type ScVal --output single-base64 "${key_json}"
}

verify_non_clawbackable_balance() {
    local label="$1" token="$2" holder="$3" key_xdr entry facts
    key_xdr="$(sac_balance_key_xdr "${holder}")"
    entry="$(stellar_cli ledger entry fetch contract-data \
        --contract "${token}" \
        --key-xdr "${key_xdr}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json)"
    facts="$(printf '%s' "${entry}" | jq -ce '
        def field($name):
            ([.[] | select(.key.symbol? == $name) | .val] |
                if length == 1 then .[0] else error("invalid field") end);
        .entries[0].val.contract_data.val.map as $fields |
        {
            amount: ($fields | field("amount") | .i128),
            authorized: ($fields | field("authorized") | .bool),
            clawback: ($fields | field("clawback") | .bool)
        }
    ')" || die "${label} is not a Protocol-27 SAC balance entry"
    printf '%s' "${facts}" | jq -e \
        '(.amount | tonumber) > 0 and .authorized == true and .clawback == false' \
        >/dev/null || die "${label} custody is not positive, authorized, and non-clawbackable"
    note "Verified ${label} custody is positive, authorized, and non-clawbackable."
}

verify_controller_lock() {
    local controller account
    controller="$(state_value comet_controller)"
    account="$(horizon_account "${controller}")"
    printf '%s' "${account}" | jq -e \
        --arg controller "${controller}" --argjson threshold "${CONTROLLER_THRESHOLD}" '
        .thresholds.low_threshold == $threshold and
        .thresholds.med_threshold == $threshold and
        .thresholds.high_threshold == $threshold and
        (.signers | length == 1) and
        .signers[0].key == $controller and
        .signers[0].weight == 0
    ' >/dev/null || die "Comet controller is not irreversibly locked"
}

validate_release() {
    local actual_commit actual_hash temporary_dir release_wasm
    require_command cmp
    require_command curl
    require_command git
    require_command jq
    require_command shasum
    require_command stellar
    [[ "$(stellar --version | head -1)" == stellar\ 27.* ]] ||
        die "Stellar CLI major version 27 is required"
    actual_hash="$(sha256_file "${COMET_WASM}")"
    [[ "${actual_hash}" == "${COMET_SHA256}" ]] ||
        die "embedded Comet WASM hash ${actual_hash} does not match ${COMET_SHA256}"
    actual_commit="$(git -C "${ROOT_DIR}/comet-contracts-v1" rev-parse HEAD)"
    [[ "${actual_commit}" == "${COMET_SOURCE_COMMIT}" ]] ||
        die "Comet source pin ${actual_commit} does not match ${COMET_SOURCE_COMMIT}"
    [[ "$(git -C "${ROOT_DIR}/comet-contracts-v1" describe --tags --exact-match HEAD)" == "v1.1.0" ]] ||
        die "Comet source pin is not tag v1.1.0"
    temporary_dir="$(mktemp -d)"
    release_wasm="${temporary_dir}/contracts_v1.0.0.wasm"
    curl --fail --location --silent --show-error \
        --output "${release_wasm}" "${COMET_RELEASE_URL}"
    [[ "$(sha256_file "${release_wasm}")" == "${COMET_SHA256}" ]] ||
        die "downloaded Comet release artifact has an unexpected hash"
    cmp --silent "${release_wasm}" "${COMET_WASM}" ||
        die "embedded Comet WASM differs from the official release bytes"
    rm -rf "${temporary_dir}"
    note "Verified exact official Comet V1.1 release bytes and pinned v1.1.0 source."
}

preflight() {
    local account legacy_blnd legacy_usdc reference_price discount_bps
    load_wallet_config
    account="$(horizon_account "${WALLET}")"
    printf '%s' "${account}" | jq -e \
        --arg blnt_issuer "${BLNT_ISSUER}" --arg usdc_issuer "${USDC_ISSUER}" '
        ([.balances[] | select(.asset_code == "BLNT" and .asset_issuer == $blnt_issuer)][0].balance | tonumber) >= 2000000 and
        ([.balances[] | select(.asset_code == "USDC" and .asset_issuer == $usdc_issuer)][0].balance | tonumber) >= 1175 and
        ([.balances[] | select(.asset_type == "native")][0].balance | tonumber) >= 7025
    ' >/dev/null || die "deployment wallet lacks the required BLNT, USDC, or XLM"
    legacy_blnd="$(pool_value "${LEGACY_BLND_USDC_COMET}" get_balance --token "${LEGACY_BLND}")"
    legacy_usdc="$(pool_value "${LEGACY_BLND_USDC_COMET}" get_balance --token "${USDC}")"
    [[ "${legacy_blnd}" =~ ^[1-9][0-9]*$ && "${legacy_usdc}" =~ ^[1-9][0-9]*$ ]] ||
        die "invalid legacy BLND:USDC reserve snapshot"
    reference_price=$((4 * legacy_usdc * SCALAR_7 / legacy_blnd))
    (( TARGET_BLNT_PRICE < reference_price )) ||
        die "target BLNT price is not below the current BLND Comet price"
    discount_bps=$((10000 - TARGET_BLNT_PRICE * 10000 / reference_price))
    (( discount_bps >= MIN_BLND_DISCOUNT_BPS )) ||
        die "target BLNT price discount is only ${discount_bps} bps; require ${MIN_BLND_DISCOUNT_BPS}"
    note "Preflight passed: BLND reference ${reference_price} raw, BLNT target ${TARGET_BLNT_PRICE} raw, discount ${discount_bps} bps."
}

ensure_controller_account() {
    local controller
    controller="$(state_value comet_controller)"
    if ! account_exists "${controller}"; then
        capture create-comet-controller \
            wallet_stellar tx new create-account \
            --source-account "${WALLET}" \
            --destination "${controller}" \
            --starting-balance "${CONTROLLER_STARTING_XLM}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        record_transaction create-comet-controller
        wait_for_account "${controller}"
    fi
    if ! account_has_trustline "${controller}" BLNT "${BLNT_ISSUER}"; then
        capture trust-controller-blnt \
            stellar_cli tx new change-trust \
            --source-account "${CONTROLLER_IDENTITY}" \
            --line "${BLNT_ASSET}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        record_transaction trust-controller-blnt
    fi
    if ! account_has_trustline "${controller}" USDC "${USDC_ISSUER}"; then
        capture trust-controller-usdc \
            stellar_cli tx new change-trust \
            --source-account "${CONTROLLER_IDENTITY}" \
            --line "${USDC_ASSET}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        record_transaction trust-controller-usdc
    fi
    state_set phase controller-ready
}

ensure_release_uploaded() {
    local fetched temporary_dir uploaded
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/release.wasm"
    if ! fetch_contract_artifact "${fetched}" "Comet WASM ${COMET_SHA256}" \
        --wasm-hash "${COMET_SHA256}"
    then
        capture upload-comet-v1-1 \
            wallet_stellar contract upload \
            --wasm "${COMET_WASM}" \
            --optimize=false \
            --source-account "${WALLET}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost >/dev/null
        uploaded="$(<"${RUN_DIR}/upload-comet-v1-1.out")"
        uploaded="$(normalize_scalar "${uploaded}")"
        [[ "${uploaded}" == "${COMET_SHA256}" ]] || die "uploaded Comet WASM hash mismatch"
        record_transaction upload-comet-v1-1
    fi
    rm -f "${fetched}"
    fetch_contract_artifact "${fetched}" "Comet WASM ${COMET_SHA256}" \
        --wasm-hash "${COMET_SHA256}" >/dev/null
    cmp --silent "${COMET_WASM}" "${fetched}" ||
        die "on-chain Comet code differs from the official release bytes"
    rm -r -- "${temporary_dir}"
    state_set phase release-uploaded
}

ensure_pool_deployed() {
    local label="$1" contract="$2" salt="$3" deployed
    if ! contract_matches_release "${contract}"; then
        capture "deploy-${label}" \
            wallet_stellar contract deploy \
            --wasm-hash "${COMET_SHA256}" \
            --salt "${salt}" \
            --source-account "${WALLET}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost >/dev/null
        deployed="$(<"${RUN_DIR}/deploy-${label}.out")"
        deployed="$(normalize_scalar "${deployed}")"
        [[ "${deployed}" == "${contract}" ]] || die "${label} deployed at unexpected address"
        record_transaction "deploy-${label}"
    fi
    contract_matches_release "${contract}" || die "${label} release verification failed"
}

ensure_controller_token_balance() {
    local label="$1" token="$2" asset="$3" required="$4"
    local controller balance deficit
    controller="$(state_value comet_controller)"
    balance="$(token_balance "${token}" "${controller}")"
    [[ "${balance}" =~ ^[0-9]+$ ]] || die "invalid ${label} controller balance"
    if (( balance < required )); then
        deficit=$((required - balance))
        capture "fund-controller-${label}" \
            wallet_stellar tx new payment \
            --source-account "${WALLET}" \
            --destination "${controller}" \
            --asset "${asset}" \
            --amount "${deficit}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        record_transaction "fund-controller-${label}"
    fi
}

ensure_pools_initialized() {
    local controller blnt_required usdc_required
    local usdc_initialized=false xlm_initialized=false
    controller="$(state_value comet_controller)"
    pool_initialized "$(state_value blnt_usdc_token)" "${BLNT}" "${USDC}" && usdc_initialized=true
    pool_initialized "$(state_value blnt_xlm_token)" "${BLNT}" "${XLM}" && xlm_initialized=true
    blnt_required=0
    usdc_required=0
    [[ "${usdc_initialized}" == true ]] || {
        blnt_required=$((blnt_required + INIT_BLNT))
        usdc_required="${INIT_USDC}"
    }
    [[ "${xlm_initialized}" == true ]] || blnt_required=$((blnt_required + INIT_BLNT))
    ensure_controller_token_balance blnt "${BLNT}" "${BLNT_ASSET}" "${blnt_required}"
    ensure_controller_token_balance usdc "${USDC}" "${USDC_ASSET}" "${usdc_required}"

    if [[ "${usdc_initialized}" == false ]]; then
        capture initialize-blnt-usdc \
            stellar_cli contract invoke \
            --id "$(state_value blnt_usdc_token)" \
            --source-account "${CONTROLLER_IDENTITY}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost \
            -- init \
            --controller "${controller}" \
            --tokens "[\"${BLNT}\",\"${USDC}\"]" \
            --weights "[\"${WEIGHT_BLNT}\",\"${WEIGHT_PAIR}\"]" \
            --balances "[\"${INIT_BLNT}\",\"${INIT_USDC}\"]" \
            --swap_fee "${SWAP_FEE}" >/dev/null
        record_transaction initialize-blnt-usdc
    fi
    if [[ "${xlm_initialized}" == false ]]; then
        capture initialize-blnt-xlm \
            stellar_cli contract invoke \
            --id "$(state_value blnt_xlm_token)" \
            --source-account "${CONTROLLER_IDENTITY}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost \
            -- init \
            --controller "${controller}" \
            --tokens "[\"${BLNT}\",\"${XLM}\"]" \
            --weights "[\"${WEIGHT_BLNT}\",\"${WEIGHT_PAIR}\"]" \
            --balances "[\"${INIT_BLNT}\",\"${INIT_XLM}\"]" \
            --swap_fee "${SWAP_FEE}" >/dev/null
        record_transaction initialize-blnt-xlm
    fi
    verify_pool_config BLNT-USDC "$(state_value blnt_usdc_token)" "${BLNT}" "${USDC}"
    verify_pool_config BLNT-XLM "$(state_value blnt_xlm_token)" "${BLNT}" "${XLM}"
    state_set phase pools-initialized
}

ensure_wallet_join() {
    local label="$1" pool="$2" pair_join="$3" wallet_lp
    wallet_lp="$(pool_value "${pool}" balance --id "${WALLET}")"
    if [[ "${wallet_lp}" == "0" ]]; then
        capture "join-${label}" \
            wallet_stellar contract invoke \
            --id "${pool}" \
            --source-account "${WALLET}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost \
            --auto-sign \
            -- join_pool \
            --pool_amount_out "${JOIN_LP}" \
            --max_amounts_in "[\"${JOIN_BLNT}\",\"${pair_join}\"]" \
            --user "${WALLET}" >/dev/null
        record_transaction "join-${label}"
    elif [[ "${wallet_lp}" != "${JOIN_LP}" ]]; then
        die "${label} wallet LP balance is ${wallet_lp}; expected zero or ${JOIN_LP}"
    fi
}

lock_controller() {
    local controller account unlocked
    controller="$(state_value comet_controller)"
    account="$(horizon_account "${controller}")"
    if printf '%s' "${account}" | jq -e \
        --arg controller "${controller}" '
        .thresholds.low_threshold == 0 and
        .thresholds.med_threshold == 0 and
        .thresholds.high_threshold == 0 and
        (.signers | length == 1) and
        .signers[0].key == $controller and
        .signers[0].weight == 1
    ' >/dev/null
    then
        unlocked=true
    else
        unlocked=false
    fi
    if [[ "${unlocked}" == true ]]; then
        capture lock-comet-controller \
            stellar_cli tx new set-options \
            --source-account "${CONTROLLER_IDENTITY}" \
            --low-threshold "${CONTROLLER_THRESHOLD}" \
            --med-threshold "${CONTROLLER_THRESHOLD}" \
            --high-threshold "${CONTROLLER_THRESHOLD}" \
            --master-weight 0 \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        record_transaction lock-comet-controller
    fi
    verify_controller_lock
    state_set phase controller-locked
}

verify_deployment() {
    local blnt_usdc blnt_xlm
    blnt_usdc="$(state_value blnt_usdc_token)"
    blnt_xlm="$(state_value blnt_xlm_token)"
    contract_matches_release "${blnt_usdc}" || die "BLNT:USDC contract is unavailable"
    contract_matches_release "${blnt_xlm}" || die "BLNT:XLM contract is unavailable"
    verify_pool_config BLNT-USDC "${blnt_usdc}" "${BLNT}" "${USDC}"
    verify_pool_config BLNT-XLM "${blnt_xlm}" "${BLNT}" "${XLM}"
    verify_pool_balances BLNT-USDC "${blnt_usdc}" "${USDC}" "${FINAL_USDC}"
    verify_pool_balances BLNT-XLM "${blnt_xlm}" "${XLM}" "${FINAL_XLM}"
    verify_non_clawbackable_balance BLNT-USDC-BLNT "${BLNT}" "${blnt_usdc}"
    verify_non_clawbackable_balance BLNT-USDC-USDC "${USDC}" "${blnt_usdc}"
    verify_non_clawbackable_balance BLNT-XLM-BLNT "${BLNT}" "${blnt_xlm}"
    verify_controller_lock
    state_set phase verified
    state_set verified_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

write_registry() {
    local temporary="${REGISTRY_FILE}.tmp" controller_xlm
    controller_xlm="$(horizon_account "$(state_value comet_controller)" | jq -er \
        '[.balances[] | select(.asset_type == "native")][0].balance')"
    jq \
        --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg blnt "${BLNT}" --arg usdc "${USDC}" --arg xlm "${XLM}" \
        --arg artifact_url "${COMET_RELEASE_URL}" \
        --arg init_blnt "${INIT_BLNT}" --arg init_usdc "${INIT_USDC}" --arg init_xlm "${INIT_XLM}" \
        --arg join_blnt "${JOIN_BLNT}" --arg join_usdc "${JOIN_USDC}" --arg join_xlm "${JOIN_XLM}" \
        --arg join_lp "${JOIN_LP}" --arg final_blnt "${FINAL_BLNT}" \
        --arg final_usdc "${FINAL_USDC}" --arg final_xlm "${FINAL_XLM}" \
        --arg final_lp "${FINAL_LP}" --arg controller_xlm "${controller_xlm}" '
        {
            schema_version,
            network: {name: "Public Global Stellar Network", passphrase: .network_passphrase},
            generated_at: $generated_at,
            deployer: .wallet,
            controller: {
                address: .comet_controller,
                locked: true,
                thresholds: [100, 100, 100],
                master_weight: 0,
                residual_xlm: $controller_xlm
            },
            artifact: {
                release: "v1.1.0_contracts_pkg1.0.0_cli25.1.0",
                release_url: $artifact_url,
                sha256: .comet_v1_1_wasm_sha256,
                source_commit: .comet_v1_1_source_commit
            },
            assets: {blnt: $blnt, usdc: $usdc, xlm: $xlm},
            pools: {
                blnt_usdc: {
                    contract_id: .blnt_usdc_token,
                    tokens: [$blnt, $usdc],
                    weights: [8000000, 2000000],
                    swap_fee: 30000,
                    init_balances: [$init_blnt, $init_usdc],
                    join_balances: [$join_blnt, $join_usdc],
                    wallet_lp_join: $join_lp,
                    final_reserves: [$final_blnt, $final_usdc],
                    final_lp_supply: $final_lp
                },
                blnt_xlm: {
                    contract_id: .blnt_xlm_token,
                    tokens: [$blnt, $xlm],
                    weights: [8000000, 2000000],
                    swap_fee: 30000,
                    init_balances: [$init_blnt, $init_xlm],
                    join_balances: [$join_blnt, $join_xlm],
                    wallet_lp_join: $join_lp,
                    final_reserves: [$final_blnt, $final_xlm],
                    final_lp_supply: $final_lp
                }
            },
            transactions: .transactions,
            verified_at: .verified_at
        }' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${REGISTRY_FILE}"
}

deployment_plan() {
    cat <<EOF
Mainnet canonical Comet V1.1 deployment plan
  Artifact: exact official release ${COMET_SHA256}
  Controller: one dedicated account, permanently locked after verification
  BLNT:USDC init: 1,000 BLNT + 1.175 USDC
  BLNT:USDC join: 999,000 BLNT + 1,173.825 USDC for 99,900 LP
  BLNT:XLM init: 1,000 BLNT + 7 XLM
  BLNT:XLM join: 999,000 BLNT + 6,993 XLM for 99,900 LP
  Final reserves: 1,000,000 BLNT + 1,175 USDC; 1,000,000 BLNT + 7,000 XLM
  Weights: 80% BLNT / 20% paired asset
  Swap fee: 0.30%
  BLNT price: 0.0047000 USDC

This command deploys only the two canonical LP contracts. It does not deploy
or initialize the V3 emitter, backfill, backstop, factory, or lending pools.
EOF
}

deploy() {
    local controller_locked
    validate_release
    load_wallet_config
    if ! state_exists; then
        preflight
        initialize_state
    else
        load_state
    fi
    controller_locked="$(state_optional phase)"
    [[ "${controller_locked}" != "verified" ]] || {
        verify_deployment
        write_registry
        note "Mainnet canonical Comet V1.1 deployment is already verified."
        return
    }
    ensure_controller_account
    ensure_release_uploaded
    ensure_pool_deployed blnt-usdc "$(state_value blnt_usdc_token)" "${BLNT_USDC_SALT}"
    ensure_pool_deployed blnt-xlm "$(state_value blnt_xlm_token)" "${BLNT_XLM_SALT}"
    state_set phase pools-deployed
    ensure_pools_initialized
    ensure_wallet_join blnt-usdc "$(state_value blnt_usdc_token)" "${JOIN_USDC}"
    ensure_wallet_join blnt-xlm "$(state_value blnt_xlm_token)" "${JOIN_XLM}"
    state_set phase liquidity-joined
    verify_pool_config BLNT-USDC "$(state_value blnt_usdc_token)" "${BLNT}" "${USDC}"
    verify_pool_config BLNT-XLM "$(state_value blnt_xlm_token)" "${BLNT}" "${XLM}"
    verify_pool_balances BLNT-USDC "$(state_value blnt_usdc_token)" "${USDC}" "${FINAL_USDC}"
    verify_pool_balances BLNT-XLM "$(state_value blnt_xlm_token)" "${XLM}" "${FINAL_XLM}"
    verify_non_clawbackable_balance BLNT-USDC-BLNT "${BLNT}" "$(state_value blnt_usdc_token)"
    verify_non_clawbackable_balance BLNT-USDC-USDC "${USDC}" "$(state_value blnt_usdc_token)"
    verify_non_clawbackable_balance BLNT-XLM-BLNT "${BLNT}" "$(state_value blnt_xlm_token)"
    lock_controller
    verify_deployment
    write_registry
    note "Mainnet canonical Comet V1.1 pools deployed and verified."
}

status() {
    load_state
    cat <<EOF
Mainnet canonical Comet V1.1 deployment
  phase: $(state_value phase)
  controller: $(state_value comet_controller)
  BLNT:USDC: $(state_value blnt_usdc_token)
  BLNT:XLM: $(state_value blnt_xlm_token)
  artifact SHA-256: $(state_value comet_v1_1_wasm_sha256)
  registry: ${REGISTRY_FILE}
EOF
    if [[ "$(state_optional phase)" == "verified" ]]; then
        verify_deployment
    fi
}

usage() {
    cat <<'EOF'
Usage: scripts/mainnet-comet-deployment.sh COMMAND

Commands:
  plan       Print the exact approved parameters without network mutation.
  validate   Compare the embedded WASM byte-for-byte with the official release.
  preflight  Verify wallet funding and the BLNT discount without mutation.
  deploy     Deploy, initialize, join, lock, verify, and record both pools.
  status     Print and reverify the recorded deployment.
EOF
}

if [[ "${BLEND_MAINNET_COMET_LIBRARY_ONLY:-false}" != "true" ]]; then
    case "${1:-}" in
        plan) deployment_plan ;;
        validate) validate_release ;;
        preflight) validate_release; preflight ;;
        deploy) deploy ;;
        status) status ;;
        *) usage; exit 2 ;;
    esac
fi
