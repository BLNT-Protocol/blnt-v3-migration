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
V3_USER_IDENTITY="${BLEND_V3_USER_IDENTITY:-blnt-v3-only-${NETWORK_MODE}-user}"
V3_BLNT_MINT=$((1000 * SCALAR_7))
V3_USDC_MINT=$((100000 * SCALAR_7))
V3_BLNT_USDC_LP_TOTAL=$((2 * DUAL_MODELED_V3_LP_BALANCE))
V3_USER_ASSET_FUNDING=$((1000000 * SCALAR_7))
V3_USER_XLM_FUNDING=$((100000 * SCALAR_7))
V3_USER_XLM_FULL_DONORS=10
V3_USER_XLM_FULL_DONOR_PAYMENT=$((9998 * SCALAR_7))
V3_USER_XLM_FINAL_PAYMENT=$((20 * SCALAR_7))
V3_USER_XLM_BATCH_SIZE=4

# shellcheck source=permissioned-pool.sh
source "${SCRIPT_DIR}/permissioned-pool.sh"
# shellcheck source=sep41-assets-pool.sh
source "${SCRIPT_DIR}/sep41-assets-pool.sh"

usage_v3_only() {
    cat <<'EOF'
Usage: scripts/v3-only-deployment.sh COMMAND

Commands:
  plan      Describe the V3-only deployment and external Blend dependencies.
  validate  Build V3, backfill, and oracle artifacts and verify Comet V1.1.
  start     Verify the public testnet RPC (or start local Quickstart).
  deploy    Deploy and verify a fresh V3 stack without deploying or mutating V1/V2.
  resume-core
            Resume a core deployment after its prerequisites were recorded.
  deploy-modeled-pools
            Resume the V3 modeled-pool and emissions phase after core deployment.
  deploy-reserve-loss-pool
            Deploy and verify the V3 Reserve Loss scenario pool.
  verify-reserve-loss-pool
            Reverify the deployed V3 Reserve Loss scenario pool.
  deploy-permissioned-pool
            Deploy and verify the V3 Permissioned scenario pool.
  verify-permissioned-pool
            Reverify the deployed V3 Permissioned scenario pool.
  deploy-sep41-pool
            Deploy and verify the V3 SEP-41 Assets scenario pool.
  verify-sep41-pool
            Reverify the deployed V3 SEP-41 Assets scenario pool.
  run       Validate, start, and deploy.
  status    Query the recorded V3 and external legacy bindings.
  write-registry
            Write the V3-only deployment registry from verified state.
  refresh-oracle
            Refresh the controlled V3 fixture-oracle histories.
  fund-user Fund the configured user with one million of each issued V3
            reserve asset except BLND and at least 100,000 native XLM.
            LP-share tokens are not included.
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
     contract, V3 factory/backstop, and BLNT:USDC / BLNT:XLM Comet V1.1 LPs.
  3. Send the 150-million-BLNT initial drop to the backfill contract: 74
     million for pre-incident LP holders, 25 million for grants, and 51 million
     for the 1:1 BLND-to-BLNT conversion using the existing BLND token.
  4. Deploy and activate the Fixed, Custom, Reserve Loss, Permissioned, and
     SEP-41 Assets V3 pools, add the BLNT-bearing pools to the V3 reward zone,
     seed their positions, and start BLNT emissions.
  5. Preserve the configured 25-million-BLNT grant for ${V3_USER_WALLET}.
  6. Ensure that wallet holds 1 million each of BLNT, USDC, EURC, PYUSD,
     HLTY, IMPR, EXHT, USDP, wBTC, and wETH, plus at least 100,000 native
     XLM. Do not send BLND or LP-share tokens.

No V1/V2 contracts or BLND assets are deployed or mutated.
EOF
}

v3_only_require_user_access() {
    local configured_user

    configured_user="$(stellar_cli keys public-key "${V3_USER_IDENTITY}")" ||
        die "configured V3 user identity is unavailable: ${V3_USER_IDENTITY}"
    assert_equal "${V3_USER_WALLET}" "${configured_user}" \
        "configured V3 user identity"
    read_horizon_account "${V3_USER_WALLET}" >/dev/null
}

v3_only_ensure_user_trustline() {
    local label="$1" code="$2" issuer="$3" account

    account="$(read_horizon_account "${V3_USER_WALLET}")"
    if printf '%s' "${account}" | jq -e \
        --arg code "${code}" --arg issuer "${issuer}" '
          any(.balances[];
            .asset_type != "native" and
            .asset_code == $code and
            .asset_issuer == $issuer)
        ' >/dev/null
    then
        return
    fi
    capture "trust-v3-user-${label}" \
        stellar_cli tx new change-trust \
        --source-account "${V3_USER_IDENTITY}" \
        --line "${code}:${issuer}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
}

v3_only_ensure_user_token_funding() {
    local label="$1" token="$2" source="$3"
    local state_key recorded balance

    state_key="wallet_${label}_funded"
    recorded="$(state_optional "${state_key}")"
    if [[ -n "${recorded}" ]]; then
        assert_equal "${V3_USER_ASSET_FUNDING}" "${recorded}" \
            "recorded wallet ${label} funding"
        return
    fi
    balance="$(normalize_scalar "$(invoke_view \
        "query-wallet-${label}-before-funding" "${token}" balance \
        --id "${V3_USER_WALLET}")")"
    if (( balance == 0 )); then
        invoke_transaction_as "fund-wallet-${label}" "${source}" \
            "${token}" mint --to "${V3_USER_WALLET}" \
            --amount "${V3_USER_ASSET_FUNDING}" >/dev/null
    else
        assert_equal "${V3_USER_ASSET_FUNDING}" "${balance}" \
            "unrecorded wallet ${label} balance"
    fi
    balance="$(normalize_scalar "$(invoke_view \
        "verify-wallet-${label}-funding" "${token}" balance \
        --id "${V3_USER_WALLET}")")"
    assert_equal "${V3_USER_ASSET_FUNDING}" "${balance}" \
        "wallet ${label} funding"
    state_set "${state_key}" "${V3_USER_ASSET_FUNDING}"
}

v3_only_fund_user_classic_asset() {
    local label="$1" code="$2" token="$3" issuer="$4" source="$5"

    v3_only_ensure_user_trustline "${label}" "${code}" "${issuer}"
    v3_only_ensure_user_token_funding "${label}" "${token}" "${source}"
}

v3_only_fund_user_core_assets() {
    local blnt="$1" usdc="$2" issuer="$3"

    v3_only_require_user_access
    v3_only_fund_user_classic_asset \
        blnt BLNT "${blnt}" "${issuer}" "${OPERATOR_IDENTITY}"
    v3_only_fund_user_classic_asset \
        usdc USDC "${usdc}" "${issuer}" "${OPERATOR_IDENTITY}"
    state_set "funding_wallet" "${V3_USER_WALLET}"
    state_set "wallet_asset_funding" "${V3_USER_ASSET_FUNDING}"
}

v3_only_fund_and_wait_for_donor() {
    local account="$1"

    for _ in {1..120}; do
        if curl --fail --silent --show-error --max-time 5 \
            "${HORIZON_URL}/accounts/${account}" >/dev/null 2>&1 &&
            stellar_cli ledger entry fetch account \
                --account "${account}" \
                --rpc-url "${RPC_URL}" \
                --network-passphrase "${NETWORK_PASSPHRASE}" \
                --output json 2>/dev/null |
                jq -e '.entries | length == 1' >/dev/null 2>&1
        then
            return
        fi
        curl --fail --silent --show-error --max-time 10 \
            --get --data-urlencode "addr=${account}" \
            "${FRIENDBOT_URL}" >/dev/null 2>&1 || true
        sleep 1
    done
    die "funded donor did not become visible through Horizon and RPC: ${account}"
}

v3_only_wallet_xlm_payment_exists() {
    local donor="$1" amount="$2" payments match_count

    if ! payments="$(curl --fail --silent --show-error --max-time 10 \
        "${HORIZON_URL}/accounts/${donor}/payments?limit=20&order=desc" 2>/dev/null)"
    then
        return 2
    fi
    if ! match_count="$(printf '%s' "${payments}" | jq -er \
        --arg donor "${donor}" --arg wallet "${V3_USER_WALLET}" \
        --arg expected "${amount}" '
          def raw:
            split(".") as $parts |
            ((($parts[0] | tonumber) * 10000000) +
             (((($parts[1] // "") + "0000000")[0:7]) | tonumber) | tostring);
          [._embedded.records[] | select(
            .type == "payment" and .from == $donor and .to == $wallet and
            .asset_type == "native" and (.amount | raw) == $expected
          )] | length
        ' 2>/dev/null)"
    then
        return 2
    fi
    case "${match_count}" in
        0) return 1 ;;
        1) return 0 ;;
        *) return 3 ;;
    esac
}

v3_only_fund_user_xlm_donor() {
    local index="$1" amount="$2" identity donor lookup_status

    identity="blnt-v3-only-${NETWORK_MODE}-wallet-xlm-donor-${index}"
    donor="$(stellar_cli keys public-key "${identity}")"
    v3_only_fund_and_wait_for_donor "${donor}"
    lookup_status=2
    for _ in {1..30}; do
        if v3_only_wallet_xlm_payment_exists "${donor}" "${amount}"; then
            lookup_status=0
        else
            lookup_status=$?
        fi
        case "${lookup_status}" in
            0|1) break ;;
            2) sleep 1 ;;
            3) die "wallet XLM donor ${index} has duplicate payments" ;;
        esac
    done
    case "${lookup_status}" in
        0) return ;;
        1) ;;
        *) die "unable to determine wallet XLM donor ${index} payment state" ;;
    esac
    if ! capture "fund-wallet-xlm-${index}" \
            stellar_cli tx new payment \
            --source-account "${identity}" \
            --destination "${V3_USER_WALLET}" \
            --asset native \
            --amount "${amount}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    then
        note "Wallet XLM donor ${index} submission was inconclusive; verifying on-chain state."
    fi
    for _ in {1..30}; do
        if v3_only_wallet_xlm_payment_exists "${donor}" "${amount}"; then
            return
        fi
        lookup_status=$?
        (( lookup_status != 3 )) ||
            die "wallet XLM donor ${index} has duplicate payments"
        sleep 1
    done
    die "wallet XLM donor ${index} payment was not recorded by Horizon"
}

v3_only_fund_user_xlm() {
    local identity amount account balance whole fraction current_balance
    local completed total index batch_end batch_index failed pid
    local -a pids

    account="$(read_horizon_account "${V3_USER_WALLET}")"
    balance="$(printf '%s' "${account}" | jq -er '
        .balances[] | select(.asset_type == "native") | .balance
    ')"
    whole="${balance%%.*}"
    fraction="${balance#*.}0000000"
    fraction="${fraction:0:7}"
    current_balance=$((10#${whole} * SCALAR_7 + 10#${fraction}))
    state_set "wallet_xlm_funding_target" "${V3_USER_XLM_FUNDING}"
    if (( current_balance >= V3_USER_XLM_FUNDING )); then
        state_set "wallet_xlm_funded" "${current_balance}"
        return
    fi

    completed="$(state_optional wallet_xlm_donors_completed)"
    completed="${completed:-0}"
    if (( completed <= V3_USER_XLM_FULL_DONORS )); then
        total=$((completed * V3_USER_XLM_FULL_DONOR_PAYMENT))
    else
        total="${V3_USER_XLM_FUNDING}"
    fi
    index=$((completed + 1))
    while (( index <= V3_USER_XLM_FULL_DONORS + 1 )); do
        batch_end=$((index + V3_USER_XLM_BATCH_SIZE - 1))
        if (( batch_end > V3_USER_XLM_FULL_DONORS + 1 )); then
            batch_end=$((V3_USER_XLM_FULL_DONORS + 1))
        fi
        for ((batch_index = index; batch_index <= batch_end; batch_index++)); do
            identity="blnt-v3-only-${NETWORK_MODE}-wallet-xlm-donor-${batch_index}"
            if ! stellar_cli keys public-key "${identity}" >/dev/null 2>&1; then
                stellar_cli keys generate "${identity}" >/dev/null
            fi
        done
        pids=()
        for ((batch_index = index; batch_index <= batch_end; batch_index++)); do
            if (( batch_index <= V3_USER_XLM_FULL_DONORS )); then
                amount="${V3_USER_XLM_FULL_DONOR_PAYMENT}"
            else
                amount="${V3_USER_XLM_FINAL_PAYMENT}"
            fi
            v3_only_fund_user_xlm_donor "${batch_index}" "${amount}" &
            pids+=("$!")
        done
        failed=0
        for pid in "${pids[@]}"; do
            wait "${pid}" || failed=1
        done
        (( failed == 0 )) || die "one or more wallet XLM donor payments failed"
        for ((batch_index = index; batch_index <= batch_end; batch_index++)); do
            if (( batch_index <= V3_USER_XLM_FULL_DONORS )); then
                total=$((total + V3_USER_XLM_FULL_DONOR_PAYMENT))
            else
                total=$((total + V3_USER_XLM_FINAL_PAYMENT))
            fi
        done
        state_set "wallet_xlm_donors_completed" "${batch_end}"
        index=$((batch_end + 1))
    done
    assert_equal "${V3_USER_XLM_FUNDING}" "${total}" \
        "wallet XLM transfer total"
    state_set "wallet_xlm_funded" "${total}"
}

v3_only_fund_user_assets() {
    local operator reserve_issuer code label token symbol

    validate_tools
    require_network
    load_state
    v3_only_require_user_access
    operator="$(state_value operator)"
    assert_equal "${V3_USER_ASSET_FUNDING}" \
        "$(state_value wallet_blnt_funded)" "wallet BLNT funding"
    assert_equal "${V3_USER_ASSET_FUNDING}" \
        "$(state_value wallet_usdc_funded)" "wallet USDC funding"
    for code in EURC PYUSD; do
        label="$(printf '%s' "${code}" | tr '[:upper:]' '[:lower:]')"
        token="$(modeled_asset_id "${code}")"
        v3_only_fund_user_classic_asset \
            "${label}" "${code}" "${token}" "${operator}" "${OPERATOR_IDENTITY}"
    done
    reserve_issuer="$(state_value reserve_loss_issuer)"
    while IFS= read -r code; do
        label="$(printf '%s' "${code}" | tr '[:upper:]' '[:lower:]')"
        token="$(reserve_loss_asset_id "${code}")"
        v3_only_fund_user_classic_asset \
            "${label}" "${code}" "${token}" "${reserve_issuer}" \
            "${RESERVE_LOSS_ISSUER_IDENTITY}"
    done < <(jq -r '.reserves[].asset' "${RESERVE_LOSS_FIXTURE}")
    while IFS= read -r symbol; do
        label="$(sep41_label "${symbol}")"
        token="$(sep41_asset_id "${symbol}")"
        v3_only_ensure_user_token_funding \
            "${label}" "${token}" "${OPERATOR_IDENTITY}"
    done < <(jq -r '.reserves[].asset' "${SEP41_POOL_FIXTURE}")
    v3_only_fund_user_xlm
    state_set "wallet_funded_at" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    v3_only_write_registry
    note "Funded ${V3_USER_WALLET} with every V3 reserve asset except BLND."
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
            backfill_allocation: .blnt_backfill_claim_total,
            grant_allocation: .blnt_grant_total,
            swap_blnt_cap: .blnt_swap_cap,
            swap_blnd_input_cap: .blnd_swap_input_cap,
            blnd_per_blnt: 1,
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
                reward_zone_member: false,
                pool_emission_position: null
              }
            }
          },
          reward_zones: {
            external_blend: (.external_blend_reward_zone | fromjson),
            v3: (.v3_reward_zone | fromjson)
          }
        }
        + {
          test_scenarios: (
            (if ((.reserve_loss_pool // "") | length) > 0 then {
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
            } else {} end)
            +
            (if ((.permissioned_pool // "") | length) > 0 then {
              permissioned: {
                purpose: "V3 permissioned-pool UI scenario",
                deployed_at: .permissioned_pool_deployed_at,
                verified_at: .permissioned_pool_verified_at,
                contracts: {
                  pool_factory: .v3_factory,
                  backstop: .v3_backstop,
                  pool: .permissioned_pool,
                  oracle: .permissioned_pool_oracle,
                  access_controller: .permissioned_access_controller,
                  access_controller_wasm_sha256: .permissioned_access_controller_wasm_sha256
                },
                setup_user: .permissioned_setup_user,
                permissions: {
                  setup_user: 7,
                  all_other_users: 0
                },
                backstop: {
                  tier: "FirstLoss",
                  asset: "USDC",
                  deposited: (.permissioned_backstop_deposit | tonumber),
                  reward_zone_member: false
                },
                reserves: {
                  XLM: {
                    contract_id: .xlm_token,
                    supplied: (.permissioned_supply_amount | tonumber),
                    borrowed: (.permissioned_borrow_amount | tonumber)
                  },
                  EURC: {
                    contract_id: .modeled_asset_eurc,
                    supplied: (.permissioned_supply_amount | tonumber),
                    borrowed: (.permissioned_borrow_amount | tonumber)
                  }
                }
              }
            } else {} end)
            +
            (if ((.sep41_pool // "") | length) > 0 then {
              sep41_assets: {
                purpose: "V3 custom SEP-41 reserve UI scenario",
                deployed_at: .sep41_pool_deployed_at,
                verified_at: .sep41_pool_verified_at,
                contracts: {
                  pool_factory: .v3_factory,
                  backstop: .v3_backstop,
                  pool: .sep41_pool,
                  oracle: .sep41_pool_oracle,
                  token_wasm_sha256: .sep41_token_wasm_sha256
                },
                setup_user: .sep41_setup_user,
                backstop: {
                  tier: "FirstLoss",
                  asset: "USDC",
                  deposited: (.sep41_backstop_deposit | tonumber),
                  reward_zone_member: false
                },
                reserves: {
                  wBTC: {
                    contract_id: .sep41_asset_wbtc,
                    supplied: (.sep41_wBTC_supplied | tonumber),
                    borrowed: (.sep41_wBTC_borrowed | tonumber)
                  },
                  wETH: {
                    contract_id: .sep41_asset_weth,
                    supplied: (.sep41_wETH_supplied | tonumber),
                    borrowed: (.sep41_wETH_borrowed | tonumber)
                  }
                }
              }
            } else {} end)
          )
        }
        + (if ((.funding_wallet // "") | length) > 0 then {
          wallet_funding: {
            recipient: .funding_wallet,
            amount_per_asset: .wallet_asset_funding,
            assets: ["BLNT", "USDC", "EURC", "PYUSD", "HLTY", "IMPR", "EXHT", "USDP", "wBTC", "wETH"],
            native_xlm_minimum: .wallet_xlm_funding_target,
            excluded_assets: ["BLND"],
            lp_share_tokens_included: false,
            completed_at: (.wallet_funded_at // null)
          }
        } else {} end)
    ' "${STATE_FILE}" >"${registry_tmp}"
    mv "${registry_tmp}" "${REGISTRY_FILE}"
    note "Wrote V3-only deployment registry: ${REGISTRY_FILE}"
}

v3_only_refresh_oracles() {
    local model oracle reserve_loss_oracle sep41_oracle

    validate_tools
    require_network
    load_state
    require_modeled_pool_support
    for model in fixed custom; do
        oracle="$(state_value "$(modeled_oracle_state_key v3 "${model}")")" || return
        seed_modeled_sep40_oracle v3 "${model}" "${oracle}" || return
    done
    reserve_loss_oracle="$(state_optional reserve_loss_oracle)"
    if [[ -n "${reserve_loss_oracle}" ]]; then
        require_reserve_loss_support
        seed_reserve_loss_oracle "${reserve_loss_oracle}" || return
    fi
    sep41_oracle="$(state_optional sep41_pool_oracle)"
    if [[ -n "${sep41_oracle}" ]]; then
        require_sep41_pool_support
        seed_sep41_oracle "${sep41_oracle}" || return
    fi
    state_set "modeled_oracles_refreshed_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" || return
    note "Refreshed the V3-only fixture-oracle histories."
}

v3_only_validate_artifacts() {
    local artifact

    validate_comet_v1_1_artifact
    for artifact in \
        "${V3_FACTORY_WASM}" \
        "${V3_BACKSTOP_WASM}" \
        "${V3_EMITTER_WASM}" \
        "${V3_POOL_WASM}" \
        "${V3_TEST_SEP40_ORACLE_WASM}" \
        "${SEP41_TOKEN_WASM}" \
        "${PERMISSIONED_ACCESS_CONTROLLER_WASM}" \
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
    note "Validated V3, backfill, oracle, and official Comet V1.1 artifacts only."
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
    fixed_oracle="$(ensure_modeled_v3_oracle fixed)" || return
    custom_oracle="$(ensure_modeled_v3_oracle custom)" || return
    state_set "modeled_oracles_refreshed_at" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" || return
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
    if ! printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${fixed_pool}" 'index($pool) != null' >/dev/null
    then
        invoke_transaction "add-v3-only-modeled-reward" \
            "$(state_value v3_backstop)" add_reward \
            --to_add "${fixed_pool}" --to_remove null >/dev/null
        reward_zone="$(invoke_view "query-updated-v3-only-reward-zone" \
            "$(state_value v3_backstop)" reward_zone)"
    fi
    printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${fixed_pool}" 'index($pool) != null' >/dev/null ||
        die "modeled fixed v3 pool missing from reward zone: ${fixed_pool}"
    if printf '%s' "${reward_zone}" | jq -e \
        --arg pool "${custom_pool}" 'index($pool) != null' >/dev/null
    then
        die "plain-asset modeled custom v3 pool unexpectedly entered the reward zone"
    fi

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

v3_only_complete_core_deployment() {
    local operator depositor controller blnt usdc xlm
    local v3_emitter backfill blnt_usdc blnt_xlm
    local balance account admin v3_pool_hash v3_backstop v3_factory
    local deployed_v3_backstop backfill_balance recipient emitted reward_zone
    local total_swapped total_blnd_burned capacity
    local vesting_start vesting_end grant_start grant_end swap_deadline

    operator="$(state_value operator)"
    depositor="$(state_value depositor)"
    controller="$(state_value comet_controller)"
    blnt="$(state_value blnt_token)"
    usdc="$(state_value usdc_token)"
    xlm="$(state_value xlm_token)"
    v3_emitter="$(state_value v3_emitter)"
    backfill="$(state_value blnt_backfill)"
    blnt_usdc="$(state_value blnt_usdc_token)"
    blnt_xlm="$(state_value blnt_xlm_token)"

    balance="$(normalize_scalar "$(invoke_view \
        "query-v3-only-user-blnt-usdc" "${blnt_usdc}" balance \
        --id "${depositor}")")"
    if (( balance == 0 )); then
        invoke_transaction_as "join-v3-only-blnt-usdc" \
            "${DEPOSITOR_IDENTITY}" "${blnt_usdc}" join_pool \
            --pool_amount_out "${V3_BLNT_USDC_LP_TOTAL}" \
            --user "${depositor}" \
            --max_amounts_in "[\"${V3_BLNT_MINT}\",\"${V3_USDC_MINT}\"]" >/dev/null
        balance="$(normalize_scalar "$(invoke_view \
            "verify-v3-only-user-blnt-usdc" "${blnt_usdc}" balance \
            --id "${depositor}")")"
    fi
    assert_equal "${V3_BLNT_USDC_LP_TOTAL}" "${balance}" \
        "V3-only depositor BLNT:USDC shares"
    state_set "phase" "blnt-usdc-liquidity-seeded"

    balance="$(normalize_scalar "$(invoke_view \
        "query-v3-only-user-blnt-xlm" "${blnt_xlm}" balance \
        --id "${depositor}")")"
    if (( balance == 0 )); then
        invoke_transaction_as "join-v3-only-blnt-xlm" \
            "${DEPOSITOR_IDENTITY}" "${blnt_xlm}" join_pool \
            --pool_amount_out "${DUAL_MODELED_V3_LP_BALANCE}" \
            --user "${depositor}" \
            --max_amounts_in "[\"${V3_BLNT_MINT}\",\"${COMET_INITIAL_XLM}\"]" >/dev/null
        balance="$(normalize_scalar "$(invoke_view \
            "verify-v3-only-user-blnt-xlm" "${blnt_xlm}" balance \
            --id "${depositor}")")"
    fi
    assert_equal "${DUAL_MODELED_V3_LP_BALANCE}" "${balance}" \
        "V3-only depositor BLNT:XLM shares"
    state_set "phase" "comet-liquidity-seeded"

    account="$(read_horizon_account "${controller}")"
    if ! printf '%s' "${account}" | jq -e \
        --argjson threshold "${COMET_CONTROLLER_THRESHOLD}" '
          .thresholds.low_threshold == $threshold and
          .thresholds.med_threshold == $threshold and
          .thresholds.high_threshold == $threshold and
          (.signers | length == 1) and .signers[0].weight == 0
        ' >/dev/null
    then
        capture "lock-v3-only-comet-controller" \
            stellar_cli tx new set-options \
            --source-account "${COMET_CONTROLLER_IDENTITY}" \
            --low-threshold "${COMET_CONTROLLER_THRESHOLD}" \
            --med-threshold "${COMET_CONTROLLER_THRESHOLD}" \
            --high-threshold "${COMET_CONTROLLER_THRESHOLD}" \
            --master-weight 0 \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" >/dev/null
    fi
    verify_locked_comet_controller "v3-only-blnt-usdc" \
        "${blnt_usdc}" "${controller}" "${blnt}" "${usdc}"
    verify_locked_comet_controller "v3-only-blnt-xlm" \
        "${blnt_xlm}" "${controller}" "${blnt}" "${xlm}"
    state_set "phase" "comet-controller-locked"

    admin="$(normalize_scalar "$(invoke_view \
        "query-v3-only-blnt-admin-before-handoff" "${blnt}" admin)")"
    if [[ "${admin}" == "${operator}" ]]; then
        invoke_transaction "set-v3-only-emitter-as-blnt-admin" \
            "${blnt}" set_admin --new_admin "${v3_emitter}" >/dev/null
    else
        assert_equal "${v3_emitter}" "${admin}" "BLNT administrator"
    fi
    state_set "phase" "blnt-admin-handed-off"

    v3_pool_hash="$(upload_wasm "upload-v3-only-pool" "${V3_POOL_WASM}")"
    state_set "v3_pool_hash" "${v3_pool_hash}"
    state_set "v3_pool_wasm_hash" "${v3_pool_hash}"
    v3_backstop="$(state_optional v3_backstop)"
    if [[ -z "${v3_backstop}" ]]; then
        v3_backstop="$(predict_contract_id "${CANDIDATE_SALT}")"
        state_set "v3_backstop" "${v3_backstop}"
        state_set "candidate_backstop" "${v3_backstop}"
    fi
    v3_factory="$(state_optional v3_factory)"
    if [[ -z "${v3_factory}" ]]; then
        v3_factory="$(deploy_wasm "deploy-v3-only-factory" \
            "${V3_FACTORY_WASM}" "" \
            --pool_init_meta \
            "{\"backstop\":\"${v3_backstop}\",\"blnt_id\":\"${blnt}\",\"pool_hash\":\"${v3_pool_hash}\"}")"
        state_set "v3_factory" "${v3_factory}"
        state_set "v3_pool_factory" "${v3_factory}"
    fi
    state_set "phase" "v3-factory-deployed"

    if [[ "$(state_optional v3_emitter_initialized)" != "true" ]]; then
        invoke_transaction "initialize-v3-only-emitter" "${v3_emitter}" initialize \
            --blnd_token "${blnt}" \
            --backstop "${v3_backstop}" \
            --backstop_token "${blnt_xlm}" >/dev/null
        state_set "v3_emitter_initialized" "true"
    fi
    if [[ "$(state_optional v3_backstop_deployed)" != "true" ]]; then
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
        assert_equal "${v3_backstop}" "${deployed_v3_backstop}" \
            "v3 backstop address"
        state_set "v3_backstop_deployed" "true"
    fi
    state_set "phase" "v3-core-contracts-deployed"

    backfill_balance="$(normalize_scalar "$(invoke_view \
        "query-v3-only-backfill-balance" "${blnt}" balance --id "${backfill}")")"
    if [[ "$(state_optional v3_backfill_drop_executed)" != "true" ]]; then
        if [[ "${backfill_balance}" == "${BLNT_BACKFILL_DROP}" ]]; then
            state_set "v3_backfill_drop_executed" "true"
        else
            assert_equal "0" "${backfill_balance}" \
                "pre-drop backfill BLNT custody"
            invoke_transaction "activate-v3-only" "${v3_backstop}" distribute >/dev/null
            invoke_transaction "execute-v3-only-backfill-drop" \
                "${v3_backstop}" drop >/dev/null
            backfill_balance="$(normalize_scalar "$(invoke_view \
                "verify-v3-only-backfill-balance" "${blnt}" balance \
                --id "${backfill}")")"
            state_set "v3_backfill_drop_executed" "true"
        fi
    fi
    assert_equal "${BLNT_BACKFILL_DROP}" "${backfill_balance}" \
        "backfill BLNT custody"
    total_swapped="$(normalize_scalar "$(invoke_view \
        "query-v3-only-total-swapped" "${backfill}" get_total_swapped)")"
    total_blnd_burned="$(normalize_scalar "$(invoke_view \
        "query-v3-only-total-blnd-burned" "${backfill}" get_total_blnd_burned)")"
    capacity="$(normalize_scalar "$(invoke_view \
        "query-v3-only-swap-capacity" "${backfill}" get_remaining_swap_capacity)")"
    assert_equal "0" "${total_swapped}" "initial converted BLNT output"
    assert_equal "0" "${total_blnd_burned}" "initial converted BLND burned"
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
    state_set "blnd_swap_input_cap" "${BLND_SWAP_INPUT_CAP}"
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

v3_only_resume_core_deployment() {
    local phase

    validate_tools
    require_network
    load_state
    v3_only_validate_artifacts
    phase="$(state_optional phase)"
    case "${phase}" in
        core-prerequisites-ready|blnt-usdc-liquidity-seeded|comet-liquidity-seeded|comet-controller-locked|blnt-admin-handed-off|v3-factory-deployed|v3-core-contracts-deployed|core-deployed)
            ;;
        verified)
            die "core deployment is already verified"
            ;;
        *)
            die "core prerequisites are incomplete; refusing unsafe resume from phase: ${phase:-unset}"
            ;;
    esac
    v3_only_complete_core_deployment
}

v3_only_deploy() {
    local skip_build="${1:-false}"
    local operator depositor controller issuer blnt usdc xlm xlm_balance
    local v3_emitter backfill blnt_usdc blnt_xlm
    local v3_pool_hash v3_backstop v3_factory deployed_v3_backstop
    local backfill_balance recipient admin emitted reward_zone
    local total_swapped total_blnd_burned capacity
    local vesting_start vesting_end grant_start grant_end swap_deadline

    validate_tools
    validate_backfill_manifest
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
    if [[ "${BLNT_GRANT_LIST}" == "[]" ]]; then
        BLNT_GRANT_LIST="[[\"${V3_USER_WALLET}\",\"${BLNT_GRANT_CAP}\"]]"
    fi
    load_claim_lists
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
    state_set "comet_v1_1_wasm_sha256" "$(sha256_file "${COMET_WASM}")"
    state_set "comet_v1_1_source_commit" \
        "$(git -C "${ROOT_DIR}/comet-contracts-v1" rev-parse HEAD)"

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

    # BLNT must reach the configured test wallet before its administrator is
    # permanently handed to the emitter. USDC is funded here alongside it so
    # the core funding is atomic from the runner's perspective.
    v3_only_fund_user_core_assets "${blnt}" "${usdc}" "${issuer}"

    initialize_comet "v3-only-blnt-usdc" "${blnt_usdc}" "${controller}" \
        "${blnt}" "${usdc}" "${COMET_INITIAL_BLND}" "${COMET_INITIAL_PAIR_TOKEN}"
    initialize_comet "v3-only-blnt-xlm" "${blnt_xlm}" "${controller}" \
        "${blnt}" "${xlm}" "${COMET_INITIAL_BLND}" "${xlm_balance}"
    state_set "phase" "core-prerequisites-ready"
    v3_only_complete_core_deployment
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
    local handler="$1" pool_hash authorized

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
    "${handler}"
    if [[ "${handler}" == "command_deploy_reserve_loss_pool" ]]; then
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

v3_only_permissioned() {
    local handler="$1" skip_build="${2:-false}"

    validate_tools
    require_network
    load_state
    if [[ "${handler}" == "command_deploy_permissioned_pool" ]]; then
        if [[ "${skip_build}" == "true" ]]; then
            v3_only_validate_artifacts
        else
            v3_only_validate
        fi
    else
        require_permissioned_pool_support
    fi
    "${handler}"
    v3_only_update_reward_zone
    v3_only_write_registry
}

v3_only_sep41() {
    local handler="$1" skip_build="${2:-false}"

    validate_tools
    require_network
    load_state
    if [[ "${handler}" == "command_deploy_sep41_pool" ]]; then
        if [[ "${skip_build}" == "true" ]]; then
            v3_only_validate_artifacts
        else
            v3_only_validate
        fi
    else
        require_sep41_pool_support
    fi
    "${handler}"
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
  Permissioned pool: $(state_optional permissioned_pool)
  SEP-41 Assets pool: $(state_optional sep41_pool)
  configured grant user: $(state_value configured_user)
  grant amount: $(state_value blnt_grant_total)
  funded user: $(state_optional funding_wallet)
  amount per funded asset: $(state_optional wallet_asset_funding)
  funding completed at: $(state_optional wallet_funded_at)
  state: ${STATE_FILE}
EOF
}

v3_only_run() {
    v3_only_validate
    command_start
    v3_only_deploy true
    v3_only_reserve_loss command_deploy_reserve_loss_pool
    v3_only_permissioned command_deploy_permissioned_pool true
    v3_only_sep41 command_deploy_sep41_pool true
    v3_only_fund_user_assets
}

main_v3_only() {
    case "${1:-}" in
        plan) v3_only_plan ;;
        validate) v3_only_validate ;;
        start) command_start ;;
        deploy) v3_only_deploy ;;
        resume-core) v3_only_resume_core_deployment ;;
        deploy-modeled-pools) v3_only_resume_modeled_pools ;;
        deploy-reserve-loss-pool) v3_only_reserve_loss command_deploy_reserve_loss_pool ;;
        verify-reserve-loss-pool) v3_only_reserve_loss command_verify_reserve_loss_pool ;;
        deploy-permissioned-pool) v3_only_permissioned command_deploy_permissioned_pool ;;
        verify-permissioned-pool) v3_only_permissioned command_verify_permissioned_pool ;;
        deploy-sep41-pool) v3_only_sep41 command_deploy_sep41_pool ;;
        verify-sep41-pool) v3_only_sep41 command_verify_sep41_pool ;;
        run) v3_only_run ;;
        status) v3_only_status ;;
        fund-user) v3_only_fund_user_assets ;;
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
