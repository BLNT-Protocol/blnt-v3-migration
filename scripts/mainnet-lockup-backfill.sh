#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="${ROOT_DIR}/blnt-lockup-backfill-contract"
MANIFEST="${SOURCE_DIR}/allocations/production.json"
MANAGER_REGISTRY="${ROOT_DIR}/deployments/mainnet-v3-yieldblent-backstop-manager-v1.0.1.json"
WASM="${BLNT_LOCKUP_BACKFILL_WASM:-}"
SOURCE_TAG="${BLNT_LOCKUP_BACKFILL_SOURCE_TAG:-}"
RELEASE_TAG="${BLNT_LOCKUP_BACKFILL_RELEASE_TAG:-}"
WORK_DIR="${BLNT_LOCKUP_BACKFILL_WORK_DIR:-${ROOT_DIR}/.mainnet-v3-lockup-backfill-v1.0.1}"
CONFIG_DIR="${BLNT_LOCKUP_BACKFILL_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${BLNT_LOCKUP_BACKFILL_STATE_FILE:-${WORK_DIR}/state.json}"
REGISTRY_FILE="${BLNT_LOCKUP_BACKFILL_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-v3-lockup-backfill-v1.0.1.json}"
ACCOUNT_CONFIG="${BLNT_LOCKUP_BACKFILL_ACCOUNT_CONFIG:-}"
RPC_URL="${BLNT_LOCKUP_BACKFILL_RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}"
HORIZON_URL="${BLNT_LOCKUP_BACKFILL_HORIZON_URL:-https://horizon.stellar.org}"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
INCLUSION_FEE="${BLNT_LOCKUP_BACKFILL_INCLUSION_FEE:-100000}"

DEPLOYER="${BLNT_LOCKUP_BACKFILL_DEPLOYER:-}"
OWNER="${BLNT_LOCKUP_BACKFILL_OWNER:-}"
BLNT="${BLNT_LOCKUP_BACKFILL_BLNT:-CCBNWP5L7ZSXA3DBWCMFAVKFVRW3ZHWO7B5J557W42O64DDZNPJP7VDS}"
SOURCE_BACKFILL="${BLNT_LOCKUP_BACKFILL_SOURCE:-CANIE64BDFKLHROLOIY2KTQVBWUZNUKZP75DLXC272QM7TIN5N3M4LVA}"
MANAGER="${BLNT_LOCKUP_BACKFILL_MANAGER:-CAJUD2IQOVKPDZFJZEX2MSG7PKIE2NF4CJQ2ERBU7VF52SEM5GQL7XEB}"
SALT="${BLNT_LOCKUP_BACKFILL_SALT:-149fd4c49b974c03e82fd107d1e80e59ee39b32cfd62eb87739476b7fd62a3c3}"
CONFIRM_MAINNET="${BLNT_LOCKUP_BACKFILL_CONFIRM_MAINNET:-false}"

EXPECTED_REPOSITORY="BLNT-Protocol/blnt-lockup-backfill-contract"
EXPECTED_PARENT_REPOSITORY="BLNT-Protocol/blnt-v3-migration"
EXPECTED_SOURCE_REPO="github:${EXPECTED_REPOSITORY}"
EXPECTED_SIGNER_WORKFLOW="stellar-expert/soroban-build-workflow/.github/workflows/release.yml"
EXPECTED_SIGNER_DIGEST="88068ec50cba931a96436869727ed08edeb76ade"
EXPECTED_MANAGER_SOURCE_REPO="github:BLNT-Protocol/backstop-manager"
EXPECTED_OPERATIONAL_MANAGER="CANSYFVMIP7JVYEZQ463Y2I2VLEVNLDJJ4QNZTDBGLOOGKURPTW4A6FQ"
EXPECTED_MANAGER_SCOPE=2
EXPECTED_ALLOCATION_COUNT=4
EXPECTED_ALLOCATION_TOTAL=37442405803143
VESTING_DURATION_SECONDS=31104000

RUN_DIR=""
LAST_TX_HASH=""
LAST_TX_CREATED_AT=""
LAST_TX_LEDGER=""
RECORDED_CONTRACT_ID=""
STELLAR_PUBLIC_KEY=""
STELLAR_SECRET_KEY=""
ALLOCATION_LIST=""
ALLOCATION_MANIFEST_SHA256=""
VESTING_START=""
VESTING_END=""
OWNER_GRANT_CLAIMABLE=""
MANAGER_ROLE=""
RELEASE_URL=""
RELEASE_ASSET_NAME=""
RELEASE_ASSET_URL=""
RELEASE_WORKFLOW_URL=""
RELEASE_ATTESTATION_COUNT=""
ARTIFACT_RUST_VERSION=""
ARTIFACT_SDK_VERSION=""
ARTIFACT_CLI_VERSION=""

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

require_address() {
    local value="$1" label="$2"
    [[ "${value}" =~ ^[CG][A-Z2-7]{55}$ ]] || die "invalid ${label}: ${value}"
}

sha256_file() {
    local value
    value="$(shasum -a 256 "$1")"
    printf '%s\n' "${value%% *}"
}

stellar_cli() {
    STELLAR_INCLUSION_FEE="${INCLUSION_FEE}" \
        stellar --config-dir "${CONFIG_DIR}" "$@"
}

wallet_stellar() {
    [[ -n "${STELLAR_SECRET_KEY}" ]] || die "mainnet deployment secret is not loaded"
    STELLAR_SIGN_WITH_KEY="${STELLAR_SECRET_KEY}" \
        STELLAR_INCLUSION_FEE="${INCLUSION_FEE}" \
        stellar --config-dir "${CONFIG_DIR}" "$@"
}

normalize_scalar() {
    local raw="$1" decoded
    if decoded="$(printf '%s' "${raw}" | jq -er \
        'if type == "string" or type == "number" then tostring else empty end' \
        2>/dev/null)"
    then
        printf '%s\n' "${decoded}"
    else
        printf '%s\n' "${raw}"
    fi
}

capture() {
    local label="$1" stdout_file stderr_file status
    shift
    [[ -n "${RUN_DIR}" ]] || die "run directory is not initialized"
    stdout_file="${RUN_DIR}/${label}.out"
    stderr_file="${RUN_DIR}/${label}.cost.log"
    note "Executing ${label}..."
    if "$@" >"${stdout_file}" 2> >(tee "${stderr_file}" >&2); then
        status=0
    else
        status=$?
    fi
    LAST_TX_HASH="$(sed -n 's/.*Signing transaction: *\([0-9a-f]\{64\}\).*/\1/p' \
        "${stderr_file}" | tail -1)"
    cat "${stdout_file}"
    return "${status}"
}

state_value() {
    jq -er --arg key "$1" '.[$key]' "${STATE_FILE}"
}

state_optional() {
    jq -er --arg key "$1" '.[$key] // empty' "${STATE_FILE}" 2>/dev/null || true
}

state_set() {
    local key="$1" value="$2" temporary="${STATE_FILE}.tmp"
    jq --arg key "${key}" --arg value "${value}" '.[$key] = $value' \
        "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
}

record_transaction() {
    local label="$1" temporary="${STATE_FILE}.tmp"
    [[ "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]] ||
        die "${label} completed without a captured transaction hash"
    jq --arg label "${label}" --arg hash "${LAST_TX_HASH}" \
        '.transactions[$label] = $hash' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
}

transaction_hash_from_log() {
    local label="$1" log_file
    log_file="${RUN_DIR}/${label}.cost.log"
    [[ -f "${log_file}" ]] || return 1
    sed -n 's/.*Signing transaction: *\([0-9a-f]\{64\}\).*/\1/p' \
        "${log_file}" | tail -1
}

fetch_successful_transaction() {
    local hash="$1" attempt response
    for attempt in 1 2 3 4 5; do
        if response="$(curl --fail --silent --show-error --max-time 15 \
            "${HORIZON_URL}/transactions/${hash}" 2>/dev/null)" &&
            printf '%s' "${response}" | jq -e \
                --arg hash "${hash}" --arg deployer "${DEPLOYER}" \
                '.hash == $hash and .source_account == $deployer and
                 .successful == true and
                 (.created_at | type == "string" and length > 0) and
                 (.ledger | type == "number")' >/dev/null
        then
            printf '%s\n' "${response}"
            return
        fi
        if (( attempt < 5 )); then
            sleep 2
        fi
    done
    die "unable to verify successful mainnet transaction ${hash} through Horizon"
}

record_successful_transaction() {
    local log_label="$1" state_label="$2" transaction
    if [[ ! "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]]; then
        LAST_TX_HASH="$(jq -er --arg label "${state_label}" \
            '.transactions[$label] // empty' "${STATE_FILE}" 2>/dev/null || true)"
    fi
    if [[ ! "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]]; then
        LAST_TX_HASH="$(transaction_hash_from_log "${log_label}" || true)"
    fi
    [[ "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]] ||
        die "${state_label} has no recoverable signed transaction hash"
    transaction="$(fetch_successful_transaction "${LAST_TX_HASH}")"
    LAST_TX_CREATED_AT="$(printf '%s' "${transaction}" | jq -er '.created_at')"
    LAST_TX_LEDGER="$(printf '%s' "${transaction}" | jq -er '.ledger | tostring')"
    record_transaction "${state_label}"
}

load_account_config() {
    [[ -n "${ACCOUNT_CONFIG}" ]] ||
        die "BLNT_LOCKUP_BACKFILL_ACCOUNT_CONFIG must identify the deployer account config"
    [[ -f "${ACCOUNT_CONFIG}" ]] || die "account config is missing: ${ACCOUNT_CONFIG}"
    # shellcheck disable=SC1090
    source "${ACCOUNT_CONFIG}"
    [[ "${STELLAR_PUBLIC_KEY:-}" == "${DEPLOYER}" ]] ||
        die "account config does not identify BLNT_LOCKUP_BACKFILL_DEPLOYER"
    [[ "${STELLAR_SECRET_KEY:-}" =~ ^S[A-Z2-7]{55}$ ]] ||
        die "account config does not contain a valid secret key"
}

fetch_contract_artifact() {
    local destination="$1" description="$2" attempt temporary_dir stderr_file stderr_output
    shift 2
    for attempt in 1 2 3; do
        temporary_dir="$(mktemp -d)"
        stderr_file="${temporary_dir}/fetch.err"
        rm -f "${destination}"
        if stellar_cli contract fetch "$@" \
            --out-file "${destination}" \
            --rpc-url "${RPC_URL}" \
            --network-passphrase "${NETWORK_PASSPHRASE}" \
            2>"${stderr_file}"
        then
            rm -r -- "${temporary_dir}"
            return 0
        fi
        stderr_output="$(<"${stderr_file}")"
        if [[ "${stderr_output}" == *"Contract not found"* ||
            "${stderr_output}" == *"Contract Code not found"* ||
            "${stderr_output}" == *"Wasm not found"* ||
            "${stderr_output}" == *"not found in ledger"* ]]
        then
            rm -r -- "${temporary_dir}"
            return 1
        fi
        if (( attempt == 3 )); then
            note "Failed to fetch ${description} after three attempts."
            sed -n '1,20p' "${stderr_file}" >&2
            rm -r -- "${temporary_dir}"
            die "unable to determine the on-chain state of ${description}"
        fi
        rm -r -- "${temporary_dir}"
    done
}

invoke_view() {
    local contract="$1" function="$2"
    shift 2
    stellar_cli contract invoke \
        --id "${contract}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --send no \
        -- "${function}" "$@"
}

verify_source_metadata() {
    local metadata
    metadata="$(stellar contract info meta --wasm "${WASM}" 2>/dev/null)"
    printf '%s' "${metadata}" | grep -Fq "source_repo: ${EXPECTED_SOURCE_REPO}" ||
        die "lockup-backfill WASM source_repo metadata mismatch"
    ARTIFACT_RUST_VERSION="$(printf '%s\n' "${metadata}" |
        sed -n 's/.*rsver: //p' | head -1)"
    ARTIFACT_SDK_VERSION="$(printf '%s\n' "${metadata}" |
        sed -n 's/.*rssdkver: //p' | head -1)"
    ARTIFACT_CLI_VERSION="$(printf '%s\n' "${metadata}" |
        sed -n 's/.*cliver: //p' | head -1)"
    [[ -n "${ARTIFACT_RUST_VERSION}" && -n "${ARTIFACT_SDK_VERSION}" &&
        -n "${ARTIFACT_CLI_VERSION}" ]] || die "release WASM build metadata is incomplete"
}

require_clean_source_checkout() {
    [[ -z "$(git -C "${SOURCE_DIR}" status --porcelain --untracked-files=normal)" ]] ||
        die "lockup-backfill source checkout is dirty; use the exact tagged source commit"
}

validate_manifest() {
    local count total
    make -C "${SOURCE_DIR}" validate-production-allocations >/dev/null
    ALLOCATION_LIST="$(make -s -C "${SOURCE_DIR}" production-allocation-args)"
    ALLOCATION_MANIFEST_SHA256="$(sha256_file "${MANIFEST}")"
    count="$(jq -er '.allocations | length' "${MANIFEST}")"
    total="$(jq -er '.allocation_total_raw' "${MANIFEST}")"
    [[ "${count}" == "${EXPECTED_ALLOCATION_COUNT}" ]] ||
        die "production allocation count mismatch"
    [[ "${total}" == "${EXPECTED_ALLOCATION_TOTAL}" ]] ||
        die "production allocation total mismatch"
    printf '%s' "${ALLOCATION_LIST}" | jq -e \
        --argjson count "${EXPECTED_ALLOCATION_COUNT}" \
        --arg total "${EXPECTED_ALLOCATION_TOTAL}" '
        length == $count and
        (map(.[0]) | unique | length) == $count and
        (map(.[1] | tonumber) | add | tostring) == $total' >/dev/null ||
        die "generated allocation constructor argument is invalid"
}

remote_tag_commit() {
    local refs direct peeled
    refs="$(git -C "${SOURCE_DIR}" ls-remote --tags origin \
        "refs/tags/${SOURCE_TAG}" "refs/tags/${SOURCE_TAG}^{}")"
    direct="$(printf '%s\n' "${refs}" | awk '$2 !~ /\^\{\}$/ {print $1; exit}')"
    peeled="$(printf '%s\n' "${refs}" | awk '$2 ~ /\^\{\}$/ {print $1; exit}')"
    printf '%s\n' "${peeled:-${direct}}"
}

verify_release_artifact() {
    local source_commit local_tag_commit remote_commit repository_json release_json
    local workflow_runs match_count temporary_dir candidate_path candidate_name
    local candidate_url candidate_digest artifact_sha attestation_json
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    local_tag_commit="$(git -C "${SOURCE_DIR}" rev-list -n 1 "${SOURCE_TAG}" 2>/dev/null || true)"
    [[ "${local_tag_commit}" == "${source_commit}" ]] ||
        die "source tag ${SOURCE_TAG} does not identify checked-out source commit ${source_commit}"
    remote_commit="$(remote_tag_commit)"
    [[ "${remote_commit}" == "${source_commit}" ]] ||
        die "remote source tag ${SOURCE_TAG} does not identify checked-out source commit"

    repository_json="$(gh api "repos/${EXPECTED_REPOSITORY}")"
    printf '%s' "${repository_json}" | jq -e \
        '.private == false and .archived == false and .disabled == false' >/dev/null ||
        die "canonical source repository is not public and active"

    release_json="$(gh api "repos/${EXPECTED_REPOSITORY}/releases/tags/${RELEASE_TAG}")"
    printf '%s' "${release_json}" | jq -e \
        '.draft == false and .prerelease == false' >/dev/null ||
        die "release ${RELEASE_TAG} is not a final public release"
    RELEASE_URL="$(printf '%s' "${release_json}" | jq -er '.html_url')"

    temporary_dir="$(mktemp -d)"
    match_count=0
    while IFS=$'\t' read -r candidate_name candidate_url candidate_digest; do
        [[ -n "${candidate_name}" && -n "${candidate_url}" ]] || continue
        candidate_path="${temporary_dir}/${candidate_name}"
        curl --fail --location --silent --show-error --max-time 60 \
            --output "${candidate_path}" "${candidate_url}"
        if cmp -s "${WASM}" "${candidate_path}"; then
            match_count=$((match_count + 1))
            RELEASE_ASSET_NAME="${candidate_name}"
            RELEASE_ASSET_URL="${candidate_url}"
            if [[ -n "${candidate_digest}" && "${candidate_digest}" != null ]]; then
                artifact_sha="$(sha256_file "${WASM}")"
                [[ "${candidate_digest}" == "sha256:${artifact_sha}" ]] || {
                    rm -r -- "${temporary_dir}"
                    die "GitHub release digest does not match the supplied WASM"
                }
            fi
        fi
    done < <(printf '%s' "${release_json}" | jq -r \
        '.assets[] | select(.name | endswith(".wasm")) |
         [.name, .browser_download_url, (.digest // "")] | @tsv')
    rm -r -- "${temporary_dir}"
    [[ "${match_count}" == 1 ]] ||
        die "supplied WASM must match exactly one public release asset"

    workflow_runs="$(gh api \
        "repos/${EXPECTED_REPOSITORY}/actions/workflows/release.yml/runs?per_page=100")"
    RELEASE_WORKFLOW_URL="$(printf '%s' "${workflow_runs}" | jq -er \
        --arg sha "${source_commit}" --arg branch "${SOURCE_TAG}" '
        [.workflow_runs[] |
         select(.head_sha == $sha and .head_branch == $branch and
                .event == "push" and .conclusion == "success")] |
        sort_by(.created_at) | last | .html_url // empty' 2>/dev/null || true)"
    [[ -n "${RELEASE_WORKFLOW_URL}" ]] ||
        die "no successful tag-triggered release workflow run matches ${SOURCE_TAG}"

    attestation_json="$(gh attestation verify "${WASM}" \
        --repo "${EXPECTED_REPOSITORY}" \
        --signer-workflow "${EXPECTED_SIGNER_WORKFLOW}" \
        --signer-digest "${EXPECTED_SIGNER_DIGEST}" \
        --source-digest "${source_commit}" \
        --source-ref "refs/tags/${SOURCE_TAG}" \
        --format json)"
    RELEASE_ATTESTATION_COUNT="$(printf '%s' "${attestation_json}" | jq -er 'length')"
    (( RELEASE_ATTESTATION_COUNT > 0 )) || die "release artifact has no valid attestation"
}

verify_manager_artifact() {
    local recorded_id recorded_hash recorded_source temporary_dir fetched
    [[ -f "${MANAGER_REGISTRY}" ]] || die "verified manager deployment record is missing"
    recorded_id="$(jq -er '.contract.contract_id' "${MANAGER_REGISTRY}")"
    recorded_hash="$(jq -er '.contract.wasm_sha256' "${MANAGER_REGISTRY}")"
    recorded_source="$(jq -er '.contract.source_repo' "${MANAGER_REGISTRY}")"
    [[ "${recorded_id}" == "${MANAGER}" ]] || die "manager registry contract mismatch"
    [[ "${recorded_source}" == "${EXPECTED_MANAGER_SOURCE_REPO}" ]] ||
        die "manager registry source mismatch"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/manager.wasm"
    fetch_contract_artifact "${fetched}" "backstop manager ${MANAGER}" --id "${MANAGER}" || {
        rm -r -- "${temporary_dir}"
        die "backstop manager is not deployed"
    }
    [[ "$(sha256_file "${fetched}")" == "${recorded_hash}" ]] || {
        rm -r -- "${temporary_dir}"
        die "live backstop-manager WASM does not match its deployment record"
    }
    stellar contract info meta --wasm "${fetched}" 2>/dev/null |
        grep -Fq "source_repo: ${EXPECTED_MANAGER_SOURCE_REPO}" || {
        rm -r -- "${temporary_dir}"
        die "live backstop-manager source metadata mismatch"
    }
    rm -r -- "${temporary_dir}"
}

latest_close_time() {
    curl --fail --silent --show-error --max-time 15 \
        -H 'Content-Type: application/json' \
        --data '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}' \
        "${RPC_URL}" | jq -er '.result.closeTime | tonumber'
}

validate_live_bindings() {
    local decimals actual_blnt manager_owner latest
    decimals="$(normalize_scalar "$(invoke_view "${BLNT}" decimals)")"
    [[ "${decimals}" == 7 ]] || die "BLNT decimals mismatch"
    actual_blnt="$(normalize_scalar "$(invoke_view "${SOURCE_BACKFILL}" get_blnt_token)")"
    [[ "${actual_blnt}" == "${BLNT}" ]] || die "source backfill BLNT binding mismatch"
    VESTING_START="$(normalize_scalar \
        "$(invoke_view "${SOURCE_BACKFILL}" get_grant_vesting_start)")"
    VESTING_END="$(normalize_scalar \
        "$(invoke_view "${SOURCE_BACKFILL}" get_grant_vesting_end)")"
    [[ "${VESTING_START}" =~ ^[0-9]+$ && "${VESTING_END}" =~ ^[0-9]+$ ]] ||
        die "source vesting timestamps are invalid"
    [[ $((VESTING_END - VESTING_START)) == "${VESTING_DURATION_SECONDS}" ]] ||
        die "source contributor grant does not use the required 360-day vesting epoch"
    latest="$(latest_close_time)"
    (( VESTING_START <= latest )) || die "source vesting epoch has not started"

    verify_manager_artifact
    manager_owner="$(normalize_scalar "$(invoke_view "${MANAGER}" owner)")"
    [[ "${manager_owner}" == "${OWNER}" ]] ||
        die "backstop-manager owner does not match the proposed immutable owner"
    MANAGER_ROLE="$(invoke_view "${MANAGER}" manager)"
    printf '%s' "${MANAGER_ROLE}" | jq -e \
        --arg id "${EXPECTED_OPERATIONAL_MANAGER}" \
        --argjson scope "${EXPECTED_MANAGER_SCOPE}" \
        '.id == $id and .scope == $scope' >/dev/null ||
        die "backstop-manager operational role does not match YieldBlox DAO scope 2"

    OWNER_GRANT_CLAIMABLE="$(normalize_scalar "$(invoke_view "${SOURCE_BACKFILL}" \
        get_grant_claimable --grantee "${OWNER}")")"
    [[ "${OWNER_GRANT_CLAIMABLE}" =~ ^[0-9]+$ ]] || die "owner grant view is invalid"
}

require_owner_grant_claimable() {
    (( OWNER_GRANT_CLAIMABLE > 0 )) ||
        die "proposed owner has no currently claimable contributor grant"
}

predict_contract_id() {
    normalize_scalar "$(stellar_cli contract id wasm \
        --salt "${SALT}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
}

validate_inputs() {
    require_command awk
    require_command cmp
    require_command curl
    require_command gh
    require_command git
    require_command grep
    require_command jq
    require_command make
    require_command sed
    require_command shasum
    require_command stellar
    require_command wc
    [[ -d "${SOURCE_DIR}" ]] || die "lockup-backfill submodule is missing"
    [[ -s "${WASM}" ]] ||
        die "BLNT_LOCKUP_BACKFILL_WASM must identify the downloaded release WASM"
    [[ -n "${SOURCE_TAG}" ]] || die "BLNT_LOCKUP_BACKFILL_SOURCE_TAG is required"
    [[ -n "${RELEASE_TAG}" ]] || die "BLNT_LOCKUP_BACKFILL_RELEASE_TAG is required"
    [[ -n "${DEPLOYER}" ]] || die "BLNT_LOCKUP_BACKFILL_DEPLOYER is required"
    [[ -n "${OWNER}" ]] || die "BLNT_LOCKUP_BACKFILL_OWNER is required"
    [[ "${DEPLOYER}" =~ ^G[A-Z2-7]{55}$ ]] || die "invalid deployment source account"
    require_address "${OWNER}" owner
    require_address "${BLNT}" BLNT
    require_address "${SOURCE_BACKFILL}" "source backfill"
    require_address "${MANAGER}" "backstop manager"
    [[ "${SALT}" =~ ^[0-9a-f]{64}$ ]] ||
        die "deployment salt must be 64 lowercase hex characters"
    [[ "${INCLUSION_FEE}" =~ ^[0-9]+$ ]] ||
        die "inclusion fee must be a nonnegative integer"
    mkdir -p "${WORK_DIR}" "${CONFIG_DIR}"
    chmod 700 "${WORK_DIR}" "${CONFIG_DIR}"
    require_clean_source_checkout
    verify_source_metadata
    validate_manifest
    verify_release_artifact
    validate_live_bindings
}

require_clean_reviewed_source() {
    local parent_commit parent_json pinned_commit source_commit
    [[ -z "$(git -C "${ROOT_DIR}" status --porcelain --untracked-files=normal)" ]] ||
        die "parent checkout is dirty; deploy only reviewed commits"
    require_clean_source_checkout
    parent_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    parent_json="$(gh api "repos/${EXPECTED_PARENT_REPOSITORY}/commits/${parent_commit}" \
        2>/dev/null)" ||
        die "parent commit is not available from ${EXPECTED_PARENT_REPOSITORY}"
    printf '%s' "${parent_json}" | jq -e --arg sha "${parent_commit}" \
        '.sha == $sha' >/dev/null ||
        die "parent commit is not available from ${EXPECTED_PARENT_REPOSITORY}"
    pinned_commit="$(git -C "${ROOT_DIR}" ls-tree HEAD blnt-lockup-backfill-contract | awk '{print $3}')"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    [[ -n "${pinned_commit}" && "${pinned_commit}" == "${source_commit}" ]] ||
        die "parent commit does not pin the checked-out lockup-backfill commit"
}

initialize_state() {
    local expected_contract source_commit parent_commit artifact_sha256 artifact_size
    expected_contract="$(predict_contract_id)"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    parent_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    artifact_sha256="$(sha256_file "${WASM}")"
    artifact_size="$(wc -c <"${WASM}" | tr -d ' ')"
    RUN_DIR="${WORK_DIR}/runs/$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "${RUN_DIR}"
    jq -n \
        --arg network_passphrase "${NETWORK_PASSPHRASE}" \
        --arg rpc_url "${RPC_URL}" \
        --arg deployer "${DEPLOYER}" \
        --arg owner "${OWNER}" \
        --arg blnt "${BLNT}" \
        --arg source_backfill "${SOURCE_BACKFILL}" \
        --arg manager "${MANAGER}" \
        --arg salt "${SALT}" \
        --arg expected_contract "${expected_contract}" \
        --arg artifact_sha256 "${artifact_sha256}" \
        --arg artifact_size "${artifact_size}" \
        --arg source_repo "${EXPECTED_SOURCE_REPO}" \
        --arg source_commit "${source_commit}" \
        --arg source_tag "${SOURCE_TAG}" \
        --arg release_tag "${RELEASE_TAG}" \
        --arg release_url "${RELEASE_URL}" \
        --arg release_asset_name "${RELEASE_ASSET_NAME}" \
        --arg release_asset_url "${RELEASE_ASSET_URL}" \
        --arg release_workflow_url "${RELEASE_WORKFLOW_URL}" \
        --arg release_attestation_count "${RELEASE_ATTESTATION_COUNT}" \
        --arg artifact_rust_version "${ARTIFACT_RUST_VERSION}" \
        --arg artifact_sdk_version "${ARTIFACT_SDK_VERSION}" \
        --arg artifact_cli_version "${ARTIFACT_CLI_VERSION}" \
        --arg allocation_manifest_sha256 "${ALLOCATION_MANIFEST_SHA256}" \
        --arg allocation_total "${EXPECTED_ALLOCATION_TOTAL}" \
        --arg vesting_start "${VESTING_START}" \
        --arg vesting_end "${VESTING_END}" \
        --arg owner_grant_claimable "${OWNER_GRANT_CLAIMABLE}" \
        --argjson allocation_list "${ALLOCATION_LIST}" \
        --argjson manager_role "${MANAGER_ROLE}" \
        --arg parent_commit "${parent_commit}" \
        --arg run_dir "${RUN_DIR}" \
        --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '
        {
            schema_version: 1,
            network_passphrase: $network_passphrase,
            rpc_url: $rpc_url,
            deployer: $deployer,
            owner: $owner,
            blnt: $blnt,
            source_backfill: $source_backfill,
            manager: $manager,
            manager_role: $manager_role,
            salt: $salt,
            contract_id: $expected_contract,
            artifact: {
                sha256: $artifact_sha256,
                size_bytes: ($artifact_size | tonumber),
                source_repo: $source_repo,
                source_commit: $source_commit,
                source_tag: $source_tag,
                release_tag: $release_tag,
                release_url: $release_url,
                release_asset_name: $release_asset_name,
                release_asset_url: $release_asset_url,
                release_workflow_url: $release_workflow_url,
                attestation_count: ($release_attestation_count | tonumber),
                rust_version: $artifact_rust_version,
                soroban_sdk_version: $artifact_sdk_version,
                build_stellar_cli_version: $artifact_cli_version,
                target: "wasm32v1-none",
                optimization: "Cargo release opt-level=z, LTO, codegen-units=1; release-workflow output"
            },
            allocations: {
                manifest_sha256: $allocation_manifest_sha256,
                count: ($allocation_list | length),
                total_raw: $allocation_total,
                constructor_arg: $allocation_list
            },
            vesting: {
                start: ($vesting_start | tonumber),
                end: ($vesting_end | tonumber)
            },
            owner_grant_claimable_at_plan: $owner_grant_claimable,
            parent_commit: $parent_commit,
            run_dir: $run_dir,
            created_at: $created_at,
            phase: "planned",
            transactions: {}
        }' >"${STATE_FILE}"
}

assert_expected_contract_unused() {
    local contract_id temporary_dir fetched
    contract_id="$(state_value contract_id)"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    if fetch_contract_artifact "${fetched}" "predicted contract ${contract_id}" \
        --id "${contract_id}"
    then
        rm -r -- "${temporary_dir}"
        die "predicted contract already exists without local deployment state"
    fi
    rm -r -- "${temporary_dir}"
}

load_state() {
    [[ -f "${STATE_FILE}" ]] || die "deployment state does not exist: ${STATE_FILE}"
    RUN_DIR="$(state_value run_dir)"
    [[ -d "${RUN_DIR}" ]] || die "recorded run directory is missing"
    [[ "$(state_value network_passphrase)" == "${NETWORK_PASSPHRASE}" ]] ||
        die "state network mismatch"
    [[ "$(state_value deployer)" == "${DEPLOYER}" ]] || die "state deployer mismatch"
    [[ "$(state_value owner)" == "${OWNER}" ]] || die "state owner mismatch"
    [[ "$(state_value blnt)" == "${BLNT}" ]] || die "state BLNT mismatch"
    [[ "$(state_value source_backfill)" == "${SOURCE_BACKFILL}" ]] ||
        die "state source-backfill mismatch"
    [[ "$(state_value manager)" == "${MANAGER}" ]] || die "state manager mismatch"
    [[ "$(state_value salt)" == "${SALT}" ]] || die "state salt mismatch"
    [[ "$(state_value contract_id)" == "$(predict_contract_id)" ]] ||
        die "state contract ID mismatch"
    [[ "$(jq -er '.artifact.source_commit' "${STATE_FILE}")" == \
        "$(git -C "${SOURCE_DIR}" rev-parse HEAD)" ]] || die "state source commit mismatch"
    [[ "$(jq -er '.artifact.source_tag' "${STATE_FILE}")" == "${SOURCE_TAG}" ]] ||
        die "state source tag mismatch"
    [[ "$(jq -er '.artifact.release_tag' "${STATE_FILE}")" == "${RELEASE_TAG}" ]] ||
        die "state release tag mismatch"
    [[ "$(jq -er '.artifact.sha256' "${STATE_FILE}")" == "$(sha256_file "${WASM}")" ]] ||
        die "state artifact mismatch"
    [[ "$(jq -er '.allocations.manifest_sha256' "${STATE_FILE}")" == \
        "${ALLOCATION_MANIFEST_SHA256}" ]] || die "state allocation manifest mismatch"
}

load_status_inputs() {
    local source
    if [[ -f "${STATE_FILE}" ]]; then
        source="${STATE_FILE}"
    elif [[ -f "${REGISTRY_FILE}" ]]; then
        source="${REGISTRY_FILE}"
    else
        die "status requires local deployment state or ${REGISTRY_FILE}"
    fi
    [[ -n "${DEPLOYER}" ]] || DEPLOYER="$(jq -er '.deployer' "${source}")"
    [[ -n "${OWNER}" ]] || OWNER="$(jq -er '.constructor.owner // .owner' "${source}")"
    [[ -n "${SOURCE_TAG}" ]] || SOURCE_TAG="$(jq -er '.contract.source_tag // .artifact.source_tag' "${source}")"
    [[ -n "${RELEASE_TAG}" ]] || RELEASE_TAG="$(jq -er '.contract.release_tag // .artifact.release_tag' "${source}")"
    RECORDED_CONTRACT_ID="$(jq -er '.contract.contract_id // .contract_id' "${source}")"
}

ensure_artifact_uploaded() {
    local artifact_sha256 temporary_dir fetched uploaded
    artifact_sha256="$(sha256_file "${WASM}")"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/lockup-backfill.wasm"
    if fetch_contract_artifact "${fetched}" "lockup-backfill WASM ${artifact_sha256}" \
        --wasm-hash "${artifact_sha256}"
    then
        cmp -s "${WASM}" "${fetched}" || {
            rm -r -- "${temporary_dir}"
            die "uploaded WASM bytes do not match the attested release artifact"
        }
        rm -r -- "${temporary_dir}"
        if [[ -f "${RUN_DIR}/upload-lockup-backfill.cost.log" ]] ||
            jq -e '.transactions.upload_wasm | type == "string"' \
                "${STATE_FILE}" >/dev/null 2>&1
        then
            LAST_TX_HASH=""
            record_successful_transaction upload-lockup-backfill upload_wasm
        fi
        state_set phase artifact-uploaded
        return
    fi
    rm -r -- "${temporary_dir}"
    capture upload-lockup-backfill wallet_stellar contract upload \
        --wasm "${WASM}" \
        --optimize=false \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost >/dev/null
    uploaded="$(normalize_scalar "$(<"${RUN_DIR}/upload-lockup-backfill.out")")"
    [[ "${uploaded}" == "${artifact_sha256}" ]] || die "uploaded WASM hash mismatch"
    record_successful_transaction upload-lockup-backfill upload_wasm
    state_set phase artifact-uploaded
}

ensure_contract_deployed() {
    local contract_id temporary_dir fetched deployed
    contract_id="$(state_value contract_id)"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    if fetch_contract_artifact "${fetched}" "lockup-backfill contract ${contract_id}" \
        --id "${contract_id}"
    then
        cmp -s "${WASM}" "${fetched}" || {
            rm -r -- "${temporary_dir}"
            die "predicted address contains unexpected WASM"
        }
        rm -r -- "${temporary_dir}"
        [[ "$(state_optional phase)" != planned ]] ||
            die "predicted contract exists without local upload/deployment evidence"
        LAST_TX_HASH=""
        record_successful_transaction deploy-lockup-backfill deploy_contract
        state_set deployed_at "${LAST_TX_CREATED_AT}"
        state_set deployment_ledger "${LAST_TX_LEDGER}"
        state_set phase contract-deployed
        return
    fi
    rm -r -- "${temporary_dir}"
    capture deploy-lockup-backfill wallet_stellar contract deploy \
        --wasm-hash "$(sha256_file "${WASM}")" \
        --salt "${SALT}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost -- \
        --blnt "${BLNT}" \
        --source_backfill "${SOURCE_BACKFILL}" \
        --owner "${OWNER}" \
        --manager "${MANAGER}" \
        --allocation_list "${ALLOCATION_LIST}" >/dev/null
    deployed="$(normalize_scalar "$(<"${RUN_DIR}/deploy-lockup-backfill.out")")"
    [[ "${deployed}" == "${contract_id}" ]] || die "deployed contract ID mismatch"
    record_successful_transaction deploy-lockup-backfill deploy_contract
    state_set deployed_at "${LAST_TX_CREATED_AT}"
    state_set deployment_ledger "${LAST_TX_LEDGER}"
    state_set phase contract-deployed
}

recover_deployment_evidence() {
    local contract_id temporary_dir fetched
    contract_id="$(state_value contract_id)"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    fetch_contract_artifact "${fetched}" "lockup-backfill contract ${contract_id}" \
        --id "${contract_id}" || {
        rm -r -- "${temporary_dir}"
        die "record recovery found no deployed contract at ${contract_id}"
    }
    cmp -s "${WASM}" "${fetched}" || {
        rm -r -- "${temporary_dir}"
        die "deployed contract does not match the attested release WASM"
    }
    rm -r -- "${temporary_dir}"
    LAST_TX_HASH=""
    record_successful_transaction deploy-lockup-backfill deploy_contract
    state_set deployed_at "${LAST_TX_CREATED_AT}"
    state_set deployment_ledger "${LAST_TX_LEDGER}"
    state_set phase contract-deployed
}

verify_live_contract() {
    local expected_state="${1:-current}"
    local contract_id temporary_dir fetched config allocations totals claims_closed
    contract_id="$(predict_contract_id)"
    if [[ -n "${RECORDED_CONTRACT_ID}" ]]; then
        [[ "${contract_id}" == "${RECORDED_CONTRACT_ID}" ]] ||
            die "recorded contract ID does not match deployer and salt"
    fi
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    fetch_contract_artifact "${fetched}" "lockup-backfill contract ${contract_id}" \
        --id "${contract_id}" || {
        rm -r -- "${temporary_dir}"
        die "lockup-backfill contract is not deployed at ${contract_id}"
    }
    cmp -s "${WASM}" "${fetched}" || {
        rm -r -- "${temporary_dir}"
        die "deployed contract does not match the attested release WASM"
    }
    stellar contract info meta --wasm "${fetched}" 2>/dev/null |
        grep -Fq "source_repo: ${EXPECTED_SOURCE_REPO}" || {
        rm -r -- "${temporary_dir}"
        die "deployed contract source metadata mismatch"
    }
    rm -r -- "${temporary_dir}"

    config="$(invoke_view "${contract_id}" get_config)"
    printf '%s' "${config}" | jq -e \
        --arg blnt "${BLNT}" \
        --arg owner "${OWNER}" \
        --arg manager "${MANAGER}" \
        --arg source_backfill "${SOURCE_BACKFILL}" \
        --arg total "${EXPECTED_ALLOCATION_TOTAL}" \
        --arg start "${VESTING_START}" \
        --arg end "${VESTING_END}" '
        .blnt == $blnt and .owner == $owner and .manager == $manager and
        .source_backfill == $source_backfill and
        (.total_allocation | tostring) == $total and
        (.vesting_start | tostring) == $start and
        (.vesting_end | tostring) == $end' >/dev/null ||
        die "constructor configuration readback mismatch"

    allocations="$(invoke_view "${contract_id}" get_allocations)"
    jq -n -e --argjson actual "${allocations}" --argjson expected "${ALLOCATION_LIST}" '
        ($actual | map([.[0], (.[1] | tostring)]) | sort) ==
        ($expected | map([.[0], (.[1] | tostring)]) | sort)' >/dev/null ||
        die "allocation readback mismatch"

    totals="$(invoke_view "${contract_id}" get_totals)"
    claims_closed="$(normalize_scalar "$(invoke_view "${contract_id}" claims_closed)")"
    if [[ "${expected_state}" == initial ]]; then
        printf '%s' "${totals}" | jq -e --arg total "${EXPECTED_ALLOCATION_TOTAL}" '
            (.allocated | tostring) == $total and
            (.claimed | tostring) == "0" and
            (.custody | tostring) == "0" and
            (.funding_gap | tostring) == $total and
            (.outstanding | tostring) == $total' >/dev/null ||
            die "initial totals readback mismatch"
        [[ "${claims_closed}" == false ]] || die "claims unexpectedly initialized closed"
    else
        printf '%s' "${totals}" | jq -e \
            --argjson total "${EXPECTED_ALLOCATION_TOTAL}" \
            --argjson closed "${claims_closed}" '
            (.allocated | tonumber) == $total and
            (.claimed | tonumber) >= 0 and
            (.claimed | tonumber) <= $total and
            (.outstanding | tonumber) == ($total - (.claimed | tonumber)) and
            (.custody | tonumber) >= 0 and
            (.custody | tonumber) <= (.outstanding | tonumber) and
            (.funding_gap | tonumber) ==
                ((.outstanding | tonumber) - (.custody | tonumber)) and
            (($closed == false) or ($closed == true))' >/dev/null ||
            die "live totals accounting mismatch"
    fi
    printf '%s\n' "${contract_id}"
}

write_registry() {
    local temporary="${REGISTRY_FILE}.tmp" verified_at
    [[ -f "${STATE_FILE}" ]] || die "deployment state is required to write the registry"
    [[ "$(state_optional deployed_at)" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T ]] ||
        die "deployment record is missing a verified deployment timestamp"
    [[ "$(state_optional deployment_ledger)" =~ ^[0-9]+$ ]] ||
        die "deployment record is missing a verified deployment ledger"
    jq -e '.transactions.deploy_contract | type == "string" and
        test("^[0-9a-f]{64}$")' "${STATE_FILE}" >/dev/null ||
        die "deployment record is missing a verified deployment transaction hash"
    verified_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    jq \
        --arg verified_at "${verified_at}" \
        --arg stellar_cli_version "$(stellar --version | head -1)" '
        {
            schema_version: 1,
            network: {
                name: "Public Global Stellar Network",
                passphrase: .network_passphrase
            },
            purpose: "Replacement BLNT backfill for pre-incident Blend lockup owners",
            deployed_at,
            deployment_ledger: (.deployment_ledger | tonumber),
            deployer,
            contract: {
                contract_id,
                salt,
                wasm_sha256: .artifact.sha256,
                wasm_size_bytes: .artifact.size_bytes,
                source_repo: .artifact.source_repo,
                source_commit: .artifact.source_commit,
                source_tag: .artifact.source_tag,
                release_tag: .artifact.release_tag,
                release_url: .artifact.release_url,
                artifact_name: .artifact.release_asset_name,
                artifact_url: .artifact.release_asset_url,
                workflow_run_url: .artifact.release_workflow_url,
                rust_version: .artifact.rust_version,
                soroban_sdk_version: .artifact.soroban_sdk_version,
                build_stellar_cli_version: .artifact.build_stellar_cli_version,
                target: .artifact.target,
                optimization: .artifact.optimization,
                stellar_cli_version: $stellar_cli_version,
                parent_commit
            },
            constructor: {
                blnt,
                source_backfill,
                owner,
                manager,
                allocation_list: .allocations.constructor_arg
            },
            allocations: {
                manifest_sha256: .allocations.manifest_sha256,
                count: .allocations.count,
                total_raw: .allocations.total_raw
            },
            vesting,
            transactions,
            verification: {
                verified_at: $verified_at,
                artifact_matches: true,
                github_attestation_count: .artifact.attestation_count,
                attestation_predicate_type: "https://slsa.dev/provenance/v1",
                attestation_source_ref: ("refs/tags/" + .artifact.source_tag),
                attestation_signer_repository: "stellar-expert/soroban-build-workflow",
                attestation_signer_workflow: "stellar-expert/soroban-build-workflow/.github/workflows/release.yml",
                attestation_signer_digest: "88068ec50cba931a96436869727ed08edeb76ade",
                source_repo_metadata: .artifact.source_repo,
                constructor_readback_matches: true,
                allocations_match: true,
                initial_state_matches: true
            }
        }' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${REGISTRY_FILE}"
    state_set verified_at "${verified_at}"
    state_set phase verified
}

plan_command() {
    local contract_id artifact_sha256 artifact_size source_commit
    validate_inputs
    require_owner_grant_claimable
    contract_id="$(predict_contract_id)"
    artifact_sha256="$(sha256_file "${WASM}")"
    artifact_size="$(wc -c <"${WASM}" | tr -d ' ')"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    cat <<EOF
BLNT lockup-backfill mainnet deployment plan
  Source: ${EXPECTED_SOURCE_REPO} at ${source_commit}
  Source tag: ${SOURCE_TAG}
  Release: ${RELEASE_TAG}
  Release URL: ${RELEASE_URL}
  Release workflow: ${RELEASE_WORKFLOW_URL}
  Release artifact: ${RELEASE_ASSET_NAME}
  Artifact: ${artifact_sha256} (${artifact_size} bytes)
  Valid attestations: ${RELEASE_ATTESTATION_COUNT}
  Deployer: ${DEPLOYER}
  Predicted contract: ${contract_id}
  BLNT: ${BLNT}
  Source backfill: ${SOURCE_BACKFILL}
  Immutable owner: ${OWNER}
  Backstop manager: ${MANAGER}
  Operational manager: $(printf '%s' "${MANAGER_ROLE}" | jq -c '.')
  Source vesting epoch: ${VESTING_START} through ${VESTING_END}
  Owner grant currently claimable: ${OWNER_GRANT_CLAIMABLE} raw BLNT
  Allocation manifest: ${ALLOCATION_MANIFEST_SHA256}
  Allocations: ${EXPECTED_ALLOCATION_COUNT} beneficiaries / ${EXPECTED_ALLOCATION_TOTAL} raw BLNT
  Salt: ${SALT}

This command did not submit a transaction.
EOF
}

deploy_command() {
    validate_inputs
    [[ "${CONFIRM_MAINNET}" == true ]] ||
        die "set BLNT_LOCKUP_BACKFILL_CONFIRM_MAINNET=true after reviewing the plan"
    require_clean_reviewed_source
    load_account_config
    if [[ -f "${STATE_FILE}" ]]; then
        load_state
    else
        require_owner_grant_claimable
        initialize_state
    fi
    if [[ "$(state_optional phase)" == planned ]]; then
        assert_expected_contract_unused
    fi
    ensure_artifact_uploaded
    ensure_contract_deployed
    verify_live_contract initial >/dev/null
    write_registry
    note "Deployed and verified lockup-backfill $(state_value contract_id)."
    note "Deployment record written to ${REGISTRY_FILE}."
}

record_command() {
    load_status_inputs
    validate_inputs
    load_state
    recover_deployment_evidence
    verify_live_contract initial >/dev/null
    write_registry
    note "Verified deployed lockup-backfill $(state_value contract_id)."
    note "Deployment record written to ${REGISTRY_FILE}."
}

status_command() {
    load_status_inputs
    validate_inputs
    verify_live_contract current >/dev/null
    note "Verified deployed lockup-backfill $(predict_contract_id)."
}

case "${1:-}" in
    plan)
        plan_command
        ;;
    deploy)
        deploy_command
        ;;
    record)
        record_command
        ;;
    status)
        status_command
        ;;
    *)
        die "usage: $0 {plan|deploy|record|status}"
        ;;
esac
