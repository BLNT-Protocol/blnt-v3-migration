#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export BLEND_MAINNET_COMET_LIBRARY_ONLY=true
export BLEND_MAINNET_COMET_CONFIG_DIR="${BLEND_MAINNET_V3_CONFIG_DIR:-${ROOT_DIR}/.mainnet-v3/stellar-config}"
# shellcheck disable=SC1091
source "${ROOT_DIR}/scripts/mainnet-comet-deployment.sh"
unset BLEND_MAINNET_COMET_LIBRARY_ONLY

WORK_DIR="${BLEND_MAINNET_V3_FIXED_WORK_DIR:-${ROOT_DIR}/.mainnet-v3/fixed-pool}"
CONFIG_DIR="${BLEND_MAINNET_V3_CONFIG_DIR:-${ROOT_DIR}/.mainnet-v3/stellar-config}"
STATE_FILE="${BLEND_MAINNET_V3_FIXED_STATE_FILE:-${WORK_DIR}/state.json}"
REGISTRY_FILE="${BLEND_MAINNET_V3_FIXED_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-v3-fixed-pool.json}"
CORE_REGISTRY="${BLEND_MAINNET_V3_REGISTRY_FILE:-${ROOT_DIR}/deployments/mainnet-v3.json}"
FIXTURE="${ROOT_DIR}/fixtures/mainnet-fixed-pool.json"

FACTORY_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/pool_factory.wasm"
BACKSTOP_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/backstop.wasm"
POOL_WASM="${ROOT_DIR}/blnt-contracts-v3/target/wasm32v1-none/optimized/pool.wasm"

FIXTURE_SHA256="bd99448dbf0ba5a415d922e3fe2286a5321f77dc235c346ae9687fda866bd613"
FACTORY_SHA256="b956b222641fe8a419668157351311127b40afd80c7289af1cd19ac98e2e8cec"
BACKSTOP_SHA256="bda2e1cb2de7ec0621a9408e0a2d7989bc84649bc401fa879aa48fa27b349d3f"
POOL_SHA256="8df52229d11a1ede42ee8f77189e30ca54cb0b6d502db53809ba249ed6bc7782"

POOL_FACTORY="CCF73PABFG3DBKQW5QK2E5JZOQN4OJNA4PLFY7MZVQNHF676RI33QVTM"
BACKSTOP="CACD7TLQBWNR74MB3DDLZXXVM3MERNMKXCWAN3ONNADNDV7H4R54G7MU"
EMITTER="CAL2PDDBVHMONNTZ2B5KMRWJMXYJ7NWWRYEYZU2PCBWOZVKACEW7CSY3"
V2_FIXED_POOL="CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD"
FIXED_ORACLE="CCVTVW2CVA7JLH4ROQGP3CU4T3EXVCK66AZGSM4MUQPXAI4QHCZPOATS"
EXPECTED_POOL="CAOWCC32LIINM27CENUNQLIKGR3R4KONVWI7Y7H42VG7JVMUXKWQZ6YF"
POOL_SALT="8e847c43cb0fcee9531247a2d1e42715f569e04fb017ab9c0791f6154c29abd6"

assert_equal() {
    local expected="$1" actual="$2" label="$3"
    [[ "${actual}" == "${expected}" ]] ||
        die "${label}: expected ${expected}, got ${actual}"
}

assert_json_equal() {
    local expected="$1" actual="$2" label="$3"
    jq -ne --argjson expected "${expected}" --argjson actual "${actual}" \
        '$expected == $actual' >/dev/null || die "${label} mismatch"
}

fixture_value() {
    jq -er "$1" "${FIXTURE}"
}

validate_fixture() {
    [[ -s "${FIXTURE}" ]] || die "mainnet Fixed fixture is missing"
    assert_equal "${FIXTURE_SHA256}" "$(sha256_file "${FIXTURE}")" \
        "mainnet Fixed fixture SHA-256"
    jq -e \
        --arg operator "${WALLET}" \
        --arg source_pool "${V2_FIXED_POOL}" \
        --arg oracle "${FIXED_ORACLE}" \
        --arg salt "${POOL_SALT}" \
        --arg expected_pool "${EXPECTED_POOL}" '
        .schema_version == 1 and
        .source.fixed_pool_v2 == $source_pool and
        .source.oracle == $oracle and
        .deployment.name == "Fixed" and
        .deployment.operator == $operator and
        .deployment.salt == $salt and
        .deployment.expected_pool == $expected_pool and
        .deployment.backstop_take_rate == 2000000 and
        .deployment.max_positions == 6 and
        .deployment.min_collateral == "50000000" and
        .deployment.access_controller == null and
        .deployment.backstop_config == [
            {asset: "BlntXlm", take_rate_weight: 4},
            {asset: "BlntUsdc", take_rate_weight: 3},
            {asset: "Usdc", take_rate_weight: 2}
        ] and
        [.reserves[].symbol] == ["XLM", "USDC", "EURC"] and
        ([.reserves[].asset] | unique | length) == 3 and
        ([.reserves[].config.index] == [0, 1, 2]) and
        ([.reserves[].config.decimals] | all(. == 7)) and
        ([.reserves[].config.enabled] | all(. == true))
    ' "${FIXTURE}" >/dev/null || die "mainnet Fixed fixture failed invariant validation"
}

validate_local_artifacts() {
    [[ -s "${FACTORY_WASM}" ]] || die "pool-factory WASM is missing"
    [[ -s "${BACKSTOP_WASM}" ]] || die "backstop WASM is missing"
    [[ -s "${POOL_WASM}" ]] || die "pool WASM is missing"
    assert_equal "${FACTORY_SHA256}" "$(sha256_file "${FACTORY_WASM}")" \
        "pool-factory WASM SHA-256"
    assert_equal "${BACKSTOP_SHA256}" "$(sha256_file "${BACKSTOP_WASM}")" \
        "backstop WASM SHA-256"
    assert_equal "${POOL_SHA256}" "$(sha256_file "${POOL_WASM}")" \
        "pool WASM SHA-256"
}

validate_core_registry() {
    [[ -s "${CORE_REGISTRY}" ]] || die "verified mainnet V3 core registry is missing"
    jq -e \
        --arg factory "${POOL_FACTORY}" \
        --arg backstop "${BACKSTOP}" \
        --arg emitter "${EMITTER}" \
        --arg factory_hash "${FACTORY_SHA256}" \
        --arg backstop_hash "${BACKSTOP_SHA256}" \
        --arg pool_hash "${POOL_SHA256}" '
        .schema_version == 1 and
        .network.passphrase == "Public Global Stellar Network ; September 2015" and
        .contracts.pool_factory == $factory and
        .contracts.backstop == $backstop and
        .contracts.emitter == $emitter and
        .contracts.pool_wasm_hash == $pool_hash and
        .artifacts.factory == $factory_hash and
        .artifacts.backstop == $backstop_hash and
        .artifacts.pool == $pool_hash and
        .allocations.initial_drop_raw == "1500000000000000"
    ' "${CORE_REGISTRY}" >/dev/null || die "mainnet V3 core registry mismatch"
}

verify_contract_artifact() {
    local contract="$1" artifact="$2" label="$3" temporary_dir fetched
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/contract.wasm"
    if ! fetch_contract_artifact "${fetched}" "${label}" --id "${contract}"; then
        rm -r -- "${temporary_dir}"
        die "${label} is not deployed"
    fi
    cmp -s "${artifact}" "${fetched}" || {
        rm -r -- "${temporary_dir}"
        die "${label} does not match its pinned WASM"
    }
    rm -r -- "${temporary_dir}"
}

verify_pool_wasm_uploaded() {
    local temporary_dir fetched
    temporary_dir="$(mktemp -d)"
    fetched="${temporary_dir}/pool.wasm"
    if ! fetch_contract_artifact "${fetched}" "V3 pool WASM ${POOL_SHA256}" \
        --wasm-hash "${POOL_SHA256}"
    then
        rm -r -- "${temporary_dir}"
        die "pinned V3 pool WASM is not uploaded"
    fi
    cmp -s "${POOL_WASM}" "${fetched}" || {
        rm -r -- "${temporary_dir}"
        die "uploaded V3 pool WASM does not match the pinned artifact"
    }
    rm -r -- "${temporary_dir}"
}

validate_live_core() {
    local admin emitter_backstop
    verify_contract_artifact "${POOL_FACTORY}" "${FACTORY_WASM}" "V3 pool factory"
    verify_contract_artifact "${BACKSTOP}" "${BACKSTOP_WASM}" "V3 backstop"
    verify_pool_wasm_uploaded
    admin="$(normalize_scalar "$(invoke_view "${BLNT}" admin)")"
    assert_equal "${EMITTER}" "${admin}" "BLNT administrator"
    emitter_backstop="$(normalize_scalar "$(invoke_view "${EMITTER}" get_backstop)")"
    assert_equal "${BACKSTOP}" "${emitter_backstop}" "V3 emitter backstop"
}

expected_pool_config() {
    jq -c --arg oracle "${FIXED_ORACLE}" '
        {
            bstop_rate: .deployment.backstop_take_rate,
            max_positions: .deployment.max_positions,
            min_collateral: .deployment.min_collateral,
            oracle: $oracle,
            status: 6
        }
    ' "${FIXTURE}"
}

expected_reserve_list() {
    jq -c '[.reserves[].asset]' "${FIXTURE}"
}

expected_backstop_config() {
    jq -c '{access_controller: .deployment.access_controller, tiers: .deployment.backstop_config}' \
        "${FIXTURE}"
}

validate_live_v2_snapshot() {
    local config reserve_list oracle_assets oracle_base oracle_decimals oracle_max_age
    local index asset expected actual
    config="$(invoke_view "${V2_FIXED_POOL}" get_config)"
    expected="$(expected_pool_config | jq -c 'del(.status)')"
    actual="$(printf '%s' "${config}" | jq -c 'del(.status)')"
    assert_json_equal "${expected}" "${actual}" "live V2 Fixed pool configuration"

    reserve_list="$(invoke_view "${V2_FIXED_POOL}" get_reserve_list)"
    assert_json_equal "$(expected_reserve_list)" "${reserve_list}" \
        "live V2 Fixed reserve list"
    for index in 0 1 2; do
        asset="$(fixture_value ".reserves[${index}].asset")"
        expected="$(fixture_value ".reserves[${index}].config | @json")"
        actual="$(invoke_view "${V2_FIXED_POOL}" get_reserve --asset "${asset}" | \
            jq -c '.config')"
        assert_json_equal "${expected}" "${actual}" \
            "live V2 Fixed reserve ${index} configuration"
    done

    oracle_assets="$(invoke_view "${FIXED_ORACLE}" assets)"
    expected="$(jq -c '[.reserves[].asset | {Stellar: .}]' "${FIXTURE}")"
    assert_json_equal "${expected}" "${oracle_assets}" "Fixed oracle asset list"
    oracle_base="$(invoke_view "${FIXED_ORACLE}" base)"
    assert_json_equal '{"Other":"USD"}' "${oracle_base}" "Fixed oracle base"
    oracle_decimals="$(pool_value "${FIXED_ORACLE}" decimals)"
    oracle_max_age="$(pool_value "${FIXED_ORACLE}" max_age)"
    assert_equal 7 "${oracle_decimals}" "Fixed oracle decimals"
    assert_equal 900 "${oracle_max_age}" "Fixed oracle maximum age"
}

simulate_pool_deploy() {
    invoke_view "${POOL_FACTORY}" deploy \
        --admin "${WALLET}" \
        --name "$(fixture_value '.deployment.name')" \
        --salt "${POOL_SALT}" \
        --oracle "${FIXED_ORACLE}" \
        --backstop_take_rate "$(fixture_value '.deployment.backstop_take_rate')" \
        --max_positions "$(fixture_value '.deployment.max_positions')" \
        --min_collateral "$(fixture_value '.deployment.min_collateral')" \
        --backstop_config "$(fixture_value '.deployment.backstop_config | @json')" \
        --access_controller null
}

factory_is_pool() {
    normalize_scalar "$(invoke_view "${POOL_FACTORY}" is_pool --pool_address "${EXPECTED_POOL}")"
}

require_clean_reviewed_main() {
    local mode="${1:-mutation}" head remote branch
    if [[ "${mode}" == "read-only" &&
        "${BLEND_MAINNET_V3_FIXED_ALLOW_DIRTY:-false}" == "true" ]]
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
    assert_equal DEPLOY_BLNT_V3_FIXED_MAINNET \
        "${BLEND_MAINNET_V3_FIXED_CONFIRM:-}" "mainnet Fixed deployment confirmation"
    assert_equal true "${BLEND_MAINNET_V3_SEQUENCE_LOCK_CONFIRMED:-false}" \
        "exclusive sequence-lock confirmation"
    require_clean_reviewed_main
    require_process_absent \
        'blend-v2.1-migration/scripts/mainnet-emissions-keeper.sh run' \
        "the V2.1 mainnet keeper is using the deployment wallet; stop it first"
    require_process_absent 'mainnet-arbitrage\.mjs run' \
        "the arbitrage bot is using the deployment wallet; stop it first"
    load_wallet_config
}

state_has_transaction() {
    jq -e --arg label "$1" '.transactions[$label] | type == "string"' \
        "${STATE_FILE}" >/dev/null 2>&1
}

initialize_fixed_state() {
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
        --arg fixture_hash "${FIXTURE_SHA256}" \
        --arg source_pool "${V2_FIXED_POOL}" \
        --arg factory "${POOL_FACTORY}" \
        --arg backstop "${BACKSTOP}" \
        --arg oracle "${FIXED_ORACLE}" \
        --arg pool "${EXPECTED_POOL}" \
        --arg salt "${POOL_SALT}" '
        {
            schema_version: 1,
            network: "public",
            network_passphrase: $passphrase,
            rpc_url: $rpc,
            wallet: $wallet,
            source_commit: $source_commit,
            run_dir: $run_dir,
            created_at: $created_at,
            fixture_sha256: $fixture_hash,
            source_v2_fixed_pool: $source_pool,
            pool_factory: $factory,
            backstop: $backstop,
            oracle: $oracle,
            pool: $pool,
            salt: $salt,
            phase: "planned",
            transactions: {}
        }
    ' >"${STATE_FILE}"
}

load_fixed_state() {
    state_exists || die "mainnet Fixed deployment state does not exist: ${STATE_FILE}"
    RUN_DIR="$(state_value run_dir)"
    [[ -d "${RUN_DIR}" ]] || die "recorded Fixed run directory is missing"
    assert_equal "${NETWORK_PASSPHRASE}" "$(state_value network_passphrase)" \
        "Fixed state network"
    assert_equal "${WALLET}" "$(state_value wallet)" "Fixed state wallet"
    assert_equal "${FIXTURE_SHA256}" "$(state_value fixture_sha256)" \
        "Fixed state fixture digest"
    assert_equal "${EXPECTED_POOL}" "$(state_value pool)" "Fixed state pool"
}

capture_pool_transaction() {
    local label="$1" function="$2"
    shift 2
    capture "${label}" wallet_stellar contract invoke \
        --send yes \
        --id "${EXPECTED_POOL}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost --auto-sign -- "${function}" "$@" >/dev/null
    record_transaction "${label}"
}

ensure_pool_deployed() {
    local raw pool
    if [[ "$(factory_is_pool)" == "true" ]]; then
        state_has_transaction deploy-fixed-pool ||
            die "Fixed pool exists without recorded deployment evidence; review manually"
        return
    fi
    capture deploy-fixed-pool wallet_stellar contract invoke \
        --send yes \
        --id "${POOL_FACTORY}" \
        --source-account "${WALLET}" \
        --rpc-url "${RPC_URL}" \
        --network-passphrase "${NETWORK_PASSPHRASE}" \
        --cost --auto-sign -- deploy \
        --admin "${WALLET}" \
        --name "$(fixture_value '.deployment.name')" \
        --salt "${POOL_SALT}" \
        --oracle "${FIXED_ORACLE}" \
        --backstop_take_rate "$(fixture_value '.deployment.backstop_take_rate')" \
        --max_positions "$(fixture_value '.deployment.max_positions')" \
        --min_collateral "$(fixture_value '.deployment.min_collateral')" \
        --backstop_config "$(fixture_value '.deployment.backstop_config | @json')" \
        --access_controller null >/dev/null
    raw="$(<"${RUN_DIR}/deploy-fixed-pool.out")"
    pool="$(normalize_scalar "${raw}")"
    assert_equal "${EXPECTED_POOL}" "${pool}" "deployed Fixed pool"
    record_transaction deploy-fixed-pool
}

verify_reserve() {
    local index="$1" asset expected actual
    asset="$(fixture_value ".reserves[${index}].asset")"
    expected="$(fixture_value ".reserves[${index}].config | @json")"
    actual="$(invoke_view "${EXPECTED_POOL}" get_reserve --asset "${asset}" | jq -c '.config')"
    assert_json_equal "${expected}" "${actual}" "Fixed reserve ${index} configuration"
}

ensure_reserve() {
    local index="$1" symbol asset metadata reserve_list queue_label set_label
    index="$1"
    symbol="$(fixture_value ".reserves[${index}].symbol" | tr '[:upper:]' '[:lower:]')"
    asset="$(fixture_value ".reserves[${index}].asset")"
    metadata="$(fixture_value ".reserves[${index}].config | @json")"
    queue_label="queue-fixed-${symbol}-reserve"
    set_label="set-fixed-${symbol}-reserve"
    reserve_list="$(invoke_view "${EXPECTED_POOL}" get_reserve_list)"
    if printf '%s' "${reserve_list}" | jq -e --arg asset "${asset}" \
        'index($asset) != null' >/dev/null
    then
        state_has_transaction "${set_label}" ||
            die "Fixed ${symbol} reserve exists without recorded transaction evidence"
        verify_reserve "${index}"
        return
    fi

    if invoke_view "${EXPECTED_POOL}" set_reserve --asset "${asset}" >/dev/null 2>&1; then
        state_has_transaction "${queue_label}" ||
            die "Fixed ${symbol} reserve is queued without recorded transaction evidence"
    else
        state_has_transaction "${queue_label}" &&
            die "recorded Fixed ${symbol} queue is not available for execution"
        capture_pool_transaction "${queue_label}" queue_set_reserve \
            --asset "${asset}" --metadata "${metadata}"
    fi
    capture_pool_transaction "${set_label}" set_reserve --asset "${asset}"
    verify_reserve "${index}"
}

verify_live_pool() {
    local registered admin config reserve_list factory_config reward_zone pool_data
    local index
    registered="$(factory_is_pool)"
    assert_equal true "${registered}" "Fixed factory registration"
    verify_contract_artifact "${EXPECTED_POOL}" "${POOL_WASM}" "V3 Fixed pool"
    admin="$(normalize_scalar "$(invoke_view "${EXPECTED_POOL}" get_admin)")"
    assert_equal "${WALLET}" "${admin}" "Fixed pool admin/operator"
    config="$(invoke_view "${EXPECTED_POOL}" get_config)"
    assert_json_equal "$(expected_pool_config)" "${config}" "Fixed pool configuration"
    reserve_list="$(invoke_view "${EXPECTED_POOL}" get_reserve_list)"
    assert_json_equal "$(expected_reserve_list)" "${reserve_list}" \
        "Fixed reserve list"
    for index in 0 1 2; do
        verify_reserve "${index}"
    done
    factory_config="$(invoke_view "${POOL_FACTORY}" backstop_config \
        --pool_address "${EXPECTED_POOL}")"
    assert_json_equal "$(expected_backstop_config)" "${factory_config}" \
        "Fixed immutable backstop configuration"
    reward_zone="$(invoke_view "${BACKSTOP}" reward_zone)"
    printf '%s' "${reward_zone}" | jq -e --arg pool "${EXPECTED_POOL}" \
        'index($pool) == null' >/dev/null || die "Fixed pool is unexpectedly in the reward zone"
    pool_data="$(invoke_view "${BACKSTOP}" pool_data --pool "${EXPECTED_POOL}")"
    printf '%s' "${pool_data}" | jq -e '
        (.active_value | tostring) == "0" and
        (.q4w_pct | tostring) == "0" and
        (.tiers | length) == 3 and
        ([.tiers[] | ((.tokens | tostring) == "0" and
                     (.shares | tostring) == "0" and
                     (.value | tostring) == "0")] | all)
    ' >/dev/null || die "Fixed pool backstop is unexpectedly funded"
}

write_registry() {
    local temporary
    temporary="${REGISTRY_FILE}.tmp"
    jq -n \
        --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg source_commit "$(state_value source_commit)" \
        --arg operator "${WALLET}" \
        --arg source_pool "${V2_FIXED_POOL}" \
        --arg factory "${POOL_FACTORY}" \
        --arg backstop "${BACKSTOP}" \
        --arg oracle "${FIXED_ORACLE}" \
        --arg pool "${EXPECTED_POOL}" \
        --arg fixture_hash "${FIXTURE_SHA256}" \
        --arg pool_hash "${POOL_SHA256}" \
        --argjson config "$(jq -c '.deployment' "${FIXTURE}")" \
        --argjson reserves "$(jq -c '.reserves' "${FIXTURE}")" \
        --argjson transactions "$(jq -c '.transactions' "${STATE_FILE}")" '
        {
            schema_version: 1,
            network: {
                name: "Public Global Stellar Network",
                passphrase: "Public Global Stellar Network ; September 2015"
            },
            generated_at: $generated_at,
            source_commit: $source_commit,
            operator: $operator,
            source_v2_fixed_pool: $source_pool,
            contracts: {
                pool_factory: $factory,
                backstop: $backstop,
                oracle: $oracle,
                pool: $pool,
                pool_wasm_hash: $pool_hash
            },
            fixture: {sha256: $fixture_hash},
            configuration: $config + {
                initial_status: 6,
                funded: false,
                activated: false,
                reward_zone_member: false
            },
            reserves: $reserves,
            transactions: $transactions,
            verified_at: $generated_at
        }
    ' >"${temporary}"
    mv "${temporary}" "${REGISTRY_FILE}"
}

complete_deployment() {
    local phase
    phase="$(state_optional phase)"
    if [[ "${phase}" == "verified" ]]; then
        verify_live_pool
        write_registry
        note "Mainnet V3 Fixed pool is already deployed and verified in setup status."
        return
    fi
    ensure_pool_deployed
    state_set phase pool-deployed
    ensure_reserve 0
    ensure_reserve 1
    ensure_reserve 2
    state_set phase reserves-configured
    verify_live_pool
    state_set phase verified
    state_set verified_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    write_registry
    note "Mainnet V3 Fixed pool deployed and verified in setup status; no funding or activation occurred."
}

deployment_plan() {
    validate_fixture
    validate_local_artifacts
    validate_core_registry
    local predicted
    predicted="$(normalize_scalar "$(simulate_pool_deploy)")"
    assert_equal "${EXPECTED_POOL}" "${predicted}" "simulated Fixed pool address"
    cat <<EOF
BLNT V3 mainnet Fixed pool plan
  Pool:             ${EXPECTED_POOL}
  Pool factory:     ${POOL_FACTORY}
  Admin/operator:   ${WALLET}
  Source V2 pool:   ${V2_FIXED_POOL}
  Oracle:           ${FIXED_ORACLE}
  Reserves:         XLM, USDC, EURC (exact V2 Fixed configurations)
  Backstop tiers:   BLNT:XLM / BLNT:USDC / USDC
  Take-rate weights: 4 / 3 / 2
  Initial status:   6 (setup)

The command deploys the pool and configures its three reserves. It does not
fund the backstop, add the pool to the reward zone, configure emissions,
change status, activate borrowing, or start a keeper.
EOF
}

validate_command() {
    validate_fixture
    validate_local_artifacts
    validate_core_registry
    note "Validated the mainnet Fixed fixture, core registry, and pinned V3 artifacts."
}

preflight_command() {
    local predicted
    validate_command
    require_clean_reviewed_main read-only
    [[ ! -f "${STATE_FILE}" ]] ||
        die "mainnet Fixed state already exists; use status or the reviewed recovery path"
    validate_live_core
    validate_live_v2_snapshot
    predicted="$(normalize_scalar "$(simulate_pool_deploy)")"
    assert_equal "${EXPECTED_POOL}" "${predicted}" "simulated Fixed pool address"
    assert_equal false "$(factory_is_pool)" "preflight Fixed factory registration"
    note "Read-only mainnet Fixed preflight passed. No transaction was submitted."
}

deploy_command() {
    validate_fixture
    validate_local_artifacts
    validate_core_registry
    require_mutation_confirmation
    [[ ! -f "${STATE_FILE}" ]] || die "Fixed state already exists; use resume"
    validate_live_core
    validate_live_v2_snapshot
    assert_equal false "$(factory_is_pool)" "pre-deployment Fixed factory registration"
    initialize_fixed_state
    complete_deployment
}

resume_command() {
    validate_fixture
    validate_local_artifacts
    validate_core_registry
    require_mutation_confirmation
    load_fixed_state
    validate_live_core
    validate_live_v2_snapshot
    complete_deployment
}

status_command() {
    if [[ ! -f "${STATE_FILE}" ]]; then
        deployment_plan
        note "No V3 mainnet Fixed deployment state exists."
        return
    fi
    validate_fixture
    validate_local_artifacts
    validate_core_registry
    load_fixed_state
    cat <<EOF
BLNT V3 mainnet Fixed pool
  phase:          $(state_value phase)
  pool:           ${EXPECTED_POOL}
  admin/operator: ${WALLET}
  oracle:         ${FIXED_ORACLE}
  registry:       ${REGISTRY_FILE}
EOF
    if [[ "$(state_value phase)" == "verified" ]]; then
        validate_live_core
        validate_live_v2_snapshot
        verify_live_pool
        note "Live Fixed pool configuration, setup status, and zero funding verified."
    fi
}

usage() {
    cat <<EOF
Usage: scripts/mainnet-v3-fixed-pool.sh COMMAND

Commands:
  plan       Print the deterministic Fixed-pool plan; do not mutate mainnet.
  validate   Validate the fixture, core registry, and artifacts locally.
  preflight  Verify the live V2 source configuration and unused pool address.
  deploy     Deploy the Fixed pool and reserves without funding or activation.
  resume     Resume a reviewed interrupted Fixed deployment.
  status     Print state and reverify a completed deployment.

deploy and resume require both:
  BLEND_MAINNET_V3_FIXED_CONFIRM=DEPLOY_BLNT_V3_FIXED_MAINNET
  BLEND_MAINNET_V3_SEQUENCE_LOCK_CONFIRMED=true
EOF
}

case "${1:-}" in
    plan) deployment_plan ;;
    validate) validate_command ;;
    preflight) preflight_command ;;
    deploy) deploy_command ;;
    resume) resume_command ;;
    status) status_command ;;
    *) usage; exit 1 ;;
esac
