#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Reuse the mainnet RPC, signing, transaction-evidence, Comet validation, and
# SAC custody helpers without entering the Comet runner's command dispatcher.
export BLEND_MAINNET_COMET_LIBRARY_ONLY=true
export BLEND_MAINNET_COMET_CONFIG_DIR="${BLEND_MAINNET_V3_CONFIG_DIR:-${ROOT_DIR}/.mainnet-v3/stellar-config}"
# shellcheck disable=SC1091
source "${ROOT_DIR}/scripts/mainnet-comet-deployment.sh"
unset BLEND_MAINNET_COMET_LIBRARY_ONLY

WORK_DIR="${BLEND_MAINNET_V3_WORK_DIR:-${ROOT_DIR}/.mainnet-v3/core}"
CONFIG_DIR="${BLEND_MAINNET_V3_CONFIG_DIR:-${ROOT_DIR}/.mainnet-v3/stellar-config}"
STATE_FILE="${BLEND_MAINNET_V3_STATE_FILE:-${WORK_DIR}/state.json}"
REGISTRY_FILE="${BLEND_MAINNET_V3_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-v3.json}"
COMET_STATE_FILE="${BLEND_MAINNET_COMET_STATE_FILE:-${ROOT_DIR}/.mainnet-v3/comet/state.json}"
GRANT_FILE="${BLEND_MAINNET_V3_GRANT_FILE:-${ROOT_DIR}/.mainnet-v3/blnt-grants.json}"
ISSUER_IDENTITY="${BLEND_MAINNET_V3_ISSUER_IDENTITY:-blnt-v3-mainnet-issuer}"

EMITTER_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/emitter.wasm"
BACKSTOP_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
FACTORY_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/pool_factory.wasm"
POOL_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/pool.wasm"
BACKFILL_WASM="${ROOT_DIR}/blnt-backfill-contract/target/wasm32v1-none/optimized/blnt_backfill_contract.wasm"

BACKFILL_SOURCE="${ROOT_DIR}/comet_cpal_flattened_ownership_before_41c898a1.csv"
BACKFILL_MANIFEST="${ROOT_DIR}/blnt-backfill-contract/allocations/comet_cpal_backfill.json"
BACKFILL_GENERATOR="${ROOT_DIR}/blnt-backfill-contract/scripts/generate-backfill-claims.mjs"

V3_SOURCE_COMMIT="2aa1857c769771357cb5d6980555ce8b4cf9c15c"
BACKFILL_SOURCE_COMMIT="ec3b89b0853df5c1cba9ed988423aa838edee9c9"
EXPECTED_GRANT_SHA256="ef8707b6b20e9828c9086ee2b00188fff4025f84418d5bdfa2545d69c8f854a2"
EXPECTED_BACKFILL_MANIFEST_SHA256="6786d7f2e8ecf11acf322d768f650aa860e7eaa15566bf047a9a34fbffd9bdd9"

EMITTER_SHA256="bc2ba3c5b37b1daf32f9015d41b0f1c7a2e5bd79138029ef8be76de36bbb568a"
BACKSTOP_SHA256="bda2e1cb2de7ec0621a9408e0a2d7989bc84649bc401fa879aa48fa27b349d3f"
FACTORY_SHA256="b956b222641fe8a419668157351311127b40afd80c7289af1cd19ac98e2e8cec"
POOL_SHA256="8df52229d11a1ede42ee8f77189e30ca54cb0b6d502db53809ba249ed6bc7782"
BACKFILL_SHA256="035fa1de88122a2bcd0707433557914dd17136fc65676aca1ce223ca0d9c945d"

EMITTER_SALT="9ded6a157115f3af8e5454402d10a2ecd6eb687a0408b49967ec59097fae0022"
BACKFILL_SALT="86b927ed705244682236c1742f246803bafd0f40c84075a2379e4d313dea0f13"
FACTORY_SALT="7ba9b2a78e969ffb744a3ada9b943d35655fd91cbeb63b30c2f53c9cfd0a6f1e"
BACKSTOP_SALT="d88613203bbb361777a1e5e5f562f6874233b8b7e3aae406e55e6bf0ad76f653"

EXPECTED_EMITTER="CAL2PDDBVHMONNTZ2B5KMRWJMXYJ7NWWRYEYZU2PCBWOZVKACEW7CSY3"
EXPECTED_BACKFILL="CDAEU5SHJZE3XHH62IMCVOR6PX2ZPJVWS6WQMPAPSWRFDF66XKIQZBCJ"
EXPECTED_FACTORY="CCF73PABFG3DBKQW5QK2E5JZOQN4OJNA4PLFY7MZVQNHF676RI33QVTM"
EXPECTED_BACKSTOP="CACD7TLQBWNR74MB3DDLZXXVM3MERNMKXCWAN3ONNADNDV7H4R54G7MU"

BLNT_USDC="CBGQHIZUV7SSZQ3MCMN7NETTDU3Z5MCQOGHJJTRD7UTQ7ZVXUGP7AKJJ"
BLNT_XLM="CBFKHCZDHYOWGO6QQJ4MCDYGSNSVC7IZ5SSMLV5LWV5CJUJMT5AGLKI6"

BACKFILL_ALLOCATION=$((74000000 * SCALAR_7))
GRANT_ALLOCATION=$((25000000 * SCALAR_7))
SWAP_ALLOCATION=$((51000000 * SCALAR_7))
INITIAL_DROP=$((150000000 * SCALAR_7))
BACKFILL_VESTING_SECONDS=$((180 * 24 * 60 * 60))
GRANT_VESTING_SECONDS=$((360 * 24 * 60 * 60))
SWAP_DURATION_SECONDS=$((270 * 24 * 60 * 60))
MIN_DEPLOYMENT_XLM="1000"

GRANT_LIST=""
CLAIM_LIST=""
GRANT_COUNT=""
GRANT_TOTAL=""
CLAIM_COUNT=""
CLAIM_TOTAL=""

assert_equal() {
    local expected="$1" actual="$2" label="$3"
    [[ "${actual}" == "${expected}" ]] ||
        die "${label}: expected ${expected}, got ${actual}"
}

validate_address() {
    local address="$1" label="$2" resolved
    [[ "${address}" =~ ^[CG][A-Z2-7]{55}$ ]] || die "invalid ${label} address"
    resolved="$(stellar_cli keys public-key "${address}" 2>/dev/null)" ||
        die "${label} address has an invalid checksum"
    assert_equal "${address}" "${resolved}" "${label} address"
}

load_private_grants() {
    local address
    [[ -f "${GRANT_FILE}" ]] || die "private grant file is missing: ${GRANT_FILE}"
    [[ "$(stat -f '%Lp' "${GRANT_FILE}")" == "600" ]] ||
        die "private grant file must have mode 0600"
    assert_equal "${EXPECTED_GRANT_SHA256}" "$(sha256_file "${GRANT_FILE}")" \
        "private grant file SHA-256"
    jq -e --arg total "${GRANT_ALLOCATION}" '
        .schema_version == 1 and
        .total_raw == $total and
        (.allocations | type == "array" and length > 0 and length <= 100) and
        ([.allocations[].address] | unique | length) == (.allocations | length) and
        ([.allocations[] |
            (.address | type == "string" and test("^G[A-Z2-7]{55}$")) and
            (.amount_raw | type == "string" and test("^[1-9][0-9]*$"))
        ] | all) and
        (([.allocations[].amount_raw | tonumber] | add | tostring) == $total)
    ' "${GRANT_FILE}" >/dev/null || die "private grant file failed invariant validation"
    while IFS= read -r address; do
        validate_address "${address}" "grant recipient"
    done < <(jq -r '.allocations[].address' "${GRANT_FILE}")
    GRANT_LIST="$(jq -c '[.allocations[] | [.address, .amount_raw]]' "${GRANT_FILE}")"
    GRANT_COUNT="$(jq -r '.allocations | length' "${GRANT_FILE}")"
    GRANT_TOTAL="$(jq -r '[.allocations[].amount_raw | tonumber] | add | tostring' "${GRANT_FILE}")"
}

validate_backfill_manifest() {
    local generated expected_source_hash
    [[ -f "${BACKFILL_SOURCE}" ]] || die "backfill snapshot is missing"
    [[ -f "${BACKFILL_MANIFEST}" ]] || die "backfill manifest is missing"
    [[ -f "${BACKFILL_GENERATOR}" ]] || die "backfill generator is missing"
    assert_equal "${EXPECTED_BACKFILL_MANIFEST_SHA256}" \
        "$(sha256_file "${BACKFILL_MANIFEST}")" "backfill manifest SHA-256"
    expected_source_hash="$(jq -er '.source_sha256' "${BACKFILL_MANIFEST}")"
    assert_equal "${expected_source_hash}" "$(sha256_file "${BACKFILL_SOURCE}")" \
        "backfill source SHA-256"
    jq -e --arg total "${BACKFILL_ALLOCATION}" '
        .schema_version == 1 and
        .weight_field == "flattened_total_cpal_raw" and
        .allocation_total_raw == $total and
        .claimant_count == 434 and
        .claimant_type_counts == {account: 423, contract: 11} and
        (.allocations | length) == 434 and
        ([.allocations[].owner_address] | unique | length) == 434 and
        ([.allocations[].allocation_raw | test("^[1-9][0-9]*$")] | all) and
        (([.allocations[].allocation_raw | tonumber] | add | tostring) == $total)
    ' "${BACKFILL_MANIFEST}" >/dev/null ||
        die "backfill manifest failed invariant validation"
    generated="$(mktemp "${TMPDIR:-/tmp}/blnt-mainnet-backfill.XXXXXX")"
    node "${BACKFILL_GENERATOR}" "${BACKFILL_SOURCE}" "${generated}" >/dev/null
    if ! cmp -s "${BACKFILL_MANIFEST}" "${generated}"; then
        rm -f "${generated}"
        die "backfill manifest does not reproduce from its source snapshot"
    fi
    rm -f "${generated}"
    CLAIM_LIST="$(jq -c '[.allocations[] | [.owner_address, .allocation_raw]]' \
        "${BACKFILL_MANIFEST}")"
    CLAIM_COUNT="$(jq -r '.allocations | length' "${BACKFILL_MANIFEST}")"
    CLAIM_TOTAL="$(jq -r '[.allocations[].allocation_raw | tonumber] | add | tostring' \
        "${BACKFILL_MANIFEST}")"
}

validate_artifact() {
    local path="$1" expected="$2" label="$3"
    [[ -s "${path}" ]] || die "missing ${label} artifact: ${path}"
    assert_equal "${expected}" "$(sha256_file "${path}")" "${label} WASM SHA-256"
}

verify_canonical_pool_config() {
    local label="$1" pool="$2" first="$3" second="$4" controller_expected="$5"
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
    assert_equal "${WEIGHT_BLNT}" "${first_weight}" "${label} BLNT weight"
    assert_equal "${WEIGHT_PAIR}" "${second_weight}" "${label} pair weight"
    assert_equal "${SWAP_FEE}" "${fee}" "${label} swap fee"
    assert_equal "${controller_expected}" "${controller}" "${label} controller"
    assert_equal 7 "${decimals}" "${label} LP decimals"
}

verify_locked_controller() {
    local controller="$1" account
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

validate_source_pins() {
    local actual
    actual="$(git -C "${ROOT_DIR}/blnt-contracts-v3" rev-parse HEAD)"
    assert_equal "${V3_SOURCE_COMMIT}" "${actual}" "V3 source commit"
    actual="$(git -C "${ROOT_DIR}/blnt-backfill-contract" rev-parse HEAD)"
    assert_equal "${BACKFILL_SOURCE_COMMIT}" "${actual}" "backfill source commit"
    actual="$(git -C "${ROOT_DIR}/comet-contracts-v1" rev-parse HEAD)"
    assert_equal "${COMET_SOURCE_COMMIT}" "${actual}" "Comet source commit"
    assert_equal "v1.1.0" \
        "$(git -C "${ROOT_DIR}/comet-contracts-v1" describe --tags --exact-match HEAD)" \
        "Comet source tag"
}

validate_inputs() {
    require_command cmp
    require_command curl
    require_command git
    require_command jq
    require_command node
    require_command shasum
    require_command stellar
    validate_source_pins
    validate_backfill_manifest
    load_private_grants
    validate_artifact "${EMITTER_WASM}" "${EMITTER_SHA256}" emitter
    validate_artifact "${BACKSTOP_WASM}" "${BACKSTOP_SHA256}" backstop
    validate_artifact "${FACTORY_WASM}" "${FACTORY_SHA256}" pool-factory
    validate_artifact "${POOL_WASM}" "${POOL_SHA256}" pool
    validate_artifact "${BACKFILL_WASM}" "${BACKFILL_SHA256}" backfill
    validate_release
}

predict_contract() {
    local salt="$1"
    normalize_scalar "$(stellar_cli contract id wasm \
        --salt "${salt}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
}

validate_predictions() {
    assert_equal "${EXPECTED_EMITTER}" "$(predict_contract "${EMITTER_SALT}")" \
        "emitter prediction"
    assert_equal "${EXPECTED_BACKFILL}" "$(predict_contract "${BACKFILL_SALT}")" \
        "backfill prediction"
    assert_equal "${EXPECTED_FACTORY}" "$(predict_contract "${FACTORY_SALT}")" \
        "factory prediction"
    assert_equal "${EXPECTED_BACKSTOP}" "$(predict_contract "${BACKSTOP_SALT}")" \
        "backstop prediction"
}

fresh_fetch_contract() {
    local contract="$1" destination="$2" temporary_dir stderr_file stderr_output attempt
    for attempt in 1 2 3; do
        temporary_dir="$(mktemp -d)"
        stderr_file="${temporary_dir}/fetch.err"
        if stellar_cli contract fetch \
            --id "${contract}" \
            --out-file "${temporary_dir}/contract.wasm" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            2>"${stderr_file}"
        then
            cp "${temporary_dir}/contract.wasm" "${destination}"
            rm -r -- "${temporary_dir}"
            return 0
        fi
        stderr_output="$(<"${stderr_file}")"
        if [[ "${stderr_output}" == *"Contract not found"* ||
            "${stderr_output}" == *"not found in ledger"* ]]
        then
            rm -r -- "${temporary_dir}"
            return 1
        fi
        if (( attempt == 3 )); then
            note "Contract fetch failed after three attempts: ${contract}"
            sed -n '1,20p' "${stderr_file}" >&2
            rm -r -- "${temporary_dir}"
            die "unable to determine whether contract exists"
        fi
        rm -r -- "${temporary_dir}"
    done
}

contract_exists() {
    local contract="$1" temporary
    temporary="$(mktemp)"
    if fresh_fetch_contract "${contract}" "${temporary}"; then
        rm -f "${temporary}"
        return 0
    fi
    rm -f "${temporary}"
    return 1
}

verify_contract_artifact() {
    local contract="$1" artifact="$2" label="$3" temporary
    temporary="$(mktemp)"
    fresh_fetch_contract "${contract}" "${temporary}" || {
        rm -f "${temporary}"
        die "${label} contract is unavailable: ${contract}"
    }
    cmp -s "${artifact}" "${temporary}" || {
        rm -f "${temporary}"
        die "${label} deployed WASM does not match the pinned artifact"
    }
    rm -f "${temporary}"
}

assert_expected_contracts_unused() {
    local label contract
    while read -r label contract; do
        if contract_exists "${contract}"; then
            die "predicted ${label} address is already deployed: ${contract}"
        fi
    done <<EOF
emitter ${EXPECTED_EMITTER}
backfill ${EXPECTED_BACKFILL}
factory ${EXPECTED_FACTORY}
backstop ${EXPECTED_BACKSTOP}
EOF
}

require_clean_reviewed_main() {
    local mode="${1:-mutation}" head remote branch
    if [[ "${mode}" == "read-only" &&
        "${BLEND_MAINNET_V3_ALLOW_DIRTY:-false}" == "true" ]]
    then
        return
    fi
    git -C "${ROOT_DIR}" diff --quiet --ignore-submodules=none ||
        die "working tree is dirty; deploy only a reviewed commit"
    git -C "${ROOT_DIR}" diff --cached --quiet --ignore-submodules=none ||
        die "index is dirty; deploy only a reviewed commit"
    [[ -z "$(git -C "${ROOT_DIR}" status --porcelain --untracked-files=normal)" ]] ||
        die "working tree contains uncommitted files"
    branch="$(git -C "${ROOT_DIR}" branch --show-current)"
    assert_equal main "${branch}" "deployment branch"
    head="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    remote="$(git -C "${ROOT_DIR}" rev-parse origin/main)"
    assert_equal "${remote}" "${head}" "reviewed origin/main commit"
}

verify_existing_prerequisites() {
    local mode="${1:-before-handoff}" comet_controller account admin issuer xlm_balance
    [[ -f "${COMET_STATE_FILE}" ]] || die "canonical Comet deployment state is missing"
    assert_equal verified "$(jq -er '.phase' "${COMET_STATE_FILE}")" \
        "canonical Comet deployment phase"
    assert_equal "${BLNT_USDC}" "$(jq -er '.blnt_usdc_token' "${COMET_STATE_FILE}")" \
        "BLNT:USDC contract"
    assert_equal "${BLNT_XLM}" "$(jq -er '.blnt_xlm_token' "${COMET_STATE_FILE}")" \
        "BLNT:XLM contract"
    comet_controller="$(jq -er '.comet_controller' "${COMET_STATE_FILE}")"
    verify_contract_artifact "${BLNT_USDC}" "${COMET_WASM}" BLNT-USDC
    verify_contract_artifact "${BLNT_XLM}" "${COMET_WASM}" BLNT-XLM
    verify_canonical_pool_config BLNT-USDC "${BLNT_USDC}" "${BLNT}" "${USDC}" \
        "${comet_controller}"
    verify_canonical_pool_config BLNT-XLM "${BLNT_XLM}" "${BLNT}" "${XLM}" \
        "${comet_controller}"
    verify_locked_controller "${comet_controller}"
    verify_non_clawbackable_balance BLNT-USDC-BLNT "${BLNT}" "${BLNT_USDC}"
    verify_non_clawbackable_balance BLNT-USDC-USDC "${USDC}" "${BLNT_USDC}"
    verify_non_clawbackable_balance BLNT-XLM-BLNT "${BLNT}" "${BLNT_XLM}"
    assert_equal 7 "$(pool_value "${BLNT}" decimals)" "BLNT decimals"
    assert_equal 7 "$(pool_value "${LEGACY_BLND}" decimals)" "BLND decimals"
    admin="$(normalize_scalar "$(invoke_view "${BLNT}" admin)")"
    if [[ "${mode}" == "before-handoff" ]]; then
        assert_equal "${BLNT_ISSUER}" "${admin}" "pre-deployment BLNT administrator"
    elif [[ "${admin}" != "${BLNT_ISSUER}" && "${admin}" != "${EXPECTED_EMITTER}" ]]; then
        die "BLNT administrator is neither the issuer nor the V3 emitter"
    fi
    issuer="$(stellar_cli keys public-key "${ISSUER_IDENTITY}")"
    assert_equal "${BLNT_ISSUER}" "${issuer}" "BLNT issuer signing identity"
    account="$(horizon_account "${WALLET}")"
    xlm_balance="$(printf '%s' "${account}" | jq -er \
        '[.balances[] | select(.asset_type == "native")][0].balance')"
    jq -en --arg balance "${xlm_balance}" --arg minimum "${MIN_DEPLOYMENT_XLM}" \
        '($balance | tonumber) >= ($minimum | tonumber)' >/dev/null ||
        die "deployment wallet has ${xlm_balance} XLM; require at least ${MIN_DEPLOYMENT_XLM}"
}

initialize_core_state() {
    local source_commit
    mkdir -p "${WORK_DIR}" "${CONFIG_DIR}"
    chmod 700 "${WORK_DIR}" "${CONFIG_DIR}"
    RUN_DIR="${WORK_DIR}/runs/$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "${RUN_DIR}"
    source_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    jq -n \
        --arg passphrase "${NETWORK_PASSPHRASE}" \
        --arg rpc "${RPC_URL}" \
        --arg wallet "${WALLET}" \
        --arg source_commit "${source_commit}" \
        --arg run_dir "${RUN_DIR}" \
        --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg emitter "${EXPECTED_EMITTER}" \
        --arg backfill "${EXPECTED_BACKFILL}" \
        --arg factory "${EXPECTED_FACTORY}" \
        --arg backstop "${EXPECTED_BACKSTOP}" \
        --arg blnt "${BLNT}" --arg blnd "${LEGACY_BLND}" \
        --arg usdc "${USDC}" --arg xlm "${XLM}" \
        --arg blnt_usdc "${BLNT_USDC}" --arg blnt_xlm "${BLNT_XLM}" \
        --arg grant_hash "${EXPECTED_GRANT_SHA256}" \
        --arg grant_count "${GRANT_COUNT}" --arg grant_total "${GRANT_TOTAL}" \
        --arg manifest_hash "${EXPECTED_BACKFILL_MANIFEST_SHA256}" \
        --arg claim_count "${CLAIM_COUNT}" --arg claim_total "${CLAIM_TOTAL}" \
        --arg emitter_hash "${EMITTER_SHA256}" --arg backfill_hash "${BACKFILL_SHA256}" \
        --arg factory_hash "${FACTORY_SHA256}" --arg backstop_hash "${BACKSTOP_SHA256}" \
        --arg pool_hash "${POOL_SHA256}" '
        {
            schema_version: 1,
            network: "public",
            network_passphrase: $passphrase,
            rpc_url: $rpc,
            wallet: $wallet,
            source_commit: $source_commit,
            run_dir: $run_dir,
            created_at: $created_at,
            phase: "planned",
            v3_emitter: $emitter,
            blnt_backfill: $backfill,
            v3_factory: $factory,
            v3_backstop: $backstop,
            assets: {blnt: $blnt, legacy_blnd: $blnd, usdc: $usdc, xlm: $xlm},
            comet_v1_1: {blnt_usdc: $blnt_usdc, blnt_xlm: $blnt_xlm},
            private_grants: {sha256: $grant_hash, count: ($grant_count|tonumber), total_raw: $grant_total},
            backfill_claims: {sha256: $manifest_hash, count: ($claim_count|tonumber), total_raw: $claim_total},
            artifacts: {
                emitter: $emitter_hash,
                backfill: $backfill_hash,
                factory: $factory_hash,
                backstop: $backstop_hash,
                pool: $pool_hash
            },
            transactions: {}
        }' >"${STATE_FILE}"
}

load_core_state() {
    state_exists || die "mainnet V3 state does not exist: ${STATE_FILE}"
    RUN_DIR="$(state_value run_dir)"
    [[ -d "${RUN_DIR}" ]] || die "recorded run directory is missing"
    assert_equal "${NETWORK_PASSPHRASE}" "$(state_value network_passphrase)" \
        "state network"
    assert_equal "${WALLET}" "$(state_value wallet)" "state wallet"
    assert_equal "${EXPECTED_GRANT_SHA256}" \
        "$(jq -er '.private_grants.sha256' "${STATE_FILE}")" "state grant digest"
    assert_equal "${EXPECTED_BACKFILL_MANIFEST_SHA256}" \
        "$(jq -er '.backfill_claims.sha256' "${STATE_FILE}")" "state claim digest"
}

state_has_transaction() {
    jq -e --arg label "$1" '.transactions[$label] | type == "string"' \
        "${STATE_FILE}" >/dev/null 2>&1
}

deploy_expected_contract() {
    local label="$1" expected="$2" wasm="$3" salt="$4" raw deployed temporary
    shift 4
    temporary="$(mktemp)"
    if fresh_fetch_contract "${expected}" "${temporary}"; then
        cmp -s "${wasm}" "${temporary}" || {
            rm -f "${temporary}"
            die "${label} address contains unexpected WASM"
        }
        rm -f "${temporary}"
        state_has_transaction "${label}" ||
            die "${label} exists without recorded deployment evidence; review manually"
        printf '%s\n' "${expected}"
        return
    fi
    rm -f "${temporary}"
    capture "${label}" wallet_stellar contract deploy \
        --wasm "${wasm}" --optimize=false --salt "${salt}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost -- "$@" >/dev/null
    raw="$(<"${RUN_DIR}/${label}.out")"
    deployed="$(normalize_scalar "${raw}")"
    assert_equal "${expected}" "${deployed}" "${label} deployed contract"
    record_transaction "${label}"
    verify_contract_artifact "${expected}" "${wasm}" "${label}"
    printf '%s\n' "${expected}"
}

ensure_pool_wasm_uploaded() {
    local temporary_dir temporary raw uploaded
    temporary_dir="$(mktemp -d)"
    temporary="${temporary_dir}/pool.wasm"
    if fetch_contract_artifact "${temporary}" "V3 pool WASM ${POOL_SHA256}" \
        --wasm-hash "${POOL_SHA256}"
    then
        cmp -s "${POOL_WASM}" "${temporary}" || {
            rm -r -- "${temporary_dir}"
            die "on-chain pool WASM bytes do not match the pinned artifact"
        }
        rm -r -- "${temporary_dir}"
        return
    fi
    rm -r -- "${temporary_dir}"
    capture upload-v3-pool wallet_stellar contract upload \
        --wasm "${POOL_WASM}" --optimize=false --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost >/dev/null
    raw="$(<"${RUN_DIR}/upload-v3-pool.out")"
    uploaded="$(normalize_scalar "${raw}")"
    assert_equal "${POOL_SHA256}" "${uploaded}" "uploaded pool WASM hash"
    record_transaction upload-v3-pool
}

ensure_blnt_admin_handoff() {
    local admin
    admin="$(normalize_scalar "$(invoke_view "${BLNT}" admin)")"
    if [[ "${admin}" == "${BLNT_ISSUER}" ]]; then
        capture handoff-blnt-admin stellar_cli contract invoke \
            --id "${BLNT}" --source-account "${ISSUER_IDENTITY}" \
            --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost -- set_admin --new_admin "${EXPECTED_EMITTER}" >/dev/null
        record_transaction handoff-blnt-admin
    else
        assert_equal "${EXPECTED_EMITTER}" "${admin}" "BLNT administrator"
    fi
    admin="$(normalize_scalar "$(invoke_view "${BLNT}" admin)")"
    assert_equal "${EXPECTED_EMITTER}" "${admin}" "BLNT administrator after handoff"
}

ensure_emitter_initialized() {
    local current
    if current="$(invoke_view "${EXPECTED_EMITTER}" get_backstop 2>/dev/null)"; then
        assert_equal "${EXPECTED_BACKSTOP}" "$(normalize_scalar "${current}")" \
            "emitter backstop"
        return
    fi
    capture initialize-v3-emitter wallet_stellar contract invoke \
        --id "${EXPECTED_EMITTER}" --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost -- initialize --blnd_token "${BLNT}" \
        --backstop "${EXPECTED_BACKSTOP}" --backstop_token "${BLNT_XLM}" >/dev/null
    record_transaction initialize-v3-emitter
    current="$(normalize_scalar "$(invoke_view "${EXPECTED_EMITTER}" get_backstop)")"
    assert_equal "${EXPECTED_BACKSTOP}" "${current}" "initialized emitter backstop"
}

ensure_initial_drop() {
    local balance claimed swapped expired expected
    balance="$(token_balance "${BLNT}" "${EXPECTED_BACKFILL}")"
    claimed="$(pool_value "${EXPECTED_BACKFILL}" get_total_claimed)"
    swapped="$(pool_value "${EXPECTED_BACKFILL}" get_total_swapped)"
    expired="$(pool_value "${EXPECTED_BACKFILL}" get_expired_blnt_burned)"
    expected=$((INITIAL_DROP - claimed - swapped - expired))
    if [[ "${balance}" == "${expected}" ]]; then
        state_set phase funded
        return
    fi
    assert_equal 0 "${balance}" "pre-drop backfill BLNT balance"
    if ! state_has_transaction activate-v3-backstop; then
        capture activate-v3-backstop wallet_stellar contract invoke \
            --id "${EXPECTED_BACKSTOP}" --source-account "${WALLET}" \
            --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
            --cost -- distribute >/dev/null
        record_transaction activate-v3-backstop
    fi
    state_has_transaction execute-v3-initial-drop &&
        die "initial drop is recorded but backfill custody does not conserve; review manually"
    capture execute-v3-initial-drop wallet_stellar contract invoke \
        --id "${EXPECTED_BACKSTOP}" --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost -- drop >/dev/null
    record_transaction execute-v3-initial-drop
    balance="$(token_balance "${BLNT}" "${EXPECTED_BACKFILL}")"
    assert_equal "${INITIAL_DROP}" "${balance}" "funded backfill BLNT balance"
    state_set phase funded
}

verify_backfill_state() {
    local total backfill grant claimed swapped burned expired balance
    local vesting_start vesting_end grant_start grant_end swap_deadline expected_balance
    assert_equal "${LEGACY_BLND}" \
        "$(normalize_scalar "$(invoke_view "${EXPECTED_BACKFILL}" get_legacy_blnd_token)")" \
        "backfill legacy BLND binding"
    assert_equal "${BLNT}" \
        "$(normalize_scalar "$(invoke_view "${EXPECTED_BACKFILL}" get_blnt_token)")" \
        "backfill BLNT binding"
    total="$(pool_value "${EXPECTED_BACKFILL}" get_total_allocated)"
    backfill="$(pool_value "${EXPECTED_BACKFILL}" get_backfill_allocated)"
    grant="$(pool_value "${EXPECTED_BACKFILL}" get_grant_allocated)"
    assert_equal "$((BACKFILL_ALLOCATION + GRANT_ALLOCATION))" "${total}" \
        "combined claim allocation"
    assert_equal "${BACKFILL_ALLOCATION}" "${backfill}" "backfill allocation"
    assert_equal "${GRANT_ALLOCATION}" "${grant}" "grant allocation"
    vesting_start="$(pool_value "${EXPECTED_BACKFILL}" get_vesting_start)"
    vesting_end="$(pool_value "${EXPECTED_BACKFILL}" get_vesting_end)"
    grant_start="$(pool_value "${EXPECTED_BACKFILL}" get_grant_vesting_start)"
    grant_end="$(pool_value "${EXPECTED_BACKFILL}" get_grant_vesting_end)"
    swap_deadline="$(pool_value "${EXPECTED_BACKFILL}" get_swap_deadline)"
    assert_equal "${BACKFILL_VESTING_SECONDS}" "$((vesting_end - vesting_start))" \
        "backfill vesting duration"
    assert_equal "${vesting_start}" "${grant_start}" "grant vesting start"
    assert_equal "${GRANT_VESTING_SECONDS}" "$((grant_end - grant_start))" \
        "grant vesting duration"
    assert_equal "${SWAP_DURATION_SECONDS}" "$((swap_deadline - vesting_start))" \
        "conversion duration"
    claimed="$(pool_value "${EXPECTED_BACKFILL}" get_total_claimed)"
    swapped="$(pool_value "${EXPECTED_BACKFILL}" get_total_swapped)"
    burned="$(pool_value "${EXPECTED_BACKFILL}" get_total_blnd_burned)"
    expired="$(pool_value "${EXPECTED_BACKFILL}" get_expired_blnt_burned)"
    assert_equal "${swapped}" "${burned}" "converted and burned BLND"
    balance="$(token_balance "${BLNT}" "${EXPECTED_BACKFILL}")"
    expected_balance=$((INITIAL_DROP - claimed - swapped - expired))
    assert_equal "${expected_balance}" "${balance}" "backfill BLNT conservation"
}

verify_live_deployment() {
    local admin recipient queued migration_token reward_zone
    verify_contract_artifact "${EXPECTED_EMITTER}" "${EMITTER_WASM}" emitter
    verify_contract_artifact "${EXPECTED_BACKFILL}" "${BACKFILL_WASM}" backfill
    verify_contract_artifact "${EXPECTED_FACTORY}" "${FACTORY_WASM}" pool-factory
    verify_contract_artifact "${EXPECTED_BACKSTOP}" "${BACKSTOP_WASM}" backstop
    admin="$(normalize_scalar "$(invoke_view "${BLNT}" admin)")"
    assert_equal "${EXPECTED_EMITTER}" "${admin}" "BLNT administrator"
    recipient="$(normalize_scalar "$(invoke_view "${EXPECTED_EMITTER}" get_backstop)")"
    assert_equal "${EXPECTED_BACKSTOP}" "${recipient}" "emitter backstop"
    queued="$(invoke_view "${EXPECTED_EMITTER}" get_queued_swap)"
    printf '%s' "${queued}" | jq -e '. == null' >/dev/null ||
        die "new emitter unexpectedly has a queued backstop swap"
    migration_token="$(normalize_scalar "$(invoke_view "${EXPECTED_BACKSTOP}" migration_token)")"
    assert_equal "${BLNT_XLM}" "${migration_token}" "V3 migration token"
    reward_zone="$(invoke_view "${EXPECTED_BACKSTOP}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e 'type == "array"' >/dev/null ||
        die "invalid V3 reward zone response"
    verify_backfill_state
}

write_registry() {
    local temporary
    temporary="${REGISTRY_FILE}.tmp"
    jq \
        --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg pool_hash "${POOL_SHA256}" '
        {
            schema_version,
            network: {name: "Public Global Stellar Network", passphrase: .network_passphrase},
            generated_at: $generated_at,
            deployer: .wallet,
            source_commit,
            contracts: {
                emitter: .v3_emitter,
                backfill: .blnt_backfill,
                pool_factory: .v3_factory,
                backstop: .v3_backstop,
                pool_wasm_hash: $pool_hash
            },
            assets,
            comet_v1_1,
            allocations: {
                private_grants: .private_grants,
                backfill_claims: .backfill_claims,
                conversion_reserve_raw: "510000000000000",
                initial_drop_raw: "1500000000000000"
            },
            artifacts,
            transactions,
            verified_at
        }' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${REGISTRY_FILE}"
}

ensure_no_competing_signers() {
    require_process_absent \
        'blend-v2.1-migration/scripts/mainnet-emissions-keeper.sh run' \
        "the V2.1 mainnet keeper is using the deployment wallet; stop it first"
    require_process_absent \
        'mainnet-arbitrage\.mjs run' \
        "the arbitrage bot is using the deployment wallet; stop it first"
}

require_process_absent() {
    local pattern="$1" found_message="$2" output status
    if output="$(pgrep -fl "${pattern}" 2>&1)"; then
        die "${found_message} (${output})"
    else
        status=$?
    fi
    (( status == 1 )) || die "unable to inspect competing wallet processes: ${output}"
}

require_mutation_confirmation() {
    assert_equal DEPLOY_BLNT_V3_MAINNET "${BLEND_MAINNET_V3_CONFIRM:-}" \
        "mainnet deployment confirmation"
    assert_equal true "${BLEND_MAINNET_V3_SEQUENCE_LOCK_CONFIRMED:-false}" \
        "exclusive sequence-lock confirmation"
    require_clean_reviewed_main mutation
    ensure_no_competing_signers
    load_wallet_config
}

deployment_plan() {
    validate_backfill_manifest
    load_private_grants
    validate_predictions
    cat <<EOF
BLNT V3 mainnet core deployment plan (no lending pools)
  Emitter:      ${EXPECTED_EMITTER}
  Backfill:     ${EXPECTED_BACKFILL}
  Pool factory: ${EXPECTED_FACTORY}
  Backstop:     ${EXPECTED_BACKSTOP}
  Pool WASM:    ${POOL_SHA256}

  Existing BLNT:USDC: ${BLNT_USDC}
  Existing BLNT:XLM:  ${BLNT_XLM}
  Existing BLNT:      ${BLNT}
  Existing BLND:      ${LEGACY_BLND}

  Backfill claims: ${CLAIM_COUNT} recipients / ${CLAIM_TOTAL} raw BLNT
  Contributor grants: ${GRANT_COUNT} recipients / ${GRANT_TOTAL} raw BLNT
  Grant input SHA-256: ${EXPECTED_GRANT_SHA256}
  Conversion reserve: ${SWAP_ALLOCATION} raw BLNT
  Initial drop: ${INITIAL_DROP} raw BLNT

The emitter, lending-pool WASM, and factory are prepared before the backfill
constructor starts its vesting clock. Execution is deliberately continuous
from that constructor through BLNT administration handoff, emitter
initialization, backstop deployment/activation, the initial drop, and final
binding and conservation verification. Soroban rent/resource costs are
simulated by each Stellar CLI command immediately before submission; the
configured inclusion-fee ceiling is ${INCLUSION_FEE} stroops per transaction.

This plan does not deploy any lending pools and does not mutate mainnet.
EOF
}

validate_command() {
    (cd "${ROOT_DIR}" && make build-v3)
    validate_inputs
    validate_predictions
    note "Validated pinned source, reproducible allocations, exact WASM artifacts, and deterministic addresses."
}

preflight_command() {
    validate_inputs
    validate_predictions
    require_clean_reviewed_main read-only
    load_wallet_config
    verify_existing_prerequisites
    if [[ -f "${STATE_FILE}" ]]; then
        die "mainnet V3 core state already exists; use status or the reviewed recovery path"
    fi
    assert_expected_contracts_unused
    note "Read-only mainnet V3 preflight passed. No transaction was submitted."
}

complete_deployment() {
    local phase
    phase="$(state_optional phase)"
    if [[ "${phase}" == "verified" ]]; then
        verify_live_deployment
        write_registry
        note "BLNT V3 mainnet core is already deployed and verified."
        return
    fi
    deploy_expected_contract deploy-v3-emitter "${EXPECTED_EMITTER}" \
        "${EMITTER_WASM}" "${EMITTER_SALT}" --initializer "${WALLET}" >/dev/null
    state_set phase emitter-deployed
    ensure_pool_wasm_uploaded
    deploy_expected_contract deploy-v3-factory "${EXPECTED_FACTORY}" \
        "${FACTORY_WASM}" "${FACTORY_SALT}" --pool_init_meta \
        "{\"backstop\":\"${EXPECTED_BACKSTOP}\",\"blnt_id\":\"${BLNT}\",\"pool_hash\":\"${POOL_SHA256}\"}" >/dev/null
    state_set phase factory-deployed
    deploy_expected_contract deploy-blnt-backfill "${EXPECTED_BACKFILL}" \
        "${BACKFILL_WASM}" "${BACKFILL_SALT}" \
        --legacy_blnd_token "${LEGACY_BLND}" --blnt_token "${BLNT}" \
        --claim_list "${CLAIM_LIST}" --grant_list "${GRANT_LIST}" >/dev/null
    state_set phase backfill-deployed
    ensure_blnt_admin_handoff
    state_set phase blnt-admin-handed-off
    ensure_emitter_initialized
    state_set phase emitter-initialized
    deploy_expected_contract deploy-v3-backstop "${EXPECTED_BACKSTOP}" \
        "${BACKSTOP_WASM}" "${BACKSTOP_SALT}" \
        --blnt_usdc_token "${BLNT_USDC}" --blnt_xlm_token "${BLNT_XLM}" \
        --emitter "${EXPECTED_EMITTER}" --blnt_token "${BLNT}" \
        --usdc_token "${USDC}" --xlm_token "${XLM}" \
        --pool_factory "${EXPECTED_FACTORY}" \
        --drop_list "[[\"${EXPECTED_BACKFILL}\",\"${INITIAL_DROP}\"]]" >/dev/null
    state_set phase backstop-deployed
    ensure_initial_drop
    verify_live_deployment
    state_set phase verified
    state_set verified_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    write_registry
    phase="$(state_value phase)"
    assert_equal verified "${phase}" "deployment phase"
    note "BLNT V3 mainnet core deployed, funded, and verified. No lending pool was deployed."
}

deploy_core_command() {
    validate_inputs
    validate_predictions
    require_mutation_confirmation
    [[ ! -f "${STATE_FILE}" ]] || die "state already exists; use mainnet-v3-activate"
    verify_existing_prerequisites
    assert_expected_contracts_unused
    initialize_core_state
    complete_deployment
}

activate_command() {
    validate_inputs
    validate_predictions
    require_mutation_confirmation
    load_core_state
    verify_existing_prerequisites resume
    complete_deployment
}

status_command() {
    if [[ ! -f "${STATE_FILE}" ]]; then
        deployment_plan
        note "No V3 mainnet core deployment state exists."
        return
    fi
    validate_backfill_manifest
    load_core_state
    cat <<EOF
BLNT V3 mainnet core deployment
  phase: $(state_value phase)
  emitter: $(state_value v3_emitter)
  backfill: $(state_value blnt_backfill)
  pool factory: $(state_value v3_factory)
  backstop: $(state_value v3_backstop)
  grant input SHA-256: $(jq -er '.private_grants.sha256' "${STATE_FILE}")
  registry: ${REGISTRY_FILE}
EOF
    if [[ "$(state_value phase)" == "verified" ]]; then
        verify_live_deployment
        note "Live mainnet V3 bindings and backfill conservation verified."
    fi
}

usage() {
    cat <<'EOF'
Usage: scripts/mainnet-v3-deployment.sh COMMAND

Commands:
  plan         Print immutable inputs and deterministic contract addresses.
  validate     Build and verify source pins, manifests, grants, and WASM bytes.
  preflight    Read-only validation of mainnet prerequisites and unused IDs.
  deploy-core  Deploy, activate, fund, verify, and record the V3 core.
  activate     Resume an interrupted reviewed core deployment from local state.
  status       Print local progress and reverify a completed live deployment.

deploy-core and activate require both:
  BLEND_MAINNET_V3_CONFIRM=DEPLOY_BLNT_V3_MAINNET
  BLEND_MAINNET_V3_SEQUENCE_LOCK_CONFIRMED=true

The mutating commands also require a clean main branch equal to origin/main and
refuse to run while another known process is using the deployment wallet.
EOF
}

case "${1:-}" in
    plan) deployment_plan ;;
    validate) validate_command ;;
    preflight) preflight_command ;;
    deploy-core) deploy_core_command ;;
    activate) activate_command ;;
    status) status_command ;;
    *) usage; exit 2 ;;
esac
