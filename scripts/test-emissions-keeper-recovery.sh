#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/emissions-keeper.sh
source "${ROOT_DIR}/scripts/emissions-keeper.sh"

EXPECTED_KEY="GDEA2WMOWYQ3T56A2PZH5QYYEQBAORW2AUUJYX4FFD5GDPESAICTFTAL"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/blnt-keeper-recovery-test.XXXXXX")"
CONFIG_DIR="${TEST_DIR}/stellar-config"
IDENTITY_MARKER="${CONFIG_DIR}/identity-present"
LOG_FILE="${TEST_DIR}/keeper.log"

cleanup_test() {
    rm -rf -- "${TEST_DIR}"
}

fail_test() {
    printf 'not ok - %s\n' "$*" >&2
    exit 1
}

assert_log_contains() {
    local pattern="$1"
    rg -q -- "${pattern}" "${LOG_FILE}" ||
        fail_test "keeper log did not contain: ${pattern}"
}

mkdir -p "${CONFIG_DIR}"
trap cleanup_test EXIT

SOURCE_ACCOUNT="test-keeper"
EXPECTED_SOURCE_PUBLIC_KEY="${EXPECTED_KEY}"
INTERVAL_SECONDS=0
ORACLE_REFRESH_INTERVAL_SECONDS=1
MOCK_DERIVED_KEY="${EXPECTED_KEY}"

stellar_cli() {
    [[ "$*" == "keys public-key ${SOURCE_ACCOUNT}" ]] ||
        fail_test "unexpected Stellar CLI call: $*"
    if [[ ! -f "${IDENTITY_MARKER}" ]]; then
        printf 'identity not found\n' >&2
        return 1
    fi
    printf '%s\n' "${MOCK_DERIVED_KEY}"
}

note() {
    printf '%s\n' "$*" >> "${LOG_FILE}"
}

touch "${IDENTITY_MARKER}"
validate_source_identity || fail_test "valid source identity was rejected"

MOCK_DERIVED_KEY="GUNEXPECTED"
if validate_source_identity; then
    fail_test "mismatched source identity was accepted"
fi
MOCK_DERIVED_KEY="${EXPECTED_KEY}"

: > "${LOG_FILE}"
touch "${IDENTITY_MARKER}"
if (
    STOP_REQUESTED=0
    SLEEP_PID=""
    pass_count=0
    run_pass() {
        pass_count=$((pass_count + 1))
        rm -f -- "${IDENTITY_MARKER}"
    }
    run_forever
); then
    fail_test "keeper continued after its source identity disappeared"
fi
assert_log_contains 'Fatal keeper source-identity preflight failure'

: > "${LOG_FILE}"
touch "${IDENTITY_MARKER}"
(
    STOP_REQUESTED=0
    SLEEP_PID=""
    pass_count=0
    run_pass() {
        pass_count=$((pass_count + 1))
        if (( pass_count == 1 )); then
            return 1
        fi
        STOP_REQUESTED=1
    }
    run_forever
    (( pass_count == 2 )) ||
        fail_test "transient pass failure was not retried"
)
assert_log_contains 'Scheduled pass 1 failed; the keeper will retry on the next interval.'

printf 'ok - keeper identity loss exits for supervisor recovery\n'
printf 'ok - transient pass failure remains on the scheduled retry path\n'
