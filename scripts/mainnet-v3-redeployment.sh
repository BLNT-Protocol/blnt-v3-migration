#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLAN_FILE="${BLEND_MAINNET_V3_REDEPLOYMENT_PLAN:-${ROOT_DIR}/deployments/mainnet-v3-redeployment-plan.json}"
RPC_URL="${BLEND_MAINNET_RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}"
HORIZON_URL="${BLEND_MAINNET_HORIZON_URL:-https://horizon.stellar.org}"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
ARTIFACT_DIR="${ROOT_DIR}/.mainnet-v3-redeployment/release-artifacts"

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

plan_value() {
    jq -er "$1" "${PLAN_FILE}"
}

absolute_plan_path() {
    printf '%s/%s\n' "${ROOT_DIR}" "$(plan_value "$1")"
}

sha256_file() {
    local value
    value="$(shasum -a 256 "$1")"
    printf '%s\n' "${value%% *}"
}

stellar_cli() {
    stellar --config-dir "$(absolute_plan_path '.isolation.config_dir')" "$@"
}

validate_manifest() {
    jq -e '
        .schema_version == 1 and
        (.status == "prepared-not-deployed" or .status == "deployed") and
        .network.passphrase == "Public Global Stellar Network ; September 2015" and
        (.deployer | test("^G[A-Z2-7]{55}$")) and
        .asset.code == "BLNT" and
        .asset.decimals == 7 and
        .asset.home_domain == "blnt.trade" and
        .asset.premint_recipient == .deployer and
        .asset.premint_amount_raw == "20000000000000" and
        .asset.flags == {
            auth_required: false,
            auth_revocable: false,
            auth_clawback_enabled: false
        } and
        .asset.permanently_lock_issuer_after_admin_handoff == true and
        .asset.final_sac_admin == .contracts.emitter.contract_id and
        .comet_v1_1.blnt_usdc.tokens[0] == .asset.contract_id and
        .comet_v1_1.blnt_xlm.tokens[0] == .asset.contract_id and
        .comet_v1_1.blnt_usdc.weights == [8000000, 2000000] and
        .comet_v1_1.blnt_xlm.weights == [8000000, 2000000] and
        .comet_v1_1.blnt_usdc.swap_fee == 30000 and
        .comet_v1_1.blnt_xlm.swap_fee == 30000 and
        ([
            .comet_v1_1.blnt_usdc.salt,
            .comet_v1_1.blnt_xlm.salt,
            .contracts.emitter.salt,
            .contracts.backfill.salt,
            .contracts.pool_factory.salt,
            .contracts.backstop.salt
        ] | all(test("^[0-9a-f]{64}$"))) and
        ([
            .comet_v1_1.blnt_usdc.salt,
            .comet_v1_1.blnt_xlm.salt,
            .contracts.emitter.salt,
            .contracts.backfill.salt,
            .contracts.pool_factory.salt,
            .contracts.backstop.salt
        ] | unique | length == 6) and
        ([
            .asset.contract_id,
            .comet_v1_1.blnt_usdc.contract_id,
            .comet_v1_1.blnt_xlm.contract_id,
            .contracts.emitter.contract_id,
            .contracts.backfill.contract_id,
            .contracts.pool_factory.contract_id,
            .contracts.backstop.contract_id
        ] | all(test("^C[A-Z2-7]{55}$"))) and
        ([
            .asset.contract_id,
            .comet_v1_1.blnt_usdc.contract_id,
            .comet_v1_1.blnt_xlm.contract_id,
            .contracts.emitter.contract_id,
            .contracts.backfill.contract_id,
            .contracts.pool_factory.contract_id,
            .contracts.backstop.contract_id
        ] | unique | length == 7) and
        ([
            .contracts.emitter.sha256,
            .contracts.backfill.sha256,
            .contracts.pool_factory.sha256,
            .contracts.backstop.sha256,
            .contracts.pool_wasm.sha256,
            .comet_v1_1.artifact_sha256,
            .allocations.private_grant_sha256,
            .allocations.backfill_manifest_sha256
        ] | all(test("^[0-9a-f]{64}$"))) and
        ((
            (.allocations.backfill_claims_raw | tonumber) +
            (.allocations.contributor_grants_raw | tonumber) +
            (.allocations.conversion_reserve_raw | tonumber)
        ) == (.allocations.initial_drop_raw | tonumber))
    ' "${PLAN_FILE}" >/dev/null || die "invalid redeployment plan: ${PLAN_FILE}"

    jq -e --slurpfile releases "${ROOT_DIR}/deployments/mainnet-v3-next-artifacts.json" '
        .contracts.source.v3_commit == $releases[0].source.v3_commit and
        .contracts.source.backfill_commit == $releases[0].source.backfill_commit and
        .contracts.emitter.sha256 == ($releases[0].artifacts[] | select(.name == "emitter").sha256) and
        .contracts.backstop.sha256 == ($releases[0].artifacts[] | select(.name == "backstop").sha256) and
        .contracts.pool_factory.sha256 == ($releases[0].artifacts[] | select(.name == "pool-factory").sha256) and
        .contracts.pool_wasm.sha256 == ($releases[0].artifacts[] | select(.name == "pool").sha256) and
        .contracts.backfill.sha256 == ($releases[0].artifacts[] | select(.name == "backfill").sha256)
    ' "${PLAN_FILE}" >/dev/null || die "redeployment plan does not match pinned releases"
}

validate_local_keys() {
    local issuer controller
    issuer="$(stellar_cli keys public-key "$(plan_value '.asset.issuer_identity')")"
    controller="$(stellar_cli keys public-key "$(plan_value '.comet_v1_1.controller_identity')")"
    [[ "${issuer}" == "$(plan_value '.asset.issuer')" ]] || die "issuer identity mismatch"
    [[ "${controller}" == "$(plan_value '.comet_v1_1.controller')" ]] ||
        die "Comet controller identity mismatch"
}

validate_source_pins() {
    [[ "$(git -C "${ROOT_DIR}/blnt-contracts-v3" rev-parse HEAD)" == \
        "$(plan_value '.contracts.source.v3_commit')" ]] || die "V3 source commit mismatch"
    [[ "$(git -C "${ROOT_DIR}/blnt-backfill-contract" rev-parse HEAD)" == \
        "$(plan_value '.contracts.source.backfill_commit')" ]] ||
        die "backfill source commit mismatch"
    [[ "$(git -C "${ROOT_DIR}/comet-contracts-v1" rev-parse HEAD)" == \
        "$(plan_value '.comet_v1_1.source_commit')" ]] || die "Comet source commit mismatch"
    [[ "$(git -C "${ROOT_DIR}/comet-contracts-v1" describe --tags --exact-match HEAD)" == v1.1.0 ]] ||
        die "Comet source is not tag v1.1.0"
}

validate_private_inputs() {
    local grant_file="${ROOT_DIR}/.mainnet-v3/blnt-grants.json"
    local backfill_manifest="${ROOT_DIR}/blnt-backfill-contract/allocations/comet_cpal_backfill.json"
    [[ -f "${grant_file}" ]] || die "private grant file is missing"
    [[ "$(stat -f '%Lp' "${grant_file}")" == 600 ]] ||
        die "private grant file must have mode 0600"
    [[ "$(sha256_file "${grant_file}")" == "$(plan_value '.allocations.private_grant_sha256')" ]] ||
        die "private grant file SHA-256 mismatch"
    [[ "$(sha256_file "${backfill_manifest}")" == \
        "$(plan_value '.allocations.backfill_manifest_sha256')" ]] ||
        die "backfill manifest SHA-256 mismatch"
}

fetch_release_artifacts() {
    BLEND_MAINNET_V3_ARTIFACT_DIR="${ARTIFACT_DIR}" \
        bash "${ROOT_DIR}/scripts/fetch-mainnet-v3-release-artifacts.sh"
}

validate_artifacts() {
    local comet="${ROOT_DIR}/blnt-contracts-v3/comet.wasm"
    [[ "$(sha256_file "${comet}")" == "$(plan_value '.comet_v1_1.artifact_sha256')" ]] ||
        die "Comet V1.1 artifact mismatch"
    [[ "$(sha256_file "${ARTIFACT_DIR}/emitter.wasm")" == \
        "$(plan_value '.contracts.emitter.sha256')" ]] || die "emitter artifact mismatch"
    [[ "$(sha256_file "${ARTIFACT_DIR}/backstop.wasm")" == \
        "$(plan_value '.contracts.backstop.sha256')" ]] || die "backstop artifact mismatch"
    [[ "$(sha256_file "${ARTIFACT_DIR}/pool_factory.wasm")" == \
        "$(plan_value '.contracts.pool_factory.sha256')" ]] || die "pool-factory artifact mismatch"
    [[ "$(sha256_file "${ARTIFACT_DIR}/pool.wasm")" == \
        "$(plan_value '.contracts.pool_wasm.sha256')" ]] || die "pool artifact mismatch"
    [[ "$(sha256_file "${ARTIFACT_DIR}/blnt_backfill_contract.wasm")" == \
        "$(plan_value '.contracts.backfill.sha256')" ]] || die "backfill artifact mismatch"
}

derive_wasm_contract() {
    stellar_cli contract id wasm \
        --salt "$1" \
        --source-account "$(plan_value '.deployer')" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}"
}

validate_predictions() {
    local issuer derived
    issuer="$(plan_value '.asset.issuer')"
    derived="$(stellar_cli contract id asset --asset "BLNT:${issuer}" \
        --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}")"
    [[ "${derived}" == "$(plan_value '.asset.contract_id')" ]] || die "BLNT SAC prediction mismatch"

    while IFS=$'\t' read -r salt expected; do
        derived="$(derive_wasm_contract "${salt}")"
        [[ "${derived}" == "${expected}" ]] || die "contract prediction mismatch for ${expected}"
    done < <(jq -r '
        [
            [.comet_v1_1.blnt_usdc.salt, .comet_v1_1.blnt_usdc.contract_id],
            [.comet_v1_1.blnt_xlm.salt, .comet_v1_1.blnt_xlm.contract_id],
            [.contracts.emitter.salt, .contracts.emitter.contract_id],
            [.contracts.backfill.salt, .contracts.backfill.contract_id],
            [.contracts.pool_factory.salt, .contracts.pool_factory.contract_id],
            [.contracts.backstop.salt, .contracts.backstop.contract_id]
        ][] | @tsv
    ' "${PLAN_FILE}")
}

assert_account_unused() {
    local account="$1" status
    status="$(curl --silent --show-error --max-time 15 --output /dev/null \
        --write-out '%{http_code}' "${HORIZON_URL}/accounts/${account}")"
    [[ "${status}" == 404 ]] || die "replacement issuer account is not unused (HTTP ${status})"
}

assert_contract_unused() {
    local contract="$1" temporary stderr_output status
    temporary="$(mktemp -d)"
    if stellar_cli contract fetch --id "${contract}" --out-file "${temporary}/contract.wasm" \
        --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
        2>"${temporary}/fetch.err"
    then
        rm -r -- "${temporary}"
        die "predicted contract is already deployed: ${contract}"
    else
        status=$?
    fi
    stderr_output="$(<"${temporary}/fetch.err")"
    rm -r -- "${temporary}"
    [[ ${status} -ne 0 && ("${stderr_output}" == *"Contract not found"* || \
        "${stderr_output}" == *"not found in ledger"*) ]] ||
        die "unable to establish that ${contract} is unused: ${stderr_output}"
}

validate_wallet_funding() {
    local account xlm usdc issuer
    account="$(curl --fail --silent --show-error --max-time 15 \
        "${HORIZON_URL}/accounts/$(plan_value '.deployer')")"
    issuer="GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
    xlm="$(printf '%s' "${account}" | jq -er \
        '[.balances[] | select(.asset_type == "native")][0].balance')"
    usdc="$(printf '%s' "${account}" | jq -er --arg issuer "${issuer}" \
        '[.balances[] | select(.asset_code == "USDC" and .asset_issuer == $issuer)][0].balance')"
    jq -en --arg actual "${xlm}" --arg minimum "$(plan_value '.wallet_requirements.minimum_xlm')" \
        '($actual | tonumber) >= ($minimum | tonumber)' >/dev/null ||
        die "wallet has ${xlm} XLM; minimum is $(plan_value '.wallet_requirements.minimum_xlm')"
    jq -en --arg actual "${usdc}" --arg minimum "$(plan_value '.wallet_requirements.minimum_usdc')" \
        '($actual | tonumber) >= ($minimum | tonumber)' >/dev/null ||
        die "wallet has ${usdc} USDC; minimum is $(plan_value '.wallet_requirements.minimum_usdc')"
    note "Wallet funding verified: ${xlm} XLM and ${usdc} USDC."
}

validate_command() {
    for command in curl git jq shasum stellar; do
        require_command "${command}"
    done
    validate_manifest
    validate_local_keys
    validate_source_pins
    validate_private_inputs
    fetch_release_artifacts
    validate_artifacts
    validate_predictions
    note "Validated isolated state, source pins, release artifacts, private-input hashes, and predictions."
}

preflight_command() {
    local issuer
    validate_command
    [[ "$(plan_value '.status')" == "prepared-not-deployed" ]] ||
        die "replacement deployment is already recorded as deployed"
    issuer="$(plan_value '.asset.issuer')"
    assert_account_unused "${issuer}"
    while IFS= read -r contract; do
        assert_contract_unused "${contract}"
    done < <(jq -r '
        .asset.contract_id,
        .comet_v1_1.blnt_usdc.contract_id,
        .comet_v1_1.blnt_xlm.contract_id,
        .contracts.emitter.contract_id,
        .contracts.backfill.contract_id,
        .contracts.pool_factory.contract_id,
        .contracts.backstop.contract_id
    ' "${PLAN_FILE}")
    validate_wallet_funding
    note "Read-only replacement deployment preflight passed. No transaction was submitted."
}

export_common_environment() {
    BLEND_MAINNET_RPC_URL="${RPC_URL}"
    BLEND_MAINNET_WALLET="$(plan_value '.deployer')"
    BLEND_MAINNET_BLNT_ISSUER="$(plan_value '.asset.issuer')"
    BLEND_MAINNET_BLNT_TOKEN="$(plan_value '.asset.contract_id')"
    BLEND_MAINNET_COMET_CONFIG_DIR="$(absolute_plan_path '.isolation.config_dir')"
    BLEND_MAINNET_COMET_WORK_DIR="${ROOT_DIR}/.mainnet-v3-redeployment/comet"
    BLEND_MAINNET_COMET_STATE_FILE="$(absolute_plan_path '.isolation.comet_state')"
    BLEND_MAINNET_COMET_REGISTRY_FILE="$(absolute_plan_path '.isolation.comet_registry')"
    BLEND_MAINNET_COMET_CONTROLLER_IDENTITY="$(plan_value '.comet_v1_1.controller_identity')"
    BLEND_MAINNET_COMET_BLNT_USDC_SALT="$(plan_value '.comet_v1_1.blnt_usdc.salt')"
    BLEND_MAINNET_COMET_BLNT_XLM_SALT="$(plan_value '.comet_v1_1.blnt_xlm.salt')"
    BLEND_MAINNET_COMET_BLNT_USDC="$(plan_value '.comet_v1_1.blnt_usdc.contract_id')"
    BLEND_MAINNET_COMET_BLNT_XLM="$(plan_value '.comet_v1_1.blnt_xlm.contract_id')"
    export BLEND_MAINNET_RPC_URL BLEND_MAINNET_WALLET
    export BLEND_MAINNET_BLNT_ISSUER BLEND_MAINNET_BLNT_TOKEN
    export BLEND_MAINNET_COMET_CONFIG_DIR BLEND_MAINNET_COMET_WORK_DIR
    export BLEND_MAINNET_COMET_STATE_FILE BLEND_MAINNET_COMET_REGISTRY_FILE
    export BLEND_MAINNET_COMET_CONTROLLER_IDENTITY
    export BLEND_MAINNET_COMET_BLNT_USDC_SALT BLEND_MAINNET_COMET_BLNT_XLM_SALT
    export BLEND_MAINNET_COMET_BLNT_USDC BLEND_MAINNET_COMET_BLNT_XLM
}

export_core_environment() {
    export_common_environment
    BLEND_MAINNET_V3_WORK_DIR="${ROOT_DIR}/.mainnet-v3-redeployment/core"
    BLEND_MAINNET_V3_CONFIG_DIR="$(absolute_plan_path '.isolation.config_dir')"
    BLEND_MAINNET_V3_STATE_FILE="$(absolute_plan_path '.isolation.core_state')"
    BLEND_MAINNET_V3_REGISTRY_FILE="$(absolute_plan_path '.isolation.core_registry')"
    BLEND_MAINNET_V3_ISSUER_IDENTITY="$(plan_value '.asset.issuer_identity')"
    BLEND_MAINNET_V3_EMITTER_WASM="${ARTIFACT_DIR}/emitter.wasm"
    BLEND_MAINNET_V3_BACKSTOP_WASM="${ARTIFACT_DIR}/backstop.wasm"
    BLEND_MAINNET_V3_FACTORY_WASM="${ARTIFACT_DIR}/pool_factory.wasm"
    BLEND_MAINNET_V3_POOL_WASM="${ARTIFACT_DIR}/pool.wasm"
    BLEND_MAINNET_V3_BACKFILL_WASM="${ARTIFACT_DIR}/blnt_backfill_contract.wasm"
    BLEND_MAINNET_V3_SOURCE_COMMIT="$(plan_value '.contracts.source.v3_commit')"
    BLEND_MAINNET_BACKFILL_SOURCE_COMMIT="$(plan_value '.contracts.source.backfill_commit')"
    BLEND_MAINNET_V3_EMITTER_SHA256="$(plan_value '.contracts.emitter.sha256')"
    BLEND_MAINNET_V3_BACKSTOP_SHA256="$(plan_value '.contracts.backstop.sha256')"
    BLEND_MAINNET_V3_FACTORY_SHA256="$(plan_value '.contracts.pool_factory.sha256')"
    BLEND_MAINNET_V3_POOL_SHA256="$(plan_value '.contracts.pool_wasm.sha256')"
    BLEND_MAINNET_V3_BACKFILL_SHA256="$(plan_value '.contracts.backfill.sha256')"
    BLEND_MAINNET_V3_EMITTER_SALT="$(plan_value '.contracts.emitter.salt')"
    BLEND_MAINNET_V3_BACKFILL_SALT="$(plan_value '.contracts.backfill.salt')"
    BLEND_MAINNET_V3_FACTORY_SALT="$(plan_value '.contracts.pool_factory.salt')"
    BLEND_MAINNET_V3_BACKSTOP_SALT="$(plan_value '.contracts.backstop.salt')"
    BLEND_MAINNET_V3_EXPECTED_EMITTER="$(plan_value '.contracts.emitter.contract_id')"
    BLEND_MAINNET_V3_EXPECTED_BACKFILL="$(plan_value '.contracts.backfill.contract_id')"
    BLEND_MAINNET_V3_EXPECTED_FACTORY="$(plan_value '.contracts.pool_factory.contract_id')"
    BLEND_MAINNET_V3_EXPECTED_BACKSTOP="$(plan_value '.contracts.backstop.contract_id')"
    BLEND_MAINNET_V3_LOCK_BLNT_ISSUER=true
    BLEND_MAINNET_V3_SKIP_BUILD=true
    export BLEND_MAINNET_V3_WORK_DIR BLEND_MAINNET_V3_CONFIG_DIR
    export BLEND_MAINNET_V3_STATE_FILE BLEND_MAINNET_V3_REGISTRY_FILE
    export BLEND_MAINNET_V3_ISSUER_IDENTITY
    export BLEND_MAINNET_V3_EMITTER_WASM BLEND_MAINNET_V3_BACKSTOP_WASM
    export BLEND_MAINNET_V3_FACTORY_WASM BLEND_MAINNET_V3_POOL_WASM
    export BLEND_MAINNET_V3_BACKFILL_WASM
    export BLEND_MAINNET_V3_SOURCE_COMMIT BLEND_MAINNET_BACKFILL_SOURCE_COMMIT
    export BLEND_MAINNET_V3_EMITTER_SHA256 BLEND_MAINNET_V3_BACKSTOP_SHA256
    export BLEND_MAINNET_V3_FACTORY_SHA256 BLEND_MAINNET_V3_POOL_SHA256
    export BLEND_MAINNET_V3_BACKFILL_SHA256
    export BLEND_MAINNET_V3_EMITTER_SALT BLEND_MAINNET_V3_BACKFILL_SALT
    export BLEND_MAINNET_V3_FACTORY_SALT BLEND_MAINNET_V3_BACKSTOP_SALT
    export BLEND_MAINNET_V3_EXPECTED_EMITTER BLEND_MAINNET_V3_EXPECTED_BACKFILL
    export BLEND_MAINNET_V3_EXPECTED_FACTORY BLEND_MAINNET_V3_EXPECTED_BACKSTOP
    export BLEND_MAINNET_V3_LOCK_BLNT_ISSUER BLEND_MAINNET_V3_SKIP_BUILD
}

comet_plan() {
    export_common_environment
    bash "${ROOT_DIR}/scripts/mainnet-comet-deployment.sh" plan
}

core_plan() {
    export_core_environment
    bash "${ROOT_DIR}/scripts/mainnet-v3-deployment.sh" plan
}

deployment_plan() {
    cat <<EOF
Source-linked BLNT V3 mainnet replacement plan
  New BLNT issuer:    $(plan_value '.asset.issuer')
  New BLNT SAC:       $(plan_value '.asset.contract_id')
  Home domain:        $(plan_value '.asset.home_domain')
  Premint:            $(plan_value '.asset.premint_amount') BLNT

  BLNT:USDC Comet:    $(plan_value '.comet_v1_1.blnt_usdc.contract_id')
  BLNT:XLM Comet:     $(plan_value '.comet_v1_1.blnt_xlm.contract_id')
  Comet controller:   $(plan_value '.comet_v1_1.controller')

  V3 emitter:         $(plan_value '.contracts.emitter.contract_id')
  BLNT backfill:      $(plan_value '.contracts.backfill.contract_id')
  V3 pool factory:    $(plan_value '.contracts.pool_factory.contract_id')
  V3 backstop:        $(plan_value '.contracts.backstop.contract_id')

  Isolated work root: $(plan_value '.isolation.work_dir')
  Final custody: the V3 emitter becomes BLNT SAC administrator, then the
  classic issuer is permanently locked. Only the Fixed V3 lending pool is
  deployed later, in a separate reviewed phase; it is not funded or activated.

This plan is read-only and does not mutate mainnet.
EOF
}

usage() {
    cat <<'EOF'
Usage: scripts/mainnet-v3-redeployment.sh COMMAND

Commands:
  plan         Print the complete immutable replacement plan.
  validate     Verify local keys, source pins, release artifacts, and addresses.
  preflight    Verify the issuer and all predicted addresses are unused and
               that the deployment wallet is sufficiently funded.
  comet-plan   Print the exact replacement Comet initialization plan.
  core-plan    Print the exact replacement V3 core deployment plan.

No command in this preparation script submits a mainnet transaction.
EOF
}

case "${1:-}" in
    plan) deployment_plan ;;
    validate) validate_command ;;
    preflight) preflight_command ;;
    comet-plan) comet_plan ;;
    core-plan) core_plan ;;
    *) usage; exit 2 ;;
esac
