#!/usr/bin/env bash

# Copyright (C) 2026 BLNT Protocol contributors
# SPDX-License-Identifier: AGPL-3.0-only

# This file is sourced by v3-only-deployment.sh after the common deployment
# helpers and V3-only configuration have been loaded.

TRADITIONAL_POOL_SALT="${BLEND_TRADITIONAL_POOL_SALT:-00000000000000000000000000000000000000000000000000000000000000d1}"
TRADITIONAL_FIRST_LOSS_USDC=$((15000 * SCALAR_7))
TRADITIONAL_SECOND_LOSS_USDC=$((10000 * SCALAR_7))
TRADITIONAL_BACKSTOP_TOTAL=$((TRADITIONAL_FIRST_LOSS_USDC + TRADITIONAL_SECOND_LOSS_USDC))

require_traditional_pool_support() {
    require_modeled_pool_support
}

traditional_fixed_value() {
    jq -er ".pools.fixed$1" "${MODELED_POOLS_FIXTURE}"
}

traditional_reserve_config() {
    modeled_v2_reserve_config fixed "$1"
}

traditional_backstop_config() {
    printf '%s\n' '[{"asset":"Usdc","take_rate_weight":3},{"asset":"Usdc","take_rate_weight":2}]'
}

record_traditional_provenance() {
    local patch untracked_list untracked status commit dirty=false digest=""

    patch="${RUN_DIR}/traditional-source.patch"
    untracked_list="${RUN_DIR}/traditional-untracked-files"
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
    state_set "traditional_source_commit" "${commit}"
    state_set "traditional_source_dirty" "${dirty}"
    state_set "traditional_source_patch_sha256" "${digest}"
}

ensure_traditional_pool() {
    local oracle="$1" pool raw reserve_list code token config index=0 status

    pool="$(state_optional traditional_pool)"
    if [[ -z "${pool}" ]]; then
        raw="$(invoke_transaction "deploy-traditional-pool" \
            "$(state_value v3_factory)" deploy \
            --admin "$(state_value operator)" \
            --name "Traditional" \
            --salt "${TRADITIONAL_POOL_SALT}" \
            --oracle "${oracle}" \
            --backstop_take_rate "$(traditional_fixed_value '.backstop_take_rate')" \
            --max_positions "$(traditional_fixed_value '.max_positions')" \
            --min_collateral "$(traditional_fixed_value '.min_collateral')" \
            --backstop_config "$(traditional_backstop_config)" \
            --access_controller null)"
        pool="$(normalize_scalar "${raw}")"
        require_contract_id "${pool}" "Traditional pool"
        state_set "traditional_pool" "${pool}"
        state_set "traditional_pool_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fi
    require_contract_id "${pool}" "Traditional pool"
    state_set "traditional_pool_oracle" "${oracle}"

    reserve_list="$(invoke_view \
        "query-traditional-reserves" "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(modeled_asset_id "${code}")"
        if ! printf '%s' "${reserve_list}" | jq -e \
            --arg token "${token}" 'index($token) != null' >/dev/null
        then
            config="$(traditional_reserve_config "${index}")"
            if invoke_view "probe-traditional-queued-${code}" \
                "${pool}" set_reserve --asset "${token}" >/dev/null
            then
                invoke_transaction "resume-traditional-${code}" \
                    "${pool}" set_reserve --asset "${token}" >/dev/null
            else
                invoke_transaction "queue-traditional-${code}" \
                    "${pool}" queue_set_reserve \
                    --asset "${token}" --metadata "${config}" >/dev/null
                invoke_transaction "set-traditional-${code}" \
                    "${pool}" set_reserve --asset "${token}" >/dev/null
            fi
            reserve_list="$(invoke_view \
                "query-traditional-reserves-${code}" \
                "${pool}" get_reserve_list)"
            printf '%s' "${reserve_list}" | jq -e \
                --arg token "${token}" 'index($token) != null' >/dev/null ||
                die "Traditional reserve ${code} was not committed"
        fi
        index=$((index + 1))
    done < <(jq -r '.pools.fixed.reserves[].asset' "${MODELED_POOLS_FIXTURE}")

    status="$(invoke_view "query-traditional-status" "${pool}" get_config |
        jq -er '.status | tostring')"
    if [[ "${status}" == "6" ]]; then
        invoke_transaction "on-ice-traditional-pool" \
            "${pool}" set_status --pool_status 2 >/dev/null
    elif [[ "${status}" != "0" && "${status}" != "1" && "${status}" != "2" ]]; then
        die "Traditional pool has unexpected status ${status}"
    fi
    printf '%s\n' "${pool}"
}

traditional_user_shares() {
    invoke_view "query-traditional-$2-balance" \
        "$(state_value v3_backstop)" user_balance \
        --tier "$2" --pool "$1" --user "$(state_value depositor)" |
        jq -er '((.shares | tonumber) + ([.q4w[]?.amount | tonumber] | add // 0)) | tostring'
}

ensure_traditional_backstop_tier() {
    local pool="$1" tier="$2" target="$3" current delta label

    current="$(traditional_user_shares "${pool}" "${tier}")"
    [[ "${current}" =~ ^[0-9]+$ ]] ||
        die "invalid Traditional ${tier} shares: ${current}"
    (( current <= target )) ||
        die "Traditional ${tier} shares exceed target: ${current}"
    if (( current < target )); then
        delta=$((target - current))
        label="$(printf '%s' "${tier}" | tr '[:upper:]' '[:lower:]')"
        ensure_token_balance "$(state_value usdc_token)" \
            "${delta}" "traditional-${label}-usdc"
        invoke_transaction_as "deposit-traditional-${label}" \
            "${DEPOSITOR_IDENTITY}" "$(state_value v3_backstop)" deposit \
            --tier "${tier}" --from "$(state_value depositor)" \
            --pool-address "${pool}" --amount "${delta}" >/dev/null
    fi
}

ensure_traditional_pool_active() {
    local pool="$1" status

    status="$(invoke_view "query-traditional-pre-activation" \
        "${pool}" get_config | jq -er '.status | tostring')"
    if [[ "${status}" != "0" && "${status}" != "1" ]]; then
        invoke_transaction "activate-traditional-pool" \
            "${pool}" set_status --pool_status 0 >/dev/null
    fi
    status="$(invoke_view "query-traditional-post-activation" \
        "${pool}" get_config | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "Traditional pool is not active: status ${status}"
}

command_deploy_traditional_pool() {
    local oracle pool

    require_traditional_pool_support
    oracle="$(state_value modeled_v3_fixed_oracle)"
    pool="$(ensure_traditional_pool "${oracle}")"
    ensure_traditional_backstop_tier \
        "${pool}" FirstLoss "${TRADITIONAL_FIRST_LOSS_USDC}"
    ensure_traditional_backstop_tier \
        "${pool}" SecondLoss "${TRADITIONAL_SECOND_LOSS_USDC}"
    ensure_traditional_pool_active "${pool}"
    ensure_modeled_v3_supply traditional "${pool}"
    ensure_fixed_v3_eurc_borrow "${pool}" traditional
    command_verify_traditional_pool
}

command_verify_traditional_pool() {
    local pool factory backstop registered admin config status factory_config
    local expected_reserves="[]" actual_reserves code token expected_config reserve
    local actual_config index=0 positions expected_positions pool_data reward_zone
    local first_balance second_balance expected_tiers

    require_traditional_pool_support
    pool="$(state_value traditional_pool)"
    factory="$(state_value v3_factory)"
    backstop="$(state_value v3_backstop)"

    registered="$(normalize_scalar "$(invoke_view \
        "verify-traditional-registration" "${factory}" is_pool \
        --pool_address "${pool}")")"
    assert_equal "true" "${registered}" "Traditional factory registration"
    admin="$(normalize_scalar "$(invoke_view \
        "verify-traditional-admin" "${pool}" get_admin)")"
    assert_equal "$(state_value operator)" "${admin}" "Traditional pool admin"

    config="$(invoke_view "verify-traditional-config" "${pool}" get_config)"
    status="$(printf '%s' "${config}" | jq -er '.status | tostring')"
    [[ "${status}" == "0" || "${status}" == "1" ]] ||
        die "Traditional pool is not active: status ${status}"
    assert_equal "$(traditional_fixed_value '.backstop_take_rate')" \
        "$(printf '%s' "${config}" | jq -er '.bstop_rate | tostring')" \
        "Traditional take rate"
    assert_equal "$(traditional_fixed_value '.max_positions')" \
        "$(printf '%s' "${config}" | jq -er '.max_positions | tostring')" \
        "Traditional max positions"
    assert_equal "$(traditional_fixed_value '.min_collateral')" \
        "$(printf '%s' "${config}" | jq -er '.min_collateral | tostring')" \
        "Traditional minimum collateral"
    assert_equal "$(state_value modeled_v3_fixed_oracle)" \
        "$(printf '%s' "${config}" | jq -er '.oracle')" \
        "Traditional oracle"

    factory_config="$(invoke_view "verify-traditional-factory-config" \
        "${factory}" backstop_config --pool_address "${pool}")"
    assert_json_equal \
        "$(jq -cn --argjson tiers "$(traditional_backstop_config)" \
            '{access_controller:null,tiers:$tiers}')" \
        "${factory_config}" "Traditional access and tier configuration"

    actual_reserves="$(invoke_view "verify-traditional-reserve-list" \
        "${pool}" get_reserve_list)"
    while IFS= read -r code; do
        token="$(modeled_asset_id "${code}")"
        expected_reserves="$(printf '%s' "${expected_reserves}" | jq -c \
            --arg token "${token}" '. + [$token]')"
        expected_config="$(traditional_reserve_config "${index}")"
        reserve="$(invoke_view "verify-traditional-${code}-reserve" \
            "${pool}" get_reserve --asset "${token}")"
        actual_config="$(printf '%s' "${reserve}" | jq -c '.config')"
        assert_json_equal "${expected_config}" "${actual_config}" \
            "Traditional ${code} reserve configuration"
        index=$((index + 1))
    done < <(jq -r '.pools.fixed.reserves[].asset' "${MODELED_POOLS_FIXTURE}")
    assert_json_equal "${expected_reserves}" "${actual_reserves}" \
        "Traditional reserve list"

    positions="$(invoke_view "verify-traditional-positions" \
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
        "Traditional fixture positions"

    first_balance="$(traditional_user_shares "${pool}" FirstLoss)"
    second_balance="$(traditional_user_shares "${pool}" SecondLoss)"
    assert_equal "${TRADITIONAL_FIRST_LOSS_USDC}" "${first_balance}" \
        "Traditional FirstLoss USDC shares"
    assert_equal "${TRADITIONAL_SECOND_LOSS_USDC}" "${second_balance}" \
        "Traditional SecondLoss USDC shares"

    pool_data="$(invoke_view "verify-traditional-pool-data" \
        "${backstop}" pool_data --pool "${pool}")"
    assert_equal "${TRADITIONAL_BACKSTOP_TOTAL}" \
        "$(printf '%s' "${pool_data}" | jq -er '.active_value | tostring')" \
        "Traditional active value"
    assert_equal "0" \
        "$(printf '%s' "${pool_data}" | jq -er '.q4w_pct | tostring')" \
        "Traditional queued-withdrawal percentage"
    expected_tiers="$(jq -cn \
        --arg token "$(state_value usdc_token)" \
        --arg first "${TRADITIONAL_FIRST_LOSS_USDC}" \
        --arg second "${TRADITIONAL_SECOND_LOSS_USDC}" '[
          {
            asset:"Usdc", blnt_emission_eligible:false,
            take_rate_weight:3, token:$token,
            tokens:$first, shares:$first, value:$first
          },
          {
            asset:"Usdc", blnt_emission_eligible:false,
            take_rate_weight:2, token:$token,
            tokens:$second, shares:$second, value:$second
          }
        ]')"
    assert_json_equal "${expected_tiers}" \
        "$(printf '%s' "${pool_data}" | jq -c '.tiers')" \
        "Traditional tier accounting"

    reward_zone="$(invoke_view "verify-traditional-reward-zone" \
        "${backstop}" reward_zone)"
    if printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${pool}" 'index($pool) != null' >/dev/null
    then
        die "Traditional pool unexpectedly entered the reward zone"
    fi

    state_set "traditional_pool_verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    state_set "traditional_first_loss_usdc" "${TRADITIONAL_FIRST_LOSS_USDC}"
    state_set "traditional_second_loss_usdc" "${TRADITIONAL_SECOND_LOSS_USDC}"
    state_set "traditional_usdc_supply" "${POOL_SUPPLY_BALANCE}"
    state_set "traditional_usdc_collateral" "${MODELED_FIXED_V3_USDC_COLLATERAL}"
    state_set "traditional_eurc_supply" "${MODELED_FIXED_V3_EURC_SUPPLY}"
    state_set "traditional_eurc_borrow" "${MODELED_FIXED_V3_EURC_BORROW}"
    record_traditional_provenance
    cat <<EOF
${NETWORK_LABEL} Traditional pool verified
  pool:       ${pool} (active; not reward-zone eligible)
  oracle:     $(state_value modeled_v3_fixed_oracle) (shared with Fixed V3)
  reserves:   XLM, USDC, EURC (Fixed V3 configuration)
  positions:  1,000 USDC supplied; 1,000 USDC collateral; 500 EURC supplied; 100 EURC borrowed
  backstop:   15,000 USDC FirstLoss (weight 3); 10,000 USDC SecondLoss (weight 2)
EOF
}
