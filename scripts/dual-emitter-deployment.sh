#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export BLEND_MIGRATION_LIBRARY_ONLY=true
# Reuse the existing evidence, RPC, account, and transaction helpers without
# entering its legacy queued-migration command dispatcher.
# shellcheck source=localnet-migration.sh
source "${SCRIPT_DIR}/localnet-migration.sh"
unset BLEND_MIGRATION_LIBRARY_ONLY

STATE_FILE="${WORK_DIR}/dual-emitter-state.json"
V3_EMITTER_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/emitter.wasm"
BLNT_BACKFILL_WASM="${ROOT_DIR}/blnt-backfill-contract/target/wasm32v1-none/optimized/blnt_backfill_contract.wasm"
BLNT_MINT=$((2000 * SCALAR_7))
USDC_MINT=$((2 * COMET_INITIAL_PAIR_TOKEN))
CONVERSION_TEST_AMOUNT=$((10 * SCALAR_7))
BLNT_BACKFILL_DROP=$((150000000 * SCALAR_7))
BLNT_SWAP_CAP=$((51000000 * SCALAR_7))
BLND_SWAP_INPUT_CAP=$((2 * BLNT_SWAP_CAP))
BLNT_BACKFILL_ALLOCATION=$((74000000 * SCALAR_7))
BLNT_BACKFILL_VESTING_SECONDS=$((180 * 24 * 60 * 60))
BLNT_GRANT_CAP=$((25000000 * SCALAR_7))
BLNT_GRANT_VESTING_SECONDS=$((360 * 24 * 60 * 60))
BLNT_SWAP_DURATION_SECONDS=$((270 * 24 * 60 * 60))
BLNT_MAX_GRANTEES=100
BLNT_BACKFILL_CLAIMANTS=434
BLNT_BACKFILL_ACCOUNTS=423
BLNT_BACKFILL_CONTRACTS=11
BLNT_BACKFILL_SOURCE="${ROOT_DIR}/comet_cpal_flattened_ownership_before_41c898a1.csv"
BLNT_BACKFILL_MANIFEST="${ROOT_DIR}/blnt-backfill-contract/allocations/comet_cpal_backfill.json"
BLNT_BACKFILL_GENERATOR="${ROOT_DIR}/blnt-backfill-contract/scripts/generate-backfill-claims.mjs"
BLNT_BACKFILL_CLAIM_LIST=""
BLNT_BACKFILL_CLAIM_TOTAL=""
BLNT_GRANT_LIST="${BLEND_BLNT_GRANT_LIST:-[]}"
BLNT_GRANT_TOTAL=""
BLNT_CLAIM_TOTAL=""
BLNT_TOKEN_NAME="Blent Token"
BLNT_HOME_DOMAIN="${BLEND_BLNT_HOME_DOMAIN:-}"
DUAL_MODELED_V2_LP_TOTAL=$((2 * MODELED_BACKSTOP_LP_BALANCE))
DUAL_MODELED_V3_LP_BALANCE=$((10 * SCALAR_7))
DUAL_MODELED_BLND_MINT=$((440000 * SCALAR_7))
DUAL_MODELED_BLNT_MINT=$((200 * SCALAR_7))
DUAL_MODELED_USDC_MINT=$((50000 * SCALAR_7))
DUAL_MODELED_V2_EMISSION_CONFIG='[{"res_index":1,"res_type":1,"share":1}]'
DUAL_MODELED_V2_EMISSION_START_DELAY_SECONDS=15

validate_backfill_manifest() {
    local expected_source_hash actual_source_hash generated_manifest

    require_command cmp
    require_command node
    [[ -f "${BLNT_BACKFILL_SOURCE}" ]] ||
        die "backfill ownership snapshot is missing: ${BLNT_BACKFILL_SOURCE}"
    [[ -f "${BLNT_BACKFILL_MANIFEST}" ]] ||
        die "backfill allocation manifest is missing: ${BLNT_BACKFILL_MANIFEST}"
    [[ -f "${BLNT_BACKFILL_GENERATOR}" ]] ||
        die "backfill allocation generator is missing: ${BLNT_BACKFILL_GENERATOR}"

    expected_source_hash="$(jq -er '.source_sha256' "${BLNT_BACKFILL_MANIFEST}")"
    actual_source_hash="$(sha256_file "${BLNT_BACKFILL_SOURCE}")"
    assert_equal "${expected_source_hash}" "${actual_source_hash}" \
        "backfill ownership snapshot hash"

    jq -e \
        --arg allocation "${BLNT_BACKFILL_ALLOCATION}" \
        --argjson claimants "${BLNT_BACKFILL_CLAIMANTS}" \
        --argjson accounts "${BLNT_BACKFILL_ACCOUNTS}" \
        --argjson contracts "${BLNT_BACKFILL_CONTRACTS}" \
        '
            .schema_version == 1 and
            .weight_field == "flattened_total_cpal_raw" and
            .allocation_total_raw == $allocation and
            .claimant_count == $claimants and
            .claimant_type_counts.account == $accounts and
            .claimant_type_counts.contract == $contracts and
            (.allocations | length) == $claimants and
            ([.allocations[].owner_address] | unique | length) == $claimants and
            ([.allocations[].allocation_raw | test("^[1-9][0-9]*$")] | all) and
            (([.allocations[].allocation_raw | tonumber] | add | tostring) == $allocation)
        ' "${BLNT_BACKFILL_MANIFEST}" >/dev/null ||
        die "backfill allocation manifest failed invariant validation"

    generated_manifest="$(mktemp "${TMPDIR:-/tmp}/blnt-backfill-manifest.XXXXXX")"
    if ! node "${BLNT_BACKFILL_GENERATOR}" \
        "${BLNT_BACKFILL_SOURCE}" "${generated_manifest}"
    then
        rm -f "${generated_manifest}"
        die "failed to regenerate backfill allocation manifest"
    fi
    if ! cmp -s "${BLNT_BACKFILL_MANIFEST}" "${generated_manifest}"; then
        rm -f "${generated_manifest}"
        die "backfill allocation manifest does not match its source snapshot"
    fi
    rm -f "${generated_manifest}"
}

load_claim_lists() {
    BLNT_BACKFILL_CLAIM_LIST="$(jq -c \
        '[.allocations[] | [.owner_address, .allocation_raw]]' \
        "${BLNT_BACKFILL_MANIFEST}")"
    jq -e --argjson max "${BLNT_BACKFILL_CLAIMANTS}" \
        '
            type == "array" and (length as $count | $count <= $max) and
            ([.[] |
                type == "array" and length == 2 and
                (.[0] | type == "string" and test("^[CG][A-Z2-7]{55}$")) and
                (.[1] | type == "string" and test("^[1-9][0-9]*$"))
            ] | all) and
            (length as $count | ([.[][0]] | unique | length) == $count)
        ' <<<"${BLNT_BACKFILL_CLAIM_LIST}" >/dev/null ||
        die "invalid committed backfill claim list encoding"
    jq -e --argjson max "${BLNT_MAX_GRANTEES}" \
        '
            type == "array" and (length as $count | $count <= $max) and
            ([.[] |
                type == "array" and length == 2 and
                (.[0] | type == "string" and test("^[CG][A-Z2-7]{55}$")) and
                (.[1] | type == "string" and test("^[1-9][0-9]*$"))
            ] | all) and
            (length as $count | ([.[][0]] | unique | length) == $count)
        ' <<<"${BLNT_GRANT_LIST}" >/dev/null ||
        die "invalid BLEND_BLNT_GRANT_LIST encoding"
    BLNT_BACKFILL_CLAIM_TOTAL="$(jq -er \
        '[.[][1] | tonumber] | add // 0 | tostring' \
        <<<"${BLNT_BACKFILL_CLAIM_LIST}")" ||
        die "invalid committed backfill claim list encoding"
    BLNT_GRANT_TOTAL="$(jq -er \
        '[.[][1] | tonumber] | add // 0 | tostring' \
        <<<"${BLNT_GRANT_LIST}")" ||
        die "invalid BLEND_BLNT_GRANT_LIST encoding"
    assert_equal "${BLNT_BACKFILL_ALLOCATION}" "${BLNT_BACKFILL_CLAIM_TOTAL}" \
        "74-million-BLNT backfill allocation"
    assert_equal "${BLNT_GRANT_CAP}" "${BLNT_GRANT_TOTAL}" \
        "25-million-BLNT grant allocation"
    BLNT_CLAIM_TOTAL=$((BLNT_BACKFILL_CLAIM_TOTAL + BLNT_GRANT_TOTAL))
}

usage_dual() {
    cat <<'EOF'
Usage: scripts/dual-emitter-deployment.sh COMMAND

Commands:
  plan      Describe the independent BLND/BLNT deployment.
  validate  Verify legacy artifacts and build v3 plus Comet v2 artifacts.
  start     Start local Protocol-27 Quickstart or verify public testnet RPC.
  deploy    Deploy and verify the independent legacy and v3 lanes.
  run       Validate, start, and deploy.
  status    Query the recorded token, emitter, backstop, and backfill bindings.
  stop      Stop the local Quickstart container.

Set BLEND_MIGRATION_NETWORK=testnet for the public-testnet lane. Signing keys
remain in the network-specific ignored Stellar config directory; state and
transaction evidence are recorded below the matching work directory.
EOF
}

dual_plan() {
    cat <<EOF
Dual-emitter ${NETWORK_LABEL} deployment:
  1. Verify the exact deployed legacy emitter and v2 artifacts.
  2. Build the v3 contracts, BLNT backfill contract, and workspace Comet v2.
  3. Deploy legacy BLND, BLNT, USDC, and ${XLM_ASSET_MODE} XLM assets.
  4. Deploy BLND:USDC, BLNT:USDC, and BLNT:XLM Comet v2 pools.
  5. Lock the shared Comet v2 controller at 100/100/100 with signer weight zero.
  6. Leave the legacy emitter pointed at the deployed v2 backstop.
  7. Point the separate v3 emitter at the v3 backstop with BLNT:XLM designated.
  8. Send the exact 150-million-BLNT initial drop to the immutable backfill
     contract, bind the 180-day backfill and 360-day grant schedules, and
     smoke-test its same-user 2-BLND-to-1-BLNT conversion and immediate burn.
  9. Verify separate administrators, recipients, emissions, Comet v2 pools,
     backfill custody, and the immutable 51-million-BLNT conversion cap and
     270-day expiration.

BLNT Stellar TOML metadata name: ${BLNT_TOKEN_NAME}

No legacy-emitter queue or migration backfill is used.
EOF
}

dual_validate_artifacts() {
    local artifact

    resolve_emitter_wasm
    for artifact in \
        "${V2_FACTORY_WASM}" \
        "${V2_BACKSTOP_WASM}" \
        "${V2_POOL_WASM}" \
        "${V3_FACTORY_WASM}" \
        "${V3_BACKSTOP_WASM}" \
        "${V3_EMITTER_WASM}" \
        "${V3_POOL_WASM}" \
        "${V3_TEST_SEP40_ORACLE_WASM}" \
        "${BLNT_BACKFILL_WASM}" \
        "${COMET_WASM}"
    do
        [[ -s "${artifact}" ]] || die "missing or empty WASM artifact: ${artifact}"
    done
}

dual_validate() {
    validate_tools
    validate_source_provenance
    validate_backfill_manifest
    build_artifacts
    dual_validate_artifacts
    note "Validated exact legacy artifacts, v3 contracts, and workspace Comet v2."
}

create_trustline() {
    local label="$1" identity="$2" asset="$3"

    capture "${label}" \
        stellar_cli tx new change-trust \
        --source-account "${identity}" \
        --line "${asset}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
}

initialize_comet() {
    local label="$1" comet="$2" controller="$3" first="$4" second="$5"
    local first_balance="$6" second_balance="$7"

    invoke_transaction_as "initialize-${label}" \
        "${COMET_CONTROLLER_IDENTITY}" "${comet}" init \
        --controller "${controller}" \
        --tokens "[\"${first}\",\"${second}\"]" \
        --weights '["8000000","2000000"]' \
        --balances "[\"${first_balance}\",\"${second_balance}\"]" \
        --swap_fee 30000 >/dev/null
}

dual_deploy_modeled_pools() {
    local fixed_v2_oracle custom_v2_oracle fixed_v3_oracle custom_v3_oracle
    local fixed_v2_pool custom_v2_pool fixed_v3_pool custom_v3_pool
    local pool status v2_reward_zone v3_reward_zone

    # Reuse the common modeled-pool builders with the independent-lane state
    # names recorded by this runner.
    state_set "v2_pool_factory" "$(state_value v2_factory)"
    state_set "v3_pool_factory" "$(state_value v3_factory)"
    state_set "incumbent_backstop" "$(state_value v2_backstop)"
    state_set "candidate_backstop" "$(state_value v3_backstop)"

    ensure_modeled_asset EURC
    ensure_modeled_asset PYUSD
    fixed_v2_oracle="$(ensure_modeled_v2_oracle fixed)"
    custom_v2_oracle="$(ensure_modeled_v2_oracle custom)"
    fixed_v3_oracle="$(ensure_modeled_v3_oracle fixed)"
    custom_v3_oracle="$(ensure_modeled_v3_oracle custom)"
    state_set "modeled_oracles_refreshed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    fixed_v2_pool="$(ensure_modeled_v2_pool fixed "${fixed_v2_oracle}")"
    custom_v2_pool="$(ensure_modeled_v2_pool custom "${custom_v2_oracle}")"
    fixed_v3_pool="$(ensure_modeled_v3_pool fixed "${fixed_v3_oracle}")"
    custom_v3_pool="$(ensure_modeled_v3_pool custom "${custom_v3_oracle}")"

    ensure_modeled_v2_backstop_deposit fixed "${fixed_v2_pool}"
    ensure_modeled_v2_backstop_deposit custom "${custom_v2_pool}"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" FirstLoss "${DUAL_MODELED_V3_LP_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" SecondLoss "${DUAL_MODELED_V3_LP_BALANCE}"
    ensure_token_balance "$(state_value usdc_token)" \
        "$((2 * MODELED_V3_USDC_BALANCE))" "dual-modeled-usdc"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_v3_pool}" ThirdLoss "${MODELED_V3_USDC_BALANCE}"
    ensure_modeled_xlm_balance "${MODELED_CUSTOM_V3_XLM_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        custom "${custom_v3_pool}" FirstLoss "${MODELED_CUSTOM_V3_XLM_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        custom "${custom_v3_pool}" SecondLoss "${MODELED_CUSTOM_V3_USDC_BALANCE}"

    for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
        invoke_transaction "activate-dual-modeled-v2" \
            "${pool}" set_status --pool_status 0 >/dev/null
        status="$(invoke_view "query-dual-modeled-v2-status" \
            "${pool}" get_config | jq -er '.status | tostring')"
        assert_equal "0" "${status}" "modeled v2 active status"
    done
    for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
        invoke_transaction "configure-dual-modeled-v2-emissions" \
            "${pool}" set_emissions_config \
            --res_emission_metadata "${DUAL_MODELED_V2_EMISSION_CONFIG}" >/dev/null
    done
    state_set "modeled_v2_emissions_configured_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for pool in "${fixed_v3_pool}" "${custom_v3_pool}"; do
        invoke_transaction "activate-dual-modeled-v3" \
            "${pool}" set_status --pool_status 0 >/dev/null
        status="$(invoke_view "query-dual-modeled-v3-status" \
            "${pool}" get_config | jq -er '.status | tostring')"
        assert_equal "0" "${status}" "modeled v3 active status"
    done

    v2_reward_zone="$(invoke_view "query-dual-v2-reward-zone" \
        "$(state_value v2_backstop)" reward_zone)"
    if ! printf '%s' "${v2_reward_zone}" | jq -e \
        --arg pool "${fixed_v2_pool}" 'index($pool) != null' >/dev/null
    then
        invoke_transaction "add-dual-modeled-v2-fixed-reward" \
            "$(state_value v2_backstop)" add_reward \
            --to_add "${fixed_v2_pool}" --to_remove null >/dev/null
        v2_reward_zone="$(invoke_view "query-updated-dual-v2-reward-zone" \
            "$(state_value v2_backstop)" reward_zone)"
    fi
    if ! printf '%s' "${v2_reward_zone}" | jq -e \
        --arg pool "${custom_v2_pool}" 'index($pool) != null' >/dev/null
    then
        # Once V2's reward zone is nonempty, its anti-reordering guard requires
        # a backstop distribution based on an emitter timestamp from the last
        # hour before another pool can be added.
        invoke_transaction "advance-dual-v2-emitter-for-reward" \
            "$(state_value legacy_emitter)" distribute >/dev/null
        invoke_transaction "prime-dual-v2-reward-distribution" \
            "$(state_value v2_backstop)" distribute >/dev/null
        invoke_transaction "add-dual-modeled-v2-custom-reward" \
            "$(state_value v2_backstop)" add_reward \
            --to_add "${custom_v2_pool}" --to_remove null >/dev/null
        v2_reward_zone="$(invoke_view "query-final-dual-v2-reward-zone" \
            "$(state_value v2_backstop)" reward_zone)"
    fi
    for pool in "${fixed_v2_pool}" "${custom_v2_pool}"; do
        printf '%s' "${v2_reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
            die "modeled v2 pool missing from reward zone: ${pool}"
    done

    v3_reward_zone="$(invoke_view "query-dual-v3-reward-zone" \
        "$(state_value v3_backstop)" reward_zone)"
    for pool in "${fixed_v3_pool}" "${custom_v3_pool}"; do
        if ! printf '%s' "${v3_reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null
        then
            invoke_transaction "add-dual-modeled-v3-reward" \
                "$(state_value v3_backstop)" add_reward \
                --to_add "${pool}" --to_remove null >/dev/null
            v3_reward_zone="$(invoke_view "query-updated-dual-v3-reward-zone" \
                "$(state_value v3_backstop)" reward_zone)"
        fi
    done
    for pool in "${fixed_v3_pool}" "${custom_v3_pool}"; do
        printf '%s' "${v3_reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
            die "modeled v3 pool missing from reward zone: ${pool}"
    done

    ensure_modeled_v3_emissions_config fixed "${fixed_v3_pool}"
    ensure_modeled_v3_supply fixed "${fixed_v3_pool}"
    ensure_modeled_v3_supply custom "${custom_v3_pool}"
    ensure_fixed_v3_eurc_borrow "${fixed_v3_pool}"
    ensure_modeled_v2_supply fixed "${fixed_v2_pool}"
    ensure_modeled_v2_supply custom "${custom_v2_pool}"

    state_set "modeled_pools_fixture_sha256" \
        "$(sha256_file "${MODELED_POOLS_FIXTURE}")"
    state_set "modeled_v3_lending_oracle_wasm_sha256" \
        "$(sha256_file "${V3_TEST_SEP40_ORACLE_WASM}")"
    state_set "modeled_pools_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Deployed and activated modeled Fixed and Custom v2/v3 pools."
}

dual_deploy() {
    local skip_build="${1:-false}"
    local operator depositor controller issuer
    local legacy_emitter v3_emitter blnt_backfill legacy_blnd blnt usdc xlm
    local legacy_blnd_usdc blnt_usdc blnt_xlm xlm_balance
    local v2_pool_hash v2_backstop v2_factory deployed_v2_backstop
    local v3_pool_hash v3_backstop v3_factory deployed_v3_backstop
    local legacy_emitted legacy_distributed v3_emitted swapped blnd_burned
    local remaining_capacity
    local backfill_balance allocated pool pool_gulped stream now
    local vesting_start vesting_end grant_vesting_start grant_vesting_end
    local swap_deadline expired_blnt_burned
    local backfill_allocated grant_allocated
    local total_claimed backfill_claimed grant_claimed
    local post_backfill_balance holder_blnd_balance holder_blnt_balance backfill_blnd_balance
    local legacy_admin blnt_admin legacy_recipient v3_recipient

    validate_tools
    validate_source_provenance
    validate_backfill_manifest
    require_network
    [[ ! -e "${STATE_FILE}" ]] ||
        die "dual-emitter state already exists at ${STATE_FILE}; refusing to overwrite it"
    if [[ "${skip_build}" != "true" ]]; then
        build_artifacts
    fi
    dual_validate_artifacts
    new_run_dir

    stellar_cli keys generate "${OPERATOR_IDENTITY}" --overwrite >/dev/null
    operator="$(stellar_cli keys public-key "${OPERATOR_IDENTITY}")"
    fund_and_wait_for_account "${operator}"
    issuer="${operator}"
    if [[ "${BLNT_GRANT_LIST}" == "[]" ]]; then
        BLNT_GRANT_LIST="[[\"${operator}\",\"${BLNT_GRANT_CAP}\"]]"
    fi
    load_claim_lists
    initialize_state "${operator}"
    state_set "topology" "dual-emitter-blnd-blnt"
    state_set "blnt_token_name" "${BLNT_TOKEN_NAME}"
    if [[ -n "${BLNT_HOME_DOMAIN}" ]]; then
        [[ "${BLNT_HOME_DOMAIN}" =~ ^[A-Za-z0-9.-]+$ ]] ||
            die "invalid BLEND_BLNT_HOME_DOMAIN: ${BLNT_HOME_DOMAIN}"
        capture "set-blnt-issuer-home-domain" \
            stellar_cli tx new set-options \
            --source-account "${OPERATOR_IDENTITY}" \
            --home-domain "${BLNT_HOME_DOMAIN}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
        state_set "blnt_home_domain" "${BLNT_HOME_DOMAIN}"
    fi
    state_set "legacy_emitter_wasm_sha256" "${EXPECTED_EMITTER_SHA256}"
    state_set "v3_emitter_wasm_sha256" "$(sha256_file "${V3_EMITTER_WASM}")"
    state_set "v3_backstop_wasm_sha256" "$(sha256_file "${V3_BACKSTOP_WASM}")"
    state_set "blnt_backfill_wasm_sha256" "$(sha256_file "${BLNT_BACKFILL_WASM}")"
    state_set "blnt_backfill_source_commit" \
        "$(git -C "${ROOT_DIR}/blnt-backfill-contract" rev-parse HEAD)"
    state_set "comet_v2_wasm_sha256" "$(sha256_file "${COMET_WASM}")"
    state_set "comet_v2_source_commit" \
        "$(git -C "${ROOT_DIR}/comet-contracts-v2" rev-parse HEAD)"

    stellar_cli keys generate "${DEPOSITOR_IDENTITY}" --overwrite >/dev/null
    depositor="$(stellar_cli keys public-key "${DEPOSITOR_IDENTITY}")"
    fund_and_wait_for_account "${depositor}"
    state_set "depositor" "${depositor}"

    stellar_cli keys generate "${COMET_CONTROLLER_IDENTITY}" --overwrite >/dev/null
    controller="$(stellar_cli keys public-key "${COMET_CONTROLLER_IDENTITY}")"
    fund_and_wait_for_account "${controller}"
    state_set "comet_controller" "${controller}"

    legacy_blnd="$(deploy_asset "deploy-legacy-blnd" "BLND" "${issuer}")"
    blnt="$(deploy_asset "deploy-v3-blnt" "BLNT" "${issuer}")"
    usdc="$(deploy_asset "deploy-usdc" "USDC" "${issuer}")"
    if [[ "${XLM_ASSET_MODE}" == "native" ]]; then
        xlm="$(resolve_native_asset "resolve-native-xlm")"
        xlm_balance="${COMET_INITIAL_XLM}"
    else
        xlm="$(deploy_asset "deploy-issued-xlm" "XLM" "${issuer}")"
        xlm_balance="${COMET_INITIAL_PAIR_TOKEN}"
    fi
    state_set "legacy_blnd_token" "${legacy_blnd}"
    state_set "blnt_token" "${blnt}"
    state_set "usdc_token" "${usdc}"
    state_set "xlm_token" "${xlm}"

    create_trustline "trust-depositor-blnd" "${DEPOSITOR_IDENTITY}" "BLND:${issuer}"
    create_trustline "trust-depositor-blnt" "${DEPOSITOR_IDENTITY}" "BLNT:${issuer}"
    create_trustline "trust-depositor-usdc" "${DEPOSITOR_IDENTITY}" "USDC:${issuer}"
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        create_trustline "trust-depositor-xlm" \
            "${DEPOSITOR_IDENTITY}" "XLM:${issuer}"
    fi
    create_trustline "trust-controller-blnd" "${COMET_CONTROLLER_IDENTITY}" "BLND:${issuer}"
    create_trustline "trust-controller-blnt" "${COMET_CONTROLLER_IDENTITY}" "BLNT:${issuer}"
    create_trustline "trust-controller-usdc" "${COMET_CONTROLLER_IDENTITY}" "USDC:${issuer}"
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        create_trustline "trust-controller-xlm" "${COMET_CONTROLLER_IDENTITY}" "XLM:${issuer}"
    fi

    legacy_emitter="$(deploy_wasm "deploy-legacy-emitter" "${EMITTER_WASM}" "")"
    v3_emitter="$(deploy_wasm \
        "deploy-v3-emitter" "${V3_EMITTER_WASM}" "" \
        --initializer "${operator}")"
    blnt_backfill="$(deploy_wasm \
        "deploy-blnt-backfill" "${BLNT_BACKFILL_WASM}" "" \
        --legacy_blnd_token "${legacy_blnd}" \
        --blnt_token "${blnt}" \
        --claim_list "${BLNT_BACKFILL_CLAIM_LIST}" \
        --grant_list "${BLNT_GRANT_LIST}")"
    state_set "legacy_emitter" "${legacy_emitter}"
    state_set "v3_emitter" "${v3_emitter}"
    state_set "blnt_backfill" "${blnt_backfill}"
    state_set "blnt_backfill_claim_list" "${BLNT_BACKFILL_CLAIM_LIST}"
    state_set "blnt_backfill_claim_total" "${BLNT_BACKFILL_CLAIM_TOTAL}"
    state_set "blnt_grant_list" "${BLNT_GRANT_LIST}"
    state_set "blnt_grant_total" "${BLNT_GRANT_TOTAL}"
    state_set "blnt_backfill_manifest_sha256" "$(sha256_file "${BLNT_BACKFILL_MANIFEST}")"

    legacy_blnd_usdc="$(deploy_wasm "deploy-comet-blnd-usdc" "${COMET_WASM}" "")"
    blnt_usdc="$(deploy_wasm "deploy-comet-blnt-usdc" "${COMET_WASM}" "")"
    blnt_xlm="$(deploy_wasm "deploy-comet-blnt-xlm" "${COMET_WASM}" "")"
    state_set "legacy_blnd_usdc_token" "${legacy_blnd_usdc}"
    state_set "blnt_usdc_token" "${blnt_usdc}"
    state_set "blnt_xlm_token" "${blnt_xlm}"

    invoke_transaction "mint-controller-blnd" "${legacy_blnd}" mint \
        --to "${controller}" --amount "${COMET_INITIAL_BLND}" >/dev/null
    invoke_transaction "mint-depositor-blnd" "${legacy_blnd}" mint \
        --to "${depositor}" \
        --amount "$((DUAL_MODELED_BLND_MINT + 2 * CONVERSION_TEST_AMOUNT))" >/dev/null
    invoke_transaction "mint-depositor-blnt" "${blnt}" mint \
        --to "${depositor}" --amount "${DUAL_MODELED_BLNT_MINT}" >/dev/null
    invoke_transaction "mint-depositor-usdc" "${usdc}" mint \
        --to "${depositor}" --amount "${DUAL_MODELED_USDC_MINT}" >/dev/null
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        invoke_transaction "mint-depositor-xlm" "${xlm}" mint \
            --to "${depositor}" \
            --amount "$((1100 * SCALAR_7))" >/dev/null
    fi
    invoke_transaction "mint-controller-blnt" "${blnt}" mint \
        --to "${controller}" --amount "${BLNT_MINT}" >/dev/null
    invoke_transaction "mint-controller-usdc" "${usdc}" mint \
        --to "${controller}" --amount "${USDC_MINT}" >/dev/null
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        invoke_transaction "mint-controller-xlm" "${xlm}" mint \
            --to "${controller}" --amount "${xlm_balance}" >/dev/null
    fi

    initialize_comet "comet-blnd-usdc" "${legacy_blnd_usdc}" "${controller}" \
        "${legacy_blnd}" "${usdc}" "${COMET_INITIAL_BLND}" "${COMET_INITIAL_PAIR_TOKEN}"
    initialize_comet "comet-blnt-usdc" "${blnt_usdc}" "${controller}" \
        "${blnt}" "${usdc}" "${COMET_INITIAL_BLND}" "${COMET_INITIAL_PAIR_TOKEN}"
    initialize_comet "comet-blnt-xlm" "${blnt_xlm}" "${controller}" \
        "${blnt}" "${xlm}" "${COMET_INITIAL_BLND}" "${xlm_balance}"

    invoke_transaction_as "join-modeled-v2-blnd-usdc" \
        "${DEPOSITOR_IDENTITY}" "${legacy_blnd_usdc}" join_pool \
        --pool_amount_out "${DUAL_MODELED_V2_LP_TOTAL}" \
        --user "${depositor}" \
        --max_amounts_in \
        "[\"${DUAL_MODELED_BLND_MINT}\",\"${DUAL_MODELED_USDC_MINT}\"]" >/dev/null
    invoke_transaction_as "join-modeled-v3-blnt-usdc" \
        "${DEPOSITOR_IDENTITY}" "${blnt_usdc}" join_pool \
        --pool_amount_out "${DUAL_MODELED_V3_LP_BALANCE}" \
        --user "${depositor}" \
        --max_amounts_in \
        "[\"${DUAL_MODELED_BLNT_MINT}\",\"${DUAL_MODELED_USDC_MINT}\"]" >/dev/null
    invoke_transaction_as "join-modeled-v3-blnt-xlm" \
        "${DEPOSITOR_IDENTITY}" "${blnt_xlm}" join_pool \
        --pool_amount_out "${DUAL_MODELED_V3_LP_BALANCE}" \
        --user "${depositor}" \
        --max_amounts_in \
        "[\"${DUAL_MODELED_BLNT_MINT}\",\"${COMET_INITIAL_XLM}\"]" >/dev/null

    capture "lock-comet-controller" \
        stellar_cli tx new set-options \
        --source-account "${COMET_CONTROLLER_IDENTITY}" \
        --low-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --med-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --high-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --master-weight 0 \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    verify_locked_comet_controller "legacy-blnd-usdc" "${legacy_blnd_usdc}" \
        "${controller}" "${legacy_blnd}" "${usdc}"
    verify_locked_comet_controller "v3-blnt-usdc" "${blnt_usdc}" \
        "${controller}" "${blnt}" "${usdc}"
    verify_locked_comet_controller "v3-blnt-xlm" "${blnt_xlm}" \
        "${controller}" "${blnt}" "${xlm}"

    verify_non_clawbackable_sac_balance \
        "legacy-comet-blnd-custody" "${legacy_blnd}" "${legacy_blnd_usdc}"
    verify_non_clawbackable_sac_balance \
        "legacy-comet-usdc-custody" "${usdc}" "${legacy_blnd_usdc}"
    verify_non_clawbackable_sac_balance \
        "v3-comet-blnt-usdc-blnt-custody" "${blnt}" "${blnt_usdc}"
    verify_non_clawbackable_sac_balance \
        "v3-comet-blnt-usdc-usdc-custody" "${usdc}" "${blnt_usdc}"
    verify_non_clawbackable_sac_balance \
        "v3-comet-blnt-xlm-blnt-custody" "${blnt}" "${blnt_xlm}"
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        verify_non_clawbackable_sac_balance \
            "v3-comet-blnt-xlm-xlm-custody" "${xlm}" "${blnt_xlm}"
    fi

    invoke_transaction "set-legacy-emitter-as-blnd-admin" "${legacy_blnd}" set_admin \
        --new_admin "${legacy_emitter}" >/dev/null
    invoke_transaction "set-v3-emitter-as-blnt-admin" "${blnt}" set_admin \
        --new_admin "${v3_emitter}" >/dev/null

    v2_pool_hash="$(upload_wasm "upload-v2-pool" "${V2_POOL_WASM}")"
    v2_backstop="$(predict_contract_id "${INCUMBENT_SALT}")"
    state_set "v2_backstop" "${v2_backstop}"
    v2_factory="$(deploy_wasm \
        "deploy-v2-factory" "${V2_FACTORY_WASM}" "" \
        --pool_init_meta \
        "{\"backstop\":\"${v2_backstop}\",\"blnd_id\":\"${legacy_blnd}\",\"pool_hash\":\"${v2_pool_hash}\"}")"
    deployed_v2_backstop="$(deploy_wasm \
        "deploy-v2-backstop" "${V2_BACKSTOP_WASM}" "${INCUMBENT_SALT}" \
        --backstop_token "${legacy_blnd_usdc}" \
        --emitter "${legacy_emitter}" \
        --blnd_token "${legacy_blnd}" \
        --usdc_token "${usdc}" \
        --pool_factory "${v2_factory}" \
        --drop_list "[]")"
    assert_equal "${v2_backstop}" "${deployed_v2_backstop}" "v2 backstop address"
    state_set "v2_factory" "${v2_factory}"
    invoke_transaction "initialize-legacy-emitter" "${legacy_emitter}" initialize \
        --blnd_token "${legacy_blnd}" \
        --backstop "${v2_backstop}" \
        --backstop_token "${legacy_blnd_usdc}" >/dev/null

    v3_pool_hash="$(upload_wasm "upload-v3-pool" "${V3_POOL_WASM}")"
    v3_backstop="$(predict_contract_id "${CANDIDATE_SALT}")"
    state_set "v3_backstop" "${v3_backstop}"
    v3_factory="$(deploy_wasm \
        "deploy-v3-factory" "${V3_FACTORY_WASM}" "" \
        --pool_init_meta \
        "{\"backstop\":\"${v3_backstop}\",\"blnt_id\":\"${blnt}\",\"pool_hash\":\"${v3_pool_hash}\"}")"
    state_set "v3_factory" "${v3_factory}"
    invoke_transaction "initialize-v3-emitter" "${v3_emitter}" initialize \
        --blnd_token "${blnt}" \
        --backstop "${v3_backstop}" \
        --backstop_token "${blnt_xlm}" >/dev/null
    deployed_v3_backstop="$(deploy_wasm \
        "deploy-v3-backstop" "${V3_BACKSTOP_WASM}" "${CANDIDATE_SALT}" \
        --blnt_usdc_token "${blnt_usdc}" \
        --blnt_xlm_token "${blnt_xlm}" \
        --emitter "${v3_emitter}" \
        --blnt_token "${blnt}" \
        --usdc_token "${usdc}" \
        --xlm_token "${xlm}" \
        --pool_factory "${v3_factory}" \
        --drop_list "[[\"${blnt_backfill}\",\"${BLNT_BACKFILL_DROP}\"]]")"
    assert_equal "${v3_backstop}" "${deployed_v3_backstop}" "v3 backstop address"

    invoke_transaction "activate-v3-directly" "${v3_backstop}" distribute >/dev/null
    invoke_transaction "execute-v3-backfill-drop" "${v3_backstop}" drop >/dev/null

    backfill_balance="$(normalize_scalar "$(invoke_view \
        "query-backfill-blnt-balance" "${blnt}" balance --id "${blnt_backfill}")")"
    assert_equal "${BLNT_BACKFILL_DROP}" "${backfill_balance}" \
        "backfill BLNT custody after initial drop"

    invoke_transaction_as "convert-test-blnd-to-blnt" \
        "${DEPOSITOR_IDENTITY}" "${blnt_backfill}" swap_blnd_for_blnt \
        --user "${depositor}" \
        --blnt_amount "${CONVERSION_TEST_AMOUNT}" >/dev/null
    swapped="$(normalize_scalar "$(invoke_view \
        "query-total-converted" "${blnt_backfill}" get_total_swapped)")"
    assert_equal "${CONVERSION_TEST_AMOUNT}" "${swapped}" "BLNT conversion total"
    blnd_burned="$(normalize_scalar "$(invoke_view \
        "query-total-blnd-burned" "${blnt_backfill}" get_total_blnd_burned)")"
    assert_equal "$((2 * CONVERSION_TEST_AMOUNT))" \
        "${blnd_burned}" "converted BLND burned"
    remaining_capacity="$(normalize_scalar "$(invoke_view \
        "query-conversion-capacity" "${blnt_backfill}" get_remaining_swap_capacity)")"
    assert_equal "$((BLNT_SWAP_CAP - swapped))" \
        "${remaining_capacity}" "remaining conversion capacity"
    assert_equal "${legacy_blnd}" "$(normalize_scalar "$(invoke_view \
        "query-backfill-blnd-binding" "${blnt_backfill}" get_legacy_blnd_token)")" \
        "backfill legacy token"
    assert_equal "${blnt}" "$(normalize_scalar "$(invoke_view \
        "query-backfill-blnt-binding" "${blnt_backfill}" get_blnt_token)")" \
        "backfill payout token"
    allocated="$(normalize_scalar "$(invoke_view \
        "query-backfill-total-allocated" "${blnt_backfill}" get_total_allocated)")"
    assert_equal "${BLNT_CLAIM_TOTAL}" "${allocated}" \
        "combined immutable claim total"
    backfill_allocated="$(normalize_scalar "$(invoke_view \
        "query-backfill-allocated" "${blnt_backfill}" get_backfill_allocated)")"
    grant_allocated="$(normalize_scalar "$(invoke_view \
        "query-grant-allocated" "${blnt_backfill}" get_grant_allocated)")"
    assert_equal "${BLNT_BACKFILL_CLAIM_TOTAL}" "${backfill_allocated}" \
        "backfill immutable allocation"
    assert_equal "${BLNT_GRANT_TOTAL}" "${grant_allocated}" \
        "grant immutable allocation"
    vesting_start="$(normalize_scalar "$(invoke_view \
        "query-backfill-vesting-start" "${blnt_backfill}" get_vesting_start)")"
    vesting_end="$(normalize_scalar "$(invoke_view \
        "query-backfill-vesting-end" "${blnt_backfill}" get_vesting_end)")"
    assert_equal "${BLNT_BACKFILL_VESTING_SECONDS}" \
        "$((vesting_end - vesting_start))" "backfill vesting duration"
    state_set "blnt_backfill_vesting_start" "${vesting_start}"
    state_set "blnt_backfill_vesting_end" "${vesting_end}"
    grant_vesting_start="$(normalize_scalar "$(invoke_view \
        "query-grant-vesting-start" "${blnt_backfill}" get_grant_vesting_start)")"
    grant_vesting_end="$(normalize_scalar "$(invoke_view \
        "query-grant-vesting-end" "${blnt_backfill}" get_grant_vesting_end)")"
    assert_equal "${vesting_start}" "${grant_vesting_start}" \
        "shared claim vesting start"
    assert_equal "${BLNT_GRANT_VESTING_SECONDS}" \
        "$((grant_vesting_end - grant_vesting_start))" "grant vesting duration"
    state_set "blnt_grant_vesting_start" "${grant_vesting_start}"
    state_set "blnt_grant_vesting_end" "${grant_vesting_end}"
    swap_deadline="$(normalize_scalar "$(invoke_view \
        "query-swap-deadline" "${blnt_backfill}" get_swap_deadline)")"
    assert_equal "${BLNT_SWAP_DURATION_SECONDS}" \
        "$((swap_deadline - vesting_start))" "conversion duration"
    expired_blnt_burned="$(normalize_scalar "$(invoke_view \
        "query-expired-blnt-burned" "${blnt_backfill}" get_expired_blnt_burned)")"
    assert_equal "0" "${expired_blnt_burned}" "initial expired BLNT burn"
    state_set "blnt_swap_deadline" "${swap_deadline}"
    state_set "blnt_expired_blnt_burned" "${expired_blnt_burned}"
    total_claimed="$(normalize_scalar "$(invoke_view \
        "query-total-claimed" "${blnt_backfill}" get_total_claimed)")"
    backfill_claimed="$(normalize_scalar "$(invoke_view \
        "query-backfill-claimed" "${blnt_backfill}" get_backfill_claimed)")"
    grant_claimed="$(normalize_scalar "$(invoke_view \
        "query-grant-claimed" "${blnt_backfill}" get_grant_claimed)")"
    assert_equal "0" "${total_claimed}" "initial combined claimed total"
    assert_equal "0" "${backfill_claimed}" "initial backfill claimed total"
    assert_equal "0" "${grant_claimed}" "initial grant claimed total"
    holder_blnd_balance="$(normalize_scalar "$(invoke_view \
        "query-converted-holder-blnd" "${legacy_blnd}" balance --id "${depositor}")")"
    holder_blnt_balance="$(normalize_scalar "$(invoke_view \
        "query-converted-holder-blnt" "${blnt}" balance --id "${depositor}")")"
    backfill_blnd_balance="$(normalize_scalar "$(invoke_view \
        "query-backfill-blnd-balance" "${legacy_blnd}" balance --id "${blnt_backfill}")")"
    post_backfill_balance="$(normalize_scalar "$(invoke_view \
        "query-post-conversion-backfill-blnt" "${blnt}" balance \
        --id "${blnt_backfill}")")"
    assert_equal "0" "${holder_blnd_balance}" \
        "converted holder BLND balance after burn"
    assert_equal "${swapped}" "${holder_blnt_balance}" \
        "converted holder BLNT balance"
    assert_equal "0" "${backfill_blnd_balance}" \
        "backfill BLND balance after immediate burn"
    assert_equal "$((BLNT_BACKFILL_DROP - swapped))" \
        "${post_backfill_balance}" "backfill BLNT balance after conversion"

    dual_deploy_modeled_pools

    # V2 sends 30% of each pool's share to reserve emissions and rejects a
    # gulp below one BLND. With two equally weighted pools, fifteen seconds
    # provides sufficient headroom for both initial streams.
    sleep "${DUAL_MODELED_V2_EMISSION_START_DELAY_SECONDS}"
    legacy_emitted="$(normalize_scalar "$(invoke_transaction \
        "emit-legacy-blnd" "${legacy_emitter}" distribute)")"
    legacy_distributed="$(normalize_scalar "$(invoke_transaction \
        "distribute-legacy-blnd" "${v2_backstop}" distribute)")"
    for pool in \
        "$(state_value modeled_v2_fixed_pool)" \
        "$(state_value modeled_v2_custom_pool)"
    do
        pool_gulped="$(normalize_scalar "$(invoke_transaction \
            "gulp-dual-modeled-v2-emissions" "${pool}" gulp_emissions)")"
        [[ "${pool_gulped}" =~ ^[1-9][0-9]*$ ]] ||
            die "modeled v2 pool gulped no BLND: ${pool}"
    done
    now="$(date +%s)"
    for pool in \
        "$(state_value modeled_v2_fixed_pool)" \
        "$(state_value modeled_v2_custom_pool)"
    do
        stream="$(invoke_view "verify-dual-modeled-v2-emission-stream" \
            "${pool}" get_reserve_emissions --reserve_token_index 3)"
        modeled_v2_emission_stream_is_active "${stream}" "${now}" ||
            die "modeled v2 USDC supply emission stream is inactive: ${pool}"
    done
    v3_emitted="$(normalize_scalar "$(invoke_transaction \
        "emit-v3-blnt" "${v3_backstop}" distribute)")"
    [[ "${legacy_emitted}" =~ ^[1-9][0-9]*$ ]] || die "legacy emitter minted no BLND"
    [[ "${legacy_distributed}" =~ ^[1-9][0-9]*$ ]] ||
        die "v2 backstop distributed no BLND"
    [[ "${v3_emitted}" =~ ^[1-9][0-9]*$ ]] || die "v3 emitter minted no BLNT"

    legacy_admin="$(normalize_scalar "$(invoke_view \
        "query-blnd-admin" "${legacy_blnd}" admin)")"
    blnt_admin="$(normalize_scalar "$(invoke_view \
        "query-blnt-admin" "${blnt}" admin)")"
    legacy_recipient="$(normalize_scalar "$(invoke_view \
        "query-legacy-recipient" "${legacy_emitter}" get_backstop)")"
    v3_recipient="$(normalize_scalar "$(invoke_view \
        "query-v3-recipient" "${v3_emitter}" get_backstop)")"
    assert_equal "${legacy_emitter}" "${legacy_admin}" "BLND administrator"
    assert_equal "${v3_emitter}" "${blnt_admin}" "BLNT administrator"
    assert_equal "${v2_backstop}" "${legacy_recipient}" "legacy emitter recipient"
    assert_equal "${v3_backstop}" "${v3_recipient}" "v3 emitter recipient"

    state_set "blnt_swap_cap" "${BLNT_SWAP_CAP}"
    state_set "blnd_swap_input_cap" "${BLND_SWAP_INPUT_CAP}"
    state_set "blnt_backfill_initial_balance" "${BLNT_BACKFILL_DROP}"
    state_set "legacy_emitted" "${legacy_emitted}"
    state_set "legacy_backstop_distributed" "${legacy_distributed}"
    state_set "modeled_v2_emissions_started_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    state_set "v3_emitted" "${v3_emitted}"
    state_set "phase" "verified"
    state_set "verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted >"${RUN_DIR}/ledger-after-dual-emitter-deploy.json"
    note "Dual-emitter deployment verified. State: ${STATE_FILE}"
}

dual_status() {
    local legacy_emitter v3_emitter blnt_backfill legacy_recipient v3_recipient queued
    local swapped blnd_burned remaining_capacity backfill_balance vesting_start vesting_end
    local grant_vesting_start grant_vesting_end swap_deadline
    local expired_blnt_burned
    local backfill_allocated grant_allocated backfill_claimed grant_claimed

    validate_tools
    require_network
    load_state
    legacy_emitter="$(state_value legacy_emitter)"
    v3_emitter="$(state_value v3_emitter)"
    blnt_backfill="$(state_value blnt_backfill)"
    legacy_recipient="$(normalize_scalar "$(invoke_view \
        "status-legacy-recipient" "${legacy_emitter}" get_backstop)")"
    v3_recipient="$(normalize_scalar "$(invoke_view \
        "status-v3-recipient" "${v3_emitter}" get_backstop)")"
    swapped="$(normalize_scalar "$(invoke_view \
        "status-total-swapped" "${blnt_backfill}" get_total_swapped)")"
    blnd_burned="$(normalize_scalar "$(invoke_view \
        "status-total-blnd-burned" "${blnt_backfill}" get_total_blnd_burned)")"
    remaining_capacity="$(normalize_scalar "$(invoke_view \
        "status-swap-capacity" "${blnt_backfill}" get_remaining_swap_capacity)")"
    backfill_balance="$(normalize_scalar "$(invoke_view \
        "status-backfill-balance" "$(state_value blnt_token)" balance \
        --id "${blnt_backfill}")")"
    vesting_start="$(normalize_scalar "$(invoke_view \
        "status-backfill-vesting-start" "${blnt_backfill}" get_vesting_start)")"
    vesting_end="$(normalize_scalar "$(invoke_view \
        "status-backfill-vesting-end" "${blnt_backfill}" get_vesting_end)")"
    grant_vesting_start="$(normalize_scalar "$(invoke_view \
        "status-grant-vesting-start" "${blnt_backfill}" get_grant_vesting_start)")"
    grant_vesting_end="$(normalize_scalar "$(invoke_view \
        "status-grant-vesting-end" "${blnt_backfill}" get_grant_vesting_end)")"
    swap_deadline="$(normalize_scalar "$(invoke_view \
        "status-swap-deadline" "${blnt_backfill}" get_swap_deadline)")"
    expired_blnt_burned="$(normalize_scalar "$(invoke_view \
        "status-expired-blnt-burned" "${blnt_backfill}" get_expired_blnt_burned)")"
    backfill_allocated="$(normalize_scalar "$(invoke_view \
        "status-backfill-allocated" "${blnt_backfill}" get_backfill_allocated)")"
    grant_allocated="$(normalize_scalar "$(invoke_view \
        "status-grant-allocated" "${blnt_backfill}" get_grant_allocated)")"
    backfill_claimed="$(normalize_scalar "$(invoke_view \
        "status-backfill-claimed" "${blnt_backfill}" get_backfill_claimed)")"
    grant_claimed="$(normalize_scalar "$(invoke_view \
        "status-grant-claimed" "${blnt_backfill}" get_grant_claimed)")"
    queued="$(invoke_view "status-v3-queue" "${v3_emitter}" get_queued_swap)"
    cat <<EOF
Dual-emitter ${NETWORK_LABEL} status
  phase: $(state_value phase)
  BLNT metadata name: $(state_optional blnt_token_name)
  BLNT home domain: $(state_optional blnt_home_domain)
  legacy BLND emitter: ${legacy_emitter}
  legacy recipient: ${legacy_recipient}
  v3 BLNT emitter: ${v3_emitter}
  v3 recipient: ${v3_recipient}
  v3 queue: ${queued}
  BLNT backfill contract: ${blnt_backfill}
  backfill BLNT balance: ${backfill_balance}
  backfill vesting start: ${vesting_start}
  backfill vesting end: ${vesting_end}
  backfill allocated: ${backfill_allocated}
  backfill claimed: ${backfill_claimed}
  grant vesting start: ${grant_vesting_start}
  grant vesting end: ${grant_vesting_end}
  grant allocated: ${grant_allocated}
  grant claimed: ${grant_claimed}
  converted BLNT output: ${swapped}
  converted BLND burned: ${blnd_burned}
  remaining BLNT conversion capacity: ${remaining_capacity}
  conversion deadline: ${swap_deadline}
  expired conversion-reserve BLNT burned: ${expired_blnt_burned}
  Comet v2 commit: $(state_value comet_v2_source_commit)
  Comet v2 WASM: $(state_value comet_v2_wasm_sha256)
  state: ${STATE_FILE}
EOF
}

dual_run() {
    dual_validate
    command_start
    dual_deploy true
}

main_dual() {
    case "${1:-}" in
        plan) dual_plan ;;
        validate) dual_validate ;;
        start) command_start ;;
        deploy) dual_deploy ;;
        run) dual_run ;;
        status) dual_status ;;
        stop) command_stop ;;
        -h|--help|help) usage_dual ;;
        *) usage_dual >&2; exit 2 ;;
    esac
}

if [[ "${BLEND_DUAL_EMITTER_LIBRARY_ONLY:-false}" != "true" ]]; then
    main_dual "$@"
fi
