#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ACCOUNT_CONFIG="${BLEND_V3_KEEPER_ACCOUNT_CONFIG:-}"
EXPECTED_ACCOUNT="${BLEND_V3_KEEPER_ACCOUNT:-GDEA2WMOWYQ3T56A2PZH5QYYEQBAORW2AUUJYX4FFD5GDPESAICTFTAL}"
RUNTIME_DIR="${BLEND_V3_KEEPER_RUNTIME_DIR:-${ROOT_DIR}/.mainnet-v3-keeper}"
KEY_CONFIG_DIR=""
CHILD_PID=""

usage() {
    cat <<'EOF'
Usage: scripts/mainnet-v3-emissions-keeper.sh COMMAND

Commands:
  plan  Simulate one mainnet V3 keeper pass without mutation.
  once  Submit one mainnet V3 keeper pass and exit.
  run   Run one pass immediately, then repeat every hour by default.

The configured account pays fees for permissionless V3 maintenance calls. Its
secret is loaded from BLEND_V3_KEEPER_ACCOUNT_CONFIG into a temporary Stellar
CLI config directory and is removed when the keeper exits. The account-config
path must be supplied explicitly.
EOF
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

note() {
    printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" >&2
}

cleanup() {
    if [[ -n "${KEY_CONFIG_DIR}" && -d "${KEY_CONFIG_DIR}" ]]; then
        rm -rf -- "${KEY_CONFIG_DIR}"
    fi
}

forward_stop() {
    if [[ -n "${CHILD_PID}" ]]; then
        kill -TERM "${CHILD_PID}" 2>/dev/null || true
    fi
}

load_keeper_identity() {
    local derived_account import_output

    note "Loading the mainnet V3 keeper identity."
    [[ -n "${ACCOUNT_CONFIG}" ]] ||
        die "BLEND_V3_KEEPER_ACCOUNT_CONFIG must identify the keeper account file"
    [[ -f "${ACCOUNT_CONFIG}" ]] || die "keeper account file not found: ${ACCOUNT_CONFIG}"
    # shellcheck disable=SC1090
    source "${ACCOUNT_CONFIG}"
    [[ "${STELLAR_PUBLIC_KEY:-}" == "${EXPECTED_ACCOUNT}" ]] ||
        die "keeper account must be ${EXPECTED_ACCOUNT}"
    [[ "${STELLAR_SECRET_KEY:-}" =~ ^S[A-Z2-7]{55}$ ]] ||
        die "keeper account file does not contain a valid STELLAR_SECRET_KEY"

    mkdir -p "${RUNTIME_DIR}" || die "unable to create keeper runtime directory"
    KEY_CONFIG_DIR="$(mktemp -d \
        "${TMPDIR:-/tmp}/blnt-v3-mainnet-keeper.XXXXXX")" ||
        die "unable to create temporary Stellar config directory"
    chmod 700 "${KEY_CONFIG_DIR}" || die "unable to protect temporary Stellar config directory"
    if ! import_output="$(printf '%s\n' "${STELLAR_SECRET_KEY}" |
        stellar --config-dir "${KEY_CONFIG_DIR}" keys add blnt-v3-mainnet-keeper \
            --secret-key 2>&1)"
    then
        die "unable to load the keeper identity: ${import_output}"
    fi
    unset STELLAR_SECRET_KEY
    derived_account="$(stellar --config-dir "${KEY_CONFIG_DIR}" \
        keys public-key blnt-v3-mainnet-keeper)" ||
        die "unable to derive the keeper public key"
    [[ "${derived_account}" == "${EXPECTED_ACCOUNT}" ]] ||
        die "keeper secret does not match ${EXPECTED_ACCOUNT}"
    note "Loaded keeper account ${derived_account}."
}

run_keeper() {
    local command="$1" status

    export BLEND_KEEPER_REGISTRY="${ROOT_DIR}/deployments/mainnet-v3-keeper.json"
    export BLEND_KEEPER_WORK_DIR="${RUNTIME_DIR}"
    export BLEND_KEEPER_CONFIG_DIR="${KEY_CONFIG_DIR}"
    export BLEND_KEEPER_STATE_FILE="${RUNTIME_DIR}/state.json"
    export BLEND_KEEPER_SOURCE_ACCOUNT="blnt-v3-mainnet-keeper"
    export BLEND_KEEPER_INCLUSION_FEE="${BLEND_V3_KEEPER_INCLUSION_FEE:-10000}"
    export BLEND_KEEPER_INTERVAL_SECONDS="${BLEND_V3_KEEPER_INTERVAL_SECONDS:-3600}"

    note "Starting mainnet V3 keeper command: ${command}."
    bash "${ROOT_DIR}/scripts/emissions-keeper.sh" "${command}" &
    CHILD_PID=$!
    set +e
    wait "${CHILD_PID}"
    status=$?
    set -e
    CHILD_PID=""
    return "${status}"
}

main() {
    local command="${1:-}"

    case "${command}" in
        plan|once|run) ;;
        help|-h|--help) usage; return ;;
        *) usage >&2; exit 2 ;;
    esac

    command -v stellar >/dev/null 2>&1 || die "required command not found: stellar"
    load_keeper_identity
    run_keeper "${command}"
}

trap forward_stop INT TERM
trap cleanup EXIT
main "$@"
