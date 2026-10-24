#!/usr/bin/env bash

# Copyright (C) 2026 BLNT Protocol contributors
# SPDX-License-Identifier: AGPL-3.0-only

# This file is sourced by v3-only-deployment.sh after the common deployment
# helpers and V3-only configuration have been loaded.

NO_BACKSTOP_POOL_SALT="${BLEND_NO_BACKSTOP_POOL_SALT:-00000000000000000000000000000000000000000000000000000000000000d0}"

require_no_backstop_pool_support() {
    require_modeled_pool_support
}

no_backstop_fixed_value() {
    jq -er ".pools.fixed$1" "${MODELED_POOLS_FIXTURE}"
}

no_backstop_reserve_config() {
    modeled_v2_reserve_config fixed "$1"
}

record_no_backstop_provenance() {
    local patch untracked_list untracked status commit dirty=false digest=""

    patch="${RUN_DIR}/no-backstop-source.patch"
    untracked_list="${RUN_DIR}/no-backstop-untracked-files"
    commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    git -C "${ROOT_DIR}" diff --binary HEAD -- . >"${patch}"
    git -C "${ROOT_DIR}" ls-files --others --exclude-standard -z -- . \
        >"${untracked_list}"
    while IFS= read -r -d '' untracked; do
        git -C "${ROOT_DIR}" diff --binary --no-index -- \
            /dev/null "${untracked}" >>"${patch}" || {
            status=$?
            (( status == 1 )) || return "${status}"
        }
    done <"${untracked_list}"
    rm -f -- "${untracked_list}"
    if [[ -s "${patch}" ]]; then
        dirty=true
        digest="$(sha256_file "${patch}")"
    fi
    state_set "no_backstop_source_commit" "${commit}"
    state_set "no_backstop_source_dirty" "${dirty}"
    state_set "no_backstop_source_patch_sha256" "${digest}"
}

ensure_no_backstop_pool() {
    local oracle="$1" pool raw reserve_list code token config index=0 status

    pool="$(state_optional no_backstop_pool)"
    if [[ -z "${pool}" ]]; then
        raw="$(invoke_transaction "deploy-no-backstop-pool" \
            "$(state_value v3_factory)" deploy \
            --admin "$(state_value operator)" \
            --name "No Backstop" \
            --salt "${NO_BACKSTOP_POOL_SALT}" \
            --oracle "${oracle}" \
            --backstop_take_rate 0 \
            --max_positions "$(no_backstop_fixed_value '.max_positions')" \
            --min_collateral "$(no_backstop_fixed_value '.min_collateral')" \
            --backstop_config '[]' \
            --access_controller null)"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "No Backstop pool"
        state_set "no_backstop_pool" "${pool}"
        state_set "no_backstop_pool_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fi
    require_contract_id "${pool}" "No Backstop pool"
    state_set "no_backstop_pool_oracle" "${oracle}"

    reserve_list="$(invoke_view \
        "query-no-backstop-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(modeled_asset_id "${code}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg token "${token}" 'index($token) != null' >/dev/null
        then
            config="$(no_backstop_reserve_config "${index}")"
            if invoke_view "probe-no-backstop-queued-${code}" \
                "${pool}" set_reserve --asset "${token}" >/dev/null
            then
                invoke_transaction "resume-no-backstop-${code}" \
                    "${pool}" set_reserve --asset "${token}" >/dev/null
            else
                invoke_transaction "queue-no-backstop-${code}" \
                    "${pool}" queue_set_reserve \
                    --asset "${token}" --metadata "${config}" >/dev/null
                invoke_transaction "set-no-backstop-${code}" \
                    "${pool}" set_reserve --asset "${token}" >/dev/null
            fi
            reserve_list="$(invoke_view \
                "query-no-backstop-reserves-${code}" \
                "${pool}" get_reserve_list)"
            printf '%s' "${reserve_list}" | jq -e \
                --arg token "${token}" 'index($token) != null' >/dev/null ||
                die "No Backstop reserve ${code} was not committed"
        fi
        index=$((index + 1))
    done < <(jq -r '.pools.fixed.reserves[].asset' "${MODELED_POOLS_FIXTURE}")

    status="$(invoke_view "query-no-backstop-status" "${pool}" get_config |
        jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        invoke_transaction "on-ice-no-backstop-pool" \
            "${pool}" set_status --pool_status 2 >/dev/null
    elif [[ "${status}" != "0" && "${status}" != "1" && "${status}" != "2" ]]; then
        die "No Backstop pool has unexpected status ${status}"
    fi
    printf '%s\n' "${pool}"
}

ensure_no_backstop_pool_active() {
    local pool="$1" status

    status="$(invoke_view "query-no-backstop-pre-activation" \
        "${pool}" get_config | jq -er '.status | tostring')"
    if [[ "${status}" != "0" && "${status}" != "1" ]]; then
        invoke_transaction "activate-no-backstop-pool" \
            "${pool}" set_status --pool_status 0 >/dev/null
    fi
    status="$(invoke_view "query-no-backstop-post-activation" \
        "${pool}" get_config | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "No Backstop pool is not active: status ${status}"
}

command_deploy_no_backstop_pool() {
    local oracle pool

    require_no_backstop_pool_support
    oracle="$(state_value modeled_v3_fixed_oracle)"
    pool="$(ensure_no_backstop_pool "${oracle}")"
    ensure_no_backstop_pool_active "${pool}"
    ensure_modeled_v3_supply no-backstop "${pool}"
    ensure_fixed_v3_eurc_borrow "${pool}" no-backstop
    command_verify_no_backstop_pool
}

command_verify_no_backstop_pool() {
    local pool factory backstop registered admin config status factory_config
    local expected_reserves="[]" actual_reserves code token expected_config reserve
    local actual_config index=0 positions expected_positions pool_data reward_zone

    require_no_backstop_pool_support
    pool="$(state_value no_backstop_pool)"
    factory="$(state_value v3_factory)"
    backstop="$(state_value v3_backstop)"

    registered="$(normalize_scalar "$(invoke_view \
        "verify-no-backstop-registration" "${factory}" is_pool \
        --pool_address "${pool}")")"
    assert_equal "true" "${registered}" "No Backstop factory registration"
    admin="$(normalize_scalar "$(invoke_view \
        "verify-no-backstop-admin" "${pool}" get_admin)")"
    assert_equal "$(state_value operator)" "${admin}" "No Backstop pool admin"

    config="$(invoke_view "verify-no-backstop-config" "${pool}" get_config)"
    status="$(printf '%s' "${config}" | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "No Backstop pool is not active: status ${status}"
    assert_equal "0" \
        "$(printf '%s' "${config}" | jq -er '.bstop_rate | tostring')" \
        "No Backstop take rate"
    assert_equal "$(no_backstop_fixed_value '.max_positions')" \
        "$(printf '%s' "${config}" | jq -er '.max_positions | tostring')" \
        "No Backstop max positions"
    assert_equal "$(no_backstop_fixed_value '.min_collateral')" \
        "$(printf '%s' "${config}" | jq -er '.min_collateral | tostring')" \
        "No Backstop minimum collateral"
    assert_equal "$(state_value modeled_v3_fixed_oracle)" \
        "$(printf '%s' "${config}" | jq -er '.oracle')" \
        "No Backstop oracle"

    factory_config="$(invoke_view "verify-no-backstop-factory-config" \
        "${factory}" backstop_config --pool_address "${pool}")"
    assert_json_equal '{"access_controller":null,"tiers":[]}' \
        "${factory_config}" "No Backstop access and tier configuration"

    actual_reserves="$(invoke_view "verify-no-backstop-reserve-list" \
        "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(modeled_asset_id "${code}")"
        expected_reserves="$(printf '%s' "${expected_reserves}" | jq -c \
            --arg token "${token}" '. + [$token]')"
        expected_config="$(no_backstop_reserve_config "${index}")"
        reserve="$(invoke_view "verify-no-backstop-${code}-reserve" \
            "${pool}" get_reserve --asset "${token}")"
        actual_config="$(printf '%s' "${reserve}" | jq -c '.config')"
        assert_json_equal "${expected_config}" "${actual_config}" \
            "No Backstop ${code} reserve configuration"
        index=$((index + 1))
    done < <(jq -r '.pools.fixed.reserves[].asset' "${MODELED_POOLS_FIXTURE}")
    assert_json_equal "${expected_reserves}" "${actual_reserves}" \
        "No Backstop reserve list"

    positions="$(invoke_view "verify-no-backstop-positions" \
        "${pool}" get_positions --address "$(state_value depositor)")"
    expected_positions="$(jq -cn \
        --arg usdc_supply "${POOL_SUPPLY_BALANCE}" \
        --arg usdc_collateral "${MODELED_FIXED_V3_USDC_COLLATERAL}" \
        --arg eurc_supply "${MODELED_FIXED_V3_EURC_SUPPLY}" \
        --arg eurc_borrow "${MODELED_FIXED_V3_EURC_BORROW}" '
        {
          collateral: {"1": $usdc_collateral},
          liabilities: {"2": $eurc_borrow},
          supply: {"1": $usdc_supply, "2": $eurc_supply}
        }
    ')"
    assert_json_equal "${expected_positions}" "${positions}" \
        "No Backstop fixture positions"

    pool_data="$(invoke_view "verify-no-backstop-pool-data" \
        "${backstop}" pool_data --pool "${pool}")"
    assert_equal "0" \
        "$(printf '%s' "${pool_data}" | jq -er '.active_value | tostring')" \
        "No Backstop active value"
    assert_equal "0" \
        "$(printf '%s' "${pool_data}" | jq -er '.q4w_pct | tostring')" \
        "No Backstop queued-withdrawal percentage"
    assert_equal "0" \
        "$(printf '%s' "${pool_data}" | jq -er '.tiers | length | tostring')" \
        "No Backstop tier count"

    reward_zone="$(invoke_view "verify-no-backstop-reward-zone" \
        "${backstop}" reward_zone)"
    if printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null
    then
        die "No Backstop pool unexpectedly entered the reward zone"
    fi

    state_set "no_backstop_pool_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    state_set "no_backstop_usdc_supply" "${POOL_SUPPLY_BALANCE}"
    state_set "no_backstop_usdc_collateral" "${MODELED_FIXED_V3_USDC_COLLATERAL}"
    state_set "no_backstop_eurc_supply" "${MODELED_FIXED_V3_EURC_SUPPLY}"
    state_set "no_backstop_eurc_borrow" "${MODELED_FIXED_V3_EURC_BORROW}"
    record_no_backstop_provenance
    cat <<EOF
${NETWORK_LABEL} No Backstop pool verified
  pool:       ${pool} (active; not reward-zone eligible)
  oracle:     $(state_value modeled_v3_fixed_oracle) (shared with Fixed V3)
  reserves:   XLM, USDC, EURC (Fixed V3 configuration)
  positions:  1,000 USDC supplied; 1,000 USDC collateral; 500 EURC supplied; 100 EURC borrowed
  backstop:   zero tiers; zero take rate; zero active value
EOF
}
