#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="${ROOT_DIR}/blnt-backstop-manager-helper"
MANAGER_REGISTRY="${ROOT_DIR}/deployments/mainnet-v3-yieldblent-backstop-manager-v1.0.1.json"
WASM="${BLNT_BACKSTOP_MANAGER_HELPER_WASM:-}"
SOURCE_TAG="${BLNT_BACKSTOP_MANAGER_HELPER_SOURCE_TAG:-v1.0.0}"
RELEASE_TAG="${BLNT_BACKSTOP_MANAGER_HELPER_RELEASE_TAG:-v1.0.0_blnt-backstop-manager-helper_cli27.0.0}"
WORK_DIR="${BLNT_BACKSTOP_MANAGER_HELPER_WORK_DIR:-${ROOT_DIR}/.mainnet-v3-backstop-manager-helper}"
CONFIG_DIR="${BLNT_BACKSTOP_MANAGER_HELPER_CONFIG_DIR:-${WORK_DIR}/stellar-config}"
STATE_FILE="${BLNT_BACKSTOP_MANAGER_HELPER_STATE_FILE:-${WORK_DIR}/state.json}"
REGISTRY_FILE="${BLNT_BACKSTOP_MANAGER_HELPER_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-v3-yieldblent-backstop-manager-helper-v1.0.0.json}"
ACCOUNT_CONFIG="${BLNT_BACKSTOP_MANAGER_HELPER_ACCOUNT_CONFIG:-}"
RPC_URL="${BLNT_BACKSTOP_MANAGER_HELPER_RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}"
HORIZON_URL="${BLNT_BACKSTOP_MANAGER_HELPER_HORIZON_URL:-https://horizon.stellar.org}"
NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
INCLUSION_FEE="${BLNT_BACKSTOP_MANAGER_HELPER_INCLUSION_FEE:-100000}"

DEPLOYER="${BLNT_BACKSTOP_MANAGER_HELPER_DEPLOYER:-}"
OWNER="${BLNT_BACKSTOP_MANAGER_HELPER_OWNER:-}"
MANAGER="${BLNT_BACKSTOP_MANAGER_HELPER_MANAGER:-CAJUD2IQOVKPDZFJZEX2MSG7PKIE2NF4CJQ2ERBU7VF52SEM5GQL7XEB}"
BACKSTOP="${BLNT_BACKSTOP_MANAGER_HELPER_BACKSTOP:-CAHCUKBPTW5DR5RNXOHCTJD5U76A6EEZXYZAQFRQEEQRXQ3CRUZ6Z3MO}"
POOL="${BLNT_BACKSTOP_MANAGER_HELPER_POOL:-CAUTPSQDAJEUV5XEU3EXFVYRGDJLEJGVTFJCFZCSPTDCREAMZ573VF4D}"
FIRST_LP="${BLNT_BACKSTOP_MANAGER_HELPER_FIRST_LP:-CA5HX3D47PQZWC2E7VWXCKH4534P2NOJEOIXAWU47MI4QV2FQUYY3FLC}"
SECOND_LP="${BLNT_BACKSTOP_MANAGER_HELPER_SECOND_LP:-CDQMIKIHKA5UY3FQJ2UDTLNX3GOPD3GPNZWSPTOBVOHEUMQJQXOPC7Y3}"
BLNT="${BLNT_BACKSTOP_MANAGER_HELPER_BLNT:-CCBNWP5L7ZSXA3DBWCMFAVKFVRW3ZHWO7B5J557W42O64DDZNPJP7VDS}"
XLM="${BLNT_BACKSTOP_MANAGER_HELPER_XLM:-CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA}"
USDC="${BLNT_BACKSTOP_MANAGER_HELPER_USDC:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}"
SALT="${BLNT_BACKSTOP_MANAGER_HELPER_SALT:-c65e66150f37bb2b97a7f47d7151a7146e152c2df966c48eb5c340ee95b720a3}"
CONFIRM_MAINNET="${BLNT_BACKSTOP_MANAGER_HELPER_CONFIRM_MAINNET:-false}"

EXPECTED_REPOSITORY="BLNT-Protocol/blnt-backstop-manager-helper"
EXPECTED_PARENT_REPOSITORY="BLNT-Protocol/blnt-v3-migration"
EXPECTED_SOURCE_REPO="github:${EXPECTED_REPOSITORY}"
EXPECTED_SIGNER_WORKFLOW="stellar-expert/soroban-build-workflow/.github/workflows/release.yml"
EXPECTED_SIGNER_DIGEST="88068ec50cba931a96436869727ed08edeb76ade"
EXPECTED_MANAGER_SOURCE_REPO="github:BLNT-Protocol/backstop-manager"

RUN_DIR=""
LAST_TX_HASH=""
LAST_TX_CREATED_AT=""
LAST_TX_LEDGER=""
RECORDED_CONTRACT_ID=""
STELLAR_PUBLIC_KEY=""
STELLAR_SECRET_KEY=""
RELEASE_URL=""
RELEASE_ASSET_NAME=""
RELEASE_ASSET_URL=""
RELEASE_WORKFLOW_URL=""
RELEASE_ATTESTATION_COUNT=""
ARTIFACT_RUST_VERSION=""
ARTIFACT_SDK_VERSION=""
ARTIFACT_CLI_VERSION=""
FIRST_BLNT_FIRST=""
SECOND_BLNT_FIRST=""

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
    STELLAR_INCLUSION_FEE="${INCLUSION_FEE}" stellar --config-dir "${CONFIG_DIR}" "$@"
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
        'if type == "string" or type == "number" or type == "boolean" then tostring else empty end' \
        2>/dev/null)"
    then
        printf '%s\n' "${decoded}"
    else
        printf '%s\n' "${raw}"
    fi
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

fetch_contract_artifact() {
    local destination="$1"
    shift
    rm -f "${destination}"
    stellar_cli contract fetch "$@" \
        --out-file "${destination}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        >/dev/null 2>&1
}

load_account_config() {
    [[ -n "${ACCOUNT_CONFIG}" ]] ||
        die "BLNT_BACKSTOP_MANAGER_HELPER_ACCOUNT_CONFIG is required"
    [[ -f "${ACCOUNT_CONFIG}" ]] || die "account config is missing"
    # shellcheck disable=SC1090
    source "${ACCOUNT_CONFIG}"
    [[ "${STELLAR_PUBLIC_KEY:-}" == "${DEPLOYER}" ]] ||
        die "account config does not identify the declared deployer"
    [[ "${STELLAR_SECRET_KEY:-}" =~ ^S[A-Z2-7]{55}$ ]] ||
        die "account config does not contain a valid secret key"
}

capture() {
    local label="$1" stdout_file stderr_file status
    shift
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

fetch_successful_transaction() {
    local hash="$1" attempt response
    for attempt in 1 2 3 4 5; do
        if response="$(curl --fail --silent --show-error --max-time 15 \
            "${HORIZON_URL}/transactions/${hash}" 2>/dev/null)" &&
            printf '%s' "${response}" | jq -e \
                --arg hash "${hash}" --arg deployer "${DEPLOYER}" \
                '.hash == $hash and .source_account == $deployer and
                 .successful == true and (.ledger | type == "number")' >/dev/null
        then
            printf '%s\n' "${response}"
            return
        fi
        (( attempt == 5 )) || sleep 2
    done
    die "unable to verify successful transaction ${hash} through Horizon"
}

record_transaction_evidence() {
    local label="$1" hash="$2" transaction temporary
    [[ "${hash}" =~ ^[0-9a-f]{64}$ ]] || die "missing ${label} transaction hash"
    transaction="$(fetch_successful_transaction "${hash}")"
    LAST_TX_CREATED_AT="$(printf '%s' "${transaction}" | jq -er '.created_at')"
    LAST_TX_LEDGER="$(printf '%s' "${transaction}" | jq -er '.ledger | tostring')"
    temporary="${STATE_FILE}.tmp"
    jq --arg label "${label}" --arg hash "${hash}" \
        --arg created_at "${LAST_TX_CREATED_AT}" --arg ledger "${LAST_TX_LEDGER}" '
        .transactions[$label] = {
            hash: $hash,
            created_at: $created_at,
            ledger: ($ledger | tonumber)
        }' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${STATE_FILE}"
}

verify_source_metadata() {
    local metadata
    metadata="$(stellar contract info meta --wasm "${WASM}" 2>/dev/null)"
    printf '%s' "${metadata}" | grep -Fq "source_repo: ${EXPECTED_SOURCE_REPO}" ||
        die "helper WASM source_repo metadata mismatch"
    ARTIFACT_RUST_VERSION="$(printf '%s\n' "${metadata}" | sed -n 's/.*rsver: //p' | head -1)"
    ARTIFACT_SDK_VERSION="$(printf '%s\n' "${metadata}" | sed -n 's/.*rssdkver: //p' | head -1)"
    ARTIFACT_CLI_VERSION="$(printf '%s\n' "${metadata}" | sed -n 's/.*cliver: //p' | head -1)"
    [[ -n "${ARTIFACT_RUST_VERSION}" && -n "${ARTIFACT_SDK_VERSION}" &&
        -n "${ARTIFACT_CLI_VERSION}" ]] || die "release WASM metadata is incomplete"
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
    local workflow_runs match_count temporary_dir candidate_name candidate_url
    local candidate_digest candidate_path artifact_sha attestation_json
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    local_tag_commit="$(git -C "${SOURCE_DIR}" rev-list -n 1 "${SOURCE_TAG}" 2>/dev/null || true)"
    [[ "${local_tag_commit}" == "${source_commit}" ]] ||
        die "source tag does not identify the checked-out source commit"
    remote_commit="$(remote_tag_commit)"
    [[ "${remote_commit}" == "${source_commit}" ]] ||
        die "remote source tag does not identify the checked-out source commit"

    repository_json="$(gh api "repos/${EXPECTED_REPOSITORY}")"
    printf '%s' "${repository_json}" | jq -e \
        '.private == false and .archived == false and .disabled == false' >/dev/null ||
        die "canonical source repository is not public and active"

    release_json="$(gh api "repos/${EXPECTED_REPOSITORY}/releases/tags/${RELEASE_TAG}")"
    printf '%s' "${release_json}" | jq -e \
        '.draft == false and .prerelease == false' >/dev/null ||
        die "release is not final and public"
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
                [[ "${candidate_digest}" == "sha256:${artifact_sha}" ]] ||
                    die "GitHub release digest mismatch"
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
        sort_by(.created_at) | last | .html_url // empty')"
    [[ -n "${RELEASE_WORKFLOW_URL}" ]] ||
        die "no successful tag-triggered release workflow matches the source tag"

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

require_clean_reviewed_source() {
    local parent_commit parent_json pinned_commit source_commit
    [[ -z "$(git -C "${ROOT_DIR}" status --porcelain --untracked-files=normal)" ]] ||
        die "parent checkout is dirty; deploy only reviewed commits"
    [[ -z "$(git -C "${SOURCE_DIR}" status --porcelain --untracked-files=normal)" ]] ||
        die "helper source checkout is dirty"
    parent_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    parent_json="$(gh api "repos/${EXPECTED_PARENT_REPOSITORY}/commits/${parent_commit}")" ||
        die "parent commit is not available from the canonical repository"
    printf '%s' "${parent_json}" | jq -e --arg sha "${parent_commit}" \
        '.sha == $sha' >/dev/null || die "parent commit is not pushed"
    pinned_commit="$(git -C "${ROOT_DIR}" ls-tree HEAD blnt-backstop-manager-helper | awk '{print $3}')"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    [[ "${pinned_commit}" == "${source_commit}" ]] ||
        die "parent commit does not pin the checked-out helper commit"
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
    fetch_contract_artifact "${fetched}" --id "${MANAGER}" ||
        die "backstop manager is not deployed"
    [[ "$(sha256_file "${fetched}")" == "${recorded_hash}" ]] ||
        die "live manager WASM does not match its deployment record"
    rm -r -- "${temporary_dir}"
}

validate_pair() {
    local lp="$1" paired="$2" label="$3" tokens supply blnt_balance paired_balance
    tokens="$(invoke_view "${lp}" get_tokens)"
    printf '%s' "${tokens}" | jq -e --arg blnt "${BLNT}" --arg paired "${paired}" '
        length == 2 and (sort == ([$blnt, $paired] | sort))' >/dev/null ||
        die "${label} token pair mismatch"
    supply="$(normalize_scalar "$(invoke_view "${lp}" get_total_supply)")"
    blnt_balance="$(normalize_scalar "$(invoke_view "${lp}" get_balance --token "${BLNT}")")"
    paired_balance="$(normalize_scalar "$(invoke_view "${lp}" get_balance --token "${paired}")")"
    (( supply > 0 && blnt_balance > 0 && paired_balance > 0 )) ||
        die "${label} does not have positive supply and reserves"
    printf '%s' "${tokens}" | jq -er --arg blnt "${BLNT}" '.[0] == $blnt'
}

validate_live_bindings() {
    local decimals manager_owner manager_backstops manager_pools first_tier second_tier
    for token in "${BLNT}" "${XLM}" "${USDC}"; do
        decimals="$(normalize_scalar "$(invoke_view "${token}" decimals)")"
        [[ "${decimals}" == 7 ]] || die "token decimals mismatch for ${token}"
    done
    verify_manager_artifact
    manager_owner="$(normalize_scalar "$(invoke_view "${MANAGER}" owner)")"
    [[ "${manager_owner}" == "${OWNER}" ]] ||
        die "backstop-manager owner is not the proposed helper owner"
    manager_backstops="$(invoke_view "${MANAGER}" backstops)"
    printf '%s' "${manager_backstops}" | jq -e --arg value "${BACKSTOP}" \
        'index($value) != null' >/dev/null || die "manager does not allow the backstop"
    manager_pools="$(invoke_view "${MANAGER}" pools)"
    printf '%s' "${manager_pools}" | jq -e --arg value "${POOL}" \
        'index($value) != null' >/dev/null || die "manager does not allow the pool"
    first_tier="$(normalize_scalar "$(invoke_view "${BACKSTOP}" backstop_token \
        --tier FirstLoss --pool "${POOL}")")"
    second_tier="$(normalize_scalar "$(invoke_view "${BACKSTOP}" backstop_token \
        --tier SecondLoss --pool "${POOL}")")"
    [[ "${first_tier}" == "${FIRST_LP}" ]] || die "FirstLoss token mismatch"
    [[ "${second_tier}" == "${SECOND_LP}" ]] || die "SecondLoss token mismatch"
    FIRST_BLNT_FIRST="$(validate_pair "${FIRST_LP}" "${XLM}" "BLNT:XLM LP")"
    SECOND_BLNT_FIRST="$(validate_pair "${SECOND_LP}" "${USDC}" "BLNT:USDC LP")"
}

predict_contract_id() {
    normalize_scalar "$(stellar_cli contract id wasm \
        --salt "${SALT}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}")"
}

validate_inputs() {
    for command in awk cmp curl gh git grep jq sed shasum stellar wc; do
        require_command "${command}"
    done
    [[ -s "${WASM}" ]] ||
        die "BLNT_BACKSTOP_MANAGER_HELPER_WASM must identify the downloaded release WASM"
    [[ -n "${DEPLOYER}" ]] || die "BLNT_BACKSTOP_MANAGER_HELPER_DEPLOYER is required"
    [[ -n "${OWNER}" ]] || die "BLNT_BACKSTOP_MANAGER_HELPER_OWNER is required"
    [[ "${DEPLOYER}" =~ ^G[A-Z2-7]{55}$ ]] || die "invalid deployment source account"
    for binding in OWNER MANAGER BACKSTOP POOL FIRST_LP SECOND_LP BLNT XLM USDC; do
        require_address "${!binding}" "${binding}"
    done
    [[ "${SALT}" =~ ^[0-9a-f]{64}$ ]] || die "deployment salt must be lowercase hex"
    [[ "${INCLUSION_FEE}" =~ ^[0-9]+$ ]] || die "invalid inclusion fee"
    mkdir -p "${WORK_DIR}" "${CONFIG_DIR}"
    chmod 700 "${WORK_DIR}" "${CONFIG_DIR}"
    verify_source_metadata
    verify_release_artifact
    validate_live_bindings
}

print_plan() {
    jq -n \
        --arg network "${NETWORK_PASSPHRASE}" \
        --arg deployer "${DEPLOYER}" \
        --arg owner "${OWNER}" \
        --arg contract_id "$(predict_contract_id)" \
        --arg salt "${SALT}" \
        --arg wasm_sha256 "$(sha256_file "${WASM}")" \
        --arg source_commit "$(git -C "${SOURCE_DIR}" rev-parse HEAD)" \
        --arg source_tag "${SOURCE_TAG}" \
        --arg release_tag "${RELEASE_TAG}" \
        --arg release_url "${RELEASE_URL}" \
        --arg manager "${MANAGER}" \
        --arg backstop "${BACKSTOP}" \
        --arg pool "${POOL}" \
        --arg first_lp "${FIRST_LP}" \
        --arg second_lp "${SECOND_LP}" \
        --arg blnt "${BLNT}" \
        --arg xlm "${XLM}" \
        --arg usdc "${USDC}" \
        --argjson first_blnt_first "${FIRST_BLNT_FIRST}" \
        --argjson second_blnt_first "${SECOND_BLNT_FIRST}" '
        {
            network: $network,
            deployer: $deployer,
            predicted_contract_id: $contract_id,
            salt: $salt,
            wasm_sha256: $wasm_sha256,
            source_commit: $source_commit,
            source_tag: $source_tag,
            release_tag: $release_tag,
            release_url: $release_url,
            constructor: {
                owner: $owner,
                manager: $manager,
                backstop: $backstop,
                pool: $pool,
                first_lp: $first_lp,
                second_lp: $second_lp,
                blnt: $blnt,
                xlm: $xlm,
                usdc: $usdc
            },
            expected_derived_state: {
                first_blnt_first: $first_blnt_first,
                second_blnt_first: $second_blnt_first
            }
        }'
}

initialize_state() {
    local expected source_commit parent_commit sha size
    expected="$(predict_contract_id)"
    source_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
    parent_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
    sha="$(sha256_file "${WASM}")"
    size="$(wc -c <"${WASM}" | tr -d ' ')"
    RUN_DIR="${WORK_DIR}/runs/$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "${RUN_DIR}"
    jq -n \
        --arg contract_id "${expected}" --arg salt "${SALT}" \
        --arg deployer "${DEPLOYER}" --arg owner "${OWNER}" \
        --arg manager "${MANAGER}" --arg backstop "${BACKSTOP}" \
        --arg pool "${POOL}" --arg first_lp "${FIRST_LP}" \
        --arg second_lp "${SECOND_LP}" --arg blnt "${BLNT}" \
        --arg xlm "${XLM}" --arg usdc "${USDC}" \
        --arg sha "${sha}" --arg size "${size}" \
        --arg source_commit "${source_commit}" --arg parent_commit "${parent_commit}" \
        --arg source_tag "${SOURCE_TAG}" --arg release_tag "${RELEASE_TAG}" \
        --arg release_url "${RELEASE_URL}" --arg asset_name "${RELEASE_ASSET_NAME}" \
        --arg asset_url "${RELEASE_ASSET_URL}" --arg workflow_url "${RELEASE_WORKFLOW_URL}" \
        --arg attestation_count "${RELEASE_ATTESTATION_COUNT}" \
        --arg rust "${ARTIFACT_RUST_VERSION}" --arg sdk "${ARTIFACT_SDK_VERSION}" \
        --arg cli "${ARTIFACT_CLI_VERSION}" \
        --argjson first_order "${FIRST_BLNT_FIRST}" \
        --argjson second_order "${SECOND_BLNT_FIRST}" \
        --arg run_dir "${RUN_DIR}" '
        {
            schema_version: 1,
            contract_id: $contract_id,
            salt: $salt,
            deployer: $deployer,
            constructor: {
                owner: $owner, manager: $manager, backstop: $backstop, pool: $pool,
                first_lp: $first_lp, second_lp: $second_lp, blnt: $blnt, xlm: $xlm, usdc: $usdc
            },
            expected_derived_state: {
                first_blnt_first: $first_order,
                second_blnt_first: $second_order
            },
            artifact: {
                sha256: $sha,
                size_bytes: ($size | tonumber),
                source_repo: "github:BLNT-Protocol/blnt-backstop-manager-helper",
                source_commit: $source_commit,
                source_tag: $source_tag,
                release_tag: $release_tag,
                release_url: $release_url,
                asset_name: $asset_name,
                asset_url: $asset_url,
                workflow_url: $workflow_url,
                attestation_count: ($attestation_count | tonumber),
                rust_version: $rust,
                soroban_sdk_version: $sdk,
                build_stellar_cli_version: $cli
            },
            parent_commit: $parent_commit,
            run_dir: $run_dir,
            transactions: {}
        }' >"${STATE_FILE}"
}

load_state() {
    [[ -f "${STATE_FILE}" ]] || die "deployment state is missing"
    RUN_DIR="$(jq -er '.run_dir' "${STATE_FILE}")"
    [[ "$(jq -er '.artifact.sha256' "${STATE_FILE}")" == "$(sha256_file "${WASM}")" ]] ||
        die "deployment state artifact mismatch"
    [[ "$(jq -er '.contract_id' "${STATE_FILE}")" == "$(predict_contract_id)" ]] ||
        die "deployment state contract ID mismatch"
}

verify_live_contract() {
    local contract_id temporary_dir fetched config
    contract_id="$(predict_contract_id)"
    [[ -z "${RECORDED_CONTRACT_ID}" || "${RECORDED_CONTRACT_ID}" == "${contract_id}" ]] ||
        die "recorded contract ID mismatch"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/helper.wasm"
    fetch_contract_artifact "${fetched}" --id "${contract_id}" ||
        die "helper is not deployed at ${contract_id}"
    cmp -s "${WASM}" "${fetched}" || die "deployed WASM does not match release artifact"
    rm -r -- "${temporary_dir}"
    config="$(invoke_view "${contract_id}" get_config)"
    printf '%s' "${config}" | jq -e \
        --arg owner "${OWNER}" --arg manager "${MANAGER}" \
        --arg backstop "${BACKSTOP}" --arg pool "${POOL}" \
        --arg first_lp "${FIRST_LP}" --arg second_lp "${SECOND_LP}" \
        --arg blnt "${BLNT}" --arg xlm "${XLM}" --arg usdc "${USDC}" \
        --argjson first_order "${FIRST_BLNT_FIRST}" \
        --argjson second_order "${SECOND_BLNT_FIRST}" '
        .owner == $owner and .manager == $manager and .backstop == $backstop and
        .pool == $pool and .first_lp == $first_lp and .second_lp == $second_lp and
        .blnt == $blnt and .xlm == $xlm and .usdc == $usdc and
        .first_blnt_first == $first_order and .second_blnt_first == $second_order' >/dev/null ||
        die "constructor configuration readback mismatch"
    printf '%s\n' "${contract_id}"
}

ensure_uploaded() {
    local sha temporary_dir fetched uploaded
    sha="$(sha256_file "${WASM}")"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/code.wasm"
    if fetch_contract_artifact "${fetched}" --wasm-hash "${sha}"; then
        cmp -s "${WASM}" "${fetched}" || die "uploaded code bytes mismatch"
        rm -r -- "${temporary_dir}"
        return
    fi
    rm -r -- "${temporary_dir}"
    capture upload-helper wallet_stellar contract upload \
        --wasm "${WASM}" --optimize=false \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" --cost >/dev/null
    uploaded="$(normalize_scalar "$(<"${RUN_DIR}/upload-helper.out")")"
    [[ "${uploaded}" == "${sha}" ]] || die "uploaded WASM hash mismatch"
    record_transaction_evidence upload_wasm "${LAST_TX_HASH}"
}

ensure_deployed() {
    local contract_id temporary_dir fetched deployed
    contract_id="$(predict_contract_id)"
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    if fetch_contract_artifact "${fetched}" --id "${contract_id}"; then
        cmp -s "${WASM}" "${fetched}" || die "predicted address contains other WASM"
        rm -r -- "${temporary_dir}"
        return
    fi
    rm -r -- "${temporary_dir}"
    capture deploy-helper wallet_stellar contract deploy \
        --wasm-hash "$(sha256_file "${WASM}")" \
        --salt "${SALT}" \
        --source-account "${DEPLOYER}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" --cost -- \
        --owner "${OWNER}" --manager "${MANAGER}" \
        --backstop "${BACKSTOP}" --pool "${POOL}" \
        --first_lp "${FIRST_LP}" --second_lp "${SECOND_LP}" \
        --blnt "${BLNT}" --xlm "${XLM}" --usdc "${USDC}" >/dev/null
    deployed="$(normalize_scalar "$(<"${RUN_DIR}/deploy-helper.out")")"
    [[ "${deployed}" == "${contract_id}" ]] || die "deployed contract ID mismatch"
    record_transaction_evidence deploy_contract "${LAST_TX_HASH}"
}

recover_deploy_transaction() {
    local hash
    if ! jq -e '.transactions.deploy_contract.hash | type == "string"' "${STATE_FILE}" >/dev/null 2>&1; then
        hash="$(sed -n 's/.*Signing transaction: *\([0-9a-f]\{64\}\).*/\1/p' \
            "${RUN_DIR}/deploy-helper.cost.log" 2>/dev/null | tail -1)"
        record_transaction_evidence deploy_contract "${hash}"
    fi
}

write_registry() {
    local temporary verified_at
    recover_deploy_transaction
    verified_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    temporary="${REGISTRY_FILE}.tmp"
    jq --arg verified_at "${verified_at}" --arg network "${NETWORK_PASSPHRASE}" \
        --arg cli "$(stellar --version | head -1)" '
        {
            schema_version: 1,
            network: {name: "Public Global Stellar Network", passphrase: $network},
            purpose: "Atomic YieldBlent backstop-manager deposit and claim helper",
            deployed_at: .transactions.deploy_contract.created_at,
            deployment_ledger: .transactions.deploy_contract.ledger,
            deployer,
            contract: {
                contract_id, salt,
                wasm_sha256: .artifact.sha256,
                wasm_size_bytes: .artifact.size_bytes,
                source_repo: .artifact.source_repo,
                source_commit: .artifact.source_commit,
                source_tag: .artifact.source_tag,
                release_tag: .artifact.release_tag,
                release_url: .artifact.release_url,
                artifact_name: .artifact.asset_name,
                artifact_url: .artifact.asset_url,
                workflow_run_url: .artifact.workflow_url,
                rust_version: .artifact.rust_version,
                soroban_sdk_version: .artifact.soroban_sdk_version,
                build_stellar_cli_version: .artifact.build_stellar_cli_version,
                target: "wasm32v1-none",
                optimization: "Cargo release opt-level=z, LTO, codegen-units=1; release-workflow output",
                stellar_cli_version: $cli,
                parent_commit
            },
            constructor,
            expected_derived_state,
            transactions,
            verification: {
                verified_at: $verified_at,
                artifact_matches: true,
                github_attestation_count: .artifact.attestation_count,
                attestation_predicate_type: "https://slsa.dev/provenance/v1",
                attestation_source_ref: ("refs/tags/" + .artifact.source_tag),
                attestation_signer_repository: "stellar-expert/soroban-build-workflow",
                attestation_signer_digest: "88068ec50cba931a96436869727ed08edeb76ade",
                source_repo_metadata: .artifact.source_repo,
                constructor_readback_matches: true,
                manager_authorization_relationship_matches: true
            },
            status_command: "make mainnet-backstop-manager-helper-status"
        }' "${STATE_FILE}" >"${temporary}"
    mv "${temporary}" "${REGISTRY_FILE}"
}

load_status_record() {
    [[ -f "${REGISTRY_FILE}" ]] || die "deployment record is missing"
    RECORDED_CONTRACT_ID="$(jq -er '.contract.contract_id' "${REGISTRY_FILE}")"
    [[ -n "${DEPLOYER}" ]] || DEPLOYER="$(jq -er '.deployer' "${REGISTRY_FILE}")"
    [[ -n "${OWNER}" ]] || OWNER="$(jq -er '.constructor.owner' "${REGISTRY_FILE}")"
}

run_plan() {
    validate_inputs
    print_plan
}

run_deploy() {
    [[ "${CONFIRM_MAINNET}" == true ]] ||
        die "set BLNT_BACKSTOP_MANAGER_HELPER_CONFIRM_MAINNET=true"
    validate_inputs
    require_clean_reviewed_source
    load_account_config
    if [[ -f "${STATE_FILE}" ]]; then
        load_state
    else
        initialize_state
    fi
    ensure_uploaded
    ensure_deployed
    verify_live_contract >/dev/null
    write_registry
    note "Deployment complete: $(jq -er '.contract.contract_id' "${REGISTRY_FILE}")"
}

run_record() {
    validate_inputs
    require_clean_reviewed_source
    load_state
    verify_live_contract >/dev/null
    write_registry
    note "Deployment record recovered: ${REGISTRY_FILE}"
}

run_status() {
    load_status_record
    validate_inputs
    local contract_id
    contract_id="$(verify_live_contract)"
    jq -n --arg contract_id "${contract_id}" \
        --arg wasm_sha256 "$(sha256_file "${WASM}")" \
        --arg owner "${OWNER}" --arg manager "${MANAGER}" \
        '{status: "verified", contract_id: $contract_id, wasm_sha256: $wasm_sha256,
          owner: $owner, manager: $manager}'
}

case "${1:-}" in
    plan) run_plan ;;
    deploy) run_deploy ;;
    record) run_record ;;
    status) run_status ;;
    *) die "usage: $0 {plan|deploy|record|status}" ;;
esac
