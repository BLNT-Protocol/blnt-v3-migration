#!/usr/bin/env bash

# Copyright (C) 2026 BLNT Protocol contributors
# SPDX-License-Identifier: AGPL-3.0-only

# This file is sourced by v3-only-deployment.sh after the common deployment
# helpers and V3-only configuration have been loaded.

PERMISSIONED_POOL_FIXTURE="${ROOT_DIR}/fixtures/permissioned-pool.json"
PERMISSIONED_ACCESS_CONTROLLER_WASM="${ROOT_DIR}/migration-harness/target/wasm32v1-none/optimized/blnt_test_access_controller.wasm"
PERMISSIONED_SETUP_IDENTITY="${BLEND_PERMISSIONED_SETUP_IDENTITY:-blnt-v3-${NETWORK_MODE}-permissioned-setup}"
PERMISSIONED_CONTROLLER_SALT="${BLEND_PERMISSIONED_CONTROLLER_SALT:-00000000000000000000000000000000000000000000000000000000000000ac}"
PERMISSIONED_POOL_SALT="${BLEND_PERMISSIONED_POOL_SALT:-5065726d697373696f6e65640000000000000000000000000000000000000000}"
PERMISSIONED_ALL_PERMISSIONS=7
PERMISSIONED_NATIVE_XLM_BUFFER=$((1000 * SCALAR_7))

require_permissioned_pool_support() {
    [[ -s "${PERMISSIONED_POOL_FIXTURE}" ]] ||
        die "permissioned-pool fixture is missing: ${PERMISSIONED_POOL_FIXTURE}"
    [[ -s "${PERMISSIONED_ACCESS_CONTROLLER_WASM}" ]] ||
        die "test access-controller WASM is missing: ${PERMISSIONED_ACCESS_CONTROLLER_WASM}"
}

permissioned_fixture_value() {
    jq -er "$1" "${PERMISSIONED_POOL_FIXTURE}"
}

ensure_permissioned_setup_identity() {
    local identity saved_identity saved_user user

    saved_identity="$(state_optional permissioned_setup_identity)"
    identity="${saved_identity:-${PERMISSIONED_SETUP_IDENTITY}}"
    saved_user="$(state_optional permissioned_setup_user)"
    if ! stellar_cli keys public-key "${identity}" >/dev/null 2>&1; then
        [[ -z "${saved_user}" ]] ||
            die "saved permissioned setup identity is unavailable: ${identity}"
        note "Creating ${NETWORK_LABEL} permissioned-pool setup identity ${identity}..."
        stellar_cli keys generate "${identity}" --overwrite >/dev/null
    fi
    user="$(stellar_cli keys public-key "${identity}")"
    if [[ -n "${saved_user}" ]]; then
        assert_equal "${saved_user}" "${user}" "permissioned setup identity"
    else
        fund_and_wait_for_account "${user}"
    fi
    [[ "${user}" != "${V3_USER_WALLET}" ]] ||
        die "permissioned setup identity must not be the configured test user"
    state_set "permissioned_setup_identity" "${identity}"
    state_set "permissioned_setup_user" "${user}"
    printf '%s\n' "${user}"
}

permissioned_asset_id() {
    case "$1" in
        XLM) state_value xlm_token ;;
        EURC) state_value modeled_asset_eurc ;;
        *) die "unsupported permissioned-pool asset: $1" ;;
    esac
}

permissioned_token_balance() {
    local label="$1" token="$2" user="$3"

    normalize_scalar "$(invoke_view \
        "query-permissioned-${label}-balance" "${token}" balance --id "${user}")"
}

ensure_permissioned_native_xlm() {
    local user="$1" required="$2" balance donor_identity donor donor_index=0

    balance="$(permissioned_token_balance xlm "$(state_value xlm_token)" "${user}")"
    [[ "${balance}" =~ ^[0-9]+$ ]] || die "invalid permissioned setup XLM balance"
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        if (( balance < required )); then
            invoke_transaction "mint-permissioned-xlm" "$(state_value xlm_token)" mint \
                --to "${user}" --amount "$((required - balance))" >/dev/null
        fi
        return
    fi

    while (( balance < required )); do
        donor_index=$((donor_index + 1))
        (( donor_index <= 12 )) ||
            die "unable to fund the permissioned setup account with enough native XLM"
        donor_identity="${PERMISSIONED_SETUP_IDENTITY}-funding-${donor_index}"
        stellar_cli keys generate "${donor_identity}" --overwrite >/dev/null
        donor="$(stellar_cli keys public-key "${donor_identity}")"
        fund_and_wait_for_account "${donor}"
        note "Merging Friendbot-funded ${donor_identity} into the permissioned setup account..."
        stellar_cli tx new account-merge \
            --source-account "${donor_identity}" \
            --account "${user}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        stellar_cli keys rm "${donor_identity}" --force >/dev/null
        balance="$(permissioned_token_balance \
            "xlm-after-funding-${donor_index}" "$(state_value xlm_token)" "${user}")"
        [[ "${balance}" =~ ^[0-9]+$ ]] ||
            die "invalid permissioned setup XLM balance after Friendbot funding"
    done
}

ensure_permissioned_trustline() {
    local code="$1" user="$2" identity="$3" account issuer label

    issuer="$(state_value operator)"
    account="$(read_horizon_account "${user}")"
    if ! printf '%s' "${account}" | jq -e \
        --arg code "${code}" --arg issuer "${issuer}" '
            any(.balances[];
                .asset_code? == $code and .asset_issuer? == $issuer)
        ' >/dev/null
    then
        capture "create-permissioned-${code}-trustline" \
            stellar_cli tx new change-trust \
            --source-account "${identity}" \
            --line "${code}:${issuer}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    fi
    label="$(printf '%s' "${code}" | tr '[:upper:]' '[:lower:]')"
    state_set "permissioned_${label}_trustline" "created"
}

ensure_permissioned_token_balance() {
    local label="$1" token="$2" user="$3" required="$4" balance

    balance="$(permissioned_token_balance "${label}" "${token}" "${user}")"
    [[ "${balance}" =~ ^[0-9]+$ ]] ||
        die "invalid permissioned setup ${label} balance"
    if (( balance < required )); then
        invoke_transaction "mint-permissioned-${label}" "${token}" mint \
            --to "${user}" --amount "$((required - balance))" >/dev/null
    fi
}

permissioned_contract_wasm_hash() {
    capture "query-permissioned-controller-wasm-hash" \
        stellar_cli ledger entry fetch contract-data \
        --contract "$1" --instance \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json |
        jq -er '.entries[0].val.contract_data.val.contract_instance.executable.wasm'
}

ensure_permissioned_access_controller() {
    local user="$1" controller permission expected_hash actual_hash

    controller="$(state_optional permissioned_access_controller)"
    if [[ -z "${controller}" ]]; then
        controller="$(deploy_wasm \
            "deploy-permissioned-access-controller" \
            "${PERMISSIONED_ACCESS_CONTROLLER_WASM}" \
            "${PERMISSIONED_CONTROLLER_SALT}" \
            --allowed_user "${user}")"
        state_set "permissioned_access_controller" "${controller}"
    fi
    require_contract_id "${controller}" "permissioned access controller"
    expected_hash="$(sha256_file "${PERMISSIONED_ACCESS_CONTROLLER_WASM}")"
    actual_hash="$(permissioned_contract_wasm_hash "${controller}")"
    assert_equal "${expected_hash}" "${actual_hash}" \
        "permissioned access controller WASM hash"
    permission="$(normalize_scalar "$(invoke_view \
        "query-permissioned-setup-permissions" "${controller}" permissions \
        --pool "$(state_value v3_factory)" --user "${user}")")"
    assert_equal "${PERMISSIONED_ALL_PERMISSIONS}" "${permission}" \
        "permissioned setup permission mask"
    state_set "permissioned_access_controller_wasm_sha256" \
        "${actual_hash}"
    printf '%s\n' "${controller}"
}

permissioned_reserve_config() {
    local index="$1"

    jq -c --argjson index "${index}" '
        .reserves[$index].config + {
            decimals: 7,
            enabled: true,
            index: $index
        }
    ' "${PERMISSIONED_POOL_FIXTURE}"
}

ensure_permissioned_pool() {
    local setup="$1" identity="$2" controller="$3" oracle="$4"
    local pool raw reserve_list code token config index=0 status

    pool="$(state_optional permissioned_pool)"
    if [[ -z "${pool}" ]]; then
        raw="$(invoke_transaction_as \
            "deploy-permissioned-pool" "${identity}" \
            "$(state_value v3_factory)" deploy \
            --admin "${setup}" \
            --name "$(jq -c '.name' "${PERMISSIONED_POOL_FIXTURE}")" \
            --salt "${PERMISSIONED_POOL_SALT}" \
            --oracle "${oracle}" \
            --backstop_take_rate "$(permissioned_fixture_value '.backstop_take_rate')" \
            --max_positions "$(permissioned_fixture_value '.max_positions')" \
            --min_collateral "$(permissioned_fixture_value '.min_collateral')" \
            --backstop_config "$(jq -c '.backstop_config' "${PERMISSIONED_POOL_FIXTURE}")" \
            --access_controller "$(jq -cn --arg controller "${controller}" '$controller')")"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "permissioned pool"
        state_set "permissioned_pool" "${pool}"
        state_set "permissioned_pool_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fi
    require_contract_id "${pool}" "permissioned pool"
    state_set "permissioned_pool_oracle" "${oracle}"

    reserve_list="$(invoke_view \
        "query-permissioned-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(permissioned_asset_id "${code}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg token "${token}" 'index($token) != null' >/dev/null
        then
            config="$(permissioned_reserve_config "${index}")"
            invoke_transaction_as "queue-permissioned-${code}" "${identity}" \
                "${pool}" queue_set_reserve \
                --asset "${token}" --metadata "${config}" >/dev/null
            invoke_transaction_as "set-permissioned-${code}" "${identity}" \
                "${pool}" set_reserve --asset "${token}" >/dev/null
            reserve_list="$(invoke_view \
                "query-permissioned-reserves-${code}" "${pool}" get_reserve_list)"
        fi
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${PERMISSIONED_POOL_FIXTURE}")

    status="$(invoke_view "query-permissioned-status" "${pool}" get_config |
        jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        invoke_transaction_as "on-ice-permissioned-pool" "${identity}" \
            "${pool}" set_status --pool_status 2 >/dev/null
    elif [[ "${status}" != "0" && "${status}" != "1" && "${status}" != "2" ]]; then
        die "permissioned pool has unexpected status ${status}"
    fi
    printf '%s\n' "${pool}"
}

permissioned_positions() {
    invoke_view "query-permissioned-positions" "$1" get_positions --address "$2"
}

ensure_permissioned_supply_positions() {
    local pool="$1" setup="$2" identity="$3" positions supply xlm eurc
    local xlm_collateral eurc_collateral xlm_liability eurc_liability

    supply="$(permissioned_fixture_value '.positions.supplied')"
    positions="$(permissioned_positions "${pool}" "${setup}")"
    xlm_collateral="$(printf '%s' "${positions}" | jq -er '(.collateral["0"] // "0") | tostring')"
    eurc_collateral="$(printf '%s' "${positions}" | jq -er '(.collateral["1"] // "0") | tostring')"
    xlm_liability="$(printf '%s' "${positions}" | jq -er '(.liabilities["0"] // "0") | tostring')"
    eurc_liability="$(printf '%s' "${positions}" | jq -er '(.liabilities["1"] // "0") | tostring')"
    if [[ "${xlm_collateral}" == "${supply}" && "${eurc_collateral}" == "${supply}" ]]; then
        return
    fi
    [[ "${xlm_collateral}" == "0" && "${eurc_collateral}" == "0" &&
        "${xlm_liability}" == "0" && "${eurc_liability}" == "0" ]] ||
        die "permissioned setup account has partial or unexpected positions"

    xlm="$(permissioned_asset_id XLM)"
    eurc="$(permissioned_asset_id EURC)"
    ensure_permissioned_native_xlm \
        "${setup}" "$((supply + PERMISSIONED_NATIVE_XLM_BUFFER))"
    ensure_permissioned_token_balance eurc "${eurc}" "${setup}" "${supply}"
    invoke_transaction_as "seed-permissioned-supplies" "${identity}" \
        "${pool}" submit \
        --from "${setup}" --spender "${setup}" --to "${setup}" \
        --requests "[{\"address\":\"${xlm}\",\"amount\":\"${supply}\",\"request_type\":2},{\"address\":\"${eurc}\",\"amount\":\"${supply}\",\"request_type\":2}]" \
        >/dev/null
}

permissioned_backstop_shares() {
    invoke_view "query-permissioned-backstop-shares" \
        "$(state_value v3_backstop)" user_balance \
        --tier "$(permissioned_fixture_value '.backstop.tier')" \
        --pool "$1" --user "$2" |
        jq -er '.shares | tostring'
}

ensure_permissioned_backstop() {
    local pool="$1" setup="$2" identity="$3" target current usdc

    target="$(permissioned_fixture_value '.backstop.amount')"
    current="$(permissioned_backstop_shares "${pool}" "${setup}")"
    [[ "${current}" =~ ^[0-9]+$ ]] || die "invalid permissioned backstop shares"
    (( current <= target )) || die "permissioned backstop shares exceed fixture target"
    if (( current < target )); then
        usdc="$(state_value usdc_token)"
        ensure_permissioned_token_balance \
            usdc "${usdc}" "${setup}" "$((target - current))"
        invoke_transaction_as "deposit-permissioned-backstop" "${identity}" \
            "$(state_value v3_backstop)" deposit \
            --tier "$(permissioned_fixture_value '.backstop.tier')" \
            --from "${setup}" --pool-address "${pool}" \
            --amount "$((target - current))" >/dev/null
    fi
    state_set "permissioned_backstop_deposit" "${target}"
}

ensure_permissioned_pool_active() {
    local pool="$1" identity="$2" status reward_zone

    status="$(invoke_view "query-funded-permissioned-status" \
        "${pool}" get_config | jq -er '.status | tostring')"
    if [[ "${status}" != "0" && "${status}" != "1" ]]; then
        invoke_transaction_as "activate-permissioned-pool" "${identity}" \
            "${pool}" set_status --pool_status 0 >/dev/null
    fi
    reward_zone="$(invoke_view "query-permissioned-reward-zone" \
        "$(state_value v3_backstop)" reward_zone)"
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null
    then
        sleep "${EMISSION_START_DELAY_SECONDS}"
        invoke_transaction "checkpoint-permissioned-reward-zone" \
            "$(state_value v3_backstop)" distribute >/dev/null
        invoke_transaction "add-permissioned-reward" \
            "$(state_value v3_backstop)" add_reward \
            --to_add "${pool}" --to_remove null >/dev/null
    fi
}

ensure_permissioned_borrow_positions() {
    local pool="$1" setup="$2" identity="$3" positions borrow xlm eurc
    local xlm_liability eurc_liability

    borrow="$(permissioned_fixture_value '.positions.borrowed')"
    positions="$(permissioned_positions "${pool}" "${setup}")"
    xlm_liability="$(printf '%s' "${positions}" | jq -er '(.liabilities["0"] // "0") | tostring')"
    eurc_liability="$(printf '%s' "${positions}" | jq -er '(.liabilities["1"] // "0") | tostring')"
    if [[ "${xlm_liability}" == "${borrow}" && "${eurc_liability}" == "${borrow}" ]]; then
        return
    fi
    [[ "${xlm_liability}" == "0" && "${eurc_liability}" == "0" ]] ||
        die "permissioned setup account has partial or unexpected liabilities"
    seed_modeled_sep40_oracle v3 fixed "$(state_value permissioned_pool_oracle)"
    xlm="$(permissioned_asset_id XLM)"
    eurc="$(permissioned_asset_id EURC)"
    invoke_transaction_as "seed-permissioned-borrows" "${identity}" \
        "${pool}" submit \
        --from "${setup}" --spender "${setup}" --to "${setup}" \
        --requests "[{\"address\":\"${xlm}\",\"amount\":\"${borrow}\",\"request_type\":4},{\"address\":\"${eurc}\",\"amount\":\"${borrow}\",\"request_type\":4}]" \
        >/dev/null
}

command_deploy_permissioned_pool() {
    local setup identity controller oracle pool supply borrow eurc

    require_permissioned_pool_support
    eurc="$(state_optional modeled_asset_eurc)"
    if [[ -z "${eurc}" ]]; then
        ensure_modeled_asset EURC >/dev/null
    else
        require_contract_id "${eurc}" "modeled EURC"
    fi
    oracle="$(state_optional modeled_v3_fixed_oracle)"
    [[ -n "${oracle}" ]] || oracle="$(ensure_modeled_v3_oracle fixed)"
    setup="$(ensure_permissioned_setup_identity)"
    identity="$(state_value permissioned_setup_identity)"
    ensure_permissioned_trustline EURC "${setup}" "${identity}"
    ensure_permissioned_trustline USDC "${setup}" "${identity}"
    controller="$(ensure_permissioned_access_controller "${setup}")"
    pool="$(ensure_permissioned_pool \
        "${setup}" "${identity}" "${controller}" "${oracle}")"
    ensure_permissioned_supply_positions "${pool}" "${setup}" "${identity}"
    ensure_permissioned_backstop "${pool}" "${setup}" "${identity}"
    ensure_permissioned_pool_active "${pool}" "${identity}"
    ensure_permissioned_borrow_positions "${pool}" "${setup}" "${identity}"
    supply="$(permissioned_fixture_value '.positions.supplied')"
    borrow="$(permissioned_fixture_value '.positions.borrowed')"
    state_set "permissioned_supply_amount" "${supply}"
    state_set "permissioned_borrow_amount" "${borrow}"
    state_set "permissioned_pool_fixture_sha256" \
        "$(sha256_file "${PERMISSIONED_POOL_FIXTURE}")"
    command_verify_permissioned_pool
}

command_verify_permissioned_pool() {
    local setup controller pool factory backstop registered admin status reward_zone
    local config factory_config expected_factory_config permission active_value
    local expected_controller_hash actual_controller_hash
    local positions expected_positions supply borrow shares user_balance
    local expected_reserves="[]" actual_reserves
    local code token expected_config actual_config reserve index=0

    require_permissioned_pool_support
    setup="$(state_value permissioned_setup_user)"
    controller="$(state_value permissioned_access_controller)"
    pool="$(state_value permissioned_pool)"
    factory="$(state_value v3_factory)"
    backstop="$(state_value v3_backstop)"
    [[ "${setup}" != "${V3_USER_WALLET}" ]] ||
        die "permissioned setup identity is the configured test user"

    registered="$(normalize_scalar "$(invoke_view \
        "verify-permissioned-registration" "${factory}" is_pool \
        --pool_address "${pool}")")"
    assert_equal "true" "${registered}" "permissioned factory registration"
    admin="$(normalize_scalar "$(invoke_view \
        "verify-permissioned-admin" "${pool}" get_admin)")"
    assert_equal "${setup}" "${admin}" "permissioned pool admin"
    config="$(invoke_view "verify-permissioned-config" "${pool}" get_config)"
    status="$(printf '%s' "${config}" | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "permissioned pool is not active: status ${status}"
    assert_equal "$(permissioned_fixture_value '.backstop_take_rate')" \
        "$(printf '%s' "${config}" | jq -er '.bstop_rate | tostring')" \
        "permissioned backstop take rate"
    assert_equal "$(permissioned_fixture_value '.max_positions')" \
        "$(printf '%s' "${config}" | jq -er '.max_positions | tostring')" \
        "permissioned max positions"
    assert_equal "$(permissioned_fixture_value '.min_collateral')" \
        "$(printf '%s' "${config}" | jq -er '.min_collateral | tostring')" \
        "permissioned minimum collateral"
    assert_equal "$(state_value permissioned_pool_oracle)" \
        "$(printf '%s' "${config}" | jq -er '.oracle')" \
        "permissioned oracle"

    factory_config="$(invoke_view "verify-permissioned-factory-config" \
        "${factory}" backstop_config --pool_address "${pool}")"
    expected_factory_config="$(jq -cn \
        --arg controller "${controller}" \
        --argjson tiers "$(jq -c '.backstop_config' "${PERMISSIONED_POOL_FIXTURE}")" \
        '{access_controller: $controller, tiers: $tiers}')"
    assert_json_equal "${expected_factory_config}" "${factory_config}" \
        "permissioned controller and backstop configuration"

    permission="$(normalize_scalar "$(invoke_view \
        "verify-permissioned-setup-mask" "${controller}" permissions \
        --pool "${pool}" --user "${setup}")")"
    assert_equal "7" "${permission}" "permissioned setup permission mask"
    permission="$(normalize_scalar "$(invoke_view \
        "verify-permissioned-other-mask" "${controller}" permissions \
        --pool "${pool}" --user "$(state_value operator)")")"
    assert_equal "0" "${permission}" "permissioned default permission mask"
    expected_controller_hash="$(sha256_file "${PERMISSIONED_ACCESS_CONTROLLER_WASM}")"
    actual_controller_hash="$(permissioned_contract_wasm_hash "${controller}")"
    assert_equal "${expected_controller_hash}" "${actual_controller_hash}" \
        "permissioned access controller WASM hash"
    state_set "permissioned_access_controller_wasm_sha256" \
        "${actual_controller_hash}"

    actual_reserves="$(invoke_view "verify-permissioned-reserve-list" \
        "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(permissioned_asset_id "${code}")"
        expected_reserves="$(printf '%s' "${expected_reserves}" | jq -c \
            --arg token "${token}" '. + [$token]')"
        expected_config="$(permissioned_reserve_config "${index}")"
        reserve="$(invoke_view "verify-permissioned-${code}-config" \
            "${pool}" get_reserve --asset "${token}")"
        actual_config="$(printf '%s' "${reserve}" | jq -c '.config')"
        assert_json_equal "${expected_config}" "${actual_config}" \
            "permissioned ${code} reserve configuration"
        assert_equal "$(permissioned_fixture_value '.positions.supplied')" \
            "$(printf '%s' "${reserve}" | jq -er '.data.b_supply | tostring')" \
            "permissioned ${code} total supply"
        assert_equal "$(permissioned_fixture_value '.positions.borrowed')" \
            "$(printf '%s' "${reserve}" | jq -er '.data.d_supply | tostring')" \
            "permissioned ${code} total liabilities"
        index=$((index + 1))
    done < <(jq -r '.reserves[].asset' "${PERMISSIONED_POOL_FIXTURE}")
    assert_json_equal "${expected_reserves}" "${actual_reserves}" \
        "permissioned reserve list"

    supply="$(permissioned_fixture_value '.positions.supplied')"
    borrow="$(permissioned_fixture_value '.positions.borrowed')"
    positions="$(permissioned_positions "${pool}" "${setup}")"
    expected_positions="$(jq -cn --arg supply "${supply}" --arg borrow "${borrow}" '
        {
          collateral: {"0": $supply, "1": $supply},
          liabilities: {"0": $borrow, "1": $borrow},
          supply: {}
        }
    ')"
    assert_json_equal "${expected_positions}" "${positions}" \
        "permissioned setup positions"

    shares="$(permissioned_backstop_shares "${pool}" "${setup}")"
    assert_equal "$(permissioned_fixture_value '.backstop.amount')" "${shares}" \
        "permissioned backstop shares"
    user_balance="$(invoke_view "verify-permissioned-backstop-queues" \
        "${backstop}" user_balance \
        --tier "$(permissioned_fixture_value '.backstop.tier')" \
        --pool "${pool}" --user "${setup}")"
    assert_equal "0" "$(printf '%s' "${user_balance}" | jq -er '.q4w | length')" \
        "permissioned queued backstop withdrawals"
    active_value="$(invoke_view "verify-permissioned-backstop-value" \
        "${backstop}" pool_data --pool "${pool}" | jq -er '.active_value | tostring')"
    (( active_value >= ACTIVATION_USDC_BALANCE )) ||
        die "permissioned pool is below the activation threshold: ${active_value}"
    reward_zone="$(invoke_view "verify-permissioned-reward-zone" \
        "${backstop}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
        die "permissioned pool is missing from its reward zone"

    state_set "permissioned_pool_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    cat <<EOF
${NETWORK_LABEL} permissioned pool verified
  pool:        ${pool} (active; reward-zone member)
  controller:  ${controller} (setup mask 7; default mask 0)
  setup user:  ${setup}
  XLM:         100,000 supplied; 50,000 borrowed
  EURC:        100,000 supplied; 50,000 borrowed
  backstop:    25,000 USDC in FirstLoss
EOF
}
