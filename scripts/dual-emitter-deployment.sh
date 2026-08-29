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
V3_EMITTER_WASM="${ROOT_DIR}/blend-contracts-v3/target/wasm32v1-none/optimized/emitter.wasm"
BLNT_MINT=$((2000 * SCALAR_7))
USDC_MINT=$((2 * COMET_INITIAL_PAIR_TOKEN))
CONVERSION_TEST_AMOUNT=$((10 * SCALAR_7))

usage_dual() {
    cat <<'EOF'
Usage: scripts/dual-emitter-deployment.sh COMMAND

Commands:
  plan      Describe the independent BLND/BLNT deployment.
  validate  Verify legacy artifacts and build v3 plus Comet V2 artifacts.
  start     Start local Protocol-27 Quickstart or verify public testnet RPC.
  deploy    Deploy and verify the independent legacy and v3 lanes.
  run       Validate, start, and deploy.
  status    Query the recorded token, emitter, backstop, and deadline bindings.
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
  2. Build the v3 emitter/backstop/factory/pool and workspace Comet V2.
  3. Deploy legacy BLND, BLNT, USDC, and ${XLM_ASSET_MODE} XLM assets.
  4. Deploy BLND:USDC, BLNT:USDC, and BLNT:XLM Comet V2 pools.
  5. Lock the shared Comet controller at 100/100/100 with signer weight zero.
  6. Leave the legacy emitter pointed at the deployed v2 backstop.
  7. Point the separate v3 emitter at the v3 backstop with BLNT:USDC designated.
  8. Activate v3 immediately, execute its empty initial drop, and smoke-test
     one-way BLND-to-BLNT conversion.
  9. Verify separate administrators, recipients, emissions, Comets, and the
     immutable 60-day conversion deadline.

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
        "${COMET_WASM}"
    do
        [[ -s "${artifact}" ]] || die "missing or empty WASM artifact: ${artifact}"
    done
}

dual_validate() {
    validate_tools
    validate_source_provenance
    build_artifacts
    dual_validate_artifacts
    note "Validated exact legacy artifacts, v3 contracts, and workspace Comet V2."
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

dual_deploy() {
    local skip_build="${1:-false}"
    local operator depositor controller issuer
    local legacy_emitter v3_emitter legacy_blnd blnt usdc xlm
    local legacy_blnd_usdc blnt_usdc blnt_xlm xlm_balance
    local v2_pool_hash v2_backstop v2_factory deployed_v2_backstop
    local v3_pool_hash v3_backstop v3_factory deployed_v3_backstop
    local legacy_emitted v3_emitted swapped deadline now
    local legacy_admin blnt_admin legacy_recipient v3_recipient

    validate_tools
    validate_source_provenance
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
    initialize_state "${operator}"
    state_set "topology" "dual-emitter-blnd-blnt"
    state_set "legacy_emitter_wasm_sha256" "${EXPECTED_EMITTER_SHA256}"
    state_set "v3_emitter_wasm_sha256" "$(sha256_file "${V3_EMITTER_WASM}")"
    state_set "v3_backstop_wasm_sha256" "$(sha256_file "${V3_BACKSTOP_WASM}")"
    state_set "comet_v2_wasm_sha256" "$(sha256_file "${COMET_WASM}")"
    state_set "comet_v2_source_commit" \
        "$(git -C "${ROOT_DIR}/comet-contracts-v1" rev-parse HEAD)"

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
    create_trustline "trust-controller-blnd" "${COMET_CONTROLLER_IDENTITY}" "BLND:${issuer}"
    create_trustline "trust-controller-blnt" "${COMET_CONTROLLER_IDENTITY}" "BLNT:${issuer}"
    create_trustline "trust-controller-usdc" "${COMET_CONTROLLER_IDENTITY}" "USDC:${issuer}"
    if [[ "${XLM_ASSET_MODE}" == "issued" ]]; then
        create_trustline "trust-controller-xlm" "${COMET_CONTROLLER_IDENTITY}" "XLM:${issuer}"
    fi

    legacy_emitter="$(deploy_wasm "deploy-legacy-emitter" "${EMITTER_WASM}" "")"
    v3_emitter="$(deploy_wasm \
        "deploy-v3-emitter" "${V3_EMITTER_WASM}" "" \
        --legacy_blnd_token "${legacy_blnd}" \
        --initializer "${operator}")"
    state_set "legacy_emitter" "${legacy_emitter}"
    state_set "v3_emitter" "${v3_emitter}"

    legacy_blnd_usdc="$(deploy_wasm "deploy-comet-blnd-usdc" "${COMET_WASM}" "")"
    blnt_usdc="$(deploy_wasm "deploy-comet-blnt-usdc" "${COMET_WASM}" "")"
    blnt_xlm="$(deploy_wasm "deploy-comet-blnt-xlm" "${COMET_WASM}" "")"
    state_set "legacy_blnd_usdc_token" "${legacy_blnd_usdc}"
    state_set "blnt_usdc_token" "${blnt_usdc}"
    state_set "blnt_xlm_token" "${blnt_xlm}"

    invoke_transaction "mint-controller-blnd" "${legacy_blnd}" mint \
        --to "${controller}" --amount "${COMET_INITIAL_BLND}" >/dev/null
    invoke_transaction "mint-depositor-blnd" "${legacy_blnd}" mint \
        --to "${depositor}" --amount "${CONVERSION_TEST_AMOUNT}" >/dev/null
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
        --backstop_token "${blnt_usdc}" >/dev/null
    deployed_v3_backstop="$(deploy_wasm \
        "deploy-v3-backstop" "${V3_BACKSTOP_WASM}" "${CANDIDATE_SALT}" \
        --blnt_usdc_token "${blnt_usdc}" \
        --blnt_xlm_token "${blnt_xlm}" \
        --emitter "${v3_emitter}" \
        --blnt_token "${blnt}" \
        --usdc_token "${usdc}" \
        --xlm_token "${xlm}" \
        --pool_factory "${v3_factory}" \
        --drop_list "[]")"
    assert_equal "${v3_backstop}" "${deployed_v3_backstop}" "v3 backstop address"

    invoke_transaction "activate-v3-directly" "${v3_backstop}" distribute >/dev/null
    invoke_transaction "execute-empty-v3-drop" "${v3_backstop}" drop >/dev/null

    invoke_transaction_as "convert-test-blnd-to-blnt" \
        "${DEPOSITOR_IDENTITY}" "${v3_emitter}" swap_blnd_for_blnt \
        --from "${depositor}" --to "${depositor}" \
        --amount "${CONVERSION_TEST_AMOUNT}" >/dev/null
    swapped="$(normalize_scalar "$(invoke_view \
        "query-total-converted" "${v3_emitter}" get_total_swapped)")"
    assert_equal "${CONVERSION_TEST_AMOUNT}" "${swapped}" "conversion total"

    sleep "${EMISSION_START_DELAY_SECONDS}"
    legacy_emitted="$(normalize_scalar "$(invoke_transaction \
        "emit-legacy-blnd" "${legacy_emitter}" distribute)")"
    v3_emitted="$(normalize_scalar "$(invoke_transaction \
        "emit-v3-blnt" "${v3_emitter}" distribute)")"
    [[ "${legacy_emitted}" =~ ^[1-9][0-9]*$ ]] || die "legacy emitter minted no BLND"
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

    deadline="$(normalize_scalar "$(invoke_view \
        "query-conversion-deadline" "${v3_emitter}" get_swap_deadline)")"
    now="$(date +%s)"
    [[ "${deadline}" =~ ^[0-9]+$ ]] || die "invalid conversion deadline"
    (( deadline > now )) || die "conversion deadline is not in the future"
    state_set "swap_deadline" "${deadline}"
    state_set "legacy_emitted" "${legacy_emitted}"
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
    local legacy_emitter v3_emitter legacy_recipient v3_recipient deadline queued

    validate_tools
    require_network
    load_state
    legacy_emitter="$(state_value legacy_emitter)"
    v3_emitter="$(state_value v3_emitter)"
    legacy_recipient="$(normalize_scalar "$(invoke_view \
        "status-legacy-recipient" "${legacy_emitter}" get_backstop)")"
    v3_recipient="$(normalize_scalar "$(invoke_view \
        "status-v3-recipient" "${v3_emitter}" get_backstop)")"
    deadline="$(normalize_scalar "$(invoke_view \
        "status-swap-deadline" "${v3_emitter}" get_swap_deadline)")"
    queued="$(invoke_view "status-v3-queue" "${v3_emitter}" get_queued_swap)"
    cat <<EOF
Dual-emitter ${NETWORK_LABEL} status
  phase: $(state_value phase)
  legacy BLND emitter: ${legacy_emitter}
  legacy recipient: ${legacy_recipient}
  v3 BLNT emitter: ${v3_emitter}
  v3 recipient: ${v3_recipient}
  v3 queue: ${queued}
  conversion deadline: $(format_timestamp "${deadline}") (${deadline})
  Comet V2 commit: $(state_value comet_v2_source_commit)
  Comet V2 WASM: $(state_value comet_v2_wasm_sha256)
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

main_dual "$@"
