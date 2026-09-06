#!/usr/bin/env bash

# Copyright (C) 2026 BLNT Protocol contributors
# SPDX-License-Identifier: AGPL-3.0-only

# This file is sourced by v3-only-deployment.sh after the common deployment
# helpers and V3-only configuration have been loaded.

SEP41_POOL_FIXTURE="${ROOT_DIR}/fixtures/sep41-assets-pool.json"
SEP41_TOKEN_WASM="${ROOT_DIR}/migration-harness/target/wasm32v1-none/optimized/blnt_test_sep41_token.wasm"
SEP41_SETUP_IDENTITY="${BLEND_SEP41_SETUP_IDENTITY:-blnt-v3-${NETWORK_MODE}-sep41-setup}"
SEP41_WBTC_SALT="${BLEND_SEP41_WBTC_SALT:-57425443000000000000000000000000000000000000000000000000000000c1}"
SEP41_WETH_SALT="${BLEND_SEP41_WETH_SALT:-57455448000000000000000000000000000000000000000000000000000000c2}"
SEP41_ORACLE_SALT="${BLEND_SEP41_ORACLE_SALT:-5345503431204f7261636c6500000000000000000000000000000000000000c3}"
SEP41_POOL_SALT="${BLEND_SEP41_POOL_SALT:-53455034312041737365747300000000000000000000000000000000000000c4}"

require_sep41_pool_support() {
    [[ -s "${SEP41_POOL_FIXTURE}" ]] ||
        die "SEP-41 Assets fixture is missing: ${SEP41_POOL_FIXTURE}"
    [[ -s "${SEP41_TOKEN_WASM}" ]] ||
        die "test SEP-41 token WASM is missing: ${SEP41_TOKEN_WASM}"
    [[ -s "${V3_TEST_SEP40_ORACLE_WASM}" ]] ||
        die "test lending-oracle WASM is missing: ${V3_TEST_SEP40_ORACLE_WASM}"
}

sep41_fixture_value() {
    jq -er "$1" "${SEP41_POOL_FIXTURE}"
}

sep41_label() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

sep41_asset_state_key() {
    printf 'sep41_asset_%s\n' "$(sep41_label "$1")"
}

sep41_asset_id() {
    state_value "$(sep41_asset_state_key "$1")"
}

sep41_asset_salt() {
    case "$1" in
        wBTC) printf '%s\n' "${SEP41_WBTC_SALT}" ;;
        wETH) printf '%s\n' "${SEP41_WETH_SALT}" ;;
        *) die "unsupported SEP-41 Assets token: $1" ;;
    esac
}

sep41_contract_wasm_hash() {
    capture "query-sep41-contract-wasm-hash" \
        stellar_cli ledger entry fetch contract-data \
        --contract "$1" --instance \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json |
        jq -er '.entries[0].val.contract_data.val.contract_instance.executable.wasm'
}

ensure_sep41_setup_identity() {
    local identity saved_identity saved_user user

    saved_identity="$(state_optional sep41_setup_identity)"
    identity="${saved_identity:-${SEP41_SETUP_IDENTITY}}"
    saved_user="$(state_optional sep41_setup_user)"
    if ! stellar_cli keys public-key "${identity}" >/dev/null 2>&1; then
        [[ -z "${saved_user}" ]] ||
            die "saved SEP-41 setup identity is unavailable: ${identity}"
        note "Creating ${NETWORK_LABEL} SEP-41 Assets setup identity ${identity}..."
        stellar_cli keys generate "${identity}" --overwrite >/dev/null
    fi
    user="$(stellar_cli keys public-key "${identity}")"
    if [[ -n "${saved_user}" ]]; then
        assert_equal "${saved_user}" "${user}" "SEP-41 setup identity"
    else
        fund_and_wait_for_account "${user}"
    fi
    state_set "sep41_setup_identity" "${identity}"
    state_set "sep41_setup_user" "${user}"
    printf '%s\n' "${user}"
}

ensure_sep41_asset() {
    local symbol="$1" name state_key token expected_hash actual_hash

    state_key="$(sep41_asset_state_key "${symbol}")"
    token="$(state_optional "${state_key}")"
    name="$(jq -er --arg symbol "${symbol}" \
        '.reserves[] | select(.asset == $symbol) | .name' \
        "${SEP41_POOL_FIXTURE}")"
    if [[ -z "${token}" ]]; then
        token="$(deploy_wasm \
            "deploy-sep41-${symbol}" "${SEP41_TOKEN_WASM}" \
            "$(sep41_asset_salt "${symbol}")" \
            --admin "$(state_value operator)" \
            --decimal 7 \
            --name "$(jq -cn --arg name "${name}" '$name')" \
            --symbol "$(jq -cn --arg symbol "${symbol}" '$symbol')")"
        state_set "${state_key}" "${token}"
    fi
    require_contract_id "${token}" "${symbol} SEP-41 token"
    expected_hash="$(sha256_file "${SEP41_TOKEN_WASM}")"
    actual_hash="$(sep41_contract_wasm_hash "${token}")"
    assert_equal "${expected_hash}" "${actual_hash}" "${symbol} SEP-41 WASM hash"
    assert_equal "$(state_value operator)" \
        "$(normalize_scalar "$(invoke_view \
            "query-sep41-${symbol}-admin" "${token}" admin)")" \
        "${symbol} SEP-41 administrator"
    assert_equal "7" \
        "$(normalize_scalar "$(invoke_view \
            "query-sep41-${symbol}-decimals" "${token}" decimals)")" \
        "${symbol} decimals"
    assert_equal "${symbol}" \
        "$(normalize_scalar "$(invoke_view \
            "query-sep41-${symbol}-symbol" "${token}" symbol)")" \
        "${symbol} symbol"
    state_set "sep41_token_wasm_sha256" "${actual_hash}"
    printf '%s\n' "${token}"
}

sep41_pool_asset_ids() {
    local result="[]" symbol token

    while IFS= read -r symbol; do
        token="$(sep41_asset_id "${symbol}")"
        result="$(printf '%s' "${result}" | jq -c \
            --arg token "${token}" '. + [{Stellar: $token}]')"
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    printf '%s\n' "${result}"
}

seed_sep41_oracle() {
    local oracle="$1" symbol token price prices label

    while IFS= read -r symbol; do
        label="$(sep41_label "${symbol}")"
        token="$(sep41_asset_id "${symbol}")"
        price="$(jq -er --arg symbol "${symbol}" \
            '.reserves[] | select(.asset == $symbol) | .price' \
            "${SEP41_POOL_FIXTURE}")"
        prices="$(fixture_price_series "${price}")"
        invoke_transaction "seed-sep41-oracle-${label}" \
            "${oracle}" set_prices \
            --asset "{\"Stellar\":\"${token}\"}" \
            --prices "${prices}" >/dev/null
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    state_set "sep41_oracle_refreshed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}

ensure_sep41_oracle() {
    local oracle assets

    oracle="$(state_optional sep41_pool_oracle)"
    if [[ -z "${oracle}" ]]; then
        assets="$(sep41_pool_asset_ids)"
        oracle="$(deploy_wasm \
            "deploy-sep41-assets-oracle" \
            "${V3_TEST_SEP40_ORACLE_WASM}" "${SEP41_ORACLE_SALT}" \
            --admin "$(state_value operator)" \
            --base '{"Other":"USD"}' \
            --assets "${assets}" \
            --decimals 7 \
            --resolution "${FIXTURE_ORACLE_RESOLUTION}")"
        state_set "sep41_pool_oracle" "${oracle}"
    fi
    require_contract_id "${oracle}" "SEP-41 Assets oracle"
    seed_sep41_oracle "${oracle}"
    printf '%s\n' "${oracle}"
}

sep41_reserve_config() {
    local index="$1"

    jq -c --argjson index "${index}" '
        .reserves[$index].config + {
            decimals: 7,
            enabled: true,
            index: $index
        }
    ' "${SEP41_POOL_FIXTURE}"
}

ensure_sep41_pool() {
    local setup="$1" identity="$2" oracle="$3"
    local pool raw reserve_list symbol token config index=0 status

    pool="$(state_optional sep41_pool)"
    if [[ -z "${pool}" ]]; then
        raw="$(invoke_transaction_as \
            "deploy-sep41-assets-pool" "${identity}" \
            "$(state_value v3_factory)" deploy \
            --admin "${setup}" \
            --name "$(jq -c '.name' "${SEP41_POOL_FIXTURE}")" \
            --salt "${SEP41_POOL_SALT}" \
            --oracle "${oracle}" \
            --backstop_take_rate "$(sep41_fixture_value '.backstop_take_rate')" \
            --max_positions "$(sep41_fixture_value '.max_positions')" \
            --min_collateral "$(sep41_fixture_value '.min_collateral')" \
            --backstop_config "$(jq -c '.backstop_config' "${SEP41_POOL_FIXTURE}")" \
            --access_controller null)"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "SEP-41 Assets pool"
        state_set "sep41_pool" "${pool}"
        state_set "sep41_pool_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fi
    require_contract_id "${pool}" "SEP-41 Assets pool"
    state_set "sep41_pool_oracle" "${oracle}"

    reserve_list="$(invoke_view "query-sep41-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r symbol; do
        token="$(sep41_asset_id "${symbol}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg token "${token}" 'index($token) != null' >/dev/null
        then
            config="$(sep41_reserve_config "${index}")"
            invoke_transaction_as "queue-sep41-${symbol}" "${identity}" \
                "${pool}" queue_set_reserve \
                --asset "${token}" --metadata "${config}" >/dev/null
            invoke_transaction_as "set-sep41-${symbol}" "${identity}" \
                "${pool}" set_reserve --asset "${token}" >/dev/null
            reserve_list="$(invoke_view \
                "query-sep41-reserves-${symbol}" "${pool}" get_reserve_list)"
        fi
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")

    status="$(invoke_view "query-sep41-pool-status" "${pool}" get_config |
        jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        invoke_transaction_as "on-ice-sep41-pool" "${identity}" \
            "${pool}" set_status --pool_status 2 >/dev/null
    elif [[ "${status}" != "0" && "${status}" != "1" && "${status}" != "2" ]]; then
        die "SEP-41 Assets pool has unexpected status ${status}"
    fi
    printf '%s\n' "${pool}"
}

sep41_positions() {
    invoke_view "query-sep41-positions" "$1" get_positions --address "$2"
}

sep41_token_balance() {
    local symbol="$1" token="$2" holder="$3"

    normalize_scalar "$(invoke_view \
        "query-sep41-${symbol}-balance" "${token}" balance --id "${holder}")"
}

ensure_sep41_token_balance() {
    local symbol="$1" token="$2" holder="$3" required="$4" balance

    balance="$(sep41_token_balance "${symbol}" "${token}" "${holder}")"
    [[ "${balance}" =~ ^[0-9]+$ ]] || die "invalid ${symbol} token balance"
    if (( balance < required )); then
        invoke_transaction "mint-sep41-${symbol}" "${token}" mint \
            --to "${holder}" --amount "$((required - balance))" >/dev/null
    fi
}

ensure_sep41_supply_positions() {
    local pool="$1" setup="$2" identity="$3" positions requests="[]"
    local symbol token supplied borrowed collateral liability index=0

    positions="$(sep41_positions "${pool}" "${setup}")"
    while IFS= read -r symbol; do
        supplied="$(jq -er --arg symbol "${symbol}" \
            '.reserves[] | select(.asset == $symbol) | .supplied' \
            "${SEP41_POOL_FIXTURE}")"
        borrowed="$(jq -er --arg symbol "${symbol}" \
            '.reserves[] | select(.asset == $symbol) | .borrowed' \
            "${SEP41_POOL_FIXTURE}")"
        collateral="$(printf '%s' "${positions}" | jq -er \
            --arg index "${index}" '(.collateral[$index] // "0") | tostring')"
        liability="$(printf '%s' "${positions}" | jq -er \
            --arg index "${index}" '(.liabilities[$index] // "0") | tostring')"
        if [[ "${collateral}" != "0" && "${collateral}" != "${supplied}" ]]; then
            die "SEP-41 setup account has partial ${symbol} collateral"
        fi
        if [[ "${liability}" != "0" && "${liability}" != "${borrowed}" ]]; then
            die "SEP-41 setup account has partial ${symbol} liability"
        fi
        if [[ "${collateral}" == "0" ]]; then
            [[ "${liability}" == "0" ]] ||
                die "SEP-41 setup account has ${symbol} liabilities without collateral"
            token="$(sep41_asset_id "${symbol}")"
            ensure_sep41_token_balance "${symbol}" "${token}" "${setup}" "${supplied}"
            requests="$(printf '%s' "${requests}" | jq -c \
                --arg address "${token}" --arg amount "${supplied}" \
                '. + [{address: $address, amount: $amount, request_type: 2}]')"
        fi
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    if [[ "$(printf '%s' "${requests}" | jq -er 'length')" != "0" ]]; then
        invoke_transaction_as "seed-sep41-supplies" "${identity}" \
            "${pool}" submit \
            --from "${setup}" --spender "${setup}" --to "${setup}" \
            --requests "${requests}" >/dev/null
    fi
}

ensure_sep41_usdc_trustline() {
    local setup="$1" identity="$2" account issuer

    issuer="$(state_value operator)"
    account="$(read_horizon_account "${setup}")"
    if ! printf '%s' "${account}" | jq -e \
        --arg issuer "${issuer}" '
            any(.balances[];
                .asset_code? == "USDC" and .asset_issuer? == $issuer)
        ' >/dev/null
    then
        capture "create-sep41-usdc-trustline" \
            stellar_cli tx new change-trust \
            --source-account "${identity}" \
            --line "USDC:${issuer}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    fi
    state_set "sep41_usdc_trustline" "created"
}

sep41_backstop_shares() {
    invoke_view "query-sep41-backstop-shares" \
        "$(state_value v3_backstop)" user_balance \
        --tier "$(sep41_fixture_value '.backstop.tier')" \
        --pool "$1" --user "$2" |
        jq -er '.shares | tostring'
}

ensure_sep41_backstop() {
    local pool="$1" setup="$2" identity="$3" target current usdc balance

    target="$(sep41_fixture_value '.backstop.amount')"
    current="$(sep41_backstop_shares "${pool}" "${setup}")"
    [[ "${current}" =~ ^[0-9]+$ ]] || die "invalid SEP-41 backstop shares"
    (( current <= target )) || die "SEP-41 backstop shares exceed fixture target"
    if (( current < target )); then
        usdc="$(state_value usdc_token)"
        balance="$(normalize_scalar "$(invoke_view \
            "query-sep41-usdc-balance" "${usdc}" balance --id "${setup}")")"
        [[ "${balance}" =~ ^[0-9]+$ ]] || die "invalid SEP-41 setup USDC balance"
        if (( balance < target - current )); then
            invoke_transaction "mint-sep41-backstop-usdc" "${usdc}" mint \
                --to "${setup}" --amount "$((target - current - balance))" >/dev/null
        fi
        invoke_transaction_as "deposit-sep41-backstop" "${identity}" \
            "$(state_value v3_backstop)" deposit \
            --tier "$(sep41_fixture_value '.backstop.tier')" \
            --from "${setup}" --pool-address "${pool}" \
            --amount "$((target - current))" >/dev/null
    fi
    state_set "sep41_backstop_deposit" "${target}"
}

ensure_sep41_pool_active() {
    local pool="$1" identity="$2" status reward_zone

    status="$(invoke_view "query-funded-sep41-status" \
        "${pool}" get_config | jq -er '.status | tostring')"
    if [[ "${status}" != "0" && "${status}" != "1" ]]; then
        invoke_transaction_as "activate-sep41-pool" "${identity}" \
            "${pool}" set_status --pool_status 0 >/dev/null
    fi
    reward_zone="$(invoke_view "query-sep41-reward-zone" \
        "$(state_value v3_backstop)" reward_zone)"
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null
    then
        sleep "${EMISSION_START_DELAY_SECONDS}"
        invoke_transaction "checkpoint-sep41-reward-zone" \
            "$(state_value v3_backstop)" distribute >/dev/null
        invoke_transaction "add-sep41-reward" \
            "$(state_value v3_backstop)" add_reward \
            --to_add "${pool}" --to_remove null >/dev/null
    fi
}

ensure_sep41_borrow_positions() {
    local pool="$1" setup="$2" identity="$3" positions requests="[]"
    local symbol token borrowed liability collateral index=0

    positions="$(sep41_positions "${pool}" "${setup}")"
    while IFS= read -r symbol; do
        borrowed="$(jq -er --arg symbol "${symbol}" \
            '.reserves[] | select(.asset == $symbol) | .borrowed' \
            "${SEP41_POOL_FIXTURE}")"
        liability="$(printf '%s' "${positions}" | jq -er \
            --arg index "${index}" '(.liabilities[$index] // "0") | tostring')"
        collateral="$(printf '%s' "${positions}" | jq -er \
            --arg index "${index}" '(.collateral[$index] // "0") | tostring')"
        [[ "${collateral}" != "0" ]] ||
            die "SEP-41 setup account is missing ${symbol} collateral"
        if [[ "${liability}" != "0" && "${liability}" != "${borrowed}" ]]; then
            die "SEP-41 setup account has partial ${symbol} liability"
        fi
        if [[ "${liability}" == "0" ]]; then
            token="$(sep41_asset_id "${symbol}")"
            requests="$(printf '%s' "${requests}" | jq -c \
                --arg address "${token}" --arg amount "${borrowed}" \
                '. + [{address: $address, amount: $amount, request_type: 4}]')"
        fi
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    if [[ "$(printf '%s' "${requests}" | jq -er 'length')" != "0" ]]; then
        seed_sep41_oracle "$(state_value sep41_pool_oracle)"
        invoke_transaction_as "seed-sep41-borrows" "${identity}" \
            "${pool}" submit \
            --from "${setup}" --spender "${setup}" --to "${setup}" \
            --requests "${requests}" >/dev/null
    fi
}

command_deploy_sep41_pool() {
    local setup identity oracle pool symbol

    require_sep41_pool_support
    setup="$(ensure_sep41_setup_identity)"
    identity="$(state_value sep41_setup_identity)"
    while IFS= read -r symbol; do
        ensure_sep41_asset "${symbol}" >/dev/null
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    oracle="$(ensure_sep41_oracle)"
    pool="$(ensure_sep41_pool "${setup}" "${identity}" "${oracle}")"
    ensure_sep41_supply_positions "${pool}" "${setup}" "${identity}"
    ensure_sep41_usdc_trustline "${setup}" "${identity}"
    ensure_sep41_backstop "${pool}" "${setup}" "${identity}"
    ensure_sep41_pool_active "${pool}" "${identity}"
    ensure_sep41_borrow_positions "${pool}" "${setup}" "${identity}"
    state_set "sep41_pool_fixture_sha256" "$(sha256_file "${SEP41_POOL_FIXTURE}")"
    command_verify_sep41_pool
}

command_verify_sep41_pool() {
    local setup pool factory backstop registered admin config status reward_zone
    local factory_config expected_factory_config positions expected_positions shares active_value
    local expected_reserves="[]" actual_reserves symbol token expected_config reserve actual_config
    local supplied borrowed custody expected_custody actual_hash expected_hash index=0

    require_sep41_pool_support
    setup="$(state_value sep41_setup_user)"
    pool="$(state_value sep41_pool)"
    factory="$(state_value v3_factory)"
    backstop="$(state_value v3_backstop)"

    registered="$(normalize_scalar "$(invoke_view \
        "verify-sep41-registration" "${factory}" is_pool \
        --pool_address "${pool}")")"
    assert_equal "true" "${registered}" "SEP-41 Assets factory registration"
    admin="$(normalize_scalar "$(invoke_view \
        "verify-sep41-admin" "${pool}" get_admin)")"
    assert_equal "${setup}" "${admin}" "SEP-41 Assets pool admin"
    config="$(invoke_view "verify-sep41-config" "${pool}" get_config)"
    status="$(printf '%s' "${config}" | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "SEP-41 Assets pool is not active: status ${status}"
    assert_equal "$(sep41_fixture_value '.backstop_take_rate')" \
        "$(printf '%s' "${config}" | jq -er '.bstop_rate | tostring')" \
        "SEP-41 Assets backstop take rate"
    assert_equal "$(state_value sep41_pool_oracle)" \
        "$(printf '%s' "${config}" | jq -er '.oracle')" \
        "SEP-41 Assets oracle"

    factory_config="$(invoke_view "verify-sep41-factory-config" \
        "${factory}" backstop_config --pool_address "${pool}")"
    expected_factory_config="$(jq -cn \
        --argjson tiers "$(jq -c '.backstop_config' "${SEP41_POOL_FIXTURE}")" \
        '{access_controller: null, tiers: $tiers}')"
    assert_json_equal "${expected_factory_config}" "${factory_config}" \
        "SEP-41 Assets access and backstop configuration"

    positions="$(sep41_positions "${pool}" "${setup}")"
    expected_positions='{"collateral":{},"liabilities":{},"supply":{}}'
    actual_reserves="$(invoke_view "verify-sep41-reserve-list" \
        "${pool}" get_reserve_list)"
    while IFS= read -r symbol; do
        token="$(sep41_asset_id "${symbol}")"
        expected_reserves="$(printf '%s' "${expected_reserves}" | jq -c \
            --arg token "${token}" '. + [$token]')"
        expected_config="$(sep41_reserve_config "${index}")"
        reserve="$(invoke_view "verify-sep41-${symbol}-reserve" \
            "${pool}" get_reserve --asset "${token}")"
        actual_config="$(printf '%s' "${reserve}" | jq -c '.config')"
        assert_json_equal "${expected_config}" "${actual_config}" \
            "SEP-41 ${symbol} reserve configuration"
        supplied="$(jq -er --arg symbol "${symbol}" \
            '.reserves[] | select(.asset == $symbol) | .supplied' \
            "${SEP41_POOL_FIXTURE}")"
        borrowed="$(jq -er --arg symbol "${symbol}" \
            '.reserves[] | select(.asset == $symbol) | .borrowed' \
            "${SEP41_POOL_FIXTURE}")"
        state_set "sep41_${symbol}_supplied" "${supplied}"
        state_set "sep41_${symbol}_borrowed" "${borrowed}"
        assert_equal "${supplied}" \
            "$(printf '%s' "${reserve}" | jq -er '.data.b_supply | tostring')" \
            "SEP-41 ${symbol} total supply"
        assert_equal "${borrowed}" \
            "$(printf '%s' "${reserve}" | jq -er '.data.d_supply | tostring')" \
            "SEP-41 ${symbol} total liabilities"
        expected_positions="$(printf '%s' "${expected_positions}" | jq -c \
            --arg index "${index}" --arg supplied "${supplied}" --arg borrowed "${borrowed}" \
            '.collateral[$index] = $supplied | .liabilities[$index] = $borrowed')"
        expected_custody=$((supplied - borrowed))
        custody="$(sep41_token_balance "${symbol}" "${token}" "${pool}")"
        assert_equal "${expected_custody}" "${custody}" \
            "SEP-41 ${symbol} pool custody"
        expected_hash="$(sha256_file "${SEP41_TOKEN_WASM}")"
        actual_hash="$(sep41_contract_wasm_hash "${token}")"
        assert_equal "${expected_hash}" "${actual_hash}" \
            "SEP-41 ${symbol} custom-token executable"
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    assert_json_equal "${expected_reserves}" "${actual_reserves}" \
        "SEP-41 Assets reserve list"
    assert_json_equal "${expected_positions}" "${positions}" \
        "SEP-41 Assets setup positions"

    shares="$(sep41_backstop_shares "${pool}" "${setup}")"
    assert_equal "$(sep41_fixture_value '.backstop.amount')" "${shares}" \
        "SEP-41 Assets backstop shares"
    active_value="$(invoke_view "verify-sep41-backstop-value" \
        "${backstop}" pool_data --pool "${pool}" | jq -er '.active_value | tostring')"
    (( active_value >= ACTIVATION_USDC_BALANCE )) ||
        die "SEP-41 Assets pool is below the activation threshold: ${active_value}"
    reward_zone="$(invoke_view "verify-sep41-reward-zone" \
        "${backstop}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
        die "SEP-41 Assets pool is missing from its reward zone"

    state_set "sep41_pool_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    cat <<EOF
${NETWORK_LABEL} SEP-41 Assets pool verified
  pool:       ${pool} (active; reward-zone member)
  setup user: ${setup}
  wBTC:       500 supplied; 200 borrowed
  wETH:       10,000 supplied; 5,000 borrowed
  backstop:   25,000 USDC in FirstLoss
EOF
}
