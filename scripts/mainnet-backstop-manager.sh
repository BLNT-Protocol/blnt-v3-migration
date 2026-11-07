#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="${ROOT_DIR}/backstop-manager"
WASM="${SOURCE_DIR}/target/wasm32v1-none/optimized/backstop_manager.wasm"
WORK_DIR="${BLNT_BACKSTOP_MANAGER_WORK_DIR:-${ROOT_DIR}/.mainnet-v3-backstop-manager}"
CONFIG_DIR="${BLNT_BACKSTOP_MANAGER_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${BLNT_BACKSTOP_MANAGER_STATE_FILE:-${WORK_DIR}/state.json}"
REGISTRY_FILE="${BLNT_BACKSTOP_MANAGER_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-v3-yieldblent-backstop-manager.json}"
ACCOUNT_CONFIG="${BLNT_BACKSTOP_MANAGER_ACCOUNT_CONFIG:-}"
RPC_URL="${BLNT_BACKSTOP_MANAGER_RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}"
HORIZON_URL="${BLNT_BACKSTOP_MANAGER_HORIZON_URL:-https://horizon.stellar.org}"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
INCLUSION_FEE="${BLNT_BACKSTOP_MANAGER_INCLUSION_FEE:-100000}"

DEPLOYER="${BLNT_BACKSTOP_MANAGER_DEPLOYER:-}"
OWNER="${BLNT_BACKSTOP_MANAGER_OWNER:-}"
MANAGER="${BLNT_BACKSTOP_MANAGER_MANAGER:-CANSYFVMIP7JVYEZQ463Y2I2VLEVNLDJJ4QNZTDBGLOOGKURPTW4A6FQ}"
BACKSTOP="${BLNT_BACKSTOP_MANAGER_BACKSTOP:-CAHCUKBPTW5DR5RNXOHCTJD5U76A6EEZXYZAQFRQEEQRXQ3CRUZ6Z3MO}"
POOL="${BLNT_BACKSTOP_MANAGER_POOL:-CAUTPSQDAJEUV5XEU3EXFVYRGDJLEJGVTFJCFZCSPTDCREAMZ573VF4D}"
ADMIN_SCOPE="${BLNT_BACKSTOP_MANAGER_ADMIN_SCOPE:-2}"
SALT="${BLNT_BACKSTOP_MANAGER_SALT:-b1abafbe3f79604933f76603cfa27cae6bd900c8e777bfb1e1af9e2a19e1482a}"
CONFIRM_MAINNET="${BLNT_BACKSTOP_MANAGER_CONFIRM_MAINNET:-false}"

EXPECTED_SOURCE_REPO="github:BLNT-Protocol/backstop-manager"
RUN_DIR=""
LAST_TX_HASH=""
LAST_TX_CREATED_AT=""
RECORDED_CONTRACT_ID=""
STELLAR_PUBLIC_KEY=""
STELLAR_SECRET_KEY=""

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
                 (.created_at | type == "string" and length > 0)' >/dev/null
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
    local log_label="$1" state_label="$2" hash transaction
    if [[ ! "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]]; then
        LAST_TX_HASH="$(jq -er --arg label "${state_label}" \
            '.transactions[$label] // empty' "${STATE_FILE}" 2>/dev/null || true)"
    fi
    if [[ ! "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]]; then
        LAST_TX_HASH="$(transaction_hash_from_log "${log_label}" || true)"
    fi
    [[ "${LAST_TX_HASH}" =~ ^[0-9a-f]{64}$ ]] ||
        die "${state_label} has no recoverable signed transaction hash"
    hash="${LAST_TX_HASH}"
    transaction="$(fetch_successful_transaction "${hash}")"
    LAST_TX_CREATED_AT="$(printf '%s' "${transaction}" | jq -er '.created_at')"
    record_transaction "${state_label}"
}

load_account_config() {
    [[ -n "${ACCOUNT_CONFIG}" ]] ||
        die "BLNT_BACKSTOP_MANAGER_ACCOUNT_CONFIG must identify the deployer account config"
    [[ -f "${ACCOUNT_CONFIG}" ]] || die "account config is missing: ${ACCOUNT_CONFIG}"
    # shellcheck disable=SC1090
    source "${ACCOUNT_CONFIG}"
    [[ "${STELLAR_PUBLIC_KEY:-}" == "${DEPLOYER}" ]] ||
        die "account config does not identify BLNT_BACKSTOP_MANAGER_DEPLOYER"
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

build_artifact() {
    make -C "${SOURCE_DIR}" build
    [[ -s "${WASM}" ]] || die "backstop-manager WASM was not built"
}

verify_source_metadata() {
    stellar contract info meta --wasm "${WASM}" 2>/dev/null |
        grep -Fq "source_repo: ${EXPECTED_SOURCE_REPO}" ||
        die "backstop-manager WASM source_repo metadata mismatch"
}

predict_contract_id() {
    normalize_scalar "$(stellar_cli contract id wasm \
        --salt "${SALT}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
}

validate_inputs() {
    require_command cmp
    require_command curl
    require_command git
    require_command grep
    require_command jq
    require_command make
    require_command sed
    require_command shasum
    require_command stellar
    [[ -d "${SOURCE_DIR}" ]] || die "backstop-manager submodule is missing"
    [[ -n "${DEPLOYER}" ]] || die "BLNT_BACKSTOP_MANAGER_DEPLOYER is required"
    [[ -n "${OWNER}" ]] || die "BLNT_BACKSTOP_MANAGER_OWNER is required"
    [[ "${DEPLOYER}" =~ ^G[A-Z2-7]{55}$ ]] || die "invalid deployment source account"
    require_address "${OWNER}" owner
    require_address "${MANAGER}" manager
    require_address "${BACKSTOP}" backstop
    require_address "${POOL}" pool
    [[ "${ADMIN_SCOPE}" == 2 ]] || die "YieldBlent manager deployment requires high scope 2"
    [[ "${SALT}" =~ ^[0-9a-f]{64}$ ]] || die "deployment salt must be 64 lowercase hex characters"
    [[ "${INCLUSION_FEE}" =~ ^[0-9]+$ ]] || die "inclusion fee must be a nonnegative integer"
    mkdir -p "${WORK_DIR}" "${CONFIG_DIR}"
    chmod 700 "${WORK_DIR}" "${CONFIG_DIR}"
    build_artifact
    verify_source_metadata
}

require_clean_reviewed_source() {
    local pinned_commit source_commit
    git -C "${ROOT_DIR}" diff --quiet --ignore-submodules=none ||
        die "parent working tree is dirty; deploy only reviewed commits"
    git -C "${ROOT_DIR}" diff --cached --quiet --ignore-submodules=none ||
        die "parent index is dirty; deploy only reviewed commits"
    git -C "${SOURCE_DIR}" diff --quiet ||
        die "backstop-manager working tree is dirty; deploy only a reviewed commit"
    git -C "${SOURCE_DIR}" diff --cached --quiet ||
        die "backstop-manager index is dirty; deploy only a reviewed commit"
    pinned_commit="$(git -C "${ROOT_DIR}" ls-tree HEAD backstop-manager | awk '{print $3}')"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    [[ -n "${pinned_commit}" && "${pinned_commit}" == "${source_commit}" ]] ||
        die "parent commit does not pin the checked-out backstop-manager commit"
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
        --arg manager "${MANAGER}" \
        --arg backstop "${BACKSTOP}" \
        --arg pool "${POOL}" \
        --arg admin_scope "${ADMIN_SCOPE}" \
        --arg salt "${SALT}" \
        --arg expected_contract "${expected_contract}" \
        --arg artifact_sha256 "${artifact_sha256}" \
        --arg artifact_size "${artifact_size}" \
        --arg source_repo "${EXPECTED_SOURCE_REPO}" \
        --arg source_commit "${source_commit}" \
        --arg parent_commit "${parent_commit}" \
        --arg run_dir "${RUN_DIR}" \
        --arg created_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '
        {
            schema_version: 1,
            network_passphrase: $network_passphrase,
            rpc_url: $rpc_url,
            deployer: $deployer,
            owner: $owner,
            manager: $manager,
            backstop: $backstop,
            pool: $pool,
            admin_scope: ($admin_scope | tonumber),
            salt: $salt,
            contract_id: $expected_contract,
            artifact: {
                sha256: $artifact_sha256,
                size_bytes: ($artifact_size | tonumber),
                source_repo: $source_repo,
                source_commit: $source_commit
            },
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
    [[ "$(state_value manager)" == "${MANAGER}" ]] || die "state manager mismatch"
    [[ "$(state_value backstop)" == "${BACKSTOP}" ]] || die "state backstop mismatch"
    [[ "$(state_value pool)" == "${POOL}" ]] || die "state pool mismatch"
    [[ "$(state_value admin_scope)" == "${ADMIN_SCOPE}" ]] || die "state scope mismatch"
    [[ "$(state_value salt)" == "${SALT}" ]] || die "state salt mismatch"
    [[ "$(state_value contract_id)" == "$(predict_contract_id)" ]] ||
        die "state contract ID mismatch"
    [[ "$(jq -er '.artifact.source_repo' "${STATE_FILE}")" == "${EXPECTED_SOURCE_REPO}" ]] ||
        die "state source repository mismatch"
    [[ "$(jq -er '.artifact.source_commit' "${STATE_FILE}")" == \
        "$(git -C "${SOURCE_DIR}" rev-parse HEAD)" ]] || die "state source commit mismatch"
    [[ "$(jq -er '.artifact.sha256' "${STATE_FILE}")" == "$(sha256_file "${WASM}")" ]] ||
        die "state artifact mismatch"
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
    RECORDED_CONTRACT_ID="$(jq -er '.contract.contract_id // .contract_id' "${source}")"
}

ensure_artifact_uploaded() {
    local artifact_sha256 temporary_dir fetched uploaded
    artifact_sha256="$(sha256_file "${WASM}")"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/backstop-manager.wasm"
    if fetch_contract_artifact "${fetched}" "backstop-manager WASM ${artifact_sha256}" \
        --wasm-hash "${artifact_sha256}"
    then
        cmp -s "${WASM}" "${fetched}" || {
            rm -r -- "${temporary_dir}"
            die "uploaded WASM bytes do not match the reviewed artifact"
        }
        rm -r -- "${temporary_dir}"
        if [[ -f "${RUN_DIR}/upload-backstop-manager.cost.log" ]] ||
            jq -e '.transactions.upload_wasm | type == "string"' \
                "${STATE_FILE}" >/dev/null 2>&1
        then
            LAST_TX_HASH=""
            record_successful_transaction upload-backstop-manager upload_wasm
        fi
        state_set phase artifact-uploaded
        return
    fi
    rm -r -- "${temporary_dir}"
    capture upload-backstop-manager wallet_stellar contract upload \
        --wasm "${WASM}" \
        --optimize=false \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost >/dev/null
    uploaded="$(normalize_scalar "$(<"${RUN_DIR}/upload-backstop-manager.out")")"
    [[ "${uploaded}" == "${artifact_sha256}" ]] || die "uploaded WASM hash mismatch"
    record_successful_transaction upload-backstop-manager upload_wasm
    state_set phase artifact-uploaded
}

ensure_contract_deployed() {
    local contract_id temporary_dir fetched deployed
    contract_id="$(state_value contract_id)"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    if fetch_contract_artifact "${fetched}" "backstop-manager contract ${contract_id}" \
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
        record_successful_transaction deploy-backstop-manager deploy_contract
        state_set deployed_at "${LAST_TX_CREATED_AT}"
        state_set phase contract-deployed
        return
    fi
    rm -r -- "${temporary_dir}"
    capture deploy-backstop-manager wallet_stellar contract deploy \
        --wasm-hash "$(sha256_file "${WASM}")" \
        --salt "${SALT}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost -- \
        --owner "${OWNER}" \
        --manager "${MANAGER}" \
        --admin_scope "${ADMIN_SCOPE}" \
        --backstops "[\"${BACKSTOP}\"]" \
        --pools "[\"${POOL}\"]" >/dev/null
    deployed="$(normalize_scalar "$(<"${RUN_DIR}/deploy-backstop-manager.out")")"
    [[ "${deployed}" == "${contract_id}" ]] || die "deployed contract ID mismatch"
    record_successful_transaction deploy-backstop-manager deploy_contract
    state_set deployed_at "${LAST_TX_CREATED_AT}"
    state_set phase contract-deployed
}

verify_live_contract() {
    local contract_id temporary_dir fetched actual_owner actual_manager actual_backstops
    local actual_pools actual_harvest_bps
    contract_id="$(predict_contract_id)"
    if [[ -n "${RECORDED_CONTRACT_ID}" ]]; then
        [[ "${contract_id}" == "${RECORDED_CONTRACT_ID}" ]] ||
            die "recorded contract ID does not match deployer and salt"
    fi
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    fetch_contract_artifact "${fetched}" "backstop-manager contract ${contract_id}" \
        --id "${contract_id}" || {
        rm -r -- "${temporary_dir}"
        die "backstop-manager contract is not deployed at ${contract_id}"
    }
    cmp -s "${WASM}" "${fetched}" || {
        rm -r -- "${temporary_dir}"
        die "deployed contract does not match the reviewed local WASM"
    }
    verify_source_metadata
    rm -r -- "${temporary_dir}"

    actual_owner="$(normalize_scalar "$(invoke_view "${contract_id}" owner)")"
    [[ "${actual_owner}" == "${OWNER}" ]] || die "owner readback mismatch"
    actual_manager="$(invoke_view "${contract_id}" manager)"
    printf '%s' "${actual_manager}" | jq -e \
        --arg id "${MANAGER}" --argjson scope "${ADMIN_SCOPE}" \
        '.id == $id and .scope == $scope' >/dev/null || die "manager readback mismatch"
    actual_backstops="$(invoke_view "${contract_id}" backstops)"
    printf '%s' "${actual_backstops}" | jq -e --arg value "${BACKSTOP}" \
        '. == [$value]' >/dev/null || die "backstop allowlist readback mismatch"
    actual_pools="$(invoke_view "${contract_id}" pools)"
    printf '%s' "${actual_pools}" | jq -e --arg value "${POOL}" \
        '. == [$value]' >/dev/null || die "pool allowlist readback mismatch"
    actual_harvest_bps="$(normalize_scalar "$(invoke_view "${contract_id}" harvest_bps)")"
    [[ "${actual_harvest_bps}" == 0 ]] || die "harvest_bps readback mismatch"
    printf '%s\n' "${contract_id}"
}

write_registry() {
    local temporary="${REGISTRY_FILE}.tmp" verified_at
    [[ -f "${STATE_FILE}" ]] || die "deployment state is required to write the registry"
    [[ "$(state_optional deployed_at)" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T ]] ||
        die "deployment record is missing a verified deployment timestamp"
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
            purpose: "YieldBlent backstop manager",
            deployed_at,
            deployer,
            contract: {
                contract_id,
                salt,
                wasm_sha256: .artifact.sha256,
                wasm_size_bytes: .artifact.size_bytes,
                source_repo: .artifact.source_repo,
                source_commit: .artifact.source_commit,
                parent_commit,
                stellar_cli_version: $stellar_cli_version
            },
            constructor: {
                owner,
                manager,
                admin_scope,
                backstops: [.backstop],
                pools: [.pool]
            },
            initial_state: {harvest_bps: 0},
            transactions,
            verification: {
                verified_at: $verified_at,
                artifact_matches: true,
                source_repo_metadata: .artifact.source_repo,
                constructor_readback_matches: true
            }
        }' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${REGISTRY_FILE}"
    state_set verified_at "${verified_at}"
    state_set phase verified
}

plan_command() {
    local contract_id artifact_sha256 artifact_size source_commit
    validate_inputs
    contract_id="$(predict_contract_id)"
    artifact_sha256="$(sha256_file "${WASM}")"
    artifact_size="$(wc -c <"${WASM}" | tr -d ' ')"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    cat <<EOF
YieldBlent backstop-manager mainnet deployment plan
  Source: ${EXPECTED_SOURCE_REPO} at ${source_commit}
  Artifact: ${artifact_sha256} (${artifact_size} bytes)
  Deployer: ${DEPLOYER}
  Predicted contract: ${contract_id}
  Owner: ${OWNER}
  Manager: ${MANAGER}
  Manager scope: ${ADMIN_SCOPE} (high)
  Allowed backstop: ${BACKSTOP}
  Allowed pool: ${POOL}
  Initial harvest_bps: 0
  Salt: ${SALT}

This command did not submit a transaction.
EOF
}

deploy_command() {
    validate_inputs
    [[ "${CONFIRM_MAINNET}" == true ]] ||
        die "set BLNT_BACKSTOP_MANAGER_CONFIRM_MAINNET=true after reviewing the plan"
    require_clean_reviewed_source
    load_account_config
    if [[ -f "${STATE_FILE}" ]]; then
        load_state
    else
        initialize_state
    fi
    if [[ "$(state_optional phase)" == planned ]]; then
        assert_expected_contract_unused
    fi
    ensure_artifact_uploaded
    ensure_contract_deployed
    verify_live_contract >/dev/null
    write_registry
    note "Deployed and verified backstop-manager $(state_value contract_id)."
    note "Deployment record written to ${REGISTRY_FILE}."
}

record_command() {
    load_status_inputs
    validate_inputs
    load_state
    verify_live_contract >/dev/null
    write_registry
    note "Verified deployed backstop-manager $(state_value contract_id)."
    note "Deployment record written to ${REGISTRY_FILE}."
}

status_command() {
    load_status_inputs
    validate_inputs
    verify_live_contract >/dev/null
    note "Verified deployed backstop-manager $(predict_contract_id)."
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
