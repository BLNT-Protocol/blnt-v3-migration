#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export BLEND_DUAL_EMITTER_LIBRARY_ONLY=true
# Reuse the audited transaction/evidence helpers and the independent V3
# contract setup without entering the dual-emitter command dispatcher.
# shellcheck source=dual-emitter-deployment.sh
source "${SCRIPT_DIR}/dual-emitter-deployment.sh"
unset BLEND_DUAL_EMITTER_LIBRARY_ONLY

STATE_FILE="${BLEND_MIGRATION_STATE_FILE:-${WORK_DIR}/v3-only-state.json}"
REGISTRY_FILE="${BLEND_V3_ONLY_REGISTRY_FILE:-${ROOT_DIR}/deployments/testnet-v3-only.json}"
EXTERNAL_BLEND_BACKSTOP="${BLEND_EXTERNAL_BLEND_BACKSTOP:-CBDVWXT433PRVTUNM56C3JREF3HIZHRBA64NB2C3B2UNCKIS65ZYCLZA}"
EXTERNAL_BLND_TOKEN="${BLEND_EXTERNAL_BLND_TOKEN:-CB22KRA3YZVCNCQI64JQ5WE7UY2VAV7WFLK6A2JN3HEX56T2EDAFO7QF}"
EXPECTED_EXTERNAL_BLND_USDC="${BLEND_EXTERNAL_BLND_USDC:-CA5UTUUPHYL5K22UBRUVC37EARZUGYOSGK3IKIXG2JLCC5ZZLI4BDWDM}"
V3_USER_WALLET="${BLEND_V3_USER_WALLET:-GDPAIYJ2FISB5H7JNWDNRERAIMGZSP4HVQKYMKO42KRB23GANJXNG2ZI}"
V3_BLNT_MINT=$((1000 * SCALAR_7))
V3_USDC_MINT=$((100000 * SCALAR_7))
V3_BLNT_USDC_LP_TOTAL=$((2 * DUAL_MODELED_V3_LP_BALANCE))

usage_v3_only() {
    cat <<'EOF'
Usage: scripts/v3-only-deployment.sh COMMAND

Commands:
  plan      Describe the V3-only deployment and external Blend dependencies.
  validate  Build and validate only V3, backfill, oracle, and Comet v2 artifacts.
  start     Verify the public testnet RPC (or start local Quickstart).
  deploy    Deploy and verify a fresh V3 stack without deploying or mutating V1/V2.
  deploy-modeled-pools
            Resume the V3 modeled-pool and emissions phase after core deployment.
  deploy-reserve-loss-pool
            Deploy and verify the V3 Reserve Loss scenario pool.
  verify-reserve-loss-pool
            Reverify the deployed V3 Reserve Loss scenario pool.
  run       Validate, start, and deploy.
  status    Query the recorded V3 and external legacy bindings.
  write-registry
            Write the V3-only deployment registry from verified state.
  refresh-oracle
            Refresh the controlled V3 fixture-oracle histories.
  stop      Stop the local Quickstart container.

The external Blend backstop and BLND token are read-only inputs. The runner
does not deploy a legacy emitter, V1/V2 factory, V1/V2 backstop, V1/V2 pool,
BLND token, or BLND liquidity pool.
EOF
}

v3_only_plan() {
    cat <<EOF
V3-only ${NETWORK_LABEL} deployment:
  external Blend V1/V2 backstop: ${EXTERNAL_BLEND_BACKSTOP}
  external legacy BLND token:    ${EXTERNAL_BLND_TOKEN}
  expected legacy BLND:USDC LP:  ${EXPECTED_EXTERNAL_BLND_USDC}

  1. Read and verify the existing Blend backstop, reward zone, canonical LP,
     and BLND token. No transaction is submitted to the legacy lane.
  2. Build and deploy BLNT, a V3 USDC fixture asset, the V3 emitter, backfill
     contract, V3 factory/backstop, and BLNT:USDC / BLNT:XLM Comet v2 LPs.
  3. Send the 150-million-BLNT initial drop to the backfill contract, with the
     existing BLND token bound as the only legacy conversion input.
  4. Deploy and activate the Fixed, Custom, and Reserve Loss V3 pools, add
     them to the V3 reward zone, seed their positions, and start BLNT emissions.
  5. Preserve the configured 10-million-BLNT grant for ${V3_USER_WALLET}.

No V1/V2 contracts or BLND assets are deployed or mutated.
EOF
}

v3_only_update_reward_zone() {
    local reward_zone

    reward_zone="$(invoke_view "query-v3-only-registry-reward-zone" \
        "$(state_value v3_backstop)" reward_zone)"
    state_set "v3_reward_zone" "$(printf '%s' "${reward_zone}" | jq -c '.')"
}

v3_only_write_registry() {
    local network_protocol registry_dir registry_tmp

    [[ -f "${STATE_FILE}" ]] || die "v3-only state not found: ${STATE_FILE}"
    network_protocol="$(stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted | jq -er '.protocolVersion')"
    [[ "${network_protocol}" =~ ^[0-9]+$ ]] ||
        die "invalid network protocol version: ${network_protocol}"
    state_set "network_protocol" "${network_protocol}"
    registry_dir="$(dirname "${REGISTRY_FILE}")"
    mkdir -p "${registry_dir}"
    registry_tmp="${REGISTRY_FILE}.tmp"
    jq --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '
        {
          schema_version: 4,
          topology: "v3-only-external-blend",
          generated_at: $generated_at,
          network: {
            name: .network_label,
            passphrase: .network_passphrase,
            rpc_url: .rpc_url,
            protocol: (.network_protocol | tonumber),
            xlm_asset_mode: .xlm_asset_mode
          },
          source: {
            migration_repository_commit: .source_commit,
            dirty: .source_dirty,
            patch_sha256: .source_patch_sha256
          },
          accounts: {
            operator: .operator,
            depositor: .depositor,
            configured_user: .configured_user
          },
          external_blend: {
            managed: false,
            backstop: .external_blend_backstop,
            blnd_token: .external_blnd_token,
            blnd_usdc_token: .external_blnd_usdc_token,
            reward_zone: (.external_blend_reward_zone | fromjson)
          },
          assets: {
            BLND: .external_blnd_token,
            BLNT: .blnt_token,
            USDC: .usdc_token,
            XLM: .xlm_token,
            EURC: .modeled_asset_eurc,
            PYUSD: .modeled_asset_pyusd
          },
          comet_pools: {
            BLNT_USDC: {contract_id: .blnt_usdc_token},
            BLNT_XLM: {contract_id: .blnt_xlm_token}
          },
          protocol: {
            v3_emitter: .v3_emitter,
            v2: {
              enabled: false,
              external: true,
              backstop: .external_blend_backstop
            },
            v3: {
              enabled: true,
              backstop: .v3_backstop,
              pool_factory: .v3_factory
            }
          },
          backfill: {
            contract: .blnt_backfill,
            legacy_blnd_token: .external_blnd_token,
            blnt_token: .blnt_token,
            swap_cap: .blnt_swap_cap,
            swap_deadline: .blnt_swap_deadline
          },
          modeled_pools: {
            fixed: {
              v3: {
                pool: .modeled_v3_fixed_pool,
                oracle: .modeled_v3_fixed_oracle,
                reward_zone_member: true,
                pool_emission_position: "EURC borrow"
              }
            },
            custom: {
              v3: {
                pool: .modeled_v3_custom_pool,
                oracle: .modeled_v3_custom_oracle,
                reward_zone_member: true,
                pool_emission_position: null
              }
            }
          },
          reward_zones: {
            external_blend: (.external_blend_reward_zone | fromjson),
            v3: (.v3_reward_zone | fromjson)
          }
        }
        + if ((.reserve_loss_pool // "") | length) > 0 then {
          test_scenarios: {
            reserve_loss: {
              purpose: "V3 reserve-loss and reserve-authorization UI scenario",
              deployed_at: .reserve_loss_deployed_at,
              verified_at: .reserve_loss_verified_at,
              contracts: {
                pool_factory: .reserve_loss_pool_factory,
                backstop: .reserve_loss_backstop,
                pool: .reserve_loss_pool,
                oracle: .reserve_loss_oracle,
                pool_wasm_sha256: .reserve_loss_pool_wasm_hash
              },
              backstop: {reward_zone_member: true},
              reserves: {
                HLTY: {contract_id: .reserve_loss_asset_hlty},
                IMPR: {contract_id: .reserve_loss_asset_impr},
                EXHT: {contract_id: .reserve_loss_asset_exht},
                USDP: {
                  contract_id: .reserve_loss_asset_usdp,
                  authorized_to_pool: ((.reserve_loss_usdp_authorized // "false") == "true")
                }
              }
            }
          }
        } else {test_scenarios: {}} end
    ' "${STATE_FILE}" >"${registry_tmp}"
    mv "${registry_tmp}" "${REGISTRY_FILE}"
    note "Wrote V3-only deployment registry: ${REGISTRY_FILE}"
}

v3_only_refresh_oracles() {
    local model oracle reserve_loss_oracle

    validate_tools
    require_network
    load_state
    require_modeled_pool_support
    for model in fixed custom; do
        oracle="$(state_value "$(modeled_oracle_state_key v3 "${model}")")"
        seed_modeled_sep40_oracle v3 "${model}" "${oracle}"
    done
    reserve_loss_oracle="$(state_optional reserve_loss_oracle)"
    if [[ -n "${reserve_loss_oracle}" ]]; then
        require_reserve_loss_support
        seed_reserve_loss_oracle "${reserve_loss_oracle}"
    fi
    state_set "modeled_oracles_refreshed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Refreshed the V3-only fixture-oracle histories."
}

v3_only_validate_artifacts() {
    local artifact

    for artifact in \
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
    [[ -s "${MODELED_POOLS_FIXTURE}" ]] ||
        die "modeled-pool fixture is missing: ${MODELED_POOLS_FIXTURE}"
}

v3_only_validate() {
    validate_tools
    validate_backfill_manifest
    (
        cd "${ROOT_DIR}"
        make build-v3
    )
    v3_only_validate_artifacts
    note "Validated V3, backfill, oracle, and Comet v2 artifacts only."
}

verify_external_blend_lane() {
    local symbol decimals backstop_token tokens reward_zone

    require_contract_id "${EXTERNAL_BLEND_BACKSTOP}" "external Blend backstop"
    require_contract_id "${EXTERNAL_BLND_TOKEN}" "external BLND token"
    require_contract_id "${EXPECTED_EXTERNAL_BLND_USDC}" "external BLND:USDC LP"
    symbol="$(normalize_scalar "$(invoke_view \
        "external-blnd-symbol" "${EXTERNAL_BLND_TOKEN}" symbol)")"
    decimals="$(normalize_scalar "$(invoke_view \
        "external-blnd-decimals" "${EXTERNAL_BLND_TOKEN}" decimals)")"
    assert_equal "BLND" "${symbol}" "external legacy token symbol"
    assert_equal "7" "${decimals}" "external legacy token decimals"
    backstop_token="$(normalize_scalar "$(invoke_view \
        "external-backstop-token" "${EXTERNAL_BLEND_BACKSTOP}" backstop_token)")"
    assert_equal "${EXPECTED_EXTERNAL_BLND_USDC}" "${backstop_token}" \
        "external Blend backstop token"
    tokens="$(invoke_view "external-backstop-token-members" \
        "${backstop_token}" get_tokens)"
    printf '%s' "${tokens}" | jq -e --arg token "${EXTERNAL_BLND_TOKEN}" \
        'index($token) != null' >/dev/null ||
        die "external canonical LP does not contain the configured BLND token"
    reward_zone="$(invoke_view "external-blend-reward-zone" \
        "${EXTERNAL_BLEND_BACKSTOP}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e \
        'type == "array" and length > 0 and all(.[]; test("^C[A-Z2-7]{55}$"))' \
        >/dev/null || die "external Blend reward zone is empty or invalid"
    state_set "external_blend_backstop" "${EXTERNAL_BLEND_BACKSTOP}"
    state_set "external_blnd_token" "${EXTERNAL_BLND_TOKEN}"
    state_set "external_blnd_usdc_token" "${backstop_token}"
    state_set "external_blend_reward_zone" "$(printf '%s' "${reward_zone}" | jq -c '.')"
}

v3_only_deploy_modeled_pools() {
    local fixed_oracle custom_oracle fixed_pool custom_pool pool status reward_zone

    ensure_modeled_asset EURC
    ensure_modeled_asset PYUSD
    fixed_oracle="$(ensure_modeled_v3_oracle fixed)"
    custom_oracle="$(ensure_modeled_v3_oracle custom)"
    state_set "modeled_oracles_refreshed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    fixed_pool="$(ensure_modeled_v3_pool fixed "${fixed_oracle}")"
    custom_pool="$(ensure_modeled_v3_pool custom "${custom_oracle}")"

    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_pool}" FirstLoss "${DUAL_MODELED_V3_LP_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_pool}" SecondLoss "${DUAL_MODELED_V3_LP_BALANCE}"
    ensure_token_balance "$(state_value usdc_token)" \
        "$((2 * MODELED_V3_USDC_BALANCE))" "v3-only-modeled-usdc"
    ensure_modeled_v3_backstop_deposit \
        fixed "${fixed_pool}" ThirdLoss "${MODELED_V3_USDC_BALANCE}"
    ensure_modeled_xlm_balance "${MODELED_CUSTOM_V3_XLM_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        custom "${custom_pool}" FirstLoss "${MODELED_CUSTOM_V3_XLM_BALANCE}"
    ensure_modeled_v3_backstop_deposit \
        custom "${custom_pool}" SecondLoss "${MODELED_CUSTOM_V3_USDC_BALANCE}"

    for pool in "${fixed_pool}" "${custom_pool}"; do
        invoke_transaction "activate-v3-only-modeled-pool" \
            "${pool}" set_status --pool_status 0 >/dev/null
        status="$(invoke_view "query-v3-only-modeled-status" \
            "${pool}" get_config | jq -er '.status | tostring')"
        assert_equal "0" "${status}" "modeled v3 active status"
    done

    reward_zone="$(invoke_view "query-v3-only-reward-zone" \
        "$(state_value v3_backstop)" reward_zone)"
    for pool in "${fixed_pool}" "${custom_pool}"; do
        if ! printf '%s' "${reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null
        then
            invoke_transaction "add-v3-only-modeled-reward" \
                "$(state_value v3_backstop)" add_reward \
                --to_add "${pool}" --to_remove null >/dev/null
            reward_zone="$(invoke_view "query-updated-v3-only-reward-zone" \
                "$(state_value v3_backstop)" reward_zone)"
        fi
    done
    for pool in "${fixed_pool}" "${custom_pool}"; do
        printf '%s' "${reward_zone}" | jq -e \
            --arg pool "${pool}" 'index($pool) != null' >/dev/null ||
            die "modeled v3 pool missing from reward zone: ${pool}"
    done

    ensure_modeled_v3_emissions_config fixed "${fixed_pool}"
    ensure_modeled_v3_supply fixed "${fixed_pool}"
    ensure_modeled_v3_supply custom "${custom_pool}"
    ensure_fixed_v3_eurc_borrow "${fixed_pool}"
    state_set "modeled_pools_fixture_sha256" \
        "$(sha256_file "${MODELED_POOLS_FIXTURE}")"
    state_set "modeled_v3_lending_oracle_wasm_sha256" \
        "$(sha256_file "${V3_TEST_SEP40_ORACLE_WASM}")"
    state_set "modeled_pools_deployed_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    note "Deployed and activated only the modeled Fixed and Custom V3 pools."
}

v3_only_deploy() {
    local skip_build="${1:-false}"
    local operator depositor controller issuer blnt usdc xlm xlm_balance
    local v3_emitter backfill blnt_usdc blnt_xlm
    local v3_pool_hash v3_backstop v3_factory deployed_v3_backstop
    local backfill_balance recipient admin emitted reward_zone
    local total_swapped total_refunded net_swapped capacity
    local vesting_start vesting_end grant_start grant_end swap_deadline

    validate_tools
    validate_backfill_manifest
    load_claim_lists
    require_network
    [[ ! -e "${STATE_FILE}" ]] ||
        die "v3-only state already exists at ${STATE_FILE}; refusing to overwrite it"
    if [[ "${skip_build}" != "true" ]]; then
        v3_only_validate
    fi
    v3_only_validate_artifacts
    new_run_dir

    if ! stellar_cli keys public-key "${OPERATOR_IDENTITY}" >/dev/null 2>&1; then
        stellar_cli keys generate "${OPERATOR_IDENTITY}" >/dev/null
    fi
    operator="$(stellar_cli keys public-key "${OPERATOR_IDENTITY}")"
    fund_and_wait_for_account "${operator}"
    issuer="${operator}"
    initialize_state "${operator}"
    state_set "topology" "v3-only-external-blend"
    state_set "blnt_token_name" "${BLNT_TOKEN_NAME}"
    state_set "configured_user" "${V3_USER_WALLET}"
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
    state_set "v3_emitter_wasm_sha256" "$(sha256_file "${V3_EMITTER_WASM}")"
    state_set "v3_backstop_wasm_sha256" "$(sha256_file "${V3_BACKSTOP_WASM}")"
    state_set "blnt_backfill_wasm_sha256" "$(sha256_file "${BLNT_BACKFILL_WASM}")"
    state_set "comet_v2_wasm_sha256" "$(sha256_file "${COMET_WASM}")"
    state_set "comet_v2_source_commit" \
        "$(git -C "${ROOT_DIR}/comet-contracts-v2" rev-parse HEAD)"

    verify_external_blend_lane

    if ! stellar_cli keys public-key "${DEPOSITOR_IDENTITY}" >/dev/null 2>&1; then
        stellar_cli keys generate "${DEPOSITOR_IDENTITY}" >/dev/null
    fi
    depositor="$(stellar_cli keys public-key "${DEPOSITOR_IDENTITY}")"
    fund_and_wait_for_account "${depositor}"
    state_set "depositor" "${depositor}"
    if ! stellar_cli keys public-key "${COMET_CONTROLLER_IDENTITY}" >/dev/null 2>&1; then
        stellar_cli keys generate "${COMET_CONTROLLER_IDENTITY}" >/dev/null
    fi
    controller="$(stellar_cli keys public-key "${COMET_CONTROLLER_IDENTITY}")"
    fund_and_wait_for_account "${controller}"
    state_set "comet_controller" "${controller}"

    blnt="$(deploy_asset "deploy-v3-only-blnt" "BLNT" "${issuer}")"
    usdc="$(deploy_asset "deploy-v3-only-usdc" "USDC" "${issuer}")"
    if [[ "${XLM_ASSET_MODE}" == "native" ]]; then
        xlm="$(resolve_native_asset "resolve-v3-only-native-xlm")"
        xlm_balance="${COMET_INITIAL_XLM}"
    else
        xlm="$(deploy_asset "deploy-v3-only-xlm" "XLM" "${issuer}")"
        xlm_balance="${COMET_INITIAL_PAIR_TOKEN}"
    fi
    state_set "blnt_token" "${blnt}"
    state_set "usdc_token" "${usdc}"
    state_set "xlm_token" "${xlm}"
    state_set "blnd_token" "${blnt}"

    create_trustline "trust-v3-depositor-blnt" \
        "${DEPOSITOR_IDENTITY}" "BLNT:${issuer}"
    create_trustline "trust-v3-depositor-usdc" \
        "${DEPOSITOR_IDENTITY}" "USDC:${issuer}"
    create_trustline "trust-v3-controller-blnt" \
        "${COMET_CONTROLLER_IDENTITY}" "BLNT:${issuer}"
    create_trustline "trust-v3-controller-usdc" \
        "${COMET_CONTROLLER_IDENTITY}" "USDC:${issuer}"

    v3_emitter="$(deploy_wasm "deploy-v3-only-emitter" \
        "${V3_EMITTER_WASM}" "" --initializer "${operator}")"
    backfill="$(deploy_wasm "deploy-v3-only-backfill" \
        "${BLNT_BACKFILL_WASM}" "" \
        --legacy_blnd_token "${EXTERNAL_BLND_TOKEN}" \
        --blnt_token "${blnt}" \
        --claim_list "${BLNT_BACKFILL_CLAIM_LIST}" \
        --grant_list "${BLNT_GRANT_LIST}")"
    state_set "v3_emitter" "${v3_emitter}"
    state_set "emitter" "${v3_emitter}"
    state_set "blnt_backfill" "${backfill}"
    state_set "blnt_backfill_claim_list" "${BLNT_BACKFILL_CLAIM_LIST}"
    state_set "blnt_backfill_claim_total" "${BLNT_BACKFILL_CLAIM_TOTAL}"
    state_set "blnt_grant_list" "${BLNT_GRANT_LIST}"
    state_set "blnt_grant_total" "${BLNT_GRANT_TOTAL}"

    blnt_usdc="$(deploy_wasm "deploy-v3-only-comet-blnt-usdc" "${COMET_WASM}" "")"
    blnt_xlm="$(deploy_wasm "deploy-v3-only-comet-blnt-xlm" "${COMET_WASM}" "")"
    state_set "blnt_usdc_token" "${blnt_usdc}"
    state_set "blnt_xlm_token" "${blnt_xlm}"

    invoke_transaction "mint-v3-controller-blnt" "${blnt}" mint \
        --to "${controller}" --amount "${BLNT_MINT}" >/dev/null
    invoke_transaction "mint-v3-controller-usdc" "${usdc}" mint \
        --to "${controller}" --amount "${USDC_MINT}" >/dev/null
    invoke_transaction "mint-v3-depositor-blnt" "${blnt}" mint \
        --to "${depositor}" --amount "${V3_BLNT_MINT}" >/dev/null
    invoke_transaction "mint-v3-depositor-usdc" "${usdc}" mint \
        --to "${depositor}" --amount "${V3_USDC_MINT}" >/dev/null

    initialize_comet "v3-only-blnt-usdc" "${blnt_usdc}" "${controller}" \
        "${blnt}" "${usdc}" "${COMET_INITIAL_BLND}" "${COMET_INITIAL_PAIR_TOKEN}"
    initialize_comet "v3-only-blnt-xlm" "${blnt_xlm}" "${controller}" \
        "${blnt}" "${xlm}" "${COMET_INITIAL_BLND}" "${xlm_balance}"
    invoke_transaction_as "join-v3-only-blnt-usdc" \
        "${DEPOSITOR_IDENTITY}" "${blnt_usdc}" join_pool \
        --pool_amount_out "${V3_BLNT_USDC_LP_TOTAL}" \
        --user "${depositor}" \
        --max_amounts_in "[\"${V3_BLNT_MINT}\",\"${V3_USDC_MINT}\"]" >/dev/null
    invoke_transaction_as "join-v3-only-blnt-xlm" \
        "${DEPOSITOR_IDENTITY}" "${blnt_xlm}" join_pool \
        --pool_amount_out "${DUAL_MODELED_V3_LP_BALANCE}" \
        --user "${depositor}" \
        --max_amounts_in "[\"${V3_BLNT_MINT}\",\"${COMET_INITIAL_XLM}\"]" >/dev/null
    capture "lock-v3-only-comet-controller" \
        stellar_cli tx new set-options \
        --source-account "${COMET_CONTROLLER_IDENTITY}" \
        --low-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --med-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --high-threshold "${COMET_CONTROLLER_THRESHOLD}" \
        --master-weight 0 \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    verify_locked_comet_controller "v3-only-blnt-usdc" \
        "${blnt_usdc}" "${controller}" "${blnt}" "${usdc}"
    verify_locked_comet_controller "v3-only-blnt-xlm" \
        "${blnt_xlm}" "${controller}" "${blnt}" "${xlm}"

    invoke_transaction "set-v3-only-emitter-as-blnt-admin" \
        "${blnt}" set_admin --new_admin "${v3_emitter}" >/dev/null
    v3_pool_hash="$(upload_wasm "upload-v3-only-pool" "${V3_POOL_WASM}")"
    v3_backstop="$(predict_contract_id "${CANDIDATE_SALT}")"
    state_set "v3_backstop" "${v3_backstop}"
    state_set "candidate_backstop" "${v3_backstop}"
    v3_factory="$(deploy_wasm "deploy-v3-only-factory" \
        "${V3_FACTORY_WASM}" "" \
        --pool_init_meta \
        "{\"backstop\":\"${v3_backstop}\",\"blnt_id\":\"${blnt}\",\"pool_hash\":\"${v3_pool_hash}\"}")"
    state_set "v3_factory" "${v3_factory}"
    state_set "v3_pool_factory" "${v3_factory}"
    state_set "v3_pool_hash" "${v3_pool_hash}"
    state_set "v3_pool_wasm_hash" "${v3_pool_hash}"
    invoke_transaction "initialize-v3-only-emitter" "${v3_emitter}" initialize \
        --blnd_token "${blnt}" \
        --backstop "${v3_backstop}" \
        --backstop_token "${blnt_usdc}" >/dev/null
    deployed_v3_backstop="$(deploy_wasm "deploy-v3-only-backstop" \
        "${V3_BACKSTOP_WASM}" "${CANDIDATE_SALT}" \
        --blnt_usdc_token "${blnt_usdc}" \
        --blnt_xlm_token "${blnt_xlm}" \
        --emitter "${v3_emitter}" \
        --blnt_token "${blnt}" \
        --usdc_token "${usdc}" \
        --xlm_token "${xlm}" \
        --pool_factory "${v3_factory}" \
        --drop_list "[[\"${backfill}\",\"${BLNT_BACKFILL_DROP}\"]]")"
    assert_equal "${v3_backstop}" "${deployed_v3_backstop}" "v3 backstop address"
    invoke_transaction "activate-v3-only" "${v3_backstop}" distribute >/dev/null
    invoke_transaction "execute-v3-only-backfill-drop" "${v3_backstop}" drop >/dev/null

    backfill_balance="$(normalize_scalar "$(invoke_view \
        "query-v3-only-backfill-balance" "${blnt}" balance --id "${backfill}")")"
    assert_equal "${BLNT_BACKFILL_DROP}" "${backfill_balance}" "backfill BLNT custody"
    total_swapped="$(normalize_scalar "$(invoke_view \
        "query-v3-only-total-swapped" "${backfill}" get_total_swapped)")"
    total_refunded="$(normalize_scalar "$(invoke_view \
        "query-v3-only-total-refunded" "${backfill}" get_total_refunded)")"
    net_swapped="$(normalize_scalar "$(invoke_view \
        "query-v3-only-net-swapped" "${backfill}" get_net_swapped)")"
    capacity="$(normalize_scalar "$(invoke_view \
        "query-v3-only-swap-capacity" "${backfill}" get_remaining_swap_capacity)")"
    assert_equal "0" "${total_swapped}" "initial gross swapped BLND"
    assert_equal "0" "${total_refunded}" "initial refunded BLND"
    assert_equal "0" "${net_swapped}" "initial net swapped BLND"
    assert_equal "${BLNT_SWAP_CAP}" "${capacity}" "initial swap capacity"
    assert_equal "${EXTERNAL_BLND_TOKEN}" "$(normalize_scalar "$(invoke_view \
        "query-v3-only-backfill-blnd" "${backfill}" get_legacy_blnd_token)")" \
        "backfill external BLND binding"

    vesting_start="$(normalize_scalar "$(invoke_view \
        "query-v3-only-vesting-start" "${backfill}" get_vesting_start)")"
    vesting_end="$(normalize_scalar "$(invoke_view \
        "query-v3-only-vesting-end" "${backfill}" get_vesting_end)")"
    grant_start="$(normalize_scalar "$(invoke_view \
        "query-v3-only-grant-start" "${backfill}" get_grant_vesting_start)")"
    grant_end="$(normalize_scalar "$(invoke_view \
        "query-v3-only-grant-end" "${backfill}" get_grant_vesting_end)")"
    swap_deadline="$(normalize_scalar "$(invoke_view \
        "query-v3-only-swap-deadline" "${backfill}" get_swap_deadline)")"
    assert_equal "${BLNT_BACKFILL_VESTING_SECONDS}" \
        "$((vesting_end - vesting_start))" "backfill vesting duration"
    assert_equal "${vesting_start}" "${grant_start}" "shared vesting start"
    assert_equal "${BLNT_GRANT_VESTING_SECONDS}" \
        "$((grant_end - grant_start))" "grant vesting duration"
    assert_equal "${BLNT_SWAP_DURATION_SECONDS}" \
        "$((swap_deadline - vesting_start))" "swap duration"
    state_set "blnt_backfill_vesting_start" "${vesting_start}"
    state_set "blnt_backfill_vesting_end" "${vesting_end}"
    state_set "blnt_grant_vesting_start" "${grant_start}"
    state_set "blnt_grant_vesting_end" "${grant_end}"
    state_set "blnt_swap_deadline" "${swap_deadline}"
    state_set "blnt_swap_cap" "${BLNT_SWAP_CAP}"
    state_set "blnt_backfill_initial_balance" "${BLNT_BACKFILL_DROP}"
    state_set "phase" "core-deployed"

    v3_only_deploy_modeled_pools
    sleep "${EMISSION_START_DELAY_SECONDS}"
    emitted="$(normalize_scalar "$(invoke_transaction \
        "emit-v3-only-blnt" "${v3_backstop}" distribute)")"
    [[ "${emitted}" =~ ^[1-9][0-9]*$ ]] || die "v3 emitter minted no BLNT"
    admin="$(normalize_scalar "$(invoke_view \
        "query-v3-only-blnt-admin" "${blnt}" admin)")"
    recipient="$(normalize_scalar "$(invoke_view \
        "query-v3-only-emitter-recipient" "${v3_emitter}" get_backstop)")"
    assert_equal "${v3_emitter}" "${admin}" "BLNT administrator"
    assert_equal "${v3_backstop}" "${recipient}" "V3 emitter recipient"
    reward_zone="$(invoke_view "query-final-v3-only-reward-zone" \
        "${v3_backstop}" reward_zone)"
    state_set "v3_reward_zone" "$(printf '%s' "${reward_zone}" | jq -c '.')"
    state_set "v3_emitted" "${emitted}"
    state_set "phase" "verified"
    state_set "verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    v3_only_write_registry
    stellar_cli ledger latest \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --output json-formatted >"${RUN_DIR}/ledger-after-v3-only-deploy.json"
    note "V3-only deployment verified. State: ${STATE_FILE}"
}

v3_only_resume_modeled_pools() {
    local emitted admin recipient reward_zone

    validate_tools
    require_network
    load_state
    v3_only_validate_artifacts
    v3_only_deploy_modeled_pools
    sleep "${EMISSION_START_DELAY_SECONDS}"
    emitted="$(normalize_scalar "$(invoke_transaction \
        "emit-resumed-v3-only-blnt" "$(state_value v3_backstop)" distribute)")"
    [[ "${emitted}" =~ ^[1-9][0-9]*$ ]] || die "v3 emitter minted no BLNT"
    admin="$(normalize_scalar "$(invoke_view \
        "query-resumed-v3-only-blnt-admin" "$(state_value blnt_token)" admin)")"
    recipient="$(normalize_scalar "$(invoke_view \
        "query-resumed-v3-only-emitter-recipient" \
        "$(state_value v3_emitter)" get_backstop)")"
    assert_equal "$(state_value v3_emitter)" "${admin}" "BLNT administrator"
    assert_equal "$(state_value v3_backstop)" "${recipient}" "V3 emitter recipient"
    reward_zone="$(invoke_view "query-resumed-final-v3-only-reward-zone" \
        "$(state_value v3_backstop)" reward_zone)"
    state_set "v3_reward_zone" "$(printf '%s' "${reward_zone}" | jq -c '.')"
    state_set "v3_emitted" "${emitted}"
    state_set "phase" "verified"
    state_set "verified_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    v3_only_write_registry
    note "Resumed and verified the V3-only modeled-pool phase."
}

v3_only_reserve_loss() {
    local command="$1" pool_hash authorized

    validate_tools
    require_network
    load_state
    pool_hash="$(state_optional v3_pool_wasm_hash)"
    if [[ -z "${pool_hash}" ]]; then
        pool_hash="$(state_value v3_pool_hash)"
        [[ "${pool_hash}" =~ ^[0-9a-f]{64}$ ]] ||
            die "invalid V3 pool WASM hash: ${pool_hash}"
        state_set "v3_pool_wasm_hash" "${pool_hash}"
    fi
    "${command}"
    if [[ "${command}" == "command_deploy_reserve_loss_pool" ]]; then
        authorized="$(normalize_scalar "$(invoke_view \
            "query-reserve-loss-usdp-authorization" \
            "$(state_value reserve_loss_asset_usdp)" authorized \
            --id "$(state_value reserve_loss_pool)")")"
        if [[ "${authorized}" == "true" ]]; then
            invoke_transaction_as "deauthorize-reserve-loss-usdp" \
                "${RESERVE_LOSS_ISSUER_IDENTITY}" \
                "$(state_value reserve_loss_asset_usdp)" set_authorized \
                --id "$(state_value reserve_loss_pool)" --authorize false >/dev/null
        fi
        authorized="$(normalize_scalar "$(invoke_view \
            "verify-reserve-loss-usdp-deauthorization" \
            "$(state_value reserve_loss_asset_usdp)" authorized \
            --id "$(state_value reserve_loss_pool)")")"
        assert_equal "false" "${authorized}" "Reserve Loss USDP pool authorization"
        state_set "reserve_loss_usdp_authorized" "false"
    fi
    v3_only_update_reward_zone
    v3_only_write_registry
}

v3_only_status() {
    local backfill emitter backstop recipient blnd_binding reward_zone

    validate_tools
    require_network
    load_state
    backfill="$(state_value blnt_backfill)"
    emitter="$(state_value v3_emitter)"
    backstop="$(state_value v3_backstop)"
    recipient="$(normalize_scalar "$(invoke_view \
        "status-v3-only-recipient" "${emitter}" get_backstop)")"
    blnd_binding="$(normalize_scalar "$(invoke_view \
        "status-v3-only-blnd-binding" "${backfill}" get_legacy_blnd_token)")"
    reward_zone="$(invoke_view "status-v3-only-reward-zone" \
        "${backstop}" reward_zone)"
    cat <<EOF
V3-only ${NETWORK_LABEL} status
  phase: $(state_value phase)
  external Blend backstop: $(state_value external_blend_backstop)
  external BLND token: ${blnd_binding}
  external Blend reward zone: $(state_value external_blend_reward_zone)
  BLNT token: $(state_value blnt_token)
  V3 emitter: ${emitter}
  V3 emitter recipient: ${recipient}
  V3 backstop: ${backstop}
  V3 reward zone: ${reward_zone}
  BLNT backfill: ${backfill}
  configured grant user: $(state_value configured_user)
  grant amount: $(state_value blnt_grant_total)
  state: ${STATE_FILE}
EOF
}

v3_only_run() {
    v3_only_validate
    command_start
    v3_only_deploy true
    v3_only_reserve_loss command_deploy_reserve_loss_pool
}

main_v3_only() {
    case "${1:-}" in
        plan) v3_only_plan ;;
        validate) v3_only_validate ;;
        start) command_start ;;
        deploy) v3_only_deploy ;;
        deploy-modeled-pools) v3_only_resume_modeled_pools ;;
        deploy-reserve-loss-pool) v3_only_reserve_loss command_deploy_reserve_loss_pool ;;
        verify-reserve-loss-pool) v3_only_reserve_loss command_verify_reserve_loss_pool ;;
        run) v3_only_run ;;
        status) v3_only_status ;;
        write-registry)
            validate_tools
            load_state
            v3_only_update_reward_zone
            v3_only_write_registry
            ;;
        refresh-oracle) v3_only_refresh_oracles ;;
        stop) command_stop ;;
        -h|--help|help) usage_v3_only ;;
        *) usage_v3_only >&2; exit 2 ;;
    esac
}

main_v3_only "$@"
